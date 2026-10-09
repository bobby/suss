//! Original own-property adapter for GC-owned source Array data and length.
use super::*;
const METHOD_ROOT: u32 = range_errors::DESCRIPTOR_GLOBAL + 2;
pub(super) fn append_globals(globals: &mut GlobalSection) {
    globals.global(GlobalType { val_type: VALUE, mutable: true, shared: false },
        &ConstExpr::extended([Instruction::I32Const(0), Instruction::RefI31]));
}
pub(super) fn functions(b: &mut Builder, equal: u32) -> Vec<u32> {
    use Instruction::*;
    let mut code = vec![LocalGet(1), ArrayLen, I32Eqz, If(BlockType::Empty)];
    nominal::error(&mut code);
    code.extend([End, LocalGet(1), I32Const(0), ArrayGet(ARGS),
        LocalTee(5), Call(b.names["source-array-backing"]), LocalSet(2),
        LocalGet(1), ArrayLen, I32Const(1), I32GtU, If(BlockType::Result(VALUE)),
        LocalGet(1), I32Const(1), ArrayGet(ARGS), Else, I32Const(UNDEFINED), RefI31, End,
        Call(b.names["coerce-string"]), LocalSet(3), LocalGet(3)]);
    code.extend("length".encode_utf16().map(|u| I32Const(i32::from(u))));
    code.extend([ArrayNewFixed {array_type_index: STRING, array_size: 6}, Call(equal),
        If(BlockType::Empty), I32Const(4), RefI31, Return, End,
        LocalGet(3), Call(b.names["source-array-own-key-index"]), LocalTee(4),
        I64Const(-1), I64Eq, If(BlockType::Empty), LocalGet(5), LocalGet(3), Call(b.names["source-array-property-has"]), I32Const(1), I32Shl, I32Const(2), I32Add, RefI31, Return, End,
        LocalGet(2), LocalGet(4), I32WrapI64, Call(b.names["source-array-sparse-has"]),
        I32Const(1), I32Shl, I32Const(2), I32Add, RefI31]);
    let anchored = b.count;
    b.functions.function(INVOKE);
    let mut body = Function::new([(2, VALUE), (1, ValType::I64), (1, VALUE)]);
    for instruction in code { body.instruction(&instruction); }
    body.instruction(&End); b.code.function(&body); b.count += 1;
    let detached = b.count;
    b.functions.function(INVOKE);
    let mut code = vec![]; nominal::error(&mut code);
    let mut body = Function::new([]);
    for instruction in code { body.instruction(&instruction); }
    body.instruction(&End); b.code.function(&body); b.count += 1;
    b.function("source-array-has-own-method", &[], &[VALUE], &[
        GlobalGet(METHOD_ROOT), RefTestNonNull(HeapType::Concrete(CLOSURE)),
        If(BlockType::Empty), GlobalGet(METHOD_ROOT), Return, End,
        GlobalGet(object_methods::TAG_GLOBAL), I32Const(0), RefI31, RefFunc(anchored),
        I32Const(1), I32Const(-1), Call(b.names["closure-new"]),
        ArrayNewFixed {array_type_index: ARGS, array_size: 1},
        I32Const(0), RefI31, I32Const(0), RefI31, StructNew(7),
        RefFunc(detached), I32Const(0), I32Const(-1), Call(b.names["closure-new"]),
        GlobalSet(METHOD_ROOT), GlobalGet(METHOD_ROOT)]);
    vec![anchored, detached]
}
