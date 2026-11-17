//! Original universal arithmetic closures, matching pinned core.cljs arities.
//! Public source argument evaluation happens in the caller, before invocation.
//! Operands are checked Numbers, as in Clojure, even for unary + and * (#286).
use super::*;

pub(super) fn functions(b: &mut Builder, primitives: [u32; 7]) -> Vec<u32> {
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
                // (+) is 0 and (*) is 1, both longs.
                I32Const(if operator == 0 { 0 } else { 1 }),
                RefI31,
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
                sentinel_value(NIL),
                sentinel_value(NIL),
                I32Const(0), RefI31, // Identity UID, unassigned.
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
                I32Const(1),
                RefI31,
                LocalGet(4),
                Call(primitive),
            ]),
            0 => body.extend([LocalGet(4), Call(primitives[5])]),
            _ => body.extend([LocalGet(4), Call(primitives[6])]),
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
        // Direct entries for one and two arguments (#444): they call the
        // primitive without packing an arguments array for invoke. The
        // closure's code struct, ARITHMETIC_CODE_GLOBAL's, holds them with the
        // universal entries for the other counts.
        let unary = match operator {
            1 => vec![LocalGet(1), Call(primitives[4])],
            3 => vec![I32Const(1), RefI31, LocalGet(1), Call(primitive)],
            0 => vec![LocalGet(1), Call(primitives[5])],
            _ => vec![LocalGet(1), Call(primitives[6])],
        };
        let one = b.typed(CALL + 1, &[], &unary);
        // Two i31 longs add or subtract in i32 without overflow; the result
        // is an i31 when it fits, as the primitive would box it.
        let mut body = vec![];
        if operator < 2 {
            body.extend([
                LocalGet(1), RefTestNonNull(HeapType::I31), LocalGet(2), RefTestNonNull(HeapType::I31), I32And,
                If(BlockType::Empty),
                LocalGet(1), RefCastNonNull(HeapType::I31), I31GetS,
                LocalGet(2), RefCastNonNull(HeapType::I31), I31GetS,
                if operator == 0 { I32Add } else { I32Sub },
                LocalTee(3), I32Const(0x4000_0000), I32Add, I32Const(0), I32GeS,
                If(BlockType::Empty), LocalGet(3), RefI31, Return, End,
                End,
            ]);
        }
        body.extend([LocalGet(1), LocalGet(2), Call(primitive)]);
        let two = b.typed(CALL + 2, &[(1, ValType::I32)], &body);
        b.names.insert(format!("arithmetic-{name}-1"), one);
        b.names.insert(format!("arithmetic-{name}-2"), two);
        declared.extend([one, two]);
        // A universal closure, as closure-new makes, with this operator's
        // code struct.
        b.function(
            &format!("arithmetic-{name}"),
            &[],
            &[VALUE],
            &[
                GlobalGet(ARITHMETIC_CODE_GLOBAL + operator),
                sentinel_value(NIL),
                I32Const(0), // Identity UID: private unassigned marker.
                RefI31,
                sentinel_value(NIL),
                RefFunc(invoke),
                I32Const(minimum),
                I32Const(-1),
                StructNew(UNIVERSAL),
            ],
        );
    }
    declared
}
