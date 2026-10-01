//! Shared GC ABI v1. The legacy backend has not migrated to this ABI.
//! All ordinary numbers are boxed binary64; strings store UTF-16 units.
//! Construction indices below are implementation details, not artifact ABI IDs.
use std::borrow::Cow;
use wasm_encoder::*;
mod arithmetic;
mod arrays;
mod bitwise;
mod closure_properties;
mod comparisons;
mod dynamic;
mod exception_info;
mod exceptions;
mod named_properties;
mod native_protocols;
mod native_objects;
mod native_object_methods;
mod native_object_properties;
mod nominal;
mod numeric;
mod numeric_hash;
mod object_methods;
mod predicates;
mod string_methods;

pub const VERSION: u32 = 1;
// Internal constructor/ordinary-type-call undefined, distinct from source nil.
pub(crate) const UNDEFINED: i32 = 6;
const PORTABLE_FORMAT_VERSION: u32 = 2;
const MANIFEST: &str = "suss.runtime-abi";
const NUMBER: u32 = 0;
const STRING: u32 = 1;
pub(crate) const ARGS: u32 = 2;
pub(crate) const INVOKE: u32 = 3;
const DESCRIPTOR: u32 = 6;
pub(crate) const TYPE_COUNT: u32 = 10;
const VALUE: ValType = ValType::Ref(RefType::EQREF);

fn reference(index: u32) -> ValType {
    ValType::Ref(RefType {
        nullable: false,
        heap_type: HeapType::Concrete(index),
    })
}
/// Exact shared binding-cell reference type for fragment imports.
pub(crate) fn binding_cell_type() -> ValType {
    reference(5)
}
fn field(ty: ValType, mutable: bool) -> FieldType {
    FieldType {
        element_type: StorageType::Val(ty),
        mutable,
    }
}
fn structure(fields: Vec<FieldType>) -> CompositeInnerType {
    CompositeInnerType::Struct(StructType {
        fields: fields.into_boxed_slice(),
    })
}

/// The identical recursive group must begin at type index zero in every fragment.
/// Append fragment function types afterward; do not embed prototype GC layouts.
pub fn prelude() -> TypeSection {
    let mut types = TypeSection::new();
    let group = vec![
        structure(vec![field(ValType::F64, false)]),
        CompositeInnerType::Array(ArrayType(FieldType {
            element_type: StorageType::I16,
            mutable: true,
        })),
        CompositeInnerType::Array(ArrayType(field(VALUE, true))),
        CompositeInnerType::Func(FuncType::new([VALUE, reference(ARGS)], [VALUE])),
        // Closure: environment, invoke, minimum arity, maximum arity (-1 variadic).
        structure(vec![
            field(VALUE, false),
            field(reference(INVOKE), false),
            field(ValType::I32, false),
            field(ValType::I32, false),
        ]),
        // Binding cell: value and independent bound flag (nil can be a binding).
        structure(vec![field(VALUE, true), field(ValType::I32, true)]),
        // Descriptor: nominal identity, field schema, protocol table, metadata.
        structure(vec![
            field(ValType::I64, false),
            field(VALUE, false),
            field(VALUE, true),
            field(VALUE, true),
        ]),
        // User object: descriptor, fields, per-object metadata.
        structure(vec![
            field(reference(DESCRIPTOR), false),
            field(reference(ARGS), false),
            field(VALUE, false),
        ]),
        // Exception: descriptor, message, data, cause.
        structure(vec![
            field(reference(DESCRIPTOR), false),
            field(reference(STRING), false),
            field(VALUE, false),
            field(VALUE, false),
        ]),
        // Dynamic frame: parent and binding entries; scheduler roots it as Value.
        structure(vec![field(VALUE, false), field(reference(ARGS), false)]),
    ];
    types.ty().rec(group.into_iter().map(|inner| SubType {
        is_final: true,
        supertype_idx: None,
        composite_type: CompositeType {
            inner,
            shared: false,
            descriptor: None,
            describes: None,
        },
    }));
    types
}

/// Versions checked before any fragment initialization. Layout compatibility is
/// additionally checked by Wasm validation/linking; a manifest is not a proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub runtime_abi: u32,
    pub compiler: String,
    pub wasm_tools: String,
}
impl Default for Manifest {
    fn default() -> Self {
        Self {
            runtime_abi: VERSION,
            compiler: format!(
                "{}+portable.{PORTABLE_FORMAT_VERSION}",
                env!("CARGO_PKG_VERSION")
            ),
            wasm_tools: "0.258.0".into(),
        }
    }
}
impl Manifest {
    pub fn section(&self) -> CustomSection<'static> {
        let mut data = self.runtime_abi.to_le_bytes().to_vec();
        data.extend_from_slice(self.compiler.as_bytes());
        data.push(0);
        data.extend_from_slice(self.wasm_tools.as_bytes());
        data.push(0);
        CustomSection {
            name: Cow::Borrowed(MANIFEST),
            data: Cow::Owned(data),
        }
    }
}

