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
                I32Const(2),
                LocalSet(2),
                Block(BlockType::Empty),
                Loop(BlockType::Empty),
                LocalGet(2),
                LocalGet(1),
                ArrayLen,
                I32GeU,
                BrIf(1),
                LocalGet(3),
                LocalGet(1),
                LocalGet(2),
                ArrayGet(ARGS),
                Call(helper),
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
        let mut function = Function::new([(1, ValType::I32), (1, VALUE)]);
        for instruction in callback_body {
            function.instruction(&instruction);
        }
        function.instruction(&End);
        b.code.function(&function);
        b.count += 1;
        declared.push(callback);
        b.function(
            &format!("primitive-{name}-function"),
            &[],
            &[VALUE],
            &[
                I32Const(0),
                RefI31,
                RefFunc(callback),
                I32Const(arity as i32),
                I32Const(if variadic { -1 } else { arity as i32 }),
                Call(b.names["closure-new"]),
            ],
        );
    }
    declared
}
