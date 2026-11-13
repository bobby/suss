//! Link the pinned, allocator-free Rust numeric helper into the GC runtime.
//! Helper function indices remain unchanged; type indices follow the GC prelude
//! and globals follow the language error descriptors. Nothing is imported.
use super::*;
use wasm_encoder::reencode::Reencode;

const BYTES: &[u8] = include_bytes!("../../../../runtime/numeric/artifact/numeric.wasm");
pub(super) const ERROR_GLOBALS: u32 = 6;

struct Relocate;
impl Reencode for Relocate {
    type Error = core::convert::Infallible;
    fn type_index(
        &mut self,
        index: u32,
    ) -> Result<u32, wasm_encoder::reencode::Error<Self::Error>> {
        Ok(index + TYPE_COUNT)
    }
    fn global_index(
        &mut self,
        index: u32,
    ) -> Result<u32, wasm_encoder::reencode::Error<Self::Error>> {
        Ok(index + ERROR_GLOBALS)
    }
}

pub(super) struct Helper {
    pub types: TypeSection,
    pub functions: FunctionSection,
    pub code: CodeSection,
    pub memory: MemorySection,
    pub data: DataSection,
    pub parse: u32,
    pub format: u32,
    pub stack_global: u32,
    pub stack_top: i32,
    pub scratch_base: i32,
    pub type_count: u32,
    pub function_count: u32,
}

#[derive(Clone, Copy)]
pub(super) struct Info {
    parse: u32,
    format: u32,
    pub stack_global: u32,
    pub stack_top: i32,
    scratch_base: i32,
}
impl Helper {
    pub fn info(&self) -> Info {
        Info {
            parse: self.parse,
            format: self.format,
            stack_global: self.stack_global,
            stack_top: self.stack_top,
            scratch_base: self.scratch_base,
        }
    }
}

