//! Original GC-owned ordinary Array properties, separate from indexed backing.
//! Nodes use existing Args [UTF16 key, payload, next]; owner fields root the head.
use super::*;
use Instruction::*;
fn args(code: &mut Vec<Instruction<'static>>, local: u32) {
    code.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(ARGS))]);
}
pub(super) fn functions(b: &mut Builder) {
    // Canonical decimal uint32 property names, including the ordinary MAX key.
    // No parsing coercions: "01", "-0", whitespace and exponents are names.
    let mut code = vec![LocalGet(0), RefCastNonNull(HeapType::Concrete(STRING)), LocalSet(1),
        LocalGet(1), RefCastNonNull(HeapType::Concrete(STRING)), ArrayLen, LocalTee(2),
        I32Eqz, LocalGet(2), I32Const(10), I32GtU, I32Or, If(BlockType::Empty),
        I64Const(-1), Return, End,
        LocalGet(2), I32Const(1), I32GtU, If(BlockType::Empty),
        LocalGet(1), RefCastNonNull(HeapType::Concrete(STRING)), I32Const(0), ArrayGetU(STRING),
        I32Const(48), I32Eq, If(BlockType::Empty), I64Const(-1), Return, End, End,
        I32Const(0), LocalSet(3), I64Const(0), LocalSet(5),
        Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(3), LocalGet(2), I32GeU, BrIf(1),
        LocalGet(1), RefCastNonNull(HeapType::Concrete(STRING)), LocalGet(3), ArrayGetU(STRING),
        LocalTee(4), I32Const(48), I32LtU, LocalGet(4), I32Const(57), I32GtU, I32Or,
        If(BlockType::Empty), I64Const(-1), Return, End,
        LocalGet(5), I64Const(10), I64Mul, LocalGet(4), I32Const(48), I32Sub,
        I64ExtendI32U, I64Add, LocalSet(5),
        LocalGet(3), I32Const(1), I32Add, LocalSet(3), Br(0), End, End,
        LocalGet(5), I64Const(4294967295), I64GtU, If(BlockType::Empty),
        I64Const(-1), Return, End, LocalGet(5)];
    b.function_with_locals("source-array-own-key-index", &[VALUE], &[ValType::I64],
        &[(1, VALUE), (3, ValType::I32), (1, ValType::I64)], &code);

    code = vec![LocalGet(0), RefCastNonNull(HeapType::Concrete(STRING)), ArrayLen,
        LocalGet(1), RefCastNonNull(HeapType::Concrete(STRING)), ArrayLen, I32Ne,
        If(BlockType::Empty), I32Const(0), Return, End, I32Const(0), LocalSet(2),
        Block(BlockType::Empty), Loop(BlockType::Empty), LocalGet(2), LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)), ArrayLen, I32GeU, BrIf(1)];
    for local in [0,1] { code.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(STRING)), LocalGet(2), ArrayGetU(STRING)]); }
    code.extend([I32Ne, If(BlockType::Empty), I32Const(0), Return, End,
        LocalGet(2), I32Const(1), I32Add, LocalSet(2), Br(0), End, End, I32Const(1)]);
    let equal = b.function_with_locals("source-array-property-key-equal", &[VALUE,VALUE], &[ValType::I32], &[(1,ValType::I32)], &code);
    code = vec![LocalGet(0)];
    code.extend("length".encode_utf16().map(|u| I32Const(i32::from(u))));
    code.extend([ArrayNewFixed {array_type_index:STRING,array_size:6}, Call(equal)]);
    b.function("source-array-length-key?", &[VALUE], &[ValType::I32], &code);
    code = vec![LocalGet(0), I32Const(0), RefI31, RefEq, If(BlockType::Empty), LocalGet(0), Return, End,
        LocalGet(0), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, If(BlockType::Empty)];
    nominal::error(&mut code); code.push(End); args(&mut code,0);
    code.extend([ArrayLen, I32Const(3), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut code); code.push(End); args(&mut code,0);
    code.extend([I32Const(0), ArrayGet(ARGS), RefTestNonNull(HeapType::Concrete(STRING)), I32Eqz, If(BlockType::Empty)]);
    nominal::error(&mut code); code.push(End); args(&mut code,0);
    code.extend([I32Const(2), ArrayGet(ARGS), LocalTee(1), I32Const(0), RefI31, RefEq,
        LocalGet(1), RefTestNonNull(HeapType::Concrete(ARGS)), I32Or, I32Eqz, If(BlockType::Empty)]);
    nominal::error(&mut code); code.extend([End,LocalGet(1)]);
    let next=b.function_with_locals("source-array-property-node-next", &[VALUE], &[VALUE], &[(1,VALUE)], &code);
    code=vec![LocalGet(0), Call(b.names["source-array-fields"]), RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1), ArrayGet(ARGS), LocalTee(1), LocalSet(2), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(2), Call(next), Call(next), LocalTee(2), I32Const(0), RefI31, RefEq, BrIf(1),
        LocalGet(1), Call(next), LocalTee(1), LocalGet(2), RefEq, If(BlockType::Empty)];
    nominal::error(&mut code);
    code.extend([End, Br(0), End, End,
        LocalGet(0), Call(b.names["source-array-fields"]), RefCastNonNull(HeapType::Concrete(ARGS)), I32Const(1), ArrayGet(ARGS)]);
    let head=b.function_with_locals("source-array-property-head", &[VALUE], &[VALUE], &[(2,VALUE)], &code);
    code=vec![LocalGet(1), Call(b.names["coerce-string"]), LocalSet(2),
        LocalGet(0), Call(head), LocalSet(3), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(3), I32Const(0), RefI31, RefEq, BrIf(1)];
    args(&mut code,3); code.extend([I32Const(0), ArrayGet(ARGS), LocalGet(2), Call(equal), If(BlockType::Empty),
        LocalGet(3), Return, End, LocalGet(3), Call(next), LocalSet(3), Br(0), End, End, I32Const(0), RefI31]);
    let find=b.function_with_locals("source-array-property-find", &[VALUE,VALUE], &[VALUE], &[(2,VALUE)], &code);
    code=vec![LocalGet(0), LocalGet(1), Call(find), LocalTee(2), I32Const(0), RefI31, RefEq,
        If(BlockType::Result(VALUE)), I32Const(UNDEFINED), RefI31, Else];
    args(&mut code,2); code.extend([I32Const(1), ArrayGet(ARGS), End]);
    b.function_with_locals("source-array-property-get", &[VALUE,VALUE], &[VALUE], &[(1,VALUE)], &code);
    b.function("source-array-property-has", &[VALUE,VALUE], &[ValType::I32], &[
        LocalGet(0),LocalGet(1),Call(find),I32Const(0),RefI31,RefEq,I32Eqz]);
    code=vec![LocalGet(1),Call(b.names["coerce-string"]),LocalSet(3),
        LocalGet(0),LocalGet(3),Call(find),LocalTee(4),I32Const(0),RefI31,RefEq,
        If(BlockType::Empty),LocalGet(0),Call(b.names["source-array-fields"]),RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),LocalGet(3),LocalGet(2),LocalGet(0),Call(head),
        ArrayNewFixed{array_type_index:ARGS,array_size:3},ArraySet(ARGS),Else];
    args(&mut code,4); code.extend([I32Const(1),LocalGet(2),ArraySet(ARGS),End,LocalGet(2)]);
    b.function_with_locals("source-array-property-set", &[VALUE,VALUE,VALUE], &[VALUE], &[(2,VALUE)], &code);
}
