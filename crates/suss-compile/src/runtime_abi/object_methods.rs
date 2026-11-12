//! Original GC-owned Object methods. Keys and unbound functions live on the
//! class descriptor; no registry retains old classes or instance owners.
use super::*;
pub(super) const TAG_GLOBAL: u32 = arrays::TAG_GLOBAL + 1;
pub(super) const DEFAULT_THIS: u32 = TAG_GLOBAL + 1;
pub(super) const CURRENT_THIS: u32 = owned_prototypes::LAST_GLOBAL + 1;
fn guard(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32) {
    use Instruction::*;
    body.extend([
        LocalGet(local),
        RefTestNonNull(HeapType::Concrete(ty)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(body);
    body.push(End);
}
fn get(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32, field: u32) {
    use Instruction::*;
    body.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(ty)),
        StructGet {
            struct_type_index: ty,
            field_index: field,
        },
    ]);
}
fn array(body: &mut Vec<Instruction<'static>>, local: u32) {
    use Instruction::*;
    body.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(ARGS))]);
}
fn callback(b: &mut Builder, instructions: &[Instruction<'static>]) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut body = Function::new([]);
    for instruction in instructions {
        body.instruction(instruction);
    }
    body.instruction(&Instruction::End);
    b.code.function(&body);
    b.count += 1;
    index
}
pub(super) fn functions(b: &mut Builder, equal: u32) -> Vec<u32> {
    use Instruction::*;
    b.function("current-this", &[], &[VALUE], &[GlobalGet(CURRENT_THIS)]);
    // Validate the entire descriptor table even when an early name matches.
    let mut body = vec![];
    guard(&mut body, 0, DESCRIPTOR);
    guard(&mut body, 1, STRING);
    get(&mut body, 0, DESCRIPTOR, 2);
    body.push(LocalSet(2));
    guard(&mut body, 2, ARGS);
    // No callback occurs during name lookup. Retain the checked table and its
    // immutable array length for this invocation; later calls reload live data.
    array(&mut body, 2);
    body.extend([LocalTee(8), ArrayLen, LocalTee(7), I32Const(1), I32And, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.extend([
        End,
        I32Const(0),
        LocalSet(3),
        sentinel_value(NIL),
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
    ]);
    body.extend([LocalGet(7), I32GeU, BrIf(1), LocalGet(8)]);
    body.extend([LocalGet(3), ArrayGet(ARGS), LocalSet(4)]);
    guard(&mut body, 4, DESCRIPTOR);
    get(&mut body, 4, DESCRIPTOR, 3);
    body.extend([GlobalGet(TAG_GLOBAL), RefEq, If(BlockType::Empty)]);
    get(&mut body, 4, DESCRIPTOR, 1);
    body.push(LocalSet(6));
    guard(&mut body, 6, ARGS);
    array(&mut body, 6);
    body.extend([ArrayLen, I32Const(1), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    array(&mut body, 6);
    body.extend([I32Const(0), ArrayGet(ARGS), LocalSet(6)]);
    guard(&mut body, 6, STRING);
    body.extend([
        LocalGet(6),
        LocalGet(1),
        Call(equal),
        If(BlockType::Empty),
        LocalGet(4),
        LocalSet(5),
        End,
        End,
        LocalGet(3),
        I32Const(2),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        LocalGet(5),
    ]);
    let find = b.function_with_locals(
        "object-method-key",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32), (3, VALUE), (1, ValType::I32), (1, reference(ARGS))],
        &body,
    );

    // args are private invocation buffers; prepend the anchored physical this.
    let mut body = vec![];
    guard(&mut body, 0, 4);
    guard(&mut body, 2, ARGS);
    body.extend([
        LocalGet(0),
        Call(b.names["closure-environment"]),
        LocalSet(3),
    ]);
    body.extend([
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(7)),
        If(BlockType::Empty),
    ]);
    get(&mut body, 3, 7, 0);
    body.extend([GlobalGet(TAG_GLOBAL), RefEq, If(BlockType::Empty)]);
    get(&mut body, 3, 7, 1);
    body.push(LocalSet(3));
    guard(&mut body, 3, ARGS);
    array(&mut body, 3);
    body.extend([ArrayLen, I32Const(1), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    array(&mut body, 2);
    body.extend([
        ArrayLen,
        LocalTee(4),
        I32Const(arrays::MAX_LENGTH - 1),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        ArrayNewDefault(ARGS),
        LocalSet(5),
    ]);
    array(&mut body, 5);
    body.extend([I32Const(0), LocalGet(1), ArraySet(ARGS)]);
    array(&mut body, 5);
    body.push(I32Const(1));
    array(&mut body, 2);
    body.extend([
        I32Const(0),
        LocalGet(4),
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
    ]);
    array(&mut body, 3);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(5),
        Call(b.names["js-method-invoke"]),
        Return,
        End,
        End,
        LocalGet(0),
        LocalGet(2),
        Call(b.names["js-method-invoke"]),
    ]);
    let dispatch = b.function_with_locals(
        "object-method-invoke-dispatch",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32), (1, VALUE)],
        &body,
    );
    // Source accessors and object methods run with their actual JavaScript
    // receiver. Save and restore it through the language-exception path too,
    // so a throwing callback cannot leak a stale receiver into a later call.
    let body = vec![
        GlobalGet(CURRENT_THIS),
        LocalSet(3),
        LocalGet(1),
        GlobalSet(CURRENT_THIS),
        Block(BlockType::Empty),
        Block(BlockType::Result(VALUE)),
        TryTable(
            BlockType::Empty,
            Cow::Owned(vec![wasm_encoder::Catch::One { tag: 0, label: 0 }]),
        ),
        LocalGet(0),
        LocalGet(1),
        LocalGet(2),
        Call(dispatch),
        LocalSet(4),
        Br(2),
        End,
        Unreachable,
        End,
        LocalSet(5),
        LocalGet(3),
        GlobalSet(CURRENT_THIS),
        LocalGet(5),
        Throw(0),
        End,
        LocalGet(3),
        GlobalSet(CURRENT_THIS),
        LocalGet(4),
    ];
    let invoke = b.function_with_locals(
        "object-method-invoke",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(3, VALUE)],
        &body,
    );
    // `object-method-invoke-N` (#378): `(.m this a1 … aN)` with no argument
    // arrays. When the method is an Object wrapper whose function takes N + 1
    // operands, it calls that function's typed entry with this as the first,
    // as object-method-invoke-dispatch and js-method-invoke would, with this
    // as CURRENT_THIS and the same restore on a throw. Anything else takes
    // object-method-invoke with the operands in an array.
    for n in 0..MAX_FIXED {
        let k = n + 1;
        // Locals: environment, inner function, saved this, result, payload,
        // then the inner function cast once to a closure. Each cast to the
        // non-final closure type is an out-of-line subtype check in
        // wasmtime 49, so the wrapper is tested as the universal closure it
        // is (an exact match, inline) and the function is cast once.
        let (env, inner, saved, result, payload, typed) = (n + 2, n + 3, n + 4, n + 5, n + 6, n + 7);
        let mut body = vec![Block(BlockType::Empty)];
        body.extend([LocalGet(0), RefTestNonNull(HeapType::Concrete(UNIVERSAL)), I32Eqz, BrIf(0)]);
        body.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(UNIVERSAL)),
            StructGet { struct_type_index: UNIVERSAL, field_index: UNIVERSAL_ENVIRONMENT },
            LocalTee(env),
            RefTestNonNull(HeapType::Concrete(7)),
            I32Eqz,
            BrIf(0),
        ]);
        get(&mut body, env, 7, 0);
        body.extend([GlobalGet(TAG_GLOBAL), RefEq, I32Eqz, BrIf(0)]);
        get(&mut body, env, 7, 1);
        body.extend([LocalTee(inner), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz, BrIf(0)]);
        array(&mut body, inner);
        body.extend([ArrayLen, I32Const(1), I32Ne, BrIf(0)]);
        array(&mut body, inner);
        body.extend([
            I32Const(0),
            ArrayGet(ARGS),
            LocalSet(inner),
            Block(BlockType::Result(VALUE)),
            LocalGet(inner),
            BrOnCastFail {
                relative_depth: 0,
                from_ref_type: RefType::EQREF,
                to_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(CLOSURE) },
            },
            LocalSet(typed),
        ]);
        let code = |body: &mut Vec<Instruction<'static>>, field: u32| {
            body.extend([
                LocalGet(typed),
                StructGet { struct_type_index: CLOSURE, field_index: CLOSURE_CODE },
                StructGet { struct_type_index: CODE, field_index: field },
            ]);
        };
        // A universal closure keeps its arity range outside its code; every
        // one shares the universal code struct.
        body.extend([
            LocalGet(typed),
            StructGet { struct_type_index: CLOSURE, field_index: CLOSURE_CODE },
            GlobalGet(UNIVERSAL_CODE_GLOBAL),
            RefEq,
            BrIf(1),
        ]);
        code(&mut body, CODE_MIN);
        body.extend([I32Const(k as i32), I32GtS, BrIf(1)]);
        code(&mut body, CODE_MAX);
        body.extend([I32Const(k as i32), I32LtS, BrIf(1)]);
        body.extend([
            GlobalGet(CURRENT_THIS),
            LocalSet(saved),
            LocalGet(1),
            GlobalSet(CURRENT_THIS),
            Block(BlockType::Empty),
            Block(BlockType::Result(VALUE)),
            TryTable(BlockType::Empty, Cow::Owned(vec![wasm_encoder::Catch::One { tag: 0, label: 0 }])),
            LocalGet(typed),
            RefAsNonNull,
        ]);
        for operand in 1..=k {
            body.push(LocalGet(operand));
        }
        code(&mut body, CODE_ENTRY + k);
        body.extend([
            CallRef(CALL + k),
            LocalSet(result),
            Br(2),
            End,
            Unreachable,
            End,
            LocalSet(payload),
            LocalGet(saved),
            GlobalSet(CURRENT_THIS),
            LocalGet(payload),
            Throw(0),
            End,
            LocalGet(saved),
            GlobalSet(CURRENT_THIS),
            LocalGet(result),
            Return,
            End,
            Drop,
            End,
            LocalGet(0),
            LocalGet(1),
        ]);
        for operand in 2..n + 2 {
            body.push(LocalGet(operand));
        }
        body.extend([ArrayNewFixed { array_type_index: ARGS, array_size: n }, Call(invoke)]);
        let name: &'static str = Box::leak(format!("object-method-invoke-{n}").into_boxed_str());
        let closure = ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(CLOSURE) });
        b.function_with_locals(name, &vec![VALUE; n as usize + 2], &[VALUE], &[(5, VALUE), (1, closure)], &body);
    }
    // A plain detached call uses the non-strict portable realm's implicit this.
    // Rebuild the wrapper from its already unwrapped callback environment.
    let detached_index = b.count;
    // This callback belongs only to tagged Object wrappers. Foreign closures
    // can copy a typed function reference, so validate its private environment
    // before reconstruction; a plain-function fallback would invoke itself.
    let mut detached_body = vec![];
    guard(&mut detached_body, 0, 7);
    get(&mut detached_body, 0, 7, 0);
    detached_body.extend([GlobalGet(TAG_GLOBAL), RefEq, I32Eqz, If(BlockType::Empty)]);
    nominal::error(&mut detached_body);
    detached_body.push(End);
    detached_body.extend([
        LocalGet(0),
        RefFunc(detached_index),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
        GlobalGet(DEFAULT_THIS),
        LocalGet(1),
        Call(invoke),
    ]);
    let detached = callback(b, &detached_body);

    let mut body = vec![
        LocalGet(0),
        Call(b.names["constructor-descriptor"]),
        LocalSet(3),
    ];
    guard(&mut body, 1, STRING);
    guard(&mut body, 2, 4);
    body.extend([
        LocalGet(3),
        LocalGet(1),
        Call(find),
        LocalSet(4),
        LocalGet(4),
        sentinel_value(NIL),
        RefEq,
        If(BlockType::Empty),
        I64Const(0),
        LocalGet(1),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        I32Const(0),
        ArrayNewDefault(ARGS),
        GlobalGet(TAG_GLOBAL),
        I32Const(0), RefI31, sentinel_value(NIL), StructNew(DESCRIPTOR), // Identity UID (unassigned), mutable names.
        LocalSet(4),
        End,
        GlobalGet(TAG_GLOBAL),
        LocalGet(2),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        sentinel_value(NIL),
        I32Const(0), RefI31, StructNew(7), // Identity UID, unassigned.
        RefFunc(detached),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
        LocalSet(5),
        LocalGet(3),
        LocalGet(4),
        LocalGet(5),
        Call(b.names["protocol-method-set"]),
        LocalGet(5),
    ]);
    b.function_with_locals(
        "object-method-set",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(3, VALUE)],
        &body,
    );
    vec![detached]
}
