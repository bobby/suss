//! Original future storage and bounded cooperative registration/queue primitives.
//! Task cleanup, suspension lowering and host callback integration remain separate.
//! Futures are ordinary ABI objects, identified by a private rooted descriptor.
use super::*;

pub(super) const DESCRIPTOR_GLOBAL: u32 = identity_hash::COUNTER + 1;
const OBJECT: u32 = 7;
const PENDING: i32 = 0;
const READY: i32 = 1;
const FAILED: i32 = 2;
const CANCELLED: i32 = 3;
const REGISTRATION_GLOBAL: u32 = DESCRIPTOR_GLOBAL + 1;
const SCHEDULER_GLOBAL: u32 = REGISTRATION_GLOBAL + 1;
const CONTINUATION_GLOBAL: u32 = SCHEDULER_GLOBAL + 1;
const TRAP_DESCRIPTOR_GLOBAL: u32 = CONTINUATION_GLOBAL + 1;
const TRAP_VALUE_GLOBAL: u32 = TRAP_DESCRIPTOR_GLOBAL + 1;
const DEPENDENCY_CANCEL_GLOBAL: u32 = TRAP_VALUE_GLOBAL + 1;
const INVOCATION_GLOBAL: u32 = DEPENDENCY_CANCEL_GLOBAL + 1;
/// First global after all private async roots; streams append from this boundary.
pub(super) const STREAM_GLOBAL_BASE: u32 = INVOCATION_GLOBAL + 1;

fn fields(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 1,
        },
    ]);
}
fn slot(body: &mut Vec<Instruction<'static>>, index: i32) {
    use Instruction::*;
    fields(body);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(index),
        ArrayGet(ARGS),
    ]);
}
fn error(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    let text: Vec<_> = "Invalid future operation".encode_utf16().collect();
    body.push(GlobalGet(nominal::ERROR_GLOBAL));
    body.extend(text.iter().map(|unit| I32Const(*unit as i32)));
    body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: text.len() as u32,
        },
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        StructNew(8),
        Throw(0),
    ]);
}
fn guard(body: &mut Vec<Instruction<'static>>, predicate: u32) {
    use Instruction::*;
    body.extend([LocalGet(0), Call(predicate), I32Eqz, If(BlockType::Empty)]);
    error(body);
    body.push(End);
}

pub(super) fn intrinsics(b: &mut Builder) {
    use Instruction::*;
    // Predicate validates identity before looking at the mutable storage shape.
    // It never treats a structural match or matching descriptor number as identity.
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 0,
        },
        GlobalGet(DESCRIPTOR_GLOBAL),
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ];
    fields(&mut body);
    body.extend([
        ArrayLen,
        I32Const(1),
        I32Ne,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    fields(&mut body);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        LocalTee(1),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(2),
        I32Ne,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    slot(&mut body, 0);
    body.extend([
        LocalTee(1),
        RefTestNonNull(HeapType::I31),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(1),
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalTee(2),
        I32Const(CANCELLED),
        I32GtU,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(2),
        I32Const(PENDING),
        I32Eq,
        LocalGet(2),
        I32Const(CANCELLED),
        I32Eq,
        I32Or,
        If(BlockType::Empty),
    ]);
    slot(&mut body, 1);
    body.extend([I32Const(0), RefI31, RefEq, Return, End, I32Const(1)]);
    let predicate = b.function_with_locals(
        "future-is",
        &[VALUE],
        &[ValType::I32],
        &[(1, VALUE), (1, ValType::I32)],
        &body,
    );

    b.function(
        "future-pending-new",
        &[],
        &[VALUE],
        &[
            GlobalGet(DESCRIPTOR_GLOBAL),
            I32Const(PENDING),
            RefI31,
            I32Const(0),
            RefI31,
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 2,
            },
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 1,
            },
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(OBJECT),
        ],
    );
    let mut body = vec![];
    guard(&mut body, predicate);
    slot(&mut body, 0);
    body.extend([RefCastNonNull(HeapType::I31), I31GetU]);
    let status = b.function("future-status", &[VALUE], &[ValType::I32], &body);
    continuation_storage(b, predicate);
    let (refresh, enqueue) = scheduler_queue(b, predicate, status);

    // A result exists only in Ready/Failed; nil is a valid payload in either.
    let mut body = vec![
        LocalGet(0),
        Call(status),
        LocalTee(1),
        I32Const(READY),
        I32Eq,
        LocalGet(1),
        I32Const(FAILED),
        I32Eq,
        I32Or,
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    slot(&mut body, 1);
    b.function_with_locals(
        "future-result",
        &[VALUE],
        &[VALUE],
        &[(1, ValType::I32)],
        &body,
    );

    // Cooperative first-terminal-wins: false means an already terminal future.
    // Construct the complete [state,payload] outcome before a single outer-slot
    // publication. Fuel/GC traps before or after that write leave valid Pending
    // or terminal storage; there is no partially published payload/state pair.
    for (name, terminal, payload) in [
        ("future-resolve", READY, true),
        ("future-reject", FAILED, true),
        ("future-cancel", CANCELLED, false),
    ] {
        let mut body = vec![
            LocalGet(0),
            Call(status),
            I32Const(PENDING),
            I32Ne,
            If(BlockType::Empty),
            I32Const(0),
            Return,
            End,
        ];
        fields(&mut body);
        body.extend([I32Const(0), I32Const(terminal), RefI31]);
        if payload {
            body.push(LocalGet(1));
        } else {
            body.extend([I32Const(0), RefI31]);
        }
        body.extend([
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 2,
            },
            ArraySet(ARGS),
            Call(refresh),
            Drop,
            I32Const(1),
        ]);
        let params = if payload {
            vec![VALUE, VALUE]
        } else {
            vec![VALUE]
        };
        b.function(name, &params, &[ValType::I32], &body);
    }
    task_exports(b);
    source_api_cancellation(b);
    // Streams depend only on the future/task primitives above. Interleave them
    // before scheduler emission so service calls use actual defined indices.
    super::streams::intrinsics(b, STREAM_GLOBAL_BASE);
    scheduler_runner(b, refresh, enqueue);
    scheduler_reset_exports(b);
    invocation_exports(b);
}

pub(super) fn append_descriptor(globals: &mut GlobalSection) {
    use Instruction::*;
    // Check the derived append index against the actual section: later private
    // roots cannot silently redirect future operations at an unrelated global.
    assert_eq!(globals.len(), DESCRIPTOR_GLOBAL);
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(1),
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
    assert_eq!(globals.len(), REGISTRATION_GLOBAL);
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(5),
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
    assert_eq!(globals.len(), SCHEDULER_GLOBAL);
    // Immutable snapshot: registrations, FIFO ready tokens, next generation,
    // active invocation. One GlobalSet commits each queue/registration update.
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(1),
            RefI31,
            I32Const(0),
            RefI31,
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 4,
            },
        ]),
    );
    assert_eq!(globals.len(), CONTINUATION_GLOBAL);
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(1),
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
    assert_eq!(globals.len(), TRAP_DESCRIPTOR_GLOBAL);
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
    assert_eq!(globals.len(), TRAP_VALUE_GLOBAL);
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            GlobalGet(TRAP_DESCRIPTOR_GLOBAL),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(OBJECT),
        ]),
    );
    assert_eq!(globals.len(), DEPENDENCY_CANCEL_GLOBAL);
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            GlobalGet(TRAP_DESCRIPTOR_GLOBAL),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(OBJECT),
        ]),
    );
    assert_eq!(globals.len(), INVOCATION_GLOBAL);
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    );
}

