//! Original long and double representation helpers (ADR-0003, #335).
//!
//! A long is an i31 when it fits in 31 signed bits and a boxed `LONG` struct
//! otherwise, so each long has exactly one representation. A double is a
//! boxed `NUMBER` struct. Every i31 that reaches a value is a long: nil is
//! `ref.null` and the other sentinels are structs.
use super::*;

const I31_MIN: i64 = -(1 << 30);
const I31_SPAN: i64 = 1 << 31;

fn i31() -> HeapType {
    HeapType::I31
}
fn long() -> HeapType {
    HeapType::Concrete(LONG)
}
fn double() -> HeapType {
    HeapType::Concrete(NUMBER)
}

/// `[i64] -> [i32]`: whether a long fits an i31.
pub(super) fn fits_i31() -> [Instruction<'static>; 4] {
    use Instruction::*;
    [I64Const(-I31_MIN), I64Add, I64Const(I31_SPAN), I64LtU]
}

/// `[] -> [i32]`: whether a local holds a number, inline for helpers that
/// can't call `number-test`.
pub(super) fn number_test(local: u32) -> [Instruction<'static>; 8] {
    use Instruction::*;
    [
        LocalGet(local),
        RefTestNonNull(i31()),
        LocalGet(local),
        RefTestNonNull(long()),
        I32Or,
        LocalGet(local),
        RefTestNonNull(double()),
        I32Or,
    ]
}

/// Defines the representation helpers before any runtime code that uses them.
pub(super) fn intrinsics(b: &mut Builder) {
    use Instruction::*;
    let mut code = vec![LocalGet(0)];
    code.extend(fits_i31());
    code.extend([
        If(BlockType::Result(VALUE)),
        LocalGet(0),
        I32WrapI64,
        RefI31,
        Else,
        LocalGet(0),
        StructNew(LONG),
        End,
    ]);
    b.function("long-box", &[ValType::I64], &[VALUE], &code);
    b.function(
        "long-test",
        &[VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            RefTestNonNull(i31()),
            LocalGet(0),
            RefTestNonNull(long()),
            I32Or,
        ],
    );
    b.function(
        "double-test",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), RefTestNonNull(double())],
    );
    b.function(
        "number-test",
        &[VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            RefTestNonNull(i31()),
            LocalGet(0),
            RefTestNonNull(long()),
            I32Or,
            LocalGet(0),
            RefTestNonNull(double()),
            I32Or,
        ],
    );
    // Traps unless the value is a long.
    let long_unbox = b.function(
        "long-unbox",
        &[VALUE],
        &[ValType::I64],
        &[
            LocalGet(0),
            RefTestNonNull(i31()),
            If(BlockType::Result(ValType::I64)),
            LocalGet(0),
            RefCastNonNull(i31()),
            I31GetS,
            I64ExtendI32S,
            Else,
            LocalGet(0),
            RefCastNonNull(long()),
            StructGet {
                struct_type_index: LONG,
                field_index: 0,
            },
            End,
        ],
    );
    // Any number as a double; traps unless the value is a number.
    b.function(
        "number-unbox",
        &[VALUE],
        &[ValType::F64],
        &[
            LocalGet(0),
            RefTestNonNull(double()),
            If(BlockType::Result(ValType::F64)),
            LocalGet(0),
            RefCastNonNull(double()),
            StructGet {
                struct_type_index: NUMBER,
                field_index: 0,
            },
            Else,
            LocalGet(0),
            Call(long_unbox),
            F64ConvertI64S,
            End,
        ],
    );
    // A runtime count, index or code unit computed as an integral double.
    b.function(
        "long-box-f64",
        &[ValType::F64],
        &[VALUE],
        &[LocalGet(0), I64TruncSatF64S, Call(b.names["long-box"])],
    );
    b.function(
        "long-box-i32",
        &[ValType::I32],
        &[VALUE],
        &[LocalGet(0), I64ExtendI32S, Call(b.names["long-box"])],
    );
    b.function(
        "long-box-u32",
        &[ValType::I32],
        &[VALUE],
        &[LocalGet(0), I64ExtendI32U, Call(b.names["long-box"])],
    );
}