/// Reject absent, duplicate, malformed or incompatible manifests before loading
/// the artifact. Callers must then validate/link before starting guest code.
pub fn verify_artifact(bytes: &[u8], expected: &Manifest) -> Result<(), String> {
    let mut found = None;
    let mut saw_types = false;
    let mut prelude_matches = false;
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            wasmparser::Payload::Version {
                encoding: wasmparser::Encoding::Component,
                ..
            } => return Err("ABI v1 requires a core module".into()),
            wasmparser::Payload::CustomSection(section) if section.name() == MANIFEST => {
                if found.is_some() {
                    return Err("duplicate runtime ABI manifest".into());
                }
                let data = section.data();
                if data.len() < 4 || data.len() > 4096 {
                    return Err("malformed runtime ABI manifest".into());
                }
                let runtime_abi = u32::from_le_bytes(data[..4].try_into().unwrap());
                let strings =
                    std::str::from_utf8(&data[4..]).map_err(|_| "invalid manifest text")?;
                let fields: Vec<_> = strings.split('\0').collect();
                if fields.len() != 3
                    || fields[0].is_empty()
                    || fields[1].is_empty()
                    || !fields[2].is_empty()
                {
                    return Err("malformed runtime ABI versions".into());
                }
                found = Some(Manifest {
                    runtime_abi,
                    compiler: fields[0].into(),
                    wasm_tools: fields[1].into(),
                });
            }
            wasmparser::Payload::TypeSection(reader) => {
                if saw_types {
                    return Err("duplicate runtime ABI type section".into());
                }
                saw_types = true;
                if let Some(group) = reader.into_iter().next() {
                    let group = group.map_err(|e| e.to_string())?;
                    prelude_matches = group.is_explicit_rec_group()
                        && group.types().eq(expected_prelude().iter());
                }
            }
            _ => (),
        }
    }
    match found {
        None => Err("missing runtime ABI manifest".into()),
        Some(actual) if &actual != expected => Err(format!(
            "runtime ABI version mismatch: expected {expected:?}, actual {actual:?}"
        )),
        Some(_) if !prelude_matches => Err("missing or incompatible runtime ABI prelude".into()),
        Some(_) => Ok(()),
    }
}

fn expected_prelude() -> &'static Vec<wasmparser::SubType> {
    static TYPES: std::sync::OnceLock<Vec<wasmparser::SubType>> = std::sync::OnceLock::new();
    TYPES.get_or_init(|| {
        let mut module = Module::new();
        module.section(&prelude());
        let bytes = module.finish();
        for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
            if let wasmparser::Payload::TypeSection(reader) =
                payload.expect("generated ABI prelude")
            {
                return reader
                    .into_iter()
                    .next()
                    .unwrap()
                    .expect("generated recursive group")
                    .types()
                    .cloned()
                    .collect();
            }
        }
        unreachable!("generated runtime always has its type prelude")
    })
}

