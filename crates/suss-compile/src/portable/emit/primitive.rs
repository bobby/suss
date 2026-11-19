//! ABI 7 primitive entries and the generic checked bridges (#448).
use super::*;
use crate::portable::primitive::{Kind, Method, Shape, Signature};
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
    pub fn new(types: &mut TypeSection, mut next: u32, bodies: &[&ClosureBody], layout: &Layout) -> Self {
        let signatures = bodies.iter().filter_map(|body| body.primitive.clone()).collect::<BTreeSet<_>>();
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
                bodies[*body].primitive.clone().map(|signature| Method { variadic: *variadic, signature })
            }).collect::<Vec<_>>();
            methods.sort_by_key(|method| (method.signature.parameters.len(), method.variadic));
            Shape(methods)
        }).collect::<Vec<_>>();
        let mut codes = BTreeMap::new();
        for shape in shapes.iter().filter(|shape| !shape.0.is_empty()).cloned().collect::<BTreeSet<_>>() {
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
