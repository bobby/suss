//! Emit only verified IR. Operands are local value IDs, never source expressions.
mod r#async;
mod structured;
mod unboxed;
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

/// Globals per cached named-read site: its immutable `NAMED_SITE` struct (#378).
const FIELD_SITE_GLOBALS: u32 = 1;

fn arithmetic_name(
    operator: Arithmetic,
    arguments: &[ir::ValueId],
    coercing: bool,
    function: &IrFunction,
) -> &'static str {
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
// Literal units and indices are compiler-checked, unlike public mutable string
// storage calls. Keep small literals on a bounded operand stack; larger values
// come from a passive data segment. Each evaluation allocates its own array.
fn emit_utf16_literal(
    function: &mut Function,
    data: &mut LiteralData,
    units: &[u16],
    local: u32,
    span: &std::ops::Range<usize>,
) -> Result<(), Diagnostic> {
    use Instruction::*;
    let length = i32::try_from(units.len()).map_err(|_| Diagnostic {
        span: span.clone(),
        message: "UTF16 literal exceeds signed runtime length".into(),
    })?;
    if units.len() <= 64 {
        for unit in units {
            function.instruction(&I32Const(i32::from(*unit)));
        }
        function.instruction(&ArrayNewFixed {
            array_type_index: runtime_abi::STRING,
            array_size: length as u32,
        }).instruction(&LocalSet(local));
    } else {
        function.instruction(&I32Const(0))
            .instruction(&I32Const(length))
            .instruction(&ArrayNewData {
                array_type_index: runtime_abi::STRING,
                array_data_index: data.segment(units),
            })
            .instruction(&LocalSet(local));
    }
    Ok(())
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
    let mut throws = !futures.is_empty();
    if !futures.is_empty() { names.extend(r#async::IMPORTS.iter().copied()); }
    if futures.iter().any(|body| !body.continuation.backedges.is_empty()) { names.insert("async-task-yield"); }
    let mut globals = BTreeSet::new();
    for function in all_functions {
        for block in &function.blocks {
            throws |= matches!(&block.terminator, Terminator::Throw(_));
            for inst in &block.instructions {
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
                    Operation::Literal(Literal::String(_)) => {}
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
                        names.extend(runtime_abi::ARITY_ERROR_EXPORTS);
                        if let Some(factory) = &body.rest_class {
                            globals.insert(factory.clone());
                            names.extend(["binding-get", "invoke", "source-array-new", "constructor-descriptor", "source-constructor-new"]);
                        }
                    }
                    Operation::MakeClosure { .. } => {
                        names.extend(["closure-source-name-initialize", "arity-throw"]);
                        names.extend(runtime_abi::ARITY_ERROR_EXPORTS);
                    }
                    Operation::Call { operands } => match runtime_abi::CALL_EXPORTS.get(operands.len() - 1) {
                        Some(name) => { names.insert(name); }
                        None => { names.insert("call-spread"); }
                    },
                    Operation::Bitwise { operation, .. } => {
                        names.insert(operation.export());
                    }
                    Operation::Comparison { operation, .. } => {
                        names.insert(operation.export());
                    }
                    Operation::Array { operation: ArrayOperation::Get, arguments } if arguments.len() == 2 => {
                        names.insert("source-array-get-2");
                        names.insert("packed-array-get-2");
                    }
                    Operation::Array { operation: ArrayOperation::Set, arguments } if arguments.len() == 3 => {
                        names.insert("source-array-set-3");
                        names.insert("packed-array-set-3");
                    }
                    Operation::Array { operation, .. } => {
                        names.insert(operation.export());
                    }
                    Operation::Nominal { operation, arguments } => match operation {
                        Nominal::IsNumber | Nominal::IsString | Nominal::Array => {}
                        Nominal::IsKeyword => { names.insert("keyword-test"); }
                        Nominal::DirectLinkDefine(_) | Nominal::DirectLinkRead(_) | Nominal::DirectLinkSet(_) => {}
                        Nominal::KeywordIntern => { names.insert("keyword-intern"); }
                        Nominal::CoerceString => { names.insert("coerce-string"); }
                        Nominal::ObjectCoerceString => { names.insert("object-coerce-string"); }
                        Nominal::ConcatString => { names.insert("string-concat"); }
                        Nominal::ValueAdd => { names.insert("value-add"); }
                        Nominal::IsBoolean => {}
                        Nominal::Identical => { names.insert("identical-value"); }
                        Nominal::AllSmallIntegers => {}
                        Nominal::NumberHash => { names.insert("number-hash"); }
                        Nominal::StringIndexOf => { names.insert("string-index-of"); }
                        Nominal::StringSlice => { names.insert("string-slice"); }
                        Nominal::StringFromUnit => { names.insert("string-from-unit"); }
                        Nominal::BindingDefined => { names.insert("binding-defined"); }
                        Nominal::LanguageError => { names.insert("language-error-new"); }
                        Nominal::IsLanguageError => { names.insert("language-error-is"); }
                        Nominal::NativeObjectFactory => { names.insert("native-object-factory-function"); }
                        Nominal::IsNativeObject => { names.insert("native-object?"); }
                        Nominal::IsTypeConstructor => { names.insert("class-value-is"); }
                        Nominal::ValueConstructor => { names.insert("value-constructor"); }
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
                            names.insert("protocol-method-get");
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
                    } => {
                        names.insert(arithmetic_name(*operator, arguments, *coercing, function));
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
    // a site's last descriptor and its field's index, in two globals.
    let mut field_site_list = vec![];
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
            if let Operation::Nominal { operation: Nominal::CoreNamedGet, arguments } = &inst.operation
                && literals.get(&arguments[1]).is_some_and(|units| plain_field_name(units))
            {
                let site = (function as *const IrFunction as usize, inst.result.0 as u32);
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
    // Protocol method lookups get an inline cache (#355): the last
    // descriptor, its method and the protocol epoch it was found at.
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

    let imported_slots = slots.iter().copied().filter(|slot| !owned_slots.contains(slot)).collect::<Vec<_>>();
    let imported_definitions = called.iter().copied().filter(|definition| !defined_definitions.contains_key(definition)).collect::<Vec<_>>();
    for definition in &imported_definitions {
        for k in 0..=runtime_abi::MAX_FIXED {
            names.insert(Box::leak(format!("link.{definition}.{k}").into_boxed_str()));
        }
    }
    let names = names.into_iter().collect::<Vec<_>>();
    let globals = globals.into_iter().collect::<Vec<_>>();
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    let type_count = runtime_abi::TYPE_COUNT;
    for (i, name) in names.iter().enumerate() {
        let (params, results) = match *name {
            "future-pending-new" | "dynamic-fork" | "dynamic-save" => (vec![], vec![VALUE]),
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
            "binding-defined" | "language-error-is" | "native-object?" | "class-value-is" => (vec![VALUE], vec![ValType::I32]),
            "identical-value" => (vec![VALUE, VALUE], vec![VALUE]),
            "object-instance" | "object-same-descriptor" | "protocol-marker-satisfies" | "protocol-native-satisfies" => {
                (vec![VALUE, VALUE], vec![ValType::I32])
            }
            "primitive-int" | "primitive-bit-not" | "int32-int" => (vec![VALUE], vec![VALUE]),
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
            "protocol-marker-set" | "protocol-method-get" => (vec![VALUE, VALUE], vec![VALUE]),
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
            "source-construct-from-plan" => (
                vec![VALUE, ValType::Ref(RefType {
                    nullable: false,
                    heap_type: HeapType::Concrete(runtime_abi::ARGS),
                })],
                vec![VALUE],
            ),
            "number-hash"
            | "owned-descriptor-prototype"
            | "source-array-new"
            | "source-array-make"
            | "source-array-make-literal"
            | "source-array-length-args"
            | "source-array-get-indices"
            | "source-array-set-indices"
            | "source-array-set-or-property" => (vec![VALUE], vec![VALUE]),
            "predicate-number" | "predicate-string" | "arity-error" | "native-object-factory-function" | "native-object-default-prototype" | "current-this" => (vec![], vec![VALUE]),
            "number-box" => (vec![ValType::F64], vec![VALUE]),
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
            "binding-get" | "binding-bound" | "number-negate" | "value-negate"
            | "checked-negate" | "checked-identity-add" | "checked-identity-multiply"
            | "primitive-f64-coerce" | "primitive-f64-word0" | "primitive-f64-word4"
            | "primitive-f64-floor" | "primitive-f64-ceil" | "primitive-f64-finite" | "primitive-f64-safe-integer" | "primitive-f64-time-clip" | "identity-uid" | "coerce-string" | "object-coerce-string" | "language-error-new" | "string-from-unit"
            | "number-abs" | "number-unchecked-negate" | "number-long?" | "number-double?" | "number-to-long" | "number-hash" | "packed-array-test" => {
                (vec![VALUE], vec![VALUE])
            }
            "binding-set" => (vec![VALUE, VALUE], vec![]),
            "number-parse-long" => (vec![VALUE; 4], vec![VALUE]),
            "packed-array-copy" => (vec![VALUE; 5], vec![VALUE]),
            "number-range-count" | "packed-array-store" | "source-array-set-3" | "packed-array-set-3" => (vec![VALUE; 3], vec![VALUE]),
            "string-slice" | "callable-property-get" | "named-property-set" | "user-field-get-guard" | "user-field-set-guard" | "object-method-set" | "object-method-invoke" | "native-object-property-set" | "native-object-property-set-strict" => {
                (vec![VALUE, VALUE, VALUE], vec![VALUE])
            }
            "string-new" => (vec![ValType::I32], vec![VALUE]),
            "instance-site-test" => (
                vec![ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::NAMED_SITE) }), VALUE, VALUE],
                vec![ValType::I32],
            ),
            "named-site-get" => (
                vec![ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::NAMED_SITE) }), VALUE, VALUE],
                vec![VALUE],
            ),
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
        types.ty().function(params, results);
        // An arity error is an entry, of the shared group's `$callk` type.
        let ty = match runtime_abi::ARITY_ERROR_EXPORTS.iter().position(|error| error == name) {
            Some(k) => runtime_abi::CALL + k as u32,
            None => type_count + i as u32,
        };
        if let Some((link, k)) = link_entry(name) {
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
    let epoch_used = !method_site_list.is_empty() || !field_site_list.is_empty() || !instance_site_list.is_empty();
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
    let layout = Layout::new(
        &bodies, &singles, &dispatchers, futures.len(), &names,
        eval_type + 1 + u32::from(throws), globals.len() as u32 + 3 + imported_link_globals,
    ).map_err(|message| fail(message))?;
    for definition in &layout.definitions {
        let mut fields = runtime_abi::closure_fields();
        fields.extend((0..definition.captures).map(|_| FieldType {
            element_type: StorageType::Val(VALUE),
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
        .map(|(index, site)| (*site, first_method_site + 3 * index as u32))
        .collect::<std::collections::HashMap<_, _>>();
    let first_instance_site = first_method_site + 3 * method_site_list.len() as u32;
    let instance_sites = instance_site_list.iter().enumerate()
        .map(|(index, site)| (*site, first_instance_site + index as u32))
        .collect::<std::collections::HashMap<_, _>>();
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
        defined_code.push((*definition, layout.definitions[index].global));
    }
    let mut functions = FunctionSection::new();
    functions.function(eval_type);
    let mut exports = ExportSection::new();
    exports.export("eval", ExportKind::Func, names.len() as u32);
    let mut code = CodeSection::new();
    let mut data = LiteralData::default();
    let context = Context { names: &names, globals: &globals, bodies: &bodies,
        dispatchers: &dispatchers, futures: &futures, layout: &layout, links: &links, field_sites: &field_sites, length_sites: &length_sites,
        method_sites: &method_sites, instance_sites: &instance_sites, epoch_global };
    code.function(&emit_function(ir, Entry::Eval, &context, None, &mut data)?);
    for (index, body) in bodies.iter().enumerate() {
        functions.function(layout.body_types[index]);
        code.function(&emit_function(&body.function, Entry::Method(index), &context, None, &mut data)?);
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
        }
    }
    // Each definition's code struct: an immutable global with a constant
    // initializer, after the imported cells and sentinels (ADR-0004).
    let mut code_globals = GlobalSection::new();
    for definition in &layout.definitions {
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
            Instruction::StructNew(runtime_abi::CODE),
        ]);
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
    for (definition, global) in &defined_code {
        exports.export(&format!("link.{definition}.code"), ExportKind::Global, *global);
        for (k, entry) in links.entries[definition].iter().enumerate() {
            exports.export(&format!("link.{definition}.{k}"), ExportKind::Func, *entry);
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
    if !layout.definitions.is_empty() || !field_site_list.is_empty() || !method_site_list.is_empty() || !instance_site_list.is_empty() {
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
    /// Cached `instance?` sites: their first global (#378).
    instance_sites: &'a std::collections::HashMap<(usize, u32), u32>,
    /// Cached protocol method lookup sites: their first global; the epoch.
    method_sites: &'a std::collections::HashMap<(usize, u32), u32>,
    epoch_global: u32,
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
    self_capture: bool,
    spread: u32,
    entries: [u32; runtime_abi::MAX_FIXED as usize + 1],
    name: Option<Vec<u16>>,
}
/// Functions after the futures: single-method spread entries and variadic adapters.
enum Extra {
    Spread(usize),
    Adapter { definition: usize, arity: usize },
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
                global: first_global + definition as u32, captures, self_capture,
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
/// wrapper builds it over a fresh source array; Internal Args never becomes the
/// language rest sequence.
fn emit_rest(function: &mut Function, definition: &Definition, surplus: u32, context: &Context) {
    use Instruction::*;
    let factory = context.globals
        .binary_search(definition.rest_class.as_ref().expect("verified rest class"))
        .expect("collected rest class") as u32;
    let import = |name: &str| context.names.iter().position(|candidate| *candidate == name).expect("collected import") as u32;
    function
        .instruction(&GlobalGet(factory))
        .instruction(&Call(import("binding-get")))
        .instruction(&Call(import("constructor-descriptor")))
        .instruction(&Call(import("source-constructor-new")))
        .instruction(&LocalGet(surplus))
        .instruction(&Call(import("source-array-new")))
        .instruction(&I32Const(0))
        .instruction(&RefI31)
        .instruction(&runtime_abi::sentinel_in(runtime_abi::NIL, context.globals.len() as u32))
        .instruction(&ArrayNewFixed { array_type_index: runtime_abi::ARGS, array_size: 3 })
        .instruction(&Call(import("invoke")));
}
/// The spread entry, `(closure, args, rest)`: calls the method that takes
/// `args`' length, as the arity dispatcher did, or throws the arity error.
fn emit_spread(definition: &Definition, context: &Context) -> Function {
    use Instruction::*;
    let names = context.names;
    let method = |body: usize| (names.len() + 1 + body) as u32;
    // Locals: 3 the argument count, 4 the surplus array.
    let mut function = Function::new([(1, ValType::I32), (1, VALUE)]);
    // An unrealized rest comes only from `apply`, which realizes it first.
    function
        .instruction(&LocalGet(2)).instruction(&RefIsNull).instruction(&I32Eqz)
        .instruction(&If(BlockType::Empty)).instruction(&Unreachable).instruction(&End)
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
    function.instruction(&LocalGet(0)).instruction(&LocalGet(3)).instruction(&Call(error as u32)).instruction(&End);
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
    let Context { names, globals, bodies, dispatchers, futures, layout, links, field_sites, length_sites, method_sites, instance_sites, epoch_global } = *context;
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
        Entry::Method(body) => 1 + u32::try_from(bodies[body].arity).map_err(|_| fail("Too many parameters"))?,
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
    let locals = unboxed::Locals::new(unboxed::infer(ir, continuation.is_some()), pc + 1, offset, sentinels);
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
    let mut function = Function::new(declarations);
    use Instruction::*;
    if continuation.is_none() {
    if let Entry::Method(body) = entry {
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
            function.instruction(&LocalSet(value.0 as u32 + offset));
        }
    }
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
    if let Entry::Method(body) = entry {
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
        for inst in instructions {
            if unboxed::inline(&locals, inst) {
                match &inst.operation {
                    Operation::Literal(literal) => locals.literal(&mut code, literal, inst.result),
                    Operation::Arithmetic { operator, arguments, coercing } => locals.arithmetic(
                        &mut code, *operator, arguments, inst.result,
                        index(arithmetic_name(*operator, arguments, *coercing, ir)),
                    ),
                    Operation::Comparison { operation, arguments } => locals.comparison(
                        &mut code, *operation, arguments, inst.result, index(operation.export()),
                    ),
                    _ => unreachable!("inline numeric instruction"),
                }
                for instruction in code.drain(..) { function.instruction(&instruction); }
                continue;
            }
            locals.materialize(&mut code, ir::operands(&inst.operation), &mut boxed);
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
                Operation::Literal(Literal::String(units)) => {
                    emit_utf16_literal(&mut function, data, units, inst.result.0 as u32 + offset, &ir.span)?;
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
                        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::BINDING)))
                        .instruction(&StructGet { struct_type_index: runtime_abi::BINDING, field_index: 0 })
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::GlobalRead(global) => {
                    let index_global =
                        globals.binary_search(global).expect("collected global") as u32;
                    let flags = [
                        GlobalGet(index_global),
                        RefCastNonNull(HeapType::Concrete(runtime_abi::BINDING)),
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
                        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::BINDING)))
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
                        .instruction(&RefCastNonNull(HeapType::Concrete(runtime_abi::BINDING)))
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
                Operation::Call { operands } => {
                    let tail = continuation.is_none() && block.instructions.last().is_some_and(|last| std::ptr::eq(last, inst))
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
                        // this many arguments; anything else goes out of line
                        // to the runtime's `call-k`.
                        let k = arguments.len() as u32;
                        function
                            .instruction(&Block(BlockType::Result(VALUE)))
                            .instruction(&Block(BlockType::Result(VALUE)))
                            .instruction(&LocalGet(callee_local))
                            .instruction(&BrOnCastFail {
                                relative_depth: 0,
                                from_ref_type: RefType::EQREF,
                                to_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::CLOSURE) },
                            })
                            .instruction(&LocalTee(callee))
                            .instruction(&RefAsNonNull);
                        for value in arguments {
                            function.instruction(&LocalGet(value.0 as u32 + offset));
                        }
                        function
                            .instruction(&LocalGet(callee))
                            .instruction(&StructGet { struct_type_index: runtime_abi::CLOSURE, field_index: runtime_abi::CLOSURE_CODE })
                            .instruction(&StructGet { struct_type_index: runtime_abi::CODE, field_index: runtime_abi::CODE_ENTRY + k })
                            .instruction(&if tail { ReturnCallRef(runtime_abi::CALL + k) } else { CallRef(runtime_abi::CALL + k) })
                            .instruction(&Br(1))
                            .instruction(&End);
                        // The callee, left by the failed cast, then its arguments.
                        for value in arguments {
                            function.instruction(&LocalGet(value.0 as u32 + offset));
                        }
                        let call = index(runtime_abi::CALL_EXPORTS[k as usize]);
                        function
                            .instruction(&if tail { ReturnCall(call) } else { Call(call) })
                            .instruction(&End)
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
                    emit_closure(&mut function, definition, captures.iter().map(|value| value.0 as u32 + offset));
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                    if let Some(name) = &body.display_name {
                        emit_utf16_literal(&mut function, data, name, scratch, &ir.span)?;
                        function.instruction(&LocalGet(inst.result.0 as u32 + offset))
                            .instruction(&LocalGet(scratch))
                            .instruction(&Call(index("closure-source-name-initialize")));
                    }
                }
                Operation::MakeGeneralClosure { body, captures } => {
                    let definition = layout.general(body);
                    emit_closure(&mut function, definition, captures.iter().map(|value| value.0 as u32 + offset));
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                    if let Some(name) = &body.display_name {
                        emit_utf16_literal(&mut function, data, name, scratch, &ir.span)?;
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
                    for instruction in runtime_abi::boolean_in(sentinels) {
                        function.instruction(&instruction);
                    }
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
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
                }
                Operation::Array {
                    operation: ArrayOperation::Get,
                    arguments,
                } if arguments.len() == 2 => {
                    // A packed array reads its storage (#389); anything
                    // else, the source array's dense prefix (#361).
                    let array = arguments[0].0 as u32 + offset;
                    function
                        .instruction(&LocalGet(array))
                        .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::PACKED)))
                        .instruction(&If(BlockType::Result(VALUE)))
                        .instruction(&LocalGet(array))
                        .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                        .instruction(&Call(index("packed-array-get-2")))
                        .instruction(&Else)
                        .instruction(&LocalGet(array))
                        .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                        .instruction(&Call(index("source-array-get-2")))
                        .instruction(&End)
                        .instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Array {
                    operation: ArrayOperation::Set,
                    arguments,
                } if arguments.len() == 3 => {
                    // As for one-index reads (#389).
                    function
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
                        .instruction(&Call(index("source-array-set-3")))
                        .instruction(&End)
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
                            function
                                .instruction(&LocalGet(a))
                                .instruction(&LocalGet(b))
                                .instruction(&RefEq)
                                .instruction(&LocalGet(a))
                                .instruction(&RefTestNonNull(HeapType::Concrete(runtime_abi::NUMBER)))
                                .instruction(&I32Eqz)
                                .instruction(&I32And)
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&I32Const(1))
                                .instruction(&Else);
                            for local in [a, b] {
                                function
                                    .instruction(&LocalGet(local))
                                    .instruction(&RefIsNull)
                                    .instruction(&LocalGet(local))
                                    .instruction(&RefTestNonNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 }))
                                    .instruction(&I32Or);
                            }
                            // Only two boxed longs, doubles or strings compare by
                            // value; distinct references of any other type aren't
                            // identical (#361).
                            function
                                .instruction(&I32Or)
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&I32Const(0))
                                .instruction(&Else);
                            for (index, ty) in [runtime_abi::NUMBER, runtime_abi::LONG, runtime_abi::STRING].into_iter().enumerate() {
                                function.instruction(&LocalGet(a)).instruction(&RefTestNonNull(HeapType::Concrete(ty)));
                                if index > 0 {
                                    function.instruction(&I32Or);
                                }
                            }
                            function
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&LocalGet(a))
                                .instruction(&LocalGet(b))
                                .instruction(&Call(index("identical-value")))
                                .instruction(&runtime_abi::sentinel_in(runtime_abi::TRUE, sentinels))
                                .instruction(&RefEq)
                                .instruction(&Else)
                                .instruction(&I32Const(0))
                                .instruction(&End)
                                .instruction(&End)
                                .instruction(&End);
                        }
                        // The runtime predicates' own tests, inline: never a mutable
                        // public cell, and no closure call on core's hot paths (#286).
                        // Arguments are already evaluated once in source order.
                        Nominal::IsNumber => {
                            for instruction in runtime_abi::number_test(arguments[0].0 as u32 + offset) {
                                function.instruction(&instruction);
                            }
                        }
                        Nominal::Satisfies if method_sites.contains_key(&(ir as *const IrFunction as usize, inst.result.0 as u32)) => {
                            // Inline cache (#361): a user object's marker test
                            // depends only on its descriptor and the protocol
                            // epoch, so a site keeps its last descriptor and
                            // answer. Other values take the runtime's test.
                            let cache = method_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            let (marker, object) = (arguments[0].0 as u32 + offset, arguments[1].0 as u32 + offset);
                            let descriptor = |function: &mut Function| {
                                function
                                    .instruction(&LocalGet(object))
                                    .instruction(&RefCastNonNull(HeapType::Concrete(7)))
                                    .instruction(&StructGet { struct_type_index: 7, field_index: 0 });
                            };
                            function
                                .instruction(&LocalGet(object))
                                .instruction(&RefTestNonNull(HeapType::Concrete(7)))
                                .instruction(&If(BlockType::Result(ValType::I32)));
                            descriptor(&mut function);
                            function
                                .instruction(&GlobalGet(cache))
                                .instruction(&RefEq)
                                .instruction(&GlobalGet(cache + 2))
                                .instruction(&GlobalGet(epoch_global))
                                .instruction(&I32Eq)
                                .instruction(&I32And)
                                .instruction(&If(BlockType::Result(ValType::I32)))
                                .instruction(&GlobalGet(cache + 1))
                                .instruction(&RefCastNonNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 }))
                                .instruction(&I31GetU)
                                .instruction(&Else)
                                .instruction(&LocalGet(marker))
                                .instruction(&LocalGet(object))
                                .instruction(&Call(index("protocol-marker-satisfies")))
                                .instruction(&LocalTee(field_index_local))
                                .instruction(&RefI31)
                                .instruction(&GlobalSet(cache + 1));
                            descriptor(&mut function);
                            function
                                .instruction(&GlobalSet(cache))
                                .instruction(&GlobalGet(epoch_global))
                                .instruction(&GlobalSet(cache + 2))
                                .instruction(&LocalGet(field_index_local))
                                .instruction(&End)
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
                            function
                                .instruction(&LocalGet(descriptor))
                                .instruction(&GlobalGet(cache))
                                .instruction(&RefEq)
                                .instruction(&GlobalGet(cache + 2))
                                .instruction(&GlobalGet(epoch_global))
                                .instruction(&I32Eq)
                                .instruction(&I32And)
                                .instruction(&If(BlockType::Result(VALUE)))
                                .instruction(&GlobalGet(cache + 1))
                                .instruction(&Else)
                                .instruction(&LocalGet(descriptor))
                                .instruction(&LocalGet(key))
                                .instruction(&Call(index("protocol-method-get")))
                                .instruction(&GlobalSet(cache + 1))
                                .instruction(&LocalGet(descriptor))
                                .instruction(&GlobalSet(cache))
                                .instruction(&GlobalGet(epoch_global))
                                .instruction(&GlobalSet(cache + 2))
                                .instruction(&GlobalGet(cache + 1))
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
                            // The site's struct and the runtime's cached read (#378).
                            function
                                .instruction(&GlobalGet(cache))
                                .instruction(&LocalGet(object))
                                .instruction(&LocalGet(name))
                                .instruction(&Call(index("named-site-get")));
                            if length {
                                function.instruction(&End);
                            }
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
                        Nominal::NumberHash => {
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&Call(index("number-hash")));
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
                            // Operands are already evaluated SSA locals. Select the
                            // live plan at the original constructor-selection point.
                            // Plain descriptors share runtime construction directly;
                            // special constructor closures retain generic invocation.
                            function
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&Call(index("constructor-descriptor")))
                                .instruction(&Call(index("source-constructor-plan")));
                            for argument in &arguments[1..] {
                                function.instruction(&LocalGet(argument.0 as u32 + offset));
                            }
                            function
                                .instruction(&ArrayNewFixed {
                                    array_type_index: runtime_abi::ARGS,
                                    array_size: arguments.len() as u32 - 1,
                                })
                                .instruction(&Call(index("source-construct-from-plan")));
                        }
                        Nominal::Instance => {
                            // The site's cached test (#378).
                            let cache = instance_sites[&(ir as *const IrFunction as usize, inst.result.0 as u32)];
                            function
                                .instruction(&GlobalGet(cache))
                                .instruction(&LocalGet(arguments[0].0 as u32 + offset))
                                .instruction(&LocalGet(arguments[1].0 as u32 + offset))
                                .instruction(&Call(index("instance-site-test")));
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
                        Nominal::Field(field) | Nominal::Key(field) => {
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
                        | Nominal::CoerceString
                        | Nominal::ConcatString
                        | Nominal::ValueAdd
                        | Nominal::StringIndexOf
                        | Nominal::StringSlice
                        | Nominal::StringFromUnit
                        | Nominal::LanguageError
                        | Nominal::IsLanguageError
                        | Nominal::IsNativeObject
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
                                Nominal::CoerceString => "coerce-string",
                                Nominal::ObjectCoerceString => "object-coerce-string",
                                Nominal::ConcatString => "string-concat",
                                Nominal::ValueAdd => "value-add",
                                Nominal::StringIndexOf => "string-index-of",
                                Nominal::StringSlice => "string-slice",
                                Nominal::StringFromUnit => "string-from-unit",
                                Nominal::LanguageError => "language-error-new",
                                Nominal::IsLanguageError => "language-error-is",
                                Nominal::NativeObjectFactory => "native-object-factory-function",
                                Nominal::IsNativeObject => "native-object?",
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
                                Nominal::MethodGet => "protocol-method-get",
                                Nominal::NativeSatisfies => "protocol-native-satisfies",
                                _ => "protocol-marker-satisfies",
                            })));
                        }
                    }
                    if operation.result() == Type::Bool {
                        for instruction in runtime_abi::boolean_in(sentinels) {
                            function.instruction(&instruction);
                        }
                    }
                    function.instruction(&LocalSet(inst.result.0 as u32 + offset));
                }
                Operation::Arithmetic {
                    operator,
                    arguments,
                    coercing,
                } => {
                    let name = arithmetic_name(*operator, arguments, *coercing, ir);
                    // Two i31 longs add or subtract inline (#361) while the
                    // result is an i31; the runtime boxes a larger one.
                    let inline = match name {
                        // value-add and value-subtract (JS convention) treat
                        // two numbers as number-add and number-subtract do.
                        "number-add" | "checked-add" | "value-add" => Some(I32Add),
                        "number-subtract" | "checked-subtract" | "value-subtract" => Some(I32Sub),
                        _ => None,
                    }
                    .filter(|_| arguments.len() == 2);
                    let fast = inline.is_some();
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
                    function.instruction(&LocalGet(value.0 as u32 + offset)).instruction(&Return);
                }
            }
            Terminator::Jump { target, arguments } => {
                // Push every replacement value before any parameter assignment.
                for (parameter, argument) in ir.blocks[*target].parameters.iter().zip(arguments) {
                    function.instruction(&LocalGet(match locals.of(*parameter) {
                        unboxed::Repr::Boxed => argument.0 as u32 + offset,
                        _ => locals.raw(*argument),
                    }));
                }
                for parameter in ir.blocks[*target].parameters.iter().rev() {
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
