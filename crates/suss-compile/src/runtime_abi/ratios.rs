//! Original exact rational arithmetic over immutable GC-owned integers (#338).
//! Published ratios are reduced, nonzero, and have a positive denominator > 1.
//! The integer result of normalization is a BigInt, as in Clojure.
use super::*;
use Instruction::*;
const DOUBLE_ROOT: u32 = MAP_ENTRY_DESCRIPTOR_GLOBAL + 1;

fn field(local: u32, index: u32) -> [Instruction<'static>; 3] {
    [
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(RATIO)),
        StructGet {
            struct_type_index: RATIO,
            field_index: index,
        },
    ]
}
fn sign(local: u32) -> [Instruction<'static>; 3] {
    [
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(BIGINT)),
        StructGet {
            struct_type_index: BIGINT,
            field_index: 0,
        },
    ]
}
fn integer(b: &Builder, value: i64) -> [Instruction<'static>; 2] {
    [I64Const(value), Call(b.names["bigint-from-long"])]
}
pub(super) fn declare(b: &mut Builder) -> u32 {

    b.function(
        "ratio-test",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), RefTestNonNull(HeapType::Concrete(RATIO))],
    );
    let ty = b.next_type;
    b.types.ty().function([VALUE], [ValType::F64]);
    b.next_type += 1;
    b.function(
        "ratio-double",
        &[VALUE],
        &[ValType::F64],
        &[LocalGet(0), GlobalGet(DOUBLE_ROOT), CallRef(ty)],
    );
    ty
}
pub(super) fn append_global(globals: &mut GlobalSection, ty: u32, function: u32) {
    assert_eq!(globals.len(), DOUBLE_ROOT);
    globals.global(
        GlobalType {
            val_type: reference(ty),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([RefFunc(function)]),
    );
}
pub(super) fn functions(b: &mut Builder) {
    // The reader has already normalized compiler literals. Keep their exact
    // BigInteger children as stable fields; runtime arithmetic uses ratio-new.
    b.function("ratio-literal", &[VALUE, VALUE], &[VALUE], &[
        LocalGet(0), Call(b.names["integer-biginteger"]), RefCastNonNull(HeapType::Concrete(BIGINT)),
        LocalGet(1), Call(b.names["integer-biginteger"]), RefCastNonNull(HeapType::Concrete(BIGINT)), StructNew(RATIO),
    ]);

    let mut code = sign(0).to_vec();
    code.extend([
        I32Const(0),
        I32LtS,
        If(BlockType::Result(VALUE)),
        LocalGet(0),
        Call(b.names["bigint-negate"]),
        Else,
        LocalGet(0),
        Call(b.names["integer-bigint"]),
        End,
    ]);
    b.function("bigint-abs", &[VALUE], &[VALUE], &code);
    let mut code = vec![
        LocalGet(0),
        Call(b.names["bigint-abs"]),
        LocalSet(0),
        LocalGet(1),
        Call(b.names["bigint-abs"]),
        LocalSet(1),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
    ];
    code.extend(sign(1));
    code.extend([
        I32Eqz,
        BrIf(1),
        LocalGet(0),
        LocalGet(1),
        Call(b.names["bigint-rem"]),
        LocalSet(2),
        LocalGet(1),
        LocalSet(0),
        LocalGet(2),
        LocalSet(1),
        Br(0),
        End,
        End,
        LocalGet(0),
    ]);
    b.function_with_locals(
        "bigint-gcd",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &code,
    );
    let mut code = sign(1).to_vec();
    code.extend([I32Eqz, If(BlockType::Empty)]);
    code.extend(numeric::error("Divide by zero", nominal::ERROR_GLOBAL));
    code.push(End);
    code.extend(sign(1));
    code.extend([
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["bigint-negate"]),
        LocalSet(0),
        LocalGet(1),
        Call(b.names["bigint-negate"]),
        LocalSet(1),
        End,
        LocalGet(0),
        LocalGet(1),
        Call(b.names["bigint-gcd"]),
        LocalSet(2),
        LocalGet(0),
        LocalGet(2),
        Call(b.names["bigint-quot"]),
        LocalSet(0),
        LocalGet(1),
        LocalGet(2),
        Call(b.names["bigint-quot"]),
        LocalSet(1),
        LocalGet(1),
    ]);
    code.extend(integer(b, 1));
    code.extend([
        Call(b.names["bigint-compare"]),
        I32Eqz,
        If(BlockType::Result(VALUE)),
        LocalGet(0),
        Else, LocalGet(0), LocalGet(1), Call(b.names["ratio-literal"]), End,
    ]);
    b.function_with_locals("ratio-new", &[VALUE, VALUE], &[VALUE], &[(1, VALUE)], &code);
    // Internal fraction projections accept integers too; public numerator and
    // denominator below require an actual Ratio and return BigInteger objects.
    for (name, index) in [("numerator", 0), ("denominator", 1)] {
        let mut code = vec![
            LocalGet(0),
            Call(b.names["ratio-test"]),
            If(BlockType::Result(VALUE)),
        ];
        code.extend(field(0, index));
        code.push(Else);
        if index == 0 {
            code.extend([LocalGet(0), Call(b.names["integer-bigint"])]);
        } else {
            code.extend(integer(b, 1));
        }
        code.push(End);
        b.function(&format!("fraction-{name}"), &[VALUE], &[VALUE], &code);
        let mut code = vec![
            LocalGet(0),
            Call(b.names["ratio-test"]),
            I32Eqz,
            If(BlockType::Empty),
        ];
        code.extend(numeric::error("Expected a Ratio", nominal::ERROR_GLOBAL));
        code.push(End);
        code.extend(field(0, index));
        b.function(&format!("ratio-{name}"), &[VALUE], &[VALUE], &code);
    }
    // Fraction operands are stored in locals 2..5 (a,b,c,d).
    let operands = |b: &Builder| {
        let mut code = Vec::new();
        for (argument, projection, local) in [
            (0, "numerator", 2),
            (0, "denominator", 3),
            (1, "numerator", 4),
            (1, "denominator", 5),
        ] {
            code.extend([
                LocalGet(argument),
                Call(b.names[&format!("fraction-{projection}")]),
                LocalSet(local),
            ]);
        }
        code
    };
    for name in [
        "add", "subtract", "multiply", "divide", "quot", "rem", "compare",
    ] {
        let mut code = operands(b);
        if matches!(name, "add" | "subtract" | "compare") {
            code.extend([
                LocalGet(2),
                LocalGet(5),
                Call(b.names["bigint-multiply"]),
                LocalGet(4),
                LocalGet(3),
                Call(b.names["bigint-multiply"]),
                Call(
                    b.names[if name == "compare" {
                        "bigint-compare"
                    } else if name == "add" {
                        "bigint-add"
                    } else {
                        "bigint-subtract"
                    }],
                ),
            ]);
        } else {
            code.extend([
                LocalGet(2),
                LocalGet(if name == "multiply" { 4 } else { 5 }),
                Call(b.names["bigint-multiply"]),
            ]);
        }
        if name != "compare" {
            code.push(LocalSet(6));
            code.extend([
                LocalGet(3),
                LocalGet(
                    if name == "multiply" || name == "add" || name == "subtract" {
                        5
                    } else {
                        4
                    },
                ),
                Call(b.names["bigint-multiply"]),
                LocalSet(7),
            ]);
            if name == "quot" || name == "rem" {
                code.extend([LocalGet(6), LocalGet(7), Call(b.names["bigint-quot"])]);
                if name == "rem" {
                    // a/b - trunc((a*d)/(b*c)) * c/d, all exact.
                    code.extend([
                        LocalGet(4),
                        Call(b.names["bigint-multiply"]),
                        LocalGet(3),
                        Call(b.names["bigint-multiply"]),
                        LocalSet(6),
                        LocalGet(2),
                        LocalGet(5),
                        Call(b.names["bigint-multiply"]),
                        LocalGet(6),
                        Call(b.names["bigint-subtract"]),
                        LocalGet(3),
                        LocalGet(5),
                        Call(b.names["bigint-multiply"]),
                        Call(b.names["ratio-new"]),
                    ]);
                }
            } else {
                code.extend([LocalGet(6), LocalGet(7), Call(b.names["ratio-new"])]);
            }
        }
        b.function_with_locals(
            &format!("fraction-{name}"),
            &[VALUE, VALUE],
            &[if name == "compare" {
                ValType::I32
            } else {
                VALUE
            }],
            &[(6, VALUE)],
            &code,
        );
    }
    let mut code = field(0, 0).to_vec();
    code.push(Call(b.names["bigint-negate"]));
    code.extend(field(0, 1));
    code.push(Call(b.names["ratio-new"]));
    b.function("ratio-negate", &[VALUE], &[VALUE], &code);
    let mut code = field(0, 0).to_vec();
    code.extend(field(0, 1));
    code.push(Call(b.names["bigint-quot"]));
    b.function("ratio-integer", &[VALUE], &[VALUE], &code);
    let mut code = field(0,0).to_vec();
    code.extend([Call(b.names["bigint-abs"])]);
    code.extend(field(0,1)); code.push(Call(b.names["ratio-new"]));
    b.function("ratio-abs", &[VALUE], &[VALUE], &code);
    let mut code = field(0,0).to_vec();
    code.extend([Call(b.names["bigint-word-hash"]), Call(b.names["long-unbox"])]);
    code.extend(field(0,1));
    code.extend([Call(b.names["bigint-word-hash"]), Call(b.names["long-unbox"]), I64Xor, Call(b.names["long-box"])]);
    b.function("ratio-hash", &[VALUE], &[VALUE], &code);

}

