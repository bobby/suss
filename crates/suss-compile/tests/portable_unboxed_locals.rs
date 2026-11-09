//! A loop over longs compiles to raw `i64` locals: nothing in it allocates or
//! calls the runtime except the paths that throw "long overflow" (#337).
#![cfg(not(target_family = "wasm"))]

use suss_compile::Compiler;
use wasmparser::{Operator, Parser, Payload};

/// The operators of each compiled function containing `marker`.
fn functions_with(bytes: &[u8], marker: i64) -> Vec<Vec<Operator<'_>>> {
    let mut found = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            let operators = body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            if operators.iter().any(|op| matches!(op, Operator::I64Const { value } if *value == marker)) {
                found.push(operators);
            }
        }
    }
    assert!(!found.is_empty(), "a function holds {marker}");
    found
}

/// Whether the call at `index` is a slow path: one that throws, so its
/// result is dropped before `unreachable`.
fn throwing_call(operators: &[Operator], index: usize) -> bool {
    matches!(operators.get(index + 1), Some(Operator::Drop))
        && matches!(operators.get(index + 2), Some(Operator::Unreachable))
}

#[test]
fn a_long_sum_loop_allocates_only_its_result() {
    let source = "(defn ^:export sum-to [] (loop [i 0 acc 0] (if (< i 1234567) (recur (inc i) (+ acc i)) acc)))";
    let wit = "package test:unboxed; world api { export sum-to: func() -> s64; }";
    let bytes = Compiler::new().compile(source, wit).unwrap();
    for operators in functions_with(&bytes, 1234567) {
        check_sum_loop(&operators);
    }
}

fn check_sum_loop(operators: &[Operator]) {
    // The loop's two additions each have a throwing path; nothing else calls.
    let calls = (0..operators.len())
        .filter(|&index| matches!(operators[index], Operator::Call { .. }))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2, "{operators:?}");
    assert!(calls.iter().all(|&index| throwing_call(operators, index)), "{operators:?}");
    // Allocations box operands for those paths or box the returned sum.
    let mut returned = 0;
    for (index, op) in operators.iter().enumerate() {
        match op {
            Operator::StructNew { .. } => {
                let next = operators[index..]
                    .iter()
                    .position(|op| matches!(op, Operator::Call { .. } | Operator::Return))
                    .map(|offset| &operators[index + offset]);
                match next {
                    Some(Operator::Call { .. }) => {}
                    Some(Operator::Return) => returned += 1,
                    other => panic!("allocation outside a throwing path or return: {other:?}"),
                }
            }
            Operator::ArrayNew { .. }
            | Operator::ArrayNewFixed { .. }
            | Operator::ArrayNewDefault { .. }
            | Operator::StructNewDefault { .. } => panic!("unexpected allocation: {op:?}"),
            _ => {}
        }
    }
    assert_eq!(returned, 1, "only the result is boxed: {operators:?}");
    // The counter and the sum are i64 arithmetic.
    assert!(operators.iter().any(|op| matches!(op, Operator::I64LtS)));
    assert!(operators.iter().filter(|op| matches!(op, Operator::I64Add)).count() >= 2);
}
