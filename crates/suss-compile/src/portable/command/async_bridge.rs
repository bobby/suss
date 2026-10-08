//! Original scalar canonical async-import transport for compiled source closures.
//!
//! Assembly supplies the resolved canonical functions; this module neither polls
//! host Rust futures nor treats native Session completion as canonical transport.
//! A canonical task context identifies the invocation. Every operation owns a
//! rooted completion, a result allocation, a subtask and its waitable membership.
use super::diagnostic;
use crate::{
    portable::{Diagnostic, resolve::Global},
    runtime_abi,
};
use std::borrow::Cow;
use wasm_encoder::*;
use wit_parser::{
    FunctionKind, Resolve, Type,
    abi::{AbiVariant, WasmType},
};

const VALUE: ValType = ValType::Ref(RefType::EQREF);
const CAPACITY: i32 = 64;
// Private typed record: invocation, generation, completion, subtask, pointer,
// phase, joined, dropped, released, acknowledgment, packed initial receipt.
// Phase 0 is starting, 1 pending, 2 cancellation intent, 9 quarantined.
// Acknowledgment 0 means no receipt; -1 is canonical Blocked; 2/3/4 terminal.
const ROOTS: u32 = 1; // binding cell is imported global 0
const GENERATION: u32 = 2;
const RECORD: u32 = runtime_abi::TYPE_COUNT;

