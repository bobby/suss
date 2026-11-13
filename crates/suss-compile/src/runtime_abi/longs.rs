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

/// The 32-bit form of a signed 64-bit relation, for two i31 operands.
fn small_relation(exact: &Instruction<'static>) -> Instruction<'static> {
    use Instruction::*;
    match exact {
        I64LtS => I32LtS,
        I64LeS => I32LeS,
        I64GtS => I32GtS,
        I64GeS => I32GeS,
        I64Eq => I32Eq,
        _ => unreachable!("signed long relation"),
    }
}
/// Return the relation's boolean at once when locals 0 and 1 are both i31
/// longs, before any type dispatch.
pub(super) fn small_relation_fast_path(exact: &Instruction<'static>) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut code = both(i31()).to_vec();
    code.push(If(BlockType::Empty));
    code.extend([LocalGet(0), RefCastNonNull(i31()), I31GetS]);
    code.extend([LocalGet(1), RefCastNonNull(i31()), I31GetS]);
    code.push(small_relation(exact));
    code.extend(super::boolean());
    code.extend([Return, End]);
    code
}
/// Return `+`, `-` or `*` of locals 0 and 1 inline when both are i31
/// longs, which can't overflow a long: an i31 when the result fits, else a
/// boxed long. `small` is an i32 local and `wide` an i64 local.
pub(super) fn small_arithmetic_fast_path(
    operation: &Instruction<'static>,
    small: u32,
    wide: u32,
) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let get = |local| [LocalGet(local), RefCastNonNull(i31()), I31GetS];
    let mut code = both(i31()).to_vec();
    code.push(If(BlockType::Empty));
    match operation {
        // Two 31-bit operands add or subtract within 32 bits.
        I64Add | I64Sub => {
            code.extend(get(0));
            code.extend(get(1));
            code.extend([
                if matches!(operation, I64Add) { I32Add } else { I32Sub },
                LocalTee(small),
                I32Const(1 << 30),
                I32Add,
                I32Const(i32::MIN),
                I32LtU,
                If(BlockType::Result(VALUE)),
                LocalGet(small),
                RefI31,
                Else,
                LocalGet(small),
                I64ExtendI32S,
                StructNew(LONG),
                End,
            ]);
        }
        I64Mul => {
            code.extend(get(0));
            code.push(I64ExtendI32S);
            code.extend(get(1));
            code.extend([I64ExtendI32S, I64Mul, LocalTee(wide)]);
            code.extend(fits_i31());
            code.extend([
                If(BlockType::Result(VALUE)),
                LocalGet(wide),
                I32WrapI64,
                RefI31,
                Else,
                LocalGet(wide),
                StructNew(LONG),
                End,
            ]);
        }
        _ => unreachable!("small long arithmetic"),
    }
    code.extend([Return, End]);
    code
}
/// Call `callee` on locals 0 and 1 at once when both are i31 longs, before
/// any type dispatch.
pub(super) fn small_call_fast_path(callee: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut code = both(i31()).to_vec();
    code.extend([If(BlockType::Empty), LocalGet(0), LocalGet(1), Call(callee), Return, End]);
    code
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
    let mut code = both(i31()).to_vec();
    code.push(If(BlockType::Result(ValType::I32)));
    code.extend([LocalGet(0), RefCastNonNull(i31()), I31GetS]);
    code.extend([LocalGet(1), RefCastNonNull(i31()), I31GetS]);
    code.extend([small_relation(&exact), Else]);
    code.extend(both_longs(b));
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
        End,
    ]);
    code
}

