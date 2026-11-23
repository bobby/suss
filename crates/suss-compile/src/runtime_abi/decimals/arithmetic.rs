//! Original Decimal arithmetic over the measured immutable BigInt kernels.
use super::*;

pub(super) fn field(local: u32, index: u32) -> [Instruction<'static>; 3] {
    [
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(DECIMAL)),
        StructGet {
            struct_type_index: DECIMAL,
            field_index: index,
        },
    ]
}
pub(super) fn sign(local: u32) -> [Instruction<'static>; 3] {
    [
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(BIGINT)),
        StructGet {
            struct_type_index: BIGINT,
            field_index: 0,
        },
    ]
}
pub(super) fn integer(b: &Builder, value: i64) -> [Instruction<'static>; 2] {
    [I64Const(value), Call(b.names["bigint-from-long"])]
}
fn error(message: &str) -> Vec<Instruction<'static>> {
    numeric::error(message, nominal::ERROR_GLOBAL)
}

pub(super) fn functions(b: &mut Builder) {
    basics(b);
    rounding(b);
    division(b);
    coercion(b);
    super::addition::functions(b);
    operations(b);
    integral(b);
}

fn basics(b: &mut Builder) {
    // Decimal precision counts coefficient digits, with zero precision one.
    let mut code = vec![
        LocalGet(0),
        Call(b.names["integer-biginteger"]),
        Call(b.names["bigint-string"]),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
    ];
    code.extend(sign(0));
    code.extend([I32Const(0), I32LtS, I32Sub]);
    b.function(
        "decimal-integer-precision",
        &[VALUE],
        &[ValType::I32],
        &code,
    );
    // Squaring keeps large-but-reasonable scales logarithmic in the exponent.
    let mut code = integer(b, 1).to_vec();
    code.push(LocalSet(1));
    code.extend(integer(b, 10));
    code.push(LocalSet(2));
    code.extend([
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(0),
        I64Eqz,
        BrIf(1),
        LocalGet(0),
        I64Const(1),
        I64And,
        I64Eqz,
        If(BlockType::Empty),
        Else,
        LocalGet(1),
        LocalGet(2),
        Call(b.names["bigint-multiply"]),
        LocalSet(1),
        End,
        LocalGet(0),
        I64Const(1),
        I64ShrU,
        LocalTee(0),
        I64Eqz,
        BrIf(1),
        LocalGet(2),
        LocalGet(2),
        Call(b.names["bigint-multiply"]),
        LocalSet(2),
        Br(0),
        End,
        End,
        LocalGet(1),
    ]);
    b.function_with_locals(
        "decimal-power-ten",
        &[ValType::I64],
        &[VALUE],
        &[(2, VALUE)],
        &code,
    );
    let mut code = vec![];
    for (bound, op, message) in [
        (i64::from(i32::MIN), I64LtS, "Overflow"),
        (i64::from(i32::MAX), I64GtS, "Underflow"),
    ] {
        code.extend([LocalGet(1), I64Const(bound), op, If(BlockType::Empty)]);
        code.extend(sign(0));
        code.extend([
            I32Eqz,
            If(BlockType::Empty),
            LocalGet(0),
            I32Const(bound as i32),
            Call(b.names["decimal-literal"]),
            Return,
            End,
        ]);
        code.extend(error(message));
        code.push(End);
    }
    code.extend([
        LocalGet(0),
        LocalGet(1),
        I32WrapI64,
        Call(b.names["decimal-literal"]),
    ]);
    b.function("decimal-new", &[VALUE, ValType::I64], &[VALUE], &code);
}

