//! Original sequence-aware apply machinery (ADR-0004, #466).
//!
//! Core owns sequence protocols. It installs its private sequence operations
//! once per runtime instance; the runtime roots their closures and typed
//! entries, as ordinary compiled call sites do. No arity property is read.
use super::*;

const BASE: u32 = OBJECT_CALL_CACHE_GLOBAL + MAX_FIXED * OBJECT_CALL_STRIDE;
const ARITIES: [u32; 9] = [1, 1, 1, 2, 1, 2, 1, 2, 3];
const NAMES: [&str; 9] = [
    "apply-seq", "apply-first", "apply-next", "apply-cons", "apply-ifn?", "apply-object",
    "apply-count", "apply-nth", "apply-fold",
];
const CACHED_CLOSURE: u32 = BASE + 2 * ARITIES.len() as u32;
const CACHED_SPREAD: u32 = CACHED_CLOSURE + 1;
pub(super) const EMPTY_ARGUMENTS: u32 = CACHED_CLOSURE + 2;
pub(super) const GLOBAL_END: u32 = CACHED_CLOSURE + 3;
pub(super) const FAMILIES: [&str; 13] = [
    "bit-and", "bit-or", "bit-xor", "bit-and-not",
    "comparison-less", "comparison-less-equal", "comparison-greater", "comparison-greater-equal",
    "comparison-equal", "comparison-strict-equal", "array-get", "array-set", "array-make",
];
pub(super) fn native_code(name: &str) -> u32 {
    GLOBAL_END + FAMILIES.iter().position(|family| *family == name).expect("native apply family") as u32
}

fn owner(slot: usize) -> u32 { BASE + 2 * slot as u32 }
fn entry(slot: usize) -> u32 { owner(slot) + 1 }
fn nullable(ty: u32) -> ValType {
    ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(ty) })
}
fn cast_args(local: u32) -> [Instruction<'static>; 2] {
    [Instruction::LocalGet(local), Instruction::RefCastNonNull(HeapType::Concrete(ARGS))]
}
fn fail_if(code: &mut Vec<Instruction<'static>>) {
    code.push(Instruction::If(BlockType::Empty));
    nominal::error(code);
    code.push(Instruction::End);
}

