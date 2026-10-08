//! Original bounded portable GC streams; no canonical handles or host imports.
//! Private constructors/adapters must not expose mutable chunk storage to source.
//! Journal rows stay rooted before snapshot publication and until settlement.
use super::*;
use Instruction::*;
const OBJECT: u32 = 7;
pub(super) const GLOBAL_COUNT: u32 = 9;
const ROOT_LIMIT: i32 = 256;
const STATE: u32 = 0;
const READER: u32 = 1;
const WRITER: u32 = 2;
const PAIR: u32 = 3;
const CHUNK: u32 = 4;
const EOF: u32 = 5;
const OP: u32 = 6;
// State fields: immutable snapshot. Snapshot: buffer,count,writer terminal,
// reader closed,reason,pending read,pending write. Buffer is packed, not a cursor.
// Op: state,kind,demand,chunk,owner,future,phase,old,new,payload,settle,registry slot,cleanup flag.
// Phase: 0 prepared begin,1 waiting,2 prepared transfer,3 committed,4 retired.
// Settle: 1 Ready,2 Failed,3 Cancelled. Writer terminal: 0 open,1 close,2 fail.
fn nil(v: &mut Vec<Instruction<'static>>) {
    v.extend([I32Const(0), RefI31]);
}
fn num(v: &mut Vec<Instruction<'static>>, n: i32) {
    v.extend([I32Const(n), RefI31]);
}
fn arr(v: &mut Vec<Instruction<'static>>, local: u32) {
    v.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(ARGS))]);
}
fn field(v: &mut Vec<Instruction<'static>>, local: u32, slot: i32) {
    arr(v, local);
    v.extend([I32Const(slot), ArrayGet(ARGS)]);
}
fn int(v: &mut Vec<Instruction<'static>>, local: u32, slot: i32) {
    field(v, local, slot);
    v.extend([RefCastNonNull(HeapType::I31), I31GetU]);
}
fn set(v: &mut Vec<Instruction<'static>>, local: u32, slot: i32, value: &[Instruction<'static>]) {
    arr(v, local);
    v.push(I32Const(slot));
    v.extend_from_slice(value);
    v.push(ArraySet(ARGS));
}
fn fields(v: &mut Vec<Instruction<'static>>, local: u32) {
    v.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 1,
        },
    ]);
}
fn object(v: &mut Vec<Instruction<'static>>, base: u32, kind: u32, count: u32) {
    // Descriptor must already be below the field values on the stack.
    let _ = (base, kind);
    v.extend([ArrayNewFixed {
        array_type_index: ARGS,
        array_size: count,
    }]);
    nil(v);
    nil(v);
    v.push(StructNew(OBJECT));
}
fn bad(v: &mut Vec<Instruction<'static>>) {
    nil(v);
    v.push(Throw(0));
}
fn check(v: &mut Vec<Instruction<'static>>) {
    v.push(I32Eqz);
    v.push(If(BlockType::Empty));
    bad(v);
    v.push(End);
}
fn identity(v: &mut Vec<Instruction<'static>>, local: u32, base: u32, kind: u32) {
    v.extend([LocalGet(local), RefTestNonNull(HeapType::Concrete(OBJECT))]);
    check(v);
    v.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 0,
        },
        GlobalGet(base + kind),
        RefEq,
    ]);
    check(v);
}
fn snap(v: &mut Vec<Instruction<'static>>, state: u32) {
    field(v, state, 0);
}
/// Parent supplies the appended global base; existing ABI2 indices stay fixed.
pub(super) fn append_globals(globals: &mut GlobalSection, base: u32) {
    assert_eq!(globals.len(), base);
    for _ in 0..7 {
        globals.global(
            GlobalType {
                val_type: reference(DESCRIPTOR),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([
                I64Const(0),
                I32Const(0),
                ArrayNewDefault(ARGS),
                I32Const(0),
                ArrayNewDefault(ARGS),
                I32Const(0),
                RefI31,
                I32Const(0),
                RefI31,
                StructNew(DESCRIPTOR),
            ]),
        );
    }
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([I32Const(ROOT_LIMIT), ArrayNewDefault(ARGS)]),
    );
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            GlobalGet(base + EOF),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(OBJECT),
        ]),
    );
}
pub(super) fn intrinsics(b: &mut Builder, base: u32) {
    let roots = base + 7;
    let eof = base + 8;
    let pending = b.names["future-pending-new"];
    let status = b.names["future-status"];
    let owner = b.names["async-current-owner"];
    let cancelled = b.names["async-task-cancel-requested"];
    // Declare service before mutation entry points. Every state replacement
    // first completes rooted publication receipts, so newer snapshots cannot
    // erase the only evidence that an interrupted transfer already committed.
    service(b, base, status, cancelled);
    // Pair and endpoint capabilities share one state object. No clone operation.
    let mut v = vec![
        LocalGet(0),
        I32Const(1),
        I32GeU,
        LocalGet(0),
        I32Const(4096),
        I32LeU,
        I32And,
    ];
    check(&mut v);
    v.push(GlobalGet(base + STATE));
    v.extend([LocalGet(0), ArrayNewDefault(ARGS)]);
    for _ in 0..4 {
        num(&mut v, 0);
    } // count, writer terminal, reader closed, reason(nil)
    nil(&mut v);
    nil(&mut v);
    v.push(ArrayNewFixed {
        array_type_index: ARGS,
        array_size: 7,
    });
    object(&mut v, base, STATE, 1);
    v.push(LocalSet(1));
    v.push(GlobalGet(base + PAIR));
    for kind in [READER, WRITER] {
        v.extend([GlobalGet(base + kind), LocalGet(1)]);
        object(&mut v, base, kind, 1);
    }
    object(&mut v, base, PAIR, 2);
    b.function_with_locals(
        "stream-raw-pair-new",
        &[ValType::I32],
        &[VALUE],
        &[(1, VALUE)],
        &v,
    );
    for (name, slot) in [("stream-pair-reader", 0), ("stream-pair-writer", 1)] {
        let mut v = vec![];
        identity(&mut v, 0, base, PAIR);
        fields(&mut v, 0);
        v.extend([I32Const(slot), ArrayGet(ARGS)]);
        b.function(name, &[VALUE], &[VALUE], &v);
    }
    // Private mutable construction token, sealed once for immutable ownership.
    let mut v = vec![
        LocalGet(0),
        I32Const(1),
        I32GeU,
        LocalGet(0),
        I32Const(256),
        I32LeU,
        I32And,
    ];
    check(&mut v);
    v.extend([GlobalGet(base + CHUNK), LocalGet(0), ArrayNewDefault(ARGS)]);
    num(&mut v, 0);
    object(&mut v, base, CHUNK, 2);
    b.function("stream-raw-chunk-new", &[ValType::I32], &[VALUE], &v);
    let mut v = vec![];
    identity(&mut v, 0, base, CHUNK);
    fields(&mut v, 0);
    v.push(LocalSet(1));
    field(&mut v, 1, 0);
    v.extend([RefCastNonNull(HeapType::Concrete(ARGS)), ArrayLen]);
    b.function_with_locals(
        "stream-chunk-count",
        &[VALUE],
        &[ValType::I32],
        &[(1, VALUE)],
        &v,
    );
    let mut v = vec![];
    identity(&mut v, 0, base, CHUNK);
    fields(&mut v, 0);
    v.push(LocalSet(2));
    field(&mut v, 2, 0);
    v.extend([
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalGet(1),
        I32GtU,
    ]);
    check(&mut v);
    field(&mut v, 2, 0);
    v.push(RefCastNonNull(HeapType::Concrete(ARGS)));
    v.extend([LocalGet(1), ArrayGet(ARGS)]);
    b.function_with_locals(
        "stream-raw-chunk-nth",
        &[VALUE, ValType::I32],
        &[VALUE],
        &[(1, VALUE)],
        &v,
    );
    let mut v = vec![];
    identity(&mut v, 0, base, CHUNK);
    fields(&mut v, 0);
    v.push(LocalSet(3));
    int(&mut v, 3, 1);
    v.push(I32Eqz);
    check(&mut v);
    field(&mut v, 3, 0);
    v.extend([
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalGet(1),
        I32GtU,
    ]);
    check(&mut v);
    field(&mut v, 3, 0);
    v.push(RefCastNonNull(HeapType::Concrete(ARGS)));
    v.extend([LocalGet(1), LocalGet(2), ArraySet(ARGS)]);
    b.function_with_locals(
        "stream-raw-chunk-set",
        &[VALUE, ValType::I32, VALUE],
        &[],
        &[(1, VALUE)],
        &v,
    );
    let mut v = vec![];
    identity(&mut v, 0, base, CHUNK);
    fields(&mut v, 0);
    v.push(LocalSet(1));
    set(&mut v, 1, 1, &[I32Const(1), RefI31]);
    v.push(LocalGet(0));
    b.function_with_locals("stream-chunk-seal", &[VALUE], &[VALUE], &[(1, VALUE)], &v);
    b.function(
        "stream-eof-is",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), GlobalGet(eof), RefEq],
    );
    // Begin recovers journals before replacing snapshots. Service settles
    // storage/queues only; it never invokes source callbacks inline.
    begin(b, base, false, pending, owner);
    begin(b, base, true, pending, owner);
    let mut v = vec![];
    identity(&mut v, 0, base, OP);
    fields(&mut v, 0);
    v.extend([I32Const(5), ArrayGet(ARGS)]);
    b.function("stream-operation-future", &[VALUE], &[VALUE], &v);
    retire(b, base);
    terminal(b, base, false);
    terminal(b, base, true);
    let mut v = vec![
        I32Const(0),
        LocalSet(0),
        I32Const(0),
        LocalSet(1),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(0),
        I32Const(ROOT_LIMIT),
        I32GeU,
        BrIf(1),
        GlobalGet(roots),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(0),
        ArrayGet(ARGS),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(1),
        I32Const(1),
        I32Add,
        LocalSet(1),
        End,
        LocalGet(0),
        I32Const(1),
        I32Add,
        LocalSet(0),
        Br(0),
        End,
        End,
        LocalGet(1),
    ];
    b.function_with_locals(
        "stream-pending-count",
        &[],
        &[ValType::I32],
        &[(2, ValType::I32)],
        &v,
    );
    v.clear();
    boxed_exports(b, base);
}
fn copy_snapshot(v: &mut Vec<Instruction<'static>>, old: u32, new: u32) {
    for i in 0..7 {
        field(v, old, i);
    }
    v.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 7,
        },
        LocalSet(new),
    ]);
}
fn begin(b: &mut Builder, base: u32, write: bool, pending: u32, owner: u32) {
    let mut v = vec![Call(b.names["stream-raw-service"]), Drop];
    identity(&mut v, 0, base, if write { WRITER } else { READER });
    fields(&mut v, 0);
    v.extend([I32Const(0), ArrayGet(ARGS)]);
    v.push(LocalSet(2));
    fields(&mut v, 2);
    v.push(LocalSet(2));
    snap(&mut v, 2);
    v.push(LocalSet(3));
    // Enforce bounded demand/chunk and one outstanding operation per endpoint.
    if write {
        identity(&mut v, 1, base, CHUNK);
        fields(&mut v, 1);
        v.push(LocalSet(4));
        set(&mut v, 4, 1, &[I32Const(1), RefI31]);
        field(&mut v, 4, 0);
        v.extend([
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            LocalSet(8),
        ]);
    } else {
        v.extend([LocalGet(1), LocalSet(8)]);
    }
    v.extend([
        LocalGet(8),
        I32Const(1),
        I32GeU,
        LocalGet(8),
        I32Const(256),
        I32LeU,
        I32And,
    ]);
    field(&mut v, 3, 0);
    v.extend([
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalGet(8),
        I32GeU,
        I32And,
    ]);
    check(&mut v);
    field(&mut v, 3, if write { 6 } else { 5 });
    nil(&mut v);
    v.push(RefEq);
    check(&mut v);
    v.extend([Call(owner), LocalSet(4)]); // wrapper task owns retirement, never raw source completion
    v.extend([LocalGet(4), Call(b.names["future-is"])]);
    check(&mut v);
    // Cancellation does not poison operations created by dispatched finally.
    // Capture this distinction once; do not erase the owner's retained latch.
    v.extend([
        LocalGet(4),
        Call(b.names["async-task-cancel-requested"]),
        LocalGet(4),
        Call(b.names["async-owner-cancellation-pending"]),
        I32Eqz,
        I32And,
        LocalSet(10),
    ]);
    v.extend([
        Call(pending),
        LocalSet(5),
        I32Const(0),
        LocalSet(9),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(9),
        I32Const(ROOT_LIMIT),
        I32GeU,
        If(BlockType::Empty),
    ]);
    bad(&mut v);
    v.push(End);
    v.extend([
        GlobalGet(base + 7),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(9),
        ArrayGet(ARGS),
        RefIsNull,
        BrIf(1),
        LocalGet(9),
        I32Const(1),
        I32Add,
        LocalSet(9),
        Br(0),
        End,
        End,
    ]);
    copy_snapshot(&mut v, 3, 6);
    v.push(GlobalGet(base + OP));
    fields(&mut v, 0);
    v.extend([I32Const(0), ArrayGet(ARGS)]);
    num(&mut v, if write { 1 } else { 0 });
    v.extend([LocalGet(8), RefI31]);
    if write {
        v.push(LocalGet(1));
    } else {
        nil(&mut v);
    }
    v.extend([LocalGet(4), LocalGet(5)]);
    num(&mut v, 0);
    v.extend([LocalGet(3), LocalGet(6)]);
    nil(&mut v);
    num(&mut v, 0);
    v.extend([LocalGet(9), RefI31, LocalGet(10), RefI31]);
    object(&mut v, base, OP, 13);
    v.push(LocalSet(7));
    set(&mut v, 6, if write { 6 } else { 5 }, &[LocalGet(7)]);
    // Root before any pointer publication. A trap now is recovered by service.
    v.extend([
        GlobalGet(base + 7),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(9),
        LocalGet(7),
        ArraySet(ARGS),
    ]);
    set(&mut v, 2, 0, &[LocalGet(6)]);
    fields(&mut v, 7);
    v.push(LocalSet(6));
    set(&mut v, 6, 6, &[I32Const(1), RefI31]);
    v.push(LocalGet(7));
    b.function_with_locals(
        if write {
            "stream-begin-write"
        } else {
            "stream-raw-begin-read"
        },
        &[VALUE, if write { VALUE } else { ValType::I32 }],
        &[VALUE],
        &[(6, VALUE), (3, ValType::I32)],
        &v,
    );
}
// Prepare an immutable cancellation snapshot without accepting/consuming data.
fn withdraw(b: &mut Builder, base: u32) -> u32 {
    let mut v = vec![];
    fields(&mut v, 0);
    v.push(LocalSet(1));
    field(&mut v, 1, 0);

    v.push(LocalSet(2));
    fields(&mut v, 2);
    v.push(LocalSet(2));
    snap(&mut v, 2);
    v.push(LocalSet(3));
    copy_snapshot(&mut v, 3, 4);
    int(&mut v, 1, 1);
    v.push(If(BlockType::Empty));
    set(&mut v, 4, 6, &[I32Const(0), RefI31]);
    v.push(Else);
    set(&mut v, 4, 5, &[I32Const(0), RefI31]);
    v.push(End);
    set(&mut v, 1, 7, &[LocalGet(3)]);
    set(&mut v, 1, 8, &[LocalGet(4)]);
    set(&mut v, 1, 9, &[I32Const(0), RefI31]);
    set(&mut v, 1, 10, &[I32Const(3), RefI31]);
    set(&mut v, 1, 6, &[I32Const(2), RefI31]);
    b.function_with_locals("stream-withdraw-op", &[VALUE], &[], &[(4, VALUE)], &v)
}
fn prepare(b: &mut Builder, base: u32) -> u32 {
    // Params op0; values fields1,state2,snapshot3,next4,buffer5,newbuffer6,
    // payload7; integers kind8,n9,count10,index11,outcome12.
    let mut v = vec![];
    fields(&mut v, 0);
    v.push(LocalSet(1));
    field(&mut v, 1, 0);
    v.push(LocalSet(2));
    fields(&mut v, 2);
    v.push(LocalSet(2));
    snap(&mut v, 2);
    v.push(LocalSet(3));
    field(&mut v, 3, 0);
    v.push(LocalSet(5));
    int(&mut v, 1, 1);
    v.push(LocalSet(8));
    int(&mut v, 1, 2);
    v.push(LocalSet(9));
    int(&mut v, 3, 1);
    v.push(LocalSet(10));
    // Failure/reader-close takes priority over buffered data. Close rejects any
    // unaccepted write, while reads drain already accepted chunks before EOF.
    int(&mut v, 3, 3);
    int(&mut v, 3, 2);
    v.extend([I32Const(2), I32Eq, I32Or, If(BlockType::Empty)]);
    field(&mut v, 3, 4);
    v.push(LocalSet(7));
    v.extend([
        I32Const(2),
        LocalSet(12),
        Else,
        LocalGet(8),
        If(BlockType::Empty),
    ]);
    int(&mut v, 3, 2);
    v.push(If(BlockType::Empty));
    nil(&mut v);
    v.extend([LocalSet(7), I32Const(2), LocalSet(12), Else]);
    arr(&mut v, 5);
    v.extend([
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalGet(10),
        I32Sub,
        LocalGet(9),
        I32LtU,
        If(BlockType::Empty),
        Return,
        End,
    ]);
    v.extend([I32Const(1), LocalSet(12)]);
    nil(&mut v);
    v.extend([LocalSet(7), End, Else]);
    v.extend([LocalGet(10), I32Eqz, If(BlockType::Empty)]);
    int(&mut v, 3, 2);
    v.extend([
        I32Eqz,
        If(BlockType::Empty),
        Return,
        End,
        GlobalGet(base + 8),
        LocalSet(7),
        I32Const(0),
        LocalSet(9),
        I32Const(1),
        LocalSet(12),
        Else,
    ]);
    // Read consumes min(demand,available), constructing a private sealed chunk.
    v.extend([
        LocalGet(9),
        LocalGet(10),
        I32GtU,
        If(BlockType::Empty),
        LocalGet(10),
        LocalSet(9),
        End,
        LocalGet(9),
        ArrayNewDefault(ARGS),
        LocalSet(6),
        I32Const(0),
        LocalSet(11),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(11),
        LocalGet(9),
        I32GeU,
        BrIf(1),
    ]);
    arr(&mut v, 6);
    v.push(LocalGet(11));
    arr(&mut v, 5);
    v.extend([
        LocalGet(11),
        ArrayGet(ARGS),
        ArraySet(ARGS),
        LocalGet(11),
        I32Const(1),
        I32Add,
        LocalSet(11),
        Br(0),
        End,
        End,
        GlobalGet(base + CHUNK),
        LocalGet(6),
    ]);
    num(&mut v, 1);
    object(&mut v, base, CHUNK, 2);
    v.extend([LocalSet(7), I32Const(1), LocalSet(12), End, End, End]);
    copy_snapshot(&mut v, 3, 4);
    // Copy old buffer into a fresh bounded buffer, changing only accepted data.
    v.extend([LocalGet(12), I32Const(1), I32Eq, If(BlockType::Empty)]);
    arr(&mut v, 5);
    v.extend([
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        ArrayNewDefault(ARGS),
        LocalSet(6),
        I32Const(0),
        LocalSet(11),
        LocalGet(8),
        If(BlockType::Empty),
    ]);
    v.extend([
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(11),
        LocalGet(10),
        I32GeU,
        BrIf(1),
    ]);
    arr(&mut v, 6);
    v.push(LocalGet(11));
    arr(&mut v, 5);
    v.extend([
        LocalGet(11),
        ArrayGet(ARGS),
        ArraySet(ARGS),
        LocalGet(11),
        I32Const(1),
        I32Add,
        LocalSet(11),
        Br(0),
        End,
        End,
    ]);
    field(&mut v, 1, 3);
    v.push(LocalSet(5));
    fields(&mut v, 5);
    v.push(LocalSet(5));
    field(&mut v, 5, 0);
    v.extend([
        LocalSet(5),
        I32Const(0),
        LocalSet(11),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(11),
        LocalGet(9),
        I32GeU,
        BrIf(1),
    ]);
    arr(&mut v, 6);
    v.extend([LocalGet(10), LocalGet(11), I32Add]);
    arr(&mut v, 5);
    v.extend([
        LocalGet(11),
        ArrayGet(ARGS),
        ArraySet(ARGS),
        LocalGet(11),
        I32Const(1),
        I32Add,
        LocalSet(11),
        Br(0),
        End,
        End,
        LocalGet(10),
        LocalGet(9),
        I32Add,
        LocalSet(10),
        Else,
    ]);
    v.extend([
        LocalGet(10),
        LocalGet(9),
        I32Sub,
        LocalSet(10),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(11),
        LocalGet(10),
        I32GeU,
        BrIf(1),
    ]);
    arr(&mut v, 6);
    v.push(LocalGet(11));
    arr(&mut v, 5);
    v.extend([
        LocalGet(11),
        LocalGet(9),
        I32Add,
        ArrayGet(ARGS),
        ArraySet(ARGS),
        LocalGet(11),
        I32Const(1),
        I32Add,
        LocalSet(11),
        Br(0),
        End,
        End,
        End,
    ]);
    set(&mut v, 4, 0, &[LocalGet(6)]);
    set(&mut v, 4, 1, &[LocalGet(10), RefI31]);
    v.push(End);
    v.extend([LocalGet(8), If(BlockType::Empty)]);
    set(&mut v, 4, 6, &[I32Const(0), RefI31]);
    v.push(Else);
    set(&mut v, 4, 5, &[I32Const(0), RefI31]);
    v.push(End);
    set(&mut v, 1, 7, &[LocalGet(3)]);
    set(&mut v, 1, 8, &[LocalGet(4)]);
    set(&mut v, 1, 9, &[LocalGet(7)]);
    set(&mut v, 1, 10, &[LocalGet(12), RefI31]);
    set(&mut v, 1, 6, &[I32Const(2), RefI31]);
    b.function_with_locals(
        "stream-prepare-op",
        &[VALUE],
        &[],
        &[(7, VALUE), (5, ValType::I32)],
        &v,
    )
}
// Recover every already-published receipt before permitting another snapshot
// replacement. A single row-by-row service pass can otherwise erase a later
// row's receipt while advancing an earlier waiter on the same endpoint pair.
fn recover_receipts(b: &mut Builder, base: u32) -> u32 {
    let mut v = vec![
        I32Const(0),
        LocalSet(0),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(0),
        I32Const(ROOT_LIMIT),
        I32GeU,
        BrIf(1),
        GlobalGet(base + 7),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(0),
        ArrayGet(ARGS),
        LocalTee(1),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
    ];
    fields(&mut v, 1);
    v.push(LocalSet(2));
    int(&mut v, 2, 6);
    v.extend([
        LocalTee(4),
        I32Eqz,
        LocalGet(4),
        I32Const(2),
        I32Eq,
        I32Or,
        If(BlockType::Empty),
    ]);
    field(&mut v, 2, 0);
    v.push(LocalSet(3));
    fields(&mut v, 3);
    v.push(LocalSet(3));
    snap(&mut v, 3);
    field(&mut v, 2, 8);
    v.extend([
        RefEq,
        If(BlockType::Empty),
        LocalGet(4),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    set(&mut v, 2, 6, &[I32Const(1), RefI31]);
    v.push(Else);
    set(&mut v, 2, 6, &[I32Const(3), RefI31]);
    v.extend([
        End,
        End,
        End,
        End,
        LocalGet(0),
        I32Const(1),
        I32Add,
        LocalSet(0),
        Br(0),
        End,
        End,
    ]);
    b.function_with_locals(
        "stream-recover-receipts",
        &[],
        &[],
        &[(1, ValType::I32), (3, VALUE), (1, ValType::I32)],
        &v,
    )
}
fn service(b: &mut Builder, base: u32, status: u32, cancelled: u32) {
    let recover = recover_receipts(b, base);
    let withdrawal = withdraw(b, base);
    let preparation = prepare(b, base);
    // Commit publishes one snapshot pointer. Receipt recovery compares pointer
    // identity before checking cancellation; already accepted effects stay final.
    let mut v = vec![];
    fields(&mut v, 0);
    v.push(LocalSet(1));
    field(&mut v, 1, 0);
    v.push(LocalSet(2));
    fields(&mut v, 2);
    v.push(LocalSet(2));
    snap(&mut v, 2);
    field(&mut v, 1, 8);
    v.extend([RefEq, If(BlockType::Empty)]);
    int(&mut v, 1, 6);
    v.push(I32Eqz);
    v.push(If(BlockType::Empty));
    set(&mut v, 1, 6, &[I32Const(1), RefI31]);
    v.push(Else);
    set(&mut v, 1, 6, &[I32Const(3), RefI31]);
    v.extend([End, Return, End]);
    field(&mut v, 1, 4);
    v.push(LocalSet(3));
    v.extend([
        LocalGet(3),
        Call(status),
        I32Eqz,
        I32Eqz,
        LocalGet(3),
        Call(cancelled),
    ]);
    int(&mut v, 1, 12);
    v.extend([
        I32Eqz,
        I32And,
        I32Or,
        If(BlockType::Empty),
        LocalGet(0),
        Call(withdrawal),
        End,
    ]);
    // No publication to the prepared new snapshot occurred. A close/fail
    // can supersede the old snapshot between interrupted service attempts.
    snap(&mut v, 2);
    field(&mut v, 1, 7);
    v.extend([RefEq, I32Eqz, If(BlockType::Empty)]);
    int(&mut v, 1, 6);
    v.extend([I32Const(2), I32Eq]);
    check(&mut v);
    set(&mut v, 1, 6, &[I32Const(1), RefI31]);
    v.extend([LocalGet(0), Call(preparation)]);
    int(&mut v, 1, 6);
    v.extend([I32Const(2), I32Ne, If(BlockType::Empty), Return, End, End]);
    arr(&mut v, 2);
    v.push(I32Const(0));
    field(&mut v, 1, 8);
    v.push(ArraySet(ARGS));
    int(&mut v, 1, 6);
    v.extend([I32Eqz, If(BlockType::Empty)]);
    set(&mut v, 1, 6, &[I32Const(1), RefI31]);
    v.push(Else);
    set(&mut v, 1, 6, &[I32Const(3), RefI31]);
    v.push(End);
    let commit = b.function_with_locals("stream-commit-op", &[VALUE], &[], &[(3, VALUE)], &v);
    // A single bounded sweep. Waiting transfers are retried on later scheduler
    // boundaries; no spin loop, source callback, or per-endpoint waiter queue.
    let mut v = vec![
        Call(recover),
        I32Const(0),
        LocalSet(0),
        I32Const(0),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(0),
        I32Const(ROOT_LIMIT),
        I32GeU,
        BrIf(1),
        GlobalGet(base + 7),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(0),
        ArrayGet(ARGS),
        LocalTee(1),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
    ];
    fields(&mut v, 1);
    v.push(LocalSet(2));
    int(&mut v, 2, 6);
    v.extend([
        I32Const(0),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(1),
        Call(commit),
        End,
    ]);
    int(&mut v, 2, 6);
    v.extend([I32Const(1), I32Eq, If(BlockType::Empty)]);
    field(&mut v, 2, 4);
    v.push(LocalSet(3));
    v.extend([
        LocalGet(3),
        Call(status),
        I32Eqz,
        I32Eqz,
        LocalGet(3),
        Call(cancelled),
    ]);
    int(&mut v, 2, 12);
    v.extend([
        I32Eqz,
        I32And,
        I32Or,
        If(BlockType::Empty),
        LocalGet(1),
        Call(withdrawal),
        Else,
        LocalGet(1),
        Call(preparation),
        End,
        End,
    ]);
    int(&mut v, 2, 6);
    v.extend([
        I32Const(2),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(1),
        Call(commit),
        End,
    ]);
    int(&mut v, 2, 6);
    v.extend([I32Const(3), I32Eq, If(BlockType::Empty)]);
    int(&mut v, 2, 10);
    v.extend([I32Const(1), I32Eq, If(BlockType::Empty)]);
    field(&mut v, 2, 5);
    field(&mut v, 2, 9);
    v.extend([Call(b.names["future-resolve"]), Drop, Else]);
    int(&mut v, 2, 10);
    v.extend([I32Const(2), I32Eq, If(BlockType::Empty)]);
    field(&mut v, 2, 5);
    field(&mut v, 2, 9);
    v.extend([Call(b.names["future-reject"]), Drop, Else]);
    field(&mut v, 2, 5);
    v.extend([Call(b.names["future-cancel"]), Drop, End, End]);
    set(&mut v, 2, 6, &[I32Const(4), RefI31]);
    v.push(End);
    int(&mut v, 2, 6);
    v.extend([I32Const(4), I32Eq, If(BlockType::Empty)]);
    for slot in [0, 3, 4, 7, 8, 9] {
        set(&mut v, 2, slot, &[I32Const(0), RefI31]);
    }
    v.extend([
        GlobalGet(base + 7),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(0),
        RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }),
        ArraySet(ARGS),
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        End,
        End,
        LocalGet(0),
        I32Const(1),
        I32Add,
        LocalSet(0),
        Br(0),
        End,
        End,
        LocalGet(4),
    ]);
    b.function_with_locals(
        "stream-raw-service",
        &[],
        &[ValType::I32],
        &[(1, ValType::I32), (3, VALUE), (1, ValType::I32)],
        &v,
    );
}
fn retire(b: &mut Builder, base: u32) {
    let mut v = vec![];
    identity(&mut v, 0, base, OP);
    v.push(Call(b.names["stream-recover-receipts"]));
    fields(&mut v, 0);
    v.push(LocalSet(1));
    int(&mut v, 1, 6);
    v.extend([
        I32Const(4),
        I32Eq,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    // Recover a published transfer before deciding whether withdrawal is legal.
    int(&mut v, 1, 6);
    v.extend([I32Const(2), I32Eq, If(BlockType::Empty)]);
    field(&mut v, 1, 0);
    v.push(LocalSet(2));
    fields(&mut v, 2);
    v.push(LocalSet(2));
    snap(&mut v, 2);
    field(&mut v, 1, 8);
    v.extend([RefEq, If(BlockType::Empty)]);
    set(&mut v, 1, 6, &[I32Const(3), RefI31]);
    v.extend([End, End]);
    int(&mut v, 1, 6);
    v.extend([
        I32Const(3),
        I32LtU,
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["stream-withdraw-op"]),
        LocalGet(0),
        Call(b.names["stream-commit-op"]),
        End,
        Call(b.names["stream-raw-service"]),
        Drop,
        I32Const(1),
    ]);
    b.function_with_locals(
        "stream-raw-retire-op",
        &[VALUE],
        &[ValType::I32],
        &[(2, VALUE)],
        &v,
    );
}
fn terminal(b: &mut Builder, base: u32, fail: bool) {
    let mut v = vec![Call(b.names["stream-raw-service"]), Drop];
    if fail {
        identity(&mut v, 0, base, WRITER);
    } else {
        v.extend([LocalGet(0), RefTestNonNull(HeapType::Concrete(OBJECT))]);
        check(&mut v);
        v.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(OBJECT)),
            StructGet {
                struct_type_index: OBJECT,
                field_index: 0,
            },
            LocalTee(5),
            GlobalGet(base + READER),
            RefEq,
            LocalGet(5),
            GlobalGet(base + WRITER),
            RefEq,
            I32Or,
        ]);
        check(&mut v);
    }
    let offset = if fail { 2 } else { 1 };
    fields(&mut v, 0);
    v.extend([I32Const(0), ArrayGet(ARGS), LocalSet(offset)]);
    fields(&mut v, offset);
    v.push(LocalSet(offset));
    snap(&mut v, offset);
    v.push(LocalSet(offset + 1));
    if fail {
        int(&mut v, offset + 1, 2);
        v.extend([If(BlockType::Empty), I32Const(0), Return, End]);
    } else {
        v.extend([
            LocalGet(5),
            GlobalGet(base + READER),
            RefEq,
            If(BlockType::Result(ValType::I32)),
        ]);
        int(&mut v, offset + 1, 3);
        v.push(Else);
        int(&mut v, offset + 1, 2);
        v.extend([End, If(BlockType::Empty), I32Const(0), Return, End]);
    }
    copy_snapshot(&mut v, offset + 1, offset + 2);
    if fail {
        set(&mut v, offset + 2, 2, &[I32Const(2), RefI31]);
        set(&mut v, offset + 2, 4, &[LocalGet(1)]);
    } else {
        v.extend([
            LocalGet(5),
            GlobalGet(base + READER),
            RefEq,
            If(BlockType::Empty),
        ]);
        set(&mut v, offset + 2, 3, &[I32Const(1), RefI31]);
        v.push(Else);
        set(&mut v, offset + 2, 2, &[I32Const(1), RefI31]);
        v.push(End);
    }
    // Discarding endpoints allocate the replacement buffer before publication.
    if fail {
        field(&mut v, offset + 1, 0);
        v.extend([
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            ArrayNewDefault(ARGS),
            LocalSet(6),
        ]);
        set(&mut v, offset + 2, 0, &[LocalGet(6)]);
        set(&mut v, offset + 2, 1, &[I32Const(0), RefI31]);
    } else {
        v.extend([
            LocalGet(5),
            GlobalGet(base + READER),
            RefEq,
            If(BlockType::Empty),
        ]);
        field(&mut v, offset + 1, 0);
        v.extend([
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            ArrayNewDefault(ARGS),
            LocalSet(6),
        ]);
        set(&mut v, offset + 2, 0, &[LocalGet(6)]);
        set(&mut v, offset + 2, 1, &[I32Const(0), RefI31]);
        v.push(End);
    }
    set(&mut v, offset, 0, &[LocalGet(offset + 2)]);
    v.push(I32Const(1));
    // Explicit fixed locals make all helper indices valid for either arity.
    b.function_with_locals(
        if fail { "stream-fail" } else { "stream-close" },
        if fail { &[VALUE, VALUE] } else { &[VALUE] },
        &[ValType::I32],
        &[(if fail { 5 } else { 6 }, VALUE)],
        &v,
    );
}
fn boxed_exports(b: &mut Builder, base: u32) {
    let mut v = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(NUMBER))];
    check(&mut v);
    v.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalTee(1),
        F64Const(0.0.into()),
        F64Ge,
        LocalGet(1),
        F64Const(4096.0.into()),
        F64Le,
        I32And,
        LocalGet(1),
        LocalGet(1),
        F64Trunc,
        F64Eq,
        I32And,
    ]);
    check(&mut v);
    v.extend([LocalGet(1), I32TruncF64U]);
    let number = b.function_with_locals(
        "stream-checked-number",
        &[VALUE],
        &[ValType::I32],
        &[(1, ValType::F64)],
        &v,
    );
    for (name, raw) in [
        ("stream-pair-new", "stream-raw-pair-new"),
        ("stream-chunk-new", "stream-raw-chunk-new"),
    ] {
        b.function(
            name,
            &[VALUE],
            &[VALUE],
            &[LocalGet(0), Call(number), Call(b.names[raw])],
        );
    }
    let mut v = vec![];
    identity(&mut v, 0, base, READER);
    fields(&mut v, 0);
    v.extend([I32Const(0), ArrayGet(ARGS), LocalSet(2)]);
    fields(&mut v, 2);
    v.push(LocalSet(2));
    snap(&mut v, 2);
    v.push(LocalSet(2));
    field(&mut v, 2, 0);
    v.extend([
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalSet(3),
        LocalGet(1),
        Call(number),
        LocalSet(4),
        LocalGet(4),
        I32Const(1),
        I32GeU,
        LocalGet(4),
        I32Const(256),
        I32LeU,
        I32And,
        LocalGet(4),
        LocalGet(3),
        I32LeU,
        I32And,
    ]);
    check(&mut v);
    v.push(LocalGet(1));
    let read_limit = b.function_with_locals(
        "stream-read-limit",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE), (2, ValType::I32)],
        &v,
    );
    b.function(
        "stream-begin-read",
        &[VALUE, VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            LocalGet(0),
            LocalGet(1),
            Call(read_limit),
            Call(number),
            Call(b.names["stream-raw-begin-read"]),
        ],
    );
    let mut v = vec![];
    identity(&mut v, 0, base, WRITER);
    fields(&mut v, 0);
    v.extend([I32Const(0), ArrayGet(ARGS), LocalSet(2)]);
    fields(&mut v, 2);
    v.push(LocalSet(2));
    snap(&mut v, 2);
    v.push(LocalSet(2));
    field(&mut v, 2, 0);
    v.extend([RefCastNonNull(HeapType::Concrete(ARGS)), ArrayLen]);
    v.extend([
        LocalGet(1),
        Call(number),
        LocalTee(3),
        I32GeU,
        LocalGet(3),
        I32Const(1),
        I32GeU,
        I32And,
        LocalGet(3),
        I32Const(256),
        I32LeU,
        I32And,
    ]);
    check(&mut v);
    v.push(LocalGet(1));
    b.function_with_locals(
        "stream-write-limit",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32)],
        &v,
    );
    b.function(
        "stream-chunk-nth",
        &[VALUE, VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            LocalGet(1),
            Call(number),
            Call(b.names["stream-raw-chunk-nth"]),
        ],
    );
    let mut v = vec![
        LocalGet(0),
        LocalGet(1),
        Call(number),
        LocalGet(2),
        Call(b.names["stream-raw-chunk-set"]),
    ];
    nil(&mut v);
    b.function("stream-chunk-set", &[VALUE, VALUE, VALUE], &[VALUE], &v);
    b.function(
        "stream-operation-retire",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), Call(b.names["stream-raw-retire-op"])],
    );
    b.function(
        "stream-service",
        &[],
        &[],
        &[Call(b.names["stream-raw-service"]), Drop],
    );
    let mut v = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ];
    for (kind, result) in [(READER, 1), (WRITER, 2), (EOF, 3)] {
        v.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(OBJECT)),
            StructGet {
                struct_type_index: OBJECT,
                field_index: 0,
            },
            GlobalGet(base + kind),
            RefEq,
            If(BlockType::Empty),
            I32Const(result),
            Return,
            End,
        ]);
    }
    v.push(I32Const(0));
    b.function("stream-value-kind", &[VALUE], &[ValType::I32], &v);
    // Pending operation futures are source-inaccessible completion capabilities.
    // Guards must refuse host/source settlement while their journal is rooted.
    let mut v = vec![
        I32Const(0),
        LocalSet(1),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
        I32Const(ROOT_LIMIT),
        I32GeU,
        BrIf(1),
        GlobalGet(base + 7),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(1),
        ArrayGet(ARGS),
        LocalTee(2),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
    ];
    fields(&mut v, 2);
    v.push(LocalSet(2));
    field(&mut v, 2, 5);
    v.extend([
        LocalGet(0),
        RefEq,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
        End,
        LocalGet(1),
        I32Const(1),
        I32Add,
        LocalSet(1),
        Br(0),
        End,
        End,
        I32Const(0),
    ]);
    b.function_with_locals(
        "stream-operation-owned",
        &[VALUE],
        &[ValType::I32],
        &[(1, ValType::I32), (1, VALUE)],
        &v,
    );
}
