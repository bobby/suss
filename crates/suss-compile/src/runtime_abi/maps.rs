//! Direct reads of core's persistent maps and sets (#442).
//!
//! `map-lookup(m, k, mode)` answers a lookup in a PersistentArrayMap, a
//! PersistentHashMap or a PersistentHashSet without calling into core, when
//! the key is one that only its own reference equals: a long in i31 range or
//! an interned keyword whose hash is cached. Then reference identity decides
//! the lookup exactly, as `=` would. Mode 0 answers the key's value, or the
//! given not-found value (nil for `get`'s two arguments) when it's absent;
//! mode 1 answers true or false, for `contains?`. Anything
//! else answers undefined, which the caller takes to its general path: another
//! collection or key, a hash collision node, a keyword whose hash isn't cached
//! yet, or any field that isn't the expected shape.
//!
//! Core registers the types it reads, by an instance of each, with
//! `hash-map-type-register` (the array map's is `array-map-type-register`).
//! Their layouts are core's: PersistentHashMap is `[meta cnt root has_nil
//! nil_val __hash]`, BitmapIndexedNode `[edit bitmap arr]`, ArrayNode `[edit
//! cnt arr]`, PersistentHashSet `[meta hash-map __hash]` and Keyword `[ns
//! name fqn _hash]`; a node's arr is an object array.
use super::*;