/// Generate the closure and event transport for one resolved u32 async import.
///
/// Canonical shim imports: `call(i32,i32)->i32`, `context()->i32`,
/// `join(i32,i32)`, `subtask-drop(i32)`, `cancel(i32)->i32`.
/// `suss.context::waitable-set()` is supplied by context_module().
/// Memory shim exports memory, cabi_realloc, release. Assembly owns waitable
/// sets and must use a nonzero, never-reused invocation key in context slot 0.
/// `invoke` joins a pending subtask to the set returned by `waitable-set()`.
/// `complete-event(event,handle,status)->i32` queues settlement only; assembly's
/// callback subsequently pumps source work and chooses WAIT/YIELD/EXIT.
/// `cancel-invocation(key)->i32` requests cancellation of owned operations;
/// `pending-invocation(key)->i32` counts outstanding acknowledgments. Assembly
/// must drain acknowledgments before dropping that invocation's waitable set.
/// `poll-invocation(key)->i32` propagates source completion cancellation;
/// `resume-invocation(key)->i32` resumes acknowledged settlement/teardown.
/// Both are bounded by CAPACITY; their return value is work performed, not a
/// count of outstanding operations. Use pending-invocation for drain decisions.
/// `quarantine-invocation(key)` prevents retry after an uncertain builtin trap.
/// Caller retains the original error. An initial call without a receipt cannot
/// be replayed; ownership must be quarantined and the old Store retired explicitly.
/// Fuel/epoch retry safety requires executed sweeps of these progress boundaries;
/// the journal alone is not that proof.
pub fn module(
    resolve: &Resolve,
    import: &wit_parser::Function,
    cell: &Global,
) -> Result<Vec<u8>, Diagnostic> {
    if import.kind != FunctionKind::AsyncFreestanding
        || import.params.len() != 1
        || scalar(resolve, import.params[0].ty)? != Type::U32
        || scalar(
            resolve,
            import
                .result
                .ok_or_else(|| diagnostic("Async scalar import requires a result"))?,
        )? != Type::U32
    {
        return Err(diagnostic(
            "Async bridge currently requires resolved async func(u32) -> u32",
        ));
    }
    let abi = resolve.wasm_signature(AbiVariant::GuestImportAsync, import);
    if abi.params != [WasmType::I32, WasmType::Pointer] || abi.results != [WasmType::I32] {
        return Err(diagnostic(format!(
            "Unsupported resolved scalar async ABI: {abi:?}"
        )));
    }
    let mut types = runtime_abi::prelude();
    // TypeSection::len counts recursive groups, not their constituent types.
    types.ty().struct_((0..11).map(|index| FieldType {
        element_type: StorageType::Val(if index == 2 { VALUE } else { ValType::I32 }),
        mutable: true,
    }));
    let mut next_type = runtime_abi::TYPE_COUNT + 1;
    let mut imports = ImportSection::new();
    let mut functions = FunctionSection::new();
    let mut code = CodeSection::new();
    let mut n = 0;
    let mut add_import = |module: &str, name: &str, params: Vec<ValType>, results: Vec<ValType>| {
        let ty = next_type;
        next_type += 1;
        types.ty().function(params, results);
        imports.import(module, name, EntityType::Function(ty));
        let index = n;
        n += 1;
        index
    };
    let pending = add_import("suss.runtime", "future-pending-new", vec![], vec![VALUE]);
    let resolve_future = add_import(
        "suss.runtime",
        "future-resolve",
        vec![VALUE, VALUE],
        vec![ValType::I32],
    );
    let cancel_future = add_import(
        "suss.runtime",
        "future-cancel",
        vec![VALUE],
        vec![ValType::I32],
    );
    let status = add_import(
        "suss.runtime",
        "future-status",
        vec![VALUE],
        vec![ValType::I32],
    );
    let number = add_import(
        "suss.runtime",
        "number-box",
        vec![ValType::F64],
        vec![VALUE],
    );
    let set_binding = add_import("suss.runtime", "binding-set", vec![VALUE, VALUE], vec![]);
    let closure = add_import(
        "suss.runtime",
        "closure-new",
        vec![
            VALUE,
            ValType::Ref(RefType {
                nullable: false,
                heap_type: HeapType::Concrete(runtime_abi::INVOKE),
            }),
            ValType::I32,
            ValType::I32,
        ],
        vec![VALUE],
    );
    let host = add_import(
        "suss.canonical",
        "call",
        vec![ValType::I32; 2],
        vec![ValType::I32],
    );
    let context = add_import("suss.canonical", "context", vec![], vec![ValType::I32]);
    let set = add_import("suss.context", "waitable-set", vec![], vec![ValType::I32]);
    let join = add_import("suss.canonical", "join", vec![ValType::I32; 2], vec![]);
    let drop_task = add_import("suss.canonical", "subtask-drop", vec![ValType::I32], vec![]);
    let cancel_task = add_import(
        "suss.canonical",
        "cancel",
        vec![ValType::I32],
        vec![ValType::I32],
    );
    let realloc = add_import(
        "suss.memory",
        "cabi_realloc",
        vec![ValType::I32; 4],
        vec![ValType::I32],
    );
    let release = add_import("suss.memory", "release", vec![ValType::I32; 2], vec![]);
    drop(add_import);
    let tag = next_type;
    next_type += 1;
    types.ty().function([VALUE], []);
    imports.import(
        "suss.runtime",
        "language-exception",
        EntityType::Tag(TagType {
            kind: TagKind::Exception,
            func_type_idx: tag,
        }),
    );
    imports.import(
        cell.import_module(),
        &cell.import_name(),
        EntityType::Global(GlobalType {
            val_type: runtime_abi::binding_cell_type(),
            mutable: false,
            shared: false,
        }),
    );
    imports.import(
        "suss.memory",
        "memory",
        EntityType::Memory(MemoryType {
            minimum: 1,
            maximum: Some(65536),
            memory64: false,
            shared: false,
            page_size_log2: None,
        }),
    );
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            Instruction::I32Const(CAPACITY),
            Instruction::ArrayNewDefault(runtime_abi::ARGS),
        ]),
    );
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(1),
    );
    let teardown = n + 5;
    let request = n + 6;
    let invoke = n;
    functions.function(runtime_abi::INVOKE);
    // Params env/args; locals slot/key/pointer/packed/state/generation, record,
    // completion and checked numeric value. Publish roots before calling host.
    let mut body = Vec::new();
    use Instruction::*;
    body.extend([
        LocalGet(1),
        ArrayLen,
        I32Const(1),
        I32Ne,
        If(BlockType::Empty),
    ]);
    fail(&mut body);
    body.push(End);
    body.extend([
        LocalGet(1),
        I32Const(0),
        ArrayGet(runtime_abi::ARGS),
        RefTestNonNull(HeapType::Concrete(runtime_abi::NUMBER)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    fail(&mut body);
    body.push(End);
    body.extend([
        LocalGet(1),
        I32Const(0),
        ArrayGet(runtime_abi::ARGS),
        RefCastNonNull(HeapType::Concrete(runtime_abi::NUMBER)),
        StructGet {
            struct_type_index: runtime_abi::NUMBER,
            field_index: 0,
        },
        LocalSet(10),
        LocalGet(10),
        F64Const(0.0.into()),
        F64Ge,
        LocalGet(10),
        F64Const(4294967295.0.into()),
        F64Le,
        I32And,
        LocalGet(10),
        LocalGet(10),
        F64Trunc,
        F64Eq,
        I32And,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    fail(&mut body);
    body.push(End);
    body.extend([Call(context), LocalTee(3), I32Eqz, If(BlockType::Empty)]);
    fail(&mut body);
    body.push(End);
    body.extend([
        GlobalGet(GENERATION),
        LocalTee(7),
        I32Const(-1),
        I32Eq,
        If(BlockType::Empty),
    ]);
    fail(&mut body);
    body.push(End);
    body.extend([
        I32Const(0),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Const(CAPACITY),
        I32GeU,
        If(BlockType::Empty),
    ]);
    fail(&mut body);
    body.push(End);
    root(&mut body);
    body.extend([
        LocalGet(2),
        ArrayGet(runtime_abi::ARGS),
        RefIsNull,
        BrIf(1),
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        Br(0),
        End,
        End,
    ]);
    // Allocate and publish GC ownership before taking any linear-memory or
    // canonical resource. Progress stores following resource calls cannot allocate.
    body.extend([
        Call(pending),
        LocalSet(9),
        LocalGet(3),
        LocalGet(7),
        LocalGet(9),
        I32Const(0),
        I32Const(0),
        I32Const(0),
        I32Const(0),
        I32Const(1), // no subtask exists yet
        I32Const(1), // no result allocation exists yet
        I32Const(0), // acknowledgment
        I32Const(0), // packed initial-call receipt
        StructNew(RECORD),
        LocalSet(8),
    ]);
    root(&mut body);
    body.extend([
        LocalGet(2),
        LocalGet(8),
        ArraySet(runtime_abi::ARGS),
        LocalGet(7),
        I32Const(1),
        I32Add,
        GlobalSet(GENERATION),
        LocalGet(8),
        RefCastNonNull(HeapType::Concrete(RECORD)),
        I32Const(0),
        I32Const(0),
        I32Const(4),
        I32Const(4),
        Call(realloc),
        LocalTee(4),
        StructSet {
            struct_type_index: RECORD,
            field_index: 4,
        },
    ]);
    field_set(&mut body, 8, 8, &[I32Const(0)]);
    body.extend([
        LocalGet(8),
        RefCastNonNull(HeapType::Concrete(RECORD)),
        LocalGet(10),
        I32TruncF64U,
        LocalGet(4),
        Call(host),
        LocalTee(5),
        StructSet {
            struct_type_index: RECORD,
            field_index: 10,
        },
        LocalGet(5),
        I32Const(15),
        I32And,
        LocalSet(6),
        LocalGet(6),
        I32Const(2),
        I32Eq,
        If(BlockType::Empty),
    ]);
    // No handle exists for immediate RETURNED. Journal before allocating payload.
    field_set(&mut body, 8, 7, &[I32Const(1)]);
    field_set(&mut body, 8, 9, &[I32Const(2)]);
    body.extend([
        LocalGet(9),
        LocalGet(4),
        I32Load(mem()),
        F64ConvertI32U,
        Call(number),
        Call(resolve_future),
        Drop,
        LocalGet(8),
        LocalGet(2),
        Call(teardown),
        Else,
    ]);
    // Starting/Started retain a live handle; cancellation-at-start is terminal.
    body.extend([LocalGet(6), I32Const(4), I32GtU, If(BlockType::Empty)]);
    fail(&mut body);
    body.push(End);
    body.extend([LocalGet(6), I32Const(3), I32GeU, If(BlockType::Empty)]);
    field_set(&mut body, 8, 7, &[I32Const(0)]);
    field_set(&mut body, 8, 3, &[LocalGet(5), I32Const(4), I32ShrU]);
    field_set(&mut body, 8, 9, &[LocalGet(6)]);
    body.extend([
        LocalGet(9),
        Call(cancel_future),
        Drop,
        LocalGet(8),
        LocalGet(2),
        Call(teardown),
        Else,
    ]);
    field_set(&mut body, 8, 7, &[I32Const(0)]);
    field_set(&mut body, 8, 3, &[LocalGet(5), I32Const(4), I32ShrU]);
    field_set(&mut body, 8, 5, &[I32Const(1)]);
    body.extend([
        LocalGet(8),
        RefCastNonNull(HeapType::Concrete(RECORD)),
        LocalGet(5),
        I32Const(4),
        I32ShrU,
        Call(set),
        Call(join),
        I32Const(1),
        StructSet {
            struct_type_index: RECORD,
            field_index: 6,
        },
    ]);
    body.extend([End, End, LocalGet(9), End]);
    emit(
        &mut code,
        [(6, ValType::I32), (2, VALUE), (1, ValType::F64)],
        body,
    );
    let install = n + 1;
    let ty = next_type;
    next_type += 1;
    types.ty().function([], []);
    functions.function(ty);
    emit(
        &mut code,
        [],
        vec![
            GlobalGet(0),
            I32Const(0),
            RefI31,
            RefFunc(invoke),
            I32Const(1),
            I32Const(1),
            Call(closure),
            Call(set_binding),
            End,
        ],
    );
    let complete = n + 2;
    let ty = next_type;
    next_type += 1;
    types.ty().function([ValType::I32; 3], [ValType::I32]);
    functions.function(ty);
    // Params event/handle/status; locals slot/key/record. Context and generation
    // belong to the enclosing canonical task, not caller-provided host payload.
    let mut body = vec![
        LocalGet(0),
        I32Const(1),
        I32Ne,
        If(BlockType::Empty),
        I32Const(0),
        Return,
        End,
        Call(context),
        LocalSet(4),
        I32Const(0),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        I32Const(CAPACITY),
        I32GeU,
        BrIf(1),
    ];
    root(&mut body);
    body.extend([
        LocalGet(3),
        ArrayGet(runtime_abi::ARGS),
        LocalTee(5),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    field(&mut body, 5, 0);
    body.extend([LocalGet(4), I32Eq]);
    field(&mut body, 5, 3);
    body.extend([
        LocalGet(1),
        I32Eq,
        I32And,
        If(BlockType::Empty),
        LocalGet(2),
        I32Const(1),
        I32LeU,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
    ]);
    field_set(&mut body, 5, 9, &[LocalGet(2)]);
    field(&mut body, 5, 5);
    body.extend([I32Const(9), I32Eq, If(BlockType::Empty)]);
    fail(&mut body);
    body.push(End);
    field(&mut body, 5, 5);
    body.extend([I32Const(2), I32Eq]);
    field(&mut body, 5, 2);
    body.extend([
        Call(status),
        I32Const(3),
        I32Eq,
        I32Or,
        If(BlockType::Empty),
    ]);
    // Intent wins even if the transport delivers a racing successful result.
    field_set(&mut body, 5, 5, &[I32Const(2)]);
    field(&mut body, 5, 2);
    body.extend([
        Call(cancel_future),
        Drop,
        Else,
        LocalGet(2),
        I32Const(2),
        I32Eq,
        If(BlockType::Empty),
    ]);
    field(&mut body, 5, 2);
    body.extend([Call(status), I32Eqz, If(BlockType::Empty)]);
    field(&mut body, 5, 2);
    field(&mut body, 5, 4);
    body.extend([
        I32Load(mem()),
        F64ConvertI32U,
        Call(number),
        Call(resolve_future),
        Drop,
        End,
        Else,
        LocalGet(2),
        I32Const(3),
        I32Eq,
        LocalGet(2),
        I32Const(4),
        I32Eq,
        I32Or,
        If(BlockType::Empty),
    ]);
    field(&mut body, 5, 2);
    body.extend([Call(cancel_future), Drop, Else]);
    fail(&mut body);
    body.extend([
        End,
        End,
        End,
        LocalGet(5),
        LocalGet(3),
        Call(teardown),
        I32Const(1),
        Return,
        End,
        End,
        LocalGet(3),
        I32Const(1),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        I32Const(0),
        End,
    ]);
    emit(&mut code, [(2, ValType::I32), (1, VALUE)], body);
    let count = n + 3;
    let ty = next_type;
    next_type += 1;
    types.ty().function([ValType::I32], [ValType::I32]);
    functions.function(ty);
    let mut body = vec![
        I32Const(0),
        LocalSet(1),
        I32Const(0),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
        I32Const(CAPACITY),
        I32GeU,
        BrIf(1),
    ];
    root(&mut body);
    body.extend([
        LocalGet(1),
        ArrayGet(runtime_abi::ARGS),
        LocalTee(3),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    field(&mut body, 3, 0);
    body.extend([
        LocalGet(0),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        End,
        End,
        LocalGet(1),
        I32Const(1),
        I32Add,
        LocalSet(1),
        Br(0),
        End,
        End,
        LocalGet(2),
        End,
    ]);
    emit(&mut code, [(2, ValType::I32), (1, VALUE)], body);
    let cancel = n + 4;
    let ty = next_type;
    next_type += 1;
    types.ty().function([ValType::I32], [ValType::I32]);
    functions.function(ty);
    let mut body = scan_prefix();
    field(&mut body, 3, 0);
    body.extend([LocalGet(0), I32Eq, If(BlockType::Empty)]);
    field(&mut body, 3, 5);
    body.extend([
        I32Const(1),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        End,
    ]);
    body.extend([LocalGet(3), LocalGet(1), Call(request), End]);
    scan_suffix(&mut body);
    emit(&mut code, [(2, ValType::I32), (1, VALUE)], body);

    // Resource journal: joined(6), dropped(7), released(8), acknowledgment(9).
    // Every successful external action is immediately followed by a prevalidated
    // nonallocating progress write, before another call/loop checkpoint.
    let ty = next_type;
    next_type += 1;
    types.ty().function([VALUE, ValType::I32], []);
    functions.function(ty);
    let mut body = Vec::new();
    field(&mut body, 0, 6);
    body.push(If(BlockType::Empty));
    progress_call(
        &mut body,
        0,
        6,
        0,
        &[
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(RECORD)),
            StructGet {
                struct_type_index: RECORD,
                field_index: 3,
            },
            I32Const(0),
            Call(join),
        ],
    );
    body.push(End);
    field(&mut body, 0, 7);
    body.extend([I32Eqz, If(BlockType::Empty)]);
    progress_call(
        &mut body,
        0,
        7,
        1,
        &[
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(RECORD)),
            StructGet {
                struct_type_index: RECORD,
                field_index: 3,
            },
            Call(drop_task),
        ],
    );
    body.push(End);
    field(&mut body, 0, 8);
    body.extend([I32Eqz, If(BlockType::Empty)]);
    progress_call(
        &mut body,
        0,
        8,
        1,
        &[
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(RECORD)),
            StructGet {
                struct_type_index: RECORD,
                field_index: 4,
            },
            I32Const(4),
            Call(release),
        ],
    );
    body.push(End);
    root(&mut body);
    body.extend([
        LocalGet(1),
        RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }),
        ArraySet(runtime_abi::ARGS),
        End,
    ]);
    emit(&mut code, [], body);

    let ty = next_type;
    next_type += 1;
    types.ty().function([VALUE, ValType::I32], []);
    functions.function(ty);
    let mut body = Vec::new();
    field(&mut body, 0, 5);
    body.extend([I32Const(9), I32Eq, If(BlockType::Empty)]);
    fail(&mut body);
    body.push(End);
    field(&mut body, 0, 5);
    body.extend([I32Eqz]);
    field(&mut body, 0, 9);
    body.extend([I32Eqz, I32And, If(BlockType::Empty)]);
    // Interrupted before the initial receipt: retain ownership for explicit
    // quarantine/Store retirement; never replay the source import or guess a handle.
    fail(&mut body);
    body.push(End);
    field_set(&mut body, 0, 5, &[I32Const(2)]);
    // Completion storage cancellation is retryable and never executes source.
    field(&mut body, 0, 2);
    body.extend([Call(cancel_future), Drop]);
    field(&mut body, 0, 9);
    body.extend([I32Eqz, If(BlockType::Empty)]);
    field(&mut body, 0, 6);
    body.push(If(BlockType::Empty));
    progress_call(
        &mut body,
        0,
        6,
        0,
        &[
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(RECORD)),
            StructGet {
                struct_type_index: RECORD,
                field_index: 3,
            },
            I32Const(0),
            Call(join),
        ],
    );
    body.push(End);
    // Store the returned receipt immediately. -1 means Blocked, not a status.
    body.extend([LocalGet(0), RefCastNonNull(HeapType::Concrete(RECORD))]);
    field(&mut body, 0, 3);
    body.extend([
        Call(cancel_task),
        StructSet {
            struct_type_index: RECORD,
            field_index: 9,
        },
        End,
    ]);
    field(&mut body, 0, 9);
    body.extend([I32Const(-1), I32Eq, If(BlockType::Empty)]);
    field(&mut body, 0, 6);
    body.extend([I32Eqz, If(BlockType::Empty)]);
    body.extend([LocalGet(0), RefCastNonNull(HeapType::Concrete(RECORD))]);
    field(&mut body, 0, 3);
    body.extend([
        Call(set),
        Call(join),
        I32Const(1),
        StructSet {
            struct_type_index: RECORD,
            field_index: 6,
        },
        End,
        Return,
        End,
    ]);
    field(&mut body, 0, 9);
    body.extend([I32Const(2), I32Eq]);
    field(&mut body, 0, 9);
    body.extend([I32Const(3), I32Eq, I32Or]);
    field(&mut body, 0, 9);
    body.extend([I32Const(4), I32Eq, I32Or, I32Eqz, If(BlockType::Empty)]);
    fail(&mut body);
    body.push(End);
    // A terminal return is the acknowledgment; do not wait for a second event.
    body.extend([LocalGet(0), LocalGet(1), Call(teardown), End]);
    emit(&mut code, [], body);

    let poll = n + 7;
    let ty = next_type;
    next_type += 1;
    types.ty().function([ValType::I32], [ValType::I32]);
    functions.function(ty);
    let mut body = scan_prefix();
    field(&mut body, 3, 0);
    body.extend([LocalGet(0), I32Eq]);
    field(&mut body, 3, 2);
    body.extend([
        Call(status),
        I32Const(3),
        I32Eq,
        I32And,
        If(BlockType::Empty),
        LocalGet(3),
        LocalGet(1),
        Call(request),
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        End,
    ]);
    scan_suffix(&mut body);
    emit(&mut code, [(2, ValType::I32), (1, VALUE)], body);
    let resume = n + 8;
    let ty = next_type;
    next_type += 1;
    types.ty().function([ValType::I32], [ValType::I32]);
    functions.function(ty);
    let mut body = scan_prefix();
    field(&mut body, 3, 0);
    body.extend([LocalGet(0), I32Eq]);
    field(&mut body, 3, 9);
    body.extend([I32Const(2), I32GeU, I32And]);
    field(&mut body, 3, 9);
    body.extend([
        I32Const(-1),
        I32Ne,
        I32And,
        If(BlockType::Empty),
        I32Const(1),
    ]);
    field(&mut body, 3, 3);
    field(&mut body, 3, 9);
    body.extend([
        Call(complete),
        Drop,
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        End,
    ]);
    scan_suffix(&mut body);
    emit(&mut code, [(2, ValType::I32), (1, VALUE)], body);
    let quarantine = n + 9;
    let ty = next_type;
    types.ty().function([ValType::I32], []);
    functions.function(ty);
    let mut body = scan_prefix();
    field(&mut body, 3, 0);
    body.extend([LocalGet(0), I32Eq, If(BlockType::Empty)]);
    field_set(&mut body, 3, 5, &[I32Const(9)]);
    body.push(End);
    scan_suffix(&mut body);
    body.pop();
    body.pop();
    body.push(End);
    emit(&mut code, [(2, ValType::I32), (1, VALUE)], body);
    let mut exports = ExportSection::new();
    for (name, index) in [
        ("install", install),
        ("complete-event", complete),
        ("pending-invocation", count),
        ("cancel-invocation", cancel),
        ("poll-invocation", poll),
        ("resume-invocation", resume),
        ("quarantine-invocation", quarantine),
    ] {
        exports.export(name, ExportKind::Func, index);
    }
    let mut elements = ElementSection::new();
    elements.declared(Elements::Functions(Cow::Owned(vec![invoke])));
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&globals)
        .section(&exports)
        .section(&elements)
        .section(&code);
    let bytes = module.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .map_err(|e| diagnostic(format!("Invalid generated async-import bridge: {e}")))?;
    Ok(bytes)
}
fn scalar(resolve: &Resolve, mut ty: Type) -> Result<Type, Diagnostic> {
    for _ in 0..64 {
        match ty {
            Type::Id(id) => match resolve.types[id].kind {
                wit_parser::TypeDefKind::Type(next) => ty = next,
                _ => return Err(diagnostic("Async scalar bridge requires a scalar type")),
            },
            _ => return Ok(ty),
        }
    }
    Err(diagnostic("Cyclic scalar alias in async bridge"))
}
fn emit<const N: usize>(
    code: &mut CodeSection,
    locals: [(u32, ValType); N],
    body: Vec<Instruction<'static>>,
) {
    let mut f = Function::new(locals);
    for i in body {
        f.instruction(&i);
    }
    code.function(&f);
}
fn root(body: &mut Vec<Instruction<'static>>) {
    body.extend([
        Instruction::GlobalGet(ROOTS),
        Instruction::RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
    ]);
}
fn field(body: &mut Vec<Instruction<'static>>, local: u32, index: i32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(RECORD)),
        Instruction::StructGet {
            struct_type_index: RECORD,
            field_index: index as u32,
        },
    ]);
}
fn field_set(
    body: &mut Vec<Instruction<'static>>,
    local: u32,
    index: i32,
    value: &[Instruction<'static>],
) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(RECORD)),
    ]);
    body.extend_from_slice(value);
    body.push(Instruction::StructSet {
        struct_type_index: RECORD,
        field_index: index as u32,
    });
}
fn fail(body: &mut Vec<Instruction<'static>>) {
    body.extend([
        Instruction::I32Const(0),
        Instruction::RefI31,
        Instruction::Throw(0),
    ]);
}
fn mem() -> MemArg {
    MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    }
}

