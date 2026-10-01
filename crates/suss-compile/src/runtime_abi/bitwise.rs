//! Original scalar ToInt32 primitive for retained hashing dependencies.
//! Public core forms and object-to-primitive conversion are separate work.
use super::*;

pub(super) fn intrinsics(b: &mut Builder) {
    use Instruction::*;
    let convert = b.names["coerce-number"];
    b.function_with_locals(
        "coerce-int32",
        &[VALUE],
        &[ValType::I32],
        &[(1, ValType::F64)],
        &[
            LocalGet(0),
            Call(convert),
            F64Trunc,
            LocalSet(1),
            // NaN and either infinity must return zero before a Wasm integer
            // conversion. Signed zero/subnormal inputs also become zero.
            LocalGet(1),
            F64Abs,
            F64Const(f64::INFINITY.into()),
            F64Lt,
            If(BlockType::Result(ValType::I32)),
            // All finite f64 values with magnitude >= 2^84 are multiples of
            // 2^32. Below that threshold these power-of-two operations retain
            // the exact integral remainder, including negative inputs.
            LocalGet(1),
            LocalGet(1),
            F64Const(4294967296.0.into()),
            F64Div,
            F64Floor,
            F64Const(4294967296.0.into()),
            F64Mul,
            F64Sub,
            I32TruncF64U,
            Else,
            I32Const(0),
            End,
        ],
    );
}

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    let convert = b.names["coerce-int32"];
    // A variadic source body passes the current canonical var to reduce once.
    // Capturing the function body does not freeze that var's contents.
    let mut reducer_body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(5)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    let message: Vec<_> = "Invalid bitwise reducer cell".encode_utf16().collect();
    reducer_body.extend(message.iter().map(|unit| I32Const(*unit as i32)));
    reducer_body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: message.len() as u32,
        },
        Throw(0),
        End,
        LocalGet(0),
        Call(b.names["binding-get"]),
    ]);
    let reducer = b.function("bitwise-current-reducer", &[VALUE], &[VALUE], &reducer_body);
    let mut declared = Vec::new();
    for (name, arity, variadic) in [
        ("int", 1, false),
        ("bit-not", 1, false),
        ("bit-and", 2, true),
        ("bit-or", 2, true),
        ("bit-xor", 2, true),
        ("bit-and-not", 2, true),
        ("bit-clear", 2, false),
        ("bit-flip", 2, false),
        ("bit-set", 2, false),
        ("bit-test", 2, false),
        ("bit-shift-left", 2, false),
        ("bit-shift-right", 2, false),
        ("unsigned-bit-shift-right", 2, false),
        ("imul", 2, false),
    ] {
        let mut body = vec![LocalGet(0), Call(convert)];
        if arity == 2 {
            body.extend([
                LocalSet(2),
                LocalGet(1),
                Call(convert),
                LocalSet(3),
                LocalGet(2),
            ]);
        }
        match name {
            "int" => {}
            "bit-not" => body.extend([I32Const(-1), I32Xor]),
            "bit-and" => body.extend([LocalGet(3), I32And]),
            "bit-or" => body.extend([LocalGet(3), I32Or]),
            "bit-xor" => body.extend([LocalGet(3), I32Xor]),
            "bit-and-not" => body.extend([LocalGet(3), I32Const(-1), I32Xor, I32And]),
            "bit-clear" | "bit-flip" | "bit-set" | "bit-test" => {
                body.extend([I32Const(1), LocalGet(3), I32Shl]);
                match name {
                    "bit-clear" => body.extend([I32Const(-1), I32Xor, I32And]),
                    "bit-flip" => body.push(I32Xor),
                    "bit-set" => body.push(I32Or),
                    _ => body.extend([I32And, I32Eqz, I32Eqz]),
                }
            }
            "bit-shift-left" => body.extend([LocalGet(3), I32Shl]),
            "bit-shift-right" => body.extend([LocalGet(3), I32ShrS]),
            "unsigned-bit-shift-right" => body.extend([LocalGet(3), I32ShrU]),
            "imul" => body.extend([LocalGet(3), I32Mul]),
            _ => unreachable!(),
        }
        if name == "bit-test" {
            body.extend([I32Const(2), I32Mul, I32Const(2), I32Add, RefI31]);
        } else {
            body.push(if name == "unsigned-bit-shift-right" {
                F64ConvertI32U
            } else {
                F64ConvertI32S
            });
            body.push(Call(b.names["number-box"]));
        }
        let helper = b.function_with_locals(
            &format!("primitive-{name}"),
            &vec![VALUE; arity],
            &[VALUE],
            &[(2, ValType::I32)],
            &body,
        );
        let mut callback_body = vec![LocalGet(1), I32Const(0), ArrayGet(ARGS)];
        if arity == 2 {
            callback_body.extend([LocalGet(1), I32Const(1), ArrayGet(ARGS)]);
        }
        callback_body.push(Call(helper));
        if variadic {
            callback_body.extend([
                LocalSet(3),
                // Fixed two-argument calls do not consult the tail reducer.
                LocalGet(1),
                ArrayLen,
                I32Const(2),
                I32GtU,
                If(BlockType::Empty),
                LocalGet(0),
                Call(reducer),
                LocalSet(4),
                End,
                I32Const(2),
                LocalSet(2),
                Block(BlockType::Empty),
                Loop(BlockType::Empty),
                LocalGet(2),
                LocalGet(1),
                ArrayLen,
                I32GeU,
                BrIf(1),
                LocalGet(4),
                LocalGet(3),
                LocalGet(1),
                LocalGet(2),
                ArrayGet(ARGS),
                ArrayNewFixed {
                    array_type_index: ARGS,
                    array_size: 2,
                },
                Call(b.names["invoke"]),
                LocalSet(3),
                LocalGet(2),
                I32Const(1),
                I32Add,
                LocalSet(2),
                Br(0),
                End,
                End,
                LocalGet(3),
            ]);
        }
        let callback = b.count;
        b.functions.function(INVOKE);
        let mut function = Function::new([(1, ValType::I32), (2, VALUE)]);
        for instruction in callback_body {
            function.instruction(&instruction);
        }
        function.instruction(&End);
        b.code.function(&function);
        b.count += 1;
        declared.push(callback);
        b.function(
            &format!("primitive-{name}-function"),
            if variadic { &[VALUE] } else { &[] },
            &[VALUE],
            &[
                if variadic { LocalGet(0) } else { I32Const(0) },
                if variadic { Nop } else { RefI31 },
                RefFunc(callback),
                I32Const(arity as i32),
                I32Const(if variadic { -1 } else { arity as i32 }),
                Call(b.names["closure-new"]),
            ],
        );
    }
    declared
}