fn continuation_storage(b: &mut Builder, future_predicate: u32) {
    use Instruction::*;
    let copy = b.function_with_locals(
        "async-snapshot-copy",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &[
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            ArrayNewDefault(ARGS),
            LocalSet(1),
            LocalGet(1),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            ArrayCopy {
                array_type_index_dst: ARGS,
                array_type_index_src: ARGS,
            },
            LocalGet(1),
        ],
    );
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    arr(&mut body, 0);
    body.extend([ArrayLen, I32Const(8), I32Ne, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    for index in [0, 5, 6] {
        item(&mut body, 0, index);
        body.extend([RefTestNonNull(HeapType::I31), I32Eqz, If(BlockType::Empty)]);
        error(&mut body);
        body.push(End);
    }
    for index in [1, 2] {
        item(&mut body, 0, index);
        body.extend([
            RefTestNonNull(HeapType::Concrete(ARGS)),
            I32Eqz,
            If(BlockType::Empty),
        ]);
        error(&mut body);
        body.push(End);
    }
    item(&mut body, 0, 3);
    body.push(Call(b.names["dynamic-frame-check"]));
    item(&mut body, 0, 4);
    body.extend([Call(future_predicate), I32Eqz, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    item(&mut body, 0, 6);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(3),
        I32GtU,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    let check = b.function("async-continuation-state-check", &[VALUE], &[], &body);
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 0,
        },
        GlobalGet(CONTINUATION_GLOBAL),
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 1,
        },
        ArrayLen,
        I32Const(1),
        I32Ne,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    registration_item(&mut body, 0, 0);
    body.extend([LocalTee(1), Call(check), LocalGet(1)]);
    let read = b.function_with_locals(
        "async-continuation-read",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    b.function(
        "async-continuation-new",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            Call(check),
            GlobalGet(CONTINUATION_GLOBAL),
            LocalGet(0),
            Call(copy),
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 1,
            },
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(OBJECT),
        ],
    );
    // Expose a shallow copy, not the owner-held snapshot array. Live slots and
    // unwind graphs retain identity; the compiler owns their update discipline.
    b.function(
        "async-continuation-snapshot",
        &[VALUE],
        &[VALUE],
        &[LocalGet(0), Call(read), Call(copy)],
    );
    let mut body = vec![
        LocalGet(0),
        Call(read),
        LocalSet(2),
        LocalGet(1),
        Call(check),
    ];
    item(&mut body, 2, 4);
    item(&mut body, 1, 4);
    body.extend([RefEq, I32Eqz, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    item(&mut body, 1, 5);
    body.extend([RefCastNonNull(HeapType::I31), I31GetU]);
    item(&mut body, 2, 5);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32LtU,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(1),
        Call(copy),
        LocalSet(2),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 1,
        },
        I32Const(0),
        LocalGet(2),
        ArraySet(ARGS),
        LocalGet(0),
    ]);
    b.function_with_locals(
        "async-continuation-commit",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    // Compiler unwind nodes are [parent,phase,...] (9 slots). A cancellation
    // request never aborts an already running/suspended finally region.
    let mut body = vec![
        LocalGet(0),
        Call(b.names["async-continuation-read"]),
        LocalSet(1),
    ];
    item(&mut body, 1, 2);
    body.extend([LocalSet(1), Block(BlockType::Empty), Loop(BlockType::Empty)]);
    arr(&mut body, 1);
    body.extend([
        ArrayLen,
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    arr(&mut body, 1);
    body.extend([ArrayLen, I32Const(9), I32Ne, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    item(&mut body, 1, 1);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(2),
        I32Eq,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
    ]);
    item(&mut body, 1, 0);
    body.extend([LocalSet(1), Br(0), End, End, I32Const(0)]);
    b.function_with_locals(
        "async-continuation-cleanup-active",
        &[VALUE],
        &[ValType::I32],
        &[(1, VALUE)],
        &body,
    );
}

fn producer(body: &mut Vec<Instruction<'static>>, token: u32, read: u32) {
    registration_item(body, token, 0);
    body.extend([
        Instruction::Call(read),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
        Instruction::I32Const(4),
        Instruction::ArrayGet(ARGS),
    ]);
}

fn arr(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
fn item(body: &mut Vec<Instruction<'static>>, local: u32, index: i32) {
    arr(body, local);
    body.extend([Instruction::I32Const(index), Instruction::ArrayGet(ARGS)]);
}
fn registration_item(body: &mut Vec<Instruction<'static>>, local: u32, index: i32) {
    use Instruction::*;
    body.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 1,
        },
        I32Const(index),
        ArrayGet(ARGS),
    ]);
}
fn snapshot(body: &mut Vec<Instruction<'static>>, old: u32, rows: u32, queue: u32) {
    use Instruction::*;
    body.extend([LocalGet(rows), LocalGet(queue)]);
    item(body, old, 2);
    item(body, old, 3);
    body.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 4,
        },
        GlobalSet(SCHEDULER_GLOBAL),
    ]);
}

