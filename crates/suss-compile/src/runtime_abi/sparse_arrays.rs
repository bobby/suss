//! GC-owned sparse backing for source arrays. Logical uint32 length does not
//! allocate elements; linked entries preserve presence independently of value.
//! Uses only the existing shared Args and Number types, with no owner registry.
use super::*;
use Instruction::*;

fn args(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(ARGS))]);
}

// Dense prefix and presence mask are private backing, never callback Args.
fn buffer(body: &mut Vec<Instruction<'static>>, fields: u32, slot: i32, ty: u32) {
    args(body, fields);
    body.extend([
        I32Const(slot),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(ty)),
    ]);
}

// Unlink a validated entry. Removing a property never changes logical length.
fn unlink(body: &mut Vec<Instruction<'static>>, fields: u32, previous: u32, entry: u32) {
    body.extend([
        LocalGet(previous),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
    ]);
    args(body, fields);
    body.push(I32Const(1));
    args(body, entry);
    body.extend([I32Const(2), ArrayGet(ARGS), ArraySet(ARGS), Else]);
    args(body, previous);
    body.push(I32Const(2));
    args(body, entry);
    body.extend([I32Const(2), ArrayGet(ARGS), ArraySet(ARGS), End]);
}

pub(super) fn functions(b: &mut Builder) {
    // Validate every node before dereferencing it. The walker is also used by
    // Floyd validation so malformed/cyclic foreign chains fail before mutation.
    let mut body = vec![
        LocalGet(0),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(0),
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 0);
    body.extend([ArrayLen, I32Const(3), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 0);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 0);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalSet(1),
        LocalGet(1),
        F64Const(0.0.into()),
        F64Lt,
        LocalGet(1),
        F64Const(4294967295.0.into()),
        F64Gt,
        I32Or,
        LocalGet(1),
        LocalGet(1),
        F64Trunc,
        F64Ne,
        I32Or,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 0);
    body.extend([
        I32Const(2),
        ArrayGet(ARGS),
        LocalTee(2),
        I32Const(0),
        RefI31,
        RefEq,
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Or,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([End, LocalGet(2)]);
    let next = b.function_with_locals(
        "source-array-sparse-node-next",
        &[VALUE],
        &[VALUE],
        &[(1, ValType::F64), (1, VALUE)],
        &body,
    );
    b.function(
        "source-array-sparse-new",
        &[ValType::I32],
        &[VALUE],
        &[
            LocalGet(0),
            F64ConvertI32U,
            Call(b.names["number-box"]),
            I32Const(0),
            RefI31,
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 0,
            },
            ArrayNewFixed {
                array_type_index: STRING,
                array_size: 0,
            },
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 4,
            },
        ],
    );
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 0);
    body.extend([ArrayLen, I32Const(4), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 0);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 0);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalSet(1),
        LocalGet(1),
        F64Const(0.0.into()),
        F64Lt,
        LocalGet(1),
        F64Const(4294967295.0.into()),
        F64Gt,
        I32Or,
        LocalGet(1),
        LocalGet(1),
        F64Trunc,
        F64Ne,
        I32Or,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.push(End);
    for (slot, ty) in [(2, ARGS), (3, STRING)] {
        args(&mut body, 0);
        body.extend([
            I32Const(slot),
            ArrayGet(ARGS),
            RefTestNonNull(HeapType::Concrete(ty)),
            I32Eqz,
            If(BlockType::Empty),
        ]);
        nominal::error(&mut body);
        body.push(End);
    }
    buffer(&mut body, 0, 2, ARGS);
    body.push(ArrayLen);
    buffer(&mut body, 0, 3, STRING);
    body.extend([ArrayLen, I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    buffer(&mut body, 0, 2, ARGS);
    body.extend([
        ArrayLen,
        F64ConvertI32U,
        LocalGet(1),
        F64Gt,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 0);
    body.extend([
        I32Const(1),
        ArrayGet(ARGS),
        LocalTee(2),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        Call(next),
        Call(next),
        LocalTee(3),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
        LocalGet(2),
        Call(next),
        LocalTee(2),
        LocalGet(3),
        RefEq,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([End, Br(0), End, End]);
    // Canonical sparse entries are strictly ordered, disjoint from the dense
    // prefix and inside logical length (except the ordinary uint32-max key).
    body.extend([F64Const((-1.0).into()), LocalSet(4)]);
    buffer(&mut body, 0, 2, ARGS);
    body.extend([ArrayLen, LocalSet(6)]);
    args(&mut body, 0);
    body.extend([
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
    ]);
    args(&mut body, 2);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalTee(5),
        LocalGet(4),
        F64Le,
        LocalGet(5),
        LocalGet(6),
        F64ConvertI32U,
        F64Lt,
        I32Or,
        LocalGet(5),
        LocalGet(1),
        F64Ge,
        LocalGet(5),
        F64Const(4294967295.0.into()),
        F64Ne,
        I32And,
        I32Or,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(5),
        LocalSet(4),
        LocalGet(2),
        Call(next),
        LocalSet(2),
        Br(0),
        End,
        End,
        LocalGet(0),
    ]);
    let fields = b.function_with_locals(
        "source-array-sparse-fields",
        &[VALUE],
        &[VALUE],
        &[
            (1, ValType::F64),
            (2, VALUE),
            (2, ValType::F64),
            (1, ValType::I32),
        ],
        &body,
    );
    // Mutations validate the entire presence mask before changing any state.
    // Reads validate only the addressed mask slot, keeping dense reads direct.
    let mut body = vec![
        LocalGet(0),
        Call(fields),
        LocalSet(1),
        I32Const(0),
        LocalSet(2),
    ];
    buffer(&mut body, 1, 3, STRING);
    body.extend([
        ArrayLen,
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        LocalGet(3),
        I32GeU,
        BrIf(1),
    ]);
    buffer(&mut body, 1, 3, STRING);
    body.extend([
        LocalGet(2),
        ArrayGetU(STRING),
        I32Const(1),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        Br(0),
        End,
        End,
        LocalGet(1),
    ]);
    let mutation_fields = b.function_with_locals(
        "source-array-sparse-mutation-fields",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE), (2, ValType::I32)],
        &body,
    );
    let length = b.function(
        "source-array-sparse-length",
        &[VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            Call(fields),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            I32Const(0),
            ArrayGet(ARGS),
            RefCastNonNull(HeapType::Concrete(NUMBER)),
            StructGet {
                struct_type_index: NUMBER,
                field_index: 0,
            },
            I32TruncSatF64U,
        ],
    );
    let mut body = vec![
        LocalGet(0),
        Call(fields),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(2),
        Return,
        End,
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 2);
    body.extend([ArrayLen, I32Const(3), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 2);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 2);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalGet(1),
        F64ConvertI32U,
        F64Eq,
        If(BlockType::Empty),
        LocalGet(2),
        Return,
        End,
    ]);
    args(&mut body, 2);
    body.extend([
        I32Const(2),
        ArrayGet(ARGS),
        LocalSet(2),
        Br(0),
        End,
        End,
        I32Const(0),
        RefI31,
    ]);
    let find = b.function_with_locals(
        "source-array-sparse-find",
        &[VALUE, ValType::I32],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    for (export, presence) in [
        ("source-array-sparse-has", true),
        ("source-array-sparse-get", false),
    ] {
        let mut body = vec![LocalGet(0), Call(fields), LocalSet(2), LocalGet(1)];
        buffer(&mut body, 2, 2, ARGS);
        body.extend([ArrayLen, I32LtU, If(BlockType::Empty)]);
        buffer(&mut body, 2, 3, STRING);
        body.extend([
            LocalGet(1),
            ArrayGetU(STRING),
            LocalTee(3),
            I32Const(1),
            I32GtU,
            If(BlockType::Empty),
        ]);
        nominal::error(&mut body);
        body.push(End);
        if presence {
            body.extend([LocalGet(3), Return]);
        } else {
            body.extend([
                LocalGet(3),
                I32Eqz,
                If(BlockType::Empty),
                I32Const(UNDEFINED),
                RefI31,
                Return,
                End,
            ]);
            buffer(&mut body, 2, 2, ARGS);
            body.extend([LocalGet(1), ArrayGet(ARGS), Return]);
        }
        body.extend([
            End,
            LocalGet(0),
            LocalGet(1),
            Call(find),
            LocalTee(2),
            I32Const(0),
            RefI31,
            RefEq,
        ]);
        if presence {
            body.push(I32Eqz);
        } else {
            body.extend([
                If(BlockType::Empty),
                I32Const(UNDEFINED),
                RefI31,
                Return,
                End,
            ]);
            args(&mut body, 2);
            body.extend([I32Const(1), ArrayGet(ARGS)]);
        }
        b.function_with_locals(
            export,
            &[VALUE, ValType::I32],
            &[if presence { ValType::I32 } else { VALUE }],
            &[(1, VALUE), (1, ValType::I32)],
            &body,
        );
    }
    let mut body = vec![
        LocalGet(0),
        Call(fields),
        LocalSet(3),
        LocalGet(0),
        Call(length),
        LocalSet(5),
    ];
    body.push(LocalGet(1));
    buffer(&mut body, 3, 2, ARGS);
    body.extend([ArrayLen, I32LtU, If(BlockType::Empty)]);
    buffer(&mut body, 3, 3, STRING);
    body.extend([
        LocalGet(1),
        ArrayGetU(STRING),
        I32Const(1),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.push(End);
    buffer(&mut body, 3, 2, ARGS);
    body.extend([LocalGet(1), LocalGet(2), ArraySet(ARGS)]);
    buffer(&mut body, 3, 3, STRING);
    body.extend([
        LocalGet(1),
        I32Const(1),
        ArraySet(STRING),
        LocalGet(2),
        Return,
        End,
    ]);
    body.extend([
        LocalGet(0),
        LocalGet(1),
        Call(find),
        LocalTee(4),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
    ]);
    args(&mut body, 3);
    body.extend([
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(6),
        I32Const(0),
        RefI31,
        LocalSet(7),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(6),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
    ]);
    args(&mut body, 6);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalGet(1),
        F64ConvertI32U,
        F64Ge,
        BrIf(1),
        LocalGet(6),
        LocalSet(7),
        LocalGet(6),
        Call(next),
        LocalSet(6),
        Br(0),
        End,
        End,
        LocalGet(1),
        F64ConvertI32U,
        Call(b.names["number-box"]),
        LocalGet(2),
        LocalGet(6),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        LocalSet(4),
        LocalGet(7),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
    ]);
    args(&mut body, 3);
    body.extend([I32Const(1), LocalGet(4), ArraySet(ARGS), Else]);
    args(&mut body, 7);
    body.extend([I32Const(2), LocalGet(4), ArraySet(ARGS), End, Else]);
    args(&mut body, 4);
    body.extend([
        I32Const(1),
        LocalGet(2),
        ArraySet(ARGS),
        End,
        // 2^32-1 is an ordinary property, never an array index.
        LocalGet(1),
        I32Const(-1),
        I32Ne,
        LocalGet(1),
        LocalGet(5),
        I32GeU,
        I32And,
        If(BlockType::Empty),
    ]);
    args(&mut body, 3);
    body.extend([
        I32Const(0),
        LocalGet(1),
        I32Const(1),
        I32Add,
        F64ConvertI32U,
        Call(b.names["number-box"]),
        ArraySet(ARGS),
        End,
        LocalGet(2),
    ]);
    b.function_with_locals(
        "source-array-sparse-set",
        &[VALUE, ValType::I32, VALUE],
        &[VALUE],
        &[(2, VALUE), (1, ValType::I32), (2, VALUE)],
        &body,
    );
    let mut body = vec![LocalGet(0), Call(fields), LocalSet(2)];
    body.push(LocalGet(1));
    buffer(&mut body, 2, 2, ARGS);
    body.extend([ArrayLen, I32LtU, If(BlockType::Empty)]);
    buffer(&mut body, 2, 3, STRING);
    body.extend([
        LocalGet(1),
        ArrayGetU(STRING),
        I32Const(1),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.push(End);
    buffer(&mut body, 2, 2, ARGS);
    body.extend([LocalGet(1), I32Const(UNDEFINED), RefI31, ArraySet(ARGS)]);
    buffer(&mut body, 2, 3, STRING);
    body.extend([
        LocalGet(1),
        I32Const(0),
        ArraySet(STRING),
        I32Const(1),
        Return,
        End,
    ]);
    body.extend([
        LocalGet(0),
        LocalGet(1),
        Call(find),
        LocalTee(3),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        I32Const(1),
        Return,
        End,
    ]);
    args(&mut body, 2);
    body.extend([
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(4),
        I32Const(0),
        RefI31,
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
        LocalGet(3),
        RefEq,
        If(BlockType::Empty),
    ]);
    unlink(&mut body, 2, 5, 4);
    body.extend([I32Const(1), Return, End, LocalGet(4), LocalSet(5)]);
    args(&mut body, 4);
    body.extend([
        I32Const(2),
        ArrayGet(ARGS),
        LocalSet(4),
        Br(0),
        End,
        End,
        I32Const(1),
    ]);
    b.function_with_locals(
        "source-array-sparse-delete",
        &[VALUE, ValType::I32],
        &[ValType::I32],
        &[(4, VALUE)],
        &body,
    );

    let mut body = vec![LocalGet(0), Call(mutation_fields), LocalSet(2)];
    body.push(LocalGet(1));
    buffer(&mut body, 2, 2, ARGS);
    body.extend([ArrayLen, I32LtU, If(BlockType::Empty)]);
    for (slot, ty) in [(2, ARGS), (3, STRING)] {
        body.extend([
            LocalGet(1),
            ArrayNewDefault(ty),
            LocalSet(7),
            LocalGet(7),
            RefCastNonNull(HeapType::Concrete(ty)),
            I32Const(0),
        ]);
        buffer(&mut body, 2, slot, ty);
        body.extend([
            I32Const(0),
            LocalGet(1),
            ArrayCopy {
                array_type_index_dst: ty,
                array_type_index_src: ty,
            },
        ]);
        args(&mut body, 2);
        body.extend([I32Const(slot), LocalGet(7), ArraySet(ARGS)]);
    }
    body.push(End);
    args(&mut body, 2);
    body.extend([
        I32Const(0),
        LocalGet(1),
        F64ConvertI32U,
        Call(b.names["number-box"]),
        ArraySet(ARGS),
    ]);
    args(&mut body, 2);
    body.extend([
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(3),
        I32Const(0),
        RefI31,
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
    ]);
    args(&mut body, 3);
    body.extend([I32Const(2), ArrayGet(ARGS), LocalSet(5)]);
    args(&mut body, 3);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        I32TruncSatF64U,
        LocalTee(6),
        I32Const(-1),
        I32Ne,
        LocalGet(6),
        LocalGet(1),
        I32GeU,
        I32And,
        If(BlockType::Empty),
    ]);
    unlink(&mut body, 2, 4, 3);
    body.extend([
        Else,
        LocalGet(3),
        LocalSet(4),
        End,
        LocalGet(5),
        LocalSet(3),
        Br(0),
        End,
        End,
    ]);
    b.function_with_locals(
        "source-array-sparse-set-length",
        &[VALUE, ValType::I32],
        &[],
        &[(4, VALUE), (1, ValType::I32), (1, VALUE)],
        &body,
    );
    // Bulk input becomes a dense prefix: constant-time indexed reads, one copy.
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.push(End);
    args(&mut body, 0);
    body.extend([
        ArrayLen,
        LocalSet(1),
        LocalGet(1),
        ArrayNewDefault(ARGS),
        LocalSet(2),
    ]);
    args(&mut body, 2);
    body.push(I32Const(0));
    args(&mut body, 0);
    body.extend([
        I32Const(0),
        LocalGet(1),
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(1),
        F64ConvertI32U,
        Call(b.names["number-box"]),
        I32Const(0),
        RefI31,
        LocalGet(2),
        I32Const(1),
        LocalGet(1),
        ArrayNew(STRING),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 4,
        },
    ]);
    b.function_with_locals(
        "source-array-sparse-from-args",
        &[VALUE],
        &[VALUE],
        &[(1, ValType::I32), (1, VALUE)],
        &body,
    );
    // Args consumers explicitly materialize logical holes as undefined. The
    // allocation bound belongs to this call boundary, not array logical length.
    let mut body = vec![
        LocalGet(0),
        Call(mutation_fields),
        LocalSet(1),
        LocalGet(0),
        Call(length),
        LocalTee(2),
        I32Const(arrays::MAX_LENGTH),
        I32GtU,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.extend([
        End,
        I32Const(UNDEFINED),
        RefI31,
        LocalGet(2),
        ArrayNew(ARGS),
        LocalSet(3),
    ]);
    // Copy dense payload only where its presence mask is set.
    buffer(&mut body, 1, 2, ARGS);
    body.extend([
        ArrayLen,
        LocalSet(4),
        I32Const(0),
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(5),
        LocalGet(4),
        I32GeU,
        BrIf(1),
    ]);
    buffer(&mut body, 1, 3, STRING);
    body.extend([LocalGet(5), ArrayGetU(STRING), If(BlockType::Empty)]);
    args(&mut body, 3);
    body.push(LocalGet(5));
    buffer(&mut body, 1, 2, ARGS);
    body.extend([
        LocalGet(5),
        ArrayGet(ARGS),
        ArraySet(ARGS),
        End,
        LocalGet(5),
        I32Const(1),
        I32Add,
        LocalSet(5),
        Br(0),
        End,
        End,
    ]);
    // Traverse sparse entries once, excluding the uint32-max ordinary property.
    args(&mut body, 1);
    body.extend([
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(6),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(6),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
    ]);
    args(&mut body, 6);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        I32TruncSatF64U,
        LocalTee(5),
        I32Const(-1),
        I32Ne,
        If(BlockType::Empty),
    ]);
    args(&mut body, 3);
    body.push(LocalGet(5));
    args(&mut body, 6);
    body.extend([
        I32Const(1),
        ArrayGet(ARGS),
        ArraySet(ARGS),
        End,
        LocalGet(6),
        Call(next),
        LocalSet(6),
        Br(0),
        End,
        End,
        LocalGet(3),
    ]);
    b.function_with_locals(
        "source-array-sparse-to-args",
        &[VALUE],
        &[VALUE],
        &[
            (1, VALUE),
            (1, ValType::I32),
            (1, VALUE),
            (2, ValType::I32),
            (1, VALUE),
        ],
        &body,
    );
    // Indexed range copy preserves holes, traversing physical storage only.
    // End may exceed current length: callers can have captured length before
    // bound coercion shrank the source. Such trailing positions remain holes.
    let mut body = vec![
        LocalGet(0),
        Call(mutation_fields),
        LocalSet(3),
        LocalGet(1),
        LocalGet(2),
        I32GtU,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.push(End);
    buffer(&mut body, 3, 2, ARGS);
    body.extend([
        ArrayLen,
        LocalSet(4),
        LocalGet(1),
        LocalGet(4),
        I32GeU,
        If(BlockType::Result(ValType::I32)),
        I32Const(0),
        Else,
        LocalGet(4),
        LocalGet(2),
        I32LtU,
        If(BlockType::Result(ValType::I32)),
        LocalGet(4),
        Else,
        LocalGet(2),
        End,
        LocalGet(1),
        I32Sub,
        End,
        LocalSet(5),
    ]);
    for (slot, ty, dest) in [(2, ARGS, 6), (3, STRING, 7)] {
        body.extend([
            LocalGet(5),
            ArrayNewDefault(ty),
            LocalSet(dest),
            LocalGet(5),
            If(BlockType::Empty),
            LocalGet(dest),
            RefCastNonNull(HeapType::Concrete(ty)),
            I32Const(0),
        ]);
        buffer(&mut body, 3, slot, ty);
        body.extend([
            LocalGet(1),
            LocalGet(5),
            ArrayCopy {
                array_type_index_dst: ty,
                array_type_index_src: ty,
            },
            End,
        ]);
    }
    args(&mut body, 3);
    body.extend([
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(8),
        I32Const(0),
        RefI31,
        LocalTee(9),
        LocalSet(10),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(8),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
    ]);
    args(&mut body, 8);
    body.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        I32TruncSatF64U,
        LocalTee(12),
        LocalGet(1),
        I32GeU,
        LocalGet(12),
        LocalGet(2),
        I32LtU,
        I32And,
        If(BlockType::Empty),
        LocalGet(12),
        LocalGet(1),
        I32Sub,
        F64ConvertI32U,
        Call(b.names["number-box"]),
    ]);
    args(&mut body, 8);
    body.extend([
        I32Const(1),
        ArrayGet(ARGS),
        I32Const(0),
        RefI31,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        LocalSet(11),
        LocalGet(10),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        LocalGet(11),
        LocalSet(9),
        Else,
    ]);
    args(&mut body, 10);
    body.extend([
        I32Const(2),
        LocalGet(11),
        ArraySet(ARGS),
        End,
        LocalGet(11),
        LocalSet(10),
        End,
        LocalGet(8),
        Call(next),
        LocalSet(8),
        Br(0),
        End,
        End,
        LocalGet(2),
        LocalGet(1),
        I32Sub,
        F64ConvertI32U,
        Call(b.names["number-box"]),
        LocalGet(9),
        LocalGet(6),
        LocalGet(7),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 4,
        },
    ]);
    let slice = b.function_with_locals(
        "source-array-sparse-slice",
        &[VALUE, ValType::I32, ValType::I32],
        &[VALUE],
        &[(1, VALUE), (2, ValType::I32), (6, VALUE), (1, ValType::I32)],
        &body,
    );
    b.function(
        "source-array-sparse-clone",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            I32Const(0),
            LocalGet(0),
            Call(length),
            Call(slice),
        ],
    );
}