fn throw(message: &str) -> Vec<Instruction<'static>> {
    numeric::error(message, nominal::ERROR_GLOBAL)
}
fn both(test: HeapType) -> [Instruction<'static>; 5] {
    use Instruction::*;
    [
        LocalGet(0),
        RefTestNonNull(test),
        LocalGet(1),
        RefTestNonNull(test),
        I32And,
    ]
}
/// `[] -> [i32]`: whether locals 0 and 1 are both longs.
pub(super) fn both_longs(b: &Builder) -> [Instruction<'static>; 5] {
    use Instruction::*;
    let test = b.names["long-test"];
    [LocalGet(0), Call(test), LocalGet(1), Call(test), I32And]
}
fn small(local: u32) -> [Instruction<'static>; 4] {
    use Instruction::*;
    [LocalGet(local), RefCastNonNull(i31()), I31GetS, I64ExtendI32S]
}

/// `[] -> [i32]`: a numeric relation of locals 0 and 1, which are numbers:
/// exact between two longs, and between doubles otherwise.
pub(super) fn relation(
    b: &Builder,
    exact: Instruction<'static>,
    floating: Instruction<'static>,
) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let unbox = b.names["long-unbox"];
    let number = b.names["number-unbox"];
    let mut code = both_longs(b).to_vec();
    code.extend([
        If(BlockType::Result(ValType::I32)),
        LocalGet(0),
        Call(unbox),
        LocalGet(1),
        Call(unbox),
        exact,
        Else,
        LocalGet(0),
        Call(number),
        LocalGet(1),
        Call(number),
        floating,
        End,
    ]);
    code
}

/// Polymorphic `number-add`, `-subtract`, `-multiply`, `-divide` and
/// `-negate` on operands already known to be numbers. Two longs give a long
/// or throw "long overflow"; any double makes the result a double.
pub(super) fn arithmetic(b: &mut Builder) {
    use Instruction::*;
    let boxed = b.names["long-box"];
    let unbox = b.names["long-unbox"];
    let number = b.names["number-unbox"];
    let locals = [(3, ValType::I64)];
    // x, y and r are locals 2, 3 and 4.
    let exact = |b: &Builder| {
        let mut code = both_longs(b).to_vec();
        code.extend([
            If(BlockType::Empty),
            LocalGet(0),
            Call(unbox),
            LocalSet(2),
            LocalGet(1),
            Call(unbox),
            LocalSet(3),
        ]);
        code
    };
    let floating = |operation: Instruction<'static>| {
        vec![
            LocalGet(0),
            Call(number),
            LocalGet(1),
            Call(number),
            operation,
            StructNew(NUMBER),
        ]
    };
    for (name, small_operation, operation, overflow) in [
        (
            "add",
            I64Add,
            F64Add,
            // Signs of both operands differ from the result's.
            vec![
                LocalGet(2),
                LocalGet(4),
                I64Xor,
                LocalGet(3),
                LocalGet(4),
                I64Xor,
                I64And,
                I64Const(0),
                I64LtS,
            ],
        ),
        (
            "subtract",
            I64Sub,
            F64Sub,
            vec![
                LocalGet(2),
                LocalGet(3),
                I64Xor,
                LocalGet(2),
                LocalGet(4),
                I64Xor,
                I64And,
                I64Const(0),
                I64LtS,
            ],
        ),
        (
            "multiply",
            I64Mul,
            F64Mul,
            // Division undoes an exact product; MIN * -1 is the one case where
            // that division itself would trap.
            vec![
                LocalGet(3),
                I64Const(-1),
                I64Eq,
                If(BlockType::Result(ValType::I32)),
                LocalGet(2),
                I64Const(i64::MIN),
                I64Eq,
                Else,
                LocalGet(3),
                I64Eqz,
                If(BlockType::Result(ValType::I32)),
                I32Const(0),
                Else,
                LocalGet(4),
                LocalGet(3),
                I64DivS,
                LocalGet(2),
                I64Ne,
                End,
                End,
            ],
        ),
    ] {
        // Two i31 operands can't overflow 64 bits, even multiplied.
        let mut code = both(i31()).to_vec();
        code.push(If(BlockType::Empty));
        code.extend(small(0));
        code.extend(small(1));
        code.extend([small_operation.clone(), Call(boxed), Return, End]);
        code.extend(exact(b));
        code.extend([LocalGet(2), LocalGet(3), small_operation, LocalSet(4)]);
        code.extend(overflow);
        code.push(If(BlockType::Empty));
        code.extend(throw("long overflow"));
        code.extend([End, LocalGet(4), Call(boxed), Return, End]);
        code.extend(floating(operation));
        b.function_with_locals(&format!("number-{name}"), &[VALUE, VALUE], &[VALUE], &locals, &code);
    }
    // An exact quotient of longs is a long. Other quotients of longs are
    // ratios in Clojure; until Suss has them (#338) they are doubles.
    let mut code = exact(b);
    code.extend([LocalGet(3), I64Eqz, If(BlockType::Empty)]);
    code.extend(throw("Divide by zero"));
    code.extend([
        End,
        LocalGet(2),
        LocalGet(3),
        I64RemS,
        I64Eqz,
        LocalGet(2),
        I64Const(i64::MIN),
        I64Ne,
        LocalGet(3),
        I64Const(-1),
        I64Ne,
        I32Or,
        I32And,
        If(BlockType::Empty),
        LocalGet(2),
        LocalGet(3),
        I64DivS,
        Call(boxed),
        Return,
        End,
        End,
    ]);
    code.extend(floating(F64Div));
    b.function_with_locals("number-divide", &[VALUE, VALUE], &[VALUE], &locals, &code);
    let mut code = vec![
        LocalGet(0),
        Call(b.names["long-test"]),
        If(BlockType::Empty),
        LocalGet(0),
        Call(unbox),
        LocalTee(1),
        I64Const(i64::MIN),
        I64Eq,
        If(BlockType::Empty),
    ];
    code.extend(throw("long overflow"));
    code.extend([
        End,
        I64Const(0),
        LocalGet(1),
        I64Sub,
        Call(boxed),
        Return,
        End,
        LocalGet(0),
        Call(number),
        F64Neg,
        StructNew(NUMBER),
    ]);
    b.function_with_locals("number-negate", &[VALUE], &[VALUE], &[(1, ValType::I64)], &code);
}

