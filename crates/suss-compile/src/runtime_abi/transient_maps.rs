//! Guarded edits of canonical, already editable HAMT storage for i31 keys.
//! Unsupported paths decline without writing; core retains full collisions/COW.
//! Layout/algorithm reference: pinned core.cljs c4295f303100bbf5afac449242d30bca1126f1a1,
//! BitmapIndexedNode/ArrayNode, imported with EPL provenance in core-import.json.
//! This original WasmGC implementation uses the existing shared nominal storage.
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
        globals.global(
            GlobalType {
                val_type: VALUE,
                mutable: true,
                shared: false,
            },
            &ConstExpr::extended([sentinel_value(NIL)]),
        );
    }
}
fn miss(code: &mut Vec<Instruction<'static>>) {
    code.extend([If(BlockType::Empty), sentinel_value(UNDEFINED), Return, End]);
}
fn cast(ty: HeapType, depth: u32) -> Instruction<'static> {
    BrOnCastFail {
        relative_depth: depth,
        from_ref_type: RefType::EQREF,
        to_ref_type: RefType {
            nullable: false,
            heap_type: ty,
        },
    }
}
fn nullable(ty: u32) -> ValType {
    ValType::Ref(RefType {
        nullable: true,
        heap_type: HeapType::Concrete(ty),
    })
}
fn field(code: &mut Vec<Instruction<'static>>, array: u32, slot: i32) {
    code.extend([
        LocalGet(array),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(slot),
        ArrayGet(ARGS),
    ]);
}
// The checked OBJECT in6, its fields into `destination`. The caller separately
// checks its descriptor when accepting more than one node type.
fn object(
    code: &mut Vec<Instruction<'static>>,
    value: u32,
    destination: u32,
    count: i32,
    depth: u32,
) {
    code.extend([
        LocalGet(value),
        cast(HeapType::Concrete(OBJECT), depth),
        LocalTee(6),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 1,
        },
        LocalTee(destination),
        ArrayLen,
        I32Const(count),
        I32Ne,
    ]);
    miss(code);
}
fn descriptor(code: &mut Vec<Instruction<'static>>, global: u32) {
    code.extend([
        LocalGet(6),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 0,
        },
        GlobalGet(global),
        RefEq,
    ]);
}
// Checked object-array storage from the value on the stack into local5.
fn storage(code: &mut Vec<Instruction<'static>>, depth: u32) {
    code.extend([
        cast(HeapType::Concrete(PACKED), depth),
        StructGet {
            struct_type_index: PACKED,
            field_index: 0,
        },
        cast(HeapType::Concrete(ARGS), depth),
        LocalSet(5),
    ]);
}

