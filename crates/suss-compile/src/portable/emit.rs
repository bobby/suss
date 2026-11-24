//! Emit only verified IR. Operands are local value IDs, never source expressions.
mod r#async;
mod structured;
mod unboxed;
mod captures;
mod primitive;
use super::{
    hir::{Arithmetic, ArrayOperation, Literal, Nominal, Type},
    ir::{self, ClosureBody, Function as IrFunction, GeneralClosureBody, FutureBody, Operation, Terminator},
    Diagnostic,
};
use crate::runtime_abi;
use std::{borrow::Cow, collections::BTreeSet};
use wasm_encoder::*;
const VALUE: ValType = ValType::Ref(RefType::EQREF);
/// `object-method-invoke-N` (#378): a method call with N arguments after this.
fn object_method_invoke_name(n: usize) -> &'static str {
    const NAMES: [&str; runtime_abi::MAX_FIXED as usize] = [
        "object-method-invoke-0", "object-method-invoke-1", "object-method-invoke-2", "object-method-invoke-3",
        "object-method-invoke-4", "object-method-invoke-5", "object-method-invoke-6", "object-method-invoke-7",
    ];
    NAMES[n]
}
/// `object-method-site-N` and `object-method-call-N` (#462), as above.
fn object_method_site_name(n: usize) -> &'static str {
    const NAMES: [&str; runtime_abi::MAX_FIXED as usize] = [
        "object-method-site-0", "object-method-site-1", "object-method-site-2", "object-method-site-3",
        "object-method-site-4", "object-method-site-5", "object-method-site-6", "object-method-site-7",
    ];
    NAMES[n]
}
fn object_method_call_name(n: usize) -> &'static str {
    const NAMES: [&str; runtime_abi::MAX_FIXED as usize] = [
        "object-method-call-0", "object-method-call-1", "object-method-call-2", "object-method-call-3",
        "object-method-call-4", "object-method-call-5", "object-method-call-6", "object-method-call-7",
    ];
    NAMES[n]
}

/// Globals per cached named-read site: its immutable `NAMED_SITE` struct (#378).
const FIELD_SITE_GLOBALS: u32 = 1;

// Two descriptors at a protocol site cover the LazySeq/ChunkedCons pair
// used by chunked pipelines without changing the monomorphic hit path.
const METHOD_SITE_GLOBALS: u32 = 6;

