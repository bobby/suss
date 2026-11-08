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

/// Clojure's `quot`, `rem`, `abs`, `unchecked-*` and the `long?` and
/// `double?` tests, on operands core has already checked are numbers.
pub(super) fn functions(b: &mut Builder) {
    use Instruction::*;
    let boxed = b.names["long-box"];
    let unbox = b.names["long-unbox"];
    let number = b.names["number-unbox"];
    let locals = [(3, ValType::I64), (3, ValType::F64)];
    // Longs are locals 2 and 3; doubles 5 and 6.
    let operands = |b: &Builder| {
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
    let doubles = vec![
        LocalGet(0),
        Call(number),
        LocalSet(5),
        LocalGet(1),
        Call(number),
        LocalSet(6),
    ];
    let zero = |code: &mut Vec<Instruction<'static>>, test: Vec<Instruction<'static>>| {
        code.extend(test);
        code.push(If(BlockType::Empty));
        code.extend(throw("Divide by zero"));
        code.push(End);
    };
    // quot truncates. Long MIN / -1 wraps to MIN, as in Clojure, rather
    // than trapping; a double quotient is truncated toward zero.
    let mut code = operands(b);
    zero(&mut code, vec![LocalGet(3), I64Eqz]);
    code.extend([
        LocalGet(3),
        I64Const(-1),
        I64Eq,
        If(BlockType::Result(ValType::I64)),
        I64Const(0),
        LocalGet(2),
        I64Sub,
        Else,
        LocalGet(2),
        LocalGet(3),
        I64DivS,
        End,
        Call(boxed),
        Return,
        End,
    ]);
    code.extend(doubles.clone());
    zero(&mut code, vec![LocalGet(6), F64Const(0.0.into()), F64Eq]);
    code.extend([LocalGet(5), LocalGet(6), F64Div, F64Trunc, StructNew(NUMBER)]);
    b.function_with_locals("number-quot", &[VALUE, VALUE], &[VALUE], &locals, &code);
    // rem has the dividend's sign; wasm's rem_s doesn't trap on MIN % -1.
    let mut code = operands(b);
    zero(&mut code, vec![LocalGet(3), I64Eqz]);
    code.extend([LocalGet(2), LocalGet(3), I64RemS, Call(boxed), Return, End]);
    code.extend(doubles);
    zero(&mut code, vec![LocalGet(6), F64Const(0.0.into()), F64Eq]);
    // Clojure: n - trunc(n / d) * d.
    code.extend([
        LocalGet(5),
        LocalGet(5),
        LocalGet(6),
        F64Div,
        F64Trunc,
        LocalGet(6),
        F64Mul,
        F64Sub,
        StructNew(NUMBER),
    ]);
    b.function_with_locals("number-rem", &[VALUE, VALUE], &[VALUE], &locals, &code);
    // abs of Long/MIN_VALUE is itself, as in Clojure.
    let code = vec![
        LocalGet(0),
        Call(b.names["long-test"]),
        If(BlockType::Result(VALUE)),
        LocalGet(0),
        Call(unbox),
        LocalTee(1),
        I64Const(0),
        I64LtS,
        If(BlockType::Result(ValType::I64)),
        I64Const(0),
        LocalGet(1),
        I64Sub,
        Else,
        LocalGet(1),
        End,
        Call(boxed),
        Else,
        LocalGet(0),
        Call(number),
        F64Abs,
        StructNew(NUMBER),
        End,
    ];
    b.function_with_locals("number-abs", &[VALUE], &[VALUE], &[(1, ValType::I64)], &code);
    // unchecked-* wrap longs and are ordinary double arithmetic otherwise.
    for (name, exact, floating) in [
        ("add", I64Add, F64Add),
        ("subtract", I64Sub, F64Sub),
        ("multiply", I64Mul, F64Mul),
    ] {
        let mut code = operands(b);
        code.extend([LocalGet(2), LocalGet(3), exact, Call(boxed), Return, End]);
        code.extend([
            LocalGet(0),
            Call(number),
            LocalGet(1),
            Call(number),
            floating,
            StructNew(NUMBER),
        ]);
        b.function_with_locals(
            &format!("number-unchecked-{name}"),
            &[VALUE, VALUE],
            &[VALUE],
            &locals,
            &code,
        );
    }
    let code = vec![
        LocalGet(0),
        Call(b.names["long-test"]),
        If(BlockType::Result(VALUE)),
        I64Const(0),
        LocalGet(0),
        Call(unbox),
        I64Sub,
        Call(boxed),
        Else,
        LocalGet(0),
        Call(number),
        F64Neg,
        StructNew(NUMBER),
        End,
    ];
    b.function("number-unchecked-negate", &[VALUE], &[VALUE], &code);
    for (name, test) in [("long?", "long-test"), ("double?", "double-test")] {
        let mut code = vec![LocalGet(0), Call(b.names[test])];
        code.extend(super::boolean());
        b.function(&format!("number-{name}"), &[VALUE], &[VALUE], &code);
    }
}

/// `number-parse-long`: the digits `s[from, to)` in `|base|` as a long,
/// negated when `base` is negative; nil when they overflow the long range,
/// which Clojure reads as a BigInt, and false for a digit not in the base.
/// Exact, unlike parsing through a double.
pub(super) fn parse(b: &mut Builder) {
    use Instruction::*;
    let small = |local| [LocalGet(local), RefCastNonNull(i31()), I31GetS];
    // Locals: 4 index, 5 end, 6 base, 7 negative, 8 limit, 9 magnitude,
    // 10 digit, 11 overflowed, 12 unit, 13 letter.
    let mut code = small(1).to_vec();
    code.push(LocalSet(4));
    code.extend(small(2));
    code.push(LocalSet(5));
    code.extend(small(3));
    code.extend([
        I64ExtendI32S,
        LocalTee(6),
        I64Const(0),
        I64LtS,
        LocalTee(7),
        If(BlockType::Empty),
        I64Const(0),
        LocalGet(6),
        I64Sub,
        LocalSet(6),
        End,
        // 2^63 - 1, or 2^63 for a negative literal, compared unsigned.
        I64Const(i64::MAX),
        LocalGet(7),
        I64ExtendI32U,
        I64Add,
        LocalSet(8),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
        LocalGet(5),
        I32GeS,
        BrIf(1),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(4),
        ArrayGetU(STRING),
        LocalTee(12),
        I32Const(48),
        I32Sub,
        I32Const(10),
        I32LtU,
        If(BlockType::Result(ValType::I32)),
        LocalGet(12),
        I32Const(48),
        I32Sub,
        Else,
        // A letter's value, either case; anything else is never a digit.
        LocalGet(12),
        I32Const(32),
        I32Or,
        I32Const(97),
        I32Sub,
        LocalTee(13),
        I32Const(26),
        I32LtU,
        If(BlockType::Result(ValType::I32)),
        LocalGet(13),
        I32Const(10),
        I32Add,
        Else,
        I32Const(99),
        End,
        End,
        I64ExtendI32U,
        LocalTee(10),
        LocalGet(6),
        I64GeU,
        If(BlockType::Empty),
        sentinel_value(FALSE),
        Return,
        End,
        // Once overflowed, keep checking the remaining digits only.
        LocalGet(11),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(9),
        LocalGet(8),
        LocalGet(10),
        I64Sub,
        LocalGet(6),
        I64DivU,
        I64GtU,
        If(BlockType::Empty),
        I32Const(1),
        LocalSet(11),
        Else,
        LocalGet(9),
        LocalGet(6),
        I64Mul,
        LocalGet(10),
        I64Add,
        LocalSet(9),
        End,
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Br(0),
        End,
        End,
        LocalGet(11),
        If(BlockType::Empty),
        sentinel_value(NIL),
        Return,
        End,
        LocalGet(7),
        If(BlockType::Result(ValType::I64)),
        I64Const(0),
        LocalGet(9),
        I64Sub,
        Else,
        LocalGet(9),
        End,
        Call(b.names["long-box"]),
    ]);
    b.function_with_locals(
        "number-parse-long",
        &[VALUE, VALUE, VALUE, VALUE],
        &[VALUE],
        &[
            (2, ValType::I32),
            (1, ValType::I64),
            (1, ValType::I32),
            (3, ValType::I64),
            (3, ValType::I32),
        ],
        &code,
    );
}
