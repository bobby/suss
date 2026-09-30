//! Original universal arithmetic closures, matching pinned core.cljs arities.
//! Public source argument evaluation happens in the caller, before invocation.
use super::*;

pub(super) fn functions(b: &mut Builder, primitives: [u32; 5]) -> Vec<u32> {
    use Instruction::*;
    let mut declared = Vec::new();
    for (operator, name, primitive, minimum) in [
        (0, "add", primitives[0], 0),
        (1, "subtract", primitives[1], 1),
        (2, "multiply", primitives[2], 0),
        (3, "divide", primitives[3], 1),
    ] {
        let mut body = vec![
            LocalGet(1),
            ArrayLen,
            LocalTee(3),
            I32Eqz,
            If(BlockType::Empty),
        ];
        if minimum == 0 {
            body.extend([
                F64Const((if operator == 0 { 0.0 } else { 1.0 }).into()),
                StructNew(NUMBER),
                Return,
            ]);
        } else {
            // Central invoke checks this first. Keep trusted direct calls typed too.
            body.push(GlobalGet(0));
            let message: Vec<_> = "Wrong arity".encode_utf16().collect();
            body.extend(message.iter().map(|unit| I32Const(*unit as i32)));
            body.extend([
                ArrayNewFixed {
                    array_type_index: STRING,
                    array_size: message.len() as u32,
                },
                I32Const(0),
                RefI31,
                I32Const(0),
                RefI31,
                StructNew(8),
                Throw(0),
            ]);
        }
        body.extend([
            End,
            LocalGet(1),
            I32Const(0),
            ArrayGet(ARGS),
            LocalSet(4),
            LocalGet(3),
            I32Const(1),
            I32Eq,
            If(BlockType::Empty),
        ]);
        match operator {
            1 => body.extend([LocalGet(4), Call(primitives[4])]),
            3 => body.extend([
                F64Const(1.0.into()),
                StructNew(NUMBER),
                LocalGet(4),
                Call(primitive),
            ]),
            _ => body.push(LocalGet(4)),
        }
        body.extend([
            Return,
            End,
            I32Const(1),
            LocalSet(2),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(2),
            LocalGet(3),
            I32GeU,
            BrIf(1),
            LocalGet(4),
            LocalGet(1),
            LocalGet(2),
            ArrayGet(ARGS),
            Call(primitive),
            LocalSet(4),
            LocalGet(2),
            I32Const(1),
            I32Add,
            LocalSet(2),
            Br(0),
            End,
            End,
            LocalGet(4),
        ]);
        // The recursive group's shared Invoke identity is required, even when
        // an independently appended function type has the same signature.
        let invoke = b.count;
        b.functions.function(INVOKE);
        b.exports.export(
            &format!("invoke-arithmetic-{name}"),
            ExportKind::Func,
            invoke,
        );
        let mut function = Function::new([(2, ValType::I32), (1, VALUE)]);
        for instruction in &body {
            function.instruction(instruction);
        }
        function.instruction(&End);
        b.code.function(&function);
        b.count += 1;
        declared.push(invoke);
        b.function(
            &format!("arithmetic-{name}"),
            &[],
            &[VALUE],
            &[
                I32Const(0),
                RefI31,
                RefFunc(invoke),
                I32Const(minimum),
                I32Const(-1),
                Call(b.names["closure-new"]),
            ],
        );
    }
    declared
}