fn scheduler_queue(b: &mut Builder, predicate: u32, status: u32) -> (u32, u32) {
    use Instruction::*;
    // Immutable array append. Neither source array is modified before publication.
    let append = b.function_with_locals(
        "async-array-append",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32)],
        &[
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            LocalTee(3),
            I32Const(1),
            I32Add,
            ArrayNewDefault(ARGS),
            LocalSet(2),
            LocalGet(2),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            LocalGet(3),
            ArrayCopy {
                array_type_index_dst: ARGS,
                array_type_index_src: ARGS,
            },
            LocalGet(2),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            LocalGet(3),
            LocalGet(1),
            ArraySet(ARGS),
            LocalGet(2),
        ],
    );
    invocation_storage(b);
    cancellation_exports(b);

    // Row lookup: rows contain immutable [token,phase] records.
    let find = b.function_with_locals(
        "async-registration-find",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, ValType::I32)],
        &[
            I32Const(0),
            LocalSet(2),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(2),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            I32GeU,
            BrIf(1),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            LocalGet(2),
            ArrayGet(ARGS),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            ArrayGet(ARGS),
            LocalGet(1),
            RefEq,
            If(BlockType::Empty),
            LocalGet(2),
            Return,
            End,
            LocalGet(2),
            I32Const(1),
            I32Add,
            LocalSet(2),
            Br(0),
            End,
            End,
            I32Const(-1),
        ],
    );
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 0,
        },
        GlobalGet(REGISTRATION_GLOBAL),
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 1,
        },
        ArrayLen,
        I32Const(6),
        I32Ne,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    registration_item(&mut body, 0, 4);
    body.extend([RefTestNonNull(HeapType::I31), I32Eqz, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    registration_item(&mut body, 0, 4);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(3),
        I32GtU,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    registration_item(&mut body, 0, 3);
    body.extend([RefTestNonNull(HeapType::I31), I32Eqz, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    registration_item(&mut body, 0, 3);
    body.extend([RefCastNonNull(HeapType::I31), I31GetU]);
    let generation = b.function(
        "async-registration-generation",
        &[VALUE],
        &[ValType::I32],
        &body,
    );

    // Enqueue is one snapshot publication: phase Pending=0 -> Queued=1 and
    // FIFO append occur together, so interrupted/duplicate events cannot lose
    // a claimed registration or append it twice.
    // params token,generation; locals old,rows,queue,row,newrows,index.
    // A deferred request wakes only after the active turn and existing cleanup.
    let mut body = vec![GlobalGet(SCHEDULER_GLOBAL), LocalSet(1)];
    registration_item(&mut body, 0, 0);
    body.push(LocalSet(2));
    producer(&mut body, 0, b.names["async-continuation-read"]);
    body.extend([
        LocalTee(3),
        Call(status),
        I32Eqz,
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    item(&mut body, 1, 3);
    body.extend([
        LocalTee(4),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    item(&mut body, 4, 0);
    body.extend([
        LocalGet(2),
        RefEq,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        End,
        LocalGet(2),
        Call(b.names["async-continuation-cleanup-active"]),
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    item(&mut body, 1, 0);
    body.extend([
        LocalTee(4),
        LocalGet(0),
        Call(find),
        LocalTee(6),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    arr(&mut body, 4);
    body.extend([
        LocalGet(6),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Eqz,
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(4),
        LocalGet(3),
        I32Const(1),
        LocalGet(0),
        Call(b.names["async-owner-filter"]),
        LocalSet(4),
    ]);
    item(&mut body, 1, 1);
    body.extend([
        LocalGet(3),
        I32Const(0),
        LocalGet(0),
        Call(b.names["async-owner-filter"]),
        LocalSet(5),
        LocalGet(4),
        LocalGet(0),
        Call(find),
        LocalSet(6),
    ]);
    arr(&mut body, 4);
    body.extend([
        LocalGet(6),
        LocalGet(0),
        I32Const(1),
        RefI31,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        ArraySet(ARGS),
        LocalGet(5),
        LocalGet(0),
        Call(append),
        LocalSet(5),
    ]);
    snapshot(&mut body, 1, 4, 5);
    body.push(I32Const(1));
    let cancellation_enqueue = b.function_with_locals(
        "async-cancellation-enqueue",
        &[VALUE],
        &[ValType::I32],
        &[(5, VALUE), (1, ValType::I32)],
        &body,
    );

    let mut body = vec![
        LocalGet(0),
        Call(generation),
        LocalGet(1),
        I32Ne,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ];
    registration_item(&mut body, 0, 4);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(2),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(0),
        Call(cancellation_enqueue),
        Return,
        End,
        GlobalGet(SCHEDULER_GLOBAL),
        LocalSet(2),
    ]);
    registration_item(&mut body, 0, 0);
    body.extend([
        Call(b.names["async-continuation-read"]),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(5),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalGet(1),
        I32Ne,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    producer(&mut body, 0, b.names["async-continuation-read"]);
    body.extend([
        Call(status),
        I32Eqz,
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    item(&mut body, 2, 0);
    body.push(LocalSet(3));
    item(&mut body, 2, 1);
    body.push(LocalSet(4));
    body.extend([
        LocalGet(3),
        LocalGet(0),
        Call(find),
        LocalTee(7),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    arr(&mut body, 3);
    body.extend([LocalGet(7), ArrayGet(ARGS), LocalSet(5)]);
    item(&mut body, 5, 1);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Eqz,
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    registration_item(&mut body, 0, 4);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(3),
        I32Ne,
        If(BlockType::Empty),
    ]);
    registration_item(&mut body, 0, 1);
    body.extend([
        Call(status),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    body.push(End);
    arr(&mut body, 3);
    body.extend([ArrayLen, ArrayNewDefault(ARGS), LocalSet(6)]);
    arr(&mut body, 6);
    body.push(I32Const(0));
    arr(&mut body, 3);
    body.push(I32Const(0));
    arr(&mut body, 3);
    body.extend([
        ArrayLen,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
    ]);
    arr(&mut body, 6);
    body.extend([
        LocalGet(7),
        LocalGet(0),
        I32Const(1),
        RefI31,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        ArraySet(ARGS),
    ]);
    body.extend([LocalGet(4), LocalGet(0), Call(append), LocalSet(4)]);
    snapshot(&mut body, 2, 6, 4);
    body.push(I32Const(1));
    let enqueue = b.function_with_locals(
        "async-registration-enqueue",
        &[VALUE, ValType::I32],
        &[ValType::I32],
        &[(5, VALUE), (1, ValType::I32)],
        &body,
    );

    // Terminal producers retire all their registrations, not just the latest
    // token. Active invocation roots live in the snapshot's separate fourth slot.
    let mut body = vec![GlobalGet(SCHEDULER_GLOBAL), LocalSet(0)];
    item(&mut body, 0, 0);
    body.push(LocalSet(1));
    item(&mut body, 0, 1);
    body.push(LocalSet(2));
    body.extend([
        I32Const(0),
        ArrayNewDefault(ARGS),
        LocalSet(3),
        I32Const(0),
        ArrayNewDefault(ARGS),
        LocalSet(4),
        I32Const(0),
        LocalSet(8),
        I32Const(0),
        LocalSet(7),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(7),
    ]);
    arr(&mut body, 1);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 1);
    body.extend([LocalGet(7), ArrayGet(ARGS), LocalSet(6)]);
    item(&mut body, 6, 0);
    body.push(LocalSet(5));
    producer(&mut body, 5, b.names["async-continuation-read"]);
    body.extend([
        Call(status),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(3),
        LocalGet(6),
        Call(append),
        LocalSet(3),
        Else,
        LocalGet(8),
        I32Const(1),
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
        I32Const(0),
        LocalSet(7),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(7),
    ]);
    arr(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 2);
    body.extend([
        LocalGet(7),
        ArrayGet(ARGS),
        LocalSet(5),
        LocalGet(3),
        LocalGet(5),
        Call(find),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
        LocalGet(4),
        LocalGet(5),
        Call(append),
        LocalSet(4),
        End,
        LocalGet(7),
        I32Const(1),
        I32Add,
        LocalSet(7),
        Br(0),
        End,
        End,
    ]);
    snapshot(&mut body, 0, 3, 4);
    body.push(LocalGet(8));
    let prune = b.function_with_locals(
        "async-scheduler-prune",
        &[],
        &[ValType::I32],
        &[(7, VALUE), (2, ValType::I32)],
        &body,
    );

    // Refresh also recovers completion publication interrupted before wake-up.
    // Scan a fixed immutable row snapshot; enqueue reads the latest snapshot.
    let mut body = vec![Call(prune), Drop, GlobalGet(SCHEDULER_GLOBAL), LocalSet(0)];
    item(&mut body, 0, 0);
    body.push(LocalSet(0));
    body.extend([
        I32Const(0),
        LocalSet(2),
        I32Const(0),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
    ]);
    body.push(LocalGet(2));
    arr(&mut body, 0);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 0);
    body.extend([
        LocalGet(2),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(1),
        LocalGet(3),
        LocalGet(1),
        LocalGet(1),
        Call(generation),
        Call(enqueue),
        I32Add,
        LocalSet(3),
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        Br(0),
        End,
        End,
        LocalGet(3),
    ]);
    let refresh = b.function_with_locals(
        "async-scheduler-refresh",
        &[],
        &[ValType::I32],
        &[(2, VALUE), (2, ValType::I32)],
        &body,
    );

    let read = b.names["async-continuation-read"];
    let mut body = vec![LocalGet(0), Call(read), LocalSet(6)];
    body.extend([
        LocalGet(3),
        I32Const(3),
        I32GtU,
        LocalGet(3),
        I32Const(2),
        I32Eq,
        I32Or,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    item(&mut body, 6, 4);
    body.extend([Call(status), I32Eqz, I32Eqz, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    body.extend([LocalGet(1), Call(predicate), I32Eqz, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    item(&mut body, 6, 5);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalSet(7),
        GlobalGet(SCHEDULER_GLOBAL),
        LocalSet(4),
    ]);
    item(&mut body, 4, 0);
    body.push(LocalSet(6));
    // A continuation generation has at most one registered resume, including
    // consumed rows. Lowering increments generation before another suspension.
    body.extend([
        I32Const(0),
        LocalSet(8),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(8),
    ]);
    arr(&mut body, 6);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 6);
    body.extend([
        LocalGet(8),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(9),
    ]);
    registration_item(&mut body, 9, 0);
    body.extend([LocalGet(0), RefEq]);
    registration_item(&mut body, 9, 3);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalGet(7),
        I32Eq,
        I32And,
    ]);
    registration_item(&mut body, 9, 4);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(2),
        I32Ne,
        I32And,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.extend([
        End,
        LocalGet(8),
        I32Const(1),
        I32Add,
        LocalSet(8),
        Br(0),
        End,
        End,
    ]);
    body.extend([
        LocalGet(6),
        LocalGet(0),
        LocalGet(7),
        Call(b.names["async-registration-compact"]),
        LocalSet(6),
    ]);
    body.extend([
        GlobalGet(REGISTRATION_GLOBAL),
        LocalGet(0),
        LocalGet(1),
        LocalGet(2),
        LocalGet(7),
        RefI31,
        LocalGet(3),
        RefI31,
        LocalGet(0),
        Call(b.names["async-continuation-invocation"]),
        F64ConvertI32U,
        StructNew(NUMBER),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 6,
        },
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        StructNew(OBJECT),
        LocalSet(5),
    ]);
    body.extend([
        LocalGet(6),
        LocalGet(5),
        I32Const(0),
        RefI31,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        Call(append),
        LocalSet(6),
    ]);
    item(&mut body, 4, 1);
    body.push(LocalSet(9));
    snapshot(&mut body, 4, 6, 9);
    body.extend([LocalGet(5), LocalGet(7), Call(enqueue), Drop, LocalGet(5)]);
    let create = b.function_with_locals(
        "async-registration-create",
        &[VALUE, VALUE, VALUE, ValType::I32],
        &[VALUE],
        &[(3, VALUE), (2, ValType::I32), (1, VALUE)],
        &body,
    );
    b.function(
        "async-registration-new",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            LocalGet(1),
            LocalGet(2),
            I32Const(0),
            Call(create),
        ],
    );
    (refresh, enqueue)
}

// A cancellation request is a rooted registration, not a terminal future write.
// Its flag 2 survives consumption as a latch through suspending cleanup.
fn cancellation_exports(b: &mut Builder) {
    use Instruction::*;
    let append = b.names["async-array-append"];
    let read = b.names["async-continuation-read"];
    let status = b.names["future-status"];
    b.function(
        "async-cancelled-dependency-is",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), GlobalGet(DEPENDENCY_CANCEL_GLOBAL), RefEq],
    );

    // Filter either row or queue arrays without mutating a published snapshot.
    // Keep-token is used when dispatching cancellation to retain its latch.
    // Parameters: array, owner, rows?, keep-token. Locals: out, entry, token, i.
    let mut body = vec![
        I32Const(0),
        ArrayNewDefault(ARGS),
        LocalSet(4),
        I32Const(0),
        LocalSet(7),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(7),
    ];
    arr(&mut body, 0);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 0);
    body.extend([
        LocalGet(7),
        ArrayGet(ARGS),
        LocalSet(5),
        LocalGet(2),
        If(BlockType::Result(VALUE)),
    ]);
    item(&mut body, 5, 0);
    body.extend([
        Else,
        LocalGet(5),
        End,
        LocalSet(6),
        LocalGet(6),
        LocalGet(3),
        RefEq,
    ]);
    producer(&mut body, 6, read);
    body.extend([
        LocalGet(1),
        RefEq,
        I32Eqz,
        I32Or,
        If(BlockType::Empty),
        LocalGet(4),
        LocalGet(5),
        Call(append),
        LocalSet(4),
        End,
        LocalGet(7),
        I32Const(1),
        I32Add,
        LocalSet(7),
        Br(0),
        End,
        End,
        LocalGet(4),
    ]);
    let filter = b.function_with_locals(
        "async-owner-filter",
        &[VALUE, VALUE, ValType::I32, VALUE],
        &[VALUE],
        &[(3, VALUE), (1, ValType::I32)],
        &body,
    );

    // A newer generation makes consumed ordinary registrations obsolete. Keep
    // the current generation's tombstone (duplicate protection), cancellation
    // latches, and all pending/queued rows. The active record roots an executing
    // callback independently of these rows.
    let mut body = vec![
        I32Const(0),
        ArrayNewDefault(ARGS),
        LocalSet(3),
        I32Const(0),
        LocalSet(6),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(6),
    ];
    arr(&mut body, 0);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 0);
    body.extend([LocalGet(6), ArrayGet(ARGS), LocalSet(4)]);
    item(&mut body, 4, 0);
    body.push(LocalSet(5));
    item(&mut body, 4, 1);
    body.extend([RefCastNonNull(HeapType::I31), I31GetU, I32Const(2), I32Eq]);
    registration_item(&mut body, 5, 0);
    body.extend([LocalGet(1), RefEq, I32And]);
    registration_item(&mut body, 5, 3);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalGet(2),
        I32LtU,
        I32And,
    ]);
    registration_item(&mut body, 5, 4);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(2),
        I32Ne,
        I32And,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(3),
        LocalGet(4),
        Call(append),
        LocalSet(3),
        End,
        LocalGet(6),
        I32Const(1),
        I32Add,
        LocalSet(6),
        Br(0),
        End,
        End,
        LocalGet(3),
    ]);
    b.function_with_locals(
        "async-registration-compact",
        &[VALUE, VALUE, ValType::I32],
        &[VALUE],
        &[(3, VALUE), (1, ValType::I32)],
        &body,
    );

    // An unconsumed request takes precedence over terminal publication by the
    // currently executing callback. After cancellation dispatch, cleanup owns
    // the outcome and may fail instead of cancelling.
    let mut body = vec![GlobalGet(SCHEDULER_GLOBAL), LocalSet(1)];
    item(&mut body, 1, 0);
    body.extend([
        LocalSet(1),
        I32Const(0),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
    ]);
    arr(&mut body, 1);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 1);
    body.extend([LocalGet(4), ArrayGet(ARGS), LocalSet(2)]);
    item(&mut body, 2, 0);
    body.push(LocalSet(3));
    registration_item(&mut body, 3, 4);
    body.extend([RefCastNonNull(HeapType::I31), I31GetU, I32Const(2), I32Eq]);
    producer(&mut body, 3, read);
    body.extend([LocalGet(0), RefEq, I32And]);
    item(&mut body, 2, 1);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(2),
        I32Ne,
        I32And,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Br(0),
        End,
        End,
        I32Const(0),
    ]);
    b.function_with_locals(
        "async-owner-cancellation-pending",
        &[VALUE],
        &[ValType::I32],
        &[(3, VALUE), (1, ValType::I32)],
        &body,
    );

    // Producer services need the retained cancellation latch, including a
    // consumed request whose owner is still Pending while finally suspends.
    // Do not reuse async-owner-cancellation-pending: that terminal arbitration
    // helper deliberately excludes consumed phase-2 rows.
    let mut body = vec![
        LocalGet(0),
        Call(status),
        I32Eqz,
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        GlobalGet(SCHEDULER_GLOBAL),
        LocalSet(1),
    ];
    item(&mut body, 1, 0);
    body.extend([
        LocalSet(1),
        I32Const(0),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
    ]);
    arr(&mut body, 1);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 1);
    body.extend([LocalGet(4), ArrayGet(ARGS), LocalSet(2)]);
    item(&mut body, 2, 0);
    body.push(LocalSet(3));
    registration_item(&mut body, 3, 4);
    body.extend([RefCastNonNull(HeapType::I31), I31GetU, I32Const(2), I32Eq]);
    producer(&mut body, 3, read);
    body.extend([
        LocalGet(0),
        RefEq,
        I32And,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Br(0),
        End,
        End,
        I32Const(0),
    ]);
    b.function_with_locals(
        "async-task-cancel-requested",
        &[VALUE],
        &[ValType::I32],
        &[(3, VALUE), (1, ValType::I32)],
        &body,
    );
    // Active roots already commit before dispatch; reading ownership executes
    // no source and does not switch frames or invocation selection.
    let mut body = vec![GlobalGet(SCHEDULER_GLOBAL), LocalSet(0)];
    item(&mut body, 0, 3);
    body.extend([
        LocalTee(0),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Result(VALUE)),
        I32Const(0),
        RefI31,
        Else,
    ]);
    item(&mut body, 0, 0);
    body.extend([Call(read), LocalSet(0)]);
    item(&mut body, 0, 4);
    body.push(End);
    b.function_with_locals("async-current-owner", &[], &[VALUE], &[(1, VALUE)], &body);

    // Request accepts active owners too. It queues work, never invokes the
    // resume closure. The active record independently roots the old callback.
    // Parameters owner; locals old, rows, queue, token, cont, state, resume,
    // new-rows, new-queue, active, candidate (1..11); i/gen (12..13).
    let mut body = vec![
        LocalGet(0),
        Call(status),
        I32Eqz,
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        GlobalGet(SCHEDULER_GLOBAL),
        LocalSet(1),
    ];
    item(&mut body, 1, 0);
    body.push(LocalSet(2));
    item(&mut body, 1, 1);
    body.push(LocalSet(3));
    body.extend([
        I32Const(0),
        RefI31,
        LocalSet(4),
        I32Const(0),
        LocalSet(12),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(12),
    ]);
    arr(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 2);
    body.extend([
        LocalGet(12),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(11),
    ]);
    producer(&mut body, 11, read);
    body.extend([LocalGet(0), RefEq, If(BlockType::Empty)]);
    registration_item(&mut body, 11, 4);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(2),
        I32Eq,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        LocalGet(11),
        LocalSet(4),
        End,
        LocalGet(12),
        I32Const(1),
        I32Add,
        LocalSet(12),
        Br(0),
        End,
        End,
    ]);
    item(&mut body, 1, 3);
    body.extend([
        LocalTee(10),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    item(&mut body, 10, 0);
    body.extend([Call(read), LocalSet(6)]);
    item(&mut body, 6, 4);
    body.extend([LocalGet(0), RefEq, If(BlockType::Empty)]);
    item(&mut body, 10, 2);
    body.extend([
        LocalSet(4),
        End,
        End,
        LocalGet(4),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
    ]);
    error(&mut body); // Pending storage without a task registration is not a task.
    body.push(End);
    registration_item(&mut body, 4, 0);
    body.extend([LocalTee(5), Call(read), LocalSet(6)]);
    registration_item(&mut body, 4, 2);
    body.push(LocalSet(7));
    item(&mut body, 6, 5);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalTee(13),
        I32Const(0x7fffffff),
        I32Eq,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.extend([
        End,
        GlobalGet(REGISTRATION_GLOBAL),
        LocalGet(5),
        LocalGet(0),
        LocalGet(7),
        LocalGet(13),
        I32Const(1),
        I32Add,
        RefI31,
        I32Const(2),
        RefI31,
        LocalGet(4),
        Call(b.names["async-registration-invocation"]),
        F64ConvertI32U,
        StructNew(NUMBER),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 6,
        },
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        StructNew(OBJECT),
        LocalSet(4),
        LocalGet(5),
        Call(b.names["async-continuation-cleanup-active"]),
        If(BlockType::Empty),
        LocalGet(2),
        LocalSet(8),
        LocalGet(3),
        LocalSet(9),
        Else,
        LocalGet(2),
        LocalGet(0),
        I32Const(1),
        I32Const(0),
        RefI31,
        Call(filter),
        LocalSet(8),
        LocalGet(3),
        LocalGet(0),
        I32Const(0),
        I32Const(0),
        RefI31,
        Call(filter),
        LocalSet(9),
        End,
        LocalGet(8),
        LocalGet(4),
        I32Const(1),
        RefI31,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        Call(append),
        LocalSet(8),
        LocalGet(9),
        LocalGet(4),
        Call(append),
        LocalSet(9),
    ]);
    // One publication simultaneously invalidates old awaits and roots the
    // cancellation request. Fuel interruption has no partially accepted request.
    snapshot(&mut body, 1, 8, 9);
    body.push(I32Const(1));
    b.function_with_locals(
        "async-task-cancel",
        &[VALUE],
        &[ValType::I32],
        &[(11, VALUE), (2, ValType::I32)],
        &body,
    );
}

