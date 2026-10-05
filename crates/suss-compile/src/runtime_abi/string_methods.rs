//! Original bounded UTF-16 member adapter for retained string algorithms.
//! This is scalar String storage, not general JavaScript ToString/prototypes.
use super::*;
pub(super) const METHOD_ROOT: u32 = object_methods::DEFAULT_THIS + 1;

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    storage_functions(b);
    use Instruction::*;
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(1),
        Call(b.names["coerce-number"]),
        F64Trunc,
        LocalSet(2),
        // ToIntegerOrInfinity maps NaN to zero, without modulo-32 wrapping.
        LocalGet(2),
        LocalGet(2),
        F64Ne,
        If(BlockType::Empty),
        F64Const(0.0.into()),
        LocalSet(2),
        End,
        LocalGet(2),
        F64Const(0.0.into()),
        F64Ge,
        LocalGet(2),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        F64ConvertI32U,
        F64Lt,
        I32And,
        If(BlockType::Result(VALUE)),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(2),
        I32TruncF64U,
        ArrayGetU(STRING),
        F64ConvertI32U,
        Call(b.names["number-box"]),
        Else,
        F64Const(f64::NAN.into()),
        Call(b.names["number-box"]),
        End,
    ]);
    let scalar = b.function_with_locals(
        "string-char-code-at",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, ValType::F64)],
        &body,
    );
    let anchored = b.count;
    b.functions.function(INVOKE);
    let mut f = Function::new([]);
    let mut arity_guard = vec![
        LocalGet(1),
        ArrayLen,
        I32Const(2),
        I32Ne,
        If(BlockType::Empty),
    ];
    nominal::error(&mut arity_guard);
    arity_guard.push(End);
    for i in arity_guard {
        f.instruction(&i);
    }
    for i in [
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        LocalGet(1),
        I32Const(1),
        ArrayGet(ARGS),
        Call(scalar),
        End,
    ] {
        f.instruction(&i);
    }
    b.code.function(&f);
    b.count += 1;
    // The builtin is strict: extracting the function does not bind its owner.
    let detached = b.count;
    b.functions.function(INVOKE);
    let mut instructions = vec![];
    nominal::error(&mut instructions);
    let mut f = Function::new([]);
    for i in instructions {
        f.instruction(&i);
    }
    f.instruction(&End);
    b.code.function(&f);
    b.count += 1;
    b.function(
        "string-char-code-at-method",
        &[],
        &[VALUE],
        &[
            GlobalGet(METHOD_ROOT),
            I32Const(0),
            RefI31,
            RefEq,
            If(BlockType::Empty),
            // Reuse the checked Object wrapper convention: one body, anchored this
            // prepended only by member invocation. No receiver retained in the root.
            GlobalGet(object_methods::TAG_GLOBAL),
            I32Const(0),
            RefI31,
            RefFunc(anchored),
            I32Const(2),
            I32Const(2),
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

/// Original UTF-16 storage operations used by adapted identifier conversion.
/// These private operations require actual strings; slice bounds must be finite
/// integral offsets with 0 <= start <= end <= length. They do not implement
/// JavaScript receiver coercion or the public substring argument normalization.
fn storage_functions(b: &mut Builder) {
    use Instruction::*;
    let mut body = vec![];
    for parameter in [0, 1] {
        body.extend([
            LocalGet(parameter),
            RefTestNonNull(HeapType::Concrete(STRING)),
            I32Eqz,
            If(BlockType::Empty),
        ]);
        nominal::error(&mut body);
        body.push(End);
        body.extend([
            LocalGet(parameter),
            RefCastNonNull(HeapType::Concrete(STRING)),
            ArrayLen,
            LocalSet(parameter + 2),
        ]);
    }
    body.extend([
        LocalGet(3),
        LocalGet(2),
        I32GtU,
        If(BlockType::Empty),
        F64Const((-1.0).into()),
        Call(b.names["number-box"]),
        Return,
        End,
        I32Const(0),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
        LocalGet(2),
        LocalGet(3),
        I32Sub,
        I32GtU,
        BrIf(1),
        I32Const(0),
        LocalSet(5),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(5),
        LocalGet(3),
        I32Eq,
        BrIf(1),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(4),
        LocalGet(5),
        I32Add,
        ArrayGetU(STRING),
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(5),
        ArrayGetU(STRING),
        I32Ne,
        BrIf(1),
        LocalGet(5),
        I32Const(1),
        I32Add,
        LocalSet(5),
        Br(0),
        End,
        End,
        LocalGet(5),
        LocalGet(3),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(4),
        F64ConvertI32U,
        Call(b.names["number-box"]),
        Return,
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Br(0),
        End,
        End,
        F64Const((-1.0).into()),
        Call(b.names["number-box"]),
    ]);
    b.function_with_locals(
        "string-index-of",
        &[VALUE, VALUE],
        &[VALUE],
        &[(4, ValType::I32)],
        &body,
    );

    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        LocalSet(3),
    ]);
    for (parameter, offset) in [(1, 4), (2, 5)] {
        body.extend([
            LocalGet(parameter),
            Call(b.names["coerce-number"]),
            LocalSet(6),
            LocalGet(6),
            F64Const(0.0.into()),
            F64Ge,
            LocalGet(6),
            LocalGet(3),
            F64ConvertI32U,
            F64Le,
            I32And,
            LocalGet(6),
            LocalGet(6),
            F64Trunc,
            F64Eq,
            I32And,
            I32Eqz,
            If(BlockType::Empty),
        ]);
        nominal::error(&mut body);
        body.extend([End, LocalGet(6), I32TruncF64U, LocalSet(offset)]);
    }
    body.extend([LocalGet(4), LocalGet(5), I32GtU, If(BlockType::Empty)]);
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(5),
        LocalGet(4),
        I32Sub,
        ArrayNewDefault(STRING),
        LocalSet(7),
        LocalGet(7),
        RefCastNonNull(HeapType::Concrete(STRING)),
        I32Const(0),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(4),
        LocalGet(5),
        LocalGet(4),
        I32Sub,
        ArrayCopy {
            array_type_index_dst: STRING,
            array_type_index_src: STRING,
        },
        LocalGet(7),
    ]);
    b.function_with_locals(
        "string-slice",
        &[VALUE, VALUE, VALUE],
        &[VALUE],
        &[(3, ValType::I32), (1, ValType::F64), (1, VALUE)],
        &body,
    );
}