fn rounding(b: &mut Builder) {
    // n0,d1,mode2; q3,rem4; direction5,relation6.
    let mut code = vec![
        LocalGet(0),
        LocalGet(1),
        Call(b.names["bigint-quot"]),
        LocalSet(3),
        LocalGet(0),
        LocalGet(1),
        Call(b.names["bigint-rem"]),
        LocalSet(4),
    ];
    code.extend(sign(4));
    code.extend([
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(3),
        Return,
        End,
        LocalGet(2),
        I32Const(7),
        I32Eq,
        If(BlockType::Empty),
    ]);
    code.extend(error("Rounding necessary"));
    code.push(End);
    code.extend(sign(0));
    code.extend(sign(1));
    code.extend([
        I32Mul,
        LocalSet(5),
        LocalGet(4),
        Call(b.names["bigint-abs"]),
    ]);
    code.extend(integer(b, 2));
    code.extend([
        Call(b.names["bigint-multiply"]),
        LocalGet(1),
        Call(b.names["bigint-abs"]),
        Call(b.names["bigint-compare"]),
        LocalSet(6),
        // UP, CEILING positive, FLOOR negative, or a nearest rounding mode.
        LocalGet(2),
        I32Eqz,
        LocalGet(2),
        I32Const(2),
        I32Eq,
        LocalGet(5),
        I32Const(0),
        I32GtS,
        I32And,
        I32Or,
        LocalGet(2),
        I32Const(3),
        I32Eq,
        LocalGet(5),
        I32Const(0),
        I32LtS,
        I32And,
        I32Or,
        LocalGet(2),
        I32Const(4),
        I32GeU,
        LocalGet(6),
        I32Const(0),
        I32GtS,
        LocalGet(6),
        I32Eqz,
        LocalGet(2),
        I32Const(4),
        I32Eq,
        LocalGet(2),
        I32Const(6),
        I32Eq,
        LocalGet(3),
        Call(b.names["bigint-low-long"]),
        I64Const(1),
        I64And,
        I64Eqz,
        I32Eqz,
        I32And,
        I32Or,
        I32And,
        I32Or,
        I32And,
        I32Or,
        If(BlockType::Result(VALUE)),
        LocalGet(3),
        LocalGet(5),
        I64ExtendI32S,
        Call(b.names["bigint-from-long"]),
        Call(b.names["bigint-add"]),
        Else,
        LocalGet(3),
        End,
    ]);
    b.function_with_locals(
        "decimal-rounded-quotient",
        &[VALUE, VALUE, ValType::I32],
        &[VALUE],
        &[(2, VALUE), (2, ValType::I32)],
        &code,
    );

    // decimal0,p1,mode2; coefficient3, rounded4; precision5,drop6; scale7.
    let mut code = vec![
        LocalGet(1),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(0),
        Return,
        End,
    ];
    code.extend(field(0, 0));
    code.extend([
        LocalTee(3),
        Call(b.names["decimal-integer-precision"]),
        LocalTee(5),
        LocalGet(1),
        I32LeU,
        If(BlockType::Empty),
        LocalGet(0),
        Return,
        End,
        LocalGet(5),
        LocalGet(1),
        I32Sub,
        LocalSet(6),
    ]);
    code.extend(field(0, 1));
    code.extend([
        I64ExtendI32S,
        LocalGet(6),
        I64ExtendI32U,
        I64Sub,
        LocalSet(7),
        LocalGet(3),
        LocalGet(6),
        I64ExtendI32U,
        Call(b.names["decimal-power-ten"]),
        LocalGet(2),
        Call(b.names["decimal-rounded-quotient"]),
        LocalTee(4),
        Call(b.names["decimal-integer-precision"]),
        LocalGet(1),
        I32GtU,
        If(BlockType::Empty),
        LocalGet(4),
    ]);
    code.extend(integer(b, 10));
    code.extend([
        Call(b.names["bigint-quot"]),
        LocalSet(4),
        LocalGet(7),
        I64Const(1),
        I64Sub,
        LocalSet(7),
        End,
        LocalGet(4),
        LocalGet(7),
        Call(b.names["decimal-new"]),
    ]);
    b.function_with_locals(
        "decimal-round",
        &[VALUE, ValType::I32, ValType::I32],
        &[VALUE],
        &[(2, VALUE), (2, ValType::I32), (1, ValType::I64)],
        &code,
    );
}

fn division(b: &mut Builder) {
    // Filled below with exact factorization and finite-precision division.
    division_core(b);
}

fn coercion(b: &mut Builder) {
    let mut code = vec![
        LocalGet(0),
        Call(b.names["decimal-test"]),
        If(BlockType::Empty),
        LocalGet(0),
        Return,
        End,
        LocalGet(0),
        Call(b.names["ratio-test"]),
        If(BlockType::Empty),
    ];
    for index in [0, 1] {
        code.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(RATIO)),
            StructGet {
                struct_type_index: RATIO,
                field_index: index,
            },
            I32Const(0),
            Call(b.names["decimal-literal"]),
        ]);
    }
    code.extend([
        LocalGet(1),
        LocalGet(2),
        ReturnCall(b.names["decimal-divide-core"]),
        End,
        LocalGet(0),
        Call(b.names["integer-bigint"]),
        I32Const(0),
        Call(b.names["decimal-literal"]),
    ]);
    b.function(
        "decimal-from-number",
        &[VALUE, ValType::I32, ValType::I32],
        &[VALUE],
        &code,
    );
    b.function(
        "decimal-coerce",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0), Call(b.names["ratio-test"]), If(BlockType::Result(VALUE)),
            LocalGet(0), Call(b.names["decimal-context"]), Call(b.names["decimal-from-number"]),
            Else,
            // Integer and existing Decimal coercions are exact and never read
            // the dynamic context. Only Ratio conversion needs its division.
            LocalGet(0), I32Const(0), I32Const(4), Call(b.names["decimal-from-number"]), End,
        ],
    );
}

