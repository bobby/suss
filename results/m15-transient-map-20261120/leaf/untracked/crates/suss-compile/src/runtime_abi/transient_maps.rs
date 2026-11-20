//! Guarded edits of canonical, already editable HAMT storage for i31 keys.
//! Unsupported paths decline without writing; core retains growth/collision/COW.
use super::*;
use Instruction::*;

const MAP: u32 = transient_vectors::GLOBAL_LIMIT;
const BITMAP: u32 = MAP + 1;
const ARRAY: u32 = MAP + 2;
const TOKEN: u32 = MAP + 3;
const VECTOR: u32 = MAP + 4;

pub(super) fn append_globals(globals: &mut GlobalSection) {
    assert_eq!(globals.len(), MAP);
    for _ in 0..5 {
        globals.global(GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::extended([sentinel_value(NIL)]));
    }
}
fn miss(code: &mut Vec<Instruction<'static>>) {
    code.extend([If(BlockType::Empty), sentinel_value(UNDEFINED), Return, End]);
}
fn cast(ty: HeapType, depth: u32) -> Instruction<'static> {
    BrOnCastFail { relative_depth: depth, from_ref_type: RefType::EQREF,
        to_ref_type: RefType { nullable: false, heap_type: ty } }
}
fn nullable(ty: u32) -> ValType {
    ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(ty) })
}
fn field(code: &mut Vec<Instruction<'static>>, array: u32, slot: i32) {
    code.extend([LocalGet(array), RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(slot), ArrayGet(ARGS)]);
}
// The checked OBJECT in6, its fields into `destination`. The caller separately
// checks its descriptor when accepting more than one node type.
fn object(code: &mut Vec<Instruction<'static>>, value: u32, destination: u32, count: i32, depth: u32) {
    code.extend([LocalGet(value), cast(HeapType::Concrete(OBJECT), depth), LocalTee(6),
        StructGet { struct_type_index: OBJECT, field_index: 1 }, LocalTee(destination),
        ArrayLen, I32Const(count), I32Ne]);
    miss(code);
}
fn descriptor(code: &mut Vec<Instruction<'static>>, global: u32) {
    code.extend([LocalGet(6), RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet { struct_type_index: OBJECT, field_index: 0 }, GlobalGet(global), RefEq]);
}
// Checked object-array storage from the value on the stack into local5.
fn storage(code: &mut Vec<Instruction<'static>>, depth: u32) {
    code.extend([cast(HeapType::Concrete(PACKED), depth),
        StructGet { struct_type_index: PACKED, field_index: 0 },
        cast(HeapType::Concrete(ARGS), depth), LocalSet(5)]);
}