// Build fresh canonical bitmap storage, retaining the current owned prototype.
// Allocation and subtype checks happen before any existing node is modified.
fn new_nodes(b: &mut Builder) -> (u32, u32) {
    let code = vec![
        GlobalGet(BITMAP),
        RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
        LocalGet(0),
        LocalGet(1),
        Call(b.names["long-box-i32"]),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        RefI31,
        StructNew(PACKED),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        GlobalGet(BITMAP),
        Call(b.names["owned-descriptor-prototype"]),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        I32Const(0),
        RefI31,
        StructNew(OBJECT),
    ];
    let bitmap = b.function(
        "transient-map-bitmap-new",
        &[VALUE, ValType::I32, VALUE],
        &[VALUE],
        &code,
    );
    let code = vec![
        I32Const(8),
        ArrayNewDefault(ARGS),
        LocalSet(5),
        LocalGet(5),
        RefAsNonNull,
        I32Const(0),
        LocalGet(3),
        ArraySet(ARGS),
        LocalGet(5),
        RefAsNonNull,
        I32Const(1),
        LocalGet(4),
        ArraySet(ARGS),
        LocalGet(0),
        I32Const(1),
        LocalGet(1),
        LocalGet(2),
        I32ShrU,
        I32Const(31),
        I32And,
        I32Shl,
        LocalGet(5),
        Call(bitmap),
    ];
    let leaf = b.function_with_locals(
        "transient-map-leaf-new",
        &[VALUE, ValType::I32, ValType::I32, VALUE, VALUE],
        &[VALUE],
        &[(1, nullable(ARGS))],
        &code,
    );
    // Params edit, old-key/value, new-key/value, initial shift, new hash.
    // Locals7 long key;8 hash scratch,9 old hash,10 split shift,11/12 bits;
    // 13 array,14 constructed child. Full hash collisions retain core's node.
    let mut code = vec![
        Block(BlockType::Result(VALUE)),
        LocalGet(1),
        cast(HeapType::I31, 0),
        I31GetS,
        I64ExtendI32S,
        LocalSet(7),
        LocalGet(5),
        I32Const(30),
        I32GtU,
    ];
    miss(&mut code);
    code.extend(hash_long_in(7, 8));
    code.extend([LocalTee(9), LocalGet(6), I32Eq]);
    miss(&mut code);
    code.extend([
        LocalGet(5),
        LocalSet(10),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        I32Const(1),
        LocalGet(9),
        LocalGet(10),
        I32ShrU,
        I32Const(31),
        I32And,
        I32Shl,
        LocalSet(11),
        I32Const(1),
        LocalGet(6),
        LocalGet(10),
        I32ShrU,
        I32Const(31),
        I32And,
        I32Shl,
        LocalSet(12),
        LocalGet(11),
        LocalGet(12),
        I32Ne,
        BrIf(1),
        LocalGet(10),
        I32Const(30),
        I32GeU,
    ]);
    miss(&mut code);
    code.extend([
        LocalGet(10),
        I32Const(5),
        I32Add,
        LocalSet(10),
        Br(0),
        End,
        End,
        I32Const(8),
        ArrayNewDefault(ARGS),
        LocalSet(13),
        LocalGet(11),
        LocalGet(12),
        I32LtU,
        If(BlockType::Empty),
    ]);
    for (slot, value) in [(0, 1), (1, 2), (2, 3), (3, 4)] {
        code.extend([
            LocalGet(13),
            RefAsNonNull,
            I32Const(slot),
            LocalGet(value),
            ArraySet(ARGS),
        ]);
    }
    code.push(Else);
    for (slot, value) in [(0, 3), (1, 4), (2, 1), (3, 2)] {
        code.extend([
            LocalGet(13),
            RefAsNonNull,
            I32Const(slot),
            LocalGet(value),
            ArraySet(ARGS),
        ]);
    }
    code.extend([
        End,
        LocalGet(0),
        LocalGet(11),
        LocalGet(12),
        I32Or,
        LocalGet(13),
        Call(bitmap),
        LocalSet(14),
        Loop(BlockType::Empty),
        LocalGet(10),
        LocalGet(5),
        I32LeU,
        If(BlockType::Empty),
        LocalGet(14),
        Return,
        End,
        LocalGet(10),
        I32Const(5),
        I32Sub,
        LocalSet(10),
        I32Const(8),
        ArrayNewDefault(ARGS),
        LocalTee(13),
        I32Const(1),
        LocalGet(14),
        ArraySet(ARGS),
        LocalGet(0),
        I32Const(1),
        LocalGet(9),
        LocalGet(10),
        I32ShrU,
        I32Const(31),
        I32And,
        I32Shl,
        LocalGet(13),
        Call(bitmap),
        LocalSet(14),
        Br(0),
        End,
        Unreachable,
        End,
        Drop,
        sentinel_value(UNDEFINED),
    ]);
    let split = b.function_with_locals(
        "transient-map-split",
        &[
            VALUE,
            VALUE,
            VALUE,
            VALUE,
            VALUE,
            ValType::I32,
            ValType::I32,
        ],
        &[VALUE],
        &[
            (1, ValType::I64),
            (5, ValType::I32),
            (1, nullable(ARGS)),
            (1, VALUE),
        ],
        &code,
    );
    (leaf, split)
}

