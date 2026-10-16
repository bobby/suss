//! Macro-only actual protocol dispatch observation; no write authority.
//! Private callback global appended after existing globals, with no imports or
//! numeric-helper function relocation. Session installs once before bootstrap.
use super::*;
use Instruction::*;
pub(super) const SETTER: &str = "macro-dispatch-observer-install";
const CALLBACK: u32 = prototype_origins::STATE + 1;
#[derive(Clone, Copy)]
pub(super) struct Hooks {
    ty: u32,
    store_ty: u32,
}
pub(super) fn prepare(b: &mut Builder) {
    let ty = b.next_type;
    b.next_type += 1;
    // Already selected method and already prepared arguments come first.
    b.types.ty().function(
        [VALUE, reference(ARGS), VALUE, VALUE, ValType::I32],
        [VALUE],
    );
    let store_ty = b.next_type;
    b.next_type += 1;
    b.types
        .ty()
        .function([reference(ARGS), ValType::I32, VALUE], []);
    b.dispatch_observer = Some(Hooks { ty, store_ty });
}
pub(super) fn emit(
    b: &Builder,
    body: &mut Vec<Instruction<'static>>,
    environment: u32,
    receiver: &[Instruction<'static>],
    route: i32,
    original: u32,
) {
    if let Some(hook) = b.dispatch_observer {
        body.push(LocalGet(environment));
        body.extend_from_slice(receiver);
        body.extend([I32Const(route), GlobalGet(CALLBACK), CallRef(hook.ty)]);
    } else {
        body.push(Call(original));
    }
}
pub(super) fn finish(b: &mut Builder, globals: &mut GlobalSection) {
    let Some(hook) = b.dispatch_observer else {
        return;
    };
    assert_eq!(globals.len(), CALLBACK);
    globals.global(
        GlobalType {
            val_type: ValType::Ref(RefType {
                nullable: true,
                heap_type: HeapType::Concrete(hook.ty),
            }),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Concrete(hook.ty)),
    );
    let mut code = vec![GlobalGet(CALLBACK), RefIsNull, I32Eqz, If(BlockType::Empty)];
    nominal::error(&mut code);
    code.extend([End, LocalGet(0), GlobalSet(CALLBACK)]);
    b.function(SETTER, &[reference(hook.ty)], &[], &code);
    globals.global(
        GlobalType {
            val_type: ValType::Ref(RefType {
                nullable: true,
                heap_type: HeapType::Concrete(hook.store_ty),
            }),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Concrete(hook.store_ty)),
    );
    let mut code = vec![
        GlobalGet(CALLBACK + 1),
        RefIsNull,
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.extend([End, LocalGet(0), GlobalSet(CALLBACK + 1)]);
    b.function(
        "macro-memo-store-install",
        &[reference(hook.store_ty)],
        &[],
        &code,
    );
    b.function(
        "macro-memo-array-store",
        &[reference(ARGS), ValType::I32, VALUE],
        &[],
        &[LocalGet(0), LocalGet(1), LocalGet(2), ArraySet(ARGS)],
    );
}

// Operand preparation, casts and guards stay at their original sites.
pub(super) fn store(b: &Builder, body: &mut Vec<Instruction<'static>>) {
    if let Some(hook) = b.dispatch_observer {
        body.extend([GlobalGet(CALLBACK + 1), CallRef(hook.store_ty)]);
    } else {
        body.push(ArraySet(ARGS));
    }
}