/// DECIMAL64 (16 significant decimal digits, HALF_EVEN), then binary64.
/// This reproduces Ratio.doubleValue, including ties and signed underflow.
/// It does not divide two independently rounded binary64 magnitudes.
pub(super) fn late_functions(b: &mut Builder) -> u32 {
    let mut code = field(0, 0).to_vec();
    code.push(Call(b.names["bigint-abs"]));
    code.push(LocalSet(1));
    code.extend(field(0, 1));
    code.push(LocalSet(2));
    code.extend([
        I32Const(15),
        LocalGet(1),
        Call(b.names["bigint-string"]),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        LocalGet(2),
        Call(b.names["bigint-string"]),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        I32Sub,
        I32Sub,
        LocalSet(5),
        LocalGet(5),
        LocalSet(6),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(6),
        I32Eqz,
        BrIf(1),
        LocalGet(6),
        I32Const(0),
        I32GtS,
        If(BlockType::Empty),
        LocalGet(1),
    ]);
    code.extend(integer(b, 10));
    code.extend([
        Call(b.names["bigint-multiply"]),
        LocalSet(1),
        LocalGet(6),
        I32Const(1),
        I32Sub,
        LocalSet(6),
        Else,
        LocalGet(2),
    ]);
    code.extend(integer(b, 10));
    code.extend([
        Call(b.names["bigint-multiply"]),
        LocalSet(2),
        LocalGet(6),
        I32Const(1),
        I32Add,
        LocalSet(6),
        End,
        Br(0),
        End,
        End,
        LocalGet(1),
        LocalGet(2),
        Call(b.names["bigint-divrem"]),
        LocalSet(4),
        LocalSet(3),
        LocalGet(3),
        Call(b.names["bigint-low-long"]),
        I64Const(1_000_000_000_000_000),
        I64LtU,
        If(BlockType::Empty),
        LocalGet(1),
    ]);
    code.extend(integer(b, 10));
    code.extend([
        Call(b.names["bigint-multiply"]),
        LocalSet(1),
        LocalGet(5),
        I32Const(1),
        I32Add,
        LocalSet(5),
        LocalGet(1),
        LocalGet(2),
        Call(b.names["bigint-divrem"]),
        LocalSet(4),
        LocalSet(3),
        End,
        LocalGet(3),
        Call(b.names["bigint-low-long"]),
        LocalSet(7),
        LocalGet(4),
        LocalGet(4),
        Call(b.names["bigint-add"]),
        LocalGet(2),
        Call(b.names["bigint-compare"]),
        LocalTee(6),
        I32Const(0),
        I32GtS,
        LocalGet(6),
        I32Eqz,
        LocalGet(7),
        I64Const(1),
        I64And,
        I64Eqz,
        I32Eqz,
        I32And,
        I32Or,
        If(BlockType::Empty),
        LocalGet(7),
        I64Const(1),
        I64Add,
        LocalSet(7),
        End,
    ]);
    code.extend(field(0, 0));
    code.extend([
        RefCastNonNull(HeapType::Concrete(BIGINT)),
        StructGet {
            struct_type_index: BIGINT,
            field_index: 0,
        },
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
        I64Const(0),
        LocalGet(7),
        I64Sub,
        LocalSet(7),
        End,
        LocalGet(7),
        Call(b.names["long-string"]),
        I32Const(69),
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: 1,
        },
        Call(b.names["string-concat"]),
        I32Const(0),
        LocalGet(5),
        I32Sub,
        I64ExtendI32S,
        Call(b.names["long-string"]),
        Call(b.names["string-concat"]),
        Call(b.names["coerce-number"]),
    ]);
    let double = b.function_with_locals(
        "ratio-double-implementation",
        &[VALUE],
        &[ValType::F64],
        &[(4, VALUE), (2, ValType::I32), (1, ValType::I64)],
        &code,
    );
    double
}

pub(super) fn string(b: &mut Builder) {
    let mut code = field(0, 0).to_vec();
    code.extend([
        Call(b.names["bigint-string"]),
        I32Const(47),
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: 1,
        },
        Call(b.names["string-concat"]),
    ]);
    code.extend(field(0, 1));
    code.extend([
        Call(b.names["bigint-string"]),
        Call(b.names["string-concat"]),
    ]);
    b.function("ratio-string", &[VALUE], &[VALUE], &code);
}

/// At least one Ratio, with no floating operand. Callers have checked numbers.
pub(super) fn pair(b: &Builder) -> Vec<Instruction<'static>> {
    vec![LocalGet(0), Call(b.names["ratio-test"]), LocalGet(1), Call(b.names["ratio-test"]), I32Or,
        LocalGet(0), Call(b.names["double-test"]), LocalGet(1), Call(b.names["double-test"]), I32Or, I32Eqz, I32And]
}
