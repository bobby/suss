//! Direct reads of core's persistent maps and sets (#442).
//!
//! `map-lookup(m, k, mode)` answers a lookup in a PersistentArrayMap, a
//! PersistentHashMap or a PersistentHashSet, and `get` on a PersistentVector
//! through vector-nth, without calling into core, when
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
    // A vector's get is its nth (#433): vector-nth answers it, undefined for
    // an index out of range or any other key, which core answers.
    body.extend([
        LocalGet(14), GlobalGet(VECTOR_DESCRIPTOR_GLOBAL), RefEq, LocalGet(2), I32Eqz, I32And,
        If(BlockType::Empty), LocalGet(0), LocalGet(1), ReturnCall(b.names["vector-nth"]), End,
    ]);
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
    assoc_functions(b);
}

/// `map-assoc(m, k, v)` (#443): `(assoc m k v)` of a PersistentHashMap whose
/// trie already holds `k`, a key map-lookup reads, with the path to it
/// copied and `v` in its place; `m` itself when `v` is already its value, as
/// core answers. Anything else answers undefined, for core's assoc: another
/// collection or key, a new key, a collision node or an unexpected shape.
/// Each new node is built as object-new builds it, with the old node's
/// descriptor and a copy of its prototype cell.
fn assoc_functions(b: &mut Builder) {
    use Instruction::*;
    let miss = || [sentinel_value(UNDEFINED), Return];
    let typed = |ty: u32| ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(ty) });
    // `object` (a nominal object) into `obj` and its descriptor into `desc`,
    // its fields into `fields`, needing at least `count`, else a miss; and
    // its prototype cell must be a one-element array.
    let object = |body: &mut Vec<Instruction<'static>>, object: u32, obj: u32, desc: u32, fields: u32, count: i32| {
        body.extend([LocalGet(object), RefTestNonNull(HeapType::Concrete(7)), I32Eqz, If(BlockType::Empty)]);
        body.extend(miss());
        body.extend([
            End,
            LocalGet(object), RefCastNonNull(HeapType::Concrete(7)), LocalTee(obj),
            StructGet { struct_type_index: 7, field_index: 0 }, LocalSet(desc),
            LocalGet(obj), StructGet { struct_type_index: 7, field_index: 1 },
            LocalTee(fields), ArrayLen, I32Const(count), I32LtU,
            LocalGet(obj), StructGet { struct_type_index: 7, field_index: 2 }, RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, I32Or,
            If(BlockType::Empty),
        ]);
        body.extend(miss());
        body.push(End);
    };
    // A new object of `obj`'s type and prototype with the fields on the stack.
    let rebuild = |body: &mut Vec<Instruction<'static>>, obj: u32, desc: u32, fields: u32| {
        body.extend([
            LocalSet(fields),
            LocalGet(desc), RefAsNonNull,
            LocalGet(fields), RefAsNonNull,
            LocalGet(obj), StructGet { struct_type_index: 7, field_index: 2 }, RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0), ArrayGet(ARGS),
            ArrayNewFixed { array_type_index: ARGS, array_size: 1 },
            I32Const(0), RefI31, // Identity UID, unassigned.
            StructNew(7),
        ]);
    };
    // Whether the value in `local` is no key: null or nil, a subnode's slot.
    let empty = |local: u32| [LocalGet(local), RefIsNull, LocalGet(local), sentinel_value(NIL), RefEq, I32Or];

    // `map-assoc-node(node, shift, hash, k, v)`: the node with k's value v,
    // the node itself when unchanged, or undefined. Params: 0 node, 1 shift,
    // 2 hash, 3 key, 4 value. Locals: 5 fields, 6 storage, 7 index, 8 bitmap,
    // 9 bit, 10 child, 11 new child, 12 the node object, 13 its descriptor,
    // 14 scratch, 15 the copy, 16 new fields.
    let node = b.count;
    let mut body = vec![];
    object(&mut body, 0, 12, 13, 5, 3);
    // The node's object array storage, into local 6.
    body.extend([
        LocalGet(5), I32Const(2), ArrayGet(ARGS), LocalTee(14), RefTestNonNull(HeapType::Concrete(PACKED)), I32Eqz,
        If(BlockType::Empty),
    ]);
    body.extend(miss());
    body.extend([
        End,
        LocalGet(14), RefCastNonNull(HeapType::Concrete(PACKED)), StructGet { struct_type_index: PACKED, field_index: 0 },
        LocalTee(14), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz,
        If(BlockType::Empty),
    ]);
    body.extend(miss());
    body.extend([End, LocalGet(14), RefCastNonNull(HeapType::Concrete(ARGS)), LocalSet(6)]);
    // The node's copy with `value` at the index on the stack, as fields
    // [nil, field 1, the copy] of a new node of its type.
    let copy = |body: &mut Vec<Instruction<'static>>, index: Vec<Instruction<'static>>, value: u32| {
        body.extend([
            LocalGet(6), ArrayLen, ArrayNewDefault(ARGS), LocalTee(15),
            I32Const(0), LocalGet(6), I32Const(0), LocalGet(6), ArrayLen,
            ArrayCopy { array_type_index_dst: ARGS, array_type_index_src: ARGS },
            LocalGet(15),
        ]);
        body.extend(index);
        body.extend([
            LocalGet(value), ArraySet(ARGS),
            sentinel_value(NIL),
            LocalGet(5), I32Const(1), ArrayGet(ARGS),
            LocalGet(15), RefAsNonNull, I32Const(0), RefI31, StructNew(PACKED),
            ArrayNewFixed { array_type_index: ARGS, array_size: 3 },
        ]);
        rebuild(body, 12, 13, 16);
        body.push(Return);
    };
    // One level down: the child in local 10, its answer into local 11. An
    // undefined answer is this one's; an unchanged child, an unchanged node.
    let descend = |body: &mut Vec<Instruction<'static>>| {
        body.extend([
            LocalGet(10), LocalGet(1), I32Const(5), I32Add, LocalGet(2), LocalGet(3), LocalGet(4), Call(node),
            LocalTee(11), sentinel_value(UNDEFINED), RefEq, If(BlockType::Empty),
        ]);
        body.extend(miss());
        body.extend([End, LocalGet(11), LocalGet(10), RefEq, If(BlockType::Empty), LocalGet(0), Return, End]);
    };
    // A bitmap node: [edit bitmap arr], its pairs at twice the bit's rank.
    body.extend([LocalGet(13), GlobalGet(BITMAP_NODE_DESCRIPTOR_GLOBAL), RefEq, If(BlockType::Empty)]);
    body.extend([
        LocalGet(5), I32Const(1), ArrayGet(ARGS), LocalTee(14), RefTestNonNull(HeapType::I31),
        If(BlockType::Result(ValType::I32)),
        LocalGet(14), RefCastNonNull(HeapType::I31), I31GetS,
        Else,
        LocalGet(14), RefTestNonNull(HeapType::Concrete(LONG)), I32Eqz, If(BlockType::Empty),
    ]);
    body.extend(miss());
    body.extend([
        End,
        LocalGet(14), RefCastNonNull(HeapType::Concrete(LONG)), StructGet { struct_type_index: LONG, field_index: 0 }, I32WrapI64,
        End,
        LocalSet(8),
        I32Const(1), LocalGet(2), LocalGet(1), I32ShrU, I32Const(31), I32And, I32Shl, LocalTee(9),
        LocalGet(8), I32And, I32Eqz, If(BlockType::Empty),
    ]);
    // A new key: core's to add.
    body.extend(miss());
    body.extend([
        End,
        LocalGet(8), LocalGet(9), I32Const(1), I32Sub, I32And, I32Popcnt, I32Const(1), I32Shl, LocalTee(7),
        I32Const(1), I32Add, LocalGet(6), ArrayLen, I32GeU, If(BlockType::Empty),
    ]);
    body.extend(miss());
    body.extend([End, LocalGet(6), LocalGet(7), ArrayGet(ARGS), LocalSet(14)]);
    body.extend(empty(14));
    body.extend([If(BlockType::Empty), LocalGet(6), LocalGet(7), I32Const(1), I32Add, ArrayGet(ARGS), LocalSet(10)]);
    descend(&mut body);
    copy(&mut body, vec![LocalGet(7), I32Const(1), I32Add], 11);
    body.push(End);
    // The key itself: its value replaced, or a collision, core's to split.
    body.extend([LocalGet(14), LocalGet(3), RefEq, I32Eqz, If(BlockType::Empty)]);
    body.extend(miss());
    body.extend([
        End,
        LocalGet(6), LocalGet(7), I32Const(1), I32Add, ArrayGet(ARGS), LocalGet(4), RefEq,
        If(BlockType::Empty), LocalGet(0), Return, End,
    ]);
    copy(&mut body, vec![LocalGet(7), I32Const(1), I32Add], 4);
    body.push(End);
    // An array node: [edit cnt arr], the child at the hash's next five bits.
    body.extend([LocalGet(13), GlobalGet(ARRAY_NODE_DESCRIPTOR_GLOBAL), RefEq, I32Eqz, If(BlockType::Empty)]);
    body.extend(miss());
    body.extend([
        End,
        LocalGet(2), LocalGet(1), I32ShrU, I32Const(31), I32And, LocalTee(7),
        LocalGet(6), ArrayLen, I32GeU, If(BlockType::Empty),
    ]);
    body.extend(miss());
    body.extend([End, LocalGet(6), LocalGet(7), ArrayGet(ARGS), LocalSet(10)]);
    body.extend(empty(10));
    body.push(If(BlockType::Empty));
    body.extend(miss());
    body.push(End);
    descend(&mut body);
    copy(&mut body, vec![LocalGet(7)], 11);
    b.function_with_locals(
        "map-assoc-node",
        &[VALUE, ValType::I32, ValType::I32, VALUE, VALUE],
        &[VALUE],
        &[
            (2, typed(ARGS)),     // 5 fields, 6 storage
            (3, ValType::I32),    // 7 index, 8 bitmap, 9 bit
            (2, VALUE),           // 10 child, 11 new child
            (1, typed(7)),        // 12 the node object
            (1, typed(DESCRIPTOR)), // 13 its descriptor
            (1, VALUE),           // 14 scratch
            (2, typed(ARGS)),     // 15 the copy, 16 new fields
        ],
        &body,
    );

    // `map-assoc(m, k, v)`. Params: 0 map, 1 key, 2 value. Locals: 3 fields,
    // 4 the map object, 5 its descriptor, 6 the hash, 7 the key as a long,
    // 8 scratch i32, 9 the keyword key, 10 its fields, 11 the root, 12 the
    // new root, 13 new fields, 14 a keyword's hash.
    let mut body = vec![];
    object(&mut body, 0, 4, 5, 3, 6);
    body.extend([LocalGet(5), GlobalGet(HASH_MAP_DESCRIPTOR_GLOBAL), RefEq, I32Eqz, If(BlockType::Empty)]);
    body.extend(miss());
    body.push(End);
    // The key's hash, as map-lookup takes it: an i31 long's, or an interned
    // keyword's cached hash.
    body.extend([
        LocalGet(1), RefTestNonNull(HeapType::I31),
        If(BlockType::Empty),
        LocalGet(1), RefCastNonNull(HeapType::I31), I31GetS, I64ExtendI32S, LocalSet(7),
    ]);
    body.extend(hash_long_in(7, 8));
    body.extend([LocalSet(6), Else]);
    body.extend([LocalGet(1), RefTestNonNull(HeapType::Concrete(7)), I32Eqz, If(BlockType::Empty)]);
    body.extend(miss());
    body.extend([
        End,
        LocalGet(1), RefCastNonNull(HeapType::Concrete(7)), LocalTee(9),
        StructGet { struct_type_index: 7, field_index: 0 }, GlobalGet(KEYWORD_DESCRIPTOR_GLOBAL), RefEq, I32Eqz,
        LocalGet(9), StructGet { struct_type_index: 7, field_index: 1 }, LocalTee(10), ArrayLen, I32Const(4), I32LtU, I32Or,
        If(BlockType::Empty),
    ]);
    body.extend(miss());
    body.extend([
        End,
        LocalGet(10), I32Const(3), ArrayGet(ARGS), LocalTee(14), RefTestNonNull(HeapType::I31),
        If(BlockType::Result(ValType::I32)),
        LocalGet(14), RefCastNonNull(HeapType::I31), I31GetS,
        Else,
        LocalGet(14), RefTestNonNull(HeapType::Concrete(LONG)), I32Eqz, If(BlockType::Empty),
    ]);
    body.extend(miss());
    body.extend([
        End,
        LocalGet(14), RefCastNonNull(HeapType::Concrete(LONG)), StructGet { struct_type_index: LONG, field_index: 0 }, I32WrapI64,
        End,
        LocalSet(6),
        End,
    ]);
    // [meta cnt root has_nil nil_val __hash]: the root, then the new map with
    // the new root and no cached hash.
    body.extend([LocalGet(3), I32Const(2), ArrayGet(ARGS), LocalSet(11)]);
    body.extend(empty(11));
    body.push(If(BlockType::Empty));
    body.extend(miss());
    body.extend([
        End,
        LocalGet(11), I32Const(0), LocalGet(6), LocalGet(1), LocalGet(2), Call(node),
        LocalTee(12), sentinel_value(UNDEFINED), RefEq, If(BlockType::Empty),
    ]);
    body.extend(miss());
    body.extend([
        End,
        LocalGet(12), LocalGet(11), RefEq, If(BlockType::Empty), LocalGet(0), Return, End,
        LocalGet(3), I32Const(0), ArrayGet(ARGS),
        LocalGet(3), I32Const(1), ArrayGet(ARGS),
        LocalGet(12),
        LocalGet(3), I32Const(3), ArrayGet(ARGS),
        LocalGet(3), I32Const(4), ArrayGet(ARGS),
        sentinel_value(NIL),
        ArrayNewFixed { array_type_index: ARGS, array_size: 6 },
    ]);
    rebuild(&mut body, 4, 5, 13);
    b.function_with_locals(
        "map-assoc",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[
            (1, typed(ARGS)),       // 3 fields
            (1, typed(7)),          // 4 the map object
            (1, typed(DESCRIPTOR)), // 5 its descriptor
            (1, ValType::I32),      // 6 hash
            (1, ValType::I64),      // 7 key as a long
            (1, ValType::I32),      // 8 scratch
            (1, typed(7)),          // 9 the keyword key
            (1, typed(ARGS)),       // 10 its fields
            (2, VALUE),             // 11 root, 12 new root
            (1, typed(ARGS)),       // 13 new fields
            (1, VALUE),             // 14 keyword hash
        ],
        &body,
    );
}