/// Sets i64 local `raw` to parameter `local`'s long, an i31 or a boxed long,
/// or branches out of the enclosing block for anything else (#385).
fn long_operand(code: &mut Vec<Instruction<'static>>, local: u32, raw: u32) {
    use Instruction::*;
    code.extend([
        LocalGet(local), RefTestNonNull(HeapType::I31), If(BlockType::Result(ValType::I64)),
        LocalGet(local), RefCastNonNull(HeapType::I31), I31GetS, I64ExtendI32S,
        Else,
        LocalGet(local), RefTestNonNull(HeapType::Concrete(LONG)), I32Eqz, BrIf(1),
        LocalGet(local), RefCastNonNull(HeapType::Concrete(LONG)), StructGet { struct_type_index: LONG, field_index: 0 },
        End, LocalSet(raw),
    ]);
}
/// Two longs (i31 or boxed) added or subtracted as i64s, returning the boxed
/// result (#385). Overflow, or any other operand, falls through to the
/// polymorphic path, which converts or throws as before. `x`, `y` and `r` are
/// i64 locals.
pub(super) fn long_arithmetic_fast_path(add: bool, x: u32, y: u32, r: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut code = vec![Block(BlockType::Empty)];
    long_operand(&mut code, 0, x);
    long_operand(&mut code, 1, y);
    if add {
        code.extend([LocalGet(x), LocalGet(y), I64Add, LocalTee(r), LocalGet(x), I64Xor,
            LocalGet(y), LocalGet(r), I64Xor, I64And, I64Const(0), I64LtS]);
    } else {
        code.extend([LocalGet(x), LocalGet(y), I64Sub, LocalTee(r), LocalGet(x), LocalGet(y), I64Xor,
            LocalGet(x), LocalGet(r), I64Xor, I64And, I64Const(0), I64LtS]);
    }
    code.extend([
        BrIf(0),
        LocalGet(r), I64Const(1 << 30), I64Add, I64Const(1 << 31), I64LtU, If(BlockType::Result(VALUE)),
        LocalGet(r), I32WrapI64, RefI31,
        Else,
        LocalGet(r), StructNew(LONG),
        End, Return, End,
    ]);
    code
}
/// Two longs (i31 or boxed) compared as i64s, returning the boolean (#385);
/// anything else falls through. `x` and `y` are i64 locals.
pub(super) fn long_relation_fast_path(exact: &Instruction<'static>, x: u32, y: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut code = vec![Block(BlockType::Empty)];
    long_operand(&mut code, 0, x);
    long_operand(&mut code, 1, y);
    code.extend([LocalGet(x), LocalGet(y), exact.clone()]);
    code.extend(super::boolean());
    code.extend([Return, End]);
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
    // Clojure divides by way of BigIntegers when either long is MIN, so the
    // quotient is a BigInt or a ratio, even (/ Long/MIN_VALUE 1).
    code.extend([
        End,
        LocalGet(2),
        I64Const(i64::MIN),
        I64Eq,
        LocalGet(3),
        I64Const(i64::MIN),
        I64Eq,
        I32Or,
        If(BlockType::Empty),
    ]);
    code.extend(throw(
        "Unsupported quotient: a Long/MIN_VALUE operand gives a BigInt or ratio, which needs Clojure's numeric types (#338)",
    ));
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
    // A u64 has at most 20 digits.
    const DIGITS: u32 = 20;
    // Locals: 1 the sign, 2 the unsigned magnitude still to write, 3 the digit
    // count, then 4.. the digits as code units, least significant first. The
    // string is built at its exact length with array.new_fixed (#388): no
    // zeroed scratch buffer and no copy, each a host call in wasmtime.
    let digit = |k: u32| 4 + k;
    let mut code = vec![
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
        LocalSet(2),
    ];
    // Unrolled: digit k is written, then the loop stops once nothing remains.
    // One division per digit: the digit is the remainder, n - 10q (#388).
    let quotient = 4 + DIGITS;
    code.push(Block(BlockType::Empty));
    for k in 0..DIGITS {
        code.extend([
            LocalGet(2),
            LocalGet(2),
            I64Const(10),
            I64DivU,
            LocalTee(quotient),
            I64Const(10),
            I64Mul,
            I64Sub,
            I32WrapI64,
            I32Const(i32::from(b'0')),
            I32Add,
            LocalSet(digit(k)),
            I32Const(k as i32 + 1),
            LocalSet(3),
            LocalGet(quotient),
            LocalTee(2),
            I64Eqz,
            BrIf(0),
        ]);
    }
    code.push(End);
    // The digit count selects its builder with one br_table (#388): blocks
    // nest so that depth `count - 1` ends just before that count's builder.
    for _ in 0..DIGITS {
        code.push(Block(BlockType::Empty));
    }
    code.push(Block(BlockType::Empty));
    code.extend([LocalGet(3), I32Const(1), I32Sub]);
    code.push(BrTable(Cow::Owned((0..DIGITS).collect()), DIGITS));
    code.push(End);
    for count in 1..=DIGITS {
        for negative in [true, false] {
            if negative {
                code.extend([LocalGet(1), If(BlockType::Empty), I32Const(i32::from(b'-'))]);
            }
            code.extend((0..count).rev().map(|k| LocalGet(digit(k))));
            code.extend([
                ArrayNewFixed { array_type_index: STRING, array_size: count + u32::from(negative) },
                Return,
            ]);
            if negative {
                code.push(End);
            }
        }
        code.push(End);
    }
    code.push(Unreachable);
    let mut locals = vec![(1, ValType::I32), (1, ValType::I64), (1, ValType::I32)];
    locals.push((DIGITS, ValType::I32));
    locals.push((1, ValType::I64));
    b.function_with_locals("long-string", &[ValType::I64], &[VALUE], &locals, &code);
}

