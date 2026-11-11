//! Original closure-owned native dispatch storage, using the existing shared ABI.
//! No global registry retains replaced closures or their implementations.
use super::*;
pub(super) const TAG_GLOBAL: u32 = exception_info::ORDINARY_ROOT + 1;
/// Keyed properties: native kinds 0..7, then keys 8 and 9.
const KEYS: i32 = 10;

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
fn field(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32, index: u32) {
    use Instruction::*;
    body.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(ty)),
        StructGet {
            struct_type_index: ty,
            field_index: index,
        },
    ]);
}
fn array(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
fn nil(body: &mut Vec<Instruction<'static>>) {
    body.extend([sentinel_value(NIL)]);
}

pub(super) fn functions(b: &mut Builder) -> u32 {
    use Instruction::*;
    // A universal closure's environment; any other closure has none (nil).
    let mut body = vec![];
    guard(&mut body, 0, CLOSURE);
    body.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(UNIVERSAL)),
        If(BlockType::Result(VALUE)),
    ]);
    field(&mut body, 0, UNIVERSAL, UNIVERSAL_ENVIRONMENT);
    body.push(Else);
    nil(&mut body);
    body.push(End);
    let environment = b.function("closure-environment", &[VALUE], &[VALUE], &body);

    // A closure's property storage, `[keyed properties, named table, display name]`,
    // created in its `properties` field on first use (ADR-0004).
    let mut body = vec![];
    guard(&mut body, 0, CLOSURE);
    field(&mut body, 0, CLOSURE, CLOSURE_PROPERTIES);
    body.extend([LocalTee(1), RefIsNull, If(BlockType::Empty)]);
    nil(&mut body);
    body.extend([I32Const(0), ArrayNewDefault(ARGS)]);
    nil(&mut body);
    body.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        LocalSet(1),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(CLOSURE)),
        LocalGet(1),
        StructSet {
            struct_type_index: CLOSURE,
            field_index: CLOSURE_PROPERTIES,
        },
        End,
        LocalGet(1),
    ]);
    let fields = b.function_with_locals(
        "closure-property-fields",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    let mut body = vec![LocalGet(0), Call(fields), LocalSet(1)];
    guard(&mut body, 1, ARGS);
    array(&mut body, 1);
    body.extend([I32Const(2), ArrayGet(ARGS), LocalSet(1)]);
    // An unset kernel name must fail visibly; it is not an anonymous source fn.
    guard(&mut body, 1, super::STRING);
    body.push(LocalGet(1));
    b.function_with_locals(
        "closure-source-name",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    let mut body = vec![LocalGet(0), Call(fields), LocalSet(2)];
    guard(&mut body, 2, ARGS);
    guard(&mut body, 1, super::STRING);
    array(&mut body, 2);
    body.extend([I32Const(2), LocalGet(1), ArraySet(ARGS)]);
    b.function_with_locals(
        "closure-source-name-initialize",
        &[VALUE, VALUE],
        &[],
        &[(1, VALUE)],
        &body,
    );
    for (writing, max_kind, name) in [
        (false, 7, "closure-property-get"),
        (true, 7, "closure-property-set"),
        (false, 9, "closure-internal-property-get"),
        (true, 9, "closure-internal-property-set"),
    ] {
        // Params: function, key, optional assigned value. Keys 0..7 are
        // native kinds; 8 is the private constructor prototype carrier and 9
        // the static initializer, which source string properties can't
        // address. They live in a dense array indexed by key, in the
        // properties' first slot, created on first assignment: a lookup is
        // one read, with no search (#355). Locals: owner, then keys.
        let owner = if writing { 3 } else { 2 };
        let keys = owner + 1;
        let mut body = vec![
            LocalGet(1),
            I32Const(max_kind),
            I32GtU,
            If(BlockType::Empty),
        ];
        nominal::error(&mut body);
        body.extend([End, LocalGet(0), Call(fields), LocalSet(owner)]);
        guard(&mut body, owner, ARGS);
        array(&mut body, owner);
        body.extend([I32Const(0), ArrayGet(ARGS), LocalTee(keys)]);
        body.extend([
            RefTestNonNull(HeapType::Concrete(ARGS)),
            I32Eqz,
            If(BlockType::Empty),
        ]);
        if writing {
            nil(&mut body);
            body.extend([I32Const(KEYS), ArrayNew(ARGS), LocalSet(keys)]);
            array(&mut body, owner);
            body.extend([I32Const(0), LocalGet(keys), ArraySet(ARGS)]);
        } else {
            nil(&mut body);
            body.push(Return);
        }
        body.push(End);
        array(&mut body, keys);
        body.extend([ArrayLen, I32Const(KEYS), I32Ne, If(BlockType::Empty)]);
        nominal::error(&mut body);
        body.push(End);
        array(&mut body, keys);
        body.push(LocalGet(1));
        if writing {
            body.extend([LocalGet(2), ArraySet(ARGS), LocalGet(2)]);
        } else {
            body.push(ArrayGet(ARGS));
        }
        b.function_with_locals(
            name,
            if writing {
                &[VALUE, ValType::I32, VALUE]
            } else {
                &[VALUE, ValType::I32]
            },
            &[VALUE],
            &[(2, VALUE)],
            &body,
        );
    }
    environment
}