fn operations(b: &mut Builder) {
    for operation in ["add", "subtract", "multiply", "divide"] {
        // x0,y1; converted2,3; p4,mode5; scale6; result coefficient7.
        let mut code = vec![
            Call(b.names["decimal-context"]),
            LocalSet(5),
            LocalSet(4),
            LocalGet(0),
            LocalGet(4),
            LocalGet(5),
            Call(b.names["decimal-from-number"]),
            LocalSet(2),
            LocalGet(1),
            LocalGet(4),
            LocalGet(5),
            Call(b.names["decimal-from-number"]),
            LocalSet(3),
        ];
        if operation == "add" {
            code.extend([LocalGet(2), LocalGet(3), LocalGet(4), LocalGet(5),
                Call(b.names["decimal-add-rounded"])]);
        } else if operation == "divide" {
            code.extend([
                LocalGet(2),
                LocalGet(3),
                LocalGet(4),
                LocalGet(5),
                Call(b.names["decimal-divide-core"]),
            ]);
        } else {
            if operation == "multiply" {
                code.extend(field(2, 1));
                code.push(I64ExtendI32S);
                code.extend(field(3, 1));
                code.extend([I64ExtendI32S, I64Add, LocalSet(6)]);
                code.extend(field(2, 0));
                code.extend(field(3, 0));
                code.extend([Call(b.names["bigint-multiply"]), LocalSet(7)]);
            } else {
                code.extend(field(2, 1));
                code.extend(field(3, 1));
                code.extend([I32GtS, If(BlockType::Result(ValType::I32))]);
                code.extend(field(2, 1));
                code.push(Else);
                code.extend(field(3, 1));
                code.extend([End, I64ExtendI32S, LocalSet(6)]);
                for local in [2, 3] {
                    code.extend(field(local, 0));
                    code.extend([LocalGet(6)]);
                    code.extend(field(local, 1));
                    code.extend([
                        I64ExtendI32S,
                        I64Sub,
                        Call(b.names["decimal-power-ten"]),
                        Call(b.names["bigint-multiply"]),
                    ]);
                }
                code.extend([Call(b.names[&format!("bigint-{operation}")]), LocalSet(7)]);
            }
            code.extend([
                LocalGet(7),
                LocalGet(6),
                Call(b.names["decimal-new"]),
                LocalGet(4),
                LocalGet(5),
                Call(b.names["decimal-round"]),
            ]);
        }
        b.function_with_locals(
            &format!("decimal-{operation}"),
            &[VALUE, VALUE],
            &[VALUE],
            &[(2, VALUE), (2, ValType::I32), (1, ValType::I64), (1, VALUE)],
            &code,
        );
    }
    let mut code = field(0, 0).to_vec();
    code.extend([Call(b.names["bigint-negate"])]);
    code.extend(field(0, 1));
    code.extend([
        Call(b.names["decimal-literal"]),
        LocalSet(1),
        LocalGet(1),
        Call(b.names["decimal-context"]),
        Call(b.names["decimal-round"]),
    ]);
    b.function_with_locals("decimal-negate", &[VALUE], &[VALUE], &[(1, VALUE)], &code);
    let mut code = field(0, 0).to_vec();
    code.extend([Call(b.names["bigint-abs"])]);
    code.extend(field(0, 1));
    code.extend([
        Call(b.names["decimal-literal"]),
        LocalSet(1),
        LocalGet(1),
        Call(b.names["decimal-context"]),
        Call(b.names["decimal-round"]),
    ]);
    b.function_with_locals("decimal-abs", &[VALUE], &[VALUE], &[(1, VALUE)], &code);
}

