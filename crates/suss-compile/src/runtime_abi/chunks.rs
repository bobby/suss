//! Guarded access to core's canonical ArrayChunk and ChunkBuffer (#482).
//! Custom chunks and unsupported storage/indices retain source dispatch.
use super::*;
use Instruction::*;

const ARRAY_CHUNK: u32 = apply::GLOBAL_LIMIT;
const CHUNK_BUFFER: u32 = ARRAY_CHUNK + 1;

pub(super) fn append_globals(globals: &mut GlobalSection) {
    assert_eq!(globals.len(), ARRAY_CHUNK);
    for _ in 0..2 {
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
    // Stage both descriptors before publication. The instances are only
    // witnesses supplied by the owned core loader; they need not be retained.
    let mut code = vec![];
    for i in 0..2 {
        code.extend([LocalGet(i), RefCastNonNull(HeapType::Concrete(OBJECT)),
            StructGet { struct_type_index: OBJECT, field_index: 0 }, LocalSet(i + 2)]);
    }
    code.extend([LocalGet(2), GlobalSet(ARRAY_CHUNK), LocalGet(3), GlobalSet(CHUNK_BUFFER), sentinel_value(NIL)]);
    b.function_with_locals("chunk-types-register", &[VALUE; 2], &[VALUE], &[(2, VALUE)], &code);

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
