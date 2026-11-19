//! ABI 7 primitive entries and the generic checked bridges (#448).
use super::*;
use crate::portable::primitive::{Kind, Method, Shape, Signature};
use crate::portable::ir::ValueId;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn value_type(kind: Kind) -> ValType {
    match kind { Kind::Boxed => VALUE, Kind::Long => ValType::I64, Kind::Double => ValType::F64 }
}
pub(super) fn repr(kind: Kind) -> unboxed::Repr {
    match kind { Kind::Boxed => unboxed::Repr::Boxed, Kind::Long => unboxed::Repr::Long, Kind::Double => unboxed::Repr::Double }
}

pub(super) struct Types {
    pub entries: BTreeMap<Signature, u32>,
    pub codes: BTreeMap<Shape, u32>,
    pub shapes: Vec<Shape>,
}
impl Types {
    pub fn new(types: &mut TypeSection, mut next: u32, bodies: &[&ClosureBody], layout: &Layout, calls: &[&Shape]) -> Self {
        let signatures = bodies.iter().filter_map(|body| body.primitive.clone())
            .chain(calls.iter().flat_map(|shape| shape.0.iter().map(|method| method.signature.clone())))
            .collect::<BTreeSet<_>>();
        let mut entries = BTreeMap::new();
        for signature in signatures {
            let mut params = vec![VALUE];
            params.extend(signature.parameters.iter().copied().map(value_type));
            types.ty().function(params, [value_type(signature.result)]);
            entries.insert(signature, next);
            next += 1;
        }
        let shapes = layout.definitions.iter().map(|definition| {
            let mut methods = definition.methods.iter().filter_map(|(body, _, variadic)| {
                (!*variadic).then(|| bodies[*body].primitive.clone()).flatten()
                    .map(|signature| Method { variadic: false, signature })
            }).collect::<Vec<_>>();
            methods.sort_by_key(|method| (method.signature.parameters.len(), method.variadic));
            Shape(methods)
        }).collect::<Vec<_>>();
        let mut codes = BTreeMap::new();
        for shape in shapes.iter().chain(calls.iter().copied()).filter(|shape| !shape.0.is_empty()).cloned().collect::<BTreeSet<_>>() {
            let mut fields = runtime_abi::code_fields();
            for method in &shape.0 {
                fields.push(FieldType {
                    element_type: StorageType::Val(ValType::Ref(RefType {
                        nullable: false, heap_type: HeapType::Concrete(entries[&method.signature]),
                    })), mutable: false,
                });
            }
            types.ty().subtype(&SubType {
                is_final: true, supertype_idx: Some(runtime_abi::CODE),
                composite_type: CompositeType {
                    inner: CompositeInnerType::Struct(StructType { fields: fields.into_boxed_slice() }),
                    shared: false, descriptor: None, describes: None,
                },
            });
            codes.insert(shape, next);
            next += 1;
        }
        Self { entries, codes, shapes }
    }
    pub fn code_type(&self, definition: usize) -> u32 {
        self.codes.get(&self.shapes[definition]).copied().unwrap_or(runtime_abi::CODE)
    }
}

/// Push a boxed parameter's checked primitive value without a temporary box on
/// the usual i31/own-box paths. The runtime handles other numbers or errors.
fn parameter(function: &mut Function, value: u32, kind: Kind, temp: u32, index: &impl Fn(&str) -> u32) {
    use Instruction::*;
    if kind == Kind::Boxed { function.instruction(&LocalGet(value)); return; }
    let long = kind == Kind::Long;
    let boxed = if long { runtime_abi::LONG } else { runtime_abi::NUMBER };
    let raw = value_type(kind);
    function
        .instruction(&LocalGet(value))
        .instruction(&RefTestNonNull(HeapType::I31))
        .instruction(&If(BlockType::Result(raw)))
        .instruction(&LocalGet(value))
        .instruction(&RefCastNonNull(HeapType::I31))
        .instruction(&I31GetS)
        .instruction(&if long { I64ExtendI32S } else { F64ConvertI32S })
        .instruction(&Else)
        .instruction(&LocalGet(value))
        .instruction(&RefTestNonNull(HeapType::Concrete(boxed)))
        .instruction(&If(BlockType::Result(raw)))
        .instruction(&LocalGet(value))
        .instruction(&RefCastNonNull(HeapType::Concrete(boxed)))
        .instruction(&StructGet { struct_type_index: boxed, field_index: 0 })
        .instruction(&Else)
        .instruction(&LocalGet(value))
        .instruction(&Call(index(if long { "number-cast-long" } else { "number-cast-double" })))
        .instruction(&LocalSet(temp));
    for instruction in if long { runtime_abi::long_to_i64(temp) } else { runtime_abi::number_to_f64(temp) } {
        function.instruction(&instruction);
    }
    function.instruction(&End).instruction(&End);
}