/// `string-hash(s)` (#409): `hash` of string `s`, Murmur3's `hashInt` of
/// its Java `hashCode`, boxed: ClojureScript's `(m3-hash-int
/// (hash-string* s))`. hash-string* wraps to int32 only at its next imul, but
/// m3-hash-int converts its operand with ToInt32 first, and the unwrapped sum
/// is below 2^32, so its zero test agrees too. Locals: 1 the string, 2 its
/// length, 3 the index, 4 the hash.
pub(super) fn string_hash(b: &mut Builder) {
    use Instruction::*;
    let typed_string = ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(STRING) });
    let mut code = vec![
        LocalGet(0), RefCastNonNull(HeapType::Concrete(STRING)), LocalTee(1), ArrayLen, LocalSet(2),
        Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(3), LocalGet(2), I32GeU, BrIf(1),
        LocalGet(4), I32Const(31), I32Mul, LocalGet(1), LocalGet(3), ArrayGetU(STRING), I32Add, LocalSet(4),
        LocalGet(3), I32Const(1), I32Add, LocalSet(3), Br(0),
        End, End,
        // m3-hash-int: zero hashes as zero.
        LocalGet(4), If(BlockType::Empty),
        LocalGet(4),
    ];
    code.extend(mix_k1());
    // mixH1 with seed 0: the hash local starts at 0.
    code.extend([I32Const(0), LocalSet(4)]);
    code.extend(mix_h1(4));
    code.extend([
        // fmix(h1, 4)
        LocalGet(4), I32Const(4), I32Xor, LocalTee(4),
        LocalGet(4), I32Const(16), I32ShrU, I32Xor,
        I32Const(0x85ebca6b_u32 as i32), I32Mul, LocalTee(4),
        LocalGet(4), I32Const(13), I32ShrU, I32Xor,
        I32Const(0xc2b2ae35_u32 as i32), I32Mul, LocalTee(4),
        LocalGet(4), I32Const(16), I32ShrU, I32Xor,
        LocalSet(4),
        End,
        LocalGet(4), I32Const(0x4000_0000), I32Add, I32Const(0), I32GeS, If(BlockType::Result(VALUE)),
        LocalGet(4), RefI31,
        Else,
        LocalGet(4), I64ExtendI32S, StructNew(LONG),
        End,
    ]);
    b.function_with_locals("string-hash", &[VALUE], &[VALUE], &[(1, typed_string), (3, ValType::I32)], &code);
}