pub(super) fn helper() -> Helper {
    helper_from(BYTES)
}
fn helper_from(bytes: &[u8]) -> Helper {
    let mut result = Helper {
        types: prelude(),
        functions: FunctionSection::new(),
        code: CodeSection::new(),
        memory: MemorySection::new(),
        data: DataSection::new(),
        parse: u32::MAX,
        format: u32::MAX,
        stack_global: u32::MAX,
        stack_top: 0,
        scratch_base: 0,
        type_count: TYPE_COUNT,
        function_count: 0,
    };
    let mut relocate = Relocate;
    let mut types = Vec::new();
    let mut functions = Vec::new();
    wasmparser::Validator::new()
        .validate_all(bytes)
        .expect("pinned numeric helper validates");
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.expect("pinned numeric helper parses") {
            wasmparser::Payload::TypeSection(reader) => {
                // This Rust helper only has ordinary function types. Counting
                // individual types avoids conflating a recursive group with one type.
                for group in reader.clone() {
                    let group = group.unwrap();
                    result.type_count += group.types().len() as u32;
                    for ty in group.types() {
                        let wasmparser::CompositeInnerType::Func(ty) = &ty.composite_type.inner
                        else {
                            panic!("numeric helper function types only")
                        };
                        types.push(ty.clone());
                    }
                }
                relocate
                    .parse_type_section(&mut result.types, reader)
                    .unwrap();
            }
            wasmparser::Payload::ImportSection(reader) => {
                assert_eq!(reader.count(), 0, "numeric helper cannot import a host")
            }
            wasmparser::Payload::FunctionSection(reader) => {
                result.function_count = reader.count();
                for ty in reader {
                    let ty = ty.unwrap();
                    functions.push(ty);
                    result.functions.function(ty + TYPE_COUNT);
                }
            }
            wasmparser::Payload::MemorySection(reader) => {
                assert_eq!(reader.count(), 1);
                for memory in reader {
                    let memory = memory.unwrap();
                    assert!(!memory.memory64 && !memory.shared && memory.page_size_log2.is_none());
                    result.scratch_base = i32::try_from(memory.initial * 65536).unwrap();
                    assert!(result.scratch_base > 0);
                    result.memory.memory(relocate.memory_type(memory).unwrap());
                }
            }
            wasmparser::Payload::GlobalSection(reader) => {
                // No allocator state: exactly one mutable Rust stack pointer.
                assert_eq!(reader.count(), 1);
                let global = reader.clone().into_iter().next().unwrap().unwrap();
                assert_eq!(global.ty.content_type, wasmparser::ValType::I32);
                assert!(global.ty.mutable);
                let mut ops = global.init_expr.get_operators_reader();
                let wasmparser::Operator::I32Const { value } = ops.read().unwrap() else {
                    panic!("constant numeric stack")
                };
                assert!(matches!(ops.read().unwrap(), wasmparser::Operator::End));
                result.stack_top = value;
                result.stack_global = ERROR_GLOBALS;
                assert!(ops.eof());
            }
            wasmparser::Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.unwrap();
                    match export.name {
                        "suss_parse_number" | "suss_format_number" => {
                            assert_eq!(export.kind, wasmparser::ExternalKind::Func);
                            if export.name == "suss_parse_number" {
                                result.parse = export.index;
                            } else {
                                result.format = export.index;
                            }
                        }
                        "memory" => {
                            assert_eq!(export.kind, wasmparser::ExternalKind::Memory);
                            assert_eq!(export.index, 0);
                        }
                        _ => panic!("unexpected numeric helper export"),
                    }
                }
            }
            wasmparser::Payload::CodeSectionEntry(body) => relocate
                .parse_function_body(&mut result.code, body)
                .unwrap(),
            wasmparser::Payload::DataSection(reader) => {
                for datum in reader.clone() {
                    let datum = datum.unwrap();
                    let wasmparser::DataKind::Active {
                        memory_index: 0,
                        offset_expr,
                    } = datum.kind
                    else {
                        panic!("numeric helper active data only")
                    };
                    let mut ops = offset_expr.get_operators_reader();
                    let wasmparser::Operator::I32Const { value } = ops.read().unwrap() else {
                        panic!("numeric helper constant data offset")
                    };
                    assert!(
                        value >= 0
                            && value as u64 + datum.data.len() as u64 <= result.scratch_base as u64
                    );
                    assert!(matches!(ops.read().unwrap(), wasmparser::Operator::End) && ops.eof());
                }
                relocate
                    .parse_data_section(&mut result.data, reader)
                    .unwrap();
            }
            wasmparser::Payload::Version {
                encoding: wasmparser::Encoding::Module,
                ..
            }
            | wasmparser::Payload::CodeSectionStart { .. }
            | wasmparser::Payload::CustomSection(_)
            | wasmparser::Payload::End(_) => {}
            _ => panic!("unexpected numeric helper section"),
        }
    }
    assert_ne!(result.parse, u32::MAX);
    assert_ne!(result.format, u32::MAX);
    assert_ne!(result.stack_global, u32::MAX);
    assert!(result.stack_top > 0 && result.stack_top < result.scratch_base);
    for (function, params, results) in [
        (
            result.parse,
            vec![wasmparser::ValType::I32, wasmparser::ValType::I32],
            vec![wasmparser::ValType::F64],
        ),
        (
            result.format,
            vec![wasmparser::ValType::F64, wasmparser::ValType::I32],
            vec![wasmparser::ValType::I32],
        ),
    ] {
        let ty = &types[functions[function as usize] as usize];
        assert_eq!(ty.params(), params);
        assert_eq!(ty.results(), results);
    }
    result
}

/// Coercing arithmetic on two numbers is Clojure's, longs and overflow
/// checks included; only other operands take the JavaScript conversions.
fn numbers(b: &Builder, operation: &str) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let test = b.names["number-test"];
    let mut code = longs::small_call_fast_path(b.names[operation]);
    code.extend([
        LocalGet(0),
        Call(test),
        LocalGet(1),
        Call(test),
        I32And,
        If(BlockType::Empty),
        LocalGet(0),
        LocalGet(1),
        Call(b.names[operation]),
        Return,
        End,
    ]);
    code
}