pub(super) fn append_globals(globals: &mut GlobalSection, b: &Builder, universal_entries: &[u32]) {
    assert_eq!(globals.len(), BASE);
    for (slot, arity) in ARITIES.into_iter().enumerate() {
        globals.global(GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::extended([sentinel_value(NIL)]));
        globals.global(GlobalType { val_type: reference(CALL + arity), mutable: true, shared: false },
            &ConstExpr::ref_func(b.names[CALL_FALLBACK_EXPORTS[arity as usize]]));
        assert_eq!(globals.len(), entry(slot) + 1);
    }
    for ty in [CLOSURE, SPREAD] {
        globals.global(GlobalType { val_type: nullable(ty), mutable: true, shared: false },
            &ConstExpr::ref_null(HeapType::Concrete(ty)));
    }
    // This empty internal array never escapes as a source array or rest seq.
    globals.global(GlobalType { val_type: reference(ARGS), mutable: false, shared: false },
        &ConstExpr::extended([Instruction::ArrayNewFixed { array_type_index: ARGS, array_size: 0 }]));
    assert_eq!(globals.len(), GLOBAL_END);
    for name in FAMILIES {
        let mut code = vec![sentinel_value(NIL)];
        code.extend(universal_entries[..=MAX_FIXED as usize].iter().copied().map(Instruction::RefFunc));
        code.extend([Instruction::RefFunc(b.names[&format!("apply-{name}-spread")]),
            Instruction::I32Const(0), Instruction::I32Const(-1), Instruction::StructNew(CODE)]);
        globals.global(GlobalType { val_type: reference(CODE), mutable: false, shared: false },
            &ConstExpr::extended(code));
    }
}

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    // Stage every reference before publishing any of the callback set. There
    // are no user calls in this callback-free admission/publication interval.
    let mut code = vec![];
    for (slot, arity) in ARITIES.into_iter().enumerate() {
        code.extend([LocalGet(slot as u32), RefTestNonNull(HeapType::Concrete(CLOSURE)), I32Eqz]);
        fail_if(&mut code);
        code.extend([LocalGet(slot as u32), Call(b.names[CALL_ENTRY_EXPORTS[arity as usize]]), LocalSet(ARITIES.len() as u32 + slot as u32)]);
    }
    for slot in 0..ARITIES.len() {
        code.extend([LocalGet(slot as u32), GlobalSet(owner(slot)),
            LocalGet(ARITIES.len() as u32 + slot as u32), RefAsNonNull, GlobalSet(entry(slot))]);
    }
    code.push(sentinel_value(NIL));
    let locals = ARITIES.iter().map(|arity| (1, nullable(CALL + arity))).collect::<Vec<_>>();
    b.function_with_locals("apply-sequence-ops", &[VALUE; ARITIES.len()], &[VALUE], &locals, &code);
    for (slot, arity) in ARITIES.into_iter().enumerate() {
        let mut code = vec![GlobalGet(owner(slot))];
        code.extend((0..arity).map(LocalGet));
        code.extend([GlobalGet(entry(slot)), ReturnCallRef(CALL + arity)]);
        b.function(NAMES[slot], &vec![VALUE; arity as usize], &[VALUE], &code);
    }

    // A spread entry receives an already normalized rest. Joining a realized
    // prefix must not call seq again on that rest, especially on a custom ISeq.
    let mut code = vec![LocalGet(0), ArrayLen, LocalSet(2), LocalGet(1), LocalSet(3),
        Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(2), I32Eqz, BrIf(1), LocalGet(2), I32Const(1), I32Sub, LocalSet(2),
        LocalGet(0), LocalGet(2), ArrayGet(ARGS), LocalGet(3), Call(b.names["apply-cons"]), LocalSet(3),
        Br(0), End, End, LocalGet(3)];
    b.function_with_locals("apply-join", &[reference(ARGS), VALUE], &[VALUE],
        &[(1, ValType::I32), (1, VALUE)], &code);
    // Public apply normalizes once when no prefix is present. A nonempty
    // prefix follows list*: cons normalizes a non-seq tail, as core specifies.
    code = vec![LocalGet(0), ArrayLen, I32Eqz, If(BlockType::Empty),
        LocalGet(1), ReturnCall(b.names["apply-seq"]), End,
        LocalGet(0), LocalGet(1), ReturnCall(b.names["apply-join"])];
    b.function("apply-arguments", &[reference(ARGS), VALUE], &[VALUE], &code);

    // RT.boundedLength's next-only preflight. Public bounded-count deliberately
    // has a different Counted fast path and is not used here.
    code = vec![I32Const(0), LocalSet(2), LocalGet(0), LocalSet(3),
        Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(3), RefIsNull, BrIf(1), LocalGet(2), LocalGet(1), I32GtS, BrIf(1),
        LocalGet(2), I32Const(1), I32Add, LocalSet(2),
        LocalGet(3), Call(b.names["apply-next"]), LocalSet(3), Br(0), End, End, LocalGet(2)];
    b.function_with_locals("apply-bounded-length", &[VALUE, ValType::I32], &[ValType::I32],
        &[(1, ValType::I32), (1, VALUE)], &code);

    fixed_arguments(b);

    // A closure and its immutable spread entry are cached as typed references.
    // An object callee is checked using core's actual IFn predicate, never an
    // inferred RestFn arity from UNIVERSAL's accepted min/max range.
    code = vec![
        // A JVM checkcast admits nil; the invocation fails after its arguments
        // are evaluated. An unbound function is likewise a callable marker.
        LocalGet(0), RefIsNull, LocalGet(0), sentinel_value(UNDEFINED), RefEq, I32Or,
        If(BlockType::Empty), LocalGet(0), Return, End,
        GlobalGet(CACHED_CLOSURE), LocalGet(0), RefEq,
        GlobalGet(CACHED_SPREAD), RefIsNull, I32Eqz, I32And,
        If(BlockType::Empty), LocalGet(0), Return, End,
        Block(BlockType::Result(VALUE)), LocalGet(0),
        BrOnCastFail { relative_depth: 0, from_ref_type: RefType::EQREF,
            to_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(CLOSURE) } },
        LocalTee(1), StructGet { struct_type_index: CLOSURE, field_index: CLOSURE_CODE },
        StructGet { struct_type_index: CODE, field_index: CODE_SPREAD }, GlobalSet(CACHED_SPREAD),
        LocalGet(1), GlobalSet(CACHED_CLOSURE), LocalGet(0), Return, End, Drop,
        LocalGet(0), Call(b.names["apply-ifn?"]), sentinel_value(TRUE), RefEq,
        If(BlockType::Empty), LocalGet(0), Return, End,
        // invoke's existing unbound/not-callable language errors precede seq.
        LocalGet(0), GlobalGet(EMPTY_ARGUMENTS), ReturnCall(b.names["invoke"])];
    b.function_with_locals("apply-callee", &[VALUE], &[VALUE], &[(1, nullable(CLOSURE))], &code);
    code = vec![LocalGet(0), Call(b.names["apply-callee"]), Drop,
        // nil has no applyTo method. Its seq argument is already normalized,
        // but no AFn traversal occurs before the ordinary language exception.
        LocalGet(0), RefIsNull, If(BlockType::Empty),
        LocalGet(0), GlobalGet(EMPTY_ARGUMENTS), ReturnCall(b.names["invoke"]), End,
        GlobalGet(CACHED_CLOSURE), LocalGet(0), RefEq,
        GlobalGet(CACHED_SPREAD), RefIsNull, I32Eqz, I32And,
        If(BlockType::Empty), GlobalGet(CACHED_CLOSURE), RefAsNonNull,
        GlobalGet(EMPTY_ARGUMENTS), LocalGet(1), GlobalGet(CACHED_SPREAD), RefAsNonNull,
        ReturnCallRef(SPREAD), End,
        LocalGet(0), LocalGet(1), ReturnCall(b.names["apply-object"])];
    b.function("apply-to-seq", &[VALUE, VALUE], &[VALUE], &code);
    code = vec![LocalGet(0), Call(b.names["apply-callee"]), LocalGet(1), LocalGet(2),
        Call(b.names["apply-arguments"]), ReturnCall(b.names["apply-to-seq"])];
    b.function("apply", &[VALUE, reference(ARGS), VALUE], &[VALUE], &code);
    code = vec![LocalGet(0), LocalGet(1), Call(b.names["apply-fixed-arguments"]),
        ReturnCall(b.names["call-spread"])];
    b.function("apply-fixed", &[VALUE, VALUE], &[VALUE], &code);

    // Opaque native families retain their ordinary realized-argument callback.
    // Public RestFn families install their own lazy spread entries separately.
    code = vec![LocalGet(0), LocalGet(2), RefIsNull, If(BlockType::Result(reference(ARGS))),
        LocalGet(1), Else, LocalGet(1), LocalGet(2), Call(b.names["apply-join"]),
        Call(b.names["apply-fixed-arguments"]), RefCastNonNull(HeapType::Concrete(ARGS)), End,
        ReturnCall(b.names["invoke"])];
    let spread = b.typed(SPREAD, &[], &code);
    b.names.insert("universal-spread".into(), spread);
    let mut declared = vec![spread];
    declared.extend(native_entries(b));
    declared
}

