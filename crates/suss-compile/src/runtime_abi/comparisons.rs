//! Original primitive comparison helpers and universal runtime function values.
//! No upstream forms are copied; object coercion remains explicitly unsupported.
use super::*;

fn boolean(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    body.extend([I32Const(2), I32Mul, I32Const(2), I32Add, RefI31]);
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
pub(super) fn functions(b: &mut Builder, coercion_types: coercions::Types) -> Vec<u32> {
    use Instruction::*;
    let mut helpers = Vec::new();
    for (op, name) in ["less", "less-equal", "greater", "greater-equal"]
        .into_iter()
        .enumerate()
    {
        let mut body = vec![];
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
    // Strict JS primitive identity already has checked Number/UTF-16/reference
    // behavior. Use its private factory rather than mutable public predicate cells.
    let helper = b.function(
        "comparison-strict-equal",
        &[VALUE, VALUE],
        &[VALUE],
        &[
            Call(b.names["predicate-identical"]),
            LocalGet(0),
            LocalGet(1),
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 2,
            },
            Call(b.names["invoke"]),
        ],
    );
    helpers.push(("strict-equal", helper));
    let mut declared = Vec::new();
    for (name, helper) in helpers {
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
            I32Const(2),
            RefI31,
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
            I32Const(4),
            RefI31,
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
                I32Const(0),
                RefI31,
                RefFunc(callback),
                I32Const(1),
                I32Const(-1),
                Call(b.names["closure-new"]),
            ],
        );
    }
    declared
}
