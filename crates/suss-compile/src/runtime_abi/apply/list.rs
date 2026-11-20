//! Ordinary short list calls have direct helpers; apply retains the original
//! zero-required RestFn spread. Owners belong to each wrapper, including old
//! wrappers retained across core reloads. Entry caches are keyed by those owners.
use super::*;

const CODE_GLOBAL: u32 = LIST_DESCRIPTOR + 1;
pub(super) const GLOBAL_END: u32 = CODE_GLOBAL + 9;
fn owner(arity: u32) -> u32 { CODE_GLOBAL + 1 + 2 * arity }
fn entry(arity: u32) -> u32 { owner(arity) + 1 }

pub(super) fn append_globals(globals: &mut GlobalSection, b: &Builder, universal: &[u32]) {
    assert_eq!(globals.len(), CODE_GLOBAL);
    let mut code = vec![sentinel_value(NIL)];
    for k in 0..=MAX_FIXED {
        code.push(Instruction::RefFunc(if k <= 3 { b.names[&format!("list-entry-{k}")] } else { universal[k as usize] }));
    }
    code.extend([Instruction::RefFunc(b.names["list-spread"]), Instruction::I32Const(0),
        Instruction::I32Const(-1), Instruction::StructNew(CODE)]);
    globals.global(GlobalType { val_type: reference(CODE), mutable: false, shared: false }, &ConstExpr::extended(code));
    for k in 0..=3 {
        globals.global(GlobalType { val_type: VALUE, mutable: true, shared: false }, &ConstExpr::extended([sentinel_value(NIL)]));
        globals.global(GlobalType { val_type: reference(CALL + k), mutable: true, shared: false },
            &ConstExpr::ref_func(b.names[CALL_FALLBACK_EXPORTS[k as usize]]));
        assert_eq!(globals.len(), entry(k) + 1);
    }
}

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    let mut code = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(ARGS)), I32Eqz];
    fail_if(&mut code);
    code.extend(cast_args(0));
    code.extend([ArrayLen, I32Const(5), I32LtU]);
    fail_if(&mut code);
    code.extend(cast_args(0));
    b.function("list-captured", &[VALUE], &[reference(ARGS)], &code);

    code = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(UNIVERSAL)), I32Eqz];
    fail_if(&mut code);
    code.extend([LocalGet(0), RefCastNonNull(HeapType::Concrete(UNIVERSAL)),
        StructGet { struct_type_index: UNIVERSAL, field_index: UNIVERSAL_ENVIRONMENT },
        ReturnCall(b.names["list-captured"])]);
    b.function("list-owner-env", &[VALUE], &[reference(ARGS)], &code);

    let mut declared = vec![];
    for k in 0..=3 {
        let helper = k + 1;
        code = vec![LocalGet(0), Call(b.names["list-owner-env"]), I32Const((k + 1) as i32),
            ArrayGet(ARGS), LocalTee(helper), GlobalGet(owner(k)), RefEq, I32Eqz,
            If(BlockType::Empty), LocalGet(helper), Call(b.names[CALL_ENTRY_EXPORTS[k as usize]]),
            GlobalSet(entry(k)), LocalGet(helper), GlobalSet(owner(k)), End,
            LocalGet(helper)];
        code.extend((1..=k).map(LocalGet));
        code.extend([GlobalGet(entry(k)), ReturnCallRef(CALL + k)]);
        let f = b.typed(CALL + k, &[(1, VALUE)], &code);
        b.names.insert(format!("list-entry-{k}"), f);
        declared.push(f);
    }

    // The generic callback receives this wrapper's environment, not its self.
    code = vec![LocalGet(0), Call(b.names["list-captured"]), I32Const(0), ArrayGet(ARGS),
        LocalGet(1), ReturnCall(b.names["call-spread"])];
    let invoke = b.typed(INVOKE, &[], &code);
    declared.push(invoke);

    // Forward the captured original as the callee, so its spread entry sees
    // its own captures and required arity. A normalized rest is not seq'd here.
    code = vec![LocalGet(0), Call(b.names["list-owner-env"]), I32Const(0), ArrayGet(ARGS),
        LocalSet(3), LocalGet(2), RefIsNull, If(BlockType::Empty),
        LocalGet(3), LocalGet(1), ReturnCall(b.names["call-spread"]), End,
        LocalGet(3), LocalGet(1), LocalGet(2), Call(b.names["apply-join"]),
        ReturnCall(b.names["apply-to-seq"])];
    let spread = b.typed(SPREAD, &[(1, VALUE)], &code);
    b.names.insert("list-spread".into(), spread);
    declared.push(spread);

    code = vec![];
    for i in 0..5 {
        code.extend([LocalGet(i), RefTestNonNull(HeapType::Concrete(CLOSURE)), I32Eqz]);
        fail_if(&mut code);
    }
    code.extend([GlobalGet(CODE_GLOBAL), sentinel_value(NIL), I32Const(0), RefI31]);
    code.extend((0..5).map(LocalGet));
    code.extend([ArrayNewFixed { array_type_index: ARGS, array_size: 5 },
        RefFunc(invoke), I32Const(0), I32Const(-1), StructNew(UNIVERSAL)]);
    b.function("list-function", &[VALUE; 5], &[VALUE], &code);
    declared
}