fn fixed_arguments(b: &mut Builder) {
    use Instruction::*;
    // Params: 0 normalized seq. Locals: 1 bounded count, 2 output array,
    // 3 current seq, 4 index, 5 remainder root, 6 full length, 7 current value.
    let mut code = vec![LocalGet(0), I32Const(20), Call(b.names["apply-bounded-length"]), LocalSet(1),
        LocalGet(1), I32Const(20), I32LeU, If(BlockType::Empty),
        LocalGet(1), ArrayNewDefault(ARGS), LocalSet(2), LocalGet(0), LocalSet(3),
        I32Const(0), LocalSet(4), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(4), LocalGet(1), I32GeU, BrIf(1)];
    code.extend(cast_args(2));
    code.extend([LocalGet(4), LocalGet(3), Call(b.names["apply-first"]), ArraySet(ARGS),
        LocalGet(4), I32Const(1), I32Add, LocalTee(4), LocalGet(1), I32LtU,
        If(BlockType::Empty), LocalGet(3), Call(b.names["apply-next"]), LocalSet(3), End,
        Br(0), End, End, LocalGet(2), Return, End,
        I32Const(20), ArrayNewDefault(ARGS), LocalSet(2), LocalGet(0), LocalSet(3),
        I32Const(0), LocalSet(4), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(4), I32Const(20), I32GeU, BrIf(1)]);
    code.extend(cast_args(2));
    code.extend([LocalGet(4), LocalGet(3), Call(b.names["apply-first"]), ArraySet(ARGS),
        LocalGet(3), Call(b.names["apply-next"]), LocalSet(3),
        LocalGet(4), I32Const(1), I32Add, LocalSet(4), Br(0), End, End,
        LocalGet(3), LocalSet(5), I32Const(20), LocalSet(6),
        // Count the remainder without reading its first values, as seqToArray.
        Block(BlockType::Empty), Loop(BlockType::Empty), LocalGet(3), RefIsNull, BrIf(1),
        LocalGet(6), I32Const(i32::MAX), I32Eq]);
    fail_if(&mut code);
    code.extend([LocalGet(6), I32Const(1), I32Add, LocalSet(6),
        LocalGet(3), Call(b.names["apply-next"]), LocalSet(3), Br(0), End, End,
        LocalGet(6), I32Const(arrays::MAX_LENGTH), I32GtU]);
    fail_if(&mut code);
    code.extend([LocalGet(6), ArrayNewDefault(ARGS), LocalSet(3)]);
    code.extend(cast_args(3));
    code.push(I32Const(0));
    code.extend(cast_args(2));
    code.extend([I32Const(0), I32Const(20), ArrayCopy { array_type_index_dst: ARGS, array_type_index_src: ARGS },
        LocalGet(3), LocalSet(2), LocalGet(5), LocalSet(3), I32Const(20), LocalSet(4),
        Block(BlockType::Empty), Loop(BlockType::Empty), LocalGet(3), RefIsNull, BrIf(1),
        // Read first before checking storage bounds, matching aastore's order.
        LocalGet(3), Call(b.names["apply-first"]), LocalSet(7),
        LocalGet(4), LocalGet(6), I32GeU]);
    fail_if(&mut code);
    code.extend(cast_args(2));
    code.extend([LocalGet(4), LocalGet(7), ArraySet(ARGS),
        LocalGet(4), I32Const(1), I32Add, LocalSet(4),
        LocalGet(3), Call(b.names["apply-next"]), LocalSet(3), Br(0), End, End, LocalGet(2)]);
    b.function_with_locals("apply-fixed-arguments", &[VALUE], &[VALUE],
        &[(1, ValType::I32), (2, VALUE), (1, ValType::I32), (1, VALUE), (1, ValType::I32), (1, VALUE)], &code);
}