pub(super) fn bridge(body: usize, context: &Context) -> Function {
    use Instruction::*;
    let method = context.bodies[body];
    let signature = method.primitive.as_ref().expect("primitive bridge");
    let temp = 1 + method.arity as u32;
    let long = temp + 1;
    let mut function = Function::new([(1, VALUE), (1, ValType::I64)]);
    let index = |name: &str| context.names.iter().position(|candidate| *candidate == name).expect("primitive import") as u32;
    function.instruction(&LocalGet(0));
    for (i, kind) in signature.parameters.iter().enumerate() {
        parameter(&mut function, 1 + i as u32, *kind, temp, &index);
    }
    function.instruction(&Call(context.layout.primitive_entries[body].expect("primitive function")));
    match signature.result {
        Kind::Boxed => {},
        Kind::Long => {
            for instruction in runtime_abi::long_box_in(long) { function.instruction(&instruction); }
        }
        Kind::Double => { function.instruction(&StructNew(runtime_abi::NUMBER)); },
    }
    function.instruction(&End);
    function
}

pub(super) fn argument(code: &mut Vec<Instruction<'static>>, locals: &unboxed::Locals, value: ValueId,
    kind: Kind, checked: bool, temp: u32, index: &impl Fn(&str) -> u32)
{
    use Instruction::*;
    if kind == Kind::Boxed { locals.push_boxed(code, value); return; }
    let from = locals.of(value);
    let raw = from.definite();
    let target = repr(kind);
    let numeric = matches!(raw, unboxed::Repr::Long | unboxed::Repr::Double);
    if numeric {
        if from.maybe() { code.extend([LocalGet(locals.valid(value)), If(BlockType::Result(value_type(kind)))]); }
        if checked && raw == unboxed::Repr::Double && target == unboxed::Repr::Long {
            code.extend([LocalGet(locals.raw(value)), F64Const((-9_223_372_036_854_775_808.0).into()), F64Lt,
                LocalGet(locals.raw(value)), F64Const(9_223_372_036_854_775_808.0.into()), F64Gt, I32Or, If(BlockType::Empty)]);
            locals.push_boxed(code, value);
            code.extend([Call(index("number-cast-long")), Drop, Unreachable, End]);
        }
        code.push(LocalGet(locals.raw(value)));
        if raw != target { code.push(if kind == Kind::Double { F64ConvertI64S } else { I64TruncSatF64S }); }
        if !from.maybe() { return; }
        code.push(Else);
    }
    // The generic replacement branch may contain any value. Preserve the
    // original checked argument conversion at this actual typed boundary.
    if from.maybe() { code.push(LocalGet(value.0 as u32 + locals.offset())); }
    else { locals.push_boxed(code, value); }
    let cast = if kind == Kind::Long {
        if checked { "number-cast-long" } else { "number-cast-unchecked-long" }
    } else { "number-cast-double" };
    code.extend([Call(index(cast)), LocalSet(temp)]);
    code.extend(if kind == Kind::Long { runtime_abi::long_to_i64(temp) } else { runtime_abi::number_to_f64(temp) });
    if numeric && from.maybe() { code.push(End); }
}

