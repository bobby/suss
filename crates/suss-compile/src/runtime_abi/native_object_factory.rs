//! Original first-class variadic factory for adapted js-obj. Invocation arguments
//! have already been evaluated; odd pairs fail here, not during source analysis.
use super::*;
pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    let next = b.function("native-object-factory-next", &[VALUE], &[VALUE], &{
        let mut code = vec![
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(ARGS)),
            I32Eqz,
            If(BlockType::Empty),
        ];
        nominal::error(&mut code);
        code.push(End);
        code.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            I32Const(1),
            I32Eq,
            If(BlockType::Empty),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            ArrayGet(ARGS),
            Call(b.names["source-array?"]),
            If(BlockType::Empty),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            ArrayGet(ARGS),
            Call(b.names["source-array-storage"]),
            Return,
            End,
            End,
            LocalGet(0),
        ]);
        code
    });
    let mut code = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(0),
        LocalSet(1),
        LocalGet(0),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
        Call(next),
        LocalSet(3),
        LocalGet(1),
        LocalGet(3),
        RefEq,
        If(BlockType::Empty),
        LocalGet(1),
        Return,
        End,
        LocalGet(3),
        LocalSet(1),
        LocalGet(2),
        Call(next),
        Call(next),
        LocalSet(2),
        LocalGet(1),
        LocalGet(2),
        RefEq,
        If(BlockType::Empty),
        LocalGet(1),
        Call(next),
        LocalGet(1),
        RefEq,
        If(BlockType::Empty),
        LocalGet(1),
        Return,
        End,
    ]);
    nominal::error(&mut code);
    code.extend([End, Br(0), End, End, LocalGet(1)]);
    let flatten = b.function_with_locals(
        "native-object-factory-flatten",
        &[VALUE],
        &[VALUE],
        &[(3, VALUE)],
        &code,
    );
    code = vec![
        LocalGet(1),
        Call(flatten),
        LocalSet(2),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalTee(3),
        I32Const(1),
        I32And,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.extend([
        End,
        Call(b.names["native-object-default-new"]),
        LocalSet(4),
        I32Const(0),
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(5),
        LocalGet(3),
        I32GeU,
        BrIf(1),
        LocalGet(4),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(5),
        ArrayGet(ARGS),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(5),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        Call(b.names["native-object-property-set"]),
        Drop,
        LocalGet(5),
        I32Const(2),
        I32Add,
        LocalSet(5),
        Br(0),
        End,
        End,
        LocalGet(4),
    ]);
    let callback = b.count;
    b.functions.function(INVOKE);
    let mut f = Function::new([(1, VALUE), (1, ValType::I32), (1, VALUE), (1, ValType::I32)]);
    for instruction in code {
        f.instruction(&instruction);
    }
    f.instruction(&End);
    b.code.function(&f);
    b.count += 1;
    b.function(
        "native-object-factory-function",
        &[],
        &[VALUE],
        &[
            I32Const(0),
            RefI31,
            RefFunc(callback),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
        ],
    );
    vec![callback]
}