/// `string-append-long(s, n)` (#388): string `s` followed by the decimal
/// digits of long `n`, an i31 or a boxed long, in one new string, with no
/// intermediate digit string and one copy.
pub(super) fn append(b: &mut Builder) {
    use Instruction::*;
    const DIGITS: u32 = 20;
    // Locals: 2 the sign, 3 the magnitude still to write, 4 the digit count,
    // 5 the quotient, 6 the prefix length, 7 the result, 8 the first digit's
    // index, then 9.. the digits as code units, least significant first.
    let digit = |k: u32| 9 + k;
    let string_ref = ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(STRING) });
    let mut code = vec![
        LocalGet(1), RefTestNonNull(i31()), If(BlockType::Result(ValType::I64)),
        LocalGet(1), RefCastNonNull(i31()), I31GetS, I64ExtendI32S,
        Else,
        LocalGet(1), RefCastNonNull(long()), StructGet { struct_type_index: LONG, field_index: 0 },
        End,
        LocalTee(3), I64Const(0), I64LtS, LocalTee(2),
        If(BlockType::Empty), I64Const(0), LocalGet(3), I64Sub, LocalSet(3), End,
        Block(BlockType::Empty),
    ];
    for k in 0..DIGITS {
        code.extend([
            LocalGet(3), LocalGet(3), I64Const(10), I64DivU, LocalTee(5), I64Const(10), I64Mul, I64Sub,
            I32WrapI64, I32Const(i32::from(b'0')), I32Add, LocalSet(digit(k)),
            I32Const(k as i32 + 1), LocalSet(4),
            LocalGet(5), LocalTee(3), I64Eqz, BrIf(0),
        ]);
    }
    code.extend([
        End,
        LocalGet(0), RefCastNonNull(HeapType::Concrete(STRING)), ArrayLen, LocalTee(6),
        LocalGet(2), I32Add, LocalTee(8), LocalGet(4), I32Add, ArrayNewDefault(STRING), LocalSet(7),
        LocalGet(6), If(BlockType::Empty),
        LocalGet(7), I32Const(0), LocalGet(0), RefCastNonNull(HeapType::Concrete(STRING)), I32Const(0), LocalGet(6),
        ArrayCopy { array_type_index_dst: STRING, array_type_index_src: STRING },
        End,
        LocalGet(2), If(BlockType::Empty),
        LocalGet(7), LocalGet(6), I32Const(i32::from(b'-')), ArraySet(STRING),
        End,
    ]);
    // Digit k goes at first + count - 1 - k, for each k below the count.
    for k in 0..DIGITS {
        code.extend([
            LocalGet(4), I32Const(k as i32), I32GtU, If(BlockType::Empty),
            LocalGet(7), LocalGet(8), LocalGet(4), I32Add, I32Const(k as i32 + 1), I32Sub, LocalGet(digit(k)), ArraySet(STRING),
            End,
        ]);
    }
    code.push(LocalGet(7));
    let mut locals = vec![(1, ValType::I32), (1, ValType::I64), (1, ValType::I32), (1, ValType::I64), (1, ValType::I32), (1, string_ref), (1, ValType::I32)];
    locals.push((DIGITS, ValType::I32));
    b.function_with_locals("string-append-long", &[VALUE, VALUE], &[VALUE], &locals, &code);
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
    // Clojure's double quotient: n / d must be finite ("Infinite or NaN"),
    // and is truncated through a long when it fits, so -0.5 gives 0.0; a
    // larger quotient is already integral. Leaves it in local 7.
    let truncated = |code: &mut Vec<Instruction<'static>>| {
        code.extend([LocalGet(5), LocalGet(6), F64Div, LocalTee(7), F64Abs]);
        code.extend([F64Const(f64::INFINITY.into()), F64Lt, I32Eqz, If(BlockType::Empty)]);
        code.extend(throw("Infinite or NaN"));
        code.extend([
            End,
            LocalGet(7),
            F64Abs,
            F64Const(9_223_372_036_854_775_808.0f64.into()),
            F64Lt,
            If(BlockType::Empty),
            LocalGet(7),
            I64TruncF64S,
            F64ConvertI64S,
            LocalSet(7),
            End,
        ]);
    };
    code.extend(doubles.clone());
    zero(&mut code, vec![LocalGet(6), F64Const(0.0.into()), F64Eq]);
    truncated(&mut code);
    code.extend([LocalGet(7), StructNew(NUMBER)]);
    b.function_with_locals("number-quot", &[VALUE, VALUE], &[VALUE], &locals, &code);
    // rem has the dividend's sign; wasm's rem_s doesn't trap on MIN % -1.
    let mut code = operands(b);
    zero(&mut code, vec![LocalGet(3), I64Eqz]);
    code.extend([LocalGet(2), LocalGet(3), I64RemS, Call(boxed), Return, End]);
    code.extend(doubles);
    zero(&mut code, vec![LocalGet(6), F64Const(0.0.into()), F64Eq]);
    // Clojure: n - quotient * d, so (rem -0.0 2) is -0.0.
    truncated(&mut code);
    code.extend([LocalGet(5), LocalGet(7), LocalGet(6), F64Mul, F64Sub, StructNew(NUMBER)]);
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
    // A long as is; a double truncated toward zero, as Clojure's longCast:
    // outside the long range, infinities included, it throws, and NaN is 0.
    let mut code = vec![LocalGet(0), Call(b.names["long-test"]), If(BlockType::Empty), LocalGet(0), Return, End];
    code.extend([LocalGet(0), Call(b.names["double-test"]), I32Eqz, If(BlockType::Empty)]);
    code.extend(throw("to-long requires a number operand"));
    code.extend([
        End,
        LocalGet(0),
        Call(b.names["number-unbox"]),
        LocalTee(1),
        F64Const(9_223_372_036_854_775_808.0f64.into()),
        F64Ge,
        LocalGet(1),
        F64Const((-9_223_372_036_854_775_808.0f64).into()),
        F64Lt,
        I32Or,
        If(BlockType::Empty),
    ]);
    code.extend(throw("Value out of range for long"));
    code.extend([End, LocalGet(1), I64TruncSatF64S, Call(b.names["long-box"])]);
    b.function_with_locals("number-to-long", &[VALUE], &[VALUE], &[(1, ValType::F64)], &code);
    // The count of an integer range: start, end and a nonzero step are longs.
    // The distance is unsigned, so no subtraction overflows, and a count
    // beyond the long range saturates (Clojure's own count of such a range
    // runs for ever). Locals 3-5 are start, end and step; 6 the distance and
    // 7 the step's magnitude.
    let mut code = vec![];
    for (argument, local) in [(0, 3), (1, 4), (2, 5)] {
        code.extend([LocalGet(argument), Call(unbox), LocalSet(local)]);
    }
    code.extend([
        LocalGet(5),
        I64Const(0),
        I64GtS,
        If(BlockType::Empty),
        LocalGet(4),
        LocalGet(3),
        I64LeS,
        If(BlockType::Empty),
        I32Const(0),
        RefI31,
        Return,
        End,
        LocalGet(4),
        LocalGet(3),
        I64Sub,
        LocalSet(6),
        LocalGet(5),
        LocalSet(7),
        Else,
        LocalGet(4),
        LocalGet(3),
        I64GeS,
        If(BlockType::Empty),
        I32Const(0),
        RefI31,
        Return,
        End,
        LocalGet(3),
        LocalGet(4),
        I64Sub,
        LocalSet(6),
        I64Const(0),
        LocalGet(5),
        I64Sub,
        LocalSet(7),
        End,
        // The unsigned ceiling of distance / magnitude.
        LocalGet(6),
        LocalGet(7),
        I64DivU,
        LocalGet(6),
        LocalGet(7),
        I64RemU,
        I64Const(0),
        I64Ne,
        I64ExtendI32U,
        I64Add,
        LocalTee(6),
        I64Const(0),
        I64LtS,
        If(BlockType::Result(ValType::I64)),
        I64Const(i64::MAX),
        Else,
        LocalGet(6),
        End,
        Call(boxed),
    ]);
    b.function_with_locals(
        "number-range-count",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(5, ValType::I64)],
        &code,
    );
    hash(b);
}

