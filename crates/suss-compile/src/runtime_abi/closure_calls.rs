//! Original Function.call/apply adapters over rooted closures and source arrays.
use super::*;

pub(super) fn functions(b: &mut Builder) -> (u32, u32) {
    use Instruction::*;
    let mut indices = vec![];
    for applying in [false, true] {
        let mut code = vec![
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(4)),
            I32Eqz,
            If(BlockType::Empty),
        ];
        nominal::error(&mut code);
        code.extend([End, LocalGet(1), ArrayLen, LocalSet(2)]);
        if applying {
            code.extend([
                I32Const(0),
                ArrayNewDefault(ARGS),
                LocalSet(4),
                LocalGet(2),
                I32Const(1),
                I32GtU,
                If(BlockType::Empty),
                LocalGet(1),
                I32Const(1),
                ArrayGet(ARGS),
                LocalSet(3),
                LocalGet(3),
                I32Const(0),
                RefI31,
                RefEq,
                LocalGet(3),
                I32Const(UNDEFINED),
                RefI31,
                RefEq,
                I32Or,
                I32Eqz,
                If(BlockType::Empty),
                LocalGet(3),
                Call(b.names["source-array-storage"]),
                LocalSet(4),
                End,
                End,
            ]);
        } else {
            code.extend([
                I32Const(0),
                LocalSet(3),
                LocalGet(2),
                I32Eqz,
                I32Eqz,
                If(BlockType::Empty),
                I32Const(1),
                LocalSet(3),
                End,
                LocalGet(2),
                LocalGet(3),
                I32Sub,
                ArrayNewDefault(ARGS),
                LocalSet(4),
                LocalGet(4),
                RefCastNonNull(HeapType::Concrete(ARGS)),
                I32Const(0),
                LocalGet(1),
                LocalGet(3),
                LocalGet(2),
                LocalGet(3),
                I32Sub,
                ArrayCopy {
                    array_type_index_dst: ARGS,
                    array_type_index_src: ARGS,
                },
            ]);
        }
        code.extend([
            LocalGet(0),
            LocalGet(4),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            Call(b.names["invoke"]),
        ]);
        let callback = b.count;
        b.functions.function(INVOKE);
        let locals = if applying {
            vec![(1, ValType::I32), (2, VALUE)]
        } else {
            vec![(2, ValType::I32), (1, VALUE)]
        };
        let mut body = Function::new(locals);
        for instruction in code {
            body.instruction(&instruction);
        }
        body.instruction(&End);
        b.code.function(&body);
        b.count += 1;
        indices.push(callback);
        b.function(
            if applying {
                "closure-apply-method"
            } else {
                "closure-call-method"
            },
            &[VALUE],
            &[VALUE],
            &[
                LocalGet(0),
                RefFunc(callback),
                I32Const(0),
                I32Const(-1),
                Call(b.names["closure-new"]),
            ],
        );
    }
    (indices[0], indices[1])
}
