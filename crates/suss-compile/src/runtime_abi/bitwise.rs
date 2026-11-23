//! Bit operations and numeric casts (ADR-0003, #336).
//!
//! Public bit operations take longs and operate on 64 bits, as Clojure's do;
//! any other operand throws. Shipped core's hashing keeps the 32-bit
//! ClojureScript operations it was written for, as private `int32-*`
//! helpers that its inline uses lower to. `number-cast` narrows and widens
//! numbers as Clojure's `int`, `long`, `short`, `byte`, `float` and `double`
//! and their `unchecked-` forms do.
use super::*;

/// `number-cast`'s kinds, as core's coercions pass them.
pub(crate) const CASTS: [(&str, Cast); 16] = [
    ("long", Cast::Checked(64)),
    ("int", Cast::Checked(32)),
    ("short", Cast::Checked(16)),
    ("byte", Cast::Checked(8)),
    ("unchecked-long", Cast::Unchecked(64)),
    ("unchecked-int", Cast::Unchecked(32)),
    ("unchecked-short", Cast::Unchecked(16)),
    ("unchecked-byte", Cast::Unchecked(8)),
    ("float", Cast::Float(true)),
    ("unchecked-float", Cast::Float(false)),
    ("double", Cast::Double),
    ("float-long", Cast::FloatLong),
    ("int-object", Cast::ObjectChecked(32)),
    ("short-object", Cast::ObjectChecked(16)),
    ("byte-object", Cast::ObjectChecked(8)),
    ("int-float", Cast::IntFloat),
];
pub(crate) const CAST_RAW_EXPORTS: [&str; 16] = [
    "number-cast-long-raw",
    "number-cast-int-raw",
    "number-cast-short-raw",
    "number-cast-byte-raw",
    "number-cast-unchecked-long-raw",
    "number-cast-unchecked-int-raw",
    "number-cast-unchecked-short-raw",
    "number-cast-unchecked-byte-raw",
    "number-cast-float-raw",
    "number-cast-unchecked-float-raw",
    "number-cast-double-raw",
    "number-cast-float-long-raw",
    "number-cast-int-object-raw",
    "number-cast-short-object-raw",
    "number-cast-byte-object-raw",
    "number-cast-int-float-raw",
];
#[derive(Clone, Copy)]
pub(crate) enum Cast {
    /// A long in range, or a double truncated toward zero; out of range throws.
    Checked(u32),
    /// Object overloads narrow Number.longValue(), retaining the original
    /// operand for byte/short range diagnostics.
    ObjectChecked(u32),
    /// float's comparison rounds Integer/MAX_VALUE to 2^31 before checking.
    IntFloat,
    /// The low bits of a long; a double truncated and saturated, as Java's
    /// primitive casts.
    Unchecked(u32),
    /// Binary32 rounding, returned as a double; checked against the float range.
    Float(bool),
    /// Source float(long) rounds directly; replacement doubles remain checked.
    FloatLong,
    Double,
}

/// Throw a language error whose message is `prefix` and the printed number in
/// local `local`.
fn throw_with_value(b: &Builder, prefix: &str, local: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let units = prefix.encode_utf16().collect::<Vec<_>>();
    let mut code = vec![GlobalGet(nominal::ERROR_GLOBAL)];
    code.extend(units.iter().map(|unit| I32Const(i32::from(*unit))));
    code.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        },
        LocalGet(local),
        Call(b.names["coerce-string"]),
        Call(b.names["string-concat"]),
        RefCastNonNull(HeapType::Concrete(STRING)),
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        StructNew(8),
        Throw(0),
    ]);
    code
}

