//! Original ABI2 callable adapters for the compiled suss.async source library.
//! No source body is evaluated here; scheduler operations keep their own roots.
use super::*;

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    let mut declared = Vec::new();
    for (name, export, arity, result) in [
        ("pending", "future-pending-new", 0, 0),
        ("resolve!", "future-resolve", 2, 1),
        ("reject!", "future-reject", 2, 1),
        ("cancel!", "async-future-cancel", 1, 1),
        ("status", "future-status", 1, 2),
        ("result", "future-result", 1, 0),
        // All stream parameters are ABI2 Values. The runtime validates boxed
        // integer limits and nominal ownership before changing endpoint state.
        // 0 preserves a Value, 1 boxes a boolean, 2 boxes an unsigned count.
        ("stream-pair", "stream-pair-new", 1, 0),
        ("stream-pair-reader", "stream-pair-reader", 1, 0),
        ("stream-pair-writer", "stream-pair-writer", 1, 0),
        ("stream-read-limit", "stream-read-limit", 2, 0),
        ("stream-write-limit", "stream-write-limit", 2, 0),
        ("stream-begin-read", "stream-begin-read", 2, 0),
        ("stream-begin-write", "stream-begin-write", 2, 0),
        ("stream-operation-future", "stream-operation-future", 1, 0),
        ("stream-operation-retire", "stream-operation-retire", 1, 1),
        ("stream-chunk-new", "stream-chunk-new", 1, 0),
        ("stream-chunk-set", "stream-chunk-set", 3, 0),
        ("stream-chunk-count", "stream-chunk-count", 1, 2),
        ("stream-chunk-nth", "stream-chunk-nth", 2, 0),
        ("stream-eof?", "stream-eof-is", 1, 1),
        ("stream-close!", "stream-close", 1, 1),
        ("stream-fail!", "stream-fail", 2, 1),
    ] {
        let mut callback = Function::new([]);
        callback
            .instruction(&LocalGet(1))
            .instruction(&ArrayLen)
            .instruction(&I32Const(arity))
            .instruction(&I32Ne)
            .instruction(&If(BlockType::Empty))
            .instruction(&Call(b.names["arity-error"]))
            .instruction(&Return)
            .instruction(&End);
        if matches!(name, "resolve!" | "reject!" | "cancel!") {
            // Task results and private stream-operation completions are owned
            // by their respective unwind/transfer paths. Refuse both before
            // calling raw settlement; guards do not evaluate source arguments.
            for guard in ["async-future-task-owned", "stream-operation-owned"] {
                if name == "cancel!" && guard == "async-future-task-owned" {
                    continue;
                }
                callback
                    .instruction(&LocalGet(1))
                    .instruction(&I32Const(0))
                    .instruction(&ArrayGet(ARGS))
                    .instruction(&Call(b.names[guard]))
                    .instruction(&If(BlockType::Empty))
                    .instruction(&I32Const(0))
                    .instruction(&RefI31)
                    .instruction(&Throw(0))
                    .instruction(&End);
            }
        }
        for index in 0..arity {
            callback
                .instruction(&LocalGet(1))
                .instruction(&I32Const(index))
                .instruction(&ArrayGet(ARGS));
        }
        callback.instruction(&Call(b.names[export]));
        if result == 1 {
            callback
                .instruction(&I32Const(2))
                .instruction(&I32Mul)
                .instruction(&I32Const(2))
                .instruction(&I32Add)
                .instruction(&RefI31);
        } else if result == 2 {
            callback
                .instruction(&F64ConvertI32U)
                .instruction(&Call(b.names["number-box"]));
        }
        callback.instruction(&End);
        let index = b.count;
        b.functions.function(INVOKE);
        b.code.function(&callback);
        b.count += 1;
        declared.push(index);
        b.function(
            &format!("async-source-{name}-function"),
            &[],
            &[VALUE],
            &[
                I32Const(0),
                RefI31,
                RefFunc(index),
                I32Const(arity),
                I32Const(arity),
                Call(b.names["closure-new"]),
            ],
        );
    }
    declared
}
