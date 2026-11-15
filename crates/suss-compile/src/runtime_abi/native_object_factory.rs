//! The private native object factory, `suss.bootstrap/object-factory`: a
//! first-class variadic function of keys and values, evaluated already; odd
//! pairs fail here, not during source analysis.
use super::*;
pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    // The arguments are the keys and values, in pairs (#425: no nested
    // arrays of pairs).
    let mut code = vec![
        LocalGet(1),
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
        Call(b.names["native-object-property-set-strict"]),
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
            sentinel_value(NIL),
            RefFunc(callback),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
        ],
    );
    vec![callback]
}
