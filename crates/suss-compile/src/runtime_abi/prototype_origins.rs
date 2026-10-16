//! Private, optional provenance for prototypes made by the real inline factory.
//! No registration export exists. All arrays reachable from STATE are private,
//! freshly allocated and never mutated after their one reference publication.
use super::*;
use Instruction::*;
pub(super) const STATE: u32 = object_methods::CURRENT_THIS + 1;
const COUNT: i32 = 23;
const WIDTH: u32 = 13;
fn nil(c: &mut Vec<Instruction<'static>>) { c.extend([I32Const(0), RefI31]); }
fn array(c: &mut Vec<Instruction<'static>>, local: u32) {
    c.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(ARGS))]);
}
fn get(c: &mut Vec<Instruction<'static>>, local: u32, index: i32) {
    array(c, local); c.extend([I32Const(index), ArrayGet(ARGS)]);
}
fn mode(c: &mut Vec<Instruction<'static>>, local: u32) {
    get(c, local, 0); c.extend([RefCastNonNull(HeapType::I31), I31GetU]);
}
fn fail(c: &mut Vec<Instruction<'static>>) { nominal::error(c); }
pub(super) fn append_global(g: &mut GlobalSection) {
    assert_eq!(g.len(), STATE);
    g.global(GlobalType { val_type: VALUE, mutable: true, shared: false },
        &ConstExpr::extended([I32Const(0), RefI31]));
}
pub(super) fn functions(b: &mut Builder) {
    // Lifecycle controls are host-only: Session installation rejects their
    // imports even for authenticated artifacts. They cannot mint a factory entry.
    let mut c = vec![GlobalGet(STATE), RefTestNonNull(HeapType::Concrete(ARGS)), If(BlockType::Empty)];
    fail(&mut c); c.push(End);
    c.extend([I32Const(1), RefI31]); nil(&mut c); nil(&mut c); nil(&mut c);
    c.extend([ArrayNewFixed { array_type_index: ARGS, array_size: 4 }, GlobalSet(STATE)]);
    nil(&mut c);
    b.function("prototype-origin-begin", &[], &[VALUE], &c);

    // Local1 state,2 allowed,3 slots,4 constructor,5 creator descriptor,
    // 6 collection cursor,7 unused scratch,8 initial parent,9 i,10 j.
    let mut c = vec![GlobalGet(STATE), LocalTee(1), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, If(BlockType::Empty)];
    fail(&mut c); c.push(End);
    mode(&mut c, 1); c.extend([I32Const(1), I32Ne, If(BlockType::Empty)]); fail(&mut c); c.push(End);
    c.extend([LocalGet(0), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, If(BlockType::Empty)]); fail(&mut c); c.push(End);
    array(&mut c, 0); c.extend([ArrayLen, I32Const(COUNT), I32Ne, If(BlockType::Empty)]); fail(&mut c); c.push(End);
    c.extend([Call(b.names["native-object-default-prototype"]), LocalSet(8),
        I32Const(COUNT), ArrayNewDefault(ARGS), LocalSet(2),
        I32Const(COUNT), ArrayNewDefault(ARGS), LocalSet(3),
        I32Const(0), LocalSet(9), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(9), I32Const(COUNT), I32GeU, BrIf(1)]);
    array(&mut c, 0); c.extend([LocalGet(9), ArrayGet(ARGS), LocalSet(4),
        LocalGet(4), Call(b.names["constructor-descriptor"]), LocalSet(5),
        I32Const(0), LocalSet(10), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(10), LocalGet(9), I32GeU, BrIf(1)]);
    array(&mut c, 0); c.extend([LocalGet(10), ArrayGet(ARGS), LocalGet(4), RefEq, If(BlockType::Empty)]); fail(&mut c); c.push(End);
    c.extend([LocalGet(10), I32Const(1), I32Add, LocalSet(10), Br(0), End, End]);
    array(&mut c, 2); c.extend([LocalGet(9), LocalGet(4), LocalGet(5), ArrayNewFixed { array_type_index: ARGS, array_size: 2 }, ArraySet(ARGS)]);
    get(&mut c, 1, 2); c.extend([LocalSet(6), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(6), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, BrIf(1)]);
    get(&mut c, 6, 0); c.extend([LocalGet(4), RefEq]); get(&mut c, 6, 1);
    c.extend([LocalGet(5), RefEq, I32And, If(BlockType::Empty)]);
    array(&mut c, 3); c.push(LocalGet(9));
    // Strip the transient collection link, otherwise one slot retains its tail.
    for field in 0..12 { get(&mut c, 6, field); }
    nil(&mut c); c.extend([ArrayNewFixed { array_type_index: ARGS, array_size: WIDTH }, ArraySet(ARGS), Br(2), End]);
    get(&mut c, 6, 12); c.extend([LocalSet(6), Br(0), End, End,
        LocalGet(9), I32Const(1), I32Add, LocalSet(9), Br(0), End, End,
        I32Const(2), RefI31, LocalGet(2), LocalGet(3), LocalGet(8),
        ArrayNewFixed { array_type_index: ARGS, array_size: 4 }, GlobalSet(STATE)]);
    nil(&mut c);
    b.function_with_locals("prototype-origin-seal", &[VALUE], &[VALUE], &[(8, VALUE), (2, ValType::I32)], &c);

    // Query returns twelve freshly copied fields, never the entry or its link.
    // Parameters0 constructor,1 creator descriptor,2 consumed prototype;
    // locals3 state,4 allowed,5 slots,6 entry,7 allowed pair,8 i.
    let mut c = vec![GlobalGet(STATE), LocalTee(3), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, If(BlockType::Empty)];
    nil(&mut c); c.extend([Return, End]);
    mode(&mut c, 3); c.extend([I32Const(2), I32Ne, If(BlockType::Empty)]); nil(&mut c); c.extend([Return, End]);
    // Read the existing carrier; never invoke the allocating prototype getter.
    c.extend([LocalGet(0), I32Const(8), Call(b.names["closure-internal-property-get"]), LocalTee(7), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, If(BlockType::Empty)]);
    nil(&mut c); c.extend([Return, End]); array(&mut c, 7);
    c.extend([ArrayLen, I32Const(1), I32Ne, If(BlockType::Empty)]); nil(&mut c); c.extend([Return, End]);
    get(&mut c, 7, 0); c.extend([LocalGet(2), RefEq, I32Eqz, If(BlockType::Empty)]); nil(&mut c); c.extend([Return, End]);
    get(&mut c, 3, 1); c.push(LocalSet(4)); get(&mut c, 3, 2); c.push(LocalSet(5));
    c.extend([I32Const(0), LocalSet(8), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(8), I32Const(COUNT), I32GeU, BrIf(1)]);
    array(&mut c, 4); c.extend([LocalGet(8), ArrayGet(ARGS), LocalSet(7)]);
    get(&mut c, 7, 0); c.extend([LocalGet(0), RefEq]); get(&mut c, 7, 1);
    c.extend([LocalGet(1), RefEq, I32And, If(BlockType::Empty)]);
    array(&mut c, 5); c.extend([LocalGet(8), ArrayGet(ARGS), LocalTee(6), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, If(BlockType::Empty)]);
    nil(&mut c); c.extend([Return, End]);
    for (field, argument) in [(0, 0), (1, 1), (2, 2)] {
        get(&mut c, 6, field); c.extend([LocalGet(argument), RefEq, I32Eqz, If(BlockType::Empty)]);
        nil(&mut c); c.extend([Return, End]);
    }
    for field in 0..12 { get(&mut c, 6, field); }
    c.extend([ArrayNewFixed { array_type_index: ARGS, array_size: 12 }, Return, End,
        LocalGet(8), I32Const(1), I32Add, LocalSet(8), Br(0), End, End]); nil(&mut c);
    b.function_with_locals("prototype-origin-query", &[VALUE, VALUE, VALUE], &[VALUE], &[(5, VALUE), (1, ValType::I32)], &c);
    let mut c = vec![GlobalGet(STATE), LocalTee(0), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, If(BlockType::Empty)];
    nil(&mut c); c.extend([Return, End]); mode(&mut c, 0);
    c.extend([I32Const(2), I32Ne, If(BlockType::Empty)]); nil(&mut c); c.extend([Return, End]); get(&mut c, 0, 3);
    b.function_with_locals("prototype-origin-default-parent", &[], &[VALUE], &[(1, VALUE)], &c);
}

// Emit ONLY at the genuine factory's creation branch. Never a callable mint.
// Getter params0 ctor; existing locals1 creator descriptor,2 prototype.
// New locals3 state,4 allowed,5 slots/head,6 new entry,7 fields,8 pairs,
// 9 property descriptor,10 staged slots,11 index,12 mode.
pub(super) fn record_inline(c: &mut Vec<Instruction<'static>>) {
    c.extend([Block(BlockType::Empty), GlobalGet(STATE), LocalTee(3),
        RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, BrIf(0)]);
    mode(c, 3); c.push(LocalSet(12));
    c.extend([LocalGet(12), I32Const(2), I32Eq, If(BlockType::Empty)]);
    get(c, 3, 1); c.push(LocalSet(4)); get(c, 3, 2); c.push(LocalSet(5));
    c.extend([I32Const(0), LocalSet(11), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(11), I32Const(COUNT), I32GeU, BrIf(1)]);
    array(c, 4); c.extend([LocalGet(11), ArrayGet(ARGS), LocalSet(6)]);
    get(c, 6, 0); c.extend([LocalGet(0), RefEq]); get(c, 6, 1);
    c.extend([LocalGet(1), RefEq, I32And, BrIf(1), LocalGet(11), I32Const(1), I32Add, LocalSet(11), Br(0), End, End,
        LocalGet(11), I32Const(COUNT), I32GeU, BrIf(1), End]);
    // Only the one-own-property source prototype shape is in this policy.
    c.extend([LocalGet(2), RefCastNonNull(HeapType::Concrete(7)), StructGet { struct_type_index: 7, field_index: 1 }, LocalSet(7)]);
    get(c, 7, 0); c.push(LocalSet(8)); array(c, 8); c.extend([ArrayLen, I32Const(2), I32Ne, BrIf(0)]);
    get(c, 8, 1); c.push(LocalSet(9));
    // The immutable/private entry stores initial REFERENCES plus scalar inputs.
    // Its key reference is not trusted for spelling by the host audit.
    c.extend([LocalGet(0), LocalGet(1), LocalGet(2), LocalGet(2), RefCastNonNull(HeapType::Concrete(7)), StructGet { struct_type_index: 7, field_index: 0 }, LocalGet(7), LocalGet(8)]);
    get(c, 7, 1); get(c, 8, 0); c.push(LocalGet(9)); get(c, 9, 0); get(c, 9, 1); nil(c);
    c.extend([LocalGet(12), I32Const(1), I32Eq, If(BlockType::Result(VALUE))]); get(c, 3, 2); c.push(Else); nil(c); c.push(End);
    c.extend([ArrayNewFixed { array_type_index: ARGS, array_size: WIDTH }, LocalSet(6),
        LocalGet(12), I32Const(1), I32Eq, If(BlockType::Empty), I32Const(1), RefI31]); nil(c); c.push(LocalGet(6)); nil(c);
    c.extend([ArrayNewFixed { array_type_index: ARGS, array_size: 4 }, GlobalSet(STATE), Else,
        I32Const(COUNT), ArrayNewDefault(ARGS), LocalSet(10)]);
    array(c, 10); c.push(I32Const(0)); array(c, 5);
    c.extend([I32Const(0), I32Const(COUNT), ArrayCopy { array_type_index_dst: ARGS, array_type_index_src: ARGS }]);
    array(c, 10); c.extend([LocalGet(11), LocalGet(6), ArraySet(ARGS),
        I32Const(2), RefI31, LocalGet(4), LocalGet(10)]); get(c, 3, 3);
    c.extend([ArrayNewFixed { array_type_index: ARGS, array_size: 4 }, GlobalSet(STATE), End, End]);
}