fn division_core(b: &mut Builder) {
    // x0,y1,p2,mode3; n4,d5,scratch6,reduced denominator7,q8;
    // preferred9,scale10,shift11,twos12,fives13,ratio exponent14 (all i64).
    let mut code = field(0, 0).to_vec();
    code.push(LocalSet(4));
    code.extend(field(1, 0));
    code.push(LocalSet(5));
    code.extend(sign(5));
    code.extend([I32Eqz, If(BlockType::Empty)]);
    code.extend(error("Divide by zero"));
    code.push(End);
    code.extend(field(0, 1));
    code.push(I64ExtendI32S);
    code.extend(field(1, 1));
    code.extend([I64ExtendI32S, I64Sub, LocalSet(9)]);
    code.extend(sign(4));
    code.extend([
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(4),
        LocalGet(9),
        ReturnCall(b.names["decimal-new"]),
        End,
    ]);
    code.extend(sign(5));
    code.extend([
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
        LocalGet(4),
        Call(b.names["bigint-negate"]),
        LocalSet(4),
        LocalGet(5),
        Call(b.names["bigint-negate"]),
        LocalSet(5),
        End,
        LocalGet(2),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(4),
        LocalGet(5),
        Call(b.names["bigint-gcd"]),
        LocalSet(6),
        LocalGet(4),
        LocalGet(6),
        Call(b.names["bigint-quot"]),
        LocalSet(4),
        LocalGet(5),
        LocalGet(6),
        Call(b.names["bigint-quot"]),
        LocalTee(5),
        LocalSet(7),
    ]);
    for (prime, counter) in [(2, 12), (5, 13)] {
        code.extend([Block(BlockType::Empty), Loop(BlockType::Empty), LocalGet(5)]);
        code.extend(integer(b, prime));
        code.extend([Call(b.names["bigint-rem"]), LocalSet(6)]);
        code.extend(sign(6));
        code.extend([BrIf(1), LocalGet(5)]);
        code.extend(integer(b, prime));
        code.extend([
            Call(b.names["bigint-quot"]),
            LocalSet(5),
            LocalGet(counter),
            I64Const(1),
            I64Add,
            LocalSet(counter),
            Br(0),
            End,
            End,
        ]);
    }
    code.extend([LocalGet(5)]);
    code.extend(integer(b, 1));
    code.extend([
        Call(b.names["bigint-compare"]),
        I32Eqz,
        If(BlockType::Empty),
        Else,
    ]);
    code.extend(error(
        "Non-terminating decimal expansion; no exact representable decimal result.",
    ));
    code.push(End);
    code.extend([
        LocalGet(12),
        LocalGet(13),
        I64GtU,
        If(BlockType::Result(ValType::I64)),
        LocalGet(12),
        Else,
        LocalGet(13),
        End,
        LocalSet(11),
        LocalGet(9),
        LocalGet(11),
        I64Add,
        LocalSet(10),
        LocalGet(10),
        I64Const(i64::from(i32::MIN)),
        I64LtS,
        LocalGet(10),
        I64Const(i64::from(i32::MAX)),
        I64GtS,
        I32Or,
        If(BlockType::Empty),
    ]);
    code.extend(error(
        "Non-terminating decimal expansion; no exact representable decimal result.",
    ));
    code.push(End);
    code.extend([
        LocalGet(4),
        LocalGet(11),
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        LocalGet(7),
        Call(b.names["bigint-quot"]),
        LocalGet(10),
        I32WrapI64,
        ReturnCall(b.names["decimal-literal"]),
        End,
        LocalGet(4),
        Call(b.names["decimal-integer-precision"]),
        I64ExtendI32U,
        LocalGet(5),
        Call(b.names["decimal-integer-precision"]),
        I64ExtendI32U,
        I64Sub,
        LocalSet(14),
        LocalGet(14),
        I64Const(0),
        I64GeS,
        If(BlockType::Result(ValType::I32)),
        LocalGet(4),
        Call(b.names["bigint-abs"]),
        LocalGet(5),
        LocalGet(14),
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        Call(b.names["bigint-compare"]),
        Else,
        LocalGet(4),
        Call(b.names["bigint-abs"]),
        I64Const(0),
        LocalGet(14),
        I64Sub,
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        LocalGet(5),
        Call(b.names["bigint-compare"]),
        End,
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
        LocalGet(14),
        I64Const(1),
        I64Sub,
        LocalSet(14),
        End,
        LocalGet(2),
        I64ExtendI32U,
        I64Const(1),
        I64Sub,
        LocalGet(14),
        I64Sub,
        LocalSet(11),
        LocalGet(9),
        LocalGet(11),
        I64Add,
        LocalSet(10),
        LocalGet(11),
        I64Const(0),
        I64GeS,
        If(BlockType::Result(VALUE)),
        LocalGet(4),
        LocalGet(11),
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        LocalGet(5),
        LocalGet(3),
        Call(b.names["decimal-rounded-quotient"]),
        Else,
        LocalGet(4),
        LocalGet(5),
        I64Const(0),
        LocalGet(11),
        I64Sub,
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        LocalGet(3),
        Call(b.names["decimal-rounded-quotient"]),
        End,
        LocalSet(8),
        LocalGet(8),
        Call(b.names["decimal-integer-precision"]),
        LocalGet(2),
        I32GtU,
        If(BlockType::Empty),
        LocalGet(8),
    ]);
    code.extend(integer(b, 10));
    code.extend([
        Call(b.names["bigint-quot"]),
        LocalSet(8),
        LocalGet(10),
        I64Const(1),
        I64Sub,
        LocalSet(10),
        End,
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(10),
        LocalGet(9),
        I64LeS,
        BrIf(1),
        LocalGet(8),
    ]);
    code.extend(integer(b, 10));
    code.extend([Call(b.names["bigint-rem"]), LocalSet(6)]);
    code.extend(sign(6));
    code.extend([BrIf(1), LocalGet(8)]);
    code.extend(integer(b, 10));
    code.extend([
        Call(b.names["bigint-quot"]),
        LocalSet(8),
        LocalGet(10),
        I64Const(1),
        I64Sub,
        LocalSet(10),
        Br(0),
        End,
        End,
        LocalGet(8),
        LocalGet(10),
        Call(b.names["decimal-new"]),
    ]);
    b.function_with_locals(
        "decimal-divide-core",
        &[VALUE, VALUE, ValType::I32, ValType::I32],
        &[VALUE],
        &[(5, VALUE), (6, ValType::I64)],
        &code,
    );
}