pub(super) fn functions(b: &mut Builder) {
    let mut code = vec![];
    for i in 0..5 {
        code.extend([LocalGet(i), RefCastNonNull(HeapType::Concrete(OBJECT)),
            StructGet { struct_type_index: OBJECT, field_index: 0 }, LocalSet(i + 5)]);
    }
    for (i, global) in [MAP, BITMAP, ARRAY, TOKEN, VECTOR].into_iter().enumerate() {
        code.extend([LocalGet(i as u32 + 5), GlobalSet(global)]);
    }
    code.push(sentinel_value(NIL));
    b.function_with_locals("transient-map-types-register", &[VALUE; 5], &[VALUE], &[(5, VALUE)], &code);

    // Params0..2 map/key/value. Locals3 map fields,4 node fields,5 storage,
    // 6 temporary object,7 scratch value,8 edit,9 node;10 hash,11 count,
    // 12 shift,13 bitmap,14 bit,15 pair index,16 pair count,17 array length,
    // 18 hash scratch,19 raw key,20 replacement storage,21 replacement flag.
    code = vec![Block(BlockType::Result(VALUE)),
        LocalGet(1), cast(HeapType::I31, 0), I31GetS, I64ExtendI32S, LocalSet(19)];
    object(&mut code, 0, 3, 5, 0);
    descriptor(&mut code, MAP);
    code.push(I32Eqz); miss(&mut code);
    field(&mut code, 3, 2);
    code.extend([cast(HeapType::I31, 0), I31GetS, LocalTee(11), I32Const(0x3fff_ffff), I32GeU]);
    miss(&mut code);
    field(&mut code, 3, 0); code.push(LocalSet(8));
    object(&mut code, 8, 4, 0, 0);
    descriptor(&mut code, TOKEN);
    code.push(I32Eqz); miss(&mut code);
    field(&mut code, 3, 1); code.push(LocalSet(9));
    code.extend(hash_long_in(19, 18));
    code.extend([LocalSet(10), I32Const(0), LocalSet(12), Loop(BlockType::Empty)]);
    object(&mut code, 9, 4, 3, 1);
    field(&mut code, 4, 0);
    code.extend([LocalGet(8), RefEq, I32Eqz]); miss(&mut code);
    // Both supported node classes have [edit bitmap-or-count object-array].
    // Validate nominal identity before looking through the object-array wrapper.
    descriptor(&mut code, BITMAP);
    descriptor(&mut code, ARRAY);
    code.extend([I32Or, I32Eqz]); miss(&mut code);
    field(&mut code, 4, 2); storage(&mut code, 1);
    code.extend([LocalGet(5), RefCastNonNull(HeapType::Concrete(ARGS)), ArrayLen, LocalSet(17)]);
    descriptor(&mut code, BITMAP);
    code.push(If(BlockType::Empty));
    // Bitmap is exactly a signed int32, either i31 or a boxed long.
    field(&mut code, 4, 1);
    code.extend([LocalTee(7), RefTestNonNull(HeapType::I31), If(BlockType::Result(ValType::I32)),
        LocalGet(7), RefCastNonNull(HeapType::I31), I31GetS, Else,
        LocalGet(7), cast(HeapType::Concrete(LONG), 3),
        StructGet { struct_type_index: LONG, field_index: 0 }, LocalTee(19),
        I64Const(i32::MIN as i64), I64LtS,
        LocalGet(19), I64Const(i32::MAX as i64), I64GtS, I32Or]);
    miss(&mut code);
    code.extend([LocalGet(19), I32WrapI64, End, LocalTee(13), I32Popcnt, LocalTee(16),
        I32Const(1), I32Shl, LocalGet(17), I32GtU]); miss(&mut code);
    code.extend([I32Const(1), LocalGet(10), LocalGet(12), I32ShrU,
        I32Const(31), I32And, I32Shl, LocalTee(14), I32Const(1), I32Sub,
        LocalGet(13), I32And, I32Popcnt, I32Const(1), I32Shl, LocalSet(15),
        LocalGet(13), LocalGet(14), I32And, I32Eqz, If(BlockType::Empty)]);
    // New leaf: reserve storage before writes. Odd, truncated or oversized
    // shapes decline; full bitmap nodes at16+ pairs retain source promotion.
    code.extend([LocalGet(17), I32Const(1), I32And]); miss(&mut code);
    code.extend([I32Const(0), LocalSet(21), LocalGet(5), LocalSet(20),
        LocalGet(17), LocalGet(16), I32Const(1), I32Add, I32Const(1), I32Shl,
        I32LtU, If(BlockType::Empty), LocalGet(16), I32Const(16), I32GeU]); miss(&mut code);
    code.extend([LocalGet(16), I32Const(4), I32Add, I32Const(1), I32Shl,
        ArrayNewDefault(ARGS), LocalTee(20), I32Const(0),
        LocalGet(5), RefCastNonNull(HeapType::Concrete(ARGS)), I32Const(0),
        LocalGet(16), I32Const(1), I32Shl,
        ArrayCopy { array_type_index_dst: ARGS, array_type_index_src: ARGS },
        I32Const(1), LocalSet(21), End]);
    // array.copy is overlap-safe; no callback can observe the private slots.
    code.extend([LocalGet(20), RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15), I32Const(2), I32Add,
        LocalGet(20), RefCastNonNull(HeapType::Concrete(ARGS)), LocalGet(15),
        LocalGet(16), I32Const(1), I32Shl, LocalGet(15), I32Sub,
        ArrayCopy { array_type_index_dst: ARGS, array_type_index_src: ARGS },
        LocalGet(20), RefCastNonNull(HeapType::Concrete(ARGS)), LocalGet(15), LocalGet(1), ArraySet(ARGS),
        LocalGet(20), RefCastNonNull(HeapType::Concrete(ARGS)), LocalGet(15), I32Const(1), I32Add, LocalGet(2), ArraySet(ARGS),
        LocalGet(21), If(BlockType::Empty),
        LocalGet(4), RefCastNonNull(HeapType::Concrete(ARGS)), I32Const(2),
        LocalGet(20), RefAsNonNull, I32Const(0), RefI31, StructNew(PACKED), ArraySet(ARGS), End,
        LocalGet(4), RefCastNonNull(HeapType::Concrete(ARGS)), I32Const(1),
        LocalGet(13), LocalGet(14), I32Or, Call(b.names["long-box-i32"]), ArraySet(ARGS),
        LocalGet(3), RefCastNonNull(HeapType::Concrete(ARGS)), I32Const(2),
        LocalGet(11), I32Const(1), I32Add, RefI31, ArraySet(ARGS), LocalGet(0), Return, End]);
    // Existing slot: either descend without changing its ancestors, replace a
    // reference-identical i31 key's value, or let source handle key collision.
    code.extend([LocalGet(5), RefCastNonNull(HeapType::Concrete(ARGS)), LocalGet(15), ArrayGet(ARGS), LocalTee(7),
        RefIsNull, If(BlockType::Empty), LocalGet(12), I32Const(30), I32GeU]); miss(&mut code);
    code.extend([LocalGet(5), RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15), I32Const(1), I32Add, ArrayGet(ARGS), LocalSet(9),
        LocalGet(12), I32Const(5), I32Add, LocalSet(12), Br(2), End,
        LocalGet(7), LocalGet(1), RefEq, I32Eqz]); miss(&mut code);
    code.extend([LocalGet(5), RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15), I32Const(1), I32Add, LocalGet(2), ArraySet(ARGS), LocalGet(0), Return, End]);
    // ArrayNode: exactly32 slots and a supported count. Missing child creation
    // retains source; this helper mutates only a completely admitted leaf.
    field(&mut code, 4, 1);
    code.extend([cast(HeapType::I31, 1), I31GetS, I32Const(32), I32GtU,
        LocalGet(17), I32Const(32), I32Ne, I32Or]); miss(&mut code);
    code.extend([LocalGet(12), I32Const(30), I32GeU]); miss(&mut code);
    code.extend([LocalGet(5), RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(10), LocalGet(12), I32ShrU, I32Const(31), I32And, ArrayGet(ARGS), LocalSet(9),
        LocalGet(12), I32Const(5), I32Add, LocalSet(12), Br(0), End,
        Unreachable, End, Drop, sentinel_value(UNDEFINED)]);
    let assoc = b.function_with_locals("transient-map-assoc", &[VALUE; 3], &[VALUE],
        &[(3, nullable(ARGS)), (1, nullable(OBJECT)), (3, VALUE), (9, ValType::I32),
          (1, ValType::I64), (1, nullable(ARGS)), (1, ValType::I32)], &code);

    // Canonical two-element vector entry. Its fields/storage remain private;
    // custom map entries and vectors of other lengths retain protocol handling.
    code = vec![Block(BlockType::Result(VALUE))];
    object(&mut code, 1, 4, 6, 0);
    descriptor(&mut code, VECTOR); code.push(I32Eqz); miss(&mut code);
    field(&mut code, 4, 1);
    code.extend([I32Const(2), RefI31, RefEq, I32Eqz]); miss(&mut code);
    field(&mut code, 4, 4); storage(&mut code, 0);
    code.extend([LocalGet(5), RefCastNonNull(HeapType::Concrete(ARGS)), ArrayLen,
        I32Const(2), I32Ne]); miss(&mut code);
    code.push(LocalGet(0));
    field(&mut code, 5, 0); field(&mut code, 5, 1);
    code.extend([Call(assoc), Return, End, Drop, sentinel_value(UNDEFINED)]);
    b.function_with_locals("transient-map-conj", &[VALUE; 2], &[VALUE],
        &[(2, VALUE), (2, nullable(ARGS)), (1, nullable(OBJECT))], &code);
}