fn native_entries(b: &mut Builder) -> Vec<u32> {
    // Native family bodies are wired independently of source closure spreads.
    // Keep the ordinary callback route while those bodies are installed.
    let mut declared = vec![];
    for name in ["add", "subtract", "multiply", "divide"] {
        declared.push(fold_spread(b, name, false));
    }
    for name in FAMILIES {
        if name.starts_with("bit-") {
            declared.push(fold_spread(b, name, true));
        } else {
            b.names.insert(format!("apply-{name}-spread"), b.names["universal-spread"]);
        }
    }
    declared
}

// Every public numeric RestFn has two required parameters. The preflight
// reads next only; its short case delegates to AFn, while its long case
// extracts two values and leaves the rest lazy. The private source fold
// follows core.clj's reduce1, including chunk protocol dispatch.
fn fold_spread(b: &mut Builder, name: &str, bitwise: bool) -> u32 {
    use Instruction::*;
    // 0 closure, 1 realized prefix, 2 normalized rest; 3 joined seq,
    // 4 first argument, 5 second argument, 6 accumulator, 7 reducer.
    let mut code = vec![LocalGet(2), RefIsNull, If(BlockType::Empty),
        LocalGet(0), LocalGet(1), ReturnCall(b.names["invoke"]), End,
        LocalGet(1), LocalGet(2), Call(b.names["apply-join"]), LocalSet(3),
        LocalGet(3), I32Const(2), Call(b.names["apply-bounded-length"]),
        I32Const(2), I32LeU, If(BlockType::Empty),
        LocalGet(0), LocalGet(3), Call(b.names["apply-fixed-arguments"]),
        RefCastNonNull(HeapType::Concrete(ARGS)), ReturnCall(b.names["invoke"]), End];
    for target in [4, 5] {
        code.extend([LocalGet(3), Call(b.names["apply-first"]), LocalSet(target),
            LocalGet(3), Call(b.names["apply-next"]), LocalSet(3)]);
    }
    if bitwise {
        code.extend([LocalGet(4), LocalGet(5), Call(b.names[&format!("primitive-{name}")]),
            LocalSet(6), LocalGet(0), RefCastNonNull(HeapType::Concrete(UNIVERSAL)),
            StructGet { struct_type_index: UNIVERSAL, field_index: UNIVERSAL_ENVIRONMENT },
            Call(b.names["bitwise-current-reducer"]), LocalSet(7)]);
    } else {
        code.extend([LocalGet(0), LocalGet(4), LocalGet(5),
            Call(b.names[&format!("arithmetic-{name}-2")]), LocalSet(6),
            LocalGet(0), LocalSet(7)]);
    }
    code.extend([LocalGet(7), LocalGet(6), LocalGet(3), ReturnCall(b.names["apply-fold"])]);
    let spread = b.typed(SPREAD, &[(5, VALUE)], &code);
    b.names.insert(format!("apply-{name}-spread"), spread);
    spread
}
