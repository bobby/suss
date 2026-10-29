//! Original checked numeric operands (#286). Clojure arithmetic and numeric
//! comparison reject non-numbers with a language Error instead of JS coercion; the
//! coercing value-* and comparison-* helpers remain for shipped core.
use super::*;

/// Throw unless both (or the only) operands are boxed Numbers.
fn require_numbers(code: &mut Vec<Instruction<'static>>, symbol: &str, operands: u32) {
    use Instruction::*;
    let number = HeapType::Concrete(NUMBER);
    code.extend([LocalGet(0), RefTestNonNull(number)]);
    if operands == 2 {
        code.extend([LocalGet(1), RefTestNonNull(number), I32And]);
    }
    code.extend([I32Eqz, If(BlockType::Empty)]);
    code.extend(numeric::error(
        &format!("{symbol} requires number operands"),
        nominal::ERROR_GLOBAL,
    ));
    code.push(End);
}
fn unbox(code: &mut Vec<Instruction<'static>>, local: u32) {
    use Instruction::*;
    code.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
    ]);
}

/// Returns checked [add, subtract, multiply, divide, negate, identity-add,
/// identity-multiply]; the unary identities return their checked operand.
pub(super) fn intrinsics(b: &mut Builder) -> [u32; 7] {
    use Instruction::*;
    let mut checked = Vec::new();
    for (name, symbol, operation) in [
        ("add", "+", F64Add),
        ("subtract", "-", F64Sub),
        ("multiply", "*", F64Mul),
        ("divide", "/", F64Div),
    ] {
        let mut code = vec![];
        require_numbers(&mut code, symbol, 2);
        unbox(&mut code, 0);
        unbox(&mut code, 1);
        code.extend([operation, StructNew(NUMBER)]);
        checked.push(b.function(&format!("checked-{name}"), &[VALUE, VALUE], &[VALUE], &code));
    }
    let mut code = vec![];
    require_numbers(&mut code, "-", 1);
    unbox(&mut code, 0);
    code.extend([F64Neg, StructNew(NUMBER)]);
    checked.push(b.function("checked-negate", &[VALUE], &[VALUE], &code));
    for (name, symbol) in [("add", "+"), ("multiply", "*")] {
        let mut code = vec![];
        require_numbers(&mut code, symbol, 1);
        code.push(LocalGet(0));
        checked.push(b.function(
            &format!("checked-identity-{name}"),
            &[VALUE],
            &[VALUE],
            &code,
        ));
    }
    for (name, symbol, relation) in [
        ("less", "<", F64Lt),
        ("less-equal", "<=", F64Le),
        ("greater", ">", F64Gt),
        ("greater-equal", ">=", F64Ge),
        ("equal", "==", F64Eq),
        // Inline zero?, pos? and neg? compare their operand with literal 0.
        ("zero", "zero?", F64Eq),
        ("positive", "pos?", F64Gt),
        ("negative", "neg?", F64Lt),
    ] {
        let mut code = vec![];
        require_numbers(&mut code, symbol, 2);
        unbox(&mut code, 0);
        unbox(&mut code, 1);
        code.extend([relation, I32Const(2), I32Mul, I32Const(2), I32Add, RefI31]);
        b.function(
            &format!("comparison-number-{name}"),
            &[VALUE, VALUE],
            &[VALUE],
            &code,
        );
    }
    checked.try_into().unwrap()
}