// Promote a full editable bitmap node. Validate every key before allocating or
// writing; noninteger keys retain core's generic hashing/callback path.
fn promotion(b: &mut Builder, leaf: u32) -> u32 {
    // Params edit, bitmap, storage, key, value, hash, shift. Locals7 slot,
    // 8 pair offset,9 old key,10 new children,11 raw old key,12 hash scratch,
    // 13 used pair slots,14 old hash. No existing node is written here.
    let mut code = vec![
        LocalGet(1),
        I32Popcnt,
        I32Const(1),
        I32Shl,
        LocalSet(13),
        I32Const(0),
        LocalSet(8),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(8),
        LocalGet(13),
        I32GeU,
        BrIf(1),
    ];
    code.extend([
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(8),
        ArrayGet(ARGS),
        LocalTee(9),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(9),
        RefTestNonNull(HeapType::I31),
        I32Eqz,
    ]);
    miss(&mut code);
    code.extend([
        End,
        LocalGet(8),
        I32Const(2),
        I32Add,
        LocalSet(8),
        Br(0),
        End,
        End,
        I32Const(32),
        ArrayNewDefault(ARGS),
        LocalSet(10),
        LocalGet(10),
        RefAsNonNull,
        LocalGet(5),
        LocalGet(6),
        I32ShrU,
        I32Const(31),
        I32And,
        LocalGet(0),
        LocalGet(5),
        LocalGet(6),
        I32Const(5),
        I32Add,
        LocalGet(3),
        LocalGet(4),
        Call(leaf),
        ArraySet(ARGS),
        I32Const(0),
        LocalSet(7),
        I32Const(0),
        LocalSet(8),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(7),
        I32Const(32),
        I32GeU,
        BrIf(1),
        LocalGet(1),
        I32Const(1),
        LocalGet(7),
        I32Shl,
        I32And,
        If(BlockType::Empty),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(8),
        ArrayGet(ARGS),
        LocalTee(9),
        RefIsNull,
        If(BlockType::Empty),
        LocalGet(10),
        RefAsNonNull,
        LocalGet(7),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(8),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        ArraySet(ARGS),
        Else,
        LocalGet(9),
        RefCastNonNull(HeapType::I31),
        I31GetS,
        I64ExtendI32S,
        LocalSet(11),
    ]);
    code.extend(hash_long_in(11, 12));
    code.extend([
        LocalSet(14),
        LocalGet(10),
        RefAsNonNull,
        LocalGet(7),
        LocalGet(0),
        LocalGet(14),
        LocalGet(6),
        I32Const(5),
        I32Add,
        LocalGet(9),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(8),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        Call(leaf),
        ArraySet(ARGS),
        End,
        LocalGet(8),
        I32Const(2),
        I32Add,
        LocalSet(8),
        End,
        LocalGet(7),
        I32Const(1),
        I32Add,
        LocalSet(7),
        Br(0),
        End,
        End,
        GlobalGet(ARRAY),
        RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
        LocalGet(0),
        LocalGet(1),
        I32Popcnt,
        I32Const(1),
        I32Add,
        RefI31,
        LocalGet(10),
        RefAsNonNull,
        I32Const(0),
        RefI31,
        StructNew(PACKED),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        GlobalGet(ARRAY),
        Call(b.names["owned-descriptor-prototype"]),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        I32Const(0),
        RefI31,
        StructNew(OBJECT),
    ]);
    b.function_with_locals(
        "transient-map-promote",
        &[
            VALUE,
            ValType::I32,
            VALUE,
            VALUE,
            VALUE,
            ValType::I32,
            ValType::I32,
        ],
        &[VALUE],
        &[
            (2, ValType::I32),
            (1, VALUE),
            (1, nullable(ARGS)),
            (1, ValType::I64),
            (3, ValType::I32),
        ],
        &code,
    )
}

