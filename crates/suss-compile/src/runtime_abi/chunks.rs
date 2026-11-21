//! Guarded access to core's canonical ArrayChunk and ChunkBuffer (#482).
//! Custom chunks and unsupported storage/indices retain source dispatch.
use super::*;
use Instruction::*;

const ARRAY_CHUNK: u32 = apply::GLOBAL_LIMIT;
const CHUNK_BUFFER: u32 = ARRAY_CHUNK + 1;
const CHUNKED_CONS: u32 = CHUNK_BUFFER + 1;
pub(super) const GLOBAL_LIMIT: u32 = CHUNKED_CONS + 1;

pub(super) fn append_globals(globals: &mut GlobalSection) {
    assert_eq!(globals.len(), ARRAY_CHUNK);
    for _ in 0..3 {
        globals.global(GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::extended([sentinel_value(NIL)]));
    }
}

fn nullable(ty: u32) -> ValType {
    ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(ty) })
}
fn cast(ty: HeapType) -> Instruction<'static> {
    BrOnCastFail { relative_depth: 0, from_ref_type: RefType::EQREF,
        to_ref_type: RefType { nullable: false, heap_type: ty } }
}
fn decline_if(code: &mut Vec<Instruction<'static>>) {
    code.extend([If(BlockType::Empty), sentinel_value(UNDEFINED), Return, End]);
}
fn fields(code: &mut Vec<Instruction<'static>>, descriptor: u32, count: i32) {
    code.extend([LocalGet(0), cast(HeapType::Concrete(OBJECT)), LocalTee(2),
        StructGet { struct_type_index: OBJECT, field_index: 0 }, GlobalGet(descriptor), RefEq, I32Eqz]);
    decline_if(code);
    code.extend([LocalGet(2), RefAsNonNull, StructGet { struct_type_index: OBJECT, field_index: 1 },
        LocalTee(3), ArrayLen, I32Const(count), I32Ne]);
    decline_if(code);
}
fn field(code: &mut Vec<Instruction<'static>>, index: i32) {
    code.extend([LocalGet(3), RefAsNonNull, I32Const(index), ArrayGet(ARGS)]);
}
fn storage(code: &mut Vec<Instruction<'static>>) {
    field(code, 0);
    code.extend([cast(HeapType::Concrete(PACKED)),
        StructGet { struct_type_index: PACKED, field_index: 0 },
        cast(HeapType::Concrete(ARGS)), LocalSet(4)]);
}
fn declined(code: &mut Vec<Instruction<'static>>) {
    // br_on_cast_fail carries its failed reference out of the result block.
    code.extend([End, Drop, sentinel_value(UNDEFINED)]);
}