fn protocol_site_lookup(
    function: &mut Function,
    cache: u32,
    epoch: u32,
    descriptor: &[Instruction<'_>],
    lookup: &[Instruction<'_>],
) {
    use Instruction::*;
    for way in [cache, cache + 3] {
        for instruction in descriptor {
            function.instruction(instruction);
        }
        for instruction in [
            GlobalGet(way), RefEq, GlobalGet(way + 2), GlobalGet(epoch),
            I32Eq, I32And, If(BlockType::Result(VALUE)),
            GlobalGet(way + 1), Else,
        ] {
            function.instruction(&instruction);
        }
    }
    // Evaluate the lookup before changing the cache. Its result stays on
    // the stack while the previous first entry moves to the second way.
    for instruction in lookup {
        function.instruction(instruction);
    }
    for field in 0..3 {
        function.instruction(&GlobalGet(cache + field));
        function.instruction(&GlobalSet(cache + 3 + field));
    }
    function.instruction(&GlobalSet(cache + 1));
    for instruction in descriptor {
        function.instruction(instruction);
    }
    for instruction in [
        GlobalSet(cache), GlobalGet(epoch), GlobalSet(cache + 2),
        GlobalGet(cache + 1), End, End,
    ] {
        function.instruction(&instruction);
    }
}

fn arithmetic_name(
    operator: Arithmetic,
    arguments: &[ir::ValueId],
    coercing: bool,
    primitive_overload: bool,
    function: &IrFunction,
) -> &'static str {
    if operator == Arithmetic::Divide && !coercing {
        return if primitive_overload { "checked-divide-primitive" } else { "checked-divide" };
    }
    if operator == Arithmetic::Subtract && !coercing && primitive_overload {
        return "checked-subtract-primitive";
    }
    let numeric = arguments
        .iter()
        .all(|arg| function.values[arg.0].ty == Type::Number);
    match (operator, numeric) {
        // Checked unary + and * only check their operand, even a typed Number.
        (Arithmetic::Add, _) if arguments.len() == 1 && !coercing => "checked-identity-add",
        (Arithmetic::Multiply, _) if arguments.len() == 1 && !coercing => {
            "checked-identity-multiply"
        }
        (Arithmetic::Add, true) => "number-add",
        (Arithmetic::Subtract, true) => "number-subtract",
        (Arithmetic::Multiply, true) => "number-multiply",
        (Arithmetic::Divide, true) => "number-divide",
        (Arithmetic::Negate, true) => "number-negate",
        (Arithmetic::Add, false) if coercing => "value-add",
        (Arithmetic::Subtract, false) if coercing => "value-subtract",
        (Arithmetic::Multiply, false) if coercing => "value-multiply",
        (Arithmetic::Divide, false) if coercing => "value-divide",
        (Arithmetic::Negate, false) if coercing => "value-negate",
        // Clojure operand checks (#286).
        (Arithmetic::Add, false) => "checked-add",
        (Arithmetic::Subtract, false) => "checked-subtract",
        (Arithmetic::Multiply, false) => "checked-multiply",
        (Arithmetic::Divide, false) => "checked-divide",
        (Arithmetic::Negate, false) => "checked-negate",
    }
}
/// Passive data segments holding long string literals, one per distinct value.
#[derive(Default)]
struct LiteralData {
    segments: Vec<Vec<u8>>,
    indices: std::collections::BTreeMap<Vec<u16>, u32>,
}
impl LiteralData {
    fn segment(&mut self, units: &[u16]) -> u32 {
        if let Some(index) = self.indices.get(units) {
            return *index;
        }
        let index = self.segments.len() as u32;
        self.segments.push(units.iter().flat_map(|unit| unit.to_le_bytes()).collect());
        self.indices.insert(units.to_vec(), index);
        index
    }
}
/// The longest string literal whose global has a constant initializer, kept on
/// a bounded operand stack; a longer one comes from a passive data segment.
const SMALL_LITERAL: usize = 64;
// Literal units and indices are compiler-checked, unlike public mutable string
// storage calls. Each distinct literal is one global of its fragment (#442), so
// evaluating it reads the global instead of allocating a string: a small
// literal's global is immutable, and a larger one's is filled on its first
// evaluation. Strings are written only while they're built (`string-new`,
// `string-set-unit`), never after, so evaluations can share one.
fn emit_utf16_literal(
    function: &mut Function,
    data: &mut LiteralData,
    literals: &std::collections::HashMap<Vec<u16>, u32>,
    units: &[u16],
    local: u32,
    span: &std::ops::Range<usize>,
) -> Result<(), Diagnostic> {
    use Instruction::*;
    let fail = |message: &str| Diagnostic { span: span.clone(), message: message.into() };
    let length = i32::try_from(units.len()).map_err(|_| fail("UTF16 literal exceeds signed runtime length"))?;
    let global = *literals.get(units).ok_or_else(|| fail("uncollected string literal"))?;
    if units.len() > SMALL_LITERAL {
        function
            .instruction(&GlobalGet(global))
            .instruction(&RefIsNull)
            .instruction(&If(BlockType::Empty))
            .instruction(&I32Const(0))
            .instruction(&I32Const(length))
            .instruction(&ArrayNewData {
                array_type_index: runtime_abi::STRING,
                array_data_index: data.segment(units),
            })
            .instruction(&GlobalSet(global))
            .instruction(&End);
    }
    function.instruction(&GlobalGet(global)).instruction(&LocalSet(local));
    Ok(())
}
/// A string literal's global: immutable with a constant initializer for a small
/// literal, or null until its first evaluation fills it.
fn literal_global(units: &[u16]) -> (GlobalType, ConstExpr) {
    let string = |nullable| ValType::Ref(RefType { nullable, heap_type: HeapType::Concrete(runtime_abi::STRING) });
    if units.len() <= SMALL_LITERAL {
        let mut init: Vec<_> = units.iter().map(|unit| Instruction::I32Const(i32::from(*unit))).collect();
        init.push(Instruction::ArrayNewFixed { array_type_index: runtime_abi::STRING, array_size: units.len() as u32 });
        (GlobalType { val_type: string(false), mutable: false, shared: false }, ConstExpr::extended(init))
    } else {
        (
            GlobalType { val_type: string(true), mutable: true, shared: false },
            ConstExpr::ref_null(HeapType::Concrete(runtime_abi::STRING)),
        )
    }
}
pub fn emit(ir: &IrFunction) -> Result<Vec<u8>, Diagnostic> {
    ir::verify(ir)?;
    emit_verified(ir)
}
/// The caller must verify this exact immutable graph before entry.
pub(super) fn emit_verified(ir: &IrFunction) -> Result<Vec<u8>, Diagnostic> {
    let fail = |message: &str| Diagnostic {
        span: ir.span.clone(),
        message: message.into(),
    };
    let mut bodies = Vec::new();
    let mut dispatchers = Vec::new();
    let mut futures = Vec::new();
    collect_bodies(ir, &mut bodies, &mut dispatchers, &mut futures);
    let singles = bodies.iter().copied().filter(|body| !dispatchers.iter()
        .any(|general| general.methods.iter().any(|method| std::ptr::eq(method, *body))))
        .collect::<Vec<_>>();
    let all_functions = std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function));
    let mut names = BTreeSet::new();
    for body in &bodies {
        if let Some(signature) = &body.primitive {
            for kind in &signature.parameters {
                match kind {
                    super::primitive::Kind::Long => { names.insert("number-cast-long"); },
                    super::primitive::Kind::Double => { names.insert("number-cast-double"); },
                    _ => {},
                }
            }
        }
    }
    // Whether a variadic fn here builds a rest seq (#466).
    let mut rest_site_used = false;
    let mut throws = !futures.is_empty();
    let mut descriptor_call_guard = false;
    if !futures.is_empty() { names.extend(r#async::IMPORTS.iter().copied()); }
    if futures.iter().any(|body| !body.continuation.backedges.is_empty()) { names.insert("async-task-yield"); }
    let mut globals = BTreeSet::new();
    for function in all_functions {
        for block in &function.blocks {
            throws |= matches!(&block.terminator, Terminator::Throw(_));
            for inst in &block.instructions {
                // A hinted array access throws for a wrong hint (#389).
                throws |= matches!(inst.operation, Operation::Nominal { operation: Nominal::PackedGet(_) | Nominal::PackedSet(_) | Nominal::CheckNumericCallee, .. });
                match &inst.operation {
                    Operation::Await { .. } | Operation::MakeFuture { .. } => {}
                    Operation::DynamicScope {
                        globals: targets, ..
                    } | Operation::AsyncDynamicEnter { globals: targets, .. } => {
                        names.insert("dynamic-invoke");
                        globals.extend(targets.iter().cloned());
                    }
                    Operation::Try { .. } => {
                        names.insert("try-invoke");
                    }
                    Operation::Literal(Literal::Number(_)) => {
                        names.insert("number-box");
                    }
                    Operation::Literal(Literal::Ratio { .. }) => { names.insert("bigint-new"); names.insert("ratio-literal"); }
                    Operation::Literal(Literal::Decimal { .. }) => { names.insert("bigint-new"); names.insert("decimal-literal"); }
                    Operation::Literal(Literal::String(_)) => {}
                    Operation::Literal(Literal::BigInt { big_integer, .. }) => { names.insert("bigint-new"); if *big_integer { names.insert("integer-biginteger"); } }
                    Operation::GlobalCell(global) => {
                        globals.insert(global.clone());
                    }
                    Operation::GlobalRead(global) => {
                        globals.insert(global.clone());
                        names.insert("binding-get");
                    }
                    Operation::GlobalBound(global) => {
                        globals.insert(global.clone());
                        names.insert("binding-bound");
                    }
                    Operation::GlobalWrite { global, .. } => {
                        globals.insert(global.clone());
                        names.insert("binding-set");
                    }
                    Operation::MakeGeneralClosure { body, .. } => {
                        names.extend(["closure-source-name-initialize", "arity-throw"]);
                        names.extend(["apply-join", "apply-bounded-length", "apply-first", "apply-next", "apply-fixed-arguments"]);
                        names.extend(runtime_abi::ARITY_ERROR_EXPORTS);
                        if let Some(factory) = &body.rest_class {
                            globals.insert(factory.clone());
                            names.extend(["binding-get", "invoke", "object-array-new", "constructor-descriptor", "source-constructor-plan", "source-construct-from-plan",
                                "construct-site-admit", "owned-descriptor-prototype"]);
                            rest_site_used = true;
                        }
                    }
                    Operation::MakeClosure { .. } => {
                        names.extend(["closure-source-name-initialize", "arity-throw"]);
                        names.extend(["apply-join", "apply-fixed-arguments"]);
                        names.extend(runtime_abi::ARITY_ERROR_EXPORTS);
                    }
                    Operation::Call { operands, primitive } => {
                        match runtime_abi::CALL_EXPORTS.get(operands.len() - 1) {
                            Some(name) => { names.insert(name); }
                            None => { names.insert("call-spread"); }
                        }
                        if let Some(signature) = primitive.as_ref().and_then(|shape| shape.fixed(operands.len() - 1)) {
                            for kind in &signature.parameters {
                                match kind {
                                    super::primitive::Kind::Long => { names.insert("number-cast-long"); },
                                    super::primitive::Kind::Double => { names.insert("number-cast-double"); },
                                    _ => {},
                                }
                            }
                        }
                    },
                    Operation::Bitwise { operation, .. } => {
                        names.insert(operation.export());
                        if let Some(raw) = operation.raw_export() { names.insert(raw); }
                        if matches!(operation, super::hir::Bitwise::Cast | super::hir::Bitwise::Int | super::hir::Bitwise::IntPrimitive | super::hir::Bitwise::IntFloat) {
                            names.extend(runtime_abi::CAST_RAW_EXPORTS);
                        }
                    }
                    Operation::Comparison { operation, .. } => {
                        names.insert(operation.export());
                    }
                    Operation::Array { operation: ArrayOperation::Get, arguments } if arguments.len() == 2 => {
                        names.insert("array-get");
                        names.insert("packed-array-get-2");
                    }
                    Operation::Array { operation: ArrayOperation::Set, arguments } if arguments.len() == 3 => {
                        names.insert("array-set");
                        names.insert("packed-array-set-3");
                    }
                    Operation::Array { operation, .. } => {
                        names.insert(operation.export());
                    }
                    Operation::Nominal { operation, arguments } => match operation {
                        Nominal::IsNumber | Nominal::IsRatio | Nominal::IsDecimal | Nominal::IsMathContext | Nominal::IsBigInt | Nominal::IsBigInteger | Nominal::IsString | Nominal::Array => {}
                        Nominal::DecimalNew => { names.insert("decimal-from-integer"); }
                        Nominal::DecimalCoerce => { names.insert("decimal-coerce"); }
                        Nominal::IsKeyword => { names.insert("keyword-test"); }
                        Nominal::DirectLinkDefine(_) | Nominal::DirectLinkRead(_) | Nominal::DirectLinkSet(_)
                        | Nominal::KeywordCacheGet(_) | Nominal::KeywordCacheSet(_) | Nominal::KeywordCacheEmpty(_) => {}
                        Nominal::KeywordIntern => { names.insert("keyword-intern"); }
                        Nominal::CoerceString => { names.insert("coerce-string"); }
                        Nominal::DecimalContextNew => { names.insert("decimal-context-new"); }
                        Nominal::DecimalContextGetter => { names.insert("decimal-context-getter-install"); }
                        Nominal::ObjectCoerceString => { names.insert("object-coerce-string"); }
                        Nominal::ConcatString => { names.insert("string-concat"); }
                        Nominal::JoinStrings => { names.insert("string-join"); }
                        Nominal::StringAppendLong => { names.insert("string-append-long"); }
                        Nominal::StringHash => { names.insert("string-hash"); }
                        Nominal::VectorNth => { names.insert("vector-nth"); }
                        Nominal::ScalarVectorGuard => { names.insert("scalar-vector-guard"); }
                        Nominal::ScalarMapEntryGuard => { names.insert("scalar-map-entry-guard"); }
                        Nominal::MapEntryType => { names.insert("map-entry-type-register"); }
                        Nominal::ObjectArrayCopy(backward) => {
                            names.insert(if *backward { "object-array-copy-backward" } else { "object-array-copy-forward" });
                        }
                        Nominal::ArrayMapGet | Nominal::MapContains | Nominal::MapGetOr => { names.insert("map-lookup"); }
                        Nominal::MapAssoc => { names.insert("map-assoc"); }
                        Nominal::HashMapTypes => { names.insert("hash-map-type-register"); }
                        Nominal::ChunkTypes => { names.insert("chunk-types-register"); }
                        Nominal::ChunkLongSum => { names.insert("chunk-long-sum"); }
                        Nominal::TransientVectorTypes => { names.insert("transient-vector-types-register"); }
                        Nominal::TransientVectorAppend => { names.insert("transient-vector-append"); }
                        Nominal::TransientMapTypes => { names.insert("transient-map-types-register"); }
                        Nominal::TransientMapAssoc => { names.insert("transient-map-assoc"); }
                        Nominal::TransientMapConj => { names.insert("transient-map-conj"); }
                        Nominal::ChunkNth => { names.insert("chunk-nth"); }
                        Nominal::ChunkAppend => { names.insert("chunk-buffer-append"); }
                        Nominal::ChunkArray => { names.insert("chunk-array"); }
                        Nominal::ChunkOffset => { names.insert("chunk-offset"); }
                        Nominal::ChunkSeq => { names.insert("chunk-seq"); }
                        Nominal::LongCast => { names.insert("number-cast-long"); }
                        Nominal::ChunkIndexAdd => {},
                        Nominal::UncheckedLongCast => { names.insert("number-cast-unchecked-long"); }
                        Nominal::DoubleCast => { names.insert("number-cast-double"); }
                        Nominal::PackedGet(_) => {
                            names.insert("packed-array-get-2");
                            names.insert("language-error-new");
                        }
                        Nominal::PackedSet(_) => {
                            names.insert("packed-array-set-3");
                            names.insert("language-error-new");
                        }
                        Nominal::ValueAdd => { names.insert("value-add"); }
                        Nominal::IsBoolean | Nominal::IsUndefined => {}
                        Nominal::Identical => { names.insert("identical-value"); }
                        Nominal::AllSmallIntegers | Nominal::IsLongValue | Nominal::IsObject | Nominal::IsDescriptor | Nominal::SameReference => {}
                        Nominal::NumberHash => { names.insert("number-hash"); }
                        Nominal::StringIndexOf => { names.insert("string-index-of"); }
                        Nominal::StringSlice => { names.insert("string-slice"); }
                        Nominal::StringFromUnit => { names.insert("string-from-unit"); }
                        Nominal::BindingDefined => { names.insert("binding-defined"); }
                        Nominal::BindingGetter => { names.insert("binding-getter"); }
                        Nominal::LanguageError => { names.insert("language-error-new"); }
                        Nominal::IsLanguageError => { names.insert("language-error-is"); }
                        Nominal::NativeObjectFactory => { names.insert("native-object-factory-function"); }
                        Nominal::IsNativeObject => { names.insert("native-object?"); }
                        Nominal::FixedArityFn => { names.insert("fn-fixed-arity?"); }
                        Nominal::ApplySequenceOps => { names.insert("apply-sequence-ops"); }
                        Nominal::ListFunction => { names.insert("list-function"); }
                        Nominal::ApplyCallee => { names.insert("apply-callee"); }
                        Nominal::ApplySpread => { names.insert("apply"); }
                        Nominal::ApplyToSeq => { names.insert("apply-to-seq"); }
                        Nominal::ApplyFixed => { names.insert("apply-fixed"); }
                        Nominal::IsTypeConstructor => { names.insert("class-value-is"); }
                        Nominal::ValueConstructor => { names.insert("value-constructor"); }
                        Nominal::NumericClass(_) => { names.insert("numeric-class-value"); }
                        Nominal::CheckNumericCallee => { descriptor_call_guard = true; }
                        Nominal::TypeName => { names.insert("class-value-name"); }
                        Nominal::DisplayLeaf => { names.insert("value-display-text"); }
                        Nominal::SourceFunctionName => { names.insert("closure-source-name"); }
                        Nominal::NativeObjectDefaultPrototype => { names.insert("native-object-default-prototype"); }
                        Nominal::CurrentThis => { names.insert("current-this"); }
                        Nominal::NativeObjectGet => { names.insert("native-object-property-get"); }
                        Nominal::NativeObjectSet => { names.insert("native-object-property-set"); }
                        Nominal::NativeObjectStrictSet => { names.insert("native-object-property-set-strict"); }
                        Nominal::ObjectSet => {
                            names.insert("object-method-set");
                        }
                        Nominal::ObjectInvoke if arguments.len() - 2 < runtime_abi::MAX_FIXED as usize => {
                            names.insert(object_method_invoke_name(arguments.len() - 2));
                        }
                        Nominal::ObjectInvoke => {
                            names.insert("object-method-invoke");
                        }
                        Nominal::CallableGet => {
                            names.insert("builtin-internal-get-guard");
                            names.insert("user-field-get-guard");
                            names.insert("callable-property-get");
                        }
                        Nominal::NamedGet => {
                            names.insert("builtin-internal-get-guard");
                            names.insert("user-field-get-guard");
                            names.insert("named-property-get");
                        }
                        Nominal::NamedSet => {
                            names.insert("builtin-internal-set-guard");
                            names.insert("user-field-set-guard");
                            names.insert("named-property-set");
                        }
                        Nominal::CoreNamedGet => {
                            names.insert("named-property-get");
                        }
                        Nominal::CoreNamedSet => {
                            names.insert("named-property-set");
                        }
                        Nominal::IsClosure => {}
                        Nominal::BindCallable => {
                            names.insert("callable-bind");
                        }
                        Nominal::CallableKeyRegister => {
                            names.insert("callable-key-register");
                        }
                        Nominal::LiveDispatcher => {
                            names.insert("protocol-live-dispatcher-new");
                        }
                        Nominal::IFnLiveDispatcher => {
                            names.insert("ifn-live-dispatcher-new");
                        }
                        Nominal::NativeMarker(_) => {
                            names.insert("protocol-native-marker-set");
                        }
                        Nominal::NativeSet(_) => {
                            names.insert("protocol-native-method-set");
                        }
                        Nominal::Descriptor(_) => {
                            names.insert("descriptor-new");
                        }
                        Nominal::CoreDescriptor => {
                            names.insert("descriptor-new-core");
                        }
                        Nominal::CoreStaticsDescriptor => {
                            names.insert("descriptor-new-core-statics");
                        }
                        Nominal::CoreClosedDescriptor => {
                            names.insert("descriptor-new-core-closed");
                        }
                        Nominal::CoreConstructibleDescriptor => {
                            names.insert("descriptor-new-core-constructible");
                        }
                        Nominal::BuiltinClassGuard => {
                            names.insert("builtin-extension-guard");
                        }
                        Nominal::BuiltinConstructGuard => {
                            names.insert("builtin-construct-guard");
                        }
                        Nominal::Protocol => {
                            names.insert("protocol-value-new");
                        }
                        Nominal::Class => {
                            names.insert("class-value-new");
                        }
                        Nominal::Construct => {
                            names.insert("constructor-descriptor");
                            names.insert("source-constructor-plan");
                            names.insert("source-construct-from-plan");
                            names.insert("construct-site-admit");
                            names.insert("owned-descriptor-prototype");
                        }
                        Nominal::SameNominalType => {
                            names.insert("object-same-descriptor");
                        }
                        Nominal::Instance => {
                            names.insert("instance-site-test");
                        }
                        Nominal::Field(_) => {
                            names.insert("object-field-get");
                        }
                        Nominal::FieldSet(_) => {
                            names.insert("object-field-set");
                        }
                        Nominal::Key(_) => {
                            names.insert("protocol-key");
                        }
                        Nominal::ObjectDescriptor => {
                            names.insert("object-descriptor");
                        }
                        Nominal::MethodGet => {
                            names.insert("protocol-class-method-get");
                        }
                        Nominal::NativeMethodGet => {
                            names.insert("protocol-native-kind");
                            names.insert("protocol-native-method");
                        }
                        Nominal::Dispatcher => {
                            names.insert("protocol-dispatcher-new");
                        }
                        Nominal::Marker => {
                            names.insert("constructor-descriptor");
                            names.insert("protocol-marker-set");
                        }
                        Nominal::Set => {
                            names.insert("constructor-descriptor");
                            names.insert("protocol-method-set");
                        }
                        Nominal::NativeSatisfies => {
                            names.insert("protocol-native-satisfies");
                        }
                        Nominal::Satisfies => {
                            names.insert("protocol-marker-satisfies");
                        }
                    },
                    Operation::Arithmetic {
                        operator,
                        arguments,
                        coercing,
                        primitive_overload,
                    } => {
                        names.insert(arithmetic_name(*operator, arguments, *coercing, *primitive_overload, function));
                    }
                    _ => {}
                }
            }
        }
    }
    // Direct links (#360): links this fragment defines, and links it calls
    // that an earlier fragment defined, whose closure global and entries it
    // imports from that fragment's `suss.link.n`.
    let mut defined_links = std::collections::BTreeMap::new();
    let mut read_links = BTreeSet::new();
    let mut set_slots = BTreeSet::new();
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        for block in &function.blocks {
            for inst in &block.instructions {
                match &inst.operation {
                    Operation::Nominal { operation: Nominal::DirectLinkDefine(link), arguments } => {
                        defined_links.insert(*link, closure_definition(function, arguments[0])
                            .ok_or_else(|| fail("Direct link of a value that isn't a closure"))?);
                    }
                    Operation::Nominal { operation: Nominal::DirectLinkRead(link), .. } => {
                        read_links.insert(*link);
                    }
                    Operation::Nominal { operation: Nominal::DirectLinkSet(slot), .. } => {
                        set_slots.insert(*slot);
                    }
                    _ => {}
                }
            }
        }
    }
    // Packed (slot, definition) operands, by part.
    let defined_definitions = defined_links.iter()
        .map(|(link, source)| (super::hir::link_parts(*link).1, (super::hir::link_parts(*link).0, source)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let owned_slots = defined_definitions.iter().filter(|(definition, (slot, _))| *definition == slot)
        .map(|(_, (slot, _))| *slot).collect::<BTreeSet<_>>();
    let mut slots = set_slots.clone();
    let mut called = BTreeSet::new();
    for link in defined_links.keys().chain(&read_links) {
        let (slot, definition) = super::hir::link_parts(*link);
        slots.insert(slot);
        if read_links.contains(link) {
            called.insert(definition);
        }
    }
    // Core field reads by a plain literal name get an inline cache (#361):
    // a site's last descriptor and its field's index, in two globals. So do
    // core field writes (#446).
    let mut field_site_list = vec![];
    let mut write_site = false;
    let mut user_site = false;
    // Of those, the `.-length` reads, which test for a string first.
    let mut length_sites = std::collections::HashSet::new();
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        let literals = function.blocks.iter().flat_map(|block| &block.instructions)
            .filter_map(|inst| match &inst.operation {
                Operation::Literal(Literal::String(units)) => Some((inst.result, units)),
                _ => None,
            })
            .collect::<std::collections::HashMap<_, _>>();
        for inst in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let Operation::Nominal { operation: operation @ (Nominal::CoreNamedGet | Nominal::CoreNamedSet | Nominal::NamedGet), arguments } = &inst.operation
                && literals.get(&arguments[1]).is_some_and(|units| plain_field_name(units))
            {
                let site = (function as *const IrFunction as usize, inst.result.0 as u32);
                // User reads, with their caller operand, too (#462).
                if *operation == Nominal::NamedGet {
                    if arguments.len() == 3 {
                        user_site = true;
                        field_site_list.push(site);
                    }
                    continue;
                }
                if *operation == Nominal::CoreNamedSet {
                    write_site = true;
                    field_site_list.push(site);
                    continue;
                }
                if literals[&arguments[1]].iter().copied().eq("length".encode_utf16()) {
                    length_sites.insert(site);
                }
                field_site_list.push(site);
            }
        }
    }
    if !field_site_list.is_empty() {
        names.insert("named-site-get");
    }
    if write_site {
        names.insert("named-site-set");
    }
    if user_site {
        names.insert("user-named-site-get");
    }
    // Protocol method lookups cache two descriptors (#355), each with its
    // method and the protocol epoch it was found at.
    let mut method_site_list = vec![];
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        for inst in function.blocks.iter().flat_map(|block| &block.instructions) {
            // Marker tests (`implements?`) cache their answer the same way.
            if let Operation::Nominal { operation: Nominal::MethodGet | Nominal::Satisfies, .. } = &inst.operation {
                method_site_list.push((function as *const IrFunction as usize, inst.result.0 as u32));
            }
        }
    }
    // `instance?` sites cache their constructor's descriptor and prototype
    // (#378): the descriptor is immutable and the prototype is created once.
    let mut instance_site_list = vec![];
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        for inst in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let Operation::Nominal { operation: Nominal::Instance, .. } = &inst.operation {
                instance_site_list.push((function as *const IrFunction as usize, inst.result.0 as u32));
            }
        }
    }
    // Construction sites cache a type they build directly (#443), as
    // construct-site-admit decides: its constructor, descriptor and prototype,
    // which is created once.
    let mut construct_site_list = vec![];
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        for inst in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let Operation::Nominal { operation: Nominal::Construct | Nominal::ScalarVectorGuard | Nominal::ScalarMapEntryGuard, .. } = &inst.operation {
                construct_site_list.push((function as *const IrFunction as usize, inst.result.0 as u32));
            }
        }
    }
    // Native protocol sites cache their protocol fn, the receiver's kind, the
    // protocol epoch and the implementation found (#445).
    let mut native_site_list = vec![];
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        for inst in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let Operation::Nominal { operation: Nominal::NativeMethodGet, .. } = &inst.operation {
                native_site_list.push((function as *const IrFunction as usize, inst.result.0 as u32));
            }
        }
    }
    // Closure call sites cache their last callee and its entry (#378): a
    // closure's code never changes, and an entry held in a global calls
    // without wasmtime's out-of-line function-reference read from a struct.
    // Callees the function makes or direct-links call their entries
    // statically.
    let mut call_site_list = vec![];
    let mut primitive_site_list = vec![];
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        let static_callees = function.blocks.iter().flat_map(|block| &block.instructions)
            .filter(|inst| matches!(&inst.operation,
                Operation::MakeClosure { .. } | Operation::MakeGeneralClosure { .. }
                | Operation::Nominal { operation: Nominal::DirectLinkRead(_), .. }))
            .map(|inst| inst.result)
            .collect::<std::collections::HashSet<_>>();
        for inst in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let Operation::Call { operands, primitive } = &inst.operation
                && operands.len() - 1 <= runtime_abi::MAX_FIXED as usize
                && !static_callees.contains(&operands[0])
            {
                call_site_list.push(((function as *const IrFunction as usize, inst.result.0 as u32), operands.len() as u32 - 1));
                if let Some(shape) = primitive && shape.fixed(operands.len() - 1).is_some() {
                    primitive_site_list.push(((function as *const IrFunction as usize, inst.result.0 as u32), shape, operands.len() - 1));
                }
            }
        }
    }
    // Method call sites, `(.m obj …)`, cache their last method, its function
    // and its entry (#462), as closure call sites do: three globals each.
    let mut object_call_site_list = vec![];
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        for inst in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let Operation::Nominal { operation: Nominal::ObjectInvoke, arguments } = &inst.operation
                && arguments.len() >= 2
                && arguments.len() - 2 < runtime_abi::MAX_FIXED as usize
            {
                let n = arguments.len() - 2;
                names.insert(object_method_site_name(n));
                names.insert(object_method_call_name(n));
                object_call_site_list.push(((function as *const IrFunction as usize, inst.result.0 as u32), n as u32));
            }
        }
    }
    // Keyword literal caches (#387): one global per keyword cell read.
    let mut keyword_cache_list = std::collections::BTreeSet::new();
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        for inst in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let Operation::Nominal { operation: Nominal::KeywordCacheGet(cache) | Nominal::KeywordCacheSet(cache) | Nominal::KeywordCacheEmpty(cache), .. } = &inst.operation {
                keyword_cache_list.insert(*cache);
            }
        }
    }
    for (_, k) in &call_site_list {
        names.insert(runtime_abi::CALL_ENTRY_EXPORTS[*k as usize]);
        names.insert(runtime_abi::CALL_FALLBACK_EXPORTS[*k as usize]);
    }

    let imported_slots = slots.iter().copied().filter(|slot| !owned_slots.contains(slot)).collect::<Vec<_>>();
    let imported_definitions = called.iter().copied().filter(|definition| !defined_definitions.contains_key(definition)).collect::<Vec<_>>();
    for definition in &imported_definitions {
        for k in 0..=runtime_abi::MAX_FIXED {
            names.insert(Box::leak(format!("link.{definition}.{k}").into_boxed_str()));
        }
    }
    let mut typed_links = std::collections::BTreeMap::new();
    let mut call_shapes = Vec::new();
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        let linked = function.blocks.iter().flat_map(|block| &block.instructions).filter_map(|instruction|
            match instruction.operation {
                Operation::Nominal { operation: Nominal::DirectLinkRead(link), .. } => Some((instruction.result, link)),
                _ => None,
            }).collect::<std::collections::HashMap<_, _>>();
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let Operation::Call { operands, primitive: Some(shape) } = &instruction.operation {
                call_shapes.push(shape);
                if let Some(link) = linked.get(&operands[0])
                    && imported_definitions.contains(&super::hir::link_parts(*link).1)
                    && let Some(signature) = shape.fixed(operands.len() - 1)
                {
                    let name: &'static str = Box::leak(format!("link.{}.{}.typed", super::hir::link_parts(*link).1, operands.len() - 1).into_boxed_str());
                    typed_links.insert(name, signature);
                    names.insert(name);
                }
            }
        }
    }
    let names = names.into_iter().collect::<Vec<_>>();
    let globals = globals.into_iter().collect::<Vec<_>>();
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    let type_count = runtime_abi::TYPE_COUNT;
    for (i, name) in names.iter().enumerate() {
        let (params, results) = match *name {
            name if name.starts_with("number-cast-") && name.ends_with("-raw") =>
                (vec![VALUE], vec![if runtime_abi::CAST_RAW_EXPORTS.iter().position(|export| *export == name)
                    .is_some_and(|index| (8..=11).contains(&index)) { ValType::F64 } else { ValType::I64 }]),
            name if name.starts_with("primitive-bit-") && name.ends_with("-raw") =>
                (vec![VALUE; if name == "primitive-bit-not-raw" { 1 } else { 2 }],
                    vec![if name == "primitive-bit-test-raw" { ValType::I32 } else { ValType::I64 }]),
            name if name == "primitive-unsigned-bit-shift-right-raw" => (vec![VALUE; 2], vec![ValType::I64]),
            name if name.starts_with("int32-") && name.ends_with("-raw") =>
                (vec![VALUE; if matches!(name, "int32-int-raw" | "int32-bit-count-raw") { 1 } else { 2 }], vec![ValType::I64]),
            "future-pending-new" | "dynamic-fork" | "dynamic-save" | "random-double" => (vec![], vec![VALUE]),
            "evaluate-form" => (vec![VALUE], vec![VALUE]),
            "async-continuation-new" | "async-continuation-snapshot" | "dynamic-push" => (vec![VALUE], vec![VALUE]),
            "async-continuation-commit" | "async-task-start" | "async-task-yield" => (vec![VALUE, VALUE], vec![VALUE]),
            "dynamic-pop" => (vec![VALUE], vec![]),
            "async-registration-new" => (vec![VALUE, VALUE, VALUE], vec![VALUE]),
            "async-task-complete" => (vec![VALUE, ValType::I32, VALUE], vec![ValType::I32]),
            "protocol-live-dispatcher-new" => (vec![VALUE, VALUE], vec![VALUE]),
            "descriptor-new"
            | "descriptor-new-core"
            | "descriptor-new-core-statics"
            | "descriptor-new-core-closed"
            | "descriptor-new-core-constructible" => (vec![VALUE, VALUE, VALUE], vec![VALUE]),
            "ifn-live-dispatcher-new" => (vec![VALUE, VALUE, VALUE], vec![VALUE]),
            "protocol-native-marker-set" => (vec![VALUE, ValType::I32], vec![ValType::I32]),
            "protocol-native-method-set" => (vec![VALUE, ValType::I32, VALUE], vec![VALUE]),
            "try-invoke" => (vec![VALUE, VALUE, VALUE], vec![VALUE]),
            "binding-defined" | "language-error-is" | "native-object?" | "fn-fixed-arity?" | "class-value-is" => (vec![VALUE], vec![ValType::I32]),
            "apply-sequence-ops" => (vec![VALUE; 10], vec![VALUE]),
            "list-function" => (vec![VALUE; 5], vec![VALUE]),
            "apply-callee" | "apply-first" | "apply-next" | "apply-fixed-arguments" => (vec![VALUE], vec![VALUE]),
            "apply-to-seq" | "apply-fixed" => (vec![VALUE; 2], vec![VALUE]),
            "apply-join" => (vec![ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::ARGS) }), VALUE], vec![VALUE]),
            "apply" => (vec![VALUE, ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::ARGS) }), VALUE], vec![VALUE]),
            "apply-bounded-length" => (vec![VALUE, ValType::I32], vec![ValType::I32]),
            "identical-value" => (vec![VALUE, VALUE], vec![VALUE]),
            "object-instance" | "object-same-descriptor" | "protocol-marker-satisfies" | "protocol-native-satisfies" => {
                (vec![VALUE, VALUE], vec![ValType::I32])
            }
            "primitive-int" | "primitive-bit-not" | "int32-int" | "int32-bit-count" => (vec![VALUE], vec![VALUE]),
            "primitive-bit-and"
            | "primitive-bit-or"
            | "primitive-bit-xor"
            | "primitive-bit-and-not"
            | "primitive-bit-clear"
            | "primitive-bit-flip"
            | "primitive-bit-set"
            | "primitive-bit-test"
            | "primitive-bit-shift-left"
            | "primitive-bit-shift-right"
            | "primitive-unsigned-bit-shift-right"
            | "int32-bit-and"
            | "int32-bit-or"
            | "int32-bit-xor"
            | "int32-bit-shift-left"
            | "int32-bit-shift-right"
            | "int32-unsigned-bit-shift-right"
            | "int32-imul" => (vec![VALUE, VALUE], vec![VALUE]),
            "comparison-less"
            | "comparison-less-equal"
            | "comparison-greater"
            | "comparison-greater-equal"
            | "comparison-strict-equal"
            | "comparison-case-equal"
            | "comparison-number-less"
            | "comparison-number-less-equal"
            | "comparison-number-greater"
            | "comparison-number-greater-equal"
            | "comparison-number-equal"
            | "comparison-number-zero"
            | "comparison-number-positive"
            | "comparison-number-negative" => (vec![VALUE, VALUE], vec![VALUE]),
            "object-field-set" => (vec![VALUE, ValType::I32, VALUE], vec![]),
            "object-field-get" | "protocol-key" => (vec![VALUE, ValType::I32], vec![VALUE]),
            "protocol-marker-set" | "protocol-method-get" | "protocol-class-method-get" => (vec![VALUE, VALUE], vec![VALUE]),
            "protocol-method-set" => (vec![VALUE, VALUE, VALUE], vec![]),
            "builtin-extension-guard"
            | "builtin-construct-guard"
            | "class-value-new"
            | "protocol-value-new"
            | "constructor-new"
            | "source-constructor-new"
            | "source-constructor-plan"
            | "value-constructor"
            | "class-value-name"
            | "value-display-text"
            | "constructor-descriptor"
            | "object-descriptor"
            | "protocol-dispatcher-new" => (vec![VALUE], vec![VALUE]),
            "construct-site-admit" | "protocol-native-method" => (vec![VALUE, ValType::I32], vec![VALUE]),
            "protocol-native-kind" => (vec![VALUE], vec![ValType::I32]),
            "source-construct-from-plan" => (
                vec![VALUE, ValType::Ref(RefType {
                    nullable: false,
                    heap_type: HeapType::Concrete(runtime_abi::ARGS),
                })],
                vec![VALUE],
            ),
            "number-hash"
            | "owned-descriptor-prototype"
            | "array-clone"
            | "object-array-new"
            | "object-array-make-args"
            | "array-length-args"
            | "array-get-indices"
            | "array-set-indices" => (vec![VALUE], vec![VALUE]),
            "predicate-number" | "predicate-string" | "arity-error" | "native-object-factory-function" | "native-object-default-prototype" | "current-this" => (vec![], vec![VALUE]),
            "scalar-vector-guard" | "scalar-map-entry-guard" => (vec![VALUE, VALUE], vec![ValType::I32]),
            "map-entry-type-register" => (vec![VALUE], vec![VALUE]),
            "number-box" => (vec![ValType::F64], vec![VALUE]),
            "ratio-new" | "ratio-literal" | "decimal-from-integer" => (vec![VALUE, VALUE], vec![VALUE]),
            "decimal-literal" => (vec![VALUE, ValType::I32], vec![VALUE]),
            "decimal-coerce" => (vec![VALUE], vec![VALUE]),
            "decimal-context-new" => (vec![VALUE,VALUE],vec![VALUE]),
            "decimal-context-getter-install" => (vec![VALUE],vec![VALUE]),
            "bigint-new" => (vec![ValType::I32, ValType::Ref(RefType {
                nullable: true, heap_type: HeapType::Concrete(runtime_abi::PACKED_STORAGE + 2),
            }), ValType::I32], vec![VALUE]),
            "closure-source-name-initialize" => (vec![VALUE, VALUE], vec![]),
            "closure-source-name" => (vec![VALUE], vec![VALUE]),
            "closure-new" => (
                vec![
                    VALUE,
                    ValType::Ref(RefType {
                        nullable: false,
                        heap_type: HeapType::Concrete(runtime_abi::INVOKE),
                    }),
                    ValType::I32,
                    ValType::I32,
                ],
                vec![VALUE],
            ),
            "ratio-numerator" | "ratio-denominator" | "ratio-integer" | "integer-bigint" | "integer-biginteger" | "number-promote-negate" | "binding-get" | "binding-bound" | "number-negate" | "value-negate"
            | "checked-negate" | "checked-identity-add" | "checked-identity-multiply"
            | "primitive-f64-coerce" | "primitive-f64-word0" | "primitive-f64-word4"
            | "primitive-f64-floor" | "primitive-f64-ceil" | "primitive-f64-finite" | "primitive-f64-safe-integer" | "primitive-f64-time-clip" | "identity-uid" | "coerce-string" | "object-coerce-string" | "language-error-new" | "string-from-unit" | "string-join"
            | "number-abs" | "number-unchecked-negate" | "number-long?" | "number-double?" | "number-to-long" | "number-int-value" | "case-number-hash" | "number-hash" | "string-hash" | "number-cast-int" | "number-cast-int-float" | "number-cast-long" | "number-cast-double" | "number-cast-unchecked-long" | "packed-array-test" | "object-array-test" | "vector-type-register" | "array-map-type-register" | "vector-count" => {
                (vec![VALUE], vec![VALUE])
            }
            "binding-set" => (vec![VALUE, VALUE], vec![]),
            "number-parse-long" | "hash-map-type-register" => (vec![VALUE; 4], vec![VALUE]),
            "chunk-types-register" | "transient-vector-types-register" => (vec![VALUE; 3], vec![VALUE]),
            "chunk-nth" | "chunk-buffer-append" | "transient-vector-append" | "transient-map-conj" => (vec![VALUE; 2], vec![VALUE]),
            "chunk-long-sum" => (vec![VALUE; 6], vec![VALUE]),
            "chunk-array" | "chunk-offset" | "chunk-seq" => (vec![VALUE], vec![VALUE]),
            "transient-map-types-register" | "packed-array-copy" | "object-array-copy-forward" | "object-array-copy-backward" => (vec![VALUE; 5], vec![VALUE]),
            "number-range-count" | "packed-array-store" | "array-set" | "packed-array-set-3" | "map-assoc" | "transient-map-assoc" => (vec![VALUE; 3], vec![VALUE]),
            "map-lookup" => (vec![VALUE, VALUE, ValType::I32, VALUE], vec![VALUE]),
            "string-slice" | "callable-property-get" | "named-property-set" | "user-field-get-guard" | "user-field-set-guard" | "object-method-set" | "object-method-invoke" | "native-object-property-set" | "native-object-property-set-strict" => {
                (vec![VALUE, VALUE, VALUE], vec![VALUE])
            }
            "string-new" => (vec![ValType::I32], vec![VALUE]),
            "numeric-class-value" => (vec![ValType::I32], vec![VALUE]),
            "instance-site-test" => (
                vec![ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::NAMED_SITE) }), VALUE, VALUE],
                vec![ValType::I32],
            ),
            name if name.starts_with("call-entry-") => {
                let k: u32 = name["call-entry-".len()..].parse().expect("call-entry-k");
                (vec![VALUE], vec![ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::CALL + k) })])
            }
            "named-site-get" => (
                vec![ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::NAMED_SITE) }), VALUE, VALUE],
                vec![VALUE],
            ),
            "named-site-set" | "user-named-site-get" => (
                vec![ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::NAMED_SITE) }), VALUE, VALUE, VALUE],
                vec![VALUE],
            ),
            name if name.starts_with("object-method-site-") => {
                let n: usize = name["object-method-site-".len()..].parse().expect("object-method-site-N");
                (vec![VALUE; n + 2], vec![VALUE])
            }
            name if name.starts_with("object-method-call-") => {
                let n: u32 = name["object-method-call-".len()..].parse().expect("object-method-call-N");
                let mut params = vec![VALUE, ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::CALL + n + 1) })];
                params.extend(vec![VALUE; n as usize + 1]);
                (params, vec![VALUE])
            }
            name if name.starts_with("object-method-invoke-") => {
                let n: usize = name["object-method-invoke-".len()..].parse().expect("object-method-invoke-N");
                (vec![VALUE; n + 2], vec![VALUE])
            }
            "arity-throw" => (vec![VALUE, ValType::I32], vec![VALUE]),
            "keyword-intern" => (vec![VALUE], vec![VALUE]),
            "keyword-test" => (vec![VALUE], vec![ValType::I32]),
            "named-field-slot" => (vec![VALUE, VALUE], vec![ValType::I32]),
            "string-set-unit" => (vec![VALUE, ValType::I32, ValType::I32], vec![ValType::I32]),
            _ => (vec![VALUE, VALUE], vec![VALUE]),
        };
        let (params, results) = if let Some(k) = runtime_abi::CALL_EXPORTS.iter().position(|call| call == name) {
            (vec![VALUE; k + 1], vec![VALUE])
        } else {
            (params, results)
        };
        let (params, results) = if let Some(signature) = typed_links.get(name) {
            let mut parameters = vec![VALUE];
            parameters.extend(signature.parameters.iter().copied().map(primitive::value_type));
            (parameters, vec![primitive::value_type(signature.result)])
        } else { (params, results) };
        types.ty().function(params, results);
        // An arity error or a call fallback is an entry, of the shared
        // group's `$callk` type.
        let entry = runtime_abi::ARITY_ERROR_EXPORTS.iter().position(|error| error == name)
            .or_else(|| runtime_abi::CALL_FALLBACK_EXPORTS.iter().position(|fallback| fallback == name));
        let ty = match entry {
            Some(k) => runtime_abi::CALL + k as u32,
            None => type_count + i as u32,
        };
        if let Some((link, _)) = name.strip_suffix(".typed").and_then(link_entry) {
            imports.import(&format!("suss.link.{link}"), name, EntityType::Function(ty));
        } else if let Some((link, k)) = link_entry(name) {
            imports.import(&format!("suss.link.{link}"), name, EntityType::Function(runtime_abi::CALL + k));
        } else {
            imports.import("suss.runtime", name, EntityType::Function(ty));
        }
    }
    for global in &globals {
        imports.import(
            global.import_module(),
            &global.import_name(),
            EntityType::Global(GlobalType {
                val_type: runtime_abi::binding_cell_type(),
                mutable: false,
                shared: false,
            }),
        );
    }
    // False, true and undefined follow the binding cells (`sentinels`).
    for (name, _) in runtime_abi::SENTINEL_EXPORTS {
        imports.import(
            "suss.runtime",
            name,
            EntityType::Global(GlobalType {
                val_type: runtime_abi::sentinel_type(),
                mutable: false,
                shared: false,
            }),
        );
    }
    // Imported var slots, then imported definitions' code structs, follow
    // the sentinels.
    for slot in &imported_slots {
        imports.import(
            &format!("suss.slot.{slot}"),
            &format!("slot.{slot}"),
            EntityType::Global(GlobalType { val_type: VALUE, mutable: true, shared: false }),
        );
    }
    for definition in &imported_definitions {
        imports.import(
            &format!("suss.link.{definition}"),
            &format!("link.{definition}.code"),
            EntityType::Global(GlobalType { val_type: runtime_abi::code_type(), mutable: false, shared: false }),
        );
    }
    // The protocol epoch follows the imported link globals. Field sites also
    // cache methods under it (#378).
    let epoch_used = !method_site_list.is_empty() || !field_site_list.is_empty() || !instance_site_list.is_empty() || !native_site_list.is_empty();
    if epoch_used {
        imports.import(
            "suss.runtime",
            "protocol-epoch",
            EntityType::Global(GlobalType { val_type: ValType::I32, mutable: true, shared: false }),
        );
    }
    let imported_link_globals = (imported_slots.len() + imported_definitions.len()) as u32;
    let epoch_global = globals.len() as u32 + 3 + imported_link_globals;
    let imported_link_globals = imported_link_globals + u32::from(epoch_used);
    let not_callable_global = descriptor_call_guard.then_some(globals.len() as u32 + 3 + imported_link_globals);
    if descriptor_call_guard {
        imports.import("suss.runtime", runtime_abi::NOT_CALLABLE_DESCRIPTOR_EXPORT,
            EntityType::Global(GlobalType {
                val_type: ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::DESCRIPTOR) }),
                mutable: false, shared: false,
            }));
    }
    let imported_link_globals = imported_link_globals + u32::from(descriptor_call_guard);
    let eval_type = type_count + names.len() as u32;
    types.ty().function([], [VALUE]);
    if throws {
        types.ty().function([VALUE], []);
        imports.import(
            "suss.runtime",
            "language-exception",
            EntityType::Tag(TagType {
                kind: TagKind::Exception,
                func_type_idx: eval_type + 1,
            }),
        );
    }
    let mut layout = Layout::new(
        &bodies, &singles, &dispatchers, futures.len(), &names,
        eval_type + 1 + u32::from(throws), globals.len() as u32 + 3 + imported_link_globals,
    ).map_err(|message| fail(message))?;
    captures::plan(ir, &mut layout);
    for definition in &layout.definitions {
        let mut fields = runtime_abi::closure_fields();
        fields.extend(definition.capture_reprs.iter().map(|repr| FieldType {
            element_type: StorageType::Val(repr.val_type().unwrap_or(VALUE)),
            mutable: false,
        }));
        types.ty().subtype(&SubType {
            is_final: true,
            supertype_idx: Some(runtime_abi::CLOSURE),
            composite_type: CompositeType {
                inner: CompositeInnerType::Struct(StructType { fields: fields.into_boxed_slice() }),
                shared: false,
                descriptor: None,
                describes: None,
            },
        });
    }
    let mut next_type = eval_type + 1 + u32::from(throws) + layout.definitions.len() as u32;
    for body in bodies.iter().filter(|body| body.arity > runtime_abi::MAX_FIXED as usize) {
        let mut params = vec![closure_type(false)];
        params.extend(std::iter::repeat_n(VALUE, body.arity));
        types.ty().function(params, [VALUE]);
        next_type += 1;
    }
    // A linked definition's closure: a capture struct with no captures,
    // the same canonical type in every fragment.
    let link_type = next_type;
    if !read_links.is_empty() {
        types.ty().subtype(&SubType {
            is_final: true,
            supertype_idx: Some(runtime_abi::CLOSURE),
            composite_type: CompositeType {
                inner: CompositeInnerType::Struct(StructType { fields: runtime_abi::closure_fields().into_boxed_slice() }),
                shared: false,
                descriptor: None,
                describes: None,
            },
        });
    }
    let primitive = primitive::Types::new(&mut types, next_type + u32::from(!read_links.is_empty()), &bodies, &layout, &call_shapes);
    // Each slot's global, and each called definition's code struct global and
    // entries: imported, or this fragment's.
    let first_imported_link = globals.len() as u32 + 3;
    let first_owned_slot = first_imported_link + imported_link_globals + layout.definitions.len() as u32;
    let mut links = Links { link_type, slots: Default::default(), codes: Default::default(), entries: Default::default() };
    for (index, slot) in imported_slots.iter().enumerate() {
        links.slots.insert(*slot, first_imported_link + index as u32);
    }
    for (index, slot) in owned_slots.iter().enumerate() {
        links.slots.insert(*slot, first_owned_slot + index as u32);
    }
    let first_field_site = first_owned_slot + owned_slots.len() as u32;
    let field_sites = field_site_list.iter().enumerate()
        .map(|(index, site)| (*site, first_field_site + FIELD_SITE_GLOBALS * index as u32))
        .collect::<std::collections::HashMap<_, _>>();
    let first_method_site = first_field_site + FIELD_SITE_GLOBALS * field_site_list.len() as u32;
    let method_sites = method_site_list.iter().enumerate()
        .map(|(index, site)| (*site, first_method_site + METHOD_SITE_GLOBALS * index as u32))
        .collect::<std::collections::HashMap<_, _>>();
    let first_instance_site = first_method_site + METHOD_SITE_GLOBALS * method_site_list.len() as u32;
    let instance_sites = instance_site_list.iter().enumerate()
        .map(|(index, site)| (*site, first_instance_site + index as u32))
        .collect::<std::collections::HashMap<_, _>>();
    let first_call_site = first_instance_site + instance_site_list.len() as u32;
    let call_sites = call_site_list.iter().enumerate()
        .map(|(index, (site, _))| (*site, first_call_site + 2 * index as u32))
        .collect::<std::collections::HashMap<_, _>>();
    let first_keyword_cache = first_call_site + 2 * call_site_list.len() as u32;
    let keyword_caches = keyword_cache_list.iter().enumerate()
        .map(|(index, cache)| (*cache, first_keyword_cache + index as u32))
        .collect::<std::collections::HashMap<_, _>>();
    // String literals and closure display names (#442): one global per
    // distinct value, after the keyword caches.
    let mut literal_list = std::collections::BTreeSet::new();
    for function in std::iter::once(ir).chain(bodies.iter().map(|body| &body.function))
        .chain(futures.iter().map(|body| &body.continuation.function))
    {
        for inst in function.blocks.iter().flat_map(|block| &block.instructions) {
            match &inst.operation {
                Operation::Literal(Literal::String(units)) => {
                    literal_list.insert(units.clone());
                }
                Operation::Nominal { operation: Nominal::CheckNumericCallee, .. } => {
                    literal_list.insert("Not callable".encode_utf16().collect());
                }
                Operation::MakeClosure { body, .. } => literal_list.extend(body.display_name.clone()),
                Operation::MakeGeneralClosure { body, .. } => literal_list.extend(body.display_name.clone()),
                _ => {}
            }
        }
    }
    let first_literal = first_keyword_cache + keyword_cache_list.len() as u32;
    let literals = literal_list.iter().enumerate()
        .map(|(index, units)| (units.clone(), first_literal + index as u32))
        .collect::<std::collections::HashMap<_, _>>();
    let first_construct_site = first_literal + literal_list.len() as u32;
    let construct_sites = construct_site_list.iter().enumerate()
        .map(|(index, site)| (*site, first_construct_site + 3 * index as u32))
        .collect::<std::collections::HashMap<_, _>>();
    let first_native_site = first_construct_site + 3 * construct_site_list.len() as u32;
    let first_object_call_site = first_native_site + 4 * native_site_list.len() as u32;
    let object_call_sites = object_call_site_list.iter().enumerate()
        .map(|(index, (site, _))| (*site, first_object_call_site + 3 * index as u32))
        .collect::<std::collections::HashMap<_, _>>();
    // The module's variadic rest seqs share one construction site (#466).
    let rest_site = rest_site_used.then_some(first_object_call_site + 3 * object_call_site_list.len() as u32 + primitive_site_list.len() as u32);
    let native_sites = native_site_list.iter().enumerate()
        .map(|(index, site)| (*site, first_native_site + 4 * index as u32))
        .collect::<std::collections::HashMap<_, _>>();
    let first_primitive_site = first_object_call_site + 3 * object_call_site_list.len() as u32;
    let primitive_sites = primitive_site_list.iter().enumerate().map(|(index, (site, _, _))|
        (*site, first_primitive_site + index as u32)).collect::<std::collections::HashMap<_, _>>();
    for (index, definition) in imported_definitions.iter().enumerate() {
        links.codes.insert(*definition, first_imported_link + imported_slots.len() as u32 + index as u32);
        let entries = (0..=runtime_abi::MAX_FIXED).map(|k| {
            names.iter().position(|name| *name == format!("link.{definition}.{k}")).expect("link import") as u32
        }).collect::<Vec<_>>();
        links.entries.insert(*definition, entries);
    }
    let mut defined_code = vec![];
    for (definition, (_, source)) in &defined_definitions {
        let index = match source {
            ClosureSource::Single(body) => layout.single_index(body),
            ClosureSource::General(body) => layout.general_index(body),
        };
        links.codes.insert(*definition, layout.definitions[index].global);
        links.entries.insert(*definition, layout.definitions[index].entries.to_vec());
        defined_code.push((*definition, layout.definitions[index].global, index));
    }
    let mut functions = FunctionSection::new();
    functions.function(eval_type);
    let mut exports = ExportSection::new();
    exports.export("eval", ExportKind::Func, names.len() as u32);
    let mut code = CodeSection::new();
    let mut data = LiteralData::default();
    let context = Context { names: &names, globals: &globals, bodies: &bodies,
        dispatchers: &dispatchers, futures: &futures, layout: &layout, links: &links, field_sites: &field_sites, length_sites: &length_sites,
        method_sites: &method_sites, instance_sites: &instance_sites, call_sites: &call_sites, keyword_caches: &keyword_caches,
        literals: &literals, construct_sites: &construct_sites, native_sites: &native_sites, object_call_sites: &object_call_sites, rest_site, primitive: &primitive, primitive_sites: &primitive_sites, epoch_global, not_callable_global };
    code.function(&emit_function(ir, Entry::Eval, &context, None, &mut data)?);
    for (index, body) in bodies.iter().enumerate() {
        functions.function(layout.body_types[index]);
        code.function(&if body.primitive.is_some() { primitive::bridge(index, &context) }
            else { emit_function(&body.function, Entry::Method(index), &context, None, &mut data)? });
    }
    for dispatcher in &dispatchers {
        let definition = layout.general(dispatcher);
        functions.function(runtime_abi::SPREAD);
        code.function(&emit_spread(definition, &context));
    }
    for body in &futures {
        functions.function(runtime_abi::INVOKE);
        code.function(&emit_function(&body.continuation.function, Entry::Future,
            &context, Some(&body.continuation), &mut data)?);
    }
    for extra in &layout.extras {
        match *extra {
            Extra::Spread(definition) => {
                functions.function(runtime_abi::SPREAD);
                code.function(&emit_spread(&layout.definitions[definition], &context));
            }
            Extra::Adapter { definition, arity } => {
                functions.function(runtime_abi::CALL + arity as u32);
                code.function(&emit_adapter(&layout.definitions[definition], arity, &context));
            }
            Extra::Primitive(body) => {
                functions.function(primitive.entries[bodies[body].primitive.as_ref().expect("primitive entry")]);
                code.function(&emit_function(&bodies[body].function, Entry::Primitive(body), &context, None, &mut data)?);
            }
        }
    }
    // Each definition's code struct: an immutable global with a constant
    // initializer, after the imported cells and sentinels (ADR-0004).
    let mut code_globals = GlobalSection::new();
    for (definition_index, definition) in layout.definitions.iter().enumerate() {
        let mut init = match &definition.name {
            Some(name) => {
                let mut init: Vec<_> = name.iter().map(|unit| Instruction::I32Const(i32::from(*unit))).collect();
                init.push(Instruction::ArrayNewFixed { array_type_index: runtime_abi::STRING, array_size: name.len() as u32 });
                init
            }
            None => vec![runtime_abi::sentinel_in(runtime_abi::NIL, 0)],
        };
        init.extend(definition.entries.iter().map(|entry| Instruction::RefFunc(*entry)));
        let minimum = definition.methods.iter().map(|&(_, arity, variadic)| arity - usize::from(variadic)).min().unwrap_or(0);
        let maximum = if definition.methods.iter().any(|method| method.2) {
            -1
        } else {
            definition.methods.iter().map(|method| method.1 as i32).max().unwrap_or(0)
        };
        init.extend([
            Instruction::RefFunc(definition.spread),
            Instruction::I32Const(minimum as i32),
            Instruction::I32Const(maximum),
        ]);
        for method in &primitive.shapes[definition_index].0 {
            let body = definition.methods.iter().find(|(_, arity, variadic)|
                *arity == method.signature.parameters.len() && *variadic == method.variadic).expect("primitive method").0;
            init.push(Instruction::RefFunc(layout.primitive_entries[body].expect("primitive entry")));
        }
        init.push(Instruction::StructNew(primitive.code_type(definition_index)));
        code_globals.global(
            GlobalType { val_type: runtime_abi::code_type(), mutable: false, shared: false },
            &ConstExpr::extended(init),
        );
    }
    // Owned var slots, set by each linked definition, and each defined
    // definition's code struct and entries, exported for later fragments.
    for slot in &owned_slots {
        code_globals.global(
            GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::extended([runtime_abi::sentinel_in(runtime_abi::NIL, 0)]),
        );
        exports.export(&format!("slot.{slot}"), ExportKind::Global, links.slots[slot]);
    }
    for _ in &field_site_list {
        // The site's cache, a struct of its own (#378).
        code_globals.global(
            GlobalType {
                val_type: ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::NAMED_SITE) }),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended(runtime_abi::named_site_initializer()),
        );
    }
    for _ in &method_site_list {
        for _ in 0..2 {
            for ty in [VALUE, VALUE] {
                code_globals.global(
                    GlobalType { val_type: ty, mutable: true, shared: false },
                    &ConstExpr::extended([runtime_abi::sentinel_in(runtime_abi::NIL, 0)]),
                );
            }
            code_globals.global(
                GlobalType { val_type: ValType::I32, mutable: true, shared: false },
                &ConstExpr::i32_const(-1),
            );
        }
    }
    for _ in &instance_site_list {
        // The site's cache, a NAMED_SITE struct (#378).
        code_globals.global(
            GlobalType {
                val_type: ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::NAMED_SITE) }),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended(runtime_abi::named_site_initializer()),
        );
    }
    for (_, k) in &call_site_list {
        // The site's last callee, and its entry for k arguments (#378). The
        // site starts with nil and the fallback entry, which calls `call-k`
        // and so is right for any callee.
        code_globals.global(
            GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::ref_null(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }),
        );
        let fallback = names.iter().position(|name| *name == runtime_abi::CALL_FALLBACK_EXPORTS[*k as usize]).expect("call fallback");
        code_globals.global(
            GlobalType {
                val_type: ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::CALL + k) }),
                mutable: true,
                shared: false,
            },
            &ConstExpr::ref_func(fallback as u32),
        );
    }
    for _ in &keyword_cache_list {
        code_globals.global(
            GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::ref_null(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }),
        );
    }
    for units in &literal_list {
        let (ty, init) = literal_global(units);
        code_globals.global(ty, &init);
    }
    for _ in 0..3 * construct_site_list.len() {
        code_globals.global(
            GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::ref_null(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }),
        );
    }
    for _ in &native_site_list {
        // Protocol fn, kind, epoch, implementation; null matches no fn.
        code_globals.global(
            GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::ref_null(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }),
        );
        for _ in 0..2 {
            code_globals.global(GlobalType { val_type: ValType::I32, mutable: true, shared: false }, &ConstExpr::i32_const(-1));
        }
        code_globals.global(
            GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::extended([runtime_abi::sentinel_in(runtime_abi::NIL, 0)]),
        );
    }
    for (_, n) in &object_call_site_list {
        // The site's last method and its function, null to match none, and
        // that function's entry for this, then n arguments (#462).
        for _ in 0..2 {
            code_globals.global(
                GlobalType { val_type: VALUE, mutable: true, shared: false },
                &ConstExpr::ref_null(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }),
            );
        }
        code_globals.global(
            GlobalType {
                val_type: ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(runtime_abi::CALL + n + 1) }),
                mutable: true,
                shared: false,
            },
            &ConstExpr::ref_null(HeapType::Concrete(runtime_abi::CALL + n + 1)),
        );
    }
    for (_, shape, arity) in &primitive_site_list {
        let entry = primitive.entries[shape.fixed(*arity).expect("primitive call signature")];
        code_globals.global(GlobalType { val_type: ValType::Ref(RefType { nullable: true,
            heap_type: HeapType::Concrete(entry) }), mutable: true, shared: false },
            &ConstExpr::ref_null(HeapType::Concrete(entry)));
    }
    if rest_site.is_some() {
        // The rest seq's constructor, null to match none, its admitted
        // descriptor and its prototype (#466).
        for _ in 0..3 {
            code_globals.global(
                GlobalType { val_type: VALUE, mutable: true, shared: false },
                &ConstExpr::ref_null(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }),
            );
        }
    }
    for (definition, global, layout_index) in &defined_code {
        exports.export(&format!("link.{definition}.code"), ExportKind::Global, *global);
        for (k, entry) in links.entries[definition].iter().enumerate() {
            exports.export(&format!("link.{definition}.{k}"), ExportKind::Func, *entry);
        }
        for (body, arity, variadic) in &layout.definitions[*layout_index].methods {
            if !*variadic && let Some(entry) = layout.primitive_entries[*body] {
                exports.export(&format!("link.{definition}.{arity}.typed"), ExportKind::Func, entry);
            }
        }
    }
    let mut elements = ElementSection::new();
    let defined = bodies.len() + dispatchers.len() + futures.len() + layout.extras.len();
    if defined > 0 {
        let indices = (0..defined)
            .map(|i| u32::try_from(names.len() + 1 + i).map_err(|_| fail("Too many functions")))
            .collect::<Result<Vec<_>, _>>()?;
        elements.declared(Elements::Functions(Cow::Owned(indices)));
    }
    let mut module = Module::new();
    module
        .section(&runtime_abi::Manifest::default().section())
        .section(&types)
        .section(&imports)
        .section(&functions);
    if !layout.definitions.is_empty() || !field_site_list.is_empty() || !method_site_list.is_empty() || !instance_site_list.is_empty() || !call_site_list.is_empty() || !keyword_cache_list.is_empty() || !literal_list.is_empty() || !construct_site_list.is_empty() || !native_site_list.is_empty() || !object_call_site_list.is_empty() || rest_site_used {
        module.section(&code_globals);
    }
    module.section(&exports);
    if defined > 0 {
        module.section(&elements);
    }
    if !data.segments.is_empty() {
        module.section(&DataCountSection { count: data.segments.len() as u32 });
    }
    module.section(&code);
    if !data.segments.is_empty() {
        let mut segments = DataSection::new();
        for segment in &data.segments {
            segments.passive(segment.iter().copied());
        }
        module.section(&segments);
    }
    // Function names for profilers and backtraces, only when asked for
    // (SUSS_FUNCTION_NAMES): imports by export, then eval, closure bodies by
    // their arity names, dispatchers, futures and the layout's extras (#443).
    if runtime_abi::function_names_wanted() {
        let mut map = wasm_encoder::NameMap::new();
        let text = |units: &Option<Vec<u16>>| units.as_ref().map(|units| String::from_utf16_lossy(units));
        let mut index = 0u32;
        for name in &names {
            map.append(index, &format!("suss.runtime/{name}"));
            index += 1;
        }
        map.append(index, "eval");
        index += 1;
        for (position, body) in bodies.iter().enumerate() {
            let name = text(&body.arity_name).or_else(|| text(&body.display_name)).filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("fn#{position}@{}..{}", body.function.span.start, body.function.span.end));
            map.append(index, &format!("{name}/{}{}", body.arity, if body.variadic { "+" } else { "" }));
            index += 1;
        }
        for (position, dispatcher) in dispatchers.iter().enumerate() {
            let name = text(&dispatcher.arity_name).or_else(|| text(&dispatcher.display_name)).unwrap_or_else(|| format!("fn#{position}"));
            map.append(index, &format!("{name}/spread"));
            index += 1;
        }
        for position in 0..futures.len() {
            map.append(index, &format!("future#{position}"));
            index += 1;
        }
        for extra in &layout.extras {
            let (definition, label) = match *extra {
                Extra::Spread(definition) => (definition, "spread".to_string()),
                Extra::Adapter { definition, arity } => (definition, format!("adapter{arity}")),
                Extra::Primitive(body) => (layout.body_definitions[body], format!("primitive{}", bodies[body].arity)),
            };
            let name = text(&layout.definitions[definition].name).unwrap_or_else(|| format!("def#{definition}"));
            map.append(index, &format!("{name}/{label}"));
            index += 1;
        }
        let mut section = wasm_encoder::NameSection::new();
        section.functions(&map);
        module.section(&section);
    }
    let bytes = module.finish();
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).map_err(|message| {
        Diagnostic {
            span: ir.span.clone(),
            message,
        }
    })?;
    wasmparser::Validator::new()
        .validate_all(&bytes)
        .map_err(|error| Diagnostic {
            span: ir.span.clone(),
            message: format!("Generated fragment failed validation: {error}"),
        })?;
    Ok(bytes)
}

