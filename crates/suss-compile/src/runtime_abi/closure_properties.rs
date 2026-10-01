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
    body.extend([Instruction::I32Const(0), Instruction::RefI31]);
}

pub(super) fn functions(b: &mut Builder) -> (u32, u32) {
    use Instruction::*;
    let wrap = b.function(
        "closure-wrap-environment",
        &[VALUE],
        &[VALUE],
        &[
            GlobalGet(TAG_GLOBAL),
            LocalGet(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 2,
            },
            I32Const(0),
            RefI31,
            I32Const(0), RefI31, StructNew(7),
        ],
    );
    let mut body = vec![];
    guard(&mut body, 0, 4);
    field(&mut body, 0, 4, 0);
    body.push(LocalSet(1));
    body.extend([
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(7)),
        If(BlockType::Empty),
    ]);
    field(&mut body, 1, 7, 0);
    body.extend([GlobalGet(TAG_GLOBAL), RefEq, If(BlockType::Empty)]);
    field(&mut body, 1, 7, 1);
    body.push(LocalSet(1));
    array(&mut body, 1);
    body.extend([ArrayLen, I32Const(2), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    array(&mut body, 1);
    body.extend([I32Const(0), ArrayGet(ARGS), Return, End, End, LocalGet(1)]);
    let environment = b.function_with_locals(
        "closure-environment",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );

    let mut body = vec![];
    guard(&mut body, 0, 4);
    field(&mut body, 0, 4, 0);
    body.push(LocalSet(1));
    body.extend([
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(7)),
        If(BlockType::Empty),
    ]);
    field(&mut body, 1, 7, 0);
    body.extend([GlobalGet(TAG_GLOBAL), RefEq, If(BlockType::Empty)]);
    field(&mut body, 1, 7, 1);
    body.push(LocalSet(1));
    array(&mut body, 1);
    body.extend([ArrayLen, I32Const(2), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.extend([End, LocalGet(1), Return, End, End]);
    nil(&mut body);
    let fields = b.function_with_locals(
        "closure-property-fields",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    for writing in [false, true] {
        // Params: function, native kind, optional assigned value. Locals follow.
        let owner = if writing { 3 } else { 2 };
        let table = owner + 1;
        let index = owner + 2;
        let fresh = owner + 3;
        let mut body = vec![LocalGet(1), I32Const(7), I32GtU, If(BlockType::Empty)];
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
            if writing {
                "closure-property-set"
            } else {
                "closure-property-get"
            },
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
    (wrap, environment)
}
