//! Original live Array join/conversion adapters with GC-rooted recursion context.
use super::*;
use Instruction::*;
pub(super) const ACTIVE: u32 = range_errors::DESCRIPTOR_GLOBAL + 6;
fn text(code: &mut Vec<Instruction<'static>>, s: &str) {
    code.extend(s.encode_utf16().map(|u| I32Const(i32::from(u))));
    code.push(ArrayNewFixed {
        array_type_index: STRING,
        array_size: s.encode_utf16().count() as u32,
    });
}
fn args(code: &mut Vec<Instruction<'static>>, local: u32) {
    code.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(ARGS))]);
}
fn string(code: &mut Vec<Instruction<'static>>, local: u32) {
    code.extend([LocalGet(local), RefCastNonNull(HeapType::Concrete(STRING))]);
}
fn callback(b: &mut Builder, locals: &[(u32, ValType)], code: &[Instruction<'static>]) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut f = Function::new(locals.iter().copied());
    for inst in code {
        f.instruction(inst);
    }
    f.instruction(&End);
    b.code.function(&f);
    b.count += 1;
    index
}
pub(super) fn append_globals(globals: &mut GlobalSection) {
    for _ in 0..4 {
        globals.global(
            GlobalType {
                val_type: VALUE,
                mutable: true,
                shared: false,
            },
            &ConstExpr::extended([I32Const(0), RefI31]),
        );
    }
}
pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    let mut code = vec![
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
    nominal::error(&mut code);
    code.push(End);
    args(&mut code, 0);
    code.extend([ArrayLen, I32Const(2), I32Ne, If(BlockType::Empty)]);
    nominal::error(&mut code);
    code.push(End);
    args(&mut code, 0);
    code.extend([
        I32Const(0),
        ArrayGet(ARGS),
        Call(b.names["source-array?"]),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.push(End);
    args(&mut code, 0);
    code.extend([I32Const(1), ArrayGet(ARGS)]);
    let next = b.function("source-array-join-stack-next", &[VALUE], &[VALUE], &code);
    // Find the next present own index from the current live backing. Dense masks
    // and ordered sparse nodes are traversed; uint32 MAX is an ordinary key.
    code = vec![
        LocalGet(0),
        Call(b.names["source-array-sparse-fields"]),
        LocalSet(2),
        LocalGet(1),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
    ];
    args(&mut code, 2);
    code.extend([
        I32Const(3),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        I32GeU,
        BrIf(1),
    ]);
    args(&mut code, 2);
    code.extend([
        I32Const(3),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(4),
        ArrayGetU(STRING),
        LocalTee(6),
        I32Const(1),
        I32GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(6),
        If(BlockType::Empty),
        LocalGet(4),
        Return,
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Br(0),
        End,
        End,
    ]);
    args(&mut code, 2);
    code.extend([
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
    ]);
    args(&mut code, 3);
    code.extend([
        I32Const(0),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalSet(5),
        LocalGet(5),
        F64Const(4294967295.0.into()),
        F64Lt,
        LocalGet(5),
        LocalGet(1),
        F64ConvertI32U,
        F64Ge,
        I32And,
        If(BlockType::Empty),
        LocalGet(5),
        I32TruncSatF64U,
        Return,
        End,
        LocalGet(3),
        Call(b.names["source-array-sparse-node-next"]),
        LocalSet(3),
        Br(0),
        End,
        End,
        I32Const(-1),
    ]);
    let next_index = b.function_with_locals(
        "source-array-join-next-own-index",
        &[VALUE, ValType::I32],
        &[ValType::I32],
        &[
            (2, VALUE),
            (1, ValType::I32),
            (1, ValType::F64),
            (1, ValType::I32),
        ],
        &code,
    );
    code = vec![];
    string(&mut code, 1);
    code.extend([
        ArrayLen,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(2),
        Return,
        End,
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        I32Eqz,
        BrIf(1),
    ]);
    string(&mut code, 0);
    code.push(LocalGet(2));
    string(&mut code, 1);
    code.push(I32Const(0));
    string(&mut code, 1);
    code.extend([
        ArrayLen,
        ArrayCopy {
            array_type_index_dst: STRING,
            array_type_index_src: STRING,
        },
        LocalGet(2),
    ]);
    string(&mut code, 1);
    code.extend([
        ArrayLen,
        I32Add,
        LocalSet(2),
        LocalGet(3),
        I32Const(1),
        I32Sub,
        LocalSet(3),
        Br(0),
        End,
        End,
        LocalGet(2),
    ]);
    let separators = b.function(
        "source-array-join-copy-separators",
        &[VALUE, VALUE, ValType::I32, ValType::I32],
        &[ValType::I32],
        &code,
    );
    // Only nonempty converted pieces allocate nodes. Missing own ranges are
    // skipped without logical-length-sized storage or traversal.
    code = vec![
        I32Const(0),
        RefI31,
        LocalSet(4),
        I32Const(0),
        RefI31,
        LocalSet(8),
        LocalGet(2),
        I32Eqz,
        If(BlockType::Result(ValType::I64)),
        I64Const(0),
        Else,
        LocalGet(2),
        I32Const(1),
        I32Sub,
        I64ExtendI32U,
    ];
    string(&mut code, 1);
    code.extend([
        ArrayLen,
        I64ExtendI32U,
        I64Mul,
        End,
        LocalSet(5),
        LocalGet(5),
        I64Const(i64::from(arrays::MAX_LENGTH)),
        I64GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.extend([
        End,
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(0),
        Call(b.names["source-array-backing"]),
        LocalGet(3),
        Call(next_index),
        LocalTee(3),
        LocalGet(2),
        I32GeU,
        BrIf(1),
        LocalGet(0),
        LocalGet(3),
        F64ConvertI32U,
        Call(b.names["number-box"]),
        Call(b.names["source-array-get"]),
        LocalSet(6),
        LocalGet(6),
        I32Const(0),
        RefI31,
        RefEq,
        LocalGet(6),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        I32Or,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(6),
        Call(b.names["coerce-string"]),
        LocalSet(6),
        LocalGet(5),
    ]);
    string(&mut code, 6);
    code.extend([
        ArrayLen,
        I64ExtendI32U,
        I64Add,
        LocalSet(5),
        LocalGet(5),
        I64Const(i64::from(arrays::MAX_LENGTH)),
        I64GtU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.push(End);
    string(&mut code, 6);
    code.extend([
        ArrayLen,
        If(BlockType::Empty),
        LocalGet(6),
        LocalGet(3),
        F64ConvertI32U,
        Call(b.names["number-box"]),
        LocalGet(4),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        LocalSet(4),
        End,
        End,
        LocalGet(3),
        I32Const(1),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
    ]);
    args(&mut code, 4);
    code.extend([I32Const(2), ArrayGet(ARGS), LocalSet(7)]);
    args(&mut code, 4);
    code.extend([
        I32Const(2),
        LocalGet(8),
        ArraySet(ARGS),
        LocalGet(4),
        LocalSet(8),
        LocalGet(7),
        LocalSet(4),
        Br(0),
        End,
        End,
        LocalGet(5),
        I32WrapI64,
        ArrayNewDefault(STRING),
        LocalSet(11),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(8),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
    ]);
    args(&mut code, 8);
    code.extend([
        I32Const(1),
        ArrayGet(ARGS),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        I32TruncSatF64U,
        LocalSet(3),
        LocalGet(11),
        LocalGet(1),
        LocalGet(10),
        LocalGet(3),
        LocalGet(9),
        I32Sub,
        Call(separators),
        LocalSet(10),
    ]);
    args(&mut code, 8);
    code.extend([I32Const(0), ArrayGet(ARGS), LocalSet(6)]);
    string(&mut code, 11);
    code.push(LocalGet(10));
    string(&mut code, 6);
    code.push(I32Const(0));
    string(&mut code, 6);
    code.extend([
        ArrayLen,
        ArrayCopy {
            array_type_index_dst: STRING,
            array_type_index_src: STRING,
        },
        LocalGet(10),
    ]);
    string(&mut code, 6);
    code.extend([ArrayLen, I32Add, LocalSet(10), LocalGet(3), LocalSet(9)]);
    args(&mut code, 8);
    code.extend([
        I32Const(2),
        ArrayGet(ARGS),
        LocalSet(8),
        Br(0),
        End,
        End,
        LocalGet(11),
        LocalGet(1),
        LocalGet(10),
        LocalGet(2),
        I32Eqz,
        If(BlockType::Result(ValType::I32)),
        I32Const(0),
        Else,
        LocalGet(2),
        I32Const(1),
        I32Sub,
        LocalGet(9),
        I32Sub,
        End,
        Call(separators),
        Drop,
        LocalGet(11),
    ]);
    let body = b.function_with_locals(
        "source-array-join-body",
        &[VALUE, VALUE, ValType::I32],
        &[VALUE],
        &[
            (1, ValType::I32),
            (1, VALUE),
            (1, ValType::I64),
            (3, VALUE),
            (2, ValType::I32),
            (1, VALUE),
        ],
        &code,
    );
    code = vec![
        LocalGet(0),
        Call(b.names["source-array-backing"]),
        Call(b.names["source-array-sparse-length"]),
        LocalSet(2),
        LocalGet(1),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        If(BlockType::Result(VALUE)),
    ];
    text(&mut code, ",");
    code.extend([
        Else,
        LocalGet(1),
        Call(b.names["coerce-string"]),
        End,
        LocalSet(1),
        GlobalGet(ACTIVE),
        LocalTee(3),
        LocalTee(4),
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(5),
        Call(next),
        Call(next),
        LocalTee(5),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
        LocalGet(4),
        Call(next),
        LocalTee(4),
        LocalGet(5),
        RefEq,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.extend([
        End,
        Br(0),
        End,
        End,
        LocalGet(3),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
        I32Const(0),
        RefI31,
        RefEq,
        BrIf(1),
    ]);
    args(&mut code, 4);
    code.extend([
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(0),
        RefEq,
        If(BlockType::Empty),
    ]);
    text(&mut code, "");
    code.extend([
        Return,
        End,
        LocalGet(4),
        Call(next),
        LocalSet(4),
        Br(0),
        End,
        End,
        LocalGet(0),
        LocalGet(3),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        GlobalSet(ACTIVE),
        Block(BlockType::Empty),
        Block(BlockType::Result(ValType::Ref(RefType::EXNREF))),
        TryTable(
            BlockType::Empty,
            Cow::Owned(vec![wasm_encoder::Catch::AllRef { label: 0 }]),
        ),
        LocalGet(0),
        LocalGet(1),
        LocalGet(2),
        Call(body),
        LocalSet(6),
        Br(2),
        End,
        Unreachable,
        End,
        LocalGet(3),
        GlobalSet(ACTIVE),
        ThrowRef,
        End,
        LocalGet(3),
        GlobalSet(ACTIVE),
        LocalGet(6),
    ]);
    let join = b.function_with_locals(
        "source-array-join",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, ValType::I32), (4, VALUE)],
        &code,
    );
    let mut refs = vec![];
    for (position, name) in ["join", "valueOf", "toString"].into_iter().enumerate() {
        code = vec![LocalGet(1), ArrayLen, I32Eqz, If(BlockType::Empty)];
        nominal::error(&mut code);
        code.extend([
            End,
            LocalGet(1),
            I32Const(0),
            ArrayGet(ARGS),
            LocalSet(2),
            LocalGet(2),
            Call(b.names["source-array-backing"]),
            Drop,
        ]);
        match name {
            "join" => code.extend([
                LocalGet(2),
                LocalGet(1),
                ArrayLen,
                I32Const(1),
                I32GtU,
                If(BlockType::Result(VALUE)),
                LocalGet(1),
                I32Const(1),
                ArrayGet(ARGS),
                Else,
                I32Const(UNDEFINED),
                RefI31,
                End,
                Call(join),
            ]),
            "valueOf" => code.push(LocalGet(2)),
            _ => {
                code.push(LocalGet(2));
                text(&mut code, "join");
                code.extend([
                    Call(b.names["source-array-property-has"]),
                    If(BlockType::Result(VALUE)),
                    LocalGet(2),
                ]);
                text(&mut code, "join");
                code.extend([
                    Call(b.names["source-array-property-get"]),
                    Else,
                    Call(b.names["source-array-join-method"]),
                    End,
                    LocalTee(3),
                    RefTestNonNull(HeapType::Concrete(CLOSURE)),
                    If(BlockType::Result(VALUE)),
                    LocalGet(3),
                    LocalGet(2),
                    ArrayNewFixed {
                        array_type_index: ARGS,
                        array_size: 0,
                    },
                    Call(b.names["object-method-invoke"]),
                    Else,
                ]);
                text(&mut code, "[object Array]");
                code.push(End);
            }
        }
        let anchored = callback(b, &[(2, VALUE)], &code);
        refs.push(anchored);
        code = vec![];
        nominal::error(&mut code);
        let detached = callback(b, &[], &code);
        refs.push(detached);
        let root = ACTIVE + 1 + position as u32;
        b.function(
            &format!("source-array-{name}-method"),
            &[],
            &[VALUE],
            &[
                GlobalGet(root),
                RefTestNonNull(HeapType::Concrete(CLOSURE)),
                If(BlockType::Empty),
                GlobalGet(root),
                Return,
                End,
                GlobalGet(object_methods::TAG_GLOBAL),
                I32Const(0),
                RefI31,
                RefFunc(anchored),
                I32Const(1),
                I32Const(-1),
                Call(b.names["closure-new"]),
                ArrayNewFixed {
                    array_type_index: ARGS,
                    array_size: 1,
                },
                I32Const(0),
                RefI31,
                I32Const(0),
                RefI31,
                StructNew(7),
                RefFunc(detached),
                I32Const(0),
                I32Const(-1),
                Call(b.names["closure-new"]),
                GlobalSet(root),
                GlobalGet(root),
            ],
        );
    }
    refs
}
