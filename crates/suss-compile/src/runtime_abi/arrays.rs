//! The array functions and vector reads (#378, #425). Suss's arrays are
//! Clojure's: the packed arrays of `packed_arrays`, fixed-length, with no
//! holes, properties or methods. `aget`, `aset` and `alength` of anything
//! else throw.
use super::*;
mod bulk_copy;
/// A global slot that source arrays used, kept so that later globals keep
/// their indices (#425).
pub(super) const TAG_GLOBAL: u32 = closure_properties::TAG_GLOBAL + 1;
/// The slot of the removed JS array methods' root, kept likewise.
pub(super) const RETIRED_METHOD_ROOT: u32 = native_objects::TAG_GLOBAL + 2;
/// The removed array join stack's slot, kept likewise.
pub(super) const RETIRED_JOIN_ACTIVE: u32 = range_errors::DESCRIPTOR_GLOBAL + 6;
/// `count` mutable nil globals in the slots of removed features.
pub(super) fn append_retired_globals(globals: &mut GlobalSection, count: usize) {
    for _ in 0..count {
        globals.global(
            GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::extended([sentinel_value(NIL)]),
        );
    }
}
// Keep malformed storage/unsupported properties in the language exception path.
fn error(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    let units: Vec<_> = "Invalid or unsupported array operation"
        .encode_utf16()
        .collect();
    body.push(GlobalGet(nominal::ERROR_GLOBAL));
    body.extend(units.iter().map(|unit| I32Const(*unit as i32)));
    body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        },
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        StructNew(8),
        Throw(0),
    ]);
}
// Bootstrap resource bound for named and native object tables: limits are
// diagnostics, never allocator/trap failures.
pub(super) const MAX_LENGTH: i32 = 1_000_000;
fn array(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
fn arg(body: &mut Vec<Instruction<'static>>, index: i32) {
    use Instruction::*;
    body.extend([LocalGet(1), I32Const(index), ArrayGet(ARGS)]);
}
fn callback(b: &mut Builder, locals: &[(u32, ValType)], body: &[Instruction<'static>]) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut code = Function::new(locals.iter().copied());
    for instruction in body {
        code.instruction(instruction);
    }
    code.instruction(&Instruction::End);
    b.code.function(&code);
    b.count += 1;
    index
}
fn factory(
    b: &mut Builder,
    name: &str,
    minimum: i32,
    maximum: i32,
    body: &[Instruction<'static>],
) -> u32 {
    use Instruction::*;
    let f = callback(b, &[], body);
    let family = match name {
        "array-get-function" => Some("array-get"),
        "array-set-function" => Some("array-set"),
        "array-make-function" => Some("array-make"),
        _ => None,
    };
    let mut factory = vec![];
    if let Some(family) = family {
        factory.extend([GlobalGet(apply::native_code(family)), sentinel_value(NIL), I32Const(0), RefI31]);
    }
    factory.extend([sentinel_value(NIL), RefFunc(f), I32Const(minimum), I32Const(maximum),
        if family.is_some() { StructNew(UNIVERSAL) } else { Call(b.names["closure-new"]) }]);
    b.function(name, &[], &[VALUE], &factory);
    f
}

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    bulk_copy::functions(b);
    let packed = |body: &mut Vec<Instruction<'static>>, local: u32| {
        body.extend([LocalGet(local), RefTestNonNull(HeapType::Concrete(PACKED)), I32Eqz, If(BlockType::Empty)]);
        error(body);
        body.push(End);
    };
    // `array-get(a, i)` and `array-set(a, i, v)`: one index of an array.
    let mut body = vec![];
    packed(&mut body, 0);
    body.extend([LocalGet(0), LocalGet(1), Call(b.names["packed-array-get-2"])]);
    let get = b.function("array-get", &[VALUE, VALUE], &[VALUE], &body);
    let mut body = vec![];
    packed(&mut body, 0);
    body.extend([LocalGet(0), LocalGet(1), LocalGet(2), Call(b.names["packed-array-set-3"])]);
    let set = b.function("array-set", &[VALUE, VALUE, VALUE], &[VALUE], &body);
    // `array-length-args([a])`: alength.
    let mut body = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, If(BlockType::Empty)];
    error(&mut body);
    body.push(End);
    array(&mut body, 0);
    body.extend([ArrayLen, I32Const(1), I32Ne, If(BlockType::Empty)]);
    error(&mut body);
    body.extend([End, LocalGet(0), RefCastNonNull(HeapType::Concrete(ARGS)), I32Const(0), ArrayGet(ARGS), LocalSet(1)]);
    packed(&mut body, 1);
    body.extend([LocalGet(1), Call(b.names["packed-array-length"])]);
    let length = b.function_with_locals("array-length-args", &[VALUE], &[VALUE], &[(1, VALUE)], &body);
    // `array-clone(a)`: `(aclone a)` inline (#443), as the closure answers.
    let mut body = vec![];
    packed(&mut body, 0);
    body.extend([LocalGet(0), Call(b.names["packed-array-clone"])]);
    b.function("array-clone", &[VALUE], &[VALUE], &body);
    // `array-storage(a)`: an object array's elements as a fresh argument
    // array, as `.apply` and the call paths spread them.
    let mut body = vec![LocalGet(0), Call(b.names["object-array-elements"]), BrOnNonNull(0)];
    error(&mut body);
    b.function("array-storage", &[VALUE], &[VALUE], &body);
    let mut declared = vec![];
    declared.push(factory(
        b,
        "array-function",
        0,
        -1,
        // Clojure's object array of the arguments (#425).
        &[LocalGet(1), Call(b.names["object-array-copy"])],
    ));
    // Every packed array is an array.
    let mut body = vec![];
    arg(&mut body, 0);
    body.push(Call(b.names["packed-array?"]));
    body.extend(boolean());
    declared.push(factory(b, "array-predicate-function", 1, 1, &body));
    let mut body = vec![LocalGet(1), Call(length)];
    declared.push(factory(b, "array-length-function", 1, 1, &body));
    body = vec![];
    arg(&mut body, 0);
    body.extend([LocalSet(2)]);
    packed(&mut body, 2);
    body.extend([LocalGet(2), Call(b.names["packed-array-clone"])]);
    declared.push(clone_factory(b, &body));
    let typed = |ty: u32| ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(ty) });
    // `vector-nth(v, n)` (#378): element n of PersistentVector v, walking
    // its trie as unchecked-array-for does, or undefined for anything that
    // isn't that shape, an index that isn't an i31 in range, or a slot
    // that's absent, which the caller answers with its general path. Core's
    // PersistentVector is [meta cnt shift root tail __hash] and VectorNode
    // [edit arr]; every step is tested, so another shape only misses.
    // Locals: 2 the fields, 3 the index, 4 the count, 5 the level, 6 the
    // node or array, 7 a value.
    // `vector-type-register(v)`: core names PersistentVector by its empty
    // vector, once the type is defined, and gets v back.
    b.function(
        "vector-type-register",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0), RefCastNonNull(HeapType::Concrete(7)),
            StructGet { struct_type_index: 7, field_index: 0 },
            GlobalSet(VECTOR_DESCRIPTOR_GLOBAL),
            LocalGet(0),
        ],
    );
    // `vector-count(v)`: a PersistentVector's cnt field, or undefined for
    // anything else.
    b.function(
        "vector-count",
        &[VALUE],
        &[VALUE],
        &[
            Block(BlockType::Empty),
            LocalGet(0), RefTestNonNull(HeapType::Concrete(7)), I32Eqz, BrIf(0),
            LocalGet(0), RefCastNonNull(HeapType::Concrete(7)),
            StructGet { struct_type_index: 7, field_index: 0 },
            GlobalGet(VECTOR_DESCRIPTOR_GLOBAL), RefEq, I32Eqz, BrIf(0),
            LocalGet(0), RefCastNonNull(HeapType::Concrete(7)),
            StructGet { struct_type_index: 7, field_index: 1 },
            I32Const(1), ArrayGet(ARGS), Return,
            End,
            sentinel_value(UNDEFINED),
        ],
    );
    // Locals: 2 the fields, 3 the index, 4 the count, 5 the level, 6 the
    // node or array, 7 a value, 8 an object array's storage, 9 the vector.
    let exact = |ty: u32| RefType { nullable: false, heap_type: HeapType::Concrete(ty) };
    // A cast that takes `miss` (a depth outside the cast's own block) when the
    // value on the stack isn't of type `ty`, and leaves it cast otherwise.
    let cast = |ty: u32, miss: u32| {
        [BrOnCast { relative_depth: 0, from_ref_type: RefType::EQREF, to_ref_type: exact(ty) }, Drop, Br(miss + 1)]
    };
    let field = |body: &mut Vec<Instruction<'static>>, index: i32| {
        body.extend([LocalGet(2), I32Const(index), ArrayGet(ARGS)]);
    };
    // The i31 on the stack into `local`; a miss for anything else.
    let small = |body: &mut Vec<Instruction<'static>>, local: u32, miss: u32| {
        body.extend([LocalTee(7), RefTestNonNull(HeapType::I31), I32Eqz, BrIf(miss),
            LocalGet(7), RefCastNonNull(HeapType::I31), I31GetS, LocalSet(local)]);
    };
    // The storage of the object array in local 6, into local 8.
    let storage = |body: &mut Vec<Instruction<'static>>, miss: u32| {
        body.extend([
            Block(BlockType::Result(ValType::Ref(exact(ARGS)))),
            Block(BlockType::Result(ValType::Ref(exact(PACKED)))),
            LocalGet(6),
        ]);
        body.extend(cast(PACKED, miss + 1));
        body.extend([End, StructGet { struct_type_index: PACKED, field_index: 0 }]);
        body.extend(cast(ARGS, miss));
        body.extend([End, LocalSet(8)]);
    };
    // A VectorNode's arr, from the node in local 6, back into local 6.
    let node_array = |body: &mut Vec<Instruction<'static>>, miss: u32| {
        body.extend([Block(BlockType::Result(ValType::Ref(exact(7)))), LocalGet(6)]);
        body.extend(cast(7, miss));
        body.extend([End, StructGet { struct_type_index: 7, field_index: 1 },
            LocalTee(2), ArrayLen, I32Const(2), I32LtU, BrIf(miss),
            LocalGet(2), I32Const(1), ArrayGet(ARGS), LocalSet(6)]);
    };
    // Element `index & 31` of the object array in local 6, onto the stack.
    let element = |body: &mut Vec<Instruction<'static>>, shift: Option<u32>, miss: u32| {
        storage(body, miss);
        body.push(LocalGet(3));
        if let Some(level) = shift {
            body.extend([LocalGet(level), I32ShrU]);
        }
        body.extend([I32Const(31), I32And, LocalTee(4), LocalGet(8), ArrayLen, I32GeU, BrIf(miss),
            LocalGet(8), LocalGet(4), ArrayGet(ARGS)]);
    };
    let mut body = vec![Block(BlockType::Empty)];
    body.extend([LocalGet(1), RefTestNonNull(HeapType::I31), I32Eqz, BrIf(0),
        LocalGet(1), RefCastNonNull(HeapType::I31), I31GetS, LocalSet(3),
        Block(BlockType::Result(ValType::Ref(exact(7)))), LocalGet(0)]);
    body.extend(cast(7, 0));
    // Only PersistentVector, by its registered descriptor.
    body.extend([End, LocalTee(9), StructGet { struct_type_index: 7, field_index: 0 },
        GlobalGet(VECTOR_DESCRIPTOR_GLOBAL), RefEq, I32Eqz, BrIf(0),
        LocalGet(9), StructGet { struct_type_index: 7, field_index: 1 },
        LocalTee(2), ArrayLen, I32Const(6), I32LtU, BrIf(0)]);
    field(&mut body, 1);
    small(&mut body, 4, 0);
    // 0 <= n < cnt, as an unsigned comparison.
    body.extend([LocalGet(3), LocalGet(4), I32GeU, BrIf(0)]);
    // n >= tail-off: the tail, which is the cnt < 32 case too.
    body.extend([
        LocalGet(4), I32Const(32), I32LtS,
        LocalGet(3),
        LocalGet(4), I32Const(1), I32Sub, I32Const(5), I32ShrU, I32Const(5), I32Shl,
        I32GeS, I32Or,
        If(BlockType::Empty),
    ]);
    field(&mut body, 4);
    body.extend([LocalSet(6), Else]);
    // Within this If, branch depth 1 is the outer block.
    field(&mut body, 2);
    small(&mut body, 5, 1);
    field(&mut body, 3);
    body.extend([LocalSet(6), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(5), I32Const(0), I32LeS, BrIf(1)]);
    // Inside block, loop: the outer miss block is depth 3.
    node_array(&mut body, 3);
    element(&mut body, Some(5), 3);
    body.extend([LocalSet(6), LocalGet(5), I32Const(5), I32Sub, LocalSet(5), Br(0), End, End]);
    node_array(&mut body, 1);
    body.push(End);
    element(&mut body, None, 0);
    body.extend([Return, End, sentinel_value(UNDEFINED)]);
    b.function_with_locals(
        "vector-nth",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, typed(ARGS)), (3, ValType::I32), (1, VALUE), (1, VALUE), (1, typed(ARGS)), (1, typed(7))],
        &body,
    );
    // `array-map-type-register(m)` (#442): core names PersistentArrayMap by
    // its empty map, once the type is defined, and gets m back.
    b.function(
        "array-map-type-register",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0), RefCastNonNull(HeapType::Concrete(7)),
            StructGet { struct_type_index: 7, field_index: 0 },
            GlobalSet(ARRAY_MAP_DESCRIPTOR_GLOBAL),
            LocalGet(0),
        ],
    );
    // `aget` and `aset` with any number of indices walk the nested arrays,
    // as Clojure's do.
    for (name, writing) in [("array-get-indices", false), ("array-set-indices", true)] {
        let mut body = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, If(BlockType::Empty)];
        error(&mut body);
        body.push(End);
        array(&mut body, 0);
        body.extend([ArrayLen, LocalSet(1), LocalGet(1), I32Const(if writing { 3 } else { 2 }), I32LtU, If(BlockType::Empty)]);
        error(&mut body);
        body.push(End);
        array(&mut body, 0);
        body.extend([
            I32Const(0), ArrayGet(ARGS), LocalSet(2),
            I32Const(1), LocalSet(3),
            Block(BlockType::Empty), Loop(BlockType::Empty),
            LocalGet(3), LocalGet(1), I32Const(if writing { 2 } else { 0 }), I32Sub, I32GeU, BrIf(1),
            LocalGet(2),
        ]);
        array(&mut body, 0);
        body.extend([
            LocalGet(3), ArrayGet(ARGS), Call(get), LocalSet(2),
            LocalGet(3), I32Const(1), I32Add, LocalSet(3),
            Br(0), End, End,
            LocalGet(2),
        ]);
        if writing {
            array(&mut body, 0);
            body.extend([LocalGet(1), I32Const(2), I32Sub, ArrayGet(ARGS)]);
            array(&mut body, 0);
            body.extend([LocalGet(1), I32Const(1), I32Sub, ArrayGet(ARGS), Call(set)]);
        }
        let primitive = b.function_with_locals(
            name,
            &[VALUE],
            &[VALUE],
            &[(1, ValType::I32), (1, VALUE), (1, ValType::I32)],
            &body,
        );
        declared.push(factory(
            b,
            if writing { "array-set-function" } else { "array-get-function" },
            if writing { 3 } else { 2 },
            -1,
            &[LocalGet(1), Call(primitive)],
        ));
    }
    declared.push(factory(
        b,
        "array-make-function",
        1,
        -1,
        &[LocalGet(1), Call(b.names["object-array-make-args"])],
    ));
    declared
}

/// `aclone`'s first-class function; its callback has a local for the array.
fn clone_factory(b: &mut Builder, body: &[Instruction<'static>]) -> u32 {
    use Instruction::*;
    let f = callback(b, &[(1, VALUE)], body);
    b.function(
        "array-clone-function",
        &[],
        &[VALUE],
        &[sentinel_value(NIL), RefFunc(f), I32Const(1), I32Const(1), Call(b.names["closure-new"])],
    );
    f
}