fn collect_bodies<'a>(
    function: &'a IrFunction,
    bodies: &mut Vec<&'a ClosureBody>,
    dispatchers: &mut Vec<&'a GeneralClosureBody>,
    futures: &mut Vec<&'a FutureBody>,
) {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
    for block in &function.blocks {
        for inst in &block.instructions {
            match &inst.operation {
                Operation::MakeFuture { body, .. } => {
                    futures.push(body);
                    collect_bodies(&body.continuation.function, bodies, dispatchers, futures);
                }
                Operation::MakeClosure { body, .. } => {
                    bodies.push(body);
                    collect_bodies(&body.function, bodies, dispatchers, futures);
                }
                Operation::MakeGeneralClosure { body, .. } => {
                    dispatchers.push(body);
                    for method in &body.methods {
                        bodies.push(method);
                        collect_bodies(&method.function, bodies, dispatchers, futures);
                    }
                }
                _ => {}
            }
        }
    }
    });
}
/// Where a function body begins: the fragment's `eval`, a resumable future's
/// universal continuation, or a method of a function definition, whose
/// parameters are its closure and its arguments (ADR-0004).
#[derive(Clone, Copy)]
enum Entry {
    Eval,
    Future,
    Method(usize),
    Primitive(usize),
}
/// What every function in one fragment needs to know about the others.
struct Context<'a> {
    names: &'a [&'a str],
    globals: &'a [super::resolve::Global],
    bodies: &'a [&'a ClosureBody],
    dispatchers: &'a [&'a GeneralClosureBody],
    futures: &'a [&'a FutureBody],
    layout: &'a Layout,
    links: &'a Links,
    /// Cached core field read sites, by (function, result): their first global.
    field_sites: &'a std::collections::HashMap<(usize, u32), u32>,
    length_sites: &'a std::collections::HashSet<(usize, u32)>,
    /// Keyword literal caches, by keyword cell number: their global (#387).
    keyword_caches: &'a std::collections::HashMap<usize, u32>,
    /// String literals, by their units: their global (#442).
    literals: &'a std::collections::HashMap<Vec<u16>, u32>,
    /// Cached closure call sites: their first global (#378).
    call_sites: &'a std::collections::HashMap<(usize, u32), u32>,
    /// Cached `instance?` sites: their first global (#378).
    instance_sites: &'a std::collections::HashMap<(usize, u32), u32>,
    construct_sites: &'a std::collections::HashMap<(usize, u32), u32>,
    native_sites: &'a std::collections::HashMap<(usize, u32), u32>,
    /// Cached method call sites: their first global (#462).
    object_call_sites: &'a std::collections::HashMap<(usize, u32), u32>,
    /// The module's rest-seq construction site: its first global (#466).
    rest_site: Option<u32>,
    /// Cached protocol method lookup sites: their first global; the epoch.
    method_sites: &'a std::collections::HashMap<(usize, u32), u32>,
    epoch_global: u32,
    not_callable_global: Option<u32>,
    primitive: &'a primitive::Types,
    primitive_sites: &'a std::collections::HashMap<(usize, u32), u32>,
}
/// One function definition (`fn`): its closures' capture struct, its code
/// struct global and its entries (ADR-0004).
struct Definition {
    /// Methods as (body index, arity including any rest parameter, variadic).
    methods: Vec<(usize, usize, bool)>,
    rest_class: Option<super::resolve::Global>,
    structure: u32,
    global: u32,
    /// Captures held as fields; a general closure's self reference is its
    /// closure parameter instead.
    captures: usize,
    capture_reprs: Vec<unboxed::Repr>,
    self_capture: bool,
    spread: u32,
    entries: [u32; runtime_abi::MAX_FIXED as usize + 1],
    name: Option<Vec<u16>>,
}
/// Functions after the futures: single-method spread entries and variadic adapters.
enum Extra {
    Spread(usize),
    Adapter { definition: usize, arity: usize },
    Primitive(usize),
}
struct Layout {
    definitions: Vec<Definition>,
    /// General closures in collection order; general closure j is definition j.
    generals: Vec<*const GeneralClosureBody>,
    /// For each single-method closure body, in collection order, its definition.
    singles: Vec<(*const ClosureBody, usize)>,
    /// Per body: its function type, and its definition.
    body_types: Vec<u32>,
    body_definitions: Vec<usize>,
    primitive_entries: Vec<Option<u32>>,
    extras: Vec<Extra>,
}
impl Layout {
    fn new(
        bodies: &[&ClosureBody],
        singles: &[&ClosureBody],
        dispatchers: &[&GeneralClosureBody],
        futures: usize,
        names: &[&str],
        first_type: u32,
        first_global: u32,
    ) -> Result<Self, &'static str> {
        let max = runtime_abi::MAX_FIXED as usize;
        let body_index = |body: &ClosureBody| {
            bodies.iter().position(|candidate| std::ptr::eq(*candidate, body)).expect("collected body")
        };
        let function = |index: usize| u32::try_from(names.len() + 1 + index).map_err(|_| "Too many functions");
        let import = |name: &str| names.iter().position(|candidate| *candidate == name).map(|index| index as u32);
        let extras_base = bodies.len() + dispatchers.len() + futures;
        let mut layout = Layout {
            definitions: vec![], generals: vec![], singles: vec![],
            body_types: vec![0; bodies.len()], body_definitions: vec![0; bodies.len()], extras: vec![],
            primitive_entries: vec![None; bodies.len()],
        };
        let mut sources = vec![];
        layout.generals = dispatchers.iter().map(|general| *general as *const GeneralClosureBody).collect();
        for (index, general) in dispatchers.iter().enumerate() {
            let methods = general.methods.iter()
                .map(|method| (body_index(method), method.arity, method.variadic)).collect::<Vec<_>>();
            let spread = function(bodies.len() + index)?;
            sources.push((methods, general.rest_class.clone(), general.capture_types.len(), general.self_capture, spread, general.arity_name.clone()));
        }
        for single in singles {
            let spread = function(extras_base + layout.extras.len())?;
            layout.extras.push(Extra::Spread(dispatchers.len() + layout.singles.len()));
            layout.singles.push((*single as *const ClosureBody, dispatchers.len() + layout.singles.len()));
            sources.push((vec![(body_index(single), single.arity, false)], None, single.capture_types.len(), false, spread, single.arity_name.clone()));
        }
        for (definition, (methods, rest_class, captures, self_capture, spread, name)) in sources.into_iter().enumerate() {
            let mut entries = [0; runtime_abi::MAX_FIXED as usize + 1];
            for (k, entry) in entries.iter_mut().enumerate() {
                *entry = import(runtime_abi::ARITY_ERROR_EXPORTS[k]).ok_or("Missing arity error import")?;
            }
            for &(body, arity, variadic) in &methods {
                if !variadic && arity <= max {
                    entries[arity] = function(body)?;
                }
            }
            if let Some(&(_, arity, _)) = methods.iter().find(|method| method.2) {
                let required = arity - 1;
                for k in required..=max {
                    if !methods.iter().any(|&(_, fixed, variadic)| !variadic && fixed == k) {
                        entries[k] = function(extras_base + layout.extras.len())?;
                        layout.extras.push(Extra::Adapter { definition, arity: k });
                    }
                }
            }
            for &(body, _, _) in &methods {
                layout.body_definitions[body] = definition;
            }
            layout.definitions.push(Definition {
                methods, rest_class, structure: first_type + definition as u32,
                global: first_global + definition as u32, captures,
                capture_reprs: vec![unboxed::Repr::Boxed; captures], self_capture,
                spread, entries, name,
            });
        }
        let mut big = first_type + layout.definitions.len() as u32;
        for (index, body) in bodies.iter().enumerate() {
            layout.body_types[index] = if body.arity <= max {
                runtime_abi::CALL + body.arity as u32
            } else {
                big += 1;
                big - 1
            };
            if body.primitive.is_some() {
                layout.primitive_entries[index] = Some(function(extras_base + layout.extras.len())?);
                layout.extras.push(Extra::Primitive(index));
            }
        }
        Ok(layout)
    }
    fn general_index(&self, body: &GeneralClosureBody) -> usize {
        self.generals.iter().position(|general| std::ptr::eq(*general, body)).expect("collected general closure")
    }
    fn single_index(&self, body: &ClosureBody) -> usize {
        self.singles.iter().find(|(single, _)| std::ptr::eq(*single, body)).expect("collected closure").1
    }
    fn general(&self, body: &GeneralClosureBody) -> &Definition {
        &self.definitions[self.general_index(body)]
    }
    fn single(&self, body: &ClosureBody) -> &Definition {
        &self.definitions[self.single_index(body)]
    }
}
/// `[] -> [ref $closure]`: a new closure of `definition` over the captures
/// in `captures`, with no properties and an unassigned UID.
fn emit_closure(function: &mut Function, definition: &Definition, captures: impl Iterator<Item = u32>) {
    use Instruction::*;
    function
        .instruction(&GlobalGet(definition.global))
        .instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, 0))
        .instruction(&I32Const(0))
        .instruction(&RefI31);
    for capture in captures {
        function.instruction(&LocalGet(capture));
    }
    function.instruction(&StructNew(definition.structure));
}
/// How a value was made, for a direct link's definition (#360).
enum ClosureSource<'a> {
    Single(&'a ClosureBody),
    General(&'a GeneralClosureBody),
}
/// The closure `value` holds in `function`, when one of its instructions made it.
fn closure_definition(function: &IrFunction, value: ir::ValueId) -> Option<ClosureSource<'_>> {
    function.blocks.iter().flat_map(|block| &block.instructions).find(|inst| inst.result == value)
        .and_then(|inst| match &inst.operation {
            Operation::MakeClosure { body, .. } => Some(ClosureSource::Single(body)),
            Operation::MakeGeneralClosure { body, .. } => Some(ClosureSource::General(body)),
            _ => None,
        })
}
/// A field name a schema holds as written: letters, digits and `_`, not
/// starting with a digit, so no munging applies (#361).
/// Whether locals `a` and `b` both hold i31 values, as an i32.
fn i31_operands(function: &mut Function, a: u32, b: u32) {
    let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
    function
        .instruction(&Instruction::LocalGet(a))
        .instruction(&Instruction::RefTestNonNull(i31))
        .instruction(&Instruction::LocalGet(b))
        .instruction(&Instruction::RefTestNonNull(i31))
        .instruction(&Instruction::I32And);
}
/// Sets i64 local `raw` to GC local `local`'s long, an i31 or a boxed long,
/// or branches out of the enclosing block for anything else (#385).
fn long_operand(function: &mut Function, local: u32, raw: u32) {
    use Instruction::*;
    let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
    function
        .instruction(&LocalGet(local))
        .instruction(&RefTestNonNull(i31))
        .instruction(&If(BlockType::Result(ValType::I64)))
        .instruction(&LocalGet(local))
        .instruction(&RefCastNonNull(i31))
        .instruction(&I31GetS)
        .instruction(&I64ExtendI32S)
        .instruction(&Else)
        .instruction(&LocalGet(local))
        .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::LONG)))
        .instruction(&I32Eqz)
        .instruction(&BrIf(1))
        .instruction(&LocalGet(local))
        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::LONG)))
        .instruction(&StructGet { struct_type_index: runtime_abi::LONG, field_index: 0 })
        .instruction(&End)
        .instruction(&LocalSet(raw));
}
/// The i31 values of locals `a` and `b`, sign-extended.
fn i31_values(function: &mut Function, a: u32, b: u32) {
    let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
    for local in [a, b] {
        function
            .instruction(&Instruction::LocalGet(local))
            .instruction(&Instruction::RefCastNonNull(i31))
            .instruction(&Instruction::I31GetS);
    }
}
/// The inline start of an unhinted aget or aset (#446): opens the result
/// block and the generic path's block inside it. An object array, a packed
/// array of kind 6, and an i31 index in its range set `storage` and `slot`
/// and fall through, for the caller to access the element and branch out of
/// the result block; anything else branches to the generic path, which the
/// caller ends with two `End`s.
fn object_array_element(array: u32, position: u32, storage: u32, slot: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
    let packed = RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::PACKED) };
    let args = RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::ARGS) };
    vec![
        Block(BlockType::Result(VALUE)),
        Block(BlockType::Empty),
        Block(BlockType::Result(ValType::Ref(args))),
        Block(BlockType::Result(ValType::Ref(packed))),
        LocalGet(array),
        BrOnCast { relative_depth: 0, from_ref_type: RefType::EQREF, to_ref_type: packed },
        Drop,
        Br(2),
        End,
        StructGet { struct_type_index: runtime_abi::PACKED, field_index: 0 },
        BrOnCast { relative_depth: 0, from_ref_type: RefType::EQREF, to_ref_type: args },
        Drop,
        Br(1),
        End,
        LocalSet(storage),
        LocalGet(position),
        RefTestNonNull(i31),
        I32Eqz,
        BrIf(0),
        LocalGet(position),
        RefCastNonNull(i31),
        I31GetS,
        LocalTee(slot),
        LocalGet(storage),
        ArrayLen,
        I32GeU,
        BrIf(0),
    ]
}
fn plain_field_name(units: &[u16]) -> bool {
    !units.is_empty()
        && !(units[0] >= b'0' as u16 && units[0] <= b'9' as u16)
        && units.iter().all(|&unit| unit < 128 && ((unit as u8).is_ascii_alphanumeric() || unit == b'_' as u16))
}
/// A link entry import's name, `link.n.k`, as (n, k).
fn link_entry(name: &str) -> Option<(usize, u32)> {
    let (link, k) = name.strip_prefix("link.")?.split_once('.')?;
    Some((link.parse().ok()?, k.parse().ok()?))
}
/// Direct links (#360): each link's closure global and its entries' function
/// indices, by link, and the capture-free closure type their closures share.
struct Links {
    link_type: u32,
    /// Var slot globals, by slot.
    slots: std::collections::BTreeMap<usize, u32>,
    /// Code struct globals and entries, by definition.
    codes: std::collections::BTreeMap<usize, u32>,
    entries: std::collections::BTreeMap<usize, Vec<u32>>,
}
/// A nullable or non-null reference to `$closure`.
fn closure_type(nullable: bool) -> ValType {
    ValType::Ref(RefType { nullable, heap_type: HeapType::Concrete(runtime_abi::CLOSURE) })
}
/// `[] -> [eqref]`: a variadic method's rest seq over the values in the
/// `ARGS` local `surplus`, an `IndexedSeq` from index 0, built as the pinned
/// wrapper builds it over a fresh object array (#425); Internal Args never becomes the
/// language rest sequence.
fn emit_rest(function: &mut Function, definition: &Definition, surplus: u32, context: &Context) {
    use Instruction::*;
    let factory = context.globals
        .binary_search(definition.rest_class.as_ref().expect("verified rest class"))
        .expect("collected rest class") as u32;
    let import = |name: &str| context.names.iter().position(|candidate| *candidate == name).expect("collected import") as u32;
    // `(IndexedSeq. arr 0 nil)` through the module's construction site, as a
    // construction site builds it (#443): inline while the site holds the
    // constructor, else admitted or built by the constructor's plan (#466).
    let site = context.rest_site.expect("rest site");
    let nil = runtime_abi::sentinel_in(runtime_abi::NIL, context.globals.len() as u32);
    let constructor = [GlobalGet(factory), Call(import("binding-get"))];
    let fields = [LocalGet(surplus), Call(import("object-array-new")), I32Const(0), RefI31, nil,
        ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: 3 }];
    let mut code = constructor.to_vec();
    // nil is null, as an empty site is: only a held constructor matches.
    code.extend([GlobalGet(site), RefEq, GlobalGet(site), RefIsNull, I32Eqz, I32And, If(BlockType::Result(VALUE)),
        GlobalGet(site + 1), RefCastNonNull(HeapType::Concrete(runtime_abi::DESCRIPTOR))]);
    code.extend(fields.clone());
    code.extend([GlobalGet(site + 2), ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: 1 },
        I32Const(0), RefI31, StructNew(runtime_abi::OBJECT), Else]);
    // Only a closure, a type's constructor, is admitted; anything else the
    // var holds takes the plan's path and its error, as before.
    code.extend(constructor.clone());
    code.extend([RefTestNonNull(HeapType::Concrete(runtime_abi::CLOSURE)), If(BlockType::Result(VALUE))]);
    code.extend(constructor.clone());
    code.extend([Call(import("constructor-descriptor")), I32Const(3), Call(import("construct-site-admit")),
        Else, RefNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }), End,
        GlobalSet(site + 1), GlobalGet(site + 1), RefIsNull, If(BlockType::Empty),
        RefNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }), GlobalSet(site), Else,
        GlobalGet(site + 1), Call(import("owned-descriptor-prototype")), GlobalSet(site + 2)]);
    code.extend(constructor.clone());
    code.extend([GlobalSet(site), End]);
    code.extend(constructor);
    code.extend([Call(import("constructor-descriptor")), Call(import("source-constructor-plan"))]);
    code.extend(fields);
    code.extend([Call(import("source-construct-from-plan")), End]);
    for instruction in &code {
        function.instruction(instruction);
    }
}
/// The spread entry, `(closure, args, rest)`: calls the method that takes
/// `args`' length, as the arity dispatcher did, or throws the arity error.
fn emit_spread(definition: &Definition, context: &Context) -> Function {
    use Instruction::*;
    let names = context.names;
    let method = |body: usize| (names.len() + 1 + body) as u32;
    // Locals: 3 the argument count, 4 the surplus array, 5 the apply seq.
    let mut function = Function::new([(1, ValType::I32), (2, VALUE)]);
    let import = |name: &str| names.iter().position(|candidate| *candidate == name).expect("apply import") as u32;
    // Ordinary calls keep their existing realized-array/IndexedSeq path. Apply
    // passes an already normalized rest: never seq that value a second time.
    function
        .instruction(&LocalGet(2)).instruction(&RefIsNull).instruction(&I32Eqz)
        .instruction(&If(BlockType::Empty))
        .instruction(&LocalGet(1)).instruction(&LocalGet(2)).instruction(&Call(import("apply-join")))
        .instruction(&LocalSet(5));
    if let Some(&(body, arity, _)) = definition.methods.iter().find(|method| method.2) {
        let required = arity - 1;
        function.instruction(&LocalGet(5)).instruction(&I32Const(required as i32))
            .instruction(&Call(import("apply-bounded-length")))
            .instruction(&I32Const(required as i32)).instruction(&I32GtU)
            .instruction(&If(BlockType::Empty)).instruction(&LocalGet(0));
        for _ in 0..required {
            function.instruction(&LocalGet(5)).instruction(&Call(import("apply-first")))
                .instruction(&LocalGet(5)).instruction(&Call(import("apply-next")))
                .instruction(&LocalSet(5));
        }
        function.instruction(&LocalGet(5)).instruction(&ReturnCall(method(body))).instruction(&End);
    }
    function.instruction(&LocalGet(5)).instruction(&Call(import("apply-fixed-arguments")))
        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS))).instruction(&LocalSet(1))
        .instruction(&End)
        .instruction(&LocalGet(1)).instruction(&ArrayLen).instruction(&LocalSet(3));
    for &(body, arity, _) in definition.methods.iter().filter(|method| !method.2) {
        function
            .instruction(&LocalGet(3)).instruction(&I32Const(arity as i32)).instruction(&I32Eq)
            .instruction(&If(BlockType::Empty)).instruction(&LocalGet(0));
        for index in 0..arity {
            function.instruction(&LocalGet(1)).instruction(&I32Const(index as i32)).instruction(&ArrayGet(runtime_abi::ARGS));
        }
        function.instruction(&ReturnCall(method(body))).instruction(&End);
    }
    if let Some(&(body, arity, _)) = definition.methods.iter().find(|method| method.2) {
        let required = arity - 1;
        function
            .instruction(&LocalGet(3)).instruction(&I32Const(required as i32)).instruction(&I32GeU)
            .instruction(&If(BlockType::Empty)).instruction(&LocalGet(0));
        for index in 0..required {
            function.instruction(&LocalGet(1)).instruction(&I32Const(index as i32)).instruction(&ArrayGet(runtime_abi::ARGS));
        }
        function
            .instruction(&LocalGet(3)).instruction(&I32Const(required as i32)).instruction(&I32GtU)
            .instruction(&If(BlockType::Result(VALUE)))
            .instruction(&LocalGet(3)).instruction(&I32Const(required as i32)).instruction(&I32Sub)
            .instruction(&ArrayNewDefault(runtime_abi::ARGS)).instruction(&LocalSet(4))
            .instruction(&LocalGet(4)).instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)))
            .instruction(&I32Const(0))
            .instruction(&LocalGet(1)).instruction(&I32Const(required as i32))
            .instruction(&LocalGet(3)).instruction(&I32Const(required as i32)).instruction(&I32Sub)
            .instruction(&ArrayCopy { array_type_index_dst: runtime_abi::ARGS, array_type_index_src: runtime_abi::ARGS });
        emit_rest(&mut function, definition, 4, context);
        function
            .instruction(&Else)
            .instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, 0))
            .instruction(&End)
            .instruction(&ReturnCall(method(body)))
            .instruction(&End);
    }
    let error = names.iter().position(|name| *name == "arity-throw").expect("arity helper");
    function.instruction(&LocalGet(0));
    // AFn's final invoke overload is twenty scalars plus Object[] and reports
    // arity21, even when that array contains more than one argument. Retain
    // full counts for Suss's existing wider-arity extension methods.
    if definition.methods.iter().all(|&(_, arity, variadic)| arity - usize::from(variadic) <= 20) {
        function.instruction(&LocalGet(3)).instruction(&I32Const(20)).instruction(&I32GtU)
            .instruction(&If(BlockType::Result(ValType::I32))).instruction(&I32Const(21))
            .instruction(&Else).instruction(&LocalGet(3)).instruction(&End);
    } else {
        function.instruction(&LocalGet(3));
    }
    function.instruction(&Call(error as u32)).instruction(&End);
    function
}
/// A variadic method's entry for `arity` arguments: its required arguments,
/// then the rest as a seq over the surplus, or nil when there is none.
fn emit_adapter(definition: &Definition, arity: usize, context: &Context) -> Function {
    use Instruction::*;
    let (body, method_arity, _) = *definition.methods.iter().find(|method| method.2).expect("variadic method");
    let required = method_arity - 1;
    // Local arity + 1: the surplus array.
    let mut function = Function::new([(1, VALUE)]);
    for local in 0..=required as u32 {
        function.instruction(&LocalGet(local));
    }
    if arity > required {
        for local in required + 1..=arity {
            function.instruction(&LocalGet(local as u32));
        }
        function
            .instruction(&ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: (arity - required) as u32 })
            .instruction(&LocalSet(arity as u32 + 1));
        emit_rest(&mut function, definition, arity as u32 + 1, context);
    } else {
        function.instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, 0));
    }
    function.instruction(&ReturnCall((context.names.len() + 1 + body) as u32)).instruction(&End);
    function
}
/// A call can leave its frame only when its result is forwarded unchanged to
/// return through empty blocks. Argument/callee evaluation already happened in
/// verified IR; live globals and pending effects are never reconstructed here.
fn returns_unchanged<'a>(ir: &'a IrFunction, mut terminator: &'a Terminator, mut value: ir::ValueId) -> bool {
    for _ in 0..=ir.blocks.len() {
        match terminator {
            Terminator::Return(result) => return *result == value,
            Terminator::Jump { target, arguments } => {
                let Some(position) = arguments.iter().position(|argument| *argument == value)
                else {
                    return false;
                };
                let next = &ir.blocks[*target];
                if !next.instructions.is_empty() {
                    return false;
                }
                value = next.parameters[position];
                terminator = &next.terminator;
            }
            _ => return false,
        }
    }
    false
}

