//! Original GC-owned mutable array storage for adapted collection foundations.
use super::*;
pub(super) const TAG_GLOBAL: u32 = closure_properties::TAG_GLOBAL + 1;
// Keep malformed storage/unsupported properties in the language exception path.
fn error(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    let units: Vec<_> = "Invalid or unsupported array operation"
        .encode_utf16()
        .collect();
    body.push(GlobalGet(nominal::ERROR_GLOBAL));
    body.extend(units.iter().map(|unit| I32Const(*unit as i32)));
    body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        },
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        StructNew(8),
        Throw(0),
    ]);
}
// Bootstrap resource bound: limits are diagnostics, never allocator/trap failures.
pub(super) const MAX_LENGTH: i32 = 1_000_000;
fn array(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
fn arg(body: &mut Vec<Instruction<'static>>, index: i32) {
    use Instruction::*;
    body.extend([LocalGet(1), I32Const(index), ArrayGet(ARGS)]);
}
fn callback(b: &mut Builder, locals: &[(u32, ValType)], body: &[Instruction<'static>]) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut code = Function::new(locals.iter().copied());
    for instruction in body {
        code.instruction(instruction);
    }
    code.instruction(&Instruction::End);
    b.code.function(&code);
    b.count += 1;
    index
}
fn factory(
    b: &mut Builder,
    name: &str,
    minimum: i32,
    maximum: i32,
    body: &[Instruction<'static>],
) -> u32 {
    use Instruction::*;
    let f = callback(b, &[], body);
    b.function(
        name,
        &[],
        &[VALUE],
        &[
            I32Const(0),
            RefI31,
            RefFunc(f),
            I32Const(minimum),
            I32Const(maximum),
            Call(b.names["closure-new"]),
        ],
    );
    f
}

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(7)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 0,
        },
        GlobalGet(TAG_GLOBAL),
        RefEq,
    ];
    let is_array = b.function("source-array?", &[VALUE], &[ValType::I32], &body);
    body = vec![LocalGet(0), Call(is_array), I32Eqz, If(BlockType::Empty)];
    error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 1,
        },
        LocalSet(1),
    ]);
    body.extend([
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    array(&mut body, 1);
    body.extend([ArrayLen, I32Const(2), I32Ne, If(BlockType::Empty)]);
    error(&mut body);
    body.extend([End, LocalGet(1)]);
    let fields = b.function_with_locals(
        "source-array-fields",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    body = vec![
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        Call(b.names["source-array-sparse-fields"]),
    ];
    b.function("source-array-backing", &[VALUE], &[VALUE], &body);
    // These owner adapters call a checked sparse operation immediately after
    // extracting field zero. Validate owner shape here and backing in that
    // operation once, rather than walking the same sparse chain twice.
    b.function(
        "source-array-storage",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            Call(fields),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            ArrayGet(ARGS),
            Call(b.names["source-array-sparse-to-args"]),
        ],
    );
    array_properties::functions(b);
    let wrap = b.function(
        "source-array-from-backing",
        &[VALUE],
        &[VALUE],
        &[
            GlobalGet(TAG_GLOBAL),
            LocalGet(0),
            Call(b.names["source-array-sparse-fields"]),
            I32Const(0),
            RefI31,
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 2,
            },
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(7),
        ],
    );
    let create = b.function(
        "source-array-new",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            Call(b.names["source-array-sparse-from-args"]),
            Call(wrap),
        ],
    );
    b.function(
        "source-array-holes-new",
        &[ValType::I32],
        &[VALUE],
        &[
            LocalGet(0),
            Call(b.names["source-array-sparse-new"]),
            Call(wrap),
        ],
    );
    body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        F64ConvertI32U,
        Call(b.names["number-box"]),
        Return,
        End,
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        Call(b.names["source-array-sparse-length"]),
        F64ConvertI32U,
        Call(b.names["number-box"]),
    ];
    let length = b.function("source-array-length", &[VALUE], &[VALUE], &body);
    // Numeric indexed access. Other host-property coercions remain a typed boundary.
    body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalSet(1),
        LocalGet(1),
        F64Const(0.0.into()),
        F64Lt,
        LocalGet(1),
        F64Const(4294967294.0.into()),
        F64Gt,
        I32Or,
        LocalGet(1),
        LocalGet(1),
        F64Trunc,
        F64Ne,
        I32Or,
        If(BlockType::Empty),
        I32Const(-1),
        Return,
        End,
        LocalGet(1),
        I32TruncSatF64U,
    ]);
    let index = b.function_with_locals(
        "source-array-index",
        &[VALUE],
        &[ValType::I32],
        &[(1, ValType::F64)],
        &body,
    );
    let mut property_body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    property_body.extend([
        LocalGet(0),
        Call(b.names["coerce-string"]),
        Call(b.names["source-array-own-key-index"]),
        Return,
    ]);
    property_body.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalSet(1),
        LocalGet(1),
        F64Const(0.0.into()),
        F64Lt,
        LocalGet(1),
        F64Const(4294967295.0.into()),
        F64Gt,
        I32Or,
        LocalGet(1),
        LocalGet(1),
        F64Trunc,
        F64Ne,
        I32Or,
        If(BlockType::Empty),
        I64Const(-1),
        Return,
        End,
        LocalGet(1),
        I64TruncSatF64U,
    ]);
    let property_index = b.function_with_locals(
        "source-array-property-index",
        &[VALUE],
        &[ValType::I64],
        &[(1, ValType::F64)],
        &property_body,
    );
    body = vec![
        // IndexedSeq uses the same numeric access for array and UTF-16 owners.
        // A string read allocates a one-unit string, including lone surrogates.
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        If(BlockType::Empty),
        LocalGet(1),
        Call(index),
        LocalSet(3),
        LocalGet(3),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        I32GeU,
        If(BlockType::Empty),
        I32Const(UNDEFINED),
        RefI31,
        Return,
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(3),
        ArrayGetU(STRING),
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: 1,
        },
        Return,
        End,
        // Canonical numeric index keys have no observable string conversion.
        // Avoid formatting/parsing every internal vector/sequence array read.
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        If(BlockType::Empty),
        LocalGet(1),
        Call(property_index),
        LocalTee(4),
        I64Const(0),
        I64GeS,
        If(BlockType::Empty),
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(4),
        I32WrapI64,
        Call(b.names["source-array-sparse-get"]),
        Return,
        End,
        End,
        LocalGet(1),
        Call(b.names["coerce-string"]),
        LocalSet(1),
        LocalGet(1),
        Call(b.names["source-array-length-key?"]),
        If(BlockType::Empty),
        LocalGet(0),
        Call(length),
        Return,
        End,
        LocalGet(1),
        Call(property_index),
        LocalTee(4),
        I64Const(0),
        I64LtS,
        If(BlockType::Empty),
        LocalGet(0),
        LocalGet(1),
        Call(b.names["source-array-property-get"]),
        Return,
        End,
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(4),
        I32WrapI64,
        Call(b.names["source-array-sparse-get"]),
    ];
    let get = b.function_with_locals(
        "source-array-get",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32), (1, ValType::I64)],
        &body,
    );
    body = vec![
        // Non-index numbers and effectful object keys still take the ordinary
        // checked string-hint property route exactly once.
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        If(BlockType::Empty),
        LocalGet(1),
        Call(property_index),
        LocalTee(3),
        I64Const(0),
        I64GeS,
        If(BlockType::Empty),
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(3),
        I32WrapI64,
        LocalGet(2),
        Call(b.names["source-array-sparse-set"]),
        Return,
        End,
        End,
        LocalGet(1),
        Call(b.names["coerce-string"]),
        LocalSet(1),
        LocalGet(1),
        Call(b.names["source-array-length-key?"]),
        If(BlockType::Empty),
        // ArraySetLength performs separate conversions, even for the same
        // object RHS. First conversion wraps to uint32; second may mutate/throw.
        LocalGet(2),
        Call(b.names["coerce-int32"]),
        F64ConvertI32U,
        LocalSet(4),
        LocalGet(2),
        Call(b.names["coerce-number"]),
        LocalSet(5),
        LocalGet(4),
        LocalGet(5),
        F64Ne,
        If(BlockType::Empty),
    ];
    range_errors::invalid_length(&mut body);
    body.extend([
        End,
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(4),
        I32TruncSatF64U,
        Call(b.names["source-array-sparse-set-length"]),
        LocalGet(2),
        Return,
        End,
        LocalGet(1),
        Call(property_index),
        LocalTee(3),
        I64Const(0),
        I64LtS,
        If(BlockType::Empty),
        LocalGet(0),
        LocalGet(1),
        LocalGet(2),
        Call(b.names["source-array-property-set"]),
        Return,
    ]);
    body.extend([
        End,
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(3),
        I32WrapI64,
        LocalGet(2),
        Call(b.names["source-array-sparse-set"]),
    ]);
    let set = b.function_with_locals(
        "source-array-set",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(1, ValType::I64), (2, ValType::F64)],
        &body,
    );
    // Copy mutable backing directly; materialization would turn holes into values.
    body = vec![
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        Call(b.names["source-array-sparse-clone"]),
        Call(wrap),
    ];
    let clone = b.function("source-array-clone", &[VALUE], &[VALUE], &body);
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(1),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalTee(1),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        If(BlockType::Empty),
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalSet(2),
        LocalGet(2),
        F64Const(0.0.into()),
        F64Lt,
        LocalGet(2),
        F64Const(4294967295.0.into()),
        F64Gt,
        I32Or,
        LocalGet(2),
        LocalGet(2),
        F64Trunc,
        F64Ne,
        I32Or,
        If(BlockType::Empty),
    ]);
    range_errors::invalid_length(&mut body);
    body.extend([
        End,
        LocalGet(2),
        I32TruncSatF64U,
        Call(b.names["source-array-holes-new"]),
        Return,
        End,
        End,
        LocalGet(0),
        Call(create),
    ]);
    b.function_with_locals(
        "source-array-constructor-args",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::F64)],
        &body,
    );
    // Effective Array constructor length used for container budgeting. A sole
    // nonnumeric argument is one element, not a numeric length conversion.
    body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalSet(1),
        LocalGet(1),
        F64Const(0.0.into()),
        F64Lt,
        LocalGet(1),
        F64Const(4294967295.0.into()),
        F64Gt,
        I32Or,
        LocalGet(1),
        LocalGet(1),
        F64Trunc,
        F64Ne,
        I32Or,
        If(BlockType::Empty),
    ];
    range_errors::invalid_length(&mut body);
    body.extend([End, LocalGet(1), I32TruncSatF64U]);
    let dimension = b.function_with_locals(
        "source-array-dimension-size",
        &[VALUE],
        &[ValType::I32],
        &[(1, ValType::F64)],
        &body,
    );
    // All dimensions use the runtime holes convention. Literal one-dimensional
    // macro allocation selects fill=nil through a separate entry point below.
    // Bound total cells in a multidimensional allocation, not just each leaf.
    // Stop at an empty dimension: later invalid sizes are never used by the pin.
    body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    array(&mut body, 0);
    body.extend([
        ArrayLen,
        LocalSet(2),
        LocalGet(1),
        LocalGet(2),
        I32GeU,
        LocalGet(2),
        I32Const(64),
        I32GtU,
        I32Or,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        I64Const(1),
        LocalSet(4),
        I64Const(0),
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
        LocalGet(2),
        I32Const(1),
        I32Sub,
        I32GeU,
        BrIf(1),
    ]);
    array(&mut body, 0);
    body.extend([
        LocalGet(1),
        ArrayGet(ARGS),
        Call(dimension),
        LocalSet(3),
        LocalGet(4),
        LocalGet(3),
        I64ExtendI32U,
        I64Mul,
        LocalTee(4),
        I64Eqz,
        BrIf(1),
        LocalGet(5),
        LocalGet(4),
        I64Add,
        LocalTee(5),
        I64Const(i64::from(MAX_LENGTH)),
        I64GtU,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(1),
        I32Const(1),
        I32Add,
        LocalSet(1),
        Br(0),
        End,
        End,
    ]);
    let check_dimensions = b.function_with_locals(
        "source-array-check-dimensions",
        &[VALUE, ValType::I32],
        &[],
        &[(2, ValType::I32), (2, ValType::I64)],
        &body,
    );
    let make_index = b.count;
    body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    array(&mut body, 0);
    body.extend([ArrayLen, I32Const(64), I32GtU, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    body.push(LocalGet(1));
    array(&mut body, 0);
    body.extend([ArrayLen, I32GeU, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(2),
        I32Const(0),
        I32Ne,
        LocalGet(2),
        I32Const(UNDEFINED),
        I32Ne,
        I32And,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    // Runtime leaf allocation uses canonical Array construction: holes and the
    // full uint32 length domain. Literal nil-filled allocation stays bounded.
    body.extend([
        LocalGet(2),
        I32Const(UNDEFINED),
        I32Eq,
        LocalGet(1),
        I32Const(1),
        I32Add,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Eq,
        I32And,
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(1),
        ArrayGet(ARGS),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        Call(b.names["source-array-constructor-args"]),
        Return,
        End,
        LocalGet(0),
        LocalGet(1),
        Call(check_dimensions),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(1),
        ArrayGet(ARGS),
        LocalSet(3),
        LocalGet(3),
        Call(dimension),
        LocalSet(4),
        LocalGet(4),
        I32Const(MAX_LENGTH),
        I32GtU,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(2),
        I32Const(UNDEFINED),
        I32Eq,
        If(BlockType::Result(VALUE)),
        LocalGet(4),
        Call(b.names["source-array-holes-new"]),
        Else,
        LocalGet(2),
        RefI31,
        LocalGet(4),
        ArrayNew(ARGS),
        Call(create),
        End,
        LocalSet(5),
        LocalGet(1),
        I32Const(1),
        I32Add,
        LocalSet(6),
    ]);
    array(&mut body, 0);
    body.extend([
        ArrayLen,
        LocalGet(6),
        I32GtU,
        If(BlockType::Empty),
        I32Const(0),
        LocalSet(7),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(7),
        LocalGet(4),
        I32GeU,
        BrIf(1),
        LocalGet(5),
        LocalGet(7),
        F64ConvertI32U,
        Call(b.names["number-box"]),
        LocalGet(0),
        LocalGet(6),
        I32Const(UNDEFINED),
        Call(make_index),
        Call(set),
        Drop,
        LocalGet(7),
        I32Const(1),
        I32Add,
        LocalSet(7),
        Br(0),
        End,
        End,
        End,
        LocalGet(5),
    ]);
    let make = b.function_with_locals(
        "source-array-make-dimensions",
        &[VALUE, ValType::I32, ValType::I32],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32), (1, VALUE), (2, ValType::I32)],
        &body,
    );
    body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    array(&mut body, 0);
    body.extend([ArrayLen, I32Const(1), I32Ne, If(BlockType::Empty)]);
    error(&mut body);
    body.extend([End, LocalGet(0), I32Const(0), I32Const(0), Call(make)]);
    b.function("source-array-make-literal", &[VALUE], &[VALUE], &body);
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    array(&mut body, 0);
    body.extend([ArrayLen, I32Const(0), I32Eq, If(BlockType::Empty)]);
    error(&mut body);
    body.extend([End, LocalGet(0)]);
    array(&mut body, 0);
    body.extend([
        ArrayLen,
        I32Const(1),
        I32GtU,
        I32Const(UNDEFINED),
        Call(make),
    ]);
    let make_args = b.function("source-array-make", &[VALUE], &[VALUE], &body);
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    array(&mut body, 0);
    body.extend([ArrayLen, I32Const(1), I32Ne, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    array(&mut body, 0);
    body.extend([I32Const(0), ArrayGet(ARGS), Call(length)]);
    b.function("source-array-length-args", &[VALUE], &[VALUE], &body);
    let mut declared = vec![];
    declared.push(factory(
        b,
        "array-function",
        0,
        -1,
        &[LocalGet(1), Call(create)],
    ));
    let mut body = vec![];
    arg(&mut body, 0);
    body.extend([
        Call(is_array),
        I32Const(2),
        I32Mul,
        I32Const(2),
        I32Add,
        RefI31,
    ]);
    declared.push(factory(b, "array-predicate-function", 1, 1, &body));
    for (name, primitive) in [
        ("array-length-function", length),
        ("array-clone-function", clone),
    ] {
        let mut body = vec![];
        arg(&mut body, 0);
        body.push(Call(primitive));
        declared.push(factory(b, name, 1, 1, &body));
    }
    // Macro and first-class variants share explicit sequential nested traversal.
    for (name, writing) in [
        ("source-array-get-indices", false),
        ("source-array-set-indices", true),
    ] {
        let mut body = vec![
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(ARGS)),
            I32Eqz,
            If(BlockType::Empty),
        ];
        error(&mut body);
        body.push(End);
        array(&mut body, 0);
        body.extend([
            ArrayLen,
            LocalSet(1),
            LocalGet(1),
            I32Const(if writing { 3 } else { 2 }),
            I32LtU,
            If(BlockType::Empty),
        ]);
        error(&mut body);
        body.push(End);
        array(&mut body, 0);
        body.extend([
            I32Const(0),
            ArrayGet(ARGS),
            LocalSet(2),
            I32Const(1),
            LocalSet(3),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(3),
            LocalGet(1),
            I32Const(if writing { 2 } else { 0 }),
            I32Sub,
            I32GeU,
            BrIf(1),
            LocalGet(2),
        ]);
        array(&mut body, 0);
        body.extend([
            LocalGet(3),
            ArrayGet(ARGS),
            Call(get),
            LocalSet(2),
            LocalGet(3),
            I32Const(1),
            I32Add,
            LocalSet(3),
            Br(0),
            End,
            End,
            LocalGet(2),
        ]);
        if writing {
            array(&mut body, 0);
            body.extend([LocalGet(1), I32Const(2), I32Sub, ArrayGet(ARGS)]);
            array(&mut body, 0);
            body.extend([LocalGet(1), I32Const(1), I32Sub, ArrayGet(ARGS), Call(set)]);
        }
        let primitive = b.function_with_locals(
            name,
            &[VALUE],
            &[VALUE],
            &[(1, ValType::I32), (1, VALUE), (1, ValType::I32)],
            &body,
        );
        declared.push(factory(
            b,
            if writing {
                "array-set-function"
            } else {
                "array-get-function"
            },
            if writing { 3 } else { 2 },
            -1,
            &[LocalGet(1), Call(primitive)],
        ));
    }
    declared.push(factory(
        b,
        "array-make-function",
        1,
        -1,
        &[LocalGet(1), Call(make_args)],
    ));
    declared
}