/// `long-string`: a long's decimal digits as a new string.
pub(super) fn string(b: &mut Builder) {
    use Instruction::*;
    const UNITS: i32 = 20;
    let code = vec![
        // Local 1 is the sign, 2 the digit buffer, 3 the first digit's index
        // and 4 the unsigned magnitude still to write.
        LocalGet(0),
        I64Const(0),
        I64LtS,
        LocalTee(1),
        If(BlockType::Result(ValType::I64)),
        I64Const(0),
        LocalGet(0),
        I64Sub,
        Else,
        LocalGet(0),
        End,
        LocalSet(4),
        I32Const(UNITS),
        ArrayNewDefault(STRING),
        LocalSet(2),
        I32Const(UNITS),
        LocalSet(3),
        Loop(BlockType::Empty),
        LocalGet(3),
        I32Const(1),
        I32Sub,
        LocalSet(3),
        LocalGet(2),
        LocalGet(3),
        LocalGet(4),
        I64Const(10),
        I64RemU,
        I32WrapI64,
        I32Const(i32::from(b'0')),
        I32Add,
        ArraySet(STRING),
        LocalGet(4),
        I64Const(10),
        I64DivU,
        LocalTee(4),
        I64Const(0),
        I64Ne,
        BrIf(0),
        End,
        LocalGet(1),
        If(BlockType::Empty),
        LocalGet(3),
        I32Const(1),
        I32Sub,
        LocalSet(3),
        LocalGet(2),
        LocalGet(3),
        I32Const(i32::from(b'-')),
        ArraySet(STRING),
        End,
        I32Const(UNITS),
        LocalGet(3),
        I32Sub,
        ArrayNewDefault(STRING),
        LocalTee(5),
        I32Const(0),
        LocalGet(2),
        LocalGet(3),
        I32Const(UNITS),
        LocalGet(3),
        I32Sub,
        ArrayCopy {
            array_type_index_dst: STRING,
            array_type_index_src: STRING,
        },
        LocalGet(5),
    ];
    let string = ValType::Ref(RefType {
        nullable: true,
        heap_type: HeapType::Concrete(STRING),
    });
    b.function_with_locals(
        "long-string",
        &[ValType::I64],
        &[VALUE],
        &[(1, ValType::I32), (1, string), (1, ValType::I32), (1, ValType::I64), (1, string)],
        &code,
    );
}