pub(super) fn intrinsics(b: &mut Builder) {
    use Instruction::*;
    let convert = b.names["coerce-number"];
    // ClojureScript's ToInt32, for core's private 32-bit hashing.
    b.function_with_locals(
        "coerce-int32",
        &[VALUE],
        &[ValType::I32],
        &[(1, ValType::F64)],
        &[
            // ToInt32 of a long is its low 32 bits; no binary64 detour.
            LocalGet(0),
            RefTestNonNull(HeapType::I31),
            If(BlockType::Empty),
            LocalGet(0),
            RefCastNonNull(HeapType::I31),
            I31GetS,
            Return,
            End,
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(LONG)),
            If(BlockType::Empty),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(LONG)),
            StructGet { struct_type_index: LONG, field_index: 0 },
            I32WrapI64,
            Return,
            End,
            LocalGet(0),
            Call(convert),
            F64Trunc,
            LocalSet(1),
            // NaN and either infinity must return zero before a Wasm integer
            // conversion. Signed zero/subnormal inputs also become zero.
            LocalGet(1),
            F64Abs,
            F64Const(f64::INFINITY.into()),
            F64Lt,
            If(BlockType::Result(ValType::I32)),
            // All finite f64 values with magnitude >= 2^84 are multiples of
            // 2^32. Below that threshold these power-of-two operations retain
            // the exact integral remainder, including negative inputs.
            LocalGet(1),
            LocalGet(1),
            F64Const(4294967296.0.into()),
            F64Div,
            F64Floor,
            F64Const(4294967296.0.into()),
            F64Mul,
            F64Sub,
            I32TruncF64U,
            Else,
            I32Const(0),
            End,
        ],
    );
    // A bit operation's operand: a long, as Clojure's bitOpsCast; any other
    // value throws.
    let mut operand = vec![
        LocalGet(0),
        Call(b.names["long-test"]),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["long-unbox"]),
        Return,
        End,
        LocalGet(0),
        Call(b.names["double-test"]),
        If(BlockType::Empty),
    ];
    operand.extend(numeric::error(
        "bit operation not supported for: double",
        nominal::ERROR_GLOBAL,
    ));
    operand.push(End);
    operand.extend(numeric::error(
        "bit operation not supported for a non-number",
        nominal::ERROR_GLOBAL,
    ));
    b.function("bitwise-long-operand", &[VALUE], &[ValType::I64], &operand);
}