fn integral(b: &mut Builder) {
    // Exact multiply/subtract are needed by remainder, which must not round
    // its product or difference to the context's significant-digit precision.
    for operation in ["multiply", "subtract"] {
        let mut code = vec![];
        if operation == "multiply" {
            code.extend(field(0, 1));
            code.push(I64ExtendI32S);
            code.extend(field(1, 1));
            code.extend([I64ExtendI32S, I64Add, LocalSet(2)]);
            code.extend(field(0, 0));
            code.extend(field(1, 0));
            code.push(Call(b.names["bigint-multiply"]));
        } else {
            code.extend(field(0, 1));
            code.extend(field(1, 1));
            code.push(I32GtS);
            code.push(If(BlockType::Result(ValType::I32)));
            code.extend(field(0, 1));
            code.push(Else);
            code.extend(field(1, 1));
            code.extend([End, I64ExtendI32S, LocalSet(2)]);
            for local in [0, 1] {
                code.extend(field(local, 0));
                code.push(LocalGet(2));
                code.extend(field(local, 1));
                code.extend([
                    I64ExtendI32S,
                    I64Sub,
                    Call(b.names["decimal-power-ten"]),
                    Call(b.names["bigint-multiply"]),
                ]);
            }
            code.push(Call(b.names["bigint-subtract"]));
        }
        code.extend([LocalGet(2), Call(b.names["decimal-new"])]);
        b.function_with_locals(
            &format!("decimal-exact-{operation}"),
            &[VALUE, VALUE],
            &[VALUE],
            &[(1, ValType::I64)],
            &code,
        );
    }
    // x0,y1,p2; n3,d4,q5,scratch6; preferred7,scale8,append9; precision10.
    let mut code = field(0, 0).to_vec();
    code.push(LocalSet(3));
    code.extend(field(1, 0));
    code.push(LocalSet(4));
    code.extend(sign(4));
    code.extend([I32Eqz, If(BlockType::Empty)]);
    code.extend(error("Divide by zero"));
    code.push(End);
    code.extend(field(0, 1));
    code.push(I64ExtendI32S);
    code.extend(field(1, 1));
    code.extend([
        I64ExtendI32S,
        I64Sub,
        LocalSet(7),
        LocalGet(7),
        I64Const(0),
        I64GeS,
        If(BlockType::Result(VALUE)),
        LocalGet(7),
        LocalGet(3),
        Call(b.names["decimal-integer-precision"]),
        I64ExtendI32U,
        I64GeU,
        If(BlockType::Result(VALUE)),
    ]);
    code.extend(integer(b, 0));
    code.extend([
        Else,
        LocalGet(3),
        LocalGet(4),
        LocalGet(7),
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        Call(b.names["bigint-quot"]),
        End,
        Else,
        LocalGet(3),
        I64Const(0),
        LocalGet(7),
        I64Sub,
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        LocalGet(4),
        Call(b.names["bigint-quot"]),
        End,
        LocalSet(5),
    ]);
    code.extend(sign(5));
    code.extend([
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(5),
        LocalGet(7),
        ReturnCall(b.names["decimal-new"]),
        End,
        LocalGet(5),
        Call(b.names["decimal-integer-precision"]),
        LocalSet(10),
        LocalGet(2),
        If(BlockType::Empty),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(10),
        LocalGet(2),
        I32LeU,
        BrIf(1),
        LocalGet(5),
    ]);
    code.extend(integer(b, 10));
    code.extend([Call(b.names["bigint-rem"]), LocalSet(6)]);
    code.extend(sign(6));
    code.push(If(BlockType::Empty));
    code.extend(error("Division impossible"));
    code.push(End);
    code.push(LocalGet(5));
    code.extend(integer(b, 10));
    code.extend([
        Call(b.names["bigint-quot"]),
        LocalSet(5),
        LocalGet(8),
        I64Const(1),
        I64Sub,
        LocalSet(8),
        LocalGet(10),
        I32Const(1),
        I32Sub,
        LocalSet(10),
        Br(0),
        End,
        End,
        End,
        LocalGet(8),
        LocalGet(7),
        I64LtS,
        If(BlockType::Empty),
        LocalGet(7),
        LocalGet(8),
        I64Sub,
        LocalSet(9),
        LocalGet(2),
        If(BlockType::Empty),
        LocalGet(2),
        LocalGet(10),
        I32Sub,
        I64ExtendI32U,
        LocalGet(9),
        I64LtU,
        If(BlockType::Empty),
        LocalGet(2),
        LocalGet(10),
        I32Sub,
        I64ExtendI32U,
        LocalSet(9),
        End,
        End,
        LocalGet(5),
        LocalGet(9),
        Call(b.names["decimal-power-ten"]),
        Call(b.names["bigint-multiply"]),
        LocalSet(5),
        LocalGet(8),
        LocalGet(9),
        I64Add,
        LocalSet(8),
        Else,
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(8),
        LocalGet(7),
        I64LeS,
        BrIf(1),
        LocalGet(5),
    ]);
    code.extend(integer(b, 10));
    code.extend([Call(b.names["bigint-rem"]), LocalSet(6)]);
    code.extend(sign(6));
    code.extend([BrIf(1), LocalGet(5)]);
    code.extend(integer(b, 10));
    code.extend([
        Call(b.names["bigint-quot"]),
        LocalSet(5),
        LocalGet(8),
        I64Const(1),
        I64Sub,
        LocalSet(8),
        Br(0),
        End,
        End,
        End,
        LocalGet(5),
        LocalGet(8),
        Call(b.names["decimal-new"]),
    ]);
    b.function_with_locals(
        "decimal-quot-core",
        &[VALUE, VALUE, ValType::I32],
        &[VALUE],
        &[(4, VALUE), (3, ValType::I64), (1, ValType::I32)],
        &code,
    );
    for operation in ["quot", "rem"] {
        // x0,y1; converted2,3,quotient4; p5,mode6.
        let mut code = vec![
            Call(b.names["decimal-context"]),
            LocalSet(6),
            LocalSet(5),
            LocalGet(0),
            LocalGet(5),
            LocalGet(6),
            Call(b.names["decimal-from-number"]),
            LocalSet(2),
            LocalGet(1),
            LocalGet(5),
            LocalGet(6),
            Call(b.names["decimal-from-number"]),
            LocalSet(3),
            LocalGet(2),
            LocalGet(3),
            LocalGet(5),
            Call(b.names["decimal-quot-core"]),
        ];
        if operation == "rem" {
            code.extend([
                LocalSet(4),
                LocalGet(2),
                LocalGet(4),
                LocalGet(3),
                Call(b.names["decimal-exact-multiply"]),
                Call(b.names["decimal-exact-subtract"]),
            ]);
        }
        b.function_with_locals(
            &format!("decimal-{operation}"),
            &[VALUE, VALUE],
            &[VALUE],
            &[(3, VALUE), (2, ValType::I32)],
            &code,
        );
    }
}