/// Single-slot canonical task-context registry. Pinned Wasmtime49 permits slot0
/// without component threading; waitable ownership stays in rooted guest data.
/// Instantiate before the import bridge. Driver registers (key,set) at entry and
/// retires key only after all operation acknowledgments and set teardown.
pub fn context_module() -> Vec<u8> {
    use Instruction::*;
    let mut types = runtime_abi::prelude();
    types.ty().struct_(
        [FieldType {
            element_type: StorageType::Val(ValType::I32),
            mutable: false,
        }; 2],
    );
    types.ty().function([], [ValType::I32]);
    types.ty().function([ValType::I32; 2], []);
    types.ty().function([ValType::I32], []);
    let mut imports = ImportSection::new();
    imports.import(
        "suss.canonical",
        "context",
        EntityType::Function(runtime_abi::TYPE_COUNT + 1),
    );
    let mut functions = FunctionSection::new();
    functions
        .function(runtime_abi::TYPE_COUNT + 2)
        .function(runtime_abi::TYPE_COUNT + 1)
        .function(runtime_abi::TYPE_COUNT + 3);
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([I32Const(CAPACITY), ArrayNewDefault(runtime_abi::ARGS)]),
    );
    let mut code = CodeSection::new();
    let registry = |body: &mut Vec<Instruction<'static>>| {
        body.extend([
            GlobalGet(0),
            RefCastNonNull(HeapType::Concrete(runtime_abi::ARGS)),
        ])
    };
    let mut body = vec![
        LocalGet(0),
        I32Const(0),
        I32Eq,
        If(BlockType::Empty),
        Unreachable,
        End,
        I32Const(-1),
        LocalSet(3),
        I32Const(0),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Const(CAPACITY),
        I32GeU,
        BrIf(1),
    ];
    registry(&mut body);
    body.extend([
        LocalGet(2),
        ArrayGet(runtime_abi::ARGS),
        LocalTee(4),
        RefIsNull,
        If(BlockType::Empty),
        LocalGet(2),
        LocalSet(3),
        Else,
    ]);
    context_field(&mut body, 4, 0);
    body.extend([
        LocalGet(0),
        I32Eq,
        If(BlockType::Empty),
        Unreachable,
        End,
        End,
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        Br(0),
        End,
        End,
        LocalGet(3),
        I32Const(-1),
        I32Eq,
        If(BlockType::Empty),
        Unreachable,
        End,
    ]);
    registry(&mut body);
    body.extend([
        LocalGet(3),
        LocalGet(0),
        LocalGet(1),
        StructNew(RECORD),
        ArraySet(runtime_abi::ARGS),
        End,
    ]);
    emit(&mut code, [(2, ValType::I32), (1, VALUE)], body);
    let mut body = vec![
        Call(0),
        LocalSet(1),
        I32Const(0),
        LocalSet(0),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(0),
        I32Const(CAPACITY),
        I32GeU,
        BrIf(1),
    ];
    registry(&mut body);
    body.extend([
        LocalGet(0),
        ArrayGet(runtime_abi::ARGS),
        LocalTee(2),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    context_field(&mut body, 2, 0);
    body.extend([LocalGet(1), I32Eq, If(BlockType::Empty)]);
    context_field(&mut body, 2, 1);
    body.extend([
        Return,
        End,
        End,
        LocalGet(0),
        I32Const(1),
        I32Add,
        LocalSet(0),
        Br(0),
        End,
        End,
        Unreachable,
        End,
    ]);
    emit(&mut code, [(2, ValType::I32), (1, VALUE)], body);
    let mut body = vec![
        I32Const(0),
        LocalSet(1),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
        I32Const(CAPACITY),
        I32GeU,
        BrIf(1),
    ];
    registry(&mut body);
    body.extend([
        LocalGet(1),
        ArrayGet(runtime_abi::ARGS),
        LocalTee(2),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    context_field(&mut body, 2, 0);
    body.extend([LocalGet(0), I32Eq, If(BlockType::Empty)]);
    registry(&mut body);
    body.extend([
        LocalGet(1),
        RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }),
        ArraySet(runtime_abi::ARGS),
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
        Unreachable,
        End,
    ]);
    emit(&mut code, [(1, ValType::I32), (1, VALUE)], body);
    let mut exports = ExportSection::new();
    exports
        .export("register", ExportKind::Func, 1)
        .export("waitable-set", ExportKind::Func, 2)
        .export("retire", ExportKind::Func, 3);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&globals)
        .section(&exports)
        .section(&code);
    module.finish()
}