fn emit_function(
    ir: &IrFunction,
    entry: Entry,
    context: &Context,
    continuation: Option<&ir::r#async::Continuation>,
    data: &mut LiteralData,
) -> Result<Function, Diagnostic> {
    let Context { names, globals, bodies, dispatchers, futures, layout, links, field_sites, length_sites, method_sites, instance_sites, construct_sites, native_sites, object_call_sites, call_sites, keyword_caches, literals, epoch_global, .. } = *context;
    let fail = |message: &str| Diagnostic {
        span: ir.span.clone(),
        message: message.into(),
    };
    let count = u32::try_from(ir.values.len()).map_err(|_| fail("Too many IR values"))?;
    let sentinels = globals.len() as u32;
    // Parameters before the SSA locals: none for `eval`; a future's
    // environment and arguments; a method's closure and arguments.
    let offset = match entry {
        Entry::Eval => 0,
        Entry::Future => 2,
        Entry::Method(body) | Entry::Primitive(body) => 1 + u32::try_from(bodies[body].arity).map_err(|_| fail("Too many parameters"))?,
    };
    let scratch = count
        .checked_add(offset)
        .ok_or_else(|| fail("Too many IR locals"))?;
    let pc = count
        .checked_add(14)
        .and_then(|count| count.checked_add(offset))
        .ok_or_else(|| fail("Too many IR locals"))?;
    let index = |name: &str| {
        names
            .iter()
            .position(|item| *item == name)
            .expect("collected import") as u32
    };
    // Every SSA value has one GC local. The last local is the block dispatcher,
    // used only by resumable states and irreducible graphs; every other
    // function emits structured control flow (#358). Raw numeric values get
    // extra typed locals after the dispatcher (#337).
    let structured = if continuation.is_none() { structured::plan(ir) } else { None };
    let dispatch = structured.is_none();
    let signature = match entry {
        Entry::Primitive(body) => bodies[body].primitive.as_ref(), _ => None,
    };
    let mut entry_reprs = match entry {
        Entry::Method(body) | Entry::Primitive(body) => {
            let definition = &layout.definitions[layout.body_definitions[body]];
            ir.blocks[0].parameters.iter().copied().zip(definition.capture_reprs.iter().copied())
                .filter(|(_, repr)| *repr != unboxed::Repr::Boxed).collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };
    if let Entry::Primitive(body) = entry {
        for (value, kind) in ir.blocks[0].parameters.iter().skip(bodies[body].capture_types.len())
            .zip(&signature.expect("primitive body").parameters)
        {
            if *kind != super::primitive::Kind::Boxed { entry_reprs.push((*value, primitive::repr(*kind))); }
        }
    }
    let raw_result = signature.filter(|signature| signature.result != super::primitive::Kind::Boxed)
        .map(|signature| primitive::repr(signature.result));
    let locals = unboxed::Locals::new(unboxed::infer(ir, continuation.is_some(), &entry_reprs, raw_result, &captures::raw_uses(ir, layout)), ir, pc + 1, offset, sentinels);
    if let Some(expected) = raw_result {
        for block in &ir.blocks {
            if let Terminator::Return(value) = block.terminator && locals.of(value) != expected {
                return Err(fail("IR primitive entry result kind mismatch"));
            }
        }
    }
    let mut declarations = vec![(count + 14, VALUE), (1, ValType::I32)];
    declarations.extend(locals.declarations.iter().copied());
    // A call site's callee, once it is known to be a closure.
    let callee = pc + 1 + locals.declarations.iter().map(|(count, _)| count).sum::<u32>();
    declarations.push((1, closure_type(true)));
    // A cached field read's looked-up index.
    let field_index_local = callee + 1;
    declarations.push((1, ValType::I32));
    // A cached named read's looked-up method (#378).
    let method_local = callee + 2;
    declarations.push((1, VALUE));
    // A boxed-long add or subtract's operands and result (#385).
    let long_locals = [callee + 3, callee + 4, callee + 5];
    declarations.push((3, ValType::I64));
    // An inline packed access's storage, one local per kind (#387).
    let storage_locals = callee + 6;
    // An object array's storage is the args array (#378).
    for kind in 0..7 {
        declarations.push((1, ValType::Ref(wasm_encoder::RefType {
            nullable: false,
            heap_type: HeapType::Concrete(if kind == 6 { runtime_abi::ARGS } else { runtime_abi::PACKED_STORAGE + kind }),
        })));
    }
    // A source object, cast once for an `instance?` site's inline tests (#457).
    let object_local = storage_locals + 7;
    declarations.push((1, ValType::Ref(wasm_encoder::RefType { nullable: true, heap_type: HeapType::Concrete(7) })));
    // Loop-invariant array views (#389): an inline aget or aset in a loop,
    // of an array defined before the loop, casts the array to its storage
    // once, at the end of the block that enters the loop, into a nullable
    // local of that storage type; null if the array isn't of that kind. In
    // the loop, the access reads the local instead of casting the array.
    // Each access takes its innermost such loop.
    let first_view = storage_locals + 8;
    let mut view_of: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut view_inits: Vec<(usize, u32, usize, u32)> = Vec::new();
    if continuation.is_none() {
        let mut defined = vec![usize::MAX; ir.values.len()];
        for (index, block) in ir.blocks.iter().enumerate() {
            for parameter in &block.parameters {
                defined[parameter.0] = index;
            }
            for inst in &block.instructions {
                defined[inst.result.0] = index;
            }
        }
        let mut loops = ir::single_entry_loops(ir);
        loops.sort_by_key(|(_, blocks)| blocks.len());
        let mut keys: std::collections::HashMap<(usize, u32, usize), u32> = std::collections::HashMap::new();
        for (entry_block, blocks) in &loops {
            let inside = blocks.iter().copied().collect::<std::collections::HashSet<_>>();
            for block in blocks {
                for inst in &ir.blocks[*block].instructions {
                    let (Operation::Nominal { operation: Nominal::PackedGet(kind) | Nominal::PackedSet(kind), arguments }) = &inst.operation else { continue };
                    let result = inst.result.0 as u32;
                    if view_of.contains_key(&result) || arguments.is_empty() {
                        continue;
                    }
                    let array = arguments[0];
                    let home = defined.get(array.0).copied().unwrap_or(usize::MAX);
                    if home == usize::MAX || inside.contains(&home) {
                        continue;
                    }
                    let key = (*entry_block, array.0 as u32, *kind);
                    let local = *keys.entry(key).or_insert_with(|| {
                        let local = first_view + view_inits.len() as u32;
                        view_inits.push((*entry_block, array.0 as u32, *kind, local));
                        local
                    });
                    view_of.insert(result, local);
                }
            }
        }
        for (_, _, kind, _) in &view_inits {
            declarations.push((1, ValType::Ref(wasm_encoder::RefType {
                nullable: true,
                heap_type: HeapType::Concrete(if *kind == 6 { runtime_abi::ARGS } else { runtime_abi::PACKED_STORAGE + *kind as u32 }),
            })));
        }
    }
    // Parameter field arrays (#445): a parameter that two or more field reads
    // take as their object, in a function no branch re-enters at its first
    // block, has its object's fields array read once on entry, into a
    // nullable local of the args type: null when it isn't an object.
    let mut fields_of: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut descriptors_of: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    if continuation.is_none() && ir.blocks.first().is_some()
        && !ir.blocks.iter().any(|block| match &block.terminator {
            Terminator::Jump { target, .. } => *target == 0,
            Terminator::Branch { consequent, alternative, .. } => *consequent == 0 || *alternative == 0,
            _ => false,
        })
    {
        let mut reads: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        let mut named_reads: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        for inst in ir.blocks.iter().flat_map(|block| &block.instructions) {
            if let Operation::Nominal { operation: Nominal::Field(_), arguments } = &inst.operation {
                *reads.entry(arguments[0].0 as u32).or_default() += 1;
            }
            if let Operation::Nominal { operation: Nominal::CoreNamedGet, arguments } = &inst.operation {
                *named_reads.entry(arguments[0].0 as u32).or_default() += 1;
            }
        }
        for parameter in &ir.blocks[0].parameters {
            let named = named_reads.get(&(parameter.0 as u32)).is_some_and(|count| *count >= 3);
            if reads.get(&(parameter.0 as u32)).is_some_and(|count| *count >= 2) || named {
                let local = pc + 1 + declarations.iter().skip(2).map(|(count, _)| count).sum::<u32>();
                fields_of.insert(parameter.0 as u32, local);
                declarations.push((1, ValType::Ref(wasm_encoder::RefType { nullable: true, heap_type: HeapType::Concrete(runtime_abi::ARGS) })));
                if named {
                    descriptors_of.insert(parameter.0 as u32, local + 1);
                    declarations.push((1, VALUE));
                }
            }
        }
    }
    let mut function = Function::new(declarations);
    use Instruction::*;
    if continuation.is_none() {
    if let Entry::Method(body) | Entry::Primitive(body) = entry {
        // Captures are fields of the closure's own struct; a general closure's
        // self reference is the closure; arguments are parameters (ADR-0004).
        let definition = &layout.definitions[layout.body_definitions[body]];
        let environment = definition.captures + usize::from(definition.self_capture);
        for (index, value) in ir.blocks[0].parameters.iter().enumerate() {
            if index < definition.captures {
                function
                    .instruction(&LocalGet(0))
                    .instruction(&RefCastNonNull(HeapType::Concrete(definition.structure)))
                    .instruction(&StructGet {
                        struct_type_index: definition.structure,
                        field_index: runtime_abi::CLOSURE_FIELDS + index as u32,
                    });
            } else if index < environment {
                function.instruction(&LocalGet(0));
            } else {
                function.instruction(&LocalGet(1 + (index - environment) as u32));
            }
            function.instruction(&LocalSet(if locals.of(*value) != unboxed::Repr::Boxed {
                locals.raw(*value)
            } else { value.0 as u32 + offset }));
        }
    }
    }
    for (value, local) in &fields_of {
        function
            .instruction(&LocalGet(value + offset))
            .instruction(&RefTestNonNull(HeapType::Concrete(7)))
            .instruction(&If(BlockType::Result(ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(runtime_abi::ARGS) }))));
        // Only the immutable layout is retained. Field array elements and
        // site descriptors are still read at each original use, after any
        // callbacks, mutations, protocol extension or garbage collection.
        if let Some(descriptor) = descriptors_of.get(value) {
            function.instruction(&LocalGet(value + offset))
                .instruction(&RefCastNonNull(HeapType::Concrete(7)))
                .instruction(&StructGet { struct_type_index: 7, field_index: 0 })
                .instruction(&LocalSet(*descriptor));
        }
        function.instruction(&LocalGet(value + offset))
            .instruction(&RefCastNonNull(HeapType::Concrete(7)))
            .instruction(&StructGet { struct_type_index: 7, field_index: 1 })
            .instruction(&Else)
            .instruction(&RefNull(HeapType::Concrete(runtime_abi::ARGS)))
            .instruction(&End)
            .instruction(&LocalSet(*local));
    }
    // Callees known statically (ADR-0004): closures this function makes, and
    // a method's own closure, its self reference. A call to one calls its
    // entry directly, with no cast to `$closure` or code struct reads.
    let mut known: std::collections::HashMap<u32, (usize, bool)> = std::collections::HashMap::new();
    for block in &ir.blocks {
        for inst in &block.instructions {
            match &inst.operation {
                Operation::MakeClosure { body, .. } => { known.insert(inst.result.0 as u32, (layout.single_index(body), false)); }
                Operation::MakeGeneralClosure { body, .. } => { known.insert(inst.result.0 as u32, (layout.general_index(body), false)); }
                _ => {}
            }
        }
    }
    // A direct-linked callee (#360): its definition's entries, statically.
    let mut linked: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for block in &ir.blocks {
        for inst in &block.instructions {
            if let Operation::Nominal { operation: Nominal::DirectLinkRead(link), .. } = &inst.operation {
                linked.insert(inst.result.0 as u32, *link);
            }
        }
    }
    if let Entry::Method(body) | Entry::Primitive(body) = entry {
        let definition = &layout.definitions[layout.body_definitions[body]];
        if definition.self_capture {
            let value = ir.blocks[0].parameters[definition.captures];
            known.insert(value.0 as u32, (layout.body_definitions[body], true));
        }
    }
    let slots = r#async::Locals { scratch, snapshot: scratch + 1, cont: scratch + 2,
        live: scratch + 3, resume: scratch + 4, pc, offset,
        stack: scratch + 9, frame: scratch + 10, saved: scratch + 11,
        outcome: scratch + 12, auxiliary: scratch + 13, sentinels };
    if let Some(plan) = continuation {
        r#async::initialize(&mut function, plan, &slots, &index);
    } else if dispatch {
        function.instruction(&I32Const(0)).instruction(&LocalSet(pc));
    }
    if continuation.is_some() {
        function.instruction(&Loop(BlockType::Empty))
            .instruction(&Block(BlockType::Result(VALUE)))
            .instruction(&TryTable(BlockType::Empty, Cow::Owned(vec![wasm_encoder::Catch::One { tag: 0, label: 0 }])));
    }
    if dispatch { function.instruction(&Loop(BlockType::Empty)); }
    let state_count = continuation.map_or(ir.blocks.len(), |plan| plan.states.len());
    let steps = structured.unwrap_or_else(|| (0..state_count).map(structured::Step::Code).collect());
    for step in steps {
        let state_id = match step {
            structured::Step::Code(state_id) => state_id,
            structured::Step::Loop => { function.instruction(&Loop(BlockType::Empty)); continue; }
            structured::Step::Block => { function.instruction(&Block(BlockType::Empty)); continue; }
            structured::Step::Else => { function.instruction(&Else); continue; }
            structured::Step::End => { function.instruction(&End); continue; }
            structured::Step::Br(depth) => { function.instruction(&Br(depth)); continue; }
        };
        let state = continuation.map(|plan| &plan.states[state_id]);
        let block_id = state.map_or(state_id, |state| state.block);
        let block = &ir.blocks[block_id];
        let instructions = state.map_or(block.instructions.as_slice(), |state| &block.instructions[state.instructions.clone()]);
        let state_number = i32::try_from(state_id).map_err(|_| fail("Too many IR blocks"))?;
        if dispatch {
            function
                .instruction(&LocalGet(pc))
                .instruction(&I32Const(state_number))
                .instruction(&I32Eq)
                .instruction(&If(BlockType::Empty));
        }
        if let Some(plan) = continuation { r#async::resume_result(&mut function, plan, state_id, &slots); }
        let mut boxed = std::collections::HashSet::new();
        let mut code = Vec::new();
        locals.invalidate(&mut code, block.parameters.iter().copied()
            .chain(instructions.iter().map(|instruction| instruction.result)));
        for instruction in code.drain(..) { function.instruction(&instruction); }
        for inst in instructions {
            if continuation.is_none()
                && let Operation::Call { operands, primitive: Some(shape) } = &inst.operation
                && let Some(signature) = shape.fixed(operands.len() - 1)
                && operands.len() - 1 <= runtime_abi::MAX_FIXED as usize
            {
                primitive::call(&mut code, operands, shape, signature, inst.result, ir, context, &locals,
                    known.get(&(operands[0].0 as u32)).copied(), linked.get(&(operands[0].0 as u32)).copied(),
                    method_local, long_locals[0], &index);
                for instruction in code.drain(..) { function.instruction(&instruction); }
                continue;
            }
            if !unboxed::inline(&locals, inst)
                && let Some((export, arguments)) = unboxed::raw_helper(&locals, inst)
            {
                for argument in arguments { locals.push_boxed(&mut code, argument); }
                code.extend([Call(index(export)), LocalSet(locals.raw(inst.result))]);
                for instruction in code.drain(..) { function.instruction(&instruction); }
                continue;
            }
            if unboxed::inline(&locals, inst) {
                match &inst.operation {
                    Operation::Literal(literal) => locals.literal(&mut code, literal, inst.result),
                    Operation::Arithmetic { operator, arguments, coercing, primitive_overload } => locals.arithmetic(
                        &mut code, *operator, arguments, inst.result, *primitive_overload,
                        index(arithmetic_name(*operator, arguments, *coercing, *primitive_overload, ir)),
                    ),
                    Operation::Comparison { operation, arguments } => locals.comparison(
                        &mut code, *operation, arguments, inst.result, index(operation.export()),
                    ),
                    Operation::Bitwise { operation, arguments } => locals.bitwise(
                        &mut code, *operation, arguments, inst.result, index(operation.export()),
                        unboxed::raw_helper(&locals, inst).map(|(export, _)| index(export)),
                    ),
                    _ => unreachable!("inline numeric instruction"),
                }
                for instruction in code.drain(..) { function.instruction(&instruction); }
                continue;
            }
            if unboxed::raw_small(&locals, inst) {
                let Operation::Nominal { operation, arguments } = &inst.operation else { unreachable!("small") };
                if *operation == Nominal::NumberHash {
                    locals.hash(&mut code, arguments[0], inst.result, field_index_local);
                } else {
                    locals.small(&mut code, *operation == Nominal::SameReference, arguments, inst.result);
                }
                for instruction in code.drain(..) { function.instruction(&instruction); }
                continue;
            }
            if unboxed::raw_identical(&locals, inst) {
                let Operation::Nominal { arguments, .. } = &inst.operation else { unreachable!("identical") };
                locals.identical(&mut code, arguments, inst.result);
                for instruction in code.drain(..) { function.instruction(&instruction); }
                continue;
            }
            if let Operation::Nominal { operation: Nominal::ChunkIndexAdd, arguments } = &inst.operation
                && locals.of(inst.result) == unboxed::Repr::Long
            {
                code.extend([LocalGet(locals.raw(arguments[0])), LocalGet(locals.raw(arguments[1])),
                    I64Add, LocalSet(locals.raw(inst.result))]);
                for instruction in code.drain(..) { function.instruction(&instruction); }
                continue;
            }
            // A coercion of a raw value to a raw result (#448): a long stays
            // a long and widens to a double, and unchecked-long truncates a
            // double as Java's cast does. A checked long of a double first
            // checks its range, keeping only the error path boxed.
            if let Operation::Nominal { operation: operation @ (Nominal::LongCast | Nominal::UncheckedLongCast | Nominal::DoubleCast), arguments } = &inst.operation
                && locals.of(inst.result) != unboxed::Repr::Boxed
            {
                let from = locals.of(arguments[0]);
                if from.maybe() {
                    primitive::argument(&mut code, &locals, arguments[0],
                        if *operation == Nominal::DoubleCast { super::primitive::Kind::Double } else { super::primitive::Kind::Long },
                        *operation == Nominal::LongCast, method_local, &index);
                    code.push(LocalSet(locals.raw(inst.result)));
                    for instruction in code.drain(..) { function.instruction(&instruction); }
                    continue;
                }
                if *operation == Nominal::LongCast && from == unboxed::Repr::Double {
                    // RT.longCast(double) checks the original double before
                    // truncating. NaN converts to zero; the rounded upper
                    // boundary, 2^63, saturates to Long/MAX_VALUE. Box only
                    // the out-of-range operand, for the runtime diagnostic.
                    let argument = arguments[0];
                    function
                        .instruction(&LocalGet(locals.raw(argument)))
                        .instruction(&F64Const((-9_223_372_036_854_775_808.0).into()))
                        .instruction(&F64Lt)
                        .instruction(&LocalGet(locals.raw(argument)))
                        .instruction(&F64Const(9_223_372_036_854_775_808.0.into()))
                        .instruction(&F64Gt)
                        .instruction(&I32Or)
                        .instruction(&If(BlockType::Empty));
                    locals.push_boxed(&mut code, argument);
                    for instruction in code.drain(..) { function.instruction(&instruction); }
                    function
                        .instruction(&Call(index("number-cast-long")))
                        .instruction(&Drop)
                        .instruction(&Unreachable)
                        .instruction(&End)
                        .instruction(&LocalGet(locals.raw(argument)))
                        .instruction(&I64TruncSatF64S)
                        .instruction(&LocalSet(locals.raw(inst.result)));
                    continue;
                }
                let convert = match (operation, from) {
                    (Nominal::LongCast | Nominal::UncheckedLongCast, unboxed::Repr::Long) | (Nominal::DoubleCast, unboxed::Repr::Double) => Some(None),
                    (Nominal::DoubleCast, unboxed::Repr::Long) => Some(Some(F64ConvertI64S)),
                    (Nominal::UncheckedLongCast, unboxed::Repr::Double) => Some(Some(I64TruncSatF64S)),
                    _ => None,
                };
                if let Some(convert) = convert {
                    function.instruction(&LocalGet(locals.raw(arguments[0])));
                    if let Some(convert) = convert {
                        function.instruction(&convert);
                    }
                    function.instruction(&LocalSet(locals.raw(inst.result)));
                    continue;
                }
            }
            // Physical raw numbers/flags cannot be undefined. Guarded numeric
            // calls retain their boxed replacement slot when the raw flag misses.
            if let Operation::Nominal { operation: Nominal::IsUndefined, arguments } = &inst.operation
                && locals.of(arguments[0]) != unboxed::Repr::Boxed
            {
                let argument = arguments[0];
                if locals.of(argument).maybe() {
                    code.extend([LocalGet(locals.valid(argument)), If(BlockType::Result(ValType::I32)),
                        I32Const(0), Else, LocalGet(argument.0 as u32 + offset),
                        runtime_abi::sentinel_in(runtime_abi::UNDEFINED, sentinels), RefEq, End]);
                } else {
                    code.push(I32Const(0));
                }
                if locals.of(inst.result) == unboxed::Repr::Flag {
                    code.push(LocalSet(locals.raw(inst.result)));
                } else {
                    code.extend(runtime_abi::boolean_in(sentinels));
                    code.push(LocalSet(inst.result.0 as u32 + offset));
                }
                for instruction in code.drain(..) { function.instruction(&instruction); }
                continue;
            }
            // A packed access boxes its raw operands itself, only where the
            // generic path needs them (#387).
            let capture_definition = match &inst.operation {
                Operation::MakeClosure { body, .. } => Some(layout.single(body)),
                Operation::MakeGeneralClosure { body, .. } => Some(layout.general(body)),
                _ => None,
            };
            if let Some(definition) = capture_definition {
                let operands = ir::operands(&inst.operation);
                let boxed_captures = operands.into_iter().zip(&definition.capture_reprs)
                    .filter_map(|(value, repr)| (*repr == unboxed::Repr::Boxed).then_some(*value)).collect::<Vec<_>>();
                locals.materialize(&mut code, &boxed_captures, &mut boxed);
            } else if !matches!(inst.operation, Operation::Nominal { operation: Nominal::PackedGet(_) | Nominal::PackedSet(_), .. }) {
                locals.materialize(&mut code, ir::operands(&inst.operation), &mut boxed);
            }
            for instruction in code.drain(..) { function.instruction(&instruction); }
            match &inst.operation {
                Operation::RegionPush { handler, cleanup, end, dynamic } => {
                    let plan = continuation.ok_or_else(|| fail("Resumable region outside continuation"))?;
                    r#async::region_push(&mut function, *handler, *cleanup, *end, dynamic, inst.result, plan, &slots);
                }
                Operation::RegionExit { value, cleanup } => {
                    let plan = continuation.ok_or_else(|| fail("Resumable exit outside continuation"))?;
                    r#async::region_exit(&mut function, *value, *cleanup, &slots, plan.states.len());
                    function.instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, sentinels)).instruction(&LocalSet(inst.result.0 as u32 + offset)).instruction(&Br(1));
                }
                Operation::RegionValue => {
                    if continuation.is_none() { return Err(fail("Resumable outcome outside continuation")); }
                    r#async::item(&mut function, slots.snapshot, 7);
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::AsyncDynamicEnter { globals: targets, operands } => {
                    if continuation.is_none() { return Err(fail("Async dynamic entry outside continuation")); }
                    for (i, target) in targets.iter().enumerate() {
                        function.instruction(&GlobalGet(globals.binary_search(target).expect("collected dynamic cell") as u32))
                            .instruction(&LocalGet(operands[i].0 as u32 + offset))
                            .instruction(&LocalGet(operands[targets.len() + i].0 as u32 + offset));
                    }
                    function.instruction(&ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: (targets.len() * 3) as u32 })
                        .instruction(&Call(index("dynamic-push"))).instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::MakeFuture { body, captures } => {
                    let body_index = futures.iter().position(|candidate| std::ptr::eq(*candidate, body.as_ref())).expect("collected future");
                    let function_index = u32::try_from(names.len() + 1 + bodies.len() + dispatchers.len() + body_index).map_err(|_| fail("Too many functions"))?;
                    r#async::create(&mut function, body, captures, inst.result, function_index, &slots, &index);
                }
                Operation::Await { .. } => return Err(fail("Unnormalized await reached emission")),
                Operation::DynamicScope {
                    globals: targets,
                    operands,
                } => {
                    for (i, target) in targets.iter().enumerate() {
                        let global = globals
                            .iter()
                            .position(|global| global == target)
                            .expect("collected dynamic cell");
                        function
                            .instruction(&GlobalGet(global as u32))
                            .instruction(&LocalGet(operands[i].0 as u32 + offset))
                            .instruction(&LocalGet(operands[targets.len() + i].0 as u32 + offset));
                    }
                    function
                        .instruction(&ArrayNewFixed {
                            array_type_index: runtime_abi::ARGS,
                            array_size: targets.len() as u32 * 3,
                        })
                        .instruction(&LocalGet(operands.last().unwrap().0 as u32 + offset))
                        .instruction(&Call(index("dynamic-invoke")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Try { regions } => {
                    for region in regions {
                        function.instruction(&LocalGet(region.0 as u32 + offset));
                    }
                    function
                        .instruction(&Call(index("try-invoke")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(literal @ (Literal::Nil | Literal::Undefined)) => {
                    function
                        .instruction(&runtime_abi::sentinel_in(
                            if matches!(literal, Literal::Undefined) {
                                runtime_abi::UNDEFINED
                            } else {
                                runtime_abi::NIL
                            },
                            sentinels,
                        ))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(Literal::Bool(value)) => {
                    function
                        .instruction(&runtime_abi::sentinel_in(
                            if *value { runtime_abi::TRUE } else { runtime_abi::FALSE },
                            sentinels,
                        ))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(Literal::Number(value)) => {
                    function
                        .instruction(&F64Const((*value).into()))
                        .instruction(&Call(index("number-box")))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(Literal::Long(value)) => {
                    // A long in i31's range is that i31; others are boxed.
                    match i32::try_from(*value).ok().filter(|v| (-(1 << 30)..1 << 30).contains(v)) {
                        Some(small) => function.instruction(&I32Const(small)).instruction(&RefI31),
                        None => function
                            .instruction(&I64Const(*value))
                            .instruction(&StructNew(runtime_abi::LONG)),
                    };
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(Literal::BigInt { negative, limbs, big_integer }) => {
                    function.instruction(&I32Const(if limbs.is_empty() { 0 } else if *negative { -1 } else { 1 }));
                    for limb in limbs { function.instruction(&I32Const(*limb as i32)); }
                    function.instruction(&ArrayNewFixed { array_type_index: runtime_abi::PACKED_STORAGE + 2,
                        array_size: u32::try_from(limbs.len()).map_err(|_| Diagnostic { span: ir.span.clone(), message: "BigInt literal exceeds Wasm array length".into() })? });
                    function.instruction(&I32Const(limbs.len() as i32)).instruction(&Call(index("bigint-new")));
                    if *big_integer { function.instruction(&Call(index("integer-biginteger"))); }
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(Literal::Ratio { negative, numerator, denominator }) => {
                    for (negative, limbs) in [(*negative, numerator), (false, denominator)] {
                        function.instruction(&I32Const(if negative {-1} else {1}));
                        for limb in limbs { function.instruction(&I32Const(*limb as i32)); }
                        function.instruction(&ArrayNewFixed { array_type_index: runtime_abi::PACKED_STORAGE + 2,
                            array_size: u32::try_from(limbs.len()).map_err(|_| Diagnostic { span: ir.span.clone(), message: "Ratio literal exceeds Wasm array length".into() })? });
                        function.instruction(&I32Const(limbs.len() as i32)).instruction(&Call(index("bigint-new")));
                    }
                    function.instruction(&Call(index("ratio-literal"))).instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(Literal::Decimal { negative, unscaled, scale }) => {
                    function.instruction(&I32Const(if unscaled.is_empty() { 0 } else if *negative { -1 } else { 1 }));
                    for limb in unscaled { function.instruction(&I32Const(*limb as i32)); }
                    function.instruction(&ArrayNewFixed { array_type_index: runtime_abi::PACKED_STORAGE + 2,
                        array_size: u32::try_from(unscaled.len()).map_err(|_| Diagnostic { span: ir.span.clone(), message: "Decimal literal exceeds Wasm array length".into() })? });
                    function.instruction(&I32Const(unscaled.len() as i32)).instruction(&Call(index("bigint-new")));
                    function.instruction(&I32Const(*scale)).instruction(&Call(index("decimal-literal")));
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Literal(Literal::String(units)) => {
                    emit_utf16_literal(&mut function, data, literals, units, inst.result.0 as u32 + offset, &ir.span)?;
                }
                Operation::GlobalCell(global) => {
                    let index_global = globals
                        .binary_search(global)
                        .expect("collected cell identity")
                        as u32;
                    function
                        .instruction(&GlobalGet(index_global))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                // A keyword literal's hidden cell (#361) is compiler-owned, never
                // dynamic, and read only right after the site's once-definition
                // of it, so it is bound: read its root directly.
                Operation::GlobalRead(global) if global.namespace() == "suss.internal.keywords" => {
                    let index_global =
                        globals.binary_search(global).expect("collected global") as u32;
                    function
                        .instruction(&GlobalGet(index_global))
                        .instruction(&StructGet { struct_type_index: runtime_abi::BINDING, field_index: 0 })
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::GlobalRead(global) => {
                    let index_global =
                        globals.binary_search(global).expect("collected global") as u32;
                    let flags = [
                        GlobalGet(index_global),
                        StructGet {
                            struct_type_index: runtime_abi::BINDING,
                            field_index: 1,
                        },
                    ];
                    // A bound cell that no binding frame has held reads its
                    // root inline (#354). Otherwise an unbound cell reads as
                    // undefined, and a dynamic one asks binding-get.
                    for instruction in &flags {
                        function.instruction(instruction);
                    }
                    function
                        .instruction(&I32Const(runtime_abi::CELL_BOUND))
                        .instruction(&I32Eq)
                        .instruction(&If(BlockType::Result(VALUE)))
                        .instruction(&GlobalGet(index_global))
                        .instruction(&StructGet {
                            struct_type_index: runtime_abi::BINDING,
                            field_index: 0,
                        })
                        .instruction(&Else);
                    for instruction in &flags {
                        function.instruction(instruction);
                    }
                    function
                        .instruction(&I32Const(runtime_abi::CELL_BOUND))
                        .instruction(&I32And)
                        .instruction(&If(BlockType::Result(VALUE)))
                        .instruction(&GlobalGet(index_global))
                        .instruction(&Call(index("binding-get")))
                        .instruction(&Else)
                        .instruction(&runtime_abi::sentinel_in(runtime_abi::UNDEFINED, sentinels))
                        .instruction(&End)
                        .instruction(&End)
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::GlobalBound(global) => {
                    let index_global =
                        globals.binary_search(global).expect("collected global") as u32;
                    // The cell's bound flag, inline (#361).
                    function
                        .instruction(&GlobalGet(index_global))
                        .instruction(&StructGet { struct_type_index: runtime_abi::BINDING, field_index: 1 })
                        .instruction(&I32Const(runtime_abi::CELL_BOUND))
                        .instruction(&I32And)
                        .instruction(&If(BlockType::Result(VALUE)))
                        .instruction(&runtime_abi::sentinel_in(runtime_abi::TRUE, sentinels))
                        .instruction(&Else)
                        .instruction(&runtime_abi::sentinel_in(runtime_abi::FALSE, sentinels))
                        .instruction(&End)
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::GlobalWrite { global, value } => {
                    let index_global =
                        globals.binary_search(global).expect("collected global") as u32;
                    function
                        .instruction(&GlobalGet(index_global))
                        .instruction(&LocalGet(value.0 as u32 + offset))
                        .instruction(&Call(index("binding-set")))
                        .instruction(&LocalGet(value.0 as u32 + offset))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Call { operands, .. } => {
                    let tail = continuation.is_none() && !matches!(entry, Entry::Primitive(_)) && block.instructions.last().is_some_and(|last| std::ptr::eq(last, inst))
                        && returns_unchanged(ir, &block.terminator, inst.result);
                    let arguments = &operands[1..];
                    let callee_local = operands[0].0 as u32 + offset;
                    let direct = known.get(&(operands[0].0 as u32)).filter(|_| arguments.len() <= runtime_abi::MAX_FIXED as usize);
                    let link = linked.get(&(operands[0].0 as u32)).filter(|_| arguments.len() <= runtime_abi::MAX_FIXED as usize);
                    if let Some(link) = link {
                        // The var's slot holds its latest linked definition's
                        // closure, a capture-free closure (final type, so the
                        // cast is inline). While that is still the definition
                        // this call was compiled against, it calls the entry
                        // statically; after a redefinition, through the new
                        // closure's code struct, as Clojure's direct-linked
                        // calls reach the newest compiled function.
                        // Any other value, as `set!` can leave, takes the
                        // out-of-line path.
                        let definition = super::hir::link_parts(*link).1;
                        let k = arguments.len() as u32;
                        function
                            .instruction(&Block(BlockType::Result(VALUE)))
                            .instruction(&Block(BlockType::Result(VALUE)))
                            .instruction(&LocalGet(callee_local))
                            .instruction(&BrOnCastFail {
                                relative_depth: 0,
                                from_ref_type: RefType::EQREF,
                                to_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(links.link_type) },
                            })
                            .instruction(&LocalTee(callee))
                            .instruction(&StructGet { struct_type_index: runtime_abi::CLOSURE, field_index: runtime_abi::CLOSURE_CODE })
                            .instruction(&GlobalGet(links.codes[&definition]))
                            .instruction(&RefEq)
                            .instruction(&If(BlockType::Result(VALUE)))
                            .instruction(&LocalGet(callee))
                            .instruction(&RefAsNonNull);
                        for value in arguments {
                            function.instruction(&LocalGet(value.0 as u32 + offset));
                        }
                        let entry = links.entries[&definition][arguments.len()];
                        function
                            .instruction(&if tail { ReturnCall(entry) } else { Call(entry) })
                            .instruction(&Else)
                            .instruction(&LocalGet(callee))
                            .instruction(&RefAsNonNull);
                        for value in arguments {
                            function.instruction(&LocalGet(value.0 as u32 + offset));
                        }
                        function
                            .instruction(&LocalGet(callee))
                            .instruction(&StructGet { struct_type_index: runtime_abi::CLOSURE, field_index: runtime_abi::CLOSURE_CODE })
                            .instruction(&StructGet { struct_type_index: runtime_abi::CODE, field_index: runtime_abi::CODE_ENTRY + k })
                            .instruction(&if tail { ReturnCallRef(runtime_abi::CALL + k) } else { CallRef(runtime_abi::CALL + k) })
                            .instruction(&End)
                            .instruction(&Br(1))
                            .instruction(&End);
                        for value in arguments {
                            function.instruction(&LocalGet(value.0 as u32 + offset));
                        }
                        let call = index(runtime_abi::CALL_EXPORTS[k as usize]);
                        function
                            .instruction(&if tail { ReturnCall(call) } else { Call(call) })
                            .instruction(&End)
                            .instruction(&LocalSet(inst.result.0 as u32 + offset));
                    } else if let Some(&(definition, own)) = direct {
                        let definition = &layout.definitions[definition];
                        if own {
                            function.instruction(&LocalGet(0));
                        } else {
                            function
                                .instruction(&LocalGet(callee_local))
                                .instruction(&RefCastNonNull(HeapType::Concrete(definition.structure)));
                        }
                        for value in arguments {
                            function.instruction(&LocalGet(value.0 as u32 + offset));
                        }
                        let entry = definition.entries[arguments.len()];
                        if tail {
                            function.instruction(&ReturnCall(entry));
                        } else {
                            function.instruction(&Call(entry)).instruction(&LocalSet(inst.result.0 as u32 + offset));
                        }
                    } else if arguments.len() <= runtime_abi::MAX_FIXED as usize {
                        // ADR-0004: a closure calls its code struct's entry for
                        // this many arguments, which the site caches with
                        // its last callee (#378). Any other callee caches its
                        // entry from `call-entry-k`: a closure's, or the
                        // fallback that calls the runtime's `call-k`.
                        let k = arguments.len() as u32;
                        let site = call_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                        function
                            .instruction(&GlobalGet(site))
                            .instruction(&LocalGet(callee_local))
                            .instruction(&RefEq)
                            .instruction(&I32Eqz)
                            .instruction(&If(BlockType::Empty))
                            .instruction(&LocalGet(callee_local))
                            .instruction(&Call(index(runtime_abi::CALL_ENTRY_EXPORTS[k as usize])))
                            .instruction(&GlobalSet(site + 1))
                            .instruction(&LocalGet(callee_local))
                            .instruction(&GlobalSet(site))
                            .instruction(&End)
                            .instruction(&LocalGet(callee_local));
                        for value in arguments {
                            function.instruction(&LocalGet(value.0 as u32 + offset));
                        }
                        function
                            .instruction(&GlobalGet(site + 1))
                            .instruction(&if tail { ReturnCallRef(runtime_abi::CALL + k) } else { CallRef(runtime_abi::CALL + k) })
                            .instruction(&LocalSet(inst.result.0 as u32 + offset));
                    } else {
                        function.instruction(&LocalGet(callee_local));
                        for value in arguments {
                            function.instruction(&LocalGet(value.0 as u32 + offset));
                        }
                        let size = u32::try_from(arguments.len())
                            .map_err(|_| fail("Too many call arguments"))?;
                        function.instruction(&ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: size });
                        if tail {
                            function.instruction(&ReturnCall(index("call-spread")));
                        } else {
                            function.instruction(&Call(index("call-spread")))
                                .instruction(&LocalSet(inst.result.0 as u32 + offset));
                        }
                    }
                }
                Operation::MakeClosure { body, captures } => {
                    let definition = layout.single(body);
                    emit_closure(&mut function, definition, captures.iter().zip(&definition.capture_reprs).map(|(value, repr)| if *repr == unboxed::Repr::Boxed { value.0 as u32 + offset } else { locals.raw(*value) }));
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                    if let Some(name) = &body.display_name {
                        emit_utf16_literal(&mut function, data, literals, name, scratch, &ir.span)?;
                        function.instruction(&LocalGet(inst.result.0 as u32 + offset))
                            .instruction(&LocalGet(scratch))
                            .instruction(&Call(index("closure-source-name-initialize")));
                    }
                }
                Operation::MakeGeneralClosure { body, captures } => {
                    let definition = layout.general(body);
                    emit_closure(&mut function, definition, captures.iter().zip(&definition.capture_reprs).map(|(value, repr)| if *repr == unboxed::Repr::Boxed { value.0 as u32 + offset } else { locals.raw(*value) }));
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                    if let Some(name) = &body.display_name {
                        emit_utf16_literal(&mut function, data, literals, name, scratch, &ir.span)?;
                        function.instruction(&LocalGet(inst.result.0 as u32 + offset))
                            .instruction(&LocalGet(scratch))
                            .instruction(&Call(index("closure-source-name-initialize")));
                    }
                }
                Operation::NilTest(value) => {
                    function
                        .instruction(&LocalGet(value.0 as u32 + offset))
                        .instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, sentinels))
                        .instruction(&RefEq)
                        .instruction(&LocalGet(value.0 as u32 + offset))
                        .instruction(&runtime_abi::sentinel_in(runtime_abi::UNDEFINED, sentinels))
                        .instruction(&RefEq)
                        .instruction(&I32Or);
                    if locals.of(inst.result) == unboxed::Repr::Flag {
                        function.instruction(&LocalSet(locals.raw(inst.result)));
                    } else {
                        for instruction in runtime_abi::boolean_in(sentinels) {
                            function.instruction(&instruction);
                        }
                        function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                    }
                }
                Operation::Bitwise {
                    operation,
                    arguments,
                } => {
                    // Two i31 operands (#361): and, or and xor of sign-extended
                    // 31-bit values, and an arithmetic right shift by 0..31,
                    // are i31 values again, for longs and JS int32 alike. An
                    // unsigned shift of a nonnegative value is the same shift.
                    use super::hir::Bitwise;
                    let inline = match operation {
                        Bitwise::And | Bitwise::Int32And => Some(I32And),
                        Bitwise::Or | Bitwise::Int32Or => Some(I32Or),
                        Bitwise::Xor | Bitwise::Int32Xor => Some(I32Xor),
                        Bitwise::Right | Bitwise::Int32Right => Some(I32ShrS),
                        Bitwise::Unsigned | Bitwise::Int32Unsigned => Some(I32ShrU),
                        _ => None,
                    }
                    .filter(|_| arguments.len() == 2);
                    if let Some(op) = inline {
                        let [a, b] = [arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset];
                        i31_operands(&mut function, a, b);
                        if matches!(op, I32ShrS | I32ShrU) {
                            // Only once both are i31: a shift by 0..31, and for
                            // an unsigned shift, a nonnegative value. (`i32.and`
                            // evaluates both sides, so these casts are guarded.)
                            let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
                            function
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&LocalGet(b))
                                .instruction(&RefCastNonNull(i31))
                                .instruction(&I31GetU)
                                .instruction(&I32Const(32))
                                .instruction(&I32LtU);
                            if matches!(op, I32ShrU) {
                                function
                                    .instruction(&LocalGet(a))
                                    .instruction(&RefCastNonNull(i31))
                                    .instruction(&I31GetS)
                                    .instruction(&I32Const(0))
                                    .instruction(&I32GeS)
                                    .instruction(&I32And);
                            }
                            function.instruction(&Else).instruction(&I32Const(0)).instruction(&End);
                        }
                        function.instruction(&If(BlockType::Result(VALUE)));
                        i31_values(&mut function, a, b);
                        function
                            .instruction(&op)
                            .instruction(&RefI31)
                            .instruction(&Else)
                            .instruction(&LocalGet(a))
                            .instruction(&LocalGet(b))
                            .instruction(&Call(index(operation.export())))
                            .instruction(&End)
                            .instruction(&LocalSet(inst.result.0 as u32 + offset));
                    } else {
                        for argument in arguments {
                            function.instruction(&LocalGet(argument.0 as u32 + offset));
                        }
                        function
                            .instruction(&Call(index(operation.export())))
                            .instruction(&LocalSet(inst.result.0 as u32 + offset));
                    }
                    if matches!(locals.of(inst.result), unboxed::Repr::Long | unboxed::Repr::Double | unboxed::Repr::Flag) {
                        // A checked numeric helper has completed the original
                        // operand conversions and returned a long. Reuse its
                        // GC value while keeping subsequent numeric uses raw.
                        locals.unbox_result(&mut code, inst.result);
                        for instruction in code.drain(..) { function.instruction(&instruction); }
                        boxed.insert(inst.result.0);
                    }
                }
                Operation::Comparison {
                    operation,
                    arguments,
                } => {
                    // Two i31 operands compare as i32 (#361).
                    use super::hir::Comparison;
                    let inline = match operation {
                        Comparison::Less | Comparison::NumberLess => Some(I32LtS),
                        Comparison::LessEqual | Comparison::NumberLessEqual => Some(I32LeS),
                        Comparison::Greater | Comparison::NumberGreater => Some(I32GtS),
                        Comparison::GreaterEqual | Comparison::NumberGreaterEqual => Some(I32GeS),
                        _ => None,
                    }
                    .filter(|_| arguments.len() == 2);
                    if let Some(op) = inline {
                        let [a, b] = [arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset];
                        i31_operands(&mut function, a, b);
                        function.instruction(&If(BlockType::Result(VALUE)));
                        i31_values(&mut function, a, b);
                        function
                            .instruction(&op)
                            .instruction(&If(BlockType::Result(VALUE)))
                            .instruction(&runtime_abi::sentinel_in(runtime_abi::TRUE, sentinels))
                            .instruction(&Else)
                            .instruction(&runtime_abi::sentinel_in(runtime_abi::FALSE, sentinels))
                            .instruction(&End)
                            .instruction(&Else);
                        // In a Clojure numeric comparison (not core's
                        // JS-convention one), operands that are each an i31 or
                        // a boxed long (#385) compare as i64s; anything else
                        // asks the runtime.
                        let clojure = matches!(operation, Comparison::NumberLess | Comparison::NumberLessEqual
                            | Comparison::NumberGreater | Comparison::NumberGreaterEqual);
                        let wide = match op {
                            I32LtS => I64LtS,
                            I32LeS => I64LeS,
                            I32GtS => I64GtS,
                            _ => I64GeS,
                        };
                        let [x, y, _] = long_locals;
                        function.instruction(&Block(BlockType::Result(VALUE))).instruction(&Block(BlockType::Empty));
                        if clojure {
                        long_operand(&mut function, a, x);
                        long_operand(&mut function, b, y);
                        function
                            .instruction(&LocalGet(x))
                            .instruction(&LocalGet(y))
                            .instruction(&wide)
                            .instruction(&If(BlockType::Result(VALUE)))
                            .instruction(&runtime_abi::sentinel_in(runtime_abi::TRUE, sentinels))
                            .instruction(&Else)
                            .instruction(&runtime_abi::sentinel_in(runtime_abi::FALSE, sentinels))
                            .instruction(&End)
                            .instruction(&Br(1));
                        }
                        function
                            .instruction(&End)
                            .instruction(&LocalGet(a))
                            .instruction(&LocalGet(b))
                            .instruction(&Call(index(operation.export())))
                            .instruction(&End)
                            .instruction(&End)
                            .instruction(&LocalSet(inst.result.0 as u32 + offset));
                    } else {
                        for argument in arguments {
                            function.instruction(&LocalGet(argument.0 as u32 + offset));
                        }
                        function
                            .instruction(&Call(index(operation.export())))
                            .instruction(&LocalSet(inst.result.0 as u32 + offset));
                    }
                }
                Operation::Array {
                    operation: ArrayOperation::Get,
                    arguments,
                } if arguments.len() == 2 => {
                    // A packed array reads its storage (#389); anything
                    // else, the source array's dense prefix (#361). An
                    // object array at an i31 index in range reads inline, as
                    // a hinted one does (#446).
                    let array = arguments[0].0 as u32 + offset;
                    let storage = storage_locals + 6;
                    for instruction in object_array_element(array, arguments[1].0 as u32 + offset, storage, field_index_local) {
                        function.instruction(&instruction);
                    }
                    function
                        .instruction(&LocalGet(storage))
                        .instruction(&LocalGet(field_index_local))
                        .instruction(&ArrayGet(runtime_abi::ARGS))
                        .instruction(&Br(1))
                        .instruction(&End)
                        .instruction(&LocalGet(array))
                        .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::PACKED)))
                        .instruction(&If(BlockType::Result(VALUE)))
                        .instruction(&LocalGet(array))
                        .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                        .instruction(&Call(index("packed-array-get-2")))
                        .instruction(&Else)
                        .instruction(&LocalGet(array))
                        .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                        .instruction(&Call(index("array-get")))
                        .instruction(&End)
                        .instruction(&End)
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Array {
                    operation: ArrayOperation::Set,
                    arguments,
                } if arguments.len() == 3 => {
                    // As for one-index reads (#389, #446).
                    let storage = storage_locals + 6;
                    let value = arguments[2].0 as u32 + offset;
                    for instruction in object_array_element(arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset, storage, field_index_local) {
                        function.instruction(&instruction);
                    }
                    function
                        .instruction(&LocalGet(storage))
                        .instruction(&LocalGet(field_index_local))
                        .instruction(&LocalGet(value))
                        .instruction(&ArraySet(runtime_abi::ARGS))
                        .instruction(&LocalGet(value))
                        .instruction(&Br(1))
                        .instruction(&End)
                        .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                        .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::PACKED)))
                        .instruction(&If(BlockType::Result(VALUE)));
                    for argument in arguments {
                        function.instruction(&LocalGet(argument.0 as u32 + offset));
                    }
                    function.instruction(&Call(index("packed-array-set-3"))).instruction(&Else);
                    for argument in arguments {
                        function.instruction(&LocalGet(argument.0 as u32 + offset));
                    }
                    function
                        .instruction(&Call(index("array-set")))
                        .instruction(&End)
                        .instruction(&End)
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Array {
                    operation: ArrayOperation::Length,
                    arguments,
                } if arguments.len() == 1 => {
                    // alength of a packed array reads its storage's length
                    // inline (#442), as an i31 long when it fits; anything
                    // else asks the runtime, which boxes or reports it.
                    let array = arguments[0].0 as u32 + offset;
                    function
                        .instruction(&Block(BlockType::Result(VALUE)))
                        .instruction(&Block(BlockType::Empty))
                        .instruction(&LocalGet(array))
                        .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::PACKED)))
                        .instruction(&I32Eqz)
                        .instruction(&BrIf(0))
                        .instruction(&LocalGet(array))
                        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::PACKED)))
                        .instruction(&StructGet { struct_type_index: runtime_abi::PACKED, field_index: 0 })
                        .instruction(&RefCastNonNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::Array }))
                        .instruction(&ArrayLen)
                        .instruction(&LocalTee(field_index_local))
                        .instruction(&I32Const(1 << 30))
                        .instruction(&I32GeU)
                        .instruction(&BrIf(0))
                        .instruction(&LocalGet(field_index_local))
                        .instruction(&RefI31)
                        .instruction(&Br(1))
                        .instruction(&End)
                        .instruction(&LocalGet(array))
                        .instruction(&ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: 1 })
                        .instruction(&Call(index(ArrayOperation::Length.export())))
                        .instruction(&End)
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Array {
                    operation: ArrayOperation::Clone,
                    arguments,
                } => {
                    function
                        .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                        .instruction(&Call(index(ArrayOperation::Clone.export())))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Array {
                    operation,
                    arguments,
                } => {
                    for argument in arguments {
                        function.instruction(&LocalGet(argument.0 as u32 + offset));
                    }
                    function
                        .instruction(&ArrayNewFixed {
                            array_type_index: runtime_abi::ARGS,
                            array_size: arguments.len() as u32,
                        })
                        .instruction(&Call(index(operation.export())))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Nominal {
                    operation,
                    arguments,
                } => {
                    match operation {
                        Nominal::MapEntryType => {
                            function.instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&Call(index("map-entry-type-register")));
                        }
                        Nominal::ScalarVectorGuard | Nominal::ScalarMapEntryGuard => {
                            // Reuse the construction-site cache allocation; a
                            // scalar guard needs only its first slot. Store only
                            // successful admissions. The helper rechecks current
                            // canonical registration even when this key matches.
                            let site = construct_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            let ctor = arguments[0].0 as u32 + offset;
                            function.instruction(&LocalGet(ctor)).instruction(&GlobalGet(site))
                                .instruction(&Call(index(if *operation == Nominal::ScalarVectorGuard { "scalar-vector-guard" } else { "scalar-map-entry-guard" })))
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&LocalGet(ctor)).instruction(&GlobalSet(site))
                                .instruction(&I32Const(1)).instruction(&Else)
                                .instruction(&I32Const(0)).instruction(&End);
                        }
                        Nominal::IsUndefined => {
                            function.instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&runtime_abi::sentinel_in(runtime_abi::UNDEFINED, sentinels))
                                .instruction(&RefEq);
                        }
                        Nominal::IsBoolean => {
                            for sentinel in [runtime_abi::FALSE, runtime_abi::TRUE] {
                                function.instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                    .instruction(&runtime_abi::sentinel_in(sentinel, sentinels))
                                    .instruction(&RefEq);
                            }
                            function.instruction(&I32Or);
                        }
                        // The same reference is identical, unless a boxed double,
                        // which compares by value (NaN isn't identical to
                        // itself). Otherwise nil, or a long in i31 range,
                        // which has no other representation (ABI v3), isn't;
                        // only two other values need the runtime's test of
                        // boxed numbers and strings.
                        Nominal::Identical => {
                            let [a, b] = [arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset];
                            // Tests that usually succeed come first: a failed
                            // cast to a concrete type is wasmtime's slow
                            // subtype check (#442). A long in i31 range has no
                            // other representation, so with one, identity is
                            // the reference's (#389). The same reference is
                            // identical unless a boxed double, which compares
                            // by value (NaN isn't identical to itself).
                            // Distinct nominal objects, keywords among them,
                            // aren't; only boxed longs, doubles and strings
                            // compare by value (#361).
                            let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
                            let by_value = |function: &mut Function| {
                                function
                                    .instruction(&LocalGet(a))
                                    .instruction(&LocalGet(b))
                                    .instruction(&Call(index("identical-value")))
                                    .instruction(&runtime_abi::sentinel_in(runtime_abi::TRUE, sentinels))
                                    .instruction(&RefEq);
                            };
                            function
                                .instruction(&LocalGet(a))
                                .instruction(&RefTestNonNull(i31))
                                .instruction(&LocalGet(b))
                                .instruction(&RefTestNonNull(i31))
                                .instruction(&I32Or)
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&LocalGet(a))
                                .instruction(&LocalGet(b))
                                .instruction(&RefEq)
                                .instruction(&Else)
                                .instruction(&LocalGet(a))
                                .instruction(&LocalGet(b))
                                .instruction(&RefEq)
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&LocalGet(a))
                                .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::NUMBER)))
                                .instruction(&If(BlockType::Result(ValType::I32)));
                            by_value(&mut function);
                            function
                                .instruction(&Else)
                                .instruction(&I32Const(1))
                                .instruction(&End)
                                .instruction(&Else)
                                .instruction(&LocalGet(a))
                                .instruction(&RefIsNull)
                                .instruction(&LocalGet(b))
                                .instruction(&RefIsNull)
                                .instruction(&I32Or)
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&I32Const(0))
                                .instruction(&Else)
                                .instruction(&LocalGet(a))
                                .instruction(&RefTestNonNull(HeapType::Concrete(7)))
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&I32Const(0))
                                .instruction(&Else);
                            for (index, ty) in [runtime_abi::NUMBER, runtime_abi::LONG, runtime_abi::STRING].into_iter().enumerate() {
                                function.instruction(&LocalGet(a)).instruction(&RefTestNonNull(HeapType::Concrete(ty)));
                                if index > 0 {
                                    function.instruction(&I32Or);
                                }
                            }
                            function.instruction(&If(BlockType::Result(ValType::I32)));
                            by_value(&mut function);
                            function
                                .instruction(&Else)
                                .instruction(&I32Const(0))
                                .instruction(&End)
                                .instruction(&End)
                                .instruction(&End)
                                .instruction(&End)
                                .instruction(&End);
                        }
                        // The runtime predicates' own tests, inline: never a mutable
                        // public cell, and no closure call on core's hot paths (#286).
                        // Arguments are already evaluated once in source order.
                        Nominal::IsBigInteger => {
                            let value = arguments[0].0 as u32 + offset;
                            function.instruction(&LocalGet(value)).instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::BIGINT)))
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&LocalGet(value)).instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::BIGINT)))
                                .instruction(&StructGet { struct_type_index: runtime_abi::BIGINT, field_index: 3 })
                                .instruction(&Else).instruction(&I32Const(0)).instruction(&End);
                        }
                        Nominal::IsRatio => {
                            function.instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::RATIO)));
                        }
                        Nominal::DecimalCoerce => {
                            function.instruction(&LocalGet(arguments[0].0 as u32 + offset));
                            function.instruction(&Call(index("decimal-coerce")));
                        }
                        Nominal::DecimalNew => {
                            for value in arguments { function.instruction(&LocalGet(value.0 as u32 + offset)); }
                            function.instruction(&Call(index("decimal-from-integer")));
                        }
                        Nominal::IsMathContext => {
                            function.instruction(&LocalGet(arguments[0].0 as u32 + offset));
                            function.instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::MATH_CONTEXT)));
                        }
                        Nominal::IsDecimal => {
                            function.instruction(&LocalGet(arguments[0].0 as u32 + offset));
                            function.instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::DECIMAL)));
                        }
                        Nominal::IsBigInt => {
                            function.instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::BIGINT)));
                        }
                        Nominal::IsNumber => {
                            for instruction in runtime_abi::number_test(arguments[0].0 as u32 + offset) {
                                function.instruction(&instruction);
                            }
                        }
                        Nominal::Satisfies if method_sites.contains_key(&(ir as *const IrFunction as usize, inst.result.0 as u32)) => {
                            // Inline cache (#361): a user object's marker test
                            // depends only on its descriptor and the protocol
                            // epoch, so a site keeps two descriptors and
                            // answers. Other values take the runtime's test.
                            let cache = method_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            let (marker, object) = (arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset);
                            let descriptor = [LocalGet(object), RefCastNonNull(HeapType::Concrete(7)),
                                StructGet { struct_type_index: 7, field_index: 0 }];
                            function
                                .instruction(&LocalGet(object))
                                .instruction(&RefTestNonNull(HeapType::Concrete(7)))
                                .instruction(&If(BlockType::Result(ValType::I32)));
                            protocol_site_lookup(&mut function, cache, epoch_global, &descriptor,
                                &[LocalGet(marker), LocalGet(object),
                                    Call(index("protocol-marker-satisfies")), RefI31]);
                            function
                                .instruction(&RefCastNonNull(HeapType::I31))
                                .instruction(&I31GetU)
                                .instruction(&Else)
                                .instruction(&LocalGet(marker))
                                .instruction(&LocalGet(object))
                                .instruction(&Call(index("protocol-marker-satisfies")))
                                .instruction(&End);
                        }
                        Nominal::MethodGet if method_sites.contains_key(&(ir as *const IrFunction as usize, inst.result.0 as u32)) => {
                            // Inline cache (#355): the same descriptor at the
                            // same protocol epoch has the same method; any
                            // method table write since bumps the epoch.
                            let cache = method_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            let (descriptor, key) = (arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset);
                            protocol_site_lookup(&mut function, cache, epoch_global,
                                &[LocalGet(descriptor)], &[LocalGet(descriptor), LocalGet(key),
                                    Call(index("protocol-class-method-get"))]);
                        }
                        Nominal::NativeMethodGet => {
                            // Inline cache (#445): the same protocol fn and
                            // receiver kind at the same protocol epoch have
                            // the same implementation; extending a kind writes
                            // a closure property, which bumps the epoch.
                            let cache = native_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            let (protocol, receiver) = (arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset);
                            function
                                .instruction(&LocalGet(receiver))
                                .instruction(&Call(index("protocol-native-kind")))
                                .instruction(&LocalSet(field_index_local))
                                .instruction(&LocalGet(protocol))
                                .instruction(&GlobalGet(cache))
                                .instruction(&RefEq)
                                .instruction(&LocalGet(field_index_local))
                                .instruction(&GlobalGet(cache + 1))
                                .instruction(&I32Eq)
                                .instruction(&I32And)
                                .instruction(&GlobalGet(cache + 2))
                                .instruction(&GlobalGet(epoch_global))
                                .instruction(&I32Eq)
                                .instruction(&I32And)
                                .instruction(&If(BlockType::Result(VALUE)))
                                .instruction(&GlobalGet(cache + 3))
                                .instruction(&Else)
                                .instruction(&LocalGet(protocol))
                                .instruction(&LocalGet(field_index_local))
                                .instruction(&Call(index("protocol-native-method")))
                                .instruction(&GlobalSet(cache + 3))
                                .instruction(&LocalGet(protocol))
                                .instruction(&GlobalSet(cache))
                                .instruction(&LocalGet(field_index_local))
                                .instruction(&GlobalSet(cache + 1))
                                .instruction(&GlobalGet(epoch_global))
                                .instruction(&GlobalSet(cache + 2))
                                .instruction(&GlobalGet(cache + 3))
                                .instruction(&End);
                        }
                        Nominal::CoreNamedGet if field_sites.contains_key(&(ir as *const IrFunction as usize, inst.result.0 as u32)) => {
                            // Inline cache (#361): the same descriptor as last
                            // time reads the cached field; another looks its
                            // field up and caches it; anything else, or a name
                            // that isn't a field, takes the full lookup.
                            let site = (ir as *const IrFunction as usize, inst.result.0 as u32);
                            let cache = field_sites[&site];
                            let (object, name) = (arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset);
                            // A string's length is its array's (#361).
                            let length = length_sites.contains(&site);
                            if length {
                                function
                                    .instruction(&LocalGet(object))
                                    .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::STRING)))
                                    .instruction(&If(BlockType::Result(VALUE)))
                                    .instruction(&LocalGet(object))
                                    .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::STRING)))
                                    .instruction(&ArrayLen)
                                    .instruction(&LocalTee(field_index_local))
                                    .instruction(&I32Const(0x3fff_ffff))
                                    .instruction(&I32LeU)
                                    .instruction(&If(BlockType::Result(VALUE)))
                                    .instruction(&LocalGet(field_index_local))
                                    .instruction(&RefI31)
                                    .instruction(&Else)
                                    .instruction(&LocalGet(object))
                                    .instruction(&LocalGet(name))
                                    .instruction(&Call(index("named-property-get")))
                                    .instruction(&End)
                                    .instruction(&Else);
                            }
                            // The site's cached field, read inline when the
                            // object has the site's descriptor (#445), as
                            // named-site-get reads it; anything else, a
                            // method among them, takes the runtime's read
                            // (#378).
                            let storage = storage_locals + 6;
                            let site = |field| [GlobalGet(cache), StructGet { struct_type_index: runtime_abi::NAMED_SITE, field_index: field }];
                            let object_type = RefType { nullable: false, heap_type: HeapType::Concrete(7) };
                            let mut code = vec![Block(BlockType::Result(VALUE)), Block(BlockType::Empty)];
                            if let Some(descriptor) = descriptors_of.get(&(arguments[0].0 as u32)) {
                                code.extend([LocalGet(fields_of[&(arguments[0].0 as u32)]),
                                    BrOnNull(0), LocalSet(storage), LocalGet(*descriptor)]);
                            } else {
                            code.extend([
                                Block(BlockType::Result(ValType::Ref(object_type))),
                                LocalGet(object),
                                BrOnCast { relative_depth: 0, from_ref_type: RefType::EQREF, to_ref_type: object_type },
                                Drop,
                                Br(1),
                                End,
                                StructGet { struct_type_index: 7, field_index: 1 },
                                LocalSet(storage),
                                LocalGet(object),
                                RefCastNonNull(HeapType::Concrete(7)),
                                StructGet { struct_type_index: 7, field_index: 0 },
                            ]);
                            }
                            code.extend(site(runtime_abi::SITE_DESCRIPTOR));
                            code.extend([RefEq, I32Eqz, BrIf(0)]);
                            code.extend(site(runtime_abi::SITE_SLOT));
                            code.extend([LocalTee(field_index_local), I32Const(0), I32LtS, BrIf(0)]);
                            code.extend([LocalGet(storage), LocalGet(field_index_local), ArrayGet(runtime_abi::ARGS), Br(1), End]);
                            code.extend([GlobalGet(cache), LocalGet(object), LocalGet(name), Call(index("named-site-get")), End]);
                            for instruction in &code {
                                function.instruction(instruction);
                            }
                            if length {
                                function.instruction(&End);
                            }
                        }
                        Nominal::NamedGet if field_sites.contains_key(&(ir as *const IrFunction as usize, inst.result.0 as u32)) => {
                            // A user read (#462): a source object of the site's
                            // descriptor, for the caller the site was filled
                            // for, reads its cached field, or its method at the
                            // site's epoch, inline; anything else takes the
                            // runtime's cached read, which runs the guards.
                            let cache = field_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            let [owner, name, caller] = [0, 1, 2].map(|i| arguments[i].0 as u32 + offset);
                            let site = |field| [GlobalGet(cache), StructGet { struct_type_index: runtime_abi::NAMED_SITE, field_index: field }];
                            let object_type = RefType { nullable: false, heap_type: HeapType::Concrete(7) };
                            let mut code = vec![
                                Block(BlockType::Result(VALUE)),
                                Block(BlockType::Empty),
                                Block(BlockType::Result(ValType::Ref(object_type))),
                                LocalGet(owner),
                                BrOnCast { relative_depth: 0, from_ref_type: RefType::EQREF, to_ref_type: object_type },
                                Drop,
                                Br(1),
                                End,
                                LocalSet(object_local),
                                LocalGet(caller),
                            ];
                            code.extend(site(runtime_abi::SITE_OWNER_VALUE));
                            code.extend([RefEq, I32Eqz, BrIf(0), LocalGet(object_local), StructGet { struct_type_index: 7, field_index: 0 }]);
                            code.extend(site(runtime_abi::SITE_DESCRIPTOR));
                            code.extend([RefEq, I32Eqz, BrIf(0)]);
                            code.extend(site(runtime_abi::SITE_SLOT));
                            code.extend([LocalTee(field_index_local), I32Const(0), I32GeS, If(BlockType::Empty),
                                LocalGet(object_local), StructGet { struct_type_index: 7, field_index: 1 },
                                LocalGet(field_index_local), ArrayGet(runtime_abi::ARGS), Br(2), End]);
                            code.extend(site(runtime_abi::SITE_EPOCH));
                            code.extend([GlobalGet(epoch_global), I32Ne, BrIf(0)]);
                            code.extend(site(runtime_abi::SITE_METHOD));
                            code.extend([Br(1), End, GlobalGet(cache), LocalGet(owner), LocalGet(name), LocalGet(caller),
                                Call(index("user-named-site-get")), End]);
                            for instruction in &code {
                                function.instruction(instruction);
                            }
                        }
                        Nominal::CoreNamedSet if field_sites.contains_key(&(ir as *const IrFunction as usize, inst.result.0 as u32)) => {
                            // The runtime's admitted descriptor/slot hit, inline.
                            // Operands have already run, including a RHS that
                            // can reenter this site and replace its cache.
                            let cache = field_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            let [object, name, value] = [0, 1, 2].map(|i| arguments[i].0 as u32 + offset);
                            let site = |field| [GlobalGet(cache), StructGet { struct_type_index: runtime_abi::NAMED_SITE, field_index: field }];
                            let object_type = RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::OBJECT) };
                            let mut code = vec![Block(BlockType::Result(VALUE)), Block(BlockType::Empty)];
                            if let Some(descriptor) = descriptors_of.get(&(arguments[0].0 as u32)) {
                                code.extend([LocalGet(fields_of[&(arguments[0].0 as u32)]),
                                    BrOnNull(0), LocalSet(storage_locals + 6), LocalGet(*descriptor)]);
                            } else {
                                code.extend([Block(BlockType::Result(ValType::Ref(object_type))),
                                    LocalGet(object),
                                    BrOnCast { relative_depth: 0, from_ref_type: RefType::EQREF, to_ref_type: object_type },
                                    Drop, Br(1), End, LocalSet(object_local),
                                    LocalGet(object_local), StructGet { struct_type_index: runtime_abi::OBJECT, field_index: 0 }]);
                            }
                            code.extend(site(runtime_abi::SITE_DESCRIPTOR));
                            code.extend([RefEq, I32Eqz, BrIf(0)]);
                            code.extend(site(runtime_abi::SITE_SLOT));
                            code.extend([LocalTee(field_index_local), I32Const(0), I32LtS, BrIf(0)]);
                            if descriptors_of.contains_key(&(arguments[0].0 as u32)) {
                                code.push(LocalGet(storage_locals + 6));
                            } else {
                                code.extend([LocalGet(object_local), StructGet { struct_type_index: runtime_abi::OBJECT, field_index: 1 }]);
                            }
                            code.extend([LocalGet(field_index_local), LocalGet(value), ArraySet(runtime_abi::ARGS),
                                LocalGet(value), Br(1), End, GlobalGet(cache), LocalGet(object),
                                LocalGet(name), LocalGet(value), Call(index("named-site-set")), End]);
                            for instruction in code { function.instruction(&instruction); }
                        }
                        Nominal::IsKeyword => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&Call(index("keyword-test")));
                        }
                        Nominal::AllSmallIntegers => {
                            for (k, argument) in arguments.iter().enumerate() {
                                function
                                    .instruction(&LocalGet(argument.0 as u32 + offset))
                                    .instruction(&RefTestNonNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 }));
                                if k > 0 {
                                    function.instruction(&I32And);
                                }
                            }
                        }
                        Nominal::NumberHash if locals.of(inst.result) == unboxed::Repr::Long => {
                            // The hash, raw (#387): Murmur3 of an i31 inline,
                            // and anything else through `number-hash`.
                            let value = arguments[0].0 as u32 + offset;
                            let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
                            let [wide, _, _] = long_locals;
                            function
                                .instruction(&LocalGet(value))
                                .instruction(&RefTestNonNull(i31))
                                .instruction(&If(BlockType::Result(ValType::I64)))
                                .instruction(&LocalGet(value))
                                .instruction(&RefCastNonNull(i31))
                                .instruction(&I31GetS)
                                .instruction(&I64ExtendI32S)
                                .instruction(&LocalSet(wide));
                            for instruction in runtime_abi::hash_long_in(wide, field_index_local) {
                                function.instruction(&instruction);
                            }
                            function
                                .instruction(&I64ExtendI32S)
                                .instruction(&Else)
                                .instruction(&LocalGet(value))
                                .instruction(&Call(index("number-hash")))
                                .instruction(&LocalTee(method_local))
                                .instruction(&RefTestNonNull(i31))
                                .instruction(&If(BlockType::Result(ValType::I64)))
                                .instruction(&LocalGet(method_local))
                                .instruction(&RefCastNonNull(i31))
                                .instruction(&I31GetS)
                                .instruction(&I64ExtendI32S)
                                .instruction(&Else)
                                .instruction(&LocalGet(method_local))
                                .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::LONG)))
                                .instruction(&StructGet { struct_type_index: runtime_abi::LONG, field_index: 0 })
                                .instruction(&End)
                                .instruction(&End);
                        }
                        Nominal::NumberHash => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&Call(index("number-hash")));
                        }
                        Nominal::IsObject => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&RefTestNonNull(HeapType::Concrete(7)));
                        }
                        Nominal::SameReference => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                                .instruction(&RefEq);
                        }
                        Nominal::IsLongValue => {
                            let value = arguments[0].0 as u32 + offset;
                            function
                                .instruction(&LocalGet(value))
                                .instruction(&RefTestNonNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 }))
                                .instruction(&LocalGet(value))
                                .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::LONG)))
                                .instruction(&I32Or);
                        }
                        Nominal::PackedGet(kind) | Nominal::PackedSet(kind) => {
                            // A hinted aget or aset (#389). The array is cast
                            // to a packed array of the hinted kind, as
                            // Clojure's hint is a checked cast, so a wrong hint
                            // throws (as #385's `^long` parameters do). The
                            // element is then read or written inline for an
                            // index in range, raw (#387) when its local is;
                            // any other index takes the generic path, which
                            // reports a bad index as before.
                            let set = matches!(operation, Nominal::PackedSet(_));
                            let kind = *kind;
                            let array = arguments[0].0 as u32 + offset;
                            let position = arguments[1];
                            // An object array's storage is the args array (#378).
                            let storage = if kind == 6 { runtime_abi::ARGS } else { runtime_abi::PACKED_STORAGE + kind as u32 };
                            let typed = storage_locals + kind as u32;
                            let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
                            let cast = |heap_type| BrOnCastFail {
                                relative_depth: 0,
                                from_ref_type: wasm_encoder::RefType::EQREF,
                                to_ref_type: wasm_encoder::RefType { nullable: false, heap_type },
                            };
                            let [wide, _, _] = long_locals;
                            // Doubles and floats read as doubles; the rest as longs.
                            let floating = matches!(kind, 3 | 5);
                            // An object array's element is a value (#378).
                            let object = kind == 6;
                            let element_type = if object { VALUE } else if floating { ValType::F64 } else { ValType::I64 };
                            let name = runtime_abi::packed_kind_name(kind);
                            let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
                            let message = format!("{} ^{name}s requires {article} {name} array", if set { "aset" } else { "aget" });
                            // An aset whose value is raw returns it raw.
                            let raw_result = set && locals.of(inst.result) != unboxed::Repr::Boxed;
                            let result = if !set {
                                element_type
                            } else if raw_result {
                                locals.of(inst.result).val_type().expect("raw aset result")
                            } else {
                                VALUE
                            };
                            // The access runs inside the cast's success block,
                            // where the storage local, non-nullable, is set.
                            let mut code = vec![Block(BlockType::Result(result)), Block(BlockType::Result(VALUE))];
                            if let Some(view) = view_of.get(&(inst.result.0 as u32)) {
                                // The loop's view of the array (#389): null
                                // when it isn't of this kind, which throws.
                                code.extend([LocalGet(*view), RefIsNull, If(BlockType::Empty), LocalGet(array), Br(1), End,
                                    LocalGet(*view), RefAsNonNull, LocalSet(typed)]);
                            } else {
                                code.extend([LocalGet(array),
                                    cast(HeapType::Concrete(runtime_abi::PACKED)),
                                    StructGet { struct_type_index: runtime_abi::PACKED, field_index: 0 },
                                    cast(HeapType::Concrete(storage)), LocalSet(typed)]);
                            }
                            // The inline path, or out to the generic one.
                            code.extend([Block(BlockType::Result(result)), Block(BlockType::Empty)]);
                            match locals.of(position) {
                                unboxed::Repr::Long | unboxed::Repr::MaybeLong => {
                                    // Unsigned, so a negative index is out of range.
                                    if locals.of(position).maybe() {
                                        code.extend([LocalGet(locals.valid(position)), I32Eqz, BrIf(0)]);
                                    }
                                    let raw = locals.raw(position);
                                    code.extend([LocalGet(raw), I32WrapI64, LocalSet(field_index_local),
                                        LocalGet(typed), ArrayLen, I64ExtendI32U, LocalGet(raw), I64LeU, BrIf(0)]);
                                }
                                _ => {
                                    let boxed = position.0 as u32 + offset;
                                    code.extend([LocalGet(boxed), RefTestNonNull(i31), I32Eqz, BrIf(0),
                                        LocalGet(boxed), RefCastNonNull(i31), I31GetS, LocalTee(field_index_local),
                                        LocalGet(typed), ArrayLen, I32GeU, BrIf(0)]);
                                }
                            }
                            let element = [LocalGet(typed), LocalGet(field_index_local)];
                            if set {
                                let value = arguments[2];
                                let repr = locals.of(value);
                                let boxed = value.0 as u32 + offset;
                                match (kind, repr) {
                                    // An object array takes any value (#378).
                                    (6, _) => {
                                        code.extend(element);
                                        locals.push_boxed(&mut code, value);
                                        code.push(ArraySet(storage));
                                    }
                                    // A raw value of the element's kind; an
                                    // int takes only an i31's worth inline,
                                    // as a boxed value does.
                                    (2, unboxed::Repr::Long) => {
                                        let raw = locals.raw(value);
                                        code.extend([LocalGet(raw), I64Const(1 << 30), I64Add, I64Const(1 << 31), I64LtU, I32Eqz, BrIf(0)]);
                                        code.extend(element);
                                        code.extend([LocalGet(raw), I32WrapI64, ArraySet(storage)]);
                                    }
                                    (4, unboxed::Repr::Long) | (5, unboxed::Repr::Double) => {
                                        code.extend(element);
                                        code.extend([LocalGet(locals.raw(value)), ArraySet(storage)]);
                                    }
                                    (_, unboxed::Repr::Boxed) => {
                                        match kind {
                                            2 => {
                                                code.extend([LocalGet(boxed), RefTestNonNull(i31), I32Eqz, BrIf(0)]);
                                                code.extend(element);
                                                code.extend([LocalGet(boxed), RefCastNonNull(i31), I31GetS, ArraySet(storage)]);
                                            }
                                            4 => {
                                                code.extend([LocalGet(boxed), RefTestNonNull(i31), If(BlockType::Result(ValType::I64)),
                                                    LocalGet(boxed), RefCastNonNull(i31), I31GetS, I64ExtendI32S, Else,
                                                    LocalGet(boxed), RefTestNonNull(HeapType::Concrete(runtime_abi::LONG)), I32Eqz, BrIf(1),
                                                    LocalGet(boxed), RefCastNonNull(HeapType::Concrete(runtime_abi::LONG)),
                                                    StructGet { struct_type_index: runtime_abi::LONG, field_index: 0 }, End,
                                                    LocalSet(wide)]);
                                                code.extend(element);
                                                code.extend([LocalGet(wide), ArraySet(storage)]);
                                            }
                                            _ => {
                                                code.extend([LocalGet(boxed), RefTestNonNull(HeapType::Concrete(runtime_abi::NUMBER)), I32Eqz, BrIf(0)]);
                                                code.extend(element);
                                                code.extend([LocalGet(boxed), RefCastNonNull(HeapType::Concrete(runtime_abi::NUMBER)),
                                                    StructGet { struct_type_index: runtime_abi::NUMBER, field_index: 0 }, ArraySet(storage)]);
                                            }
                                        }
                                    }
                                    // Any other raw value takes the generic path.
                                    _ => code.push(Br(0)),
                                }
                                if !matches!(code.last(), Some(Br(0))) {
                                    if raw_result {
                                        code.push(LocalGet(locals.raw(value)));
                                    } else {
                                        locals.push_boxed(&mut code, value);
                                    }
                                    code.push(Br(1));
                                }
                            } else {
                                code.extend(element);
                                code.push(match kind {
                                    0 | 1 => ArrayGetS(storage),
                                    _ => ArrayGet(storage),
                                });
                                match kind {
                                    0 | 1 | 2 => code.push(I64ExtendI32S),
                                    3 => code.push(F64PromoteF32),
                                    _ => {}
                                }
                                code.push(Br(1));
                            }
                            code.push(End);
                            // The generic path: an index it converts, or one it
                            // reports. A packed array's element is a long or a
                            // double, by kind.
                            for argument in arguments {
                                locals.push_boxed(&mut code, *argument);
                            }
                            code.push(Call(index(if set { "packed-array-set-3" } else { "packed-array-get-2" })));
                            if raw_result {
                                code.extend([Drop, LocalGet(locals.raw(arguments[2]))]);
                            }
                            if !set && !object {
                                if floating {
                                    code.extend([RefCastNonNull(HeapType::Concrete(runtime_abi::NUMBER)),
                                        StructGet { struct_type_index: runtime_abi::NUMBER, field_index: 0 }]);
                                } else {
                                    code.extend([LocalTee(method_local), RefTestNonNull(i31), If(BlockType::Result(ValType::I64)),
                                        LocalGet(method_local), RefCastNonNull(i31), I31GetS, I64ExtendI32S, Else,
                                        LocalGet(method_local), RefCastNonNull(HeapType::Concrete(runtime_abi::LONG)),
                                        StructGet { struct_type_index: runtime_abi::LONG, field_index: 0 }, End]);
                                }
                            }
                            // Out of the cast's block; a wrong hint throws.
                            code.extend([End, Br(1), End, Drop]);
                            let error = |code: &mut Vec<Instruction<'static>>| {
                                code.extend(message.encode_utf16().map(|unit| I32Const(i32::from(unit))));
                                code.extend([ArrayNewFixed { array_type_index: runtime_abi::STRING, array_size: message.encode_utf16().count() as u32 },
                                    Call(index("language-error-new")), Throw(0)]);
                            };
                            error(&mut code);
                            code.push(End);
                            if !set && !object && locals.of(inst.result) == unboxed::Repr::Boxed {
                                // Resumable functions keep values boxed.
                                if floating {
                                    code.push(StructNew(runtime_abi::NUMBER));
                                } else {
                                    code.extend(runtime_abi::long_box_in(wide));
                                }
                            }
                            for instruction in code {
                                function.instruction(&instruction);
                            }
                        }
                        Nominal::ChunkIndexAdd => {
                            // Resumable code keeps checked long operands boxed.
                            // The owned caller's bounds proof still applies.
                            let mut code = runtime_abi::long_to_i64(arguments[0].0 as u32 + offset);
                            code.extend(runtime_abi::long_to_i64(arguments[1].0 as u32 + offset));
                            code.push(I64Add);
                            code.extend(runtime_abi::long_box_in(long_locals[0]));
                            for instruction in code { function.instruction(&instruction); }
                        }
                        Nominal::LongCast | Nominal::DoubleCast | Nominal::UncheckedLongCast => {
                            // The checked cast (#385): an i31 or a boxed long
                            // (or double) inline, anything else through the
                            // runtime's cast, which converts or throws. The
                            // value is raw when its local is.
                            let long = !matches!(operation, Nominal::DoubleCast);
                            let fallback = match operation {
                                Nominal::LongCast => "number-cast-long",
                                Nominal::UncheckedLongCast => "number-cast-unchecked-long",
                                _ => "number-cast-double",
                            };
                            let value = arguments[0].0 as u32 + offset;
                            let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
                            let boxed = if long { runtime_abi::LONG } else { runtime_abi::NUMBER };
                            let raw = if long { ValType::I64 } else { ValType::F64 };
                            let read_box = |function: &mut Function| {
                                function
                                    .instruction(&RefCastNonNull(HeapType::Concrete(boxed)))
                                    .instruction(&StructGet { struct_type_index: boxed, field_index: 0 });
                            };
                            function
                                .instruction(&LocalGet(value))
                                .instruction(&RefTestNonNull(i31))
                                .instruction(&If(BlockType::Result(raw)))
                                .instruction(&LocalGet(value))
                                .instruction(&RefCastNonNull(i31))
                                .instruction(&I31GetS)
                                .instruction(&if long { I64ExtendI32S } else { F64ConvertI32S })
                                .instruction(&Else)
                                .instruction(&LocalGet(value))
                                .instruction(&RefTestNonNull(HeapType::Concrete(boxed)))
                                .instruction(&If(BlockType::Result(raw)))
                                .instruction(&LocalGet(value));
                            read_box(&mut function);
                            function
                                .instruction(&Else)
                                .instruction(&LocalGet(value))
                                .instruction(&Call(index(fallback)));
                            if long {
                                // The cast's result is an i31 or a long box.
                                function
                                    .instruction(&LocalTee(method_local))
                                    .instruction(&RefTestNonNull(i31))
                                    .instruction(&If(BlockType::Result(ValType::I64)))
                                    .instruction(&LocalGet(method_local))
                                    .instruction(&RefCastNonNull(i31))
                                    .instruction(&I31GetS)
                                    .instruction(&I64ExtendI32S)
                                    .instruction(&Else)
                                    .instruction(&LocalGet(method_local));
                                read_box(&mut function);
                                function.instruction(&End);
                            } else {
                                read_box(&mut function);
                            }
                            function.instruction(&End).instruction(&End);
                            if locals.of(inst.result) == unboxed::Repr::Boxed {
                                // Resumable functions keep values boxed.
                                if long {
                                    function.instruction(&LocalSet(long_locals[0]));
                                    for instruction in runtime_abi::long_box_in(long_locals[0]) {
                                        function.instruction(&instruction);
                                    }
                                } else {
                                    function.instruction(&StructNew(runtime_abi::NUMBER));
                                }
                            }
                        }
                        Nominal::VectorNth => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                                .instruction(&Call(index("vector-nth")));
                        }
                        // A lookup calls map-lookup itself, with its mode and
                        // the value for an absent key (#442).
                        Nominal::ArrayMapGet | Nominal::MapContains | Nominal::MapGetOr => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                                .instruction(&I32Const(i32::from(matches!(operation, Nominal::MapContains))));
                            match arguments.get(2) {
                                Some(not_found) => function.instruction(&LocalGet(not_found.0 as u32 + offset)),
                                None => function.instruction(&RefNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq })),
                            };
                            function.instruction(&Call(index("map-lookup")));
                        }
                        Nominal::MapAssoc => {
                            for argument in arguments {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index("map-assoc")));
                        }
                        Nominal::HashMapTypes => {
                            for argument in arguments {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index("hash-map-type-register")));
                        }
                        Nominal::ChunkLongSum | Nominal::ChunkTypes | Nominal::ChunkNth | Nominal::ChunkAppend | Nominal::ChunkArray | Nominal::ChunkOffset | Nominal::ChunkSeq | Nominal::TransientVectorTypes | Nominal::TransientVectorAppend | Nominal::TransientMapTypes | Nominal::TransientMapAssoc | Nominal::TransientMapConj => {
                            let mut code = vec![];
                            for argument in arguments {
                                locals.push_boxed(&mut code, *argument);
                            }
                            code.push(Call(index(match operation {
                                Nominal::ChunkTypes => "chunk-types-register",
                                Nominal::ChunkLongSum => "chunk-long-sum",
                                Nominal::TransientVectorTypes => "transient-vector-types-register",
                                Nominal::TransientVectorAppend => "transient-vector-append",
                                Nominal::TransientMapTypes => "transient-map-types-register",
                                Nominal::TransientMapAssoc => "transient-map-assoc",
                                Nominal::TransientMapConj => "transient-map-conj",
                                Nominal::ChunkNth => "chunk-nth",
                                Nominal::ChunkArray => "chunk-array",
                                Nominal::ChunkOffset => "chunk-offset",
                                Nominal::ChunkSeq => "chunk-seq",
                                _ => "chunk-buffer-append",
                            })));
                            for instruction in code { function.instruction(&instruction); }
                        }
                        Nominal::StringHash => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&Call(index("string-hash")));
                        }
                        Nominal::ObjectArrayCopy(backward) => {
                            for argument in arguments {
                                let mut code = vec![];
                                locals.push_boxed(&mut code, *argument);
                                for instruction in code { function.instruction(&instruction); }
                            }
                            function.instruction(&Call(index(if *backward { "object-array-copy-backward" } else { "object-array-copy-forward" })));
                        }
                        Nominal::StringAppendLong => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                                .instruction(&Call(index("string-append-long")));
                        }
                        Nominal::IsDescriptor => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&RefTestNonNull(HeapType::Concrete(6)));
                        }
                        Nominal::IsString => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::STRING)));
                        }
                        Nominal::IsClosure => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&RefTestNonNull(HeapType::Concrete(4)));
                        }
                        Nominal::BindCallable => {
                            for argument in arguments {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index("callable-bind")));
                        }
                        Nominal::CallableKeyRegister => {
                            for argument in arguments {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index("callable-key-register")));
                        }
                        // A direct link's closure (#360): read for a call, and
                        // published when its definition runs.
                        Nominal::DirectLinkRead(link) => {
                            function.instruction(&GlobalGet(links.slots[&super::hir::link_parts(*link).0]));
                        }
                        Nominal::KeywordCacheGet(cache) => {
                            function.instruction(&GlobalGet(keyword_caches[cache]));
                        }
                        Nominal::KeywordCacheEmpty(cache) => {
                            function.instruction(&GlobalGet(keyword_caches[cache])).instruction(&RefIsNull);
                        }
                        Nominal::KeywordCacheSet(cache) => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&GlobalSet(keyword_caches[cache]))
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset));
                        }
                        Nominal::KeywordIntern => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&Call(index("keyword-intern")));
                        }
                        Nominal::DirectLinkSet(slot) => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&GlobalSet(links.slots[slot]))
                                .instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, sentinels));
                        }
                        Nominal::DirectLinkDefine(link) => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&GlobalSet(links.slots[&super::hir::link_parts(*link).0]))
                                .instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, sentinels));
                        }
                        Nominal::LiveDispatcher | Nominal::IFnLiveDispatcher => {
                            for argument in arguments {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index(if matches!(operation, Nominal::IFnLiveDispatcher) { "ifn-live-dispatcher-new" } else { "protocol-live-dispatcher-new" })));
                        }
                        Nominal::NativeMarker(kind) | Nominal::NativeSet(kind) => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&I32Const(kind.index()));
                            if let Some(argument) = arguments.get(1) {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index(
                                if matches!(operation, Nominal::NativeMarker(_)) {
                                    "protocol-native-marker-set"
                                } else {
                                    "protocol-native-method-set"
                                },
                            )));
                        }
                        Nominal::ApplySpread => {
                            function.instruction(&LocalGet(arguments[0].0 as u32 + offset));
                            for argument in &arguments[1..arguments.len() - 1] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: arguments.len() as u32 - 2 })
                                .instruction(&LocalGet(arguments.last().unwrap().0 as u32 + offset))
                                .instruction(&Call(index("apply")));
                        }
                        Nominal::Array | Nominal::Descriptor(_) | Nominal::CoreDescriptor | Nominal::CoreStaticsDescriptor | Nominal::CoreClosedDescriptor | Nominal::CoreConstructibleDescriptor => {
                            let mutable = match operation {
                                Nominal::Descriptor(mutable) => *mutable,
                                _ => 0,
                            };
                            // Descriptors take the type's name last (#304).
                            let named = usize::from(*operation != Nominal::Array);
                            let fields = arguments.len() - mutable - named;
                            for argument in &arguments[..fields] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&ArrayNewFixed {
                                array_type_index: runtime_abi::ARGS,
                                array_size: fields as u32,
                            });
                            // Descriptors also take their mutable field names.
                            if *operation != Nominal::Array {
                                if mutable == 0 {
                                    function.instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, sentinels));
                                } else {
                                    for argument in &arguments[fields..fields + mutable] {
                                        function.instruction(&LocalGet(argument.0 as u32 + offset));
                                    }
                                    function.instruction(&ArrayNewFixed {
                                        array_type_index: runtime_abi::ARGS,
                                        array_size: mutable as u32,
                                    });
                                }
                            }
                            if *operation != Nominal::Array {
                                let name = arguments.last().expect("descriptor name");
                                function.instruction(&LocalGet(name.0 as u32 + offset));
                            }
                            if matches!(operation, Nominal::Descriptor(_)) {
                                function.instruction(&Call(index("descriptor-new")));
                            } else if *operation == Nominal::CoreDescriptor {
                                function.instruction(&Call(index("descriptor-new-core")));
                            } else if *operation == Nominal::CoreStaticsDescriptor {
                                function.instruction(&Call(index("descriptor-new-core-statics")));
                            } else if *operation == Nominal::CoreClosedDescriptor {
                                function.instruction(&Call(index("descriptor-new-core-closed")));
                            } else if *operation == Nominal::CoreConstructibleDescriptor {
                                function.instruction(&Call(index("descriptor-new-core-constructible")));
                            }
                        }
                        Nominal::Construct => {
                            // Operands are already evaluated SSA locals. A site
                            // holding this constructor builds its object inline
                            // from the fresh arguments, as object-new would
                            // (#443): the descriptor, the fields, a prototype
                            // cell and an unassigned identity UID. Otherwise
                            // the site asks construct-site-admit whether to hold
                            // it, then selects the live plan at the original
                            // constructor-selection point: plain descriptors
                            // share runtime construction directly, and special
                            // constructor closures retain generic invocation.
                            let site = construct_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            let constructor = arguments[0].0 as u32 + offset;
                            let count = arguments.len() as u32 - 1;
                            let none = HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq };
                            function
                                .instruction(&LocalGet(constructor))
                                .instruction(&GlobalGet(site))
                                .instruction(&RefEq)
                                .instruction(&If(BlockType::Result(VALUE)))
                                .instruction(&GlobalGet(site + 1))
                                .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::DESCRIPTOR)));
                            for argument in &arguments[1..] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function
                                .instruction(&ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: count })
                                .instruction(&GlobalGet(site + 2))
                                .instruction(&ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: 1 })
                                .instruction(&I32Const(0))
                                .instruction(&RefI31)
                                .instruction(&StructNew(runtime_abi::OBJECT))
                                .instruction(&Else)
                                .instruction(&LocalGet(constructor))
                                .instruction(&Call(index("constructor-descriptor")))
                                .instruction(&I32Const(count as i32))
                                .instruction(&Call(index("construct-site-admit")))
                                .instruction(&GlobalSet(site + 1))
                                .instruction(&GlobalGet(site + 1))
                                .instruction(&RefIsNull)
                                .instruction(&If(BlockType::Empty))
                                .instruction(&RefNull(none))
                                .instruction(&GlobalSet(site))
                                .instruction(&Else)
                                .instruction(&GlobalGet(site + 1))
                                .instruction(&Call(index("owned-descriptor-prototype")))
                                .instruction(&GlobalSet(site + 2))
                                .instruction(&LocalGet(constructor))
                                .instruction(&GlobalSet(site))
                                .instruction(&End)
                                .instruction(&LocalGet(constructor))
                                .instruction(&Call(index("constructor-descriptor")))
                                .instruction(&Call(index("source-constructor-plan")));
                            for argument in &arguments[1..] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function
                                .instruction(&ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: count })
                                .instruction(&Call(index("source-construct-from-plan")))
                                .instruction(&End);
                        }
                        Nominal::CheckNumericCallee => {
                            let callee = arguments[0].0 as u32 + offset;
                            // Descriptors are never closures or callable objects.
                            // Reject them before arguments, without changing the
                            // call site's entry-cache-only convention (ADR-0004).
                            let message: Vec<_> = "Not callable".encode_utf16().collect();
                            function.instruction(&LocalGet(callee))
                                .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::DESCRIPTOR)))
                                .instruction(&If(BlockType::Empty))
                                .instruction(&GlobalGet(context.not_callable_global.expect("collected error descriptor")))
                                .instruction(&GlobalGet(literals[&message]))
                                .instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, sentinels))
                                .instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, sentinels))
                                .instruction(&I32Const(0)).instruction(&RefI31)
                                .instruction(&StructNew(runtime_abi::LANGUAGE_ERROR))
                                .instruction(&Throw(0)).instruction(&End)
                                .instruction(&LocalGet(callee));
                        }
                        Nominal::NumericClass(kind) => {
                            function.instruction(&I32Const(i32::from(*kind))).instruction(&Call(index("numeric-class-value")));
                        }
                        Nominal::Instance => {
                            // The site's cached test (#378). An i31 is no
                            // instance, which is answered inline once the site
                            // holds this constructor, as the test would answer
                            // it (#444); a cold or other site takes the test.
                            let cache = instance_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            let (constructor, value) = (arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset);
                            function
                                .instruction(&Block(BlockType::Result(ValType::I32)))
                                .instruction(&LocalGet(value))
                                .instruction(&RefTestNonNull(HeapType::I31))
                                .instruction(&If(BlockType::Empty))
                                .instruction(&GlobalGet(cache))
                                .instruction(&StructGet { struct_type_index: runtime_abi::NAMED_SITE, field_index: runtime_abi::SITE_DESCRIPTOR })
                                .instruction(&LocalGet(constructor))
                                .instruction(&RefEq)
                                .instruction(&If(BlockType::Empty))
                                .instruction(&I32Const(0))
                                .instruction(&Br(2))
                                .instruction(&End)
                                .instruction(&End);
                            let site =|field| [GlobalGet(cache), StructGet { struct_type_index: runtime_abi::NAMED_SITE, field_index: field }];
                            let object = || [LocalGet(object_local), StructGet { struct_type_index: 7, field_index: 0 }];
                            // A source object, cast once; anything else takes the test.
                            let object_type = RefType { nullable: false, heap_type: HeapType::Concrete(7) };
                            let mut code = vec![Block(BlockType::Empty), Block(BlockType::Result(ValType::Ref(object_type))), LocalGet(value),
                                BrOnCast { relative_depth: 0, from_ref_type: RefType::EQREF, to_ref_type: object_type },
                                Drop, Br(1), End, LocalSet(object_local)];
                            // The site's constructor at its epoch: an object of
                            // its descriptor, other than its prototype, is an
                            // instance (#445); one of the descriptor and
                            // prototype the test found not to be an instance
                            // isn't (#457). Its one-element prototype cell is
                            // read in the args local.
                            let storage = storage_locals + 6;
                            code.extend(site(runtime_abi::SITE_DESCRIPTOR));
                            code.extend([LocalGet(constructor), RefEq]);
                            code.extend(site(runtime_abi::SITE_EPOCH));
                            code.extend([GlobalGet(epoch_global), I32Eq, I32And, If(BlockType::Empty)]);
                            code.extend(object());
                            code.extend(site(runtime_abi::SITE_METHOD));
                            code.extend([RefEq, If(BlockType::Empty), LocalGet(value)]);
                            code.extend(site(runtime_abi::SITE_DESCRIPTOR_2));
                            code.extend([RefEq, I32Eqz, If(BlockType::Empty), I32Const(1), Br(4), End, Else]);
                            code.extend(object());
                            code.extend(site(runtime_abi::SITE_OWNER_VALUE));
                            code.push(RefEq);
                            code.extend(site(runtime_abi::SITE_OWNER_EPOCH));
                            code.extend([GlobalGet(epoch_global), I32Eq, I32And, If(BlockType::Empty)]);
                            let args = RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::ARGS) };
                            code.extend([Block(BlockType::Empty), Block(BlockType::Result(ValType::Ref(args))),
                                LocalGet(object_local), StructGet { struct_type_index: 7, field_index: 2 },
                                BrOnCast { relative_depth: 0, from_ref_type: RefType::EQREF, to_ref_type: args },
                                Drop, Br(1), End,
                                LocalTee(storage), ArrayLen, I32Const(1), I32Ne, BrIf(0),
                                LocalGet(storage), I32Const(0), ArrayGet(runtime_abi::ARGS)]);
                            code.extend(site(runtime_abi::SITE_OWNER));
                            code.extend([RefEq, If(BlockType::Empty), I32Const(0), Br(6), End, End, End, End, End, End]);
                            for instruction in &code {
                                function.instruction(instruction);
                            }
                            function
                                .instruction(&GlobalGet(cache))
                                .instruction(&LocalGet(constructor))
                                .instruction(&LocalGet(value))
                                .instruction(&Call(index("instance-site-test")))
                                .instruction(&End);
                        }
                        Nominal::Set | Nominal::Marker => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&Call(index("constructor-descriptor")));
                            for argument in &arguments[1..] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index(
                                if *operation == Nominal::Instance {
                                    "object-instance"
                                } else if *operation == Nominal::Marker {
                                    "protocol-marker-set"
                                } else {
                                    "protocol-method-set"
                                },
                            )));
                            if *operation == Nominal::Set {
                                function.instruction(&LocalGet(arguments[2].0 as u32 + offset));
                            }
                        }
                        Nominal::FieldSet(field) => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&I32Const(*field as i32))
                                .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                                .instruction(&Call(index("object-field-set")))
                                .instruction(&LocalGet(arguments[1].0 as u32 + offset));
                        }
                        Nominal::Field(field) => {
                            // Inline (#378): an object's field in range is read
                            // from its fields array; anything else, with its
                            // errors, takes object-field-get. One type test,
                            // which casts, and one read of the fields array,
                            // held in the args storage local (#445).
                            let object = arguments[0].0 as u32 + offset;
                            let storage = storage_locals + 6;
                            let cached = fields_of.get(&(arguments[0].0 as u32)).copied();
                            let slow = |function: &mut Function| {
                                function
                                    .instruction(&LocalGet(object))
                                    .instruction(&I32Const(*field as i32))
                                    .instruction(&Call(index("object-field-get")));
                            };
                            if let Some(fields) = cached {
                                // The parameter's fields array, read on entry.
                                function
                                    .instruction(&Block(BlockType::Result(VALUE)))
                                    .instruction(&Block(BlockType::Empty))
                                    .instruction(&LocalGet(fields))
                                    .instruction(&BrOnNull(0))
                                    .instruction(&LocalTee(storage))
                                    .instruction(&ArrayLen)
                                    .instruction(&I32Const(*field as i32))
                                    .instruction(&I32LeU)
                                    .instruction(&BrIf(0))
                                    .instruction(&LocalGet(storage))
                                    .instruction(&I32Const(*field as i32))
                                    .instruction(&ArrayGet(runtime_abi::ARGS))
                                    .instruction(&Br(1))
                                    .instruction(&End);
                                slow(&mut function);
                                function.instruction(&End);
                            } else {
                            function
                                .instruction(&Block(BlockType::Result(VALUE)))
                                .instruction(&Block(BlockType::Result(ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(7) }))))
                                .instruction(&LocalGet(object))
                                .instruction(&BrOnCast {
                                    relative_depth: 0,
                                    from_ref_type: RefType::EQREF,
                                    to_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(7) },
                                })
                                .instruction(&Drop);
                            slow(&mut function);
                            function
                                .instruction(&Br(1))
                                .instruction(&End)
                                .instruction(&StructGet { struct_type_index: 7, field_index: 1 })
                                .instruction(&LocalTee(storage))
                                .instruction(&ArrayLen)
                                .instruction(&I32Const(*field as i32))
                                .instruction(&I32GtU)
                                .instruction(&If(BlockType::Result(VALUE)))
                                .instruction(&LocalGet(storage))
                                .instruction(&I32Const(*field as i32))
                                .instruction(&ArrayGet(runtime_abi::ARGS))
                                .instruction(&Else);
                            slow(&mut function);
                            function.instruction(&End).instruction(&End);
                            }
                        }
                        Nominal::Key(field) => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&I32Const(*field as i32))
                                .instruction(&Call(index(
                                    if matches!(operation, Nominal::Field(_)) {
                                        "object-field-get"
                                    } else {
                                        "protocol-key"
                                    },
                                )));
                        }
                        Nominal::ObjectInvoke if object_call_sites.contains_key(&(ir as *const IrFunction as usize, inst.result.0 as u32)) => {
                            // A method call site (#462): the method it last saw
                            // calls the entry it holds; another asks the
                            // runtime for an Object wrapper's function, which
                            // the site then holds with its entry. A method
                            // that isn't one is held with a null function, and
                            // it and any other take object-method-invoke-N.
                            let first = object_call_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            let (method_global, function_global, entry_global) = (first, first + 1, first + 2);
                            let n = arguments.len() - 2;
                            let k = n as u32 + 1;
                            let method = arguments[0].0 as u32 + offset;
                            let operands = |function: &mut Function, from: usize| {
                                for argument in &arguments[from..] {
                                    function.instruction(&LocalGet(argument.0 as u32 + offset));
                                }
                            };
                            let cached_call = |function: &mut Function| {
                                function.instruction(&GlobalGet(function_global)).instruction(&GlobalGet(entry_global)).instruction(&RefAsNonNull);
                                operands(function, 1);
                                function.instruction(&Call(index(object_method_call_name(n))));
                            };
                            let uncached = |function: &mut Function| {
                                operands(function, 0);
                                function.instruction(&Call(index(object_method_invoke_name(n))));
                            };
                            function
                                .instruction(&Block(BlockType::Result(VALUE)))
                                .instruction(&Block(BlockType::Empty))
                                .instruction(&LocalGet(method))
                                .instruction(&GlobalGet(method_global))
                                .instruction(&RefEq)
                                .instruction(&I32Eqz)
                                .instruction(&BrIf(0))
                                .instruction(&GlobalGet(function_global))
                                .instruction(&RefIsNull)
                                .instruction(&If(BlockType::Result(VALUE)));
                            uncached(&mut function);
                            function.instruction(&Else);
                            cached_call(&mut function);
                            function.instruction(&End).instruction(&Br(1)).instruction(&End);
                            operands(&mut function, 0);
                            function
                                .instruction(&Call(index(object_method_site_name(n))))
                                .instruction(&LocalSet(method_local))
                                .instruction(&LocalGet(method))
                                .instruction(&GlobalSet(method_global))
                                .instruction(&LocalGet(method_local))
                                .instruction(&GlobalSet(function_global))
                                .instruction(&LocalGet(method_local))
                                .instruction(&RefIsNull)
                                .instruction(&If(BlockType::Result(VALUE)));
                            uncached(&mut function);
                            function
                                .instruction(&Else)
                                .instruction(&LocalGet(method_local))
                                .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::CLOSURE)))
                                .instruction(&StructGet { struct_type_index: runtime_abi::CLOSURE, field_index: runtime_abi::CLOSURE_CODE })
                                .instruction(&StructGet { struct_type_index: runtime_abi::CODE, field_index: runtime_abi::CODE_ENTRY + k })
                                .instruction(&GlobalSet(entry_global));
                            cached_call(&mut function);
                            function.instruction(&End).instruction(&End);
                        }
                        Nominal::ObjectInvoke if arguments.len() - 2 < runtime_abi::MAX_FIXED as usize => {
                            // No argument array (#378): object-method-invoke-N.
                            for argument in arguments {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function.instruction(&Call(index(object_method_invoke_name(arguments.len() - 2))));
                        }
                        Nominal::ObjectInvoke => {
                            for argument in &arguments[..2] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            for argument in &arguments[2..] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function
                                .instruction(&ArrayNewFixed {
                                    array_type_index: runtime_abi::ARGS,
                                    array_size: arguments.len() as u32 - 2,
                                })
                                .instruction(&Call(index("object-method-invoke")));
                        }
                        Nominal::Class
                        | Nominal::BindingDefined
                        | Nominal::BindingGetter
                        | Nominal::DecimalContextNew
                        | Nominal::DecimalContextGetter
                        | Nominal::CoerceString
                        | Nominal::ConcatString
                        | Nominal::JoinStrings
                        | Nominal::ValueAdd
                        | Nominal::StringIndexOf
                        | Nominal::StringSlice
                        | Nominal::StringFromUnit
                        | Nominal::LanguageError
                        | Nominal::IsLanguageError
                        | Nominal::IsNativeObject
                        | Nominal::FixedArityFn
                        | Nominal::ApplySequenceOps
                        | Nominal::ListFunction
                        | Nominal::ApplyCallee
                        | Nominal::ApplyToSeq
                        | Nominal::ApplyFixed
                        | Nominal::IsTypeConstructor
                        | Nominal::ValueConstructor
                        | Nominal::TypeName
                        | Nominal::DisplayLeaf
                        | Nominal::SourceFunctionName
                        | Nominal::SameNominalType
                        | Nominal::NativeObjectFactory
                        | Nominal::NativeObjectDefaultPrototype
                        | Nominal::CurrentThis
                        | Nominal::ObjectCoerceString
                        | Nominal::NativeObjectGet
                        | Nominal::NativeObjectSet
                        | Nominal::NativeObjectStrictSet
                        | Nominal::ObjectSet
                        | Nominal::NamedGet
                        | Nominal::CallableGet
                        | Nominal::NamedSet
                        | Nominal::CoreNamedGet
                        | Nominal::CoreNamedSet
                        | Nominal::BuiltinClassGuard
                        | Nominal::BuiltinConstructGuard
                        | Nominal::Protocol
                        | Nominal::Dispatcher
                        | Nominal::ObjectDescriptor
                        | Nominal::MethodGet
                        | Nominal::Satisfies
                        | Nominal::NativeSatisfies => {
                            let named = matches!(operation, Nominal::NamedGet | Nominal::NamedSet);
                            // An unprivileged callable read (owner, name, keys,
                            // caller) runs the same guards on owner and name.
                            let guarded_callable = *operation == Nominal::CallableGet && arguments.len() == 4;
                            if guarded_callable {
                                let [owner, name, _, caller] = [0, 1, 2, 3].map(|i| LocalGet(arguments[i].0 as u32 + offset));
                                for instruction in [owner, name.clone(), Call(index("builtin-internal-get-guard")), name, caller,
                                    Call(index("user-field-get-guard"))] {
                                    function.instruction(&instruction);
                                }
                            }
                            for (position, argument) in arguments.iter().enumerate() {
                                if guarded_callable && (position == 0 || position == 3) {
                                    continue;
                                }
                                let local = LocalGet(argument.0 as u32 + offset);
                                // Unprivileged named access first rejects built-in
                                // fields, then user field mutability and type
                                // privacy: guard(owner, name[, caller]) leaves
                                // the owner. The caller operand is not passed on.
                                if named && position == 2 {
                                    function.instruction(&local).instruction(&Call(index(
                                        if *operation == Nominal::NamedSet { "user-field-set-guard" } else { "user-field-get-guard" })));
                                    function.instruction(&LocalGet(arguments[1].0 as u32 + offset));
                                    continue;
                                }
                                function.instruction(&local);
                                if named && position == 1 {
                                    function
                                        .instruction(&Call(index(if *operation == Nominal::NamedSet {
                                            "builtin-internal-set-guard"
                                        } else {
                                            "builtin-internal-get-guard"
                                        })))
                                        .instruction(&local);
                                }
                            }
                            function.instruction(&Call(index(match operation {
                                Nominal::Class => "class-value-new",
                                Nominal::BindingDefined => "binding-defined",
                                Nominal::BindingGetter => "binding-getter",
                                Nominal::DecimalContextNew => "decimal-context-new",
                                Nominal::DecimalContextGetter => "decimal-context-getter-install",
                                Nominal::CoerceString => "coerce-string",
                                Nominal::ObjectCoerceString => "object-coerce-string",
                                Nominal::ConcatString => "string-concat",
                                Nominal::JoinStrings => "string-join",
                                Nominal::ValueAdd => "value-add",
                                Nominal::StringIndexOf => "string-index-of",
                                Nominal::StringSlice => "string-slice",
                                Nominal::StringFromUnit => "string-from-unit",
                                Nominal::LanguageError => "language-error-new",
                                Nominal::IsLanguageError => "language-error-is",
                                Nominal::NativeObjectFactory => "native-object-factory-function",
                                Nominal::IsNativeObject => "native-object?",
                                Nominal::FixedArityFn => "fn-fixed-arity?",
                                Nominal::ApplySequenceOps => "apply-sequence-ops",
                                Nominal::ListFunction => "list-function",
                                Nominal::ApplyCallee => "apply-callee",
                                Nominal::ApplyToSeq => "apply-to-seq",
                                Nominal::ApplyFixed => "apply-fixed",
                                Nominal::IsTypeConstructor => "class-value-is",
                                Nominal::ValueConstructor => "value-constructor",
                                Nominal::TypeName => "class-value-name",
                                Nominal::DisplayLeaf => "value-display-text",
                                Nominal::SourceFunctionName => "closure-source-name",
                                Nominal::SameNominalType => "object-same-descriptor",
                                Nominal::NativeObjectDefaultPrototype => "native-object-default-prototype",
                                Nominal::CurrentThis => "current-this",
                                Nominal::NativeObjectGet => "native-object-property-get",
                                Nominal::NativeObjectSet => "native-object-property-set",
                                Nominal::NativeObjectStrictSet => "native-object-property-set-strict",
                                Nominal::ObjectSet => "object-method-set",
                                Nominal::NamedGet | Nominal::CoreNamedGet => "named-property-get",
                                Nominal::BuiltinClassGuard => "builtin-extension-guard",
                                Nominal::BuiltinConstructGuard => "builtin-construct-guard",
                            Nominal::CallableGet => "callable-property-get",
                                Nominal::NamedSet | Nominal::CoreNamedSet => "named-property-set",
                                Nominal::Protocol => "protocol-value-new",
                                Nominal::Dispatcher => "protocol-dispatcher-new",
                                Nominal::ObjectDescriptor => "object-descriptor",
                                Nominal::MethodGet => "protocol-class-method-get",
                                Nominal::NativeSatisfies => "protocol-native-satisfies",
                                _ => "protocol-marker-satisfies",
                            })));
                        }
                    }
                    if matches!(locals.of(inst.result), unboxed::Repr::Flag | unboxed::Repr::Long | unboxed::Repr::Double) {
                        // Its i32 truth value (#387), or a parameter cast's
                        // raw long or double (#385); uses that need a value
                        // take it boxed.
                        function.instruction(&LocalSet(locals.raw(inst.result)));
                    } else {
                        if operation.result() == Type::Bool {
                            for instruction in runtime_abi::boolean_in(sentinels) {
                                function.instruction(&instruction);
                            }
                        }
                        function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                    }
                }
                Operation::Arithmetic {
                    operator,
                    arguments,
                    coercing,
                    primitive_overload,
                } => {
                    let name = arithmetic_name(*operator, arguments, *coercing, *primitive_overload, ir);
                    // Two i31 longs add or subtract inline (#361) while the
                    // result is an i31; the runtime boxes a larger one.
                    let inline = match name {
                        // value-add and value-subtract (JS convention) treat
                        // two numbers as number-add and number-subtract do.
                        "number-add" | "checked-add" | "value-add" => Some(I32Add),
                        "number-subtract" | "checked-subtract" | "checked-subtract-primitive" | "value-subtract" => Some(I32Sub),
                        _ => None,
                    }
                    .filter(|_| arguments.len() == 2);
                    // Two i31 longs multiply inline too (#443): their i64
                    // product can't overflow, and is an i31 when it fits or
                    // a boxed long, as the runtime's product of two longs.
                    let multiply = matches!(name, "number-multiply" | "checked-multiply" | "value-multiply")
                        && arguments.len() == 2;
                    let fast = inline.is_some() || multiply;
                    if multiply {
                        let [a, b] = [arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset];
                        let [_, _, r] = long_locals;
                        let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
                        function.instruction(&Block(BlockType::Empty));
                        i31_operands(&mut function, a, b);
                        function
                            .instruction(&If(BlockType::Empty))
                            .instruction(&LocalGet(a))
                            .instruction(&RefCastNonNull(i31))
                            .instruction(&I31GetS)
                            .instruction(&I64ExtendI32S)
                            .instruction(&LocalGet(b))
                            .instruction(&RefCastNonNull(i31))
                            .instruction(&I31GetS)
                            .instruction(&I64ExtendI32S)
                            .instruction(&I64Mul)
                            .instruction(&LocalTee(r))
                            .instruction(&I64Const(1 << 30))
                            .instruction(&I64Add)
                            .instruction(&I64Const(1 << 31))
                            .instruction(&I64LtU)
                            .instruction(&If(BlockType::Result(VALUE)))
                            .instruction(&LocalGet(r))
                            .instruction(&I32WrapI64)
                            .instruction(&RefI31)
                            .instruction(&Else)
                            .instruction(&LocalGet(r))
                            .instruction(&StructNew(runtime_abi::LONG))
                            .instruction(&End)
                            .instruction(&LocalSet(inst.result.0 as u32 + offset))
                            .instruction(&Br(1))
                            .instruction(&End);
                    }
                    // block: the fast path's result skips the runtime call.
                    if let Some(op) = inline {
                        let [a, b] = [arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset];
                        function.instruction(&Block(BlockType::Empty));
                        i31_operands(&mut function, a, b);
                        function.instruction(&If(BlockType::Empty));
                        i31_values(&mut function, a, b);
                        // An i32 sum of two i31 values is an i31 when adding
                        // 2^30 leaves it nonnegative.
                        function
                            .instruction(&op)
                            .instruction(&LocalTee(field_index_local))
                            .instruction(&I32Const(0x4000_0000))
                            .instruction(&I32Add)
                            .instruction(&I32Const(0))
                            .instruction(&I32GeS)
                            .instruction(&If(BlockType::Empty))
                            .instruction(&LocalGet(field_index_local))
                            .instruction(&RefI31)
                            .instruction(&LocalSet(inst.result.0 as u32 + offset))
                            .instruction(&Br(2))
                            .instruction(&End)
                            .instruction(&End);
                        // In Clojure arithmetic (not core's JS-convention
                        // value-*), operands that are each an i31 or a boxed
                        // long (#385) add or subtract as i64s, without the
                        // runtime's dispatch; overflow, or any other operand,
                        // takes the call, which throws or converts as it does.
                        let [x, y, r] = long_locals;
                        if !name.starts_with("value-") {
                        function.instruction(&Block(BlockType::Empty));
                        for (operand, raw) in [(a, x), (b, y)] {
                            long_operand(&mut function, operand, raw);
                        }
                        let overflow: &[Instruction] = if matches!(op, I32Add) {
                            &[LocalGet(x), LocalGet(y), I64Add, LocalTee(r), LocalGet(x), I64Xor,
                              LocalGet(y), LocalGet(r), I64Xor, I64And, I64Const(0), I64LtS]
                        } else {
                            &[LocalGet(x), LocalGet(y), I64Sub, LocalTee(r), LocalGet(x), LocalGet(y), I64Xor,
                              LocalGet(x), LocalGet(r), I64Xor, I64And, I64Const(0), I64LtS]
                        };
                        for instruction in overflow {
                            function.instruction(instruction);
                        }
                        function
                            .instruction(&BrIf(0))
                            .instruction(&LocalGet(r))
                            .instruction(&I64Const(1 << 30))
                            .instruction(&I64Add)
                            .instruction(&I64Const(1 << 31))
                            .instruction(&I64LtU)
                            .instruction(&If(BlockType::Result(VALUE)))
                            .instruction(&LocalGet(r))
                            .instruction(&I32WrapI64)
                            .instruction(&RefI31)
                            .instruction(&Else)
                            .instruction(&LocalGet(r))
                            .instruction(&StructNew(runtime_abi::LONG))
                            .instruction(&End)
                            .instruction(&LocalSet(inst.result.0 as u32 + offset))
                            .instruction(&Br(1))
                            .instruction(&End);
                        }
                    }
                    for argument in arguments {
                        function.instruction(&LocalGet(argument.0 as u32 + offset));
                    }
                    function
                        .instruction(&Call(index(name)))
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                    if fast {
                        function.instruction(&End);
                    }
                }
            }
        }
        if let Some(plan) = continuation {
            if let ir::r#async::Exit::Suspend { future, resume, spill, .. } = &plan.states[state_id].exit {
                r#async::suspend(&mut function, *future, *resume, spill, &slots, ir.values.len(), &index);
                function.instruction(&End);
                continue;
            }
        }
        let target_state = |target: usize| continuation.map_or(target, |plan| plan.block_entries[target]);
        match &block.terminator {
            Terminator::Return(_) if raw_result.is_some() => {},
            Terminator::Throw(value) | Terminator::Return(value) => locals.materialize(&mut code, &[*value], &mut boxed),
            Terminator::Branch { condition, .. } if locals.of(*condition) != unboxed::Repr::Flag => {
                locals.materialize(&mut code, &[*condition], &mut boxed);
            }
            Terminator::Jump { target, arguments } => {
                let boxed_arguments = ir.blocks[*target].parameters.iter().zip(arguments)
                    .filter(|(parameter, _)| locals.of(**parameter) == unboxed::Repr::Boxed)
                    .map(|(_, argument)| *argument).collect::<Vec<_>>();
                locals.materialize(&mut code, &boxed_arguments, &mut boxed);
            }
            Terminator::Branch { .. } => {}
        }
        for instruction in code.drain(..) { function.instruction(&instruction); }
        for (_, array, kind, local) in view_inits.iter().filter(|(entry_block, ..)| *entry_block == block_id) {
            let storage = if *kind == 6 { runtime_abi::ARGS } else { runtime_abi::PACKED_STORAGE + *kind as u32 };
            function
                .instruction(&Block(BlockType::Empty))
                .instruction(&RefNull(HeapType::Concrete(storage)))
                .instruction(&LocalSet(*local))
                .instruction(&LocalGet(*array + offset))
                .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::PACKED)))
                .instruction(&I32Eqz)
                .instruction(&BrIf(0))
                .instruction(&LocalGet(*array + offset))
                .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::PACKED)))
                .instruction(&StructGet { struct_type_index: runtime_abi::PACKED, field_index: 0 })
                // The method local is free between instructions.
                .instruction(&LocalTee(method_local))
                .instruction(&RefTestNonNull(HeapType::Concrete(storage)))
                .instruction(&I32Eqz)
                .instruction(&BrIf(0))
                .instruction(&LocalGet(method_local))
                .instruction(&RefCastNonNull(HeapType::Concrete(storage)))
                .instruction(&LocalSet(*local))
                .instruction(&End);
        }
        match &block.terminator {
            Terminator::Throw(value) => {
                if let Some(plan) = continuation {
                    r#async::event(&mut function, 2, value.0 as u32 + offset, &slots, plan.states.len());
                    function.instruction(&Br(1));
                } else {
                    function.instruction(&LocalGet(value.0 as u32 + offset)).instruction(&Throw(0));
                }
            }
            Terminator::Return(value) => {
                if continuation.is_some() {
                    r#async::complete(&mut function, 1, *value, &slots, &index);
                } else {
                    function.instruction(&LocalGet(if raw_result.is_some() { locals.raw(*value) }
                        else { value.0 as u32 + offset })).instruction(&Return);
                }
            }
            Terminator::Jump { target, arguments } => {
                // Push every replacement value before any parameter assignment.
                for (parameter, argument) in ir.blocks[*target].parameters.iter().zip(arguments) {
                    if locals.of(*parameter).maybe() {
                        function.instruction(&LocalGet(locals.raw(*argument)))
                            .instruction(&LocalGet(argument.0 as u32 + offset));
                        if locals.of(*argument).maybe() { function.instruction(&LocalGet(locals.valid(*argument))); }
                        else { function.instruction(&I32Const(1)); }
                        continue;
                    }
                    function.instruction(&LocalGet(match locals.of(*parameter) {
                        unboxed::Repr::Boxed => argument.0 as u32 + offset,
                        _ => locals.raw(*argument),
                    }));
                }
                for parameter in ir.blocks[*target].parameters.iter().rev() {
                    if locals.of(*parameter).maybe() {
                        function.instruction(&LocalSet(locals.valid(*parameter)))
                            .instruction(&LocalSet(parameter.0 as u32 + offset))
                            .instruction(&LocalSet(locals.raw(*parameter)));
                        continue;
                    }
                    function.instruction(&LocalSet(match locals.of(*parameter) {
                        unboxed::Repr::Boxed => parameter.0 as u32 + offset,
                        _ => locals.raw(*parameter),
                    }));
                }
                if !dispatch {
                    // The plan's steps branch or continue in place.
                } else if let Some(plan) = continuation.filter(|plan| plan.backedges.contains(&(block_id, *target))) {
                    r#async::yield_turn(&mut function, plan.block_entries[*target], &slots, ir.values.len(), &index);
                } else {
                    function.instruction(&I32Const(target_state(*target) as i32))
                        .instruction(&LocalSet(pc));
                    // Forward blocks occur later in this dispatch pass. Continue
                    // directly instead of rescanning every earlier block guard.
                    // Backedges and resumable states retain the loop transfer.
                    if continuation.is_some() || *target <= block_id {
                        function.instruction(&Br(1));
                    }
                }
            }
            Terminator::Branch {
                condition,
                consequent,
                alternative,
            } => {
                // Nil, false and internal undefined are falsey, including for
                // boxed numeric zero and empty UTF-16 strings. A Bool is one
                // of the two boolean sentinels, so only false is (#356). A raw
                // truth value is falsey when zero (#337).
                if locals.of(*condition) == unboxed::Repr::Flag {
                    function.instruction(&LocalGet(locals.raw(*condition))).instruction(&I32Eqz);
                } else {
                function
                    .instruction(&LocalGet(condition.0 as u32 + offset))
                    .instruction(&runtime_abi::sentinel_in(runtime_abi::FALSE, sentinels))
                    .instruction(&RefEq);
                if ir.values[condition.0].ty != Type::Bool {
                    function
                        .instruction(&LocalGet(condition.0 as u32 + offset))
                        .instruction(&RefIsNull)
                        .instruction(&I32Or);
                    function
                        .instruction(&LocalGet(condition.0 as u32 + offset))
                        .instruction(&runtime_abi::sentinel_in(runtime_abi::UNDEFINED, sentinels))
                        .instruction(&RefEq)
                        .instruction(&I32Or);
                }
                }
                if !dispatch {
                    // The plan's steps give the alternative, then the consequent.
                    function.instruction(&If(BlockType::Empty));
                    continue;
                }
                function
                    .instruction(&If(BlockType::Result(ValType::I32)))
                    .instruction(&I32Const(target_state(*alternative) as i32))
                    .instruction(&Else)
                    .instruction(&I32Const(target_state(*consequent) as i32))
                    .instruction(&End)
                    .instruction(&LocalSet(pc));
                if let Some(plan) = continuation {
                    for target in BTreeSet::from([*consequent, *alternative]) {
                        if plan.backedges.contains(&(block_id, target)) {
                            function.instruction(&LocalGet(pc)).instruction(&I32Const(plan.block_entries[target] as i32))
                                .instruction(&I32Eq).instruction(&If(BlockType::Empty));
                            r#async::yield_turn(&mut function, plan.block_entries[target], &slots, ir.values.len(), &index);
                            function.instruction(&End);
                        }
                    }
                }
                if continuation.is_some() || *consequent <= block_id || *alternative <= block_id {
                    function.instruction(&Br(1));
                }
            }
        }
        if dispatch { function.instruction(&End); }
    }
    if let Some(plan) = continuation {
        function.instruction(&LocalGet(pc)).instruction(&I32Const(plan.states.len() as i32))
            .instruction(&I32Eq).instruction(&If(BlockType::Empty));
        r#async::unwind(&mut function, &slots, ir.values.len(), &index);
        function.instruction(&Br(1)).instruction(&End);
    }
    if dispatch { function.instruction(&Unreachable).instruction(&End); }
    if let Some(plan) = continuation {
        // Language exceptions thrown by any compiled operation become an
        // explicit unwind event. Wasm traps remain runtime/host recovery work.
        function.instruction(&Unreachable).instruction(&End).instruction(&Unreachable)
            .instruction(&End).instruction(&LocalSet(slots.outcome));
        r#async::event(&mut function, 2, slots.outcome, &slots, plan.states.len());
        function.instruction(&Br(0)).instruction(&End);
    }
    function.instruction(&Unreachable).instruction(&End);

    Ok(function)
}