pub(super) fn functions(b: &mut Builder) {
    long_sum(b);
    // Stage all descriptors before publication. The instances are only
    // witnesses supplied by the owned core loader; they need not be retained.
    let mut code = vec![];
    for i in 0..3 {
        code.extend([LocalGet(i), RefCastNonNull(HeapType::Concrete(OBJECT)),
            StructGet { struct_type_index: OBJECT, field_index: 0 }, LocalSet(i + 3)]);
    }
    code.extend([LocalGet(3), GlobalSet(ARRAY_CHUNK), LocalGet(4), GlobalSet(CHUNK_BUFFER),
        LocalGet(5), GlobalSet(CHUNKED_CONS), sentinel_value(NIL)]);
    b.function_with_locals("chunk-types-register", &[VALUE; 3], &[VALUE], &[(3, VALUE)], &code);

    // Admit the core sequence wrapper before using chunk APIs in str.
    // A user IChunkedSeq may expose different first/next effects; it must
    // retain the generic traversal even if its chunk is an ArrayChunk.
    code = vec![Block(BlockType::Result(VALUE))];
    fields(&mut code, CHUNKED_CONS, 4);
    field(&mut code, 0);
    code.push(Return);
    declined(&mut code);
    b.function_with_locals("chunk-seq", &[VALUE], &[VALUE],
        &[(1, VALUE), (1, nullable(OBJECT)), (1, nullable(ARGS))], &code);

    // Parameters: chunk/index. Locals: object, fields, Object[] storage,
    // index, offset and their sum. No callbacks or allocations intervene.
    code = vec![Block(BlockType::Result(VALUE))];
    fields(&mut code, ARRAY_CHUNK, 3);
    let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
    code.extend([LocalGet(1), cast(i31), I31GetS, LocalSet(5)]);
    field(&mut code, 1);
    code.extend([cast(i31), I31GetS, LocalSet(6),
        LocalGet(5), LocalGet(6), I32Or, I32Const(0), I32LtS]);
    decline_if(&mut code);
    storage(&mut code);
    // Nonnegative i31s sum to at most 2^31-2. Two-argument nth uses the
    // underlying array bounds, not the chunk end, as on JVM Clojure.
    code.extend([LocalGet(5), LocalGet(6), I32Add, LocalTee(7),
        LocalGet(4), RefAsNonNull, ArrayLen, I32GeU]);
    decline_if(&mut code);
    code.extend([LocalGet(4), RefAsNonNull, LocalGet(7), ArrayGet(ARGS), Return]);
    declined(&mut code);
    b.function_with_locals("chunk-nth", &[VALUE; 2], &[VALUE],
        &[(1, nullable(OBJECT)), (2, nullable(ARGS)), (3, ValType::I32)], &code);

    // A canonical, well-formed Object[] window can use a loop-invariant
    // typed array view. Its storage and offset/end fields are immutable;
    // element reads stay inside the loop, after the preceding callback.
    code = vec![Block(BlockType::Result(VALUE))];
    fields(&mut code, ARRAY_CHUNK, 3);
    field(&mut code, 1);
    code.extend([cast(i31), I31GetS, LocalSet(5)]);
    field(&mut code, 2);
    code.extend([cast(i31), I31GetS, LocalSet(6),
        LocalGet(5), LocalGet(6), I32Or, I32Const(0), I32LtS]);
    decline_if(&mut code);
    code.extend([LocalGet(5), LocalGet(6), I32GtU]);
    decline_if(&mut code);
    storage(&mut code);
    code.extend([LocalGet(6), LocalGet(4), RefAsNonNull, ArrayLen, I32GtU]);
    decline_if(&mut code);
    field(&mut code, 0);
    code.push(Return);
    declined(&mut code);
    b.function_with_locals("chunk-array", &[VALUE], &[VALUE],
        &[(1, VALUE), (1, nullable(OBJECT)), (2, nullable(ARGS)), (2, ValType::I32)], &code);

    // Called once after chunk-array admits a window. Keep independent
    // private calls checked too, without invoking a protocol or callback.
    code = vec![Block(BlockType::Result(VALUE))];
    fields(&mut code, ARRAY_CHUNK, 3);
    field(&mut code, 1);
    code.extend([cast(i31), Return]);
    declined(&mut code);
    b.function_with_locals("chunk-offset", &[VALUE], &[VALUE],
        &[(1, VALUE), (1, nullable(OBJECT)), (1, nullable(ARGS))], &code);

    // Parameters: buffer/value. Same three rooted references, then its end.
    // A failed range declines before writes; source add increments before
    // throwing. An admitted copy cannot throw or call user code in between
    // the index increment and store, so its effects have the same order.
    code = vec![Block(BlockType::Result(VALUE))];
    fields(&mut code, CHUNK_BUFFER, 2);
    field(&mut code, 1);
    code.extend([cast(i31), I31GetS, LocalTee(5), I32Const(0x3fff_ffff), I32GeU]);
    decline_if(&mut code); // Negative ends and an increment outside i31.
    storage(&mut code);
    code.extend([LocalGet(5), LocalGet(4), RefAsNonNull, ArrayLen, I32GeU]);
    decline_if(&mut code);
    code.extend([LocalGet(3), RefAsNonNull, I32Const(1), LocalGet(5), I32Const(1), I32Add,
        RefI31, ArraySet(ARGS), LocalGet(4), RefAsNonNull, LocalGet(5), LocalGet(1),
        ArraySet(ARGS), sentinel_value(NIL), Return]);
    declined(&mut code);
    b.function_with_locals("chunk-buffer-append", &[VALUE; 2], &[VALUE],
        &[(1, nullable(OBJECT)), (2, nullable(ARGS)), (1, ValType::I32)], &code);
}