fn context_field(body: &mut Vec<Instruction<'static>>, local: u32, index: i32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(RECORD)),
        Instruction::StructGet {
            struct_type_index: RECORD,
            field_index: index as u32,
        },
    ]);
}

fn scan_prefix() -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut body = vec![
        I32Const(0),
        LocalSet(1),
        I32Const(0),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(1),
        I32Const(CAPACITY),
        I32GeU,
        BrIf(1),
    ];
    root(&mut body);
    body.extend([
        LocalGet(1),
        ArrayGet(runtime_abi::ARGS),
        LocalTee(3),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    body
}
fn scan_suffix(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    body.extend([
        End,
        LocalGet(1),
        I32Const(1),
        I32Add,
        LocalSet(1),
        Br(0),
        End,
        End,
        LocalGet(2),
        End,
    ]);
}
fn progress_call(
    body: &mut Vec<Instruction<'static>>,
    record: u32,
    index: u32,
    value: i32,
    action: &[Instruction<'static>],
) {
    use Instruction::*;
    // Keep a validated record reference on the operand stack across the action.
    body.extend([LocalGet(record), RefCastNonNull(HeapType::Concrete(RECORD))]);
    body.extend_from_slice(action);
    body.extend([
        I32Const(value),
        StructSet {
            struct_type_index: RECORD,
            field_index: index,
        },
    ]);
}
