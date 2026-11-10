//! Shared GC ABI v2. The legacy backend has not migrated to this ABI.
//! All ordinary numbers are boxed binary64; strings store UTF-16 units.
//! Construction indices below are implementation details, not artifact ABI IDs.
use std::borrow::Cow;
use wasm_encoder::*;
mod arithmetic;
mod array_join;
mod array_methods;
mod array_own_properties;
mod array_pop;
mod array_shift;
mod array_properties;
mod array_push;
mod arrays;
mod r#async;
mod async_library;
mod bitwise;
mod callable_properties;
mod closure_calls;
mod closure_properties;
mod coercions;
mod comparisons;
mod dynamic;
mod error_constructors;
mod error_properties;
mod exception_info;
mod exceptions;
mod identity_hash;
mod longs;
mod named_properties;
mod native_object_factory;
mod native_object_methods;
mod native_object_properties;
mod native_objects;
mod native_protocols;
mod nominal;
mod numeric;
mod packed_arrays;
mod numeric_hash;
mod numeric_operands;
mod object_descriptors;
mod object_methods;
mod owned_prototypes;
mod predicates;
mod primitive_constructors;
mod range_errors;
mod sparse_arrays;
mod streams;
mod string_methods;
mod string_char_at;
mod type_values;

pub const VERSION: u32 = 3;
// The language sentinels. Every i31 is a long (ADR-0003); nil is a null
// eqref, and false, true and undefined are SENTINEL structs held in runtime
// globals that fragments import. Code names a sentinel with these and builds
// it with `sentinel_value`; the numbers are only the SENTINEL struct's tag.
pub const NIL: i32 = 0;
pub const FALSE: i32 = 2;
pub const TRUE: i32 = 4;
// Internal constructor/ordinary-type-call undefined, distinct from source nil.
pub const UNDEFINED: i32 = 6;
// Right after the numeric stack global, before any initializer that uses them.
pub(crate) const FALSE_GLOBAL: u32 = numeric::ERROR_GLOBALS + 1;
pub(crate) const TRUE_GLOBAL: u32 = FALSE_GLOBAL + 1;
pub(crate) const UNDEFINED_GLOBAL: u32 = TRUE_GLOBAL + 1;
/// Runtime export names of the sentinel globals, in tag order.
pub const SENTINEL_EXPORTS: [(&str, i32); 3] =
    [("sentinel-false", FALSE), ("sentinel-true", TRUE), ("sentinel-undefined", UNDEFINED)];