/// A typed hit and an arbitrary boxed fallback share no result-type promise.
/// Keep the raw hit separately, with a flag, rather than coercing a replacement.
pub(super) fn call(code: &mut Vec<Instruction<'static>>, operands: &[ValueId], shape: &Shape,
    signature: &Signature, result: ValueId, ir: &IrFunction, context: &Context,
    locals: &unboxed::Locals, known: Option<(usize, bool)>, link: Option<usize>,
    temp: u32, wide: u32, index: &impl Fn(&str) -> u32)
{
    use Instruction::*;
    let arguments = &operands[1..];
    let arity = arguments.len();
    let callee = operands[0];
    let site_key = (ir as *const IrFunction as usize, result.0 as u32);
    let typed_result = |code: &mut Vec<Instruction<'static>>| {
        if locals.of(result).maybe() {
            code.extend([LocalSet(locals.raw(result)), I32Const(1), LocalSet(locals.valid(result))]);
        } else {
            match signature.result {
                Kind::Boxed => {},
                Kind::Long => code.extend(runtime_abi::long_box_in(wide)),
                Kind::Double => code.push(StructNew(runtime_abi::NUMBER)),
            }
            code.push(LocalSet(result.0 as u32 + locals.offset()));
        }
    };
    let generic_result = |code: &mut Vec<Instruction<'static>>| {
        code.push(LocalSet(result.0 as u32 + locals.offset()));
        if locals.of(result).maybe() { code.extend([I32Const(0), LocalSet(locals.valid(result))]); }
    };
    let typed_arguments = |code: &mut Vec<Instruction<'static>>| {
        locals.push_boxed(code, callee);
        for (value, kind) in arguments.iter().zip(&signature.parameters) { argument(code, locals, *value, *kind, true, temp, index); }
    };
    let generic_arguments = |code: &mut Vec<Instruction<'static>>| {
        locals.push_boxed(code, callee);
        for value in arguments { locals.push_boxed(code, *value); }
    };
    if let Some(link) = link {
        let definition = super::super::hir::link_parts(link).1;
        let target = context.names.iter().position(|name| *name == format!("link.{definition}.{arity}.typed"))
            .map(|position| position as u32).or_else(|| {
                let (_, _, layout) = context.layout.definitions.iter().enumerate().find_map(|(layout, candidate)|
                    (Some(&candidate.global) == context.links.codes.get(&definition)).then_some((definition, candidate.global, layout)))?;
                context.layout.definitions[layout].methods.iter().find_map(|(body, count, variadic)|
                    (*count == arity && !*variadic).then_some(context.layout.primitive_entries[*body]).flatten())
            }).expect("typed link entry");
        code.extend([Block(BlockType::Empty), Block(BlockType::Result(VALUE))]);
        locals.push_boxed(code, callee);
        code.extend([BrOnCastFail { relative_depth: 0, from_ref_type: RefType::EQREF,
            to_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(context.links.link_type) } },
            StructGet { struct_type_index: runtime_abi::CLOSURE, field_index: runtime_abi::CLOSURE_CODE },
            GlobalGet(context.links.codes[&definition]), RefEq, If(BlockType::Empty)]);
        typed_arguments(code);
        code.push(Call(target));
        typed_result(code);
        code.extend([Br(2), End]);
        locals.push_boxed(code, callee);
        code.extend([End, Drop]);
        generic_arguments(code);
        code.push(Call(index(runtime_abi::CALL_EXPORTS[arity])));
        generic_result(code);
        code.push(End);
    } else if let Some((definition, _)) = known {
        let method = context.layout.definitions[definition].methods.iter().find(|(_, count, variadic)| *count == arity && !*variadic)
            .expect("known typed method");
        typed_arguments(code);
        code.push(Call(context.layout.primitive_entries[method.0].expect("known primitive body")));
        typed_result(code);
    } else {
        let site = context.call_sites[&site_key];
        let primitive_site = context.primitive_sites[&site_key];
        let function_type = context.primitive.entries[signature];
        let code_type = context.primitive.codes[shape];
        let field = runtime_abi::code_fields().len() as u32 + shape.0.iter().position(|method|
            !method.variadic && &method.signature == signature).expect("typed method field") as u32;
        code.push(GlobalGet(site));
        locals.push_boxed(code, callee);
        code.extend([RefEq, I32Eqz, If(BlockType::Empty)]);
        locals.push_boxed(code, callee);
        code.extend([Call(index(runtime_abi::CALL_ENTRY_EXPORTS[arity])), GlobalSet(site + 1),
            Block(BlockType::Result(ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(function_type) }))),
            Block(BlockType::Result(VALUE))]);
        locals.push_boxed(code, callee);
        code.extend([BrOnCastFail { relative_depth: 0, from_ref_type: RefType::EQREF,
            to_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::CLOSURE) } },
            StructGet { struct_type_index: runtime_abi::CLOSURE, field_index: runtime_abi::CLOSURE_CODE },
            BrOnCastFail { relative_depth: 0,
                from_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::CODE) },
                to_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(code_type) } },
            StructGet { struct_type_index: code_type, field_index: field }, Br(1), End, Drop,
            RefNull(HeapType::Concrete(function_type)), End, GlobalSet(primitive_site)]);
        locals.push_boxed(code, callee);
        code.extend([GlobalSet(site), End, GlobalGet(primitive_site), RefIsNull, I32Eqz, If(BlockType::Empty)]);
        typed_arguments(code);
        code.extend([GlobalGet(primitive_site), RefAsNonNull, CallRef(function_type)]);
        typed_result(code);
        code.push(Else);
        generic_arguments(code);
        code.extend([GlobalGet(site + 1), CallRef(runtime_abi::CALL + arity as u32)]);
        generic_result(code);
        code.push(End);
    }
}
