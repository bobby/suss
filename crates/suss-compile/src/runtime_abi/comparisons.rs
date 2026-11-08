//! Original primitive comparison helpers and universal runtime function values.
//! No upstream forms are copied; object coercion remains explicitly unsupported.
//! Public function values use the checked numeric helpers of `numeric_operands`.
use super::*;

fn boolean(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    body.extend(super::boolean());
}
fn units(body: &mut Vec<Instruction<'static>>, owner: u32) {
    use Instruction::*;
    body.extend([LocalGet(owner), RefCastNonNull(HeapType::Concrete(STRING))]);
}
fn relation(op: usize, floating: bool) -> Instruction<'static> {
    use Instruction::*;
    match (op, floating) {
        (0, false) => I32LtU,
        (1, false) => I32LeU,
        (2, false) => I32GtU,
        (3, false) => I32GeU,
        (0, true) => F64Lt,
        (1, true) => F64Le,
        (2, true) => F64Gt,
        (3, true) => F64Ge,
        _ => unreachable!(),
    }
}
fn exact(op: usize) -> Instruction<'static> {
    use Instruction::*;
    [I64LtS, I64LeS, I64GtS, I64GeS][op].clone()
}
pub(super) fn functions(b: &mut Builder, coercion_types: coercions::Types) -> Vec<u32> {
    use Instruction::*;
    let mut helpers = Vec::new();
    for (op, name) in ["less", "less-equal", "greater", "greater-equal"]
        .into_iter()
        .enumerate()
    {
        let mut body = longs::small_relation_fast_path(&exact(op));
        coercions::primitive(&mut body, coercion_types, 0, 0);
        coercions::primitive(&mut body, coercion_types, 1, 0);
        body.extend([
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(STRING)),
            LocalGet(1),
            RefTestNonNull(HeapType::Concrete(STRING)),
            I32And,
            If(BlockType::Empty),
        ]);
        units(&mut body, 0);
        body.extend([ArrayLen, LocalSet(3)]);
        units(&mut body, 1);
        body.extend([
            ArrayLen,
            LocalSet(4),
            I32Const(0),
            LocalSet(2),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(2),
            LocalGet(3),
            I32GeU,
            LocalGet(2),
            LocalGet(4),
            I32GeU,
            I32Or,
            BrIf(1),
        ]);
        units(&mut body, 0);
        body.extend([LocalGet(2), ArrayGetU(STRING), LocalSet(5)]);
        units(&mut body, 1);
        body.extend([
            LocalGet(2),
            ArrayGetU(STRING),
            LocalSet(6),
            LocalGet(5),
            LocalGet(6),
            I32Ne,
            If(BlockType::Empty),
            LocalGet(5),
            LocalGet(6),
            relation(op, false),
        ]);
        boolean(&mut body);
        body.extend([
            Return,
            End,
            LocalGet(2),
            I32Const(1),
            I32Add,
            LocalSet(2),
            Br(0),
            End,
            End,
            LocalGet(3),
            LocalGet(4),
            relation(op, false),
        ]);
        boolean(&mut body);
        body.extend([Return, End]);
        // Two longs compare exactly, even beyond binary64's 2^53.
        body.extend(longs::both_longs(b));
        let unbox = b.names["long-unbox"];
        body.extend([
            If(BlockType::Empty),
            LocalGet(0),
            Call(unbox),
            LocalGet(1),
            Call(unbox),
            exact(op),
        ]);
        boolean(&mut body);
        body.extend([
            Return,
            End,
            LocalGet(0),
            Call(b.names["coerce-number"]),
            LocalGet(1),
            Call(b.names["coerce-number"]),
            relation(op, true),
        ]);
        boolean(&mut body);
        let helper = b.function_with_locals(
            &format!("comparison-{name}"),
            &[VALUE, VALUE],
            &[VALUE],
            &[(5, ValType::I32)],
            &body,
        );
        helpers.push((name, helper));
    }
    // Strict JS primitive identity: numbers by value, a long equal to a double
    // too, as core's pinned `===` uses expect (Clojure's identical? keeps
    // them distinct). Otherwise use identical?'s private factory rather than
    // mutable public predicate cells.
    let mut strict = longs::number_test(0).to_vec();
    strict.extend(longs::number_test(1));
    strict.extend([I32And, If(BlockType::Empty)]);
    strict.extend(longs::relation(b, I64Eq, F64Eq));
    boolean(&mut strict);
    strict.extend([Return, End]);
    strict.extend([
        Call(b.names["predicate-identical"]),
        LocalGet(0),
        LocalGet(1),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        Call(b.names["invoke"]),
    ]);
    let helper = b.function("comparison-strict-equal", &[VALUE, VALUE], &[VALUE], &strict);
    helpers.push(("strict-equal", helper));
    // case's test of a scalar constant: a number matches only a number of its
    // own kind, a double only by its bits (so -0.0 is not 0.0 and NaN matches
    // nothing), as Clojure's case tests long constants as longs and others by
    // hash and equality. Anything else uses strict equality.
    let mut case = longs::both_longs(b).to_vec();
    case.extend([If(BlockType::Empty)]);
    case.extend([LocalGet(0), Call(b.names["long-unbox"]), LocalGet(1), Call(b.names["long-unbox"]), I64Eq]);
    boolean(&mut case);
    case.extend([Return, End]);
    let double = b.names["double-test"];
    case.extend([LocalGet(0), Call(double), LocalGet(1), Call(double), I32And, If(BlockType::Empty)]);
    let number = b.names["number-unbox"];
    case.extend([
        LocalGet(0),
        Call(number),
        LocalTee(2),
        LocalGet(1),
        Call(number),
        LocalTee(3),
        F64Eq,
        LocalGet(2),
        I64ReinterpretF64,
        LocalGet(3),
        I64ReinterpretF64,
        I64Eq,
        I32And,
    ]);
    boolean(&mut case);
    case.extend([Return, End]);
    case.extend(longs::number_test(0));
    case.extend(longs::number_test(1));
    case.extend([I32Or, If(BlockType::Empty), sentinel_value(FALSE), Return, End]);
    case.extend([LocalGet(0), LocalGet(1), Call(helper)]);
    b.function_with_locals("comparison-case-equal", &[VALUE, VALUE], &[VALUE], &[(2, ValType::F64)], &case);
    let mut declared = Vec::new();
    // First-class values are public Clojure functions: check numeric operands
    // (#286). The coercing helpers above remain for shipped core's direct use.
    for (name, _) in helpers {
        let numeric = if name == "strict-equal" { "equal" } else { name };
        let helper = b.names[&format!("comparison-number-{numeric}")];
        let body = vec![
            LocalGet(1),
            ArrayLen,
            LocalTee(3),
            I32Eqz,
            If(BlockType::Empty),
            Call(b.names["arity-error"]),
            Return,
            End,
            I32Const(1),
            LocalSet(2),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(2),
            LocalGet(3),
            I32GeU,
            BrIf(1),
            LocalGet(1),
            LocalGet(2),
            I32Const(1),
            I32Sub,
            ArrayGet(ARGS),
            LocalGet(1),
            LocalGet(2),
            ArrayGet(ARGS),
            Call(helper),
            LocalSet(4),
            LocalGet(4),
            sentinel_value(FALSE),
            RefEq,
            If(BlockType::Empty),
            LocalGet(4),
            Return,
            End,
            LocalGet(2),
            I32Const(1),
            I32Add,
            LocalSet(2),
            Br(0),
            End,
            End,
            sentinel_value(TRUE),
        ];
        let callback = b.count;
        b.functions.function(INVOKE);
        let mut function = Function::new([(2, ValType::I32), (1, VALUE)]);
        for instruction in body {
            function.instruction(&instruction);
        }
        function.instruction(&End);
        b.code.function(&function);
        b.count += 1;
        declared.push(callback);
        b.function(
            &format!("comparison-{name}-function"),
            &[],
            &[VALUE],
            &[
                sentinel_value(NIL),
                RefFunc(callback),
                I32Const(1),
                I32Const(-1),
                Call(b.names["closure-new"]),
            ],
        );
    }
    declared
}
