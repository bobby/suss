//! Original checked numeric operands (#286). Clojure arithmetic and numeric
//! comparison reject non-numbers with a language Error instead of JS coercion; the
//! coercing value-* and comparison-* helpers remain for shipped core.
use super::*;

/// Throw unless both (or the only) operands are numbers.
fn require_numbers(b: &Builder, code: &mut Vec<Instruction<'static>>, symbol: &str, operands: u32) {
    use Instruction::*;
    let test = b.names["number-test"];
    code.extend([LocalGet(0), Call(test)]);
    if operands == 2 {
        code.extend([LocalGet(1), Call(test), I32And]);
    }
    code.extend([I32Eqz, If(BlockType::Empty)]);
    code.extend(numeric::error(
        &format!("{symbol} requires number operands"),
        nominal::ERROR_GLOBAL,
    ));
    code.push(End);
}

/// Returns checked [add, subtract, multiply, divide, negate, identity-add,
/// identity-multiply]; the unary identities return their checked operand.
pub(super) fn intrinsics(b: &mut Builder) -> [u32; 7] {
    use Instruction::*;
    let mut checked = Vec::new();
    for (name, symbol) in [
        ("add", "+"),
        ("subtract", "-"),
        ("multiply", "*"),
        ("divide", "/"),
    ] {
        let mut code = vec![];
        require_numbers(b, &mut code, symbol, 2);
        code.extend([
            LocalGet(0),
            LocalGet(1),
            Call(b.names[&format!("number-{name}")]),
        ]);
        checked.push(b.function(&format!("checked-{name}"), &[VALUE, VALUE], &[VALUE], &code));
    }
    let mut code = vec![];
    require_numbers(b, &mut code, "-", 1);
    code.extend([LocalGet(0), Call(b.names["number-negate"])]);
    checked.push(b.function("checked-negate", &[VALUE], &[VALUE], &code));
    for (name, symbol) in [("add", "+"), ("multiply", "*")] {
        let mut code = vec![];
        require_numbers(b, &mut code, symbol, 1);
        code.push(LocalGet(0));
        checked.push(b.function(
            &format!("checked-identity-{name}"),
            &[VALUE],
            &[VALUE],
            &code,
        ));
    }
    for (name, symbol, exact, floating) in [
        ("less", "<", I64LtS, F64Lt),
        ("less-equal", "<=", I64LeS, F64Le),
        ("greater", ">", I64GtS, F64Gt),
        ("greater-equal", ">=", I64GeS, F64Ge),
        ("equal", "==", I64Eq, F64Eq),
        // Inline zero?, pos? and neg? compare their operand with literal 0.
        ("zero", "zero?", I64Eq, F64Eq),
        ("positive", "pos?", I64GtS, F64Gt),
        ("negative", "neg?", I64LtS, F64Lt),
    ] {
        let mut code = vec![];
        require_numbers(b, &mut code, symbol, 2);
        code.extend(longs::relation(b, exact, floating));
        code.extend(super::boolean());
        b.function(
            &format!("comparison-number-{name}"),
            &[VALUE, VALUE],
            &[VALUE],
            &code,
        );
    }
    checked.try_into().unwrap()
}