pub(super) fn functions(b: &mut Builder) {
    let mut code = vec![];
    for i in 0..5 {
        code.extend([
            LocalGet(i),
            RefCastNonNull(HeapType::Concrete(OBJECT)),
            StructGet {
                struct_type_index: OBJECT,
                field_index: 0,
            },
            LocalSet(i + 5),
        ]);
    }
    for (i, global) in [MAP, BITMAP, ARRAY, TOKEN, VECTOR].into_iter().enumerate() {
        code.extend([LocalGet(i as u32 + 5), GlobalSet(global)]);
    }
    code.push(sentinel_value(NIL));
    b.function_with_locals(
        "transient-map-types-register",
        &[VALUE; 5],
        &[VALUE],
        &[(5, VALUE)],
        &code,
    );

    let (leaf, split) = new_nodes(b);
    let promote = promotion(b, leaf);

    // Params0..2 map/key/value. Locals3 map fields,4 node fields,5 storage,
    // 6 temporary object,7 scratch value,8 edit,9 node;10 hash,11 count,
    // 12 shift,13 bitmap,14 bit,15 pair index,16 pair count,17 array length,
    // 18 hash scratch,19 raw key,20 replacement storage,21 replacement flag.
    code = vec![
        Block(BlockType::Result(VALUE)),
        LocalGet(1),
        cast(HeapType::I31, 0),
        I31GetS,
        I64ExtendI32S,
        LocalSet(19),
    ];
    object(&mut code, 0, 3, 5, 0);
    descriptor(&mut code, MAP);
    code.push(I32Eqz);
    miss(&mut code);
    field(&mut code, 3, 2);
    code.extend([
        cast(HeapType::I31, 0),
        I31GetS,
        LocalTee(11),
        I32Const(0x3fff_ffff),
        I32GeU,
    ]);
    miss(&mut code);
    field(&mut code, 3, 0);
    code.push(LocalSet(8));
    object(&mut code, 8, 4, 0, 0);
    descriptor(&mut code, TOKEN);
    code.push(I32Eqz);
    miss(&mut code);
    field(&mut code, 3, 1);
    code.push(LocalSet(9));
    code.extend(hash_long_in(19, 18));
    code.extend([
        LocalSet(10),
        I32Const(0),
        LocalSet(12),
        Loop(BlockType::Empty),
    ]);
    object(&mut code, 9, 4, 3, 1);
    field(&mut code, 4, 0);
    code.extend([LocalGet(8), RefEq, I32Eqz]);
    miss(&mut code);
    // Both supported node classes have [edit bitmap-or-count object-array].
    // Validate nominal identity before looking through the object-array wrapper.
    descriptor(&mut code, BITMAP);
    descriptor(&mut code, ARRAY);
    code.extend([I32Or, I32Eqz]);
    miss(&mut code);
    field(&mut code, 4, 2);
    storage(&mut code, 1);
    code.extend([
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalSet(17),
    ]);
    descriptor(&mut code, BITMAP);
    code.push(If(BlockType::Empty));
    // Bitmap is exactly a signed int32, either i31 or a boxed long.
    field(&mut code, 4, 1);
    code.extend([
        LocalTee(7),
        RefTestNonNull(HeapType::I31),
        If(BlockType::Result(ValType::I32)),
        LocalGet(7),
        RefCastNonNull(HeapType::I31),
        I31GetS,
        Else,
        LocalGet(7),
        cast(HeapType::Concrete(LONG), 3),
        StructGet {
            struct_type_index: LONG,
            field_index: 0,
        },
        LocalTee(19),
        I64Const(i32::MIN as i64),
        I64LtS,
        LocalGet(19),
        I64Const(i32::MAX as i64),
        I64GtS,
        I32Or,
    ]);
    miss(&mut code);
    code.extend([
        LocalGet(19),
        I32WrapI64,
        End,
        LocalTee(13),
        I32Popcnt,
        LocalTee(16),
        I32Const(1),
        I32Shl,
        LocalGet(17),
        I32GtU,
    ]);
    miss(&mut code);
    code.extend([
        I32Const(1),
        LocalGet(10),
        LocalGet(12),
        I32ShrU,
        I32Const(31),
        I32And,
        I32Shl,
        LocalTee(14),
        I32Const(1),
        I32Sub,
        LocalGet(13),
        I32And,
        I32Popcnt,
        I32Const(1),
        I32Shl,
        LocalSet(15),
        LocalGet(13),
        LocalGet(14),
        I32And,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    // New leaf: reserve storage before writes. Odd/truncated storage declines;
    // a full bitmap node promotes only after every old key passes preflight.
    code.extend([LocalGet(17), I32Const(1), I32And]);
    miss(&mut code);
    code.extend([
        I32Const(0),
        LocalSet(21),
        LocalGet(5),
        LocalSet(20),
        LocalGet(17),
        LocalGet(16),
        I32Const(1),
        I32Add,
        I32Const(1),
        I32Shl,
        I32LtU,
        If(BlockType::Empty),
        LocalGet(16),
        I32Const(16),
        I32GeU,
        If(BlockType::Empty),
        LocalGet(12),
        I32Const(30),
        I32GeU,
    ]);
    miss(&mut code);
    code.extend([
        LocalGet(8),
        LocalGet(13),
        LocalGet(5),
        LocalGet(1),
        LocalGet(2),
        LocalGet(10),
        LocalGet(12),
        Call(promote),
        LocalTee(9),
        sentinel_value(UNDEFINED),
        RefEq,
    ]);
    miss(&mut code);
    code.extend([
        LocalGet(22),
        RefIsNull,
        If(BlockType::Empty),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        LocalGet(9),
        ArraySet(ARGS),
        Else,
        LocalGet(22),
        RefAsNonNull,
        LocalGet(23),
        LocalGet(9),
        ArraySet(ARGS),
        End,
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(2),
        LocalGet(11),
        I32Const(1),
        I32Add,
        RefI31,
        ArraySet(ARGS),
        LocalGet(0),
        Return,
        End,
    ]);
    code.extend([
        LocalGet(16),
        I32Const(4),
        I32Add,
        I32Const(1),
        I32Shl,
        ArrayNewDefault(ARGS),
        LocalTee(20),
        I32Const(0),
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(16),
        I32Const(1),
        I32Shl,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        I32Const(1),
        LocalSet(21),
        End,
    ]);
    // array.copy is overlap-safe; no callback can observe the private slots.
    code.extend([
        LocalGet(20),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15),
        I32Const(2),
        I32Add,
        LocalGet(20),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15),
        LocalGet(16),
        I32Const(1),
        I32Shl,
        LocalGet(15),
        I32Sub,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(20),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15),
        LocalGet(1),
        ArraySet(ARGS),
        LocalGet(20),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15),
        I32Const(1),
        I32Add,
        LocalGet(2),
        ArraySet(ARGS),
        LocalGet(21),
        If(BlockType::Empty),
        LocalGet(4),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(2),
        LocalGet(20),
        RefAsNonNull,
        I32Const(0),
        RefI31,
        StructNew(PACKED),
        ArraySet(ARGS),
        End,
        LocalGet(4),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        LocalGet(13),
        LocalGet(14),
        I32Or,
        Call(b.names["long-box-i32"]),
        ArraySet(ARGS),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(2),
        LocalGet(11),
        I32Const(1),
        I32Add,
        RefI31,
        ArraySet(ARGS),
        LocalGet(0),
        Return,
        End,
    ]);
    // Existing slot: either descend without changing its ancestors, replace a
    // reference-identical i31 key's value, or let source handle key collision.
    code.extend([
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15),
        ArrayGet(ARGS),
        LocalTee(7),
        RefIsNull,
        If(BlockType::Empty),
        LocalGet(12),
        I32Const(30),
        I32GeU,
    ]);
    miss(&mut code);
    code.extend([
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        LocalSet(9),
        LocalGet(5),
        LocalSet(22),
        LocalGet(15),
        I32Const(1),
        I32Add,
        LocalSet(23),
        LocalGet(12),
        I32Const(5),
        I32Add,
        LocalSet(12),
        Br(2),
        End,
        LocalGet(7),
        LocalGet(1),
        RefEq,
        If(BlockType::Empty),
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15),
        I32Const(1),
        I32Add,
        LocalGet(2),
        ArraySet(ARGS),
        LocalGet(0),
        Return,
        End,
        LocalGet(12),
        I32Const(30),
        I32GeU,
    ]);
    miss(&mut code);
    code.extend([LocalGet(8), LocalGet(7)]);
    code.extend([
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15),
        I32Const(1),
        I32Add,
        ArrayGet(ARGS),
        LocalGet(1),
        LocalGet(2),
        LocalGet(12),
        I32Const(5),
        I32Add,
        LocalGet(10),
        Call(split),
        LocalTee(9),
        sentinel_value(UNDEFINED),
        RefEq,
    ]);
    miss(&mut code);
    code.extend([
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15),
        sentinel_value(NIL),
        ArraySet(ARGS),
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(15),
        I32Const(1),
        I32Add,
        LocalGet(9),
        ArraySet(ARGS),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(2),
        LocalGet(11),
        I32Const(1),
        I32Add,
        RefI31,
        ArraySet(ARGS),
        LocalGet(0),
        Return,
        End,
    ]);
    // ArrayNode: exactly32 slots and a supported count. Missing children are
    // built privately before publication; existing children retain edit checks.
    field(&mut code, 4, 1);
    code.extend([
        cast(HeapType::I31, 1),
        I31GetS,
        LocalTee(16),
        I32Const(32),
        I32GtU,
        LocalGet(17),
        I32Const(32),
        I32Ne,
        I32Or,
    ]);
    miss(&mut code);
    code.extend([LocalGet(12), I32Const(30), I32GeU]);
    miss(&mut code);
    code.extend([
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(10),
        LocalGet(12),
        I32ShrU,
        I32Const(31),
        I32And,
        ArrayGet(ARGS),
        LocalTee(9),
        RefIsNull,
        If(BlockType::Empty),
        LocalGet(16),
        I32Const(32),
        I32GeU,
    ]);
    miss(&mut code);
    code.extend([
        LocalGet(8),
        LocalGet(10),
        LocalGet(12),
        I32Const(5),
        I32Add,
        LocalGet(1),
        LocalGet(2),
        Call(leaf),
        LocalSet(9),
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(10),
        LocalGet(12),
        I32ShrU,
        I32Const(31),
        I32And,
        LocalGet(9),
        ArraySet(ARGS),
        LocalGet(4),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        LocalGet(16),
        I32Const(1),
        I32Add,
        RefI31,
        ArraySet(ARGS),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(2),
        LocalGet(11),
        I32Const(1),
        I32Add,
        RefI31,
        ArraySet(ARGS),
        LocalGet(0),
        Return,
        End,
        LocalGet(5),
        LocalSet(22),
        LocalGet(10),
        LocalGet(12),
        I32ShrU,
        I32Const(31),
        I32And,
        LocalSet(23),
        LocalGet(12),
        I32Const(5),
        I32Add,
        LocalSet(12),
        Br(0),
        End,
        Unreachable,
        End,
        Drop,
        sentinel_value(UNDEFINED),
    ]);
    let assoc = b.function_with_locals(
        "transient-map-assoc",
        &[VALUE; 3],
        &[VALUE],
        &[
            (3, nullable(ARGS)),
            (1, nullable(OBJECT)),
            (3, VALUE),
            (9, ValType::I32),
            (1, ValType::I64),
            (1, nullable(ARGS)),
            (1, ValType::I32),
            (1, nullable(ARGS)),
            (1, ValType::I32),
        ],
        &code,
    );

    // Canonical two-element vector entry. Its fields/storage remain private;
    // custom map entries and vectors of other lengths retain protocol handling.
    code = vec![Block(BlockType::Result(VALUE))];
    object(&mut code, 1, 4, 6, 0);
    descriptor(&mut code, VECTOR);
    code.push(I32Eqz);
    miss(&mut code);
    field(&mut code, 4, 1);
    code.extend([I32Const(2), RefI31, RefEq, I32Eqz]);
    miss(&mut code);
    field(&mut code, 4, 4);
    storage(&mut code, 0);
    code.extend([
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(2),
        I32Ne,
    ]);
    miss(&mut code);
    code.push(LocalGet(0));
    field(&mut code, 5, 0);
    field(&mut code, 5, 1);
    code.extend([Call(assoc), Return, End, Drop, sentinel_value(UNDEFINED)]);
    b.function_with_locals(
        "transient-map-conj",
        &[VALUE; 2],
        &[VALUE],
        &[(2, VALUE), (2, nullable(ARGS)), (1, nullable(OBJECT))],
        &code,
    );
}