/// The body of one cast of the value in local 0. Local 1 is an i64, local 2
/// an f64.
fn cast(b: &Builder, name: &str, kind: Cast, raw: bool) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let name = match name {
        "float-long" => "float", "int-float" => "int",
        other => other.strip_suffix("-object").unwrap_or(other),
    };
    let long = b.names["long-test"];
    let unbox = b.names["long-unbox"];
    let boxed = b.names["long-box"];
    let double = b.names["number-unbox"];
    let mut code = vec![LocalGet(0), Call(b.names["number-test"]), I32Eqz, If(BlockType::Empty)];
    code.extend(numeric::error(
        &format!("{name} requires a number operand"),
        nominal::ERROR_GLOBAL,
    ));
    code.push(End);
    let range = format!("Value out of range for {}: ", name.trim_start_matches("unchecked-"));
    // Checked Ratio casts use its exact BigInteger quotient. Unchecked int
    // (and Number's byte/short narrowing) uses Ratio.intValue: decimal-rounded
    // double followed by Java's saturating int conversion. longValue is exact.
    if let Cast::Checked(bits) | Cast::ObjectChecked(bits) | Cast::Unchecked(bits) = kind {
        code.extend([LocalGet(0), Call(b.names["ratio-test"]), If(BlockType::Empty)]);
        if matches!(kind, Cast::Unchecked(_)) && bits <= 32 {
            code.extend([LocalGet(0), Call(double), I32TruncSatF64S, I64ExtendI32S, LocalSet(1)]);
        } else {
            code.extend([LocalGet(0), Call(b.names["ratio-integer"]), Call(b.names["integer-biginteger"]), LocalSet(3)]);
            if !matches!(kind, Cast::Unchecked(_)) {
                code.extend([LocalGet(3), Call(b.names["bigint-fits-long"]), I32Eqz, If(BlockType::Empty)]);
                code.extend(throw_with_value(b, "Value out of range for long: ", 3)); code.push(End);
            }
            code.extend([LocalGet(3), Call(b.names["bigint-low-long"]), LocalSet(1)]);
        }
        if bits < 64 {
            let shift = I64Const(i64::from(64-bits));
            code.extend([LocalGet(1), shift.clone(), I64Shl, shift, I64ShrS]);
            if !matches!(kind, Cast::Unchecked(_)) {
                code.extend([LocalGet(1), I64Ne, If(BlockType::Empty)]);
                if bits==32 { code.extend(numeric::error("integer overflow", nominal::ERROR_GLOBAL)); }
                else { code.extend(throw_with_value(b, &range, 0)); }
                code.extend([End, LocalGet(1)]);
            }
        } else { code.push(LocalGet(1)); }
        if !raw { code.push(Call(boxed)); }
        code.extend([Return, End]);
    }
    // BigInt/BigInteger must not round through binary64 before integer casts.
    if let Cast::Checked(bits) | Cast::ObjectChecked(bits) | Cast::Unchecked(bits) = kind {
        code.extend([LocalGet(0), Call(b.names["bigint-test"]), If(BlockType::Empty)]);
        if !matches!(kind, Cast::Unchecked(_)) {
            code.extend([LocalGet(0), Call(b.names["bigint-fits-long"]), I32Eqz, If(BlockType::Empty)]);
            code.extend(throw_with_value(b, "Value out of range for long: ", 0)); code.push(End);
        }
        code.extend([LocalGet(0), Call(b.names["bigint-low-long"]), LocalSet(1)]);
        if bits < 64 {
            let shift=I64Const(i64::from(64-bits));
            code.extend([LocalGet(1), shift.clone(), I64Shl, shift, I64ShrS]);
            if !matches!(kind, Cast::Unchecked(_)) {
                code.extend([LocalGet(1), I64Ne, If(BlockType::Empty)]);
                if bits == 32 { code.extend(numeric::error("integer overflow", nominal::ERROR_GLOBAL)); }
                else { code.extend(throw_with_value(b, &range, 0)); }
                code.extend([End, LocalGet(1)]);
            }
        } else { code.push(LocalGet(1)); }
        if !raw { code.push(Call(boxed)); }
        code.extend([Return, End]);
    }
    match kind {
        Cast::Checked(bits) => {
            code.extend([LocalGet(0), Call(long), If(BlockType::Empty)]);
            if bits < 64 {
                let shift = I64Const(i64::from(64 - bits));
                code.extend([
                    LocalGet(0),
                    Call(unbox),
                    LocalTee(1),
                    shift.clone(),
                    I64Shl,
                    shift,
                    I64ShrS,
                    LocalGet(1),
                    I64Ne,
                    If(BlockType::Empty),
                ]);
                // Clojure's intCast of a long is Math.toIntExact.
                if bits == 32 {
                    code.extend(numeric::error("integer overflow", nominal::ERROR_GLOBAL));
                } else {
                    code.extend(throw_with_value(b, &range, 0));
                }
                code.push(End);
            }
            code.push(LocalGet(0));
            if raw { code.push(Call(unbox)); }
            code.extend([Return, End]);
            // A double is compared itself, untruncated. Clojure's long and
            // int casts let NaN through, as 0; short and byte reject it. The
            // long cast's upper limit, 2^63, saturates to Long/MAX_VALUE.
            let (low, high) = if bits == 64 {
                (-9_223_372_036_854_775_808.0, 9_223_372_036_854_775_808.0)
            } else {
                let limit = (1u64 << (bits - 1)) as f64;
                (-limit, limit - 1.0)
            };
            code.extend([LocalGet(0), Call(double), LocalSet(2)]);
            if bits >= 32 {
                code.extend([
                    LocalGet(2),
                    F64Const(low.into()),
                    F64Lt,
                    LocalGet(2),
                    F64Const(high.into()),
                    F64Gt,
                    I32Or,
                ]);
            } else {
                code.extend([
                    LocalGet(2),
                    F64Const(low.into()),
                    F64Ge,
                    LocalGet(2),
                    F64Const(high.into()),
                    F64Le,
                    I32And,
                    I32Eqz,
                ]);
            }
            code.push(If(BlockType::Empty));
            code.extend(throw_with_value(b, &range, 0));
            code.extend([End, LocalGet(2), I64TruncSatF64S]);
            if !raw { code.push(Call(boxed)); }
        }
        Cast::ObjectChecked(bits) => {
            code.extend([LocalGet(0), Call(long), If(BlockType::Result(ValType::I64)),
                LocalGet(0), Call(unbox), Else, LocalGet(0), Call(double), I64TruncSatF64S,
                End, LocalSet(1)]);
            let shift = I64Const(i64::from(64 - bits));
            code.extend([LocalGet(1), shift.clone(), I64Shl, shift, I64ShrS,
                LocalGet(1), I64Ne, If(BlockType::Empty)]);
            if bits == 32 {
                code.extend(numeric::error("integer overflow", nominal::ERROR_GLOBAL));
            } else {
                code.extend(throw_with_value(b, &range, 0));
            }
            code.extend([End, LocalGet(1)]);
            if !raw { code.push(Call(boxed)); }
        }
        Cast::IntFloat => {
            code.extend([LocalGet(0), Call(double), LocalSet(2),
                LocalGet(2), F64Const((-2147483648.0).into()), F64Lt,
                LocalGet(2), F64Const(2147483648.0.into()), F64Gt, I32Or,
                If(BlockType::Empty)]);
            code.extend(throw_with_value(b, &range, 0));
            code.extend([End, LocalGet(2), I32TruncSatF64S, I64ExtendI32S]);
            if !raw { code.push(Call(boxed)); }
        }
        Cast::Unchecked(bits) => {
            code.extend([LocalGet(0), Call(long), If(BlockType::Result(ValType::I64))]);
            code.extend([LocalGet(0), Call(unbox)]);
            code.extend([Else, LocalGet(0), Call(double)]);
            // Java narrows a double to an int, or a long, saturating; the
            // narrower casts then take that int's low bits.
            code.extend(if bits == 64 {
                vec![I64TruncSatF64S]
            } else {
                vec![I32TruncSatF64S, I64ExtendI32S]
            });
            code.push(End);
            if bits < 64 {
                let shift = I64Const(i64::from(64 - bits));
                code.extend([shift.clone(), I64Shl, shift, I64ShrS]);
            }
            if !raw { code.push(Call(boxed)); }
        }
        Cast::Float(false) => {
            // Java's (float) of a long rounds once, straight to binary32.
            code.extend([
                LocalGet(0),
                Call(long),
                If(BlockType::Result(ValType::F32)),
                LocalGet(0),
                Call(unbox),
                F32ConvertI64S,
                Else,
                LocalGet(0),
                Call(double),
                F32DemoteF64,
                End,
                F64PromoteF32,
            ]);
            if !raw { code.push(Call(b.names["number-box"])); }
        }
        Cast::Float(true) | Cast::FloatLong => {
            if matches!(kind, Cast::FloatLong) {
                code.extend([LocalGet(0), Call(long), If(BlockType::Empty),
                    LocalGet(0), Call(unbox), F32ConvertI64S, F64PromoteF32]);
                if !raw { code.push(Call(b.names["number-box"])); }
                code.extend([Return, End]);
            }
            // Generic float(Object) widens a long to a double first.
            code.extend([
                LocalGet(0),
                Call(double),
                LocalTee(2),
                F64Abs,
                F64Const(f64::from(f32::MAX).into()),
                F64Gt,
                If(BlockType::Empty),
            ]);
            code.extend(throw_with_value(b, &range, 0));
            code.extend([End, LocalGet(2), F32DemoteF64, F64PromoteF32]);
            if !raw { code.push(Call(b.names["number-box"])); }
        }
        Cast::Double => {
            code.extend([LocalGet(0), Call(double)]);
            if !raw { code.push(Call(b.names["number-box"])); }
        }
    }
    code
}

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    // Each cast, then `number-cast` selecting one by its index in CASTS.
    let mut casts = Vec::new();
    for (index, (name, kind)) in CASTS.into_iter().enumerate() {
        let body = cast(b, name, kind, false);
        casts.push(b.function_with_locals(
            &format!("number-cast-{name}"),
            &[VALUE],
            &[VALUE],
            &[(1, ValType::I64), (1, ValType::F64), (1, VALUE)],
            &body,
        ));
        let raw_body = cast(b, name, kind, true);
        b.function_with_locals(CAST_RAW_EXPORTS[index], &[VALUE],
            &[if (8..=11).contains(&index) { ValType::F64 } else { ValType::I64 }],
            &[(1, ValType::I64), (1, ValType::F64), (1, VALUE)], &raw_body);
    }
    let mut dispatch = vec![LocalGet(1), Call(b.names["long-test"]), I32Eqz, If(BlockType::Empty)];
    dispatch.extend(numeric::error("Invalid numeric cast", nominal::ERROR_GLOBAL));
    dispatch.extend([End, LocalGet(1), Call(b.names["long-unbox"]), I32WrapI64, LocalSet(2)]);
    for (index, function) in casts.iter().enumerate() {
        dispatch.extend([
            LocalGet(2),
            I32Const(index as i32),
            I32Eq,
            If(BlockType::Empty),
            LocalGet(0),
            Call(*function),
            Return,
            End,
        ]);
    }
    dispatch.extend(numeric::error("Invalid numeric cast", nominal::ERROR_GLOBAL));
    b.function_with_locals(
        "number-cast",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, ValType::I32)],
        &dispatch,
    );

    // Core's private 32-bit operations, as ClojureScript's.
    let int32 = b.names["coerce-int32"];
    for (name, arity) in [
        ("int", 1),
        ("bit-and", 2),
        ("bit-or", 2),
        ("bit-xor", 2),
        ("bit-shift-left", 2),
        ("bit-shift-right", 2),
        ("unsigned-bit-shift-right", 2),
        ("imul", 2),
        ("bit-count", 1),
    ] {
        // An i31 or boxed long operand's int32 is read inline (#387): an
        // i31 is already one, and a long's ToInt32 is its low 32 bits.
        // Anything else converts through coerce-int32.
        let operand = |body: &mut Vec<Instruction<'static>>, local: u32| {
            body.extend([
                LocalGet(local), RefTestNonNull(HeapType::I31), If(BlockType::Result(ValType::I32)),
                LocalGet(local), RefCastNonNull(HeapType::I31), I31GetS,
                Else,
                LocalGet(local), RefTestNonNull(HeapType::Concrete(LONG)), If(BlockType::Result(ValType::I32)),
                LocalGet(local), RefCastNonNull(HeapType::Concrete(LONG)), StructGet { struct_type_index: LONG, field_index: 0 }, I32WrapI64,
                Else,
                LocalGet(local), Call(int32),
                End, End,
            ]);
        };
        let mut body = vec![];
        operand(&mut body, 0);
        if arity == 2 {
            operand(&mut body, 1);
        }
        match name {
            "int" => {}
            "bit-and" => body.push(I32And),
            "bit-or" => body.push(I32Or),
            "bit-xor" => body.push(I32Xor),
            "bit-shift-left" => body.push(I32Shl),
            "bit-shift-right" => body.push(I32ShrS),
            "unsigned-bit-shift-right" => body.push(I32ShrU),
            "imul" => body.push(I32Mul),
            // Core's bit-count is the popcount of its int32 (#443).
            "bit-count" => body.push(I32Popcnt),
            _ => unreachable!(),
        }
        // Raw-return consumers avoid creating a temporary GC result.
        let mut raw_body = body.clone();
        raw_body.push(if name == "unsigned-bit-shift-right" { I64ExtendI32U } else { I64ExtendI32S });
        b.function_with_locals(&format!("int32-{name}-raw"), &vec![VALUE; arity],
            &[ValType::I64], &[(1, ValType::I64)], &raw_body);
        // The boxed entry retains its original canonical long copy.
        body.push(if name == "unsigned-bit-shift-right" { I64ExtendI32U } else { I64ExtendI32S });
        body.extend([
            LocalTee(arity as u32), I64Const(1 << 30), I64Add, I64Const(1 << 31), I64LtU, If(BlockType::Result(VALUE)),
            LocalGet(arity as u32), I32WrapI64, RefI31,
            Else,
            LocalGet(arity as u32), StructNew(LONG),
            End,
        ]);
        b.function_with_locals(&format!("int32-{name}"), &vec![VALUE; arity], &[VALUE], &[(1, ValType::I64)], &body);
    }

    let operand = b.names["bitwise-long-operand"];
    // A variadic source body passes the current canonical var to reduce once.
    // Capturing the function body does not freeze that var's contents.
    let mut reducer_body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(5)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    let message: Vec<_> = "Invalid bitwise reducer cell".encode_utf16().collect();
    reducer_body.extend(message.iter().map(|unit| I32Const(*unit as i32)));
    reducer_body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: message.len() as u32,
        },
        Throw(0),
        End,
        LocalGet(0),
        Call(b.names["binding-get"]),
    ]);
    let reducer = b.function("bitwise-current-reducer", &[VALUE], &[VALUE], &reducer_body);
    let int_cast = b.names["number-cast-int-object"];
    let mut declared = Vec::new();
    for (name, arity, variadic) in [
        ("int", 1, false),
        ("bit-not", 1, false),
        ("bit-and", 2, true),
        ("bit-or", 2, true),
        ("bit-xor", 2, true),
        ("bit-and-not", 2, true),
        ("bit-clear", 2, false),
        ("bit-flip", 2, false),
        ("bit-set", 2, false),
        ("bit-test", 2, false),
        ("bit-shift-left", 2, false),
        ("bit-shift-right", 2, false),
        ("unsigned-bit-shift-right", 2, false),
    ] {
        let body = if name == "int" {
            vec![LocalGet(0), Call(int_cast)]
        } else {
            let mut body = vec![LocalGet(0), Call(operand)];
            if arity == 2 {
                // Wasm's 64-bit shifts take their count's low six bits, as
                // Java's do.
                body.extend([LocalGet(1), Call(operand), LocalSet(2)]);
            }
            match name {
                "bit-not" => body.extend([I64Const(-1), I64Xor]),
                "bit-and" => body.extend([LocalGet(2), I64And]),
                "bit-or" => body.extend([LocalGet(2), I64Or]),
                "bit-xor" => body.extend([LocalGet(2), I64Xor]),
                "bit-and-not" => body.extend([LocalGet(2), I64Const(-1), I64Xor, I64And]),
                "bit-clear" | "bit-flip" | "bit-set" => {
                    body.extend([I64Const(1), LocalGet(2), I64Shl]);
                    match name {
                        "bit-clear" => body.extend([I64Const(-1), I64Xor, I64And]),
                        "bit-flip" => body.push(I64Xor),
                        _ => body.push(I64Or),
                    }
                }
                "bit-test" => body.extend([LocalGet(2), I64ShrU, I64Const(1), I64And, I64Eqz, I32Eqz]),
                "bit-shift-left" => body.extend([LocalGet(2), I64Shl]),
                "bit-shift-right" => body.extend([LocalGet(2), I64ShrS]),
                "unsigned-bit-shift-right" => body.extend([LocalGet(2), I64ShrU]),
                _ => unreachable!(),
            }
            b.function_with_locals(&format!("primitive-{name}-raw"), &vec![VALUE; arity],
                &[if name == "bit-test" { ValType::I32 } else { ValType::I64 }],
                &[(1, ValType::I64)], &body);
            if name == "bit-test" {
                body.extend(super::boolean());
            } else {
                body.push(Call(b.names["long-box"]));
            }
            body
        };
        let helper = b.function_with_locals(
            &format!("primitive-{name}"),
            &vec![VALUE; arity],
            &[VALUE],
            &[(1, ValType::I64)],
            &body,
        );
        let mut callback_body = vec![LocalGet(1), I32Const(0), ArrayGet(ARGS)];
        if arity == 2 {
            callback_body.extend([LocalGet(1), I32Const(1), ArrayGet(ARGS)]);
        }
        callback_body.push(Call(helper));
        if variadic {
            callback_body.extend([
                LocalSet(3),
                // Fixed two-argument calls do not consult the tail reducer.
                LocalGet(1),
                ArrayLen,
                I32Const(2),
                I32GtU,
                If(BlockType::Empty),
                LocalGet(0),
                Call(reducer),
                LocalSet(4),
                End,
                I32Const(2),
                LocalSet(2),
                Block(BlockType::Empty),
                Loop(BlockType::Empty),
                LocalGet(2),
                LocalGet(1),
                ArrayLen,
                I32GeU,
                BrIf(1),
                LocalGet(4),
                LocalGet(3),
                LocalGet(1),
                LocalGet(2),
                ArrayGet(ARGS),
                ArrayNewFixed {
                    array_type_index: ARGS,
                    array_size: 2,
                },
                Call(b.names["invoke"]),
                LocalSet(3),
                LocalGet(2),
                I32Const(1),
                I32Add,
                LocalSet(2),
                Br(0),
                End,
                End,
                LocalGet(3),
            ]);
        }
        let callback = b.count;
        b.functions.function(INVOKE);
        let mut function = Function::new([(1, ValType::I32), (2, VALUE)]);
        for instruction in callback_body {
            function.instruction(&instruction);
        }
        function.instruction(&End);
        b.code.function(&function);
        b.count += 1;
        declared.push(callback);
        let mut factory = vec![];
        if variadic {
            factory.extend([GlobalGet(apply::native_code(name)), sentinel_value(NIL), I32Const(0), RefI31]);
        }
        factory.extend([
            if variadic { LocalGet(0) } else { sentinel_value(NIL) },
            RefFunc(callback), I32Const(arity as i32),
            I32Const(if variadic { -1 } else { arity as i32 }),
            if variadic { StructNew(UNIVERSAL) } else { Call(b.names["closure-new"]) },
        ]);
        b.function(
            &format!("primitive-{name}-function"),
            if variadic { &[VALUE] } else { &[] },
            &[VALUE], &factory,
        );
    }
    declared
}