pub(super) fn functions(b: &mut Builder) {
    use Instruction::*;
    let descriptor = |local: u32| {
        [LocalGet(local), RefCastNonNull(HeapType::Concrete(7)), StructGet { struct_type_index: 7, field_index: 0 }]
    };
    let mut body = vec![];
    for (index, global) in [HASH_MAP_DESCRIPTOR_GLOBAL, BITMAP_NODE_DESCRIPTOR_GLOBAL, ARRAY_NODE_DESCRIPTOR_GLOBAL, HASH_SET_DESCRIPTOR_GLOBAL]
        .into_iter()
        .enumerate()
    {
        body.extend(descriptor(index as u32));
        body.push(GlobalSet(global));
    }
    body.push(LocalGet(0));
    b.function("hash-map-type-register", &[VALUE, VALUE, VALUE, VALUE], &[VALUE], &body);

    let typed = |ty: u32| ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(ty) });
    let exact = |ty: u32| RefType { nullable: false, heap_type: HeapType::Concrete(ty) };
    // A cast that takes `miss` (a depth outside the cast's own block) when the
    // value on the stack isn't of type `ty`, and leaves it cast otherwise.
    let cast = |ty: u32, miss: u32| {
        [BrOnCast { relative_depth: 0, from_ref_type: RefType::EQREF, to_ref_type: exact(ty) }, Drop, Br(miss + 1)]
    };

    // Params: 0 the collection, 1 the key, 2 the mode, 3 the value for an
    // absent key in mode 0. Locals: 4 fields, 5 an object array's storage,
    // 6 an index, 7 a value, 8 the hash, 9 the shift, 10 the node, 11 the
    // bitmap, 12 scratch, 13 the nominal object, 14 its descriptor, 15 the
    // key as a long, 16 the keyword key.
    // The nominal object in `local`, into local 13, and its descriptor into
    // local 14; a miss when it isn't one.
    let nominal = |body: &mut Vec<Instruction<'static>>, local: u32, miss: u32| {
        body.extend([Block(BlockType::Result(ValType::Ref(exact(7)))), LocalGet(local)]);
        body.extend(cast(7, miss));
        body.extend([End, LocalTee(13), StructGet { struct_type_index: 7, field_index: 0 }, LocalSet(14)]);
    };
    // The fields of the object in local 13, into local 4, which must hold at
    // least `count`; a miss otherwise.
    let fields = |body: &mut Vec<Instruction<'static>>, count: i32, miss: u32| {
        body.extend([
            LocalGet(13), StructGet { struct_type_index: 7, field_index: 1 },
            LocalTee(4), ArrayLen, I32Const(count), I32LtU, BrIf(miss),
        ]);
    };
    // The object array in field `field` of the fields in local 4, into local
    // 5; a miss when it isn't one.
    let storage = |body: &mut Vec<Instruction<'static>>, field: i32, miss: u32| {
        body.extend([
            Block(BlockType::Result(ValType::Ref(exact(ARGS)))),
            Block(BlockType::Result(ValType::Ref(exact(PACKED)))),
            LocalGet(4), I32Const(field), ArrayGet(ARGS),
        ]);
        body.extend(cast(PACKED, miss + 1));
        body.extend([End, StructGet { struct_type_index: PACKED, field_index: 0 }]);
        body.extend(cast(ARGS, miss));
        body.extend([End, LocalSet(5)]);
    };
    // The int on the stack, an i31 or a boxed long, wrapped to 32 bits as
    // core's int32 bit operations read it; a miss for anything else.
    let int = |body: &mut Vec<Instruction<'static>>, miss: u32| {
        body.extend([
            LocalTee(7), RefTestNonNull(HeapType::I31),
            If(BlockType::Result(ValType::I32)),
            LocalGet(7), RefCastNonNull(HeapType::I31), I31GetS,
            Else,
            Block(BlockType::Result(ValType::Ref(exact(LONG)))), LocalGet(7),
        ]);
        body.extend(cast(LONG, miss + 1));
        body.extend([End, StructGet { struct_type_index: LONG, field_index: 0 }, I32WrapI64, End]);
    };
    // The answer for a found value in local 7, or for an absent key.
    let found = |body: &mut Vec<Instruction<'static>>| {
        body.extend([
            LocalGet(2), If(BlockType::Result(VALUE)), sentinel_value(TRUE), Else, LocalGet(7), End, Return,
        ]);
    };
    let absent = |body: &mut Vec<Instruction<'static>>| {
        body.extend([
            LocalGet(2), If(BlockType::Result(VALUE)), sentinel_value(FALSE), Else, LocalGet(3), End, Return,
        ]);
    };

    let mut body = vec![Block(BlockType::Empty)];
    // Depth 0 here is the miss block.
    nominal(&mut body, 0, 0);
    // The key: an i31 long or an interned keyword, kept in local 16. Only a
    // hash map needs its hash, which is computed there.
    body.extend([LocalGet(1), RefTestNonNull(HeapType::I31), I32Eqz, If(BlockType::Empty)]);
    body.extend([Block(BlockType::Result(ValType::Ref(exact(7)))), LocalGet(1)]);
    body.extend(cast(7, 1));
    body.extend([
        End, LocalTee(16), StructGet { struct_type_index: 7, field_index: 0 },
        GlobalGet(KEYWORD_DESCRIPTOR_GLOBAL), RefEq, I32Eqz, BrIf(1), End,
    ]);

    // A set: by key presence in its map, which the paths below read. A set's
    // value isn't its key, so only contains? is answered here.
    body.extend([LocalGet(14), GlobalGet(HASH_SET_DESCRIPTOR_GLOBAL), RefEq, If(BlockType::Empty)]);
    body.extend([LocalGet(2), I32Eqz, BrIf(1)]);
    fields(&mut body, 2, 1);
    body.extend([LocalGet(4), I32Const(1), ArrayGet(ARGS), LocalSet(10)]);
    nominal(&mut body, 10, 1);
    body.push(End);

    // An array map: scan its keys by reference.
    body.extend([LocalGet(14), GlobalGet(ARRAY_MAP_DESCRIPTOR_GLOBAL), RefEq, If(BlockType::Empty)]);
    // Inside this If the miss block is depth 1.
    fields(&mut body, 3, 1);
    storage(&mut body, 2, 1);
    body.extend([
        I32Const(0), LocalSet(6),
        Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(6), I32Const(1), I32Add, LocalGet(5), ArrayLen, I32GeU, BrIf(1),
        LocalGet(5), LocalGet(6), ArrayGet(ARGS), LocalGet(1), RefEq,
        If(BlockType::Empty),
        LocalGet(5), LocalGet(6), I32Const(1), I32Add, ArrayGet(ARGS), LocalSet(7),
    ]);
    found(&mut body);
    body.extend([End, LocalGet(6), I32Const(2), I32Add, LocalSet(6), Br(0), End, End]);
    absent(&mut body);
    body.push(End);

    // A hash map: walk its trie from the root.
    body.extend([LocalGet(14), GlobalGet(HASH_MAP_DESCRIPTOR_GLOBAL), RefEq, I32Eqz, BrIf(0)]);
    // The key's hash, into local 8: a long's Murmur3 hash, computed inline as
    // number-hash computes it, or a keyword's cached hash, a miss when it
    // isn't cached yet.
    body.extend([
        LocalGet(1), RefTestNonNull(HeapType::I31),
        If(BlockType::Empty),
        LocalGet(1), RefCastNonNull(HeapType::I31), I31GetS, I64ExtendI32S, LocalSet(15),
    ]);
    body.extend(hash_long_in(15, 12));
    body.extend([
        LocalSet(8),
        Else,
        LocalGet(16), StructGet { struct_type_index: 7, field_index: 1 },
        LocalTee(4), ArrayLen, I32Const(4), I32LtU, BrIf(1),
        LocalGet(4), I32Const(3), ArrayGet(ARGS),
    ]);
    int(&mut body, 1);
    body.extend([LocalSet(8), End]);
    fields(&mut body, 3, 0);
    body.extend([LocalGet(4), I32Const(2), ArrayGet(ARGS), LocalTee(10), RefIsNull, If(BlockType::Empty)]);
    absent(&mut body);
    body.extend([End, I32Const(0), LocalSet(9)]);
    body.extend([Loop(BlockType::Empty)]);
    // Inside the loop, the miss block is depth 1.
    nominal(&mut body, 10, 1);
    body.extend([LocalGet(14), GlobalGet(BITMAP_NODE_DESCRIPTOR_GLOBAL), RefEq, If(BlockType::Empty)]);
    // A bitmap node; inside this If the miss block is depth 2.
    fields(&mut body, 3, 2);
    body.extend([LocalGet(4), I32Const(1), ArrayGet(ARGS)]);
    int(&mut body, 2);
    body.extend([
        LocalSet(11),
        I32Const(1), LocalGet(8), LocalGet(9), I32ShrU, I32Const(31), I32And, I32Shl, LocalTee(12),
        LocalGet(11), I32And, I32Eqz,
        If(BlockType::Empty),
    ]);
    absent(&mut body);
    body.push(End);
    storage(&mut body, 2, 2);
    body.extend([
        LocalGet(11), LocalGet(12), I32Const(1), I32Sub, I32And, I32Popcnt, I32Const(1), I32Shl, LocalTee(6),
        I32Const(1), I32Add, LocalGet(5), ArrayLen, I32GeU, BrIf(2),
        LocalGet(5), LocalGet(6), I32Const(1), I32Add, ArrayGet(ARGS), LocalSet(7),
        LocalGet(5), LocalGet(6), ArrayGet(ARGS), LocalTee(10), RefIsNull,
        If(BlockType::Empty),
        // A subnode, one level down.
        LocalGet(7), LocalSet(10), LocalGet(9), I32Const(5), I32Add, LocalSet(9), Br(2),
        End,
        LocalGet(10), LocalGet(1), RefEq,
        If(BlockType::Empty),
    ]);
    found(&mut body);
    body.push(End);
    absent(&mut body);
    body.push(End);
    body.extend([LocalGet(14), GlobalGet(ARRAY_NODE_DESCRIPTOR_GLOBAL), RefEq, I32Eqz, BrIf(1)]);
    // An array node: the child at the hash's next five bits.
    fields(&mut body, 3, 1);
    storage(&mut body, 2, 1);
    body.extend([
        LocalGet(8), LocalGet(9), I32ShrU, I32Const(31), I32And, LocalTee(6),
        LocalGet(5), ArrayLen, I32GeU, BrIf(1),
        LocalGet(5), LocalGet(6), ArrayGet(ARGS), LocalTee(10), RefIsNull,
        If(BlockType::Empty),
    ]);
    absent(&mut body);
    body.extend([End, LocalGet(9), I32Const(5), I32Add, LocalSet(9), Br(0), End]);
    body.extend([End, sentinel_value(UNDEFINED)]);
    b.function_with_locals(
        "map-lookup",
        &[VALUE, VALUE, ValType::I32, VALUE],
        &[VALUE],
        &[
            (2, typed(ARGS)),
            (1, ValType::I32),
            (1, VALUE),
            (2, ValType::I32),
            (1, VALUE),
            (2, ValType::I32),
            (1, typed(7)),
            (1, typed(DESCRIPTOR)),
            (1, ValType::I64),
            (1, typed(7)),
        ],
        &body,
    );
}