/// Murmur3's `mixK1` of the i32 on the stack.
fn mix_k1() -> [Instruction<'static>; 6] {
    use Instruction::*;
    [
        I32Const(0xcc9e2d51_u32 as i32),
        I32Mul,
        I32Const(15),
        I32Rotl,
        I32Const(0x1b873593),
        I32Mul,
    ]
}
/// Murmur3's `mixH1` of the hash in local `h` and the k1 on the stack.
fn mix_h1(h: u32) -> [Instruction<'static>; 9] {
    use Instruction::*;
    [
        LocalGet(h),
        I32Xor,
        I32Const(13),
        I32Rotl,
        I32Const(5),
        I32Mul,
        I32Const(0xe6546b64_u32 as i32),
        I32Add,
        LocalSet(h),
    ]
}

/// `number-hash`: Clojure's `hasheq` of a number. A long hashes as Murmur3's
/// `hashLong`; a double as `Double.hashCode`, with NaN's canonical bits and
/// -0.0 hashing as 0.0 does, as `Numbers.hasheq` makes it.
fn hash(b: &mut Builder) {
    use Instruction::*;
    // Local 1 is the long or the double's bits, local 2 the hash. A long, an
    // i31 or a boxed one, is read inline, with no long-test or long-unbox
    // call (#387).
    let mut code = vec![
        LocalGet(0),
        RefTestNonNull(i31()),
        LocalGet(0),
        RefTestNonNull(long()),
        I32Or,
        If(BlockType::Result(ValType::I32)),
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
        StructGet { struct_type_index: LONG, field_index: 0 },
        End,
        LocalTee(1),
        I64Eqz,
        If(BlockType::Result(ValType::I32)),
        I32Const(0),
        Else,
        I32Const(0),
        LocalSet(2),
        LocalGet(1),
        I32WrapI64,
    ];
    code.extend(mix_k1());
    code.extend(mix_h1(2));
    code.extend([LocalGet(1), I64Const(32), I64ShrU, I32WrapI64]);
    code.extend(mix_k1());
    code.extend(mix_h1(2));
    code.extend([
        // fmix(h1, 8)
        LocalGet(2),
        I32Const(8),
        I32Xor,
        LocalTee(2),
        LocalGet(2),
        I32Const(16),
        I32ShrU,
        I32Xor,
        I32Const(0x85ebca6b_u32 as i32),
        I32Mul,
        LocalTee(2),
        LocalGet(2),
        I32Const(13),
        I32ShrU,
        I32Xor,
        I32Const(0xc2b2ae35_u32 as i32),
        I32Mul,
        LocalTee(2),
        LocalGet(2),
        I32Const(16),
        I32ShrU,
        I32Xor,
        End,
        Else,
        LocalGet(0),
        RefTestNonNull(double()),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    code.extend(throw("hash-number requires a number operand"));
    code.extend([
        End,
        LocalGet(0),
        RefCastNonNull(double()),
        StructGet { struct_type_index: NUMBER, field_index: 0 },
        LocalTee(3),
        LocalGet(3),
        F64Ne,
        If(BlockType::Result(ValType::I64)),
        I64Const(0x7ff8_0000_0000_0000),
        Else,
        // -0.0 + 0.0 is 0.0, so both zeros hash as 0.0's bits, 0.
        LocalGet(3),
        F64Const(0.0.into()),
        F64Add,
        I64ReinterpretF64,
        End,
        LocalTee(1),
        LocalGet(1),
        I64Const(32),
        I64ShrU,
        I64Xor,
        I32WrapI64,
        End,
        // The hash, boxed inline (#387): an i31 when it fits, else a long.
        LocalTee(2),
        I32Const(0x4000_0000),
        I32Add,
        I32Const(0),
        I32GeS,
        If(BlockType::Result(VALUE)),
        LocalGet(2),
        RefI31,
        Else,
        LocalGet(2),
        I64ExtendI32S,
        StructNew(LONG),
        End,
    ]);
    b.function_with_locals(
        "number-hash",
        &[VALUE],
        &[VALUE],
        &[(1, ValType::I64), (1, ValType::I32), (1, ValType::F64)],
        &code,
    );
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
