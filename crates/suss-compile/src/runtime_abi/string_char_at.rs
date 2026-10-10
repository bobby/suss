//! Original String.charAt member adapter for portable UTF16 storage.
//! Receiver conversion precedes index conversion; one UTF16 code unit is returned.
use super::*;
use Instruction::*;
const METHOD_ROOT: u32 = array_join::ACTIVE + 1;

pub(super) fn append_globals(globals: &mut GlobalSection) {
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([I32Const(0), RefI31]),
    );
}

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    let mut body = vec![
        LocalGet(0),
        I32Const(0),
        RefI31,
        RefEq,
        LocalGet(0),
        I32Const(6),
        RefI31,
        RefEq,
        I32Or,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        Call(b.names["coerce-string"]),
        LocalSet(2),
        LocalGet(1),
        Call(b.names["coerce-number"]),
        F64Trunc,
        LocalSet(3),
        LocalGet(3),
        LocalGet(3),
        F64Ne,
        If(BlockType::Empty),
        F64Const(0.0.into()),
        LocalSet(3),
        End,
        LocalGet(3),
        F64Const(0.0.into()),
        F64Ge,
        LocalGet(3),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        F64ConvertI32U,
        F64Lt,
        I32And,
        If(BlockType::Result(VALUE)),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(3),
        I32TruncF64U,
        ArrayGetU(STRING),
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: 1,
        },
        Else,
        I32Const(0),
        ArrayNewDefault(STRING),
        End,
    ]);
    let scalar = b.function_with_locals(
        "string-char-at",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::F64)],
        &body,
    );
    let anchored = b.count;
    b.functions.function(INVOKE);
    let mut code = vec![
        LocalGet(1),
        ArrayLen,
        I32Const(1),
        I32LtU,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(1),
        ArrayLen,
        I32Const(2),
        I32GeU,
        If(BlockType::Result(VALUE)),
        LocalGet(1),
        I32Const(1),
        ArrayGet(ARGS),
        Else,
        I32Const(6),
        RefI31,
        End,
        Call(scalar),
        End,
    ]);
    let mut function = Function::new([]);
    for instruction in code {
        function.instruction(&instruction);
    }
    b.code.function(&function);
    b.count += 1;
    let detached = b.count;
    b.functions.function(INVOKE);
    let mut code = vec![];
    nominal::error(&mut code);
    code.push(End);
    let mut function = Function::new([]);
    for instruction in code {
        function.instruction(&instruction);
    }
    b.code.function(&function);
    b.count += 1;
    b.function(
        "string-char-at-method",
        &[],
        &[VALUE],
        &[
            GlobalGet(METHOD_ROOT),
            I32Const(0),
            RefI31,
            RefEq,
            If(BlockType::Empty),
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
            GlobalSet(METHOD_ROOT),
            End,
            GlobalGet(METHOD_ROOT),
        ],
    );
    vec![anchored, detached]
}