/// A sentinel inside the runtime module, where its globals are local.
pub(crate) fn sentinel_value(value: i32) -> Instruction<'static> {
    sentinel_in(value, FALSE_GLOBAL)
}
/// A sentinel in a module whose false, true and undefined globals start at
/// `first` in tag order, such as a fragment that imports them.
pub fn sentinel_in(value: i32, first: u32) -> Instruction<'static> {
    match value {
        NIL => Instruction::RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }),
        FALSE => Instruction::GlobalGet(first),
        TRUE => Instruction::GlobalGet(first + 1),
        UNDEFINED => Instruction::GlobalGet(first + 2),
        other => panic!("not a sentinel: {other}"),
    }
}
/// `[cond] -> [boolean]`: true for a nonzero i32, false for zero.
pub fn boolean_in(first: u32) -> [Instruction<'static>; 5] {
    [
        Instruction::If(BlockType::Result(VALUE)),
        Instruction::GlobalGet(first + 1),
        Instruction::Else,
        Instruction::GlobalGet(first),
        Instruction::End,
    ]
}
/// `[] -> [i32]`: whether a local holds a number, a long or a double.
pub fn number_test(local: u32) -> [Instruction<'static>; 8] {
    longs::number_test(local)
}
/// `[] -> [f64]`: the number in `local` as a double; traps unless it is one.
pub fn number_to_f64(local: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    vec![
        LocalGet(local),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        If(BlockType::Result(ValType::F64)),
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        Else,
        LocalGet(local),
        RefTestNonNull(HeapType::I31),
        If(BlockType::Result(ValType::F64)),
        LocalGet(local),
        RefCastNonNull(HeapType::I31),
        I31GetS,
        F64ConvertI32S,
        Else,
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(LONG)),
        StructGet {
            struct_type_index: LONG,
            field_index: 0,
        },
        F64ConvertI64S,
        End,
        End,
    ]
}
/// `[] -> [i32]`: whether a local holds a long.
pub fn long_test(local: u32) -> [Instruction<'static>; 5] {
    use Instruction::*;
    [
        LocalGet(local),
        RefTestNonNull(HeapType::I31),
        LocalGet(local),
        RefTestNonNull(HeapType::Concrete(LONG)),
        I32Or,
    ]
}
/// `[] -> [i64]`: the long in `local`; traps unless it is one.
pub fn long_to_i64(local: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    vec![
        LocalGet(local),
        RefTestNonNull(HeapType::I31),
        If(BlockType::Result(ValType::I64)),
        LocalGet(local),
        RefCastNonNull(HeapType::I31),
        I31GetS,
        I64ExtendI32S,
        Else,
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(LONG)),
        StructGet {
            struct_type_index: LONG,
            field_index: 0,
        },
        End,
    ]
}
/// `[i64] -> [eqref]`: a long, as an i31 when it fits and boxed otherwise,
/// using `scratch`, an `i64` local.
pub fn long_box_in(scratch: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut code = vec![LocalTee(scratch)];
    code.extend(longs::fits_i31());
    code.extend([
        If(BlockType::Result(VALUE)),
        LocalGet(scratch),
        I32WrapI64,
        RefI31,
        Else,
        LocalGet(scratch),
        StructNew(LONG),
        End,
    ]);
    code
}
pub(crate) fn boolean() -> [Instruction<'static>; 5] {
    boolean_in(FALSE_GLOBAL)
}
const PORTABLE_FORMAT_VERSION: u32 = 3;
const MANIFEST: &str = "suss.runtime-abi";
pub(crate) const NUMBER: u32 = 0;
pub(crate) const STRING: u32 = 1;
pub(crate) const ARGS: u32 = 2;
pub(crate) const INVOKE: u32 = 3;
pub(crate) const CLOSURE: u32 = 4;
pub(crate) const BINDING: u32 = 5;
/// Bits of a binding cell's flag field. A cell is dynamic once a binding frame
/// has held it, and stays so; only a dynamic cell's reads search the frames, so
/// a read of a bound, never-bound-dynamically cell is its root (#354).
pub(crate) const CELL_BOUND: i32 = 1;
pub(crate) const CELL_DYNAMIC: i32 = 2;
const DESCRIPTOR: u32 = 6;
pub const SENTINEL: u32 = 10;
pub const LONG: u32 = 11;
/// Packed primitive array storage, one type per element type, in
/// `packed_arrays::KINDS` order: byte, short, int, float, long, double (#336).
pub const PACKED_STORAGE: u32 = 12;
/// A packed primitive array: its storage and an identity UID.
pub const PACKED: u32 = 18;
pub(crate) const TYPE_COUNT: u32 = 19;
const VALUE: ValType = ValType::Ref(RefType::EQREF);