struct Builder {
    types: TypeSection,
    functions: FunctionSection,
    exports: ExportSection,
    code: CodeSection,
    count: u32,
    next_type: u32,
    names: std::collections::BTreeMap<String, u32>,
}
impl Builder {
    fn function(
        &mut self,
        name: &str,
        params: &[ValType],
        results: &[ValType],
        instructions: &[Instruction<'_>],
    ) -> u32 {
        self.function_with_locals(name, params, results, &[], instructions)
    }
    fn function_with_locals(
        &mut self,
        name: &str,
        params: &[ValType],
        results: &[ValType],
        locals: &[(u32, ValType)],
        instructions: &[Instruction<'_>],
    ) -> u32 {
        let index = self.count;
        self.names.insert(name.to_owned(), index);
        self.types
            .ty()
            .function(params.iter().copied(), results.iter().copied());
        self.functions.function(self.next_type);
        self.next_type += 1;
        self.exports.export(name, ExportKind::Func, self.count);
        let mut function = Function::new(locals.iter().copied());
        for instruction in instructions {
            function.instruction(instruction);
        }
        function.instruction(&Instruction::End);
        self.code.function(&function);
        self.count += 1;
        index
    }
}

/// Generate the production core runtime, without target imports. Private numeric
/// conversion scratch memory is separate from GC language storage.
/// These storage/numeric intrinsics are for verified lowering, not user APIs.
/// Generic invocation checks arity centrally and throws a language error.
/// Target adapters and compiler lowering remain separate.
pub fn module() -> Vec<u8> {
    static BYTES: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    BYTES.get_or_init(build_module).clone()
}
fn build_module() -> Vec<u8> {
    use Instruction::*;
    let helper = numeric::helper();
    let numeric_info = helper.info();
    let mut b = Builder {
        types: helper.types,
        functions: helper.functions,
        exports: ExportSection::new(),
        code: helper.code,
        count: helper.function_count,
        next_type: helper.type_count,
        names: std::collections::BTreeMap::new(),
    };
    let number = HeapType::Concrete(NUMBER);
    let string = HeapType::Concrete(STRING);
    for (name, sentinel) in [("nil", 0), ("false", 2), ("true", 4)] {
        b.function(name, &[], &[VALUE], &[I32Const(sentinel), RefI31]);
    }
    b.function(
        "number-box",
        &[ValType::F64],
        &[VALUE],
        &[LocalGet(0), StructNew(NUMBER)],
    );
    b.function(
        "number-unbox",
        &[VALUE],
        &[ValType::F64],
        &[
            LocalGet(0),
            RefCastNonNull(number),
            StructGet {
                struct_type_index: NUMBER,
                field_index: 0,
            },
        ],
    );
    for (name, operation) in [
        ("number-add", F64Add),
        ("number-subtract", F64Sub),
        ("number-multiply", F64Mul),
        ("number-divide", F64Div),
    ] {
        b.function(
            name,
            &[VALUE, VALUE],
            &[VALUE],
            &[
                LocalGet(0),
                RefCastNonNull(number),
                StructGet {
                    struct_type_index: NUMBER,
                    field_index: 0,
                },
                LocalGet(1),
                RefCastNonNull(number),
                StructGet {
                    struct_type_index: NUMBER,
                    field_index: 0,
                },
                operation,
                StructNew(NUMBER),
            ],
        );
    }
    b.function(
        "number-negate",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            RefCastNonNull(number),
            StructGet {
                struct_type_index: NUMBER,
                field_index: 0,
            },
            F64Neg,
            StructNew(NUMBER),
        ],
    );
    b.function(
        "string-new",
        &[ValType::I32],
        &[VALUE],
        &[LocalGet(0), ArrayNewDefault(STRING)],
    );
    b.function(
        "string-length",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), RefCastNonNull(string), ArrayLen],
    );
    b.function(
        "string-unit",
        &[VALUE, ValType::I32],
        &[ValType::I32],
        &[
            LocalGet(0),
            RefCastNonNull(string),
            LocalGet(1),
            ArrayGetU(STRING),
        ],
    );
    // Checked storage helper: false rejects invalid index/unit without truncation.
    b.function(
        "string-set-unit",
        &[VALUE, ValType::I32, ValType::I32],
        &[ValType::I32],
        &[
            LocalGet(2),
            I32Const(65535),
            I32GtU,
            LocalGet(1),
            LocalGet(0),
            RefCastNonNull(string),
            ArrayLen,
            I32GeU,
            I32Or,
            If(BlockType::Result(ValType::I32)),
            I32Const(0),
            Else,
            LocalGet(0),
            RefCastNonNull(string),
            LocalGet(1),
            LocalGet(2),
            ArraySet(STRING),
            I32Const(1),
            End,
        ],
    );
    b.function(
        "args-new",
        &[ValType::I32],
        &[VALUE],
        &[I32Const(0), RefI31, LocalGet(0), ArrayNew(ARGS)],
    );
    let (wrap_environment, closure_environment) = closure_properties::functions(&mut b);
    b.function(
        "closure-new",
        &[VALUE, reference(INVOKE), ValType::I32, ValType::I32],
        &[VALUE],
        &[
            LocalGet(0),
            Call(wrap_environment),
            LocalGet(1),
            LocalGet(2),
            LocalGet(3),
            StructNew(4),
        ],
    );
    let closure = HeapType::Concrete(4);
    let args = HeapType::Concrete(ARGS);
    let mut invoke = Vec::new();
    for (local, kind, descriptor, message) in [
        (0, closure, 2, "Not callable"),
        (1, args, 3, "Invalid argument array"),
    ] {
        invoke.extend([
            LocalGet(local),
            RefTestNonNull(kind),
            I32Eqz,
            If(BlockType::Empty),
            GlobalGet(descriptor),
        ]);
        let units: Vec<_> = message.encode_utf16().collect();
        invoke.extend(units.iter().map(|unit| I32Const(*unit as i32)));
        invoke.extend([
            ArrayNewFixed {
                array_type_index: STRING,
                array_size: units.len() as u32,
            },
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(8),
            Throw(0),
            End,
        ]);
    }
    invoke.extend([
        LocalGet(1),
        RefCastNonNull(args),
        ArrayLen,
        LocalGet(0),
        RefCastNonNull(closure),
        StructGet {
            struct_type_index: 4,
            field_index: 2,
        },
        I32LtU,
        LocalGet(0),
        RefCastNonNull(closure),
        StructGet {
            struct_type_index: 4,
            field_index: 3,
        },
        I32Const(-1),
        I32Ne,
        LocalGet(1),
        RefCastNonNull(args),
        ArrayLen,
        LocalGet(0),
        RefCastNonNull(closure),
        StructGet {
            struct_type_index: 4,
            field_index: 3,
        },
        I32GtU,
        I32And,
        I32Or,
        If(BlockType::Empty),
        GlobalGet(0),
    ]);
    let message: Vec<_> = "Wrong arity".encode_utf16().collect();
    invoke.extend(message.iter().map(|unit| I32Const(*unit as i32)));
    invoke.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: message.len() as u32,
        },
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        StructNew(8),
        Throw(0),
        End,
        LocalGet(0),
        Call(closure_environment),
        LocalGet(1),
        RefCastNonNull(args),
        LocalGet(0),
        RefCastNonNull(closure),
        StructGet {
            struct_type_index: 4,
            field_index: 1,
        },
        CallRef(INVOKE),
    ]);
    let mut arity_error = vec![GlobalGet(0)];
    arity_error.extend(message.iter().map(|unit| I32Const(*unit as i32)));
    arity_error.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: message.len() as u32,
        },
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        StructNew(8),
        Throw(0),
    ]);
    b.function("arity-error", &[], &[VALUE], &arity_error);
    let generic_invoke = b.function("invoke", &[VALUE, VALUE], &[VALUE], &invoke);
    b.function(
        "binding-new",
        &[VALUE],
        &[VALUE],
        &[LocalGet(0), I32Const(1), StructNew(5)],
    );
    b.function(
        "binding-unbound",
        &[],
        &[VALUE],
        &[I32Const(0), RefI31, I32Const(0), StructNew(5)],
    );
    b.function(
        "binding-bound",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(5)),
            StructGet {
                struct_type_index: 5,
                field_index: 1,
            },
            If(BlockType::Result(VALUE)),
            I32Const(4),
            RefI31,
            Else,
            I32Const(2),
            RefI31,
            End,
        ],
    );
    let dynamic_lookup = dynamic::lookup(&mut b);
    let mut binding_get = vec![
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(5)),
        StructGet {
            struct_type_index: 5,
            field_index: 1,
        },
        I32Eqz,
        If(BlockType::Empty),
        GlobalGet(1),
    ];
    let unbound_message: Vec<_> = "Unbound binding".encode_utf16().collect();
    binding_get.extend(unbound_message.iter().map(|unit| I32Const(*unit as i32)));
    binding_get.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: unbound_message.len() as u32,
        },
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        StructNew(8),
        Throw(0),
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(5)),
        StructGet {
            struct_type_index: 5,
            field_index: 0,
        },
    ]);
    dynamic::binding_get(&mut b, dynamic_lookup, binding_get);
    let binding_set = dynamic::binding_set(&mut b, dynamic_lookup);
    let primitives = numeric::intrinsics(&mut b, numeric_info);
    bitwise::intrinsics(&mut b);
    numeric_hash::intrinsics(&mut b);
    let mut arithmetic_functions = arithmetic::functions(&mut b, primitives);
    arithmetic_functions.extend(arrays::functions(&mut b));
    arithmetic_functions.extend(nominal::functions(&mut b, generic_invoke));
    let try_invoke = exceptions::functions(&mut b, generic_invoke);
    arithmetic_functions.extend(dynamic::functions(&mut b, binding_set, try_invoke));
    arithmetic_functions.extend(exception_info::functions(&mut b));
    arithmetic_functions.extend(predicates::functions(&mut b));
    arithmetic_functions.extend(comparisons::functions(&mut b));
    arithmetic_functions.extend(bitwise::functions(&mut b));
    arithmetic_functions.extend(named_properties::functions(&mut b));
    native_objects::functions(&mut b);
    arithmetic_functions.extend(native_object_methods::functions(&mut b));
    native_object_properties::functions(&mut b);
    let mut elements = ElementSection::new();
    elements.declared(Elements::Functions(Cow::Owned(arithmetic_functions)));
    let mut tags = TagSection::new();
    tags.tag(TagType {
        kind: TagKind::Exception,
        func_type_idx: b.next_type,
    });
    b.types.ty().function([VALUE], []);
    let mut globals = GlobalSection::new();
    // Separate rooted descriptors: arity, unbound, not callable, invalid arguments.
    for identity in 1..=numeric::ERROR_GLOBALS {
        globals.global(
            GlobalType {
                val_type: reference(DESCRIPTOR),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([
                I64Const(identity.into()),
                I32Const(0),
                RefI31,
                I32Const(0),
                RefI31,
                I32Const(0),
                RefI31,
                StructNew(DESCRIPTOR),
            ]),
        );
    }
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(numeric_info.stack_top),
    );
    // Append after the pinned numeric stack global; never relocate its index.
    globals.global(
        GlobalType {
            val_type: ValType::I64,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i64_const(i64::from(numeric::ERROR_GLOBALS) + 4),
    );
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(i64::from(numeric::ERROR_GLOBALS) + 1),
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(DESCRIPTOR),
        ]),
    );
    // A rooted opaque protocol marker, distinct from booleans and callables.
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(DESCRIPTOR),
        ]),
    );
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([I32Const(0), RefI31]),
    );
    let mut info_descriptor = vec![I64Const(i64::from(numeric::ERROR_GLOBALS) + 2)];
    for field in ["message", "data", "cause"] {
        let units: Vec<_> = field.encode_utf16().collect();
        info_descriptor.extend(units.iter().map(|x| I32Const(*x as i32)));
        info_descriptor.push(ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        });
    }
    info_descriptor.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        I32Const(0),
        ArrayNewDefault(ARGS),
        I32Const(0),
        RefI31,
        StructNew(DESCRIPTOR),
    ]);
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended(info_descriptor),
    );
    let mut ordinary_root = vec![I64Const(i64::from(numeric::ERROR_GLOBALS) + 3)];
    for field in ["message", "data", "cause"] {
        let units: Vec<_> = field.encode_utf16().collect();
        ordinary_root.extend(units.iter().map(|x| I32Const(*x as i32)));
        ordinary_root.push(ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        });
    }
    ordinary_root.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        I32Const(0),
        ArrayNewDefault(ARGS),
        I32Const(0),
        RefI31,
        StructNew(DESCRIPTOR),
        I32Const(UNDEFINED),
        RefI31,
        I32Const(3),
        ArrayNew(ARGS),
        I32Const(0),
        RefI31,
        StructNew(7),
    ]);
    globals.global(
        GlobalType {
            val_type: reference(7),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended(ordinary_root),
    );
    // Owner tag for closure environments/properties; identity is reference based.
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            StructNew(DESCRIPTOR),
        ]),
    );
    // Private source-array identity; appended without changing old global indices.
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            StructNew(DESCRIPTOR),
        ]),
    );
    // Appended private Object method tag and portable implicit-this realm.
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            StructNew(DESCRIPTOR),
        ]),
    );
    globals.global(
        GlobalType {
            val_type: reference(7),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            StructNew(DESCRIPTOR),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            StructNew(7),
        ]),
    );
    // Private singleton builtin method root; no owners or per-string registry.
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([I32Const(0), RefI31]),
    );
    // Original owned dynamic data-property storage tag, appended privately.
    globals.global(GlobalType { val_type: reference(DESCRIPTOR), mutable: false, shared: false },
        &ConstExpr::extended([I64Const(0), I32Const(0), ArrayNewDefault(ARGS),
            I32Const(0), ArrayNewDefault(ARGS), I32Const(0), RefI31, StructNew(DESCRIPTOR)]));
    // Lazy shared default Object prototype, owned by the runtime GC root.
    globals.global(GlobalType { val_type: VALUE, mutable: true, shared: false },
        &ConstExpr::extended([I32Const(0), RefI31]));
    b.exports
        .export("dynamic-frame", ExportKind::Global, dynamic::CURRENT);
    b.exports
        .export("numeric-scratch-memory", ExportKind::Memory, 0);
    b.exports.export(
        "numeric-stack-pointer",
        ExportKind::Global,
        numeric_info.stack_global,
    );
    b.exports.export("language-exception", ExportKind::Tag, 0);
    let mut runtime = Module::new();
    runtime
        .section(&b.types)
        .section(&b.functions)
        .section(&helper.memory)
        .section(&tags)
        .section(&globals)
        .section(&b.exports)
        .section(&elements)
        .section(&b.code)
        .section(&helper.data)
        .section(&Manifest::default().section());
    runtime.finish()
}