pub(super) fn error(message: &str, global: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let units = message.encode_utf16().collect::<Vec<_>>();
    let mut code = vec![GlobalGet(global)];
    code.extend(units.iter().map(|unit| I32Const(i32::from(*unit))));
    code.extend([
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
    ]);
    code
}
// Keep the established numeric unsupported-boundary descriptor/message shared
// with the ordered conversion adapter until Function conversion is implemented.
pub(super) fn unsupported_object_error() -> Vec<Instruction<'static>> {
    error("Unsupported arithmetic object coercion", 4)
}

fn text(value: &str) -> Vec<Instruction<'static>> {
    let units = value.encode_utf16().collect::<Vec<_>>();
    let mut code = units
        .iter()
        .map(|unit| Instruction::I32Const(i32::from(*unit)))
        .collect::<Vec<_>>();
    code.push(Instruction::ArrayNewFixed {
        array_type_index: STRING,
        array_size: units.len() as u32,
    });
    code
}
fn check(
    condition: &[Instruction<'static>],
    failure: &[Instruction<'static>],
) -> Vec<Instruction<'static>> {
    let mut code = condition.to_vec();
    code.push(Instruction::If(BlockType::Empty));
    code.extend_from_slice(failure);
    code.push(Instruction::End);
    code
}

pub(super) fn intrinsics(
    b: &mut Builder,
    helper: Info,
    coercion_types: coercions::Types,
) -> [u32; 5] {
    use Instruction::*;
    let number = HeapType::Concrete(NUMBER);
    let string = HeapType::Concrete(STRING);
    let unsupported = unsupported_object_error();
    let allocation = error("Numeric conversion scratch allocation failed", 5);
    let byte = MemArg {
        offset: 0,
        align: 0,
        memory_index: 0,
    };
    let unit = MemArg {
        offset: 0,
        align: 1,
        memory_index: 0,
    };

    // The helper has no allocator. The input/output region begins beyond its
    // initial memory, stack and immutable data, and grows only to a high-water
    // capacity. Bounds are checked before arithmetic and memory.grow.
    let mut reserve = check(
        &[
            LocalGet(0),
            I64Const(4294967296 - i64::from(helper.scratch_base)),
            I64GtU,
        ],
        &allocation,
    );
    reserve.extend([
        LocalGet(0),
        I64Const(i64::from(helper.scratch_base)),
        I64Add,
        I64Const(65535),
        I64Add,
        I64Const(16),
        I64ShrU,
        LocalSet(1),
        LocalGet(1),
        MemorySize(0),
        I64ExtendI32U,
        I64GtU,
        If(BlockType::Empty),
        LocalGet(1),
        MemorySize(0),
        I64ExtendI32U,
        I64Sub,
        I32WrapI64,
        MemoryGrow(0),
        I32Const(-1),
        I32Eq,
        If(BlockType::Empty),
    ]);
    reserve.extend_from_slice(&allocation);
    reserve.extend([End, End, I32Const(helper.scratch_base)]);
    let reserve = b.function_with_locals(
        "numeric-reserve",
        &[ValType::I64],
        &[ValType::I32],
        &[(1, ValType::I64)],
        &reserve,
    );

    let mut convert = vec![
        LocalGet(0),
        Call(b.names["number-test"]),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["number-unbox"]),
        Return,
        End,
    ];
    for (sentinel, value) in [(NIL, 0.0), (FALSE, 0.0), (TRUE, 1.0), (UNDEFINED, f64::NAN)] {
        convert.extend([
            LocalGet(0),
            sentinel_value(sentinel),
            RefEq,
            If(BlockType::Empty),
            F64Const(value.into()),
            Return,
            End,
        ]);
    }
    convert.extend([
        LocalGet(0),
        RefTestNonNull(string),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(string),
        ArrayLen,
        LocalTee(3),
        I64ExtendI32U,
        I64Const(2),
        I64Mul,
        Call(reserve),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
        LocalGet(3),
        I32GeU,
        BrIf(1),
        LocalGet(2),
        LocalGet(1),
        I32Const(1),
        I32Shl,
        I32Add,
        LocalGet(0),
        RefCastNonNull(string),
        LocalGet(1),
        ArrayGetU(STRING),
        I32Store16(unit),
        LocalGet(1),
        I32Const(1),
        I32Add,
        LocalSet(1),
        Br(0),
        End,
        End,
        // A trap can interrupt Rust's stack-pointer restoration. Reset it at
        // each non-reentrant helper call; scratch is freshly overwritten too.
        I32Const(helper.stack_top),
        GlobalSet(helper.stack_global),
        LocalGet(2),
        LocalGet(3),
        Call(helper.parse),
        Return,
        End,
    ]);
    coercions::number(&mut convert, coercion_types);
    let convert = b.function_with_locals(
        "coerce-number",
        &[VALUE],
        &[ValType::F64],
        &[(3, ValType::I32)],
        &convert,
    );

    // A long, an i31 or a boxed one, first (#388): its digits, with no
    // long-test or long-unbox call. An i31 is never a string or a sentinel.
    let long_string = b.names["long-string"];
    let mut to_string = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::I31),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::I31),
        I31GetS,
        I64ExtendI32S,
        ReturnCall(long_string),
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(LONG)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(LONG)),
        StructGet { struct_type_index: LONG, field_index: 0 },
        ReturnCall(long_string),
        End,
        LocalGet(0),
        RefTestNonNull(string),
        If(BlockType::Empty),
        LocalGet(0),
        Return,
        End,
    ];
    for (sentinel, value) in [
        (NIL, "null"),
        (FALSE, "false"),
        (TRUE, "true"),
        (UNDEFINED, "undefined"),
    ] {
        to_string.extend([
            LocalGet(0),
            sentinel_value(sentinel),
            RefEq,
            If(BlockType::Empty),
        ]);
        to_string.extend(text(value));
        to_string.extend([Return, End]);
    }
    to_string.extend([
        LocalGet(0),
        Call(b.names["long-test"]),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["long-unbox"]),
        Call(b.names["long-string"]),
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(number),
        If(BlockType::Empty),
        I64Const(32),
        Call(reserve),
        LocalSet(1),
        I32Const(helper.stack_top),
        GlobalSet(helper.stack_global),
        LocalGet(0),
        RefCastNonNull(number),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalGet(1),
        Call(helper.format),
        LocalSet(2),
    ]);
    to_string.extend(check(&[LocalGet(2), I32Const(32), I32GtU], &allocation));
    to_string.extend([
        LocalGet(2),
        ArrayNewDefault(STRING),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        LocalGet(2),
        I32GeU,
        BrIf(1),
        LocalGet(4),
        RefCastNonNull(string),
        LocalGet(3),
        LocalGet(1),
        LocalGet(3),
        I32Add,
        I32Load8U(byte),
        ArraySet(STRING),
        LocalGet(3),
        I32Const(1),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        LocalGet(4),
        Return,
        End,
    ]);
    coercions::string(&mut to_string, coercion_types);
    let to_string = b.function_with_locals(
        "coerce-string",
        &[VALUE],
        &[VALUE],
        &[(3, ValType::I32), (1, VALUE)],
        &to_string,
    );
    // JS property keys: an integral double names the same property as the
    // equal long ("0" for -0.0, "1" for 1.0), as JS ToString gives, while
    // coerce-string prints doubles as Java does.
    let key = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet { struct_type_index: NUMBER, field_index: 0 },
        LocalTee(1),
        LocalGet(1),
        F64Trunc,
        F64Eq,
        LocalGet(1),
        F64Abs,
        F64Const(9.0e18_f64.into()),
        F64Lt,
        I32And,
        If(BlockType::Empty),
        LocalGet(1),
        I64TruncSatF64S,
        Call(b.names["long-string"]),
        Return,
        End,
        End,
        LocalGet(0),
        Call(to_string),
    ];
    b.function_with_locals("property-key", &[VALUE], &[VALUE], &[(1, ValType::F64)], &key);

    let mut concat = check(
        &[
            LocalGet(0),
            RefTestNonNull(string),
            LocalGet(1),
            RefTestNonNull(string),
            I32And,
            I32Eqz,
        ],
        &error("String concatenation requires string operands", 4),
    );
    concat.extend([
        LocalGet(0),
        RefCastNonNull(string),
        ArrayLen,
        LocalTee(2),
        LocalGet(1),
        RefCastNonNull(string),
        ArrayLen,
        LocalTee(3),
        I32Add,
        LocalSet(4),
    ]);
    concat.extend(check(&[LocalGet(4), LocalGet(2), I32LtU], &allocation));
    concat.extend([
        LocalGet(4),
        ArrayNewDefault(STRING),
        LocalSet(5),
        LocalGet(5),
        RefCastNonNull(string),
        I32Const(0),
        LocalGet(0),
        RefCastNonNull(string),
        I32Const(0),
        LocalGet(2),
        ArrayCopy {
            array_type_index_dst: STRING,
            array_type_index_src: STRING,
        },
        LocalGet(5),
        RefCastNonNull(string),
        LocalGet(2),
        LocalGet(1),
        RefCastNonNull(string),
        I32Const(0),
        LocalGet(3),
        ArrayCopy {
            array_type_index_dst: STRING,
            array_type_index_src: STRING,
        },
        LocalGet(5),
    ]);
    let concat = b.function_with_locals(
        "string-concat",
        &[VALUE, VALUE],
        &[VALUE],
        &[(3, ValType::I32), (1, VALUE)],
        &concat,
    );

    let mut add = vec![];
    coercions::primitive(&mut add, coercion_types, 0, 0);
    coercions::primitive(&mut add, coercion_types, 1, 0);
    add.extend([
        LocalGet(0),
        RefTestNonNull(string),
        LocalGet(1),
        RefTestNonNull(string),
        I32Or,
        If(BlockType::Empty),
        LocalGet(0),
        Call(to_string),
        LocalGet(1),
        Call(to_string),
        Call(concat),
        Return,
        End,
    ]);
    add.extend(numbers(b, "number-add"));
    add.extend([
        LocalGet(0),
        Call(convert),
        LocalGet(1),
        Call(convert),
        F64Add,
        StructNew(NUMBER),
    ]);
    let add = b.function("value-add", &[VALUE, VALUE], &[VALUE], &add);
    let mut binary = Vec::new();
    for (name, operation) in [
        ("subtract", F64Sub),
        ("multiply", F64Mul),
        ("divide", F64Div),
    ] {
        let mut code = numbers(b, &format!("number-{name}"));
        code.extend([
            LocalGet(0),
            Call(convert),
            LocalGet(1),
            Call(convert),
            operation,
            StructNew(NUMBER),
        ]);
        binary.push(b.function(&format!("value-{name}"), &[VALUE, VALUE], &[VALUE], &code));
    }
    let negate = vec![
        LocalGet(0),
        Call(b.names["number-test"]),
        If(BlockType::Result(VALUE)),
        LocalGet(0),
        Call(b.names["number-negate"]),
        Else,
        LocalGet(0),
        Call(convert),
        F64Neg,
        StructNew(NUMBER),
        End,
    ];
    let negate = b.function("value-negate", &[VALUE], &[VALUE], &negate);
    [add, binary[0], binary[1], binary[2], negate]
}
