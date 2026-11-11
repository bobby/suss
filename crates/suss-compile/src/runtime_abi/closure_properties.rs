//! Original closure-owned native dispatch storage, using the existing shared ABI.
//! No global registry retains replaced closures or their implementations.
use super::*;
pub(super) const TAG_GLOBAL: u32 = exception_info::ORDINARY_ROOT + 1;

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

    // A closure's property storage, `[unused, native table, display name]`,
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
        // Params: function, native kind, optional assigned value. Locals follow.
        let owner = if writing { 3 } else { 2 };
        let table = owner + 1;
        let index = owner + 2;
        let fresh = owner + 3;
        // Kind keys0..7 and the private constructor prototype carrier at8 and static initializer at9.
        // Source string properties cannot address this integer-keyed carrier.
        let mut body = vec![LocalGet(1), I32Const(max_kind), I32GtU, If(BlockType::Empty)];
        nominal::error(&mut body);
        body.extend([
            End,
            LocalGet(0),
            Call(fields),
            LocalSet(owner),
            LocalGet(owner),
        ]);
        nil(&mut body);
        body.extend([RefEq, If(BlockType::Empty)]);
        if writing {
            nominal::error(&mut body);
        } else {
            nil(&mut body);
            body.push(Return);
        }
        body.push(End);
        array(&mut body, owner);
        body.extend([I32Const(1), ArrayGet(ARGS), LocalSet(table)]);
        guard(&mut body, table, ARGS);
        array(&mut body, table);
        body.extend([ArrayLen, I32Const(1), I32And, If(BlockType::Empty)]);
        nominal::error(&mut body);
        body.extend([
            End,
            I32Const(0),
            LocalSet(index),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(index),
        ]);
        array(&mut body, table);
        body.extend([ArrayLen, I32GeU, BrIf(1)]);
        array(&mut body, table);
        body.extend([
            LocalGet(index),
            ArrayGet(ARGS),
            LocalGet(1),
            RefI31,
            RefEq,
            If(BlockType::Empty),
        ]);
        array(&mut body, table);
        body.extend([LocalGet(index), I32Const(1), I32Add]);
        if writing {
            body.extend([LocalGet(2), ArraySet(ARGS), LocalGet(2), Return]);
        } else {
            body.extend([ArrayGet(ARGS), Return]);
        }
        body.extend([
            End,
            LocalGet(index),
            I32Const(2),
            I32Add,
            LocalSet(index),
            Br(0),
            End,
            End,
        ]);
        if writing {
            array(&mut body, table);
            body.extend([
                ArrayLen,
                I32Const(i32::MAX - 2),
                I32GtU,
                If(BlockType::Empty),
            ]);
            nominal::error(&mut body);
            body.push(End);
            array(&mut body, table);
            body.extend([
                ArrayLen,
                I32Const(2),
                I32Add,
                ArrayNewDefault(ARGS),
                LocalSet(fresh),
            ]);
            array(&mut body, fresh);
            body.push(I32Const(0));
            array(&mut body, table);
            body.push(I32Const(0));
            array(&mut body, table);
            body.extend([
                ArrayLen,
                ArrayCopy {
                    array_type_index_dst: ARGS,
                    array_type_index_src: ARGS,
                },
            ]);
            array(&mut body, fresh);
            body.extend([LocalGet(index), LocalGet(1), RefI31, ArraySet(ARGS)]);
            array(&mut body, fresh);
            body.extend([
                LocalGet(index),
                I32Const(1),
                I32Add,
                LocalGet(2),
                ArraySet(ARGS),
            ]);
            array(&mut body, owner);
            body.extend([I32Const(1), LocalGet(fresh), ArraySet(ARGS), LocalGet(2)]);
        } else {
            nil(&mut body);
        }
        let locals = if writing {
            vec![(2, VALUE), (1, ValType::I32), (1, VALUE)]
        } else {
            vec![(2, VALUE), (1, ValType::I32)]
        };
        b.function_with_locals(
            name,
            if writing {
                &[VALUE, ValType::I32, VALUE]
            } else {
                &[VALUE, ValType::I32]
            },
            &[VALUE],
            &locals,
            &body,
        );
    }
    environment
}
