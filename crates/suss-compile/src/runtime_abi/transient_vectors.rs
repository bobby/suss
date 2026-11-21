//! Guarded storage-only append to the canonical transient vector's current tail.
//! Every unsupported case declines before writes and retains core's source path.
use super::*;
use Instruction::*;

const VECTOR: u32 = chunks::GLOBAL_LIMIT;
const NODE: u32 = VECTOR + 1;
const EDIT: u32 = NODE + 1;

pub(super) fn append_globals(globals: &mut GlobalSection) {
    assert_eq!(globals.len(), VECTOR);
    for _ in 0..3 {
        globals.global(
            GlobalType {
                val_type: VALUE,
                mutable: true,
                shared: false,
            },
            &ConstExpr::extended([sentinel_value(NIL)]),
        );
    }
}

fn decline_if(code: &mut Vec<Instruction<'static>>) {
    code.extend([If(BlockType::Empty), sentinel_value(UNDEFINED), Return, End]);
}

fn cast(ty: HeapType) -> Instruction<'static> {
    BrOnCastFail {
        relative_depth: 0,
        from_ref_type: RefType::EQREF,
        to_ref_type: RefType {
            nullable: false,
            heap_type: ty,
        },
    }
}

fn nullable(ty: u32) -> ValType {
    ValType::Ref(RefType {
        nullable: true,
        heap_type: HeapType::Concrete(ty),
    })
}

fn field(fields: u32, slot: i32) -> Vec<Instruction<'static>> {
    vec![
        LocalGet(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(slot),
        ArrayGet(ARGS),
    ]
}

// Local5 is scratch. Leave the checked fields in `destination`. The descriptor
// is nominal identity; structural OBJECT compatibility alone cannot admit it.
fn fields(
    code: &mut Vec<Instruction<'static>>,
    value: Vec<Instruction<'static>>,
    descriptor: u32,
    count: i32,
    destination: u32,
) {
    code.extend(value);
    code.extend([
        cast(HeapType::Concrete(OBJECT)),
        LocalTee(5),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 0,
        },
        GlobalGet(descriptor),
        RefEq,
        I32Eqz,
    ]);
    decline_if(code);
    code.extend([
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(OBJECT)),
        StructGet {
            struct_type_index: OBJECT,
            field_index: 1,
        },
        LocalTee(destination),
        ArrayLen,
        I32Const(count),
        I32Ne,
    ]);
    decline_if(code);
}

pub(super) fn functions(b: &mut Builder) {
    // All three witnesses come from the owned loader. Stage every descriptor
    // before publication, so an invalid last witness leaves the registry intact.
    let mut code = vec![];
    for i in 0..3 {
        code.extend([
            LocalGet(i),
            RefCastNonNull(HeapType::Concrete(OBJECT)),
            StructGet {
                struct_type_index: OBJECT,
                field_index: 0,
            },
            LocalSet(i + 3),
        ]);
    }
    code.extend([
        LocalGet(3),
        GlobalSet(VECTOR),
        LocalGet(4),
        GlobalSet(NODE),
        LocalGet(5),
        GlobalSet(EDIT),
        sentinel_value(NIL),
    ]);
    b.function_with_locals(
        "transient-vector-types-register",
        &[VALUE; 3],
        &[VALUE],
        &[(3, VALUE)],
        &code,
    );

    // Params0/1: vector and already evaluated value. Locals2/3: vector/root
    // fields,4: tail storage,5: temporary object,6: count. No user code or
    // allocation intervenes between the admission checks and the two stores.
    code = vec![Block(BlockType::Result(VALUE))];
    fields(&mut code, vec![LocalGet(0)], VECTOR, 4, 2);
    fields(&mut code, field(2, 2), NODE, 2, 3);
    fields(&mut code, field(3, 0), EDIT, 0, 3);
    code.extend(field(2, 0));
    code.extend([
        cast(HeapType::I31),
        I31GetS,
        LocalTee(6),
        I32Const(0),
        I32LtS,
        LocalGet(6),
        I32Const(0x3fff_ffff),
        I32GeU,
        I32Or,
    ]);
    decline_if(&mut code);
    // Zero starts with an empty tail. Positive multiples of32 have a full
    // tail and need the unchanged trie-promotion/root-growth implementation.
    code.extend([
        LocalGet(6),
        I32Eqz,
        I32Eqz,
        LocalGet(6),
        I32Const(31),
        I32And,
        I32Eqz,
        I32And,
    ]);
    decline_if(&mut code);
    code.extend(field(2, 3));
    code.extend([
        cast(HeapType::Concrete(PACKED)),
        StructGet {
            struct_type_index: PACKED,
            field_index: 0,
        },
        cast(HeapType::Concrete(ARGS)),
        LocalTee(4),
        ArrayLen,
        I32Const(32),
        I32Ne,
    ]);
    decline_if(&mut code);

    // Source order: store the element, then increment count, then return the
    // original transient. The admitted i31 range cannot overflow either store.
    code.extend([
        LocalGet(4),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(6),
        I32Const(31),
        I32And,
        LocalGet(1),
        ArraySet(ARGS),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        LocalGet(6),
        I32Const(1),
        I32Add,
        RefI31,
        ArraySet(ARGS),
        LocalGet(0),
        Return,
        End,
        Drop,
        sentinel_value(UNDEFINED),
    ]);
    b.function_with_locals(
        "transient-vector-append",
        &[VALUE; 2],
        &[VALUE],
        &[
            (3, nullable(ARGS)),
            (1, nullable(OBJECT)),
            (1, ValType::I32),
        ],
        &code,
    );
}