fn reference(index: u32) -> ValType {
    ValType::Ref(RefType {
        nullable: false,
        heap_type: HeapType::Concrete(index),
    })
}
/// Exact type of the imported false, true and undefined globals.
pub fn sentinel_type() -> ValType {
    reference(SENTINEL)
}
/// Exact shared binding-cell reference type for fragment imports.
pub(crate) fn binding_cell_type() -> ValType {
    reference(BINDING)
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
            field(VALUE, true), // Lazily assigned, owner-held identity UID.
        ]),
        // Binding cell: value and flags, CELL_BOUND independent of the value
        // (nil can be a binding) and CELL_DYNAMIC.
        structure(vec![field(VALUE, true), field(ValType::I32, true)]),
        // Descriptor: nominal identity, field schema, protocol table, metadata.
        structure(vec![
            field(ValType::I64, false),
            field(VALUE, false),
            field(VALUE, true),
            field(VALUE, true),
            field(VALUE, true), // Identity UID, separate from metadata.
            // Munged names of `^:mutable` fields (ARGS of strings), or nil.
            field(VALUE, false),
        ]),
        // User object: descriptor, fields, per-object metadata.
        structure(vec![
            field(reference(DESCRIPTOR), false),
            field(reference(ARGS), false),
            field(VALUE, false),
            field(VALUE, true), // Identity UID.
        ]),
        // Exception: descriptor, message, data, cause.
        structure(vec![
            field(reference(DESCRIPTOR), false),
            field(reference(STRING), false),
            field(VALUE, false),
            field(VALUE, false),
            field(VALUE, true), // Identity UID; message/data/cause offsets unchanged.
        ]),
        // Dynamic frame: parent and binding entries; scheduler roots it as Value.
        structure(vec![field(VALUE, false), field(reference(ARGS), false)]),
        // ABI v3. Sentinel: false, true or undefined, by the tag it holds.
        structure(vec![field(ValType::I32, false)]),
        // Long outside i31's range; one in range is always an i31.
        structure(vec![field(ValType::I64, false)]),
    ];
    // Packed primitive array storage (#336), then the array value itself:
    // storage and a lazily assigned identity UID, as `hash` needs one.
    let mut group = group;
    for element in [
        StorageType::I8,
        StorageType::I16,
        StorageType::Val(ValType::I32),
        StorageType::Val(ValType::F32),
        StorageType::Val(ValType::I64),
        StorageType::Val(ValType::F64),
    ] {
        group.push(CompositeInnerType::Array(ArrayType(FieldType {
            element_type: element,
            mutable: true,
        })));
    }
    group.push(structure(vec![field(VALUE, false), field(VALUE, true)]));
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
            } => return Err("Shared runtime ABI requires a core module".into()),
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
    let string = HeapType::Concrete(STRING);
    for (name, sentinel) in [("nil", NIL), ("false", FALSE), ("true", TRUE)] {
        b.function(name, &[], &[VALUE], &[sentinel_value(sentinel)]);
    }
    b.function(
        "number-box",
        &[ValType::F64],
        &[VALUE],
        &[LocalGet(0), StructNew(NUMBER)],
    );
    longs::intrinsics(&mut b);
    longs::arithmetic(&mut b);
    longs::string(&mut b);
    longs::functions(&mut b);
    longs::parse(&mut b);
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
        &[sentinel_value(NIL), LocalGet(0), ArrayNew(ARGS)],
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
            I32Const(0), // Identity UID: private unassigned marker.
            RefI31,
            StructNew(4),
        ],
    );
    let closure = HeapType::Concrete(4);
    let args = HeapType::Concrete(ARGS);
    // Calling a name a failed `def` or `declare` left unbound is Clojure's
    // unbound-fn error, not a non-function's (#288).
    let mut invoke = vec![
        LocalGet(0),
        sentinel_value(UNDEFINED),
        RefEq,
        If(BlockType::Empty),
        GlobalGet(2),
    ];
    let units: Vec<_> = "Attempting to call unbound fn".encode_utf16().collect();
    invoke.extend(units.iter().map(|unit| I32Const(*unit as i32)));
    invoke.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        },
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        StructNew(8),
        Throw(0),
        End,
    ]);
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
            sentinel_value(NIL),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
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
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
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
        ReturnCallRef(INVOKE),
    ]);
    let mut arity_error = vec![GlobalGet(0)];
    arity_error.extend(message.iter().map(|unit| I32Const(*unit as i32)));
    arity_error.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: message.len() as u32,
        },
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
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
        &[sentinel_value(NIL), I32Const(0), StructNew(5)],
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
            I32Const(CELL_BOUND),
            I32And,
            If(BlockType::Result(VALUE)),
            sentinel_value(TRUE),
            Else,
            sentinel_value(FALSE),
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
        I32Const(CELL_BOUND),
        I32And,
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
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
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
    // Anonymous publication tests the same selected dynamic/root cell value
    // as an ordinary read. An unbound root must not be read eagerly, while an
    // active dynamic entry may still provide a value for that root cell.
    b.function_with_locals(
        "binding-defined",
        &[VALUE],
        &[ValType::I32],
        &[(1, VALUE), (1, ValType::I32)],
        &[
            LocalGet(0),
            GlobalGet(dynamic::CURRENT),
            Call(dynamic_lookup),
            LocalSet(2),
            LocalSet(1),
            LocalGet(2),
            I32Const(0),
            I32GeS,
            If(BlockType::Result(ValType::I32)),
            LocalGet(1),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            LocalGet(2),
            ArrayGet(ARGS),
            sentinel_value(UNDEFINED),
            RefEq,
            I32Eqz,
            Else,
            LocalGet(0),
            Call(b.names["binding-bound"]),
            sentinel_value(TRUE),
            RefEq,
            If(BlockType::Result(ValType::I32)),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(5)),
            StructGet {
                struct_type_index: 5,
                field_index: 0,
            },
            sentinel_value(UNDEFINED),
            RefEq,
            I32Eqz,
            Else,
            I32Const(0),
            End,
            End,
        ],
    );
    let binding_set = dynamic::binding_set(&mut b, dynamic_lookup);
    dynamic::switching(&mut b);
    let coercion_types = coercions::declare(&mut b);
    let prototype_types = owned_prototypes::declare(&mut b);
    error_constructors::canonical_test_function(&mut b);
    numeric::intrinsics(&mut b, numeric_info, coercion_types);
    let primitives = numeric_operands::intrinsics(&mut b);
    bitwise::intrinsics(&mut b);
    numeric_hash::intrinsics(&mut b);
    identity_hash::intrinsics(&mut b);
    // Casts come before arrays, as packed primitive arrays convert with them.
    let bitwise_functions = bitwise::functions(&mut b);
    packed_arrays::functions(&mut b);
    let mut arithmetic_functions = arithmetic::functions(&mut b, primitives);
    sparse_arrays::functions(&mut b);
    arithmetic_functions.extend(arrays::functions(&mut b));
    arithmetic_functions.extend(nominal::functions(&mut b, generic_invoke));
    let try_invoke = exceptions::functions(&mut b, generic_invoke);
    arithmetic_functions.extend(dynamic::functions(&mut b, binding_set, try_invoke));
    arithmetic_functions.extend(predicates::functions(&mut b));
    arithmetic_functions.extend(comparisons::functions(&mut b, coercion_types));
    arithmetic_functions.extend(bitwise_functions);
    arithmetic_functions.extend(named_properties::functions(&mut b));
    // Original source error adapter: retain the ABI Error descriptor/message
    // rather than substituting nil, an opaque object or a Wasm trap.
    b.function(
        "language-error-new",
        &[VALUE],
        &[VALUE],
        &[
            GlobalGet(nominal::ERROR_GLOBAL),
            LocalGet(0),
            Call(b.names["coerce-string"]),
            RefCastNonNull(HeapType::Concrete(STRING)),
            sentinel_value(NIL),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            StructNew(8),
        ],
    );
    // Native Error adapters use rooted descriptor identity. A structurally
    // identical payload with another descriptor is not a language Error.
    let mut error_test = vec![];
    error_constructors::family_test(&b, &mut error_test, 0);
    b.function("language-error-is", &[VALUE], &[ValType::I32], &error_test);
    native_objects::functions(&mut b);
    native_object_properties::primitives(&mut b);
    native_object_properties::functions(&mut b);
    arrays::set_or_property(&mut b);
    arithmetic_functions.extend(native_object_methods::functions(&mut b));
    arithmetic_functions.extend(native_object_factory::functions(&mut b));
    r#async::intrinsics(&mut b);
    arithmetic_functions.extend(async_library::functions(&mut b));
    arithmetic_functions.extend(primitive_constructors::functions(&mut b));
    let error_callback = error_constructors::callback(&mut b);
    arithmetic_functions.push(error_callback);
    arithmetic_functions.extend(range_errors::functions(&mut b, error_callback));
    error_constructors::functions(&mut b, error_callback);
    arithmetic_functions.extend(error_properties::functions(&mut b));
    let (object_descriptor_functions, object_initialize) = object_descriptors::functions(&mut b);
    arithmetic_functions.extend(object_descriptor_functions);
    arithmetic_functions.extend(exception_info::functions(&mut b));
    type_values::functions(&mut b);
    let prototype_functions = owned_prototypes::functions(&mut b);
    arithmetic_functions.extend(prototype_functions);
    let coercion_functions = coercions::functions(&mut b);
    arithmetic_functions.extend(coercion_functions);
    let mut elements = ElementSection::new();
    elements.declared(Elements::Functions(Cow::Owned(arithmetic_functions)));
    let mut tags = TagSection::new();
    tags.tag(TagType {
        kind: TagKind::Exception,
        func_type_idx: b.next_type,
    });
    b.types.ty().function([VALUE], []);
    // The Macro observer appends a setter after the exception-tag signature.
    b.next_type += 1;
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
                sentinel_value(NIL),
                sentinel_value(NIL),
                sentinel_value(NIL),
                I32Const(0), // Identity UID, unassigned.
                RefI31,
                sentinel_value(NIL), StructNew(DESCRIPTOR),
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
    assert_eq!(globals.len(), FALSE_GLOBAL);
    for (name, tag) in SENTINEL_EXPORTS {
        globals.global(
            GlobalType {
                val_type: reference(SENTINEL),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([I32Const(tag), StructNew(SENTINEL)]),
        );
        b.exports.export(name, ExportKind::Global, globals.len() - 1);
    }
    // Append after the pinned numeric stack global; never relocate its index.
    globals.global(
        GlobalType {
            val_type: ValType::I64,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i64_const(i64::from(numeric::ERROR_GLOBALS) + 5),
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
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
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
            sentinel_value(NIL),
            sentinel_value(NIL),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
        ]),
    );
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([sentinel_value(NIL)]),
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
    ]);
    // The type's name, as compiled descriptors carry theirs (#304).
    let name: Vec<_> = "suss.core.ExceptionInfo".encode_utf16().collect();
    info_descriptor.extend(name.iter().map(|x| I32Const(*x as i32)));
    info_descriptor.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: name.len() as u32,
        },
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        sentinel_value(NIL), StructNew(DESCRIPTOR),
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
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        sentinel_value(NIL), StructNew(DESCRIPTOR),
        sentinel_value(UNDEFINED),
        I32Const(3),
        ArrayNew(ARGS),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
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
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
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
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
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
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
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
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
            I32Const(0),
            ArrayNewDefault(ARGS),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
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
        // lazy-root absent marker, compared with nil on first use.
        &ConstExpr::extended([sentinel_value(NIL)]),
    );
    // Original owned dynamic data-property storage tag, appended privately.
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
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
        ]),
    );
    // Lazy shared default Object prototype, owned by the runtime GC root.
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        // lazy-root absent marker, compared with nil on first use.
        &ConstExpr::extended([sentinel_value(NIL)]),
    );
    // Shared Array.slice function root; no receiver retained.
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        // lazy-root absent marker, compared with nil on first use.
        &ConstExpr::extended([sentinel_value(NIL)]),
    );
    // Scalar-only UID allocator; no global reference table retains owners.
    globals.global(
        GlobalType {
            val_type: ValType::I64,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i64_const(1),
    );
    r#async::append_descriptor(&mut globals);
    streams::append_globals(&mut globals, r#async::STREAM_GLOBAL_BASE);
    type_values::append_globals(&mut globals);
    primitive_constructors::append_globals(&mut globals);
    range_errors::append_globals(&mut globals);
    array_own_properties::append_globals(&mut globals);
    coercions::append_globals(&mut globals, coercion_types, coercion_functions);
    array_join::append_globals(&mut globals);
    string_char_at::append_globals(&mut globals);
    owned_prototypes::append_globals(
        &mut globals,
        prototype_types,
        prototype_functions,
        object_initialize,
        b.names["owned-type-error-implementation"],
    );
    assert_eq!(globals.len(), object_methods::CURRENT_THIS);
    // Append without relocating any existing runtime global or descriptor ID.
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([GlobalGet(object_methods::DEFAULT_THIS)]),
    );
    b.exports.export("array-join-active", ExportKind::Global, array_join::ACTIVE);
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