// The owned caller supplies its immutable canonical + value. A different
// reducer declines; this helper never invokes arbitrary user code. It may
// recompute a pure long prefix on fallback, but never writes existing storage.
fn long_sum(b: &mut Builder) {
    // Params reducer/canonical +/array/offset/count/initial. Locals6 storage,
    // 7 offset,8 count,9 end,10 index,11 element,12 accumulator,13 operand,
    // 14 sum,15 boxing scratch. Bounds are checked before any element load.
    let i31 = HeapType::I31;
    let mut code = vec![LocalGet(0), LocalGet(1), RefEq, I32Eqz];
    decline_if(&mut code);
    code.extend([Block(BlockType::Result(VALUE)),
        LocalGet(2), cast(HeapType::Concrete(PACKED)),
        StructGet { struct_type_index: PACKED, field_index: 0 },
        cast(HeapType::Concrete(ARGS)), LocalSet(6),
        LocalGet(3), cast(i31), I31GetS, LocalSet(7),
        LocalGet(4), cast(i31), I31GetS, LocalSet(8),
        LocalGet(7), LocalGet(8), I32Or, I32Const(0), I32LtS]);
    decline_if(&mut code);
    code.extend([LocalGet(7), LocalGet(8), I32Add, LocalTee(9),
        LocalGet(6), RefAsNonNull, ArrayLen, I32GtU]);
    decline_if(&mut code);
    code.extend([LocalGet(8), I32Eqz, If(BlockType::Empty), LocalGet(5), Return, End,
        LocalGet(5), LocalSet(11)]);
    let unbox = |code: &mut Vec<Instruction<'static>>, destination: u32, depth: u32| {
        code.extend([LocalGet(11), RefTestNonNull(i31), If(BlockType::Result(ValType::I64)),
            LocalGet(11), RefCastNonNull(i31), I31GetS, I64ExtendI32S,
            Else, LocalGet(11), BrOnCastFail { relative_depth: depth + 1,
                from_ref_type: RefType::EQREF,
                to_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(LONG) } },
            StructGet { struct_type_index: LONG, field_index: 0 }, End, LocalSet(destination)]);
    };
    unbox(&mut code, 12, 0);
    code.extend([LocalGet(7), LocalSet(10), Loop(BlockType::Empty),
        LocalGet(10), LocalGet(9), I32GeU, If(BlockType::Empty), LocalGet(12)]);
    code.extend(long_box_in(15));
    code.extend([Return, End, LocalGet(6), RefAsNonNull, LocalGet(10), ArrayGet(ARGS), LocalSet(11)]);
    unbox(&mut code, 13, 1);
    // Preserve checked prefix overflow, including before a later unsupported
    // value. The existing numeric helper supplies the exact language error.
    code.extend([LocalGet(12), LocalGet(13), I64Add, LocalTee(14), LocalGet(12), I64Xor,
        LocalGet(14), LocalGet(13), I64Xor, I64And, I64Const(0), I64LtS,
        If(BlockType::Empty), LocalGet(12)]);
    code.extend(long_box_in(15));
    code.extend([LocalGet(11), Call(b.names["number-add"]), Return, End,
        LocalGet(14), LocalSet(12), LocalGet(10), I32Const(1), I32Add, LocalSet(10), Br(0),
        End, Unreachable]);
    declined(&mut code);
    b.function_with_locals("chunk-long-sum", &[VALUE; 6], &[VALUE],
        &[(1, nullable(ARGS)), (4, ValType::I32), (1, VALUE), (4, ValType::I64)], &code);
}