fn task_exports(b: &mut Builder) {
    use Instruction::*;
    // Queue the initial event using a private already-ready nil dependency.
    // Initial=1 is allocated into the token before queue publication; the pump
    // delivers event-kind 0/value nil, distinct from an awaited Ready event.
    b.function_with_locals(
        "async-task-start",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &[
            LocalGet(0),
            Call(b.names["async-continuation-read"]),
            Drop,
            Call(b.names["future-pending-new"]),
            LocalSet(2),
            LocalGet(2),
            I32Const(0),
            RefI31,
            Call(b.names["future-resolve"]),
            Drop,
            LocalGet(0),
            LocalGet(2),
            LocalGet(1),
            I32Const(1),
            Call(b.names["async-registration-create"]),
        ],
    );
    // Lowering has already committed the next PC/live/unwind/frame and advanced
    // generation. A yield queues that exact state, without a fake dependency.
    // The owner is a storage placeholder; flag 3 bypasses dependency readiness.
    let mut body = vec![
        LocalGet(0),
        LocalGet(0),
        Call(b.names["async-continuation-read"]),
        LocalSet(2),
    ];
    item(&mut body, 2, 4);
    body.extend([
        LocalGet(1),
        I32Const(3),
        Call(b.names["async-registration-create"]),
    ]);
    b.function_with_locals(
        "async-task-yield",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );

    // Compiler-driven cleanup must have emptied the unwind stack before calling
    // this terminal publication. A request to cancel is not this operation.
    let mut body = vec![
        LocalGet(0),
        Call(b.names["async-continuation-read"]),
        LocalSet(3),
        LocalGet(1),
        I32Const(1),
        I32LtU,
        LocalGet(1),
        I32Const(3),
        I32GtU,
        I32Or,
        If(BlockType::Empty),
    ];
    error(&mut body);
    body.push(End);
    item(&mut body, 3, 2);
    body.extend([
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    body.extend([
        LocalGet(1),
        I32Const(CANCELLED),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.extend([End, End]);
    item(&mut body, 3, 4);
    body.push(LocalSet(4));
    body.extend([
        LocalGet(1),
        I32Const(READY),
        I32Eq,
        LocalGet(4),
        Call(b.names["async-owner-cancellation-pending"]),
        I32And,
        If(BlockType::Empty),
        I32Const(CANCELLED),
        LocalSet(1),
        I32Const(0),
        RefI31,
        LocalSet(2),
        End,
    ]);
    body.extend([
        LocalGet(1),
        I32Const(CANCELLED),
        I32Eq,
        If(BlockType::Result(ValType::I32)),
        LocalGet(4),
        Call(b.names["future-cancel"]),
        Else,
        LocalGet(1),
        I32Const(READY),
        I32Eq,
        If(BlockType::Result(ValType::I32)),
        LocalGet(4),
        LocalGet(2),
        Call(b.names["future-resolve"]),
        Else,
        LocalGet(4),
        LocalGet(2),
        Call(b.names["future-reject"]),
        End,
        End,
        LocalSet(5),
        Call(b.names["async-scheduler-prune"]),
        Drop,
        LocalGet(5),
    ]);
    b.function_with_locals(
        "async-task-complete",
        &[VALUE, ValType::I32, VALUE],
        &[ValType::I32],
        &[(2, VALUE), (1, ValType::I32)],
        &body,
    );
}

fn scheduler_runner(b: &mut Builder, refresh: u32, _enqueue: u32) {
    use Instruction::*;
    let find = b.names["async-registration-find"];
    let status = b.names["future-status"];
    let read = b.names["async-continuation-read"];
    let commit = b.names["async-continuation-commit"];
    let prune = b.names["async-scheduler-prune"];
    let service = b.names["stream-raw-service"];
    let mut body = vec![
        LocalGet(0),
        Call(b.names["async-registration-generation"]),
        Drop,
    ];
    producer(&mut body, 0, read);
    body.extend([Call(status), I32Eqz, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    body.extend([Call(prune), I32Eqz, I32Eqz]);
    b.function(
        "async-registration-retire",
        &[VALUE],
        &[ValType::I32],
        &body,
    );

    // Normal and language-error exits restore the caller. Host trap recovery
    // restores the rooted caller separately and never replays a consumed token.
    // Active: [continuation,caller-frame,registration,caller-invocation].
    let mut body = vec![GlobalGet(SCHEDULER_GLOBAL), LocalSet(0)];
    item(&mut body, 0, 3);
    body.extend([
        LocalTee(1),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        Call(prune),
        Drop,
        Call(service),
        Drop,
        Return,
        End,
    ]);
    item(&mut body, 1, 3);
    body.extend([
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        I32TruncF64U,
        GlobalSet(INVOCATION_GLOBAL),
    ]);
    item(&mut body, 1, 1);
    body.extend([
        Call(b.names["dynamic-switch"]),
        Drop,
        GlobalGet(SCHEDULER_GLOBAL),
        LocalSet(0),
    ]);
    item(&mut body, 0, 0);
    item(&mut body, 0, 1);
    item(&mut body, 0, 2);
    body.extend([
        I32Const(0),
        RefI31,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 4,
        },
        GlobalSet(SCHEDULER_GLOBAL),
        Call(prune),
        Drop,
        Call(service),
        Drop,
    ]);
    let recover = b.function_with_locals(
        "async-scheduler-finish-turn",
        &[],
        &[],
        &[(2, VALUE)],
        &body,
    );

    // Host calls only after a trapped run-one, with fresh fuel. Keep active
    // rooted until failure publication succeeds; retry cannot replay effects.
    // The original Wasmtime trap remains the host error, not a language throw.
    b.function(
        "async-runtime-trap-is",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), GlobalGet(TRAP_VALUE_GLOBAL), RefEq],
    );
    let mut body = vec![GlobalGet(SCHEDULER_GLOBAL), LocalSet(0)];
    item(&mut body, 0, 3);
    body.extend([
        LocalTee(1),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        Call(prune),
        Drop,
        Call(service),
        Drop,
        Return,
        End,
    ]);
    item(&mut body, 1, 3);
    body.extend([
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        I32TruncF64U,
        GlobalSet(INVOCATION_GLOBAL),
    ]);
    item(&mut body, 1, 1);
    body.extend([Call(b.names["dynamic-switch"]), Drop]);
    item(&mut body, 1, 0);
    body.extend([Call(read), LocalSet(0)]);
    item(&mut body, 0, 4);
    body.extend([
        GlobalGet(TRAP_VALUE_GLOBAL),
        Call(b.names["future-reject"]),
        Drop,
        Call(recover),
    ]);
    b.function_with_locals("async-scheduler-recover", &[], &[], &[(2, VALUE)], &body);

    // Values 0..9: old,rows,queue,token,newrows,newqueue,continuation,
    // next-state,caller-frame,language-payload. Integers 10=index,11=event,
    // 12=caught,13=stale. Each call executes at most one compiled resume.
    let mut body = vec![GlobalGet(SCHEDULER_GLOBAL), LocalSet(0)];
    item(&mut body, 0, 3);
    body.extend([I32Const(0), RefI31, RefEq, I32Eqz, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    body.extend([
        // Retire cancelled/terminal operations before matching or dispatch.
        // Service settlement refreshes ready work but never resumes source.
        Call(service),
        Drop,
        Call(refresh),
        Drop,
        GlobalGet(SCHEDULER_GLOBAL),
        LocalSet(0),
    ]);
    item(&mut body, 0, 0);
    body.push(LocalSet(1));
    item(&mut body, 0, 1);
    body.push(LocalSet(2));
    arr(&mut body, 2);
    body.extend([
        ArrayLen,
        I32Eqz,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    body.extend([
        I32Const(0),
        LocalSet(15),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(15),
    ]);
    arr(&mut body, 2);
    body.extend([
        ArrayLen,
        I32GeU,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
    ]);
    arr(&mut body, 2);
    body.extend([
        LocalGet(15),
        ArrayGet(ARGS),
        LocalSet(3),
        LocalGet(10001),
        I32Eqz,
    ]);
    body.extend([
        LocalGet(3),
        Call(b.names["async-registration-invocation"]),
        LocalGet(10000),
        I32Eq,
        I32Or,
        BrIf(1),
        LocalGet(15),
        I32Const(1),
        I32Add,
        LocalSet(15),
        Br(0),
        End,
        End,
    ]);
    registration_item(&mut body, 3, 0);
    body.extend([LocalTee(6), Call(read), LocalSet(7)]);
    registration_item(&mut body, 3, 4);
    body.extend([RefCastNonNull(HeapType::I31), I31GetU, LocalSet(14)]);
    body.extend([
        LocalGet(14),
        I32Const(2),
        I32Eq,
        If(BlockType::Result(ValType::I32)),
        LocalGet(6),
        Call(b.names["async-continuation-cleanup-active"]),
        Else,
        I32Const(0),
        End,
        If(BlockType::Empty),
    ]);
    // Removing the queued request while retaining its phase-0 row is atomic.
    // Preserve existing cleanup waiters; refresh will requeue when cleanup exits.
    body.extend([
        LocalGet(1),
        Call(b.names["async-snapshot-copy"]),
        LocalSet(4),
        LocalGet(1),
        LocalGet(3),
        Call(find),
        LocalSet(10),
    ]);
    arr(&mut body, 4);
    body.extend([
        LocalGet(10),
        LocalGet(3),
        I32Const(0),
        RefI31,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        ArraySet(ARGS),
    ]);
    body.extend([
        LocalGet(2),
        LocalGet(3),
        Call(b.names["async-queue-without-token"]),
        LocalSet(5),
    ]);
    snapshot(&mut body, 0, 4, 5);
    body.extend([I32Const(1), Return, End]);
    // Cancellation tokens remain valid across active-turn suspension. Other
    // tokens must match their committed generation exactly.
    body.extend([
        LocalGet(14),
        I32Const(2),
        I32Eq,
        If(BlockType::Result(ValType::I32)),
        I32Const(0),
        Else,
    ]);
    item(&mut body, 7, 5);
    body.extend([RefCastNonNull(HeapType::I31), I31GetU]);
    registration_item(&mut body, 3, 3);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Ne,
        End,
        LocalSet(13),
    ]);
    // Preparation is retryable before claim. Cancellation generation is the
    // maximum of the request target and latest state, not a repeated increment.
    body.extend([
        LocalGet(13),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(7),
        Call(b.names["async-snapshot-copy"]),
        LocalSet(7),
        LocalGet(14),
        I32Const(2),
        I32Eq,
        If(BlockType::Empty),
    ]);
    item(&mut body, 7, 5);
    body.extend([RefCastNonNull(HeapType::I31), I31GetU, LocalSet(16)]);
    registration_item(&mut body, 3, 3);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalGet(16),
        I32GtU,
        If(BlockType::Empty),
    ]);
    registration_item(&mut body, 3, 3);
    body.extend([RefCastNonNull(HeapType::I31), I31GetU, LocalSet(16), End]);
    arr(&mut body, 7);
    body.extend([
        I32Const(5),
        LocalGet(16),
        RefI31,
        ArraySet(ARGS),
        End,
        LocalGet(14),
        I32Const(2),
        I32Eq,
        If(BlockType::Result(ValType::I32)),
        I32Const(CANCELLED),
        Else,
        LocalGet(14),
        I32Const(1),
        I32Eq,
        LocalGet(14),
        I32Const(3),
        I32Eq,
        I32Or,
        If(BlockType::Result(ValType::I32)),
        I32Const(0),
        Else,
    ]);
    registration_item(&mut body, 3, 1);
    body.extend([
        Call(status),
        LocalTee(15),
        I32Const(CANCELLED),
        I32Eq,
        If(BlockType::Result(ValType::I32)),
        I32Const(FAILED),
        Else,
        LocalGet(15),
        End,
        End,
        End,
        LocalSet(11),
    ]);
    arr(&mut body, 7);
    body.extend([I32Const(6), LocalGet(11), RefI31, ArraySet(ARGS)]);
    arr(&mut body, 7);
    body.extend([
        I32Const(7),
        LocalGet(14),
        I32Eqz,
        If(BlockType::Result(VALUE)),
        LocalGet(15),
        I32Const(CANCELLED),
        I32Eq,
        If(BlockType::Result(VALUE)),
        GlobalGet(DEPENDENCY_CANCEL_GLOBAL),
        Else,
    ]);
    registration_item(&mut body, 3, 1);
    body.extend([
        Call(b.names["future-result"]),
        End,
        Else,
        I32Const(0),
        RefI31,
        End,
        ArraySet(ARGS),
        LocalGet(6),
        LocalGet(7),
        Call(commit),
        Drop,
        End,
        LocalGet(14),
        I32Const(2),
        I32Eq,
        If(BlockType::Empty),
    ]);
    // An active source callback may have registered a new await after requesting
    // cancellation. Remove it at dispatch, retaining only this request's latch.
    producer(&mut body, 3, read);
    body.push(LocalSet(9));
    body.extend([
        LocalGet(1),
        LocalGet(9),
        I32Const(1),
        LocalGet(3),
        Call(b.names["async-owner-filter"]),
        LocalSet(1),
        LocalGet(2),
        LocalGet(9),
        I32Const(0),
        LocalGet(3),
        Call(b.names["async-owner-filter"]),
        LocalSet(2),
        End,
    ]);
    body.extend([LocalGet(1), LocalGet(3), Call(find), LocalSet(10)]);
    arr(&mut body, 1);
    body.extend([ArrayLen, ArrayNewDefault(ARGS), LocalSet(4)]);
    arr(&mut body, 4);
    body.push(I32Const(0));
    arr(&mut body, 1);
    body.push(I32Const(0));
    arr(&mut body, 1);
    body.extend([
        ArrayLen,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
    ]);
    arr(&mut body, 4);
    body.extend([
        LocalGet(10),
        LocalGet(3),
        I32Const(2),
        RefI31,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        ArraySet(ARGS),
    ]);
    body.extend([
        LocalGet(2),
        LocalGet(3),
        Call(b.names["async-queue-without-token"]),
        LocalSet(5),
    ]);
    body.extend([LocalGet(13), If(BlockType::Empty)]);
    snapshot(&mut body, 0, 4, 5);
    body.extend([
        I32Const(1),
        Return,
        End,
        Call(b.names["dynamic-save"]),
        LocalSet(8),
        GlobalGet(INVOCATION_GLOBAL),
        LocalSet(16),
        LocalGet(4),
        LocalGet(5),
    ]);
    item(&mut body, 0, 2);
    body.extend([
        LocalGet(6),
        LocalGet(8),
        LocalGet(3),
        LocalGet(16),
        F64ConvertI32U,
        StructNew(NUMBER),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 4,
        },
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 4,
        },
        GlobalSet(SCHEDULER_GLOBAL),
    ]);
    body.extend([
        Block(BlockType::Empty),
        Block(BlockType::Result(VALUE)),
        TryTable(
            BlockType::Empty,
            Cow::Owned(vec![wasm_encoder::Catch::One { tag: 0, label: 0 }]),
        ),
    ]);
    body.extend([
        LocalGet(3),
        Call(b.names["async-registration-invocation"]),
        GlobalSet(INVOCATION_GLOBAL),
    ]);
    item(&mut body, 7, 3);
    body.extend([Call(b.names["dynamic-switch"]), Drop]);
    registration_item(&mut body, 3, 2);
    body.extend([
        LocalGet(6),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        Call(b.names["invoke"]),
        Drop,
        Br(2),
        End,
        Unreachable,
        End,
        LocalSet(9),
        I32Const(1),
        LocalSet(12),
        End,
    ]);
    // Restore before completion/pruning (which may allocate or exhaust fuel).
    body.extend([
        LocalGet(16),
        GlobalSet(INVOCATION_GLOBAL),
        LocalGet(8),
        Call(b.names["dynamic-switch"]),
        Drop,
        LocalGet(12),
        If(BlockType::Empty),
    ]);
    producer(&mut body, 3, read);
    body.extend([
        LocalGet(9),
        Call(b.names["future-reject"]),
        Drop,
        End,
        Call(recover),
        I32Const(1),
    ]);
    // Keep original local numbering while adding private selector parameters.
    for instruction in &mut body {
        match instruction {
            LocalGet(n) | LocalSet(n) | LocalTee(n) => {
                *n = match *n {
                    10000 => 0,
                    10001 => 1,
                    n => n + 2,
                };
            }
            _ => {}
        }
    }
    let run = b.function_with_locals(
        "async-scheduler-run-selected",
        &[ValType::I32, ValType::I32],
        &[ValType::I32],
        &[(10, VALUE), (7, ValType::I32)],
        &body,
    );
    b.function(
        "async-scheduler-run-one",
        &[],
        &[ValType::I32],
        &[I32Const(0), I32Const(0), Call(run)],
    );
    b.function(
        "async-run-invocation",
        &[ValType::I32],
        &[ValType::I32],
        &[LocalGet(0), I32Const(1), Call(run)],
    );
    let mut body = vec![GlobalGet(SCHEDULER_GLOBAL), LocalSet(0)];
    item(&mut body, 0, 0);
    body.extend([RefCastNonNull(HeapType::Concrete(ARGS)), ArrayLen]);
    item(&mut body, 0, 1);
    body.extend([RefCastNonNull(HeapType::Concrete(ARGS)), ArrayLen]);
    b.function_with_locals(
        "async-scheduler-counts",
        &[],
        &[ValType::I32, ValType::I32],
        &[(1, VALUE)],
        &body,
    );
}

// Determine task ownership from the scheduler's private rooted registrations,
// including an active callback. No public object field or invented marker is
// used. Terminal storage remains subject to future-status validation.
fn source_api_cancellation(b: &mut Builder) {
    use Instruction::*;
    let read = b.names["async-continuation-read"];
    let mut body = vec![
        LocalGet(0),
        Call(b.names["future-status"]),
        I32Const(0),
        I32Ne,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        GlobalGet(SCHEDULER_GLOBAL),
        LocalSet(1),
    ];
    item(&mut body, 1, 0);
    body.extend([
        LocalSet(2),
        I32Const(0),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
    ]);
    arr(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 2);
    body.extend([LocalGet(4), ArrayGet(ARGS), LocalSet(3)]);
    item(&mut body, 3, 0);
    body.push(LocalSet(3));
    producer(&mut body, 3, read);
    body.extend([
        LocalGet(0),
        RefEq,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Br(0),
        End,
        End,
    ]);
    item(&mut body, 1, 3);
    body.extend([
        LocalTee(3),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    item(&mut body, 3, 0);
    body.push(Call(read));
    body.extend([
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(4),
        ArrayGet(ARGS),
        LocalGet(0),
        RefEq,
        Return,
        End,
        I32Const(0),
    ]);
    let owned = b.function_with_locals(
        "async-future-task-owned",
        &[VALUE],
        &[ValType::I32],
        &[(3, VALUE), (1, ValType::I32)],
        &body,
    );
    b.function(
        "async-future-cancel",
        &[VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            Call(owned),
            If(BlockType::Result(ValType::I32)),
            LocalGet(0),
            Call(b.names["async-task-cancel"]),
            Else,
            LocalGet(0),
            Call(b.names["future-cancel"]),
            End,
        ],
    );
}

// Reset preparation captures stable roots before requesting cancellation. The
// caller must still drive cleanup, or retain this Store while cleanup awaits I/O.
fn scheduler_reset_exports(b: &mut Builder) {
    use Instruction::*;
    let append = b.names["async-array-append"];
    let read = b.names["async-continuation-read"];
    let status = b.names["future-status"];
    let contains = b.function_with_locals(
        "async-array-ref-contains",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, ValType::I32)],
        &[
            I32Const(0),
            LocalSet(2),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(2),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
            I32GeU,
            BrIf(1),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            LocalGet(2),
            ArrayGet(ARGS),
            LocalGet(1),
            RefEq,
            If(BlockType::Empty),
            I32Const(1),
            Return,
            End,
            LocalGet(2),
            I32Const(1),
            I32Add,
            LocalSet(2),
            Br(0),
            End,
            End,
            I32Const(0),
        ],
    );
    // old snapshot, rows, owners, token, owner, active (0..5); index 6.
    let mut body = vec![
        GlobalGet(SCHEDULER_GLOBAL),
        LocalSet(0),
        I32Const(0),
        ArrayNewDefault(ARGS),
        LocalSet(2),
    ];
    item(&mut body, 0, 0);
    body.extend([
        LocalSet(1),
        I32Const(0),
        LocalSet(6),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(6),
    ]);
    arr(&mut body, 1);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 1);
    body.extend([
        LocalGet(6),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(3),
    ]);
    producer(&mut body, 3, read);
    body.extend([
        LocalTee(4),
        Call(status),
        I32Eqz,
        LocalGet(2),
        LocalGet(4),
        Call(contains),
        I32Eqz,
        I32And,
        If(BlockType::Empty),
        LocalGet(2),
        LocalGet(4),
        Call(append),
        LocalSet(2),
        End,
        LocalGet(6),
        I32Const(1),
        I32Add,
        LocalSet(6),
        Br(0),
        End,
        End,
    ]);
    item(&mut body, 0, 3);
    body.extend([
        LocalTee(5),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    item(&mut body, 5, 0);
    body.extend([
        Call(read),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(4),
        ArrayGet(ARGS),
        LocalTee(4),
        Call(status),
        I32Eqz,
        LocalGet(2),
        LocalGet(4),
        Call(contains),
        I32Eqz,
        I32And,
        If(BlockType::Empty),
        LocalGet(2),
        LocalGet(4),
        Call(append),
        LocalSet(2),
        End,
        End,
        LocalGet(2),
    ]);
    let owners = b.function_with_locals(
        "async-scheduler-owner-snapshot",
        &[],
        &[VALUE],
        &[(6, VALUE), (1, ValType::I32)],
        &body,
    );
    b.function(
        "async-scheduler-pending-task-count",
        &[],
        &[ValType::I32],
        &[
            Call(owners),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            ArrayLen,
        ],
    );
    // All requests read the captured array, not the mutating registry. Existing
    // requests count zero; a fuel-interrupted partial pass can safely retry.
    let mut body = vec![
        Call(owners),
        LocalSet(0),
        I32Const(0),
        LocalSet(1),
        I32Const(0),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
    ];
    arr(&mut body, 0);
    body.extend([ArrayLen, I32GeU, BrIf(1), LocalGet(2)]);
    arr(&mut body, 0);
    body.extend([
        LocalGet(1),
        ArrayGet(ARGS),
        Call(b.names["async-task-cancel"]),
        I32Add,
        LocalSet(2),
        LocalGet(1),
        I32Const(1),
        I32Add,
        LocalSet(1),
        Br(0),
        End,
        End,
        Call(b.names["stream-raw-service"]),
        Drop,
        LocalGet(2),
    ]);
    b.function_with_locals(
        "async-scheduler-cancel-all",
        &[],
        &[ValType::I32],
        &[(1, VALUE), (2, ValType::I32)],
        &body,
    );
}

// Invocation ownership is transport metadata, outside the compiler continuation
// and ABI2 recursive group. Exact f64-boxed u32 keys avoid i31 truncation.
fn invocation_storage(b: &mut Builder) {
    use Instruction::*;
    let mut body = Vec::new();
    registration_item(&mut body, 0, 5);
    body.extend([
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        I32TruncF64U,
    ]);
    let scope = b.function(
        "async-registration-invocation",
        &[VALUE],
        &[ValType::I32],
        &body,
    );
    b.function(
        "async-enter-invocation",
        &[ValType::I32],
        &[ValType::I32],
        &[
            GlobalGet(INVOCATION_GLOBAL),
            LocalGet(0),
            GlobalSet(INVOCATION_GLOBAL),
        ],
    );
    // Existing task ownership wins over the current caller's selection on every
    // await/yield registration. New descendants inherit the running selection.
    let mut body = vec![GlobalGet(SCHEDULER_GLOBAL), LocalSet(1)];
    item(&mut body, 1, 0);
    body.extend([
        LocalSet(2),
        I32Const(0),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
    ]);
    arr(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 2);
    body.extend([LocalGet(4), ArrayGet(ARGS), LocalSet(3)]);
    item(&mut body, 3, 0);
    body.push(LocalSet(3));
    registration_item(&mut body, 3, 0);
    body.extend([
        LocalGet(0),
        RefEq,
        If(BlockType::Empty),
        LocalGet(3),
        Call(scope),
        Return,
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Br(0),
        End,
        End,
    ]);
    item(&mut body, 1, 3);
    body.extend([
        LocalTee(3),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    item(&mut body, 3, 0);
    body.extend([LocalGet(0), RefEq, If(BlockType::Empty)]);
    item(&mut body, 3, 2);
    body.extend([Call(scope), Return, End, End, GlobalGet(INVOCATION_GLOBAL)]);
    b.function_with_locals(
        "async-continuation-invocation",
        &[VALUE],
        &[ValType::I32],
        &[(3, VALUE), (1, ValType::I32)],
        &body,
    );
    // Preserve the relative FIFO order of every unselected scope. Preparation
    // may trap; only the caller's final immutable scheduler publication commits.
    let mut body = vec![
        I32Const(0),
        ArrayNewDefault(ARGS),
        LocalSet(2),
        I32Const(0),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
    ];
    arr(&mut body, 0);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 0);
    body.extend([
        LocalGet(4),
        ArrayGet(ARGS),
        LocalTee(3),
        LocalGet(1),
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(2),
        LocalGet(3),
        Call(b.names["async-array-append"]),
        LocalSet(2),
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Br(0),
        End,
        End,
        LocalGet(2),
    ]);
    b.function_with_locals(
        "async-queue-without-token",
        &[VALUE, VALUE],
        &[VALUE],
        &[(2, VALUE), (1, ValType::I32)],
        &body,
    );
}

fn invocation_exports(b: &mut Builder) {
    use Instruction::*;
    let scope = b.names["async-registration-invocation"];
    let read = b.names["async-continuation-read"];
    let status = b.names["future-status"];
    let append = b.names["async-array-append"];
    let contains = b.names["async-array-ref-contains"];
    // Snapshot matching owners once, including an active callback. Cancellation
    // iterates this rooted immutable list, never the mutating live registry.
    // key0; snapshot1, rows2, owners3, token4, owner5, active6, index7.
    let mut body = vec![
        GlobalGet(SCHEDULER_GLOBAL),
        LocalSet(1),
        I32Const(0),
        ArrayNewDefault(ARGS),
        LocalSet(3),
    ];
    item(&mut body, 1, 0);
    body.extend([
        LocalSet(2),
        I32Const(0),
        LocalSet(7),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(7),
    ]);
    arr(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 2);
    body.extend([LocalGet(7), ArrayGet(ARGS), LocalSet(4)]);
    item(&mut body, 4, 0);
    body.push(LocalSet(4));
    body.extend([
        LocalGet(4),
        Call(scope),
        LocalGet(0),
        I32Eq,
        If(BlockType::Empty),
    ]);
    producer(&mut body, 4, read);
    body.extend([
        LocalTee(5),
        Call(status),
        I32Eqz,
        LocalGet(3),
        LocalGet(5),
        Call(contains),
        I32Eqz,
        I32And,
        If(BlockType::Empty),
        LocalGet(3),
        LocalGet(5),
        Call(append),
        LocalSet(3),
        End,
        End,
        LocalGet(7),
        I32Const(1),
        I32Add,
        LocalSet(7),
        Br(0),
        End,
        End,
    ]);
    item(&mut body, 1, 3);
    body.extend([
        LocalTee(6),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    item(&mut body, 6, 2);
    body.extend([
        LocalTee(4),
        Call(scope),
        LocalGet(0),
        I32Eq,
        If(BlockType::Empty),
    ]);
    producer(&mut body, 4, read);
    body.extend([
        LocalTee(5),
        Call(status),
        I32Eqz,
        LocalGet(3),
        LocalGet(5),
        Call(contains),
        I32Eqz,
        I32And,
        If(BlockType::Empty),
        LocalGet(3),
        LocalGet(5),
        Call(append),
        LocalSet(3),
        End,
        End,
        End,
        LocalGet(3),
    ]);
    let owners = b.function_with_locals(
        "async-invocation-owner-snapshot",
        &[ValType::I32],
        &[VALUE],
        &[(6, VALUE), (1, ValType::I32)],
        &body,
    );
    let mut body = vec![
        LocalGet(0),
        Call(owners),
        LocalSet(1),
        I32Const(0),
        LocalSet(2),
        I32Const(0),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
    ];
    arr(&mut body, 1);
    body.extend([ArrayLen, I32GeU, BrIf(1), LocalGet(3)]);
    arr(&mut body, 1);
    body.extend([
        LocalGet(2),
        ArrayGet(ARGS),
        Call(b.names["async-task-cancel"]),
        I32Add,
        LocalSet(3),
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        Br(0),
        End,
        End,
        Call(b.names["stream-raw-service"]),
        Drop,
        LocalGet(3),
    ]);
    b.function_with_locals(
        "async-cancel-invocation",
        &[ValType::I32],
        &[ValType::I32],
        &[(1, VALUE), (2, ValType::I32)],
        &body,
    );
    // A cancellation wave remains live until every latched owner is terminal.
    // Flag-2 rows survive queued, deferred and consumed phases through cleanup,
    // including active callbacks. Ordinary cleanup-created children have no latch
    // and must not be cancelled by a new wave while their parent is retiring.
    // key0; rows1, unique owners2, token3, owner4; index5.
    let mut body = vec![GlobalGet(SCHEDULER_GLOBAL)];
    body.extend([
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(1),
        I32Const(0),
        ArrayNewDefault(ARGS),
        LocalSet(2),
        I32Const(0),
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(5),
    ]);
    arr(&mut body, 1);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 1);
    body.extend([LocalGet(5), ArrayGet(ARGS), LocalSet(3)]);
    item(&mut body, 3, 0);
    body.push(LocalSet(3));
    registration_item(&mut body, 3, 4);
    body.extend([
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(2),
        I32Eq,
        LocalGet(3),
        Call(scope),
        LocalGet(0),
        I32Eq,
        I32And,
        If(BlockType::Empty),
    ]);
    producer(&mut body, 3, read);
    body.extend([
        LocalTee(4),
        Call(status),
        I32Eqz,
        LocalGet(2),
        LocalGet(4),
        Call(contains),
        I32Eqz,
        I32And,
        If(BlockType::Empty),
        LocalGet(2),
        LocalGet(4),
        Call(append),
        LocalSet(2),
        End,
        End,
        LocalGet(5),
        I32Const(1),
        I32Add,
        LocalSet(5),
        Br(0),
        End,
        End,
    ]);
    arr(&mut body, 2);
    body.push(ArrayLen);
    b.function_with_locals(
        "async-invocation-retiring-count",
        &[ValType::I32],
        &[ValType::I32],
        &[(4, VALUE), (1, ValType::I32)],
        &body,
    );
    // Three core results, no public array decoding or ownership marker exposed.
    // key0; snapshot1, queue2, token3, active4; pending5, queued6, active7,index8.
    let mut body = vec![
        LocalGet(0),
        Call(owners),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalSet(5),
        GlobalGet(SCHEDULER_GLOBAL),
        LocalSet(1),
    ];
    item(&mut body, 1, 1);
    body.extend([
        LocalSet(2),
        I32Const(0),
        LocalSet(8),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(8),
    ]);
    arr(&mut body, 2);
    body.extend([ArrayLen, I32GeU, BrIf(1)]);
    arr(&mut body, 2);
    body.extend([
        LocalGet(8),
        ArrayGet(ARGS),
        LocalSet(3),
        LocalGet(3),
        Call(scope),
        LocalGet(0),
        I32Eq,
    ]);
    producer(&mut body, 3, read);
    body.extend([
        Call(status),
        I32Eqz,
        I32And,
        If(BlockType::Empty),
        LocalGet(6),
        I32Const(1),
        I32Add,
        LocalSet(6),
        End,
        LocalGet(8),
        I32Const(1),
        I32Add,
        LocalSet(8),
        Br(0),
        End,
        End,
    ]);
    item(&mut body, 1, 3);
    body.extend([
        LocalTee(4),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    item(&mut body, 4, 2);
    body.extend([
        Call(scope),
        LocalGet(0),
        I32Eq,
        LocalSet(7),
        End,
        LocalGet(5),
        LocalGet(6),
        LocalGet(7),
    ]);
    b.function_with_locals(
        "async-invocation-counts",
        &[ValType::I32],
        &[ValType::I32, ValType::I32, ValType::I32],
        &[(4, VALUE), (4, ValType::I32)],
        &body,
    );
}
