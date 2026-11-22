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

#[test]
fn core_double_coercion_in_a_loop_does_not_call_or_allocate() {
    let source = "(defn ^:export sum-casts []
      (loop [i 0 acc 0.0]
        (if (< i 1234569)
          (recur (inc i) (+ acc (+ (double i) 0.5)))
          acc)))";
    let wit = "package test:casts; world api { export sum-casts: func() -> f64; }";
    let bytes = Compiler::new().compile(source, wit).unwrap();
    for operators in functions_with(&bytes, 1234569) {
        assert!(operators.iter().any(|op| matches!(op, Operator::F64ConvertI64S)));
        let calls = (0..operators.len())
            .filter(|&index| matches!(operators[index], Operator::Call { .. }))
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 1, "only inc's overflow path calls: {operators:?}");
        assert!(throwing_call(&operators, calls[0]), "{operators:?}");
        // A double box is needed only for the final boxed entry result.
        let mut returned = 0;
        for (index, op) in operators.iter().enumerate() {
            if !matches!(op, Operator::StructNew { .. }) { continue; }
            let end = operators[index..].iter()
                .position(|op| matches!(op, Operator::Call { .. } | Operator::Return))
                .map(|distance| index + distance)
                .expect("every allocation reaches an error call or the return");
            assert!(!operators[index..end].iter().any(|op| matches!(op, Operator::Br { .. } | Operator::BrIf { .. })), "no box is carried around the loop: {operators:?}");
            if matches!(operators[end], Operator::Return) {
                returned += 1;
            } else {
                assert!(throwing_call(&operators, end), "{operators:?}");
            }
        }
        assert_eq!(returned, 1, "only the result is boxed: {operators:?}");
    }
}

#[test]
fn checked_long_of_a_raw_double_calls_only_when_out_of_range() {
    let source = "(defn ^:export sum-longs []
      (loop [i 0 d 0.25 acc 0]
        (if (< i 1234571)
          (recur (inc i) (+ d 1.0) (+ acc (long d)))
          acc)))";
    let wit = "package test:casts; world api { export sum-longs: func() -> s64; }";
    let bytes = Compiler::new().compile(source, wit).unwrap();
    for operators in functions_with(&bytes, 1234571) {
        let calls = (0..operators.len())
            .filter(|&index| matches!(operators[index], Operator::Call { .. }))
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 3, "two overflow paths and one conversion error: {operators:?}");
        assert!(calls.iter().all(|&index| throwing_call(&operators, index)), "{operators:?}");
        assert!(operators.iter().any(|op| matches!(op, Operator::I64TruncSatF64S)));
    }
}

fn extrema_component(bytes: &[u8]) -> (wasmtime::Store<()>, wasmtime::component::Instance) {
    use std::sync::OnceLock;
    use wasmtime::{Config, Engine, Store};
    use wasmtime::component::{Component, Linker};
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    let engine = ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true)
            .wasm_exceptions(true).wasm_component_model(true).wasm_component_model_implements(true)
            .consume_fuel(true).cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    });
    let component = Component::new(engine, bytes).unwrap();
    let mut store = Store::new(engine, ());
    store.set_fuel(500_000_000).unwrap();
    let instance = Linker::new(engine).instantiate(&mut store, &component).unwrap();
    (store, instance)
}

#[test]
fn long_extrema_loop_stays_raw_and_executes_the_inspected_component() {
    let source = "(defn ^:export bound [] (loop [i 0 acc 0]
      (if (< i 1234573) (recur (inc i) (max acc (min i 100))) acc)))";
    let bytes = Compiler::new().compile(source,
        "package test:extrema; world api { export bound: func() -> s64; }").unwrap();
    for operators in functions_with(&bytes, 1234573) {
        let calls = (0..operators.len()).filter(|&i| matches!(operators[i], Operator::Call { .. })).collect::<Vec<_>>();
        assert_eq!(calls.len(), 1, "only inc's throwing path calls: {operators:?}");
        assert!(throwing_call(&operators, calls[0]));
        assert!(operators.iter().filter(|op| matches!(op, Operator::Select)).count() >= 2);
        // Every allocation either feeds inc's error or immediately returns.
        for (index, op) in operators.iter().enumerate() {
            if !matches!(op, Operator::StructNew { .. }) { continue; }
            let end = operators[index..].iter().position(|op| matches!(op, Operator::Call { .. } | Operator::Return))
                .map(|offset| index + offset).unwrap();
            assert!(!operators[index..end].iter().any(|op| matches!(op, Operator::Br { .. } | Operator::BrIf { .. })));
            assert!(matches!(operators[end], Operator::Return) || throwing_call(&operators, end));
        }
    }
    let (mut store, instance) = extrema_component(&bytes);
    let run = instance.get_typed_func::<(), (i64,)>(&mut store, "bound").unwrap();
    assert_eq!(run.call(&mut store, ()).unwrap().0, 100);
    run.post_return(&mut store).unwrap();
}

#[test]
fn double_extrema_loop_stays_raw_and_executes_the_inspected_component() {
    let source = "(defn ^:export bound [] (loop [i 0 acc -0.0]
      (if (< i 1234575) (recur (inc i) (max acc (min (double i) 0.0))) acc)))";
    let bytes = Compiler::new().compile(source,
        "package test:extrema; world api { export bound: func() -> f64; }").unwrap();
    for operators in functions_with(&bytes, 1234575) {
        assert!(operators.iter().any(|op| matches!(op, Operator::F64Max)));
        assert!(operators.iter().any(|op| matches!(op, Operator::F64Min)));
        let calls = (0..operators.len()).filter(|&i| matches!(operators[i], Operator::Call { .. })).collect::<Vec<_>>();
        assert_eq!(calls.len(), 1, "only inc's throwing path calls: {operators:?}");
        assert!(throwing_call(&operators, calls[0]));
        for (index, op) in operators.iter().enumerate() {
            if !matches!(op, Operator::StructNew { .. }) { continue; }
            let end = operators[index..].iter().position(|op| matches!(op, Operator::Call { .. } | Operator::Return))
                .map(|offset| index + offset).unwrap();
            assert!(!operators[index..end].iter().any(|op| matches!(op, Operator::Br { .. } | Operator::BrIf { .. })));
            assert!(matches!(operators[end], Operator::Return) || throwing_call(&operators, end));
        }
    }
    let (mut store, instance) = extrema_component(&bytes);
    let run = instance.get_typed_func::<(), (f64,)>(&mut store, "bound").unwrap();
    assert_eq!(run.call(&mut store, ()).unwrap().0.to_bits(), 0.0_f64.to_bits());
    run.post_return(&mut store).unwrap();
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

#[test]
fn known_long_core_results_stay_raw_and_execute() {
    let source = "(defn ^:export bound [] (loop [i 0 acc 0]
      (if (< i 1234577)
        (recur (inc i) (unchecked-add acc
          (unchecked-subtract (abs (rem (unchecked-negate i) 7)) (quot i 1234577))))
        acc)))";
    let bytes = Compiler::new().compile(source,
        "package test:raw-core; world api { export bound: func() -> s64; }").unwrap();
    for operators in functions_with(&bytes, 1234577) {
        assert!(operators.iter().any(|op| matches!(op, Operator::I64DivS)));
        assert!(operators.iter().any(|op| matches!(op, Operator::I64RemS)));
        let calls = (0..operators.len()).filter(|&i| matches!(operators[i], Operator::Call { .. })).collect::<Vec<_>>();
        assert!(calls.iter().all(|&i| throwing_call(&operators, i)), "only throwing paths call: {operators:?}");
        for (index, op) in operators.iter().enumerate() {
            if !matches!(op, Operator::StructNew { .. }) { continue; }
            let end = operators[index..].iter().position(|op| matches!(op, Operator::Call { .. } | Operator::Return))
                .map(|offset| index + offset).unwrap();
            assert!(!operators[index..end].iter().any(|op| matches!(op, Operator::Br { .. } | Operator::BrIf { .. })));
            assert!(matches!(operators[end], Operator::Return) || throwing_call(&operators, end));
        }
    }
    let (mut store, instance) = extrema_component(&bytes);
    let run = instance.get_typed_func::<(), (i64,)>(&mut store, "bound").unwrap();
    let expected: i64 = (0..1234577_i64).map(|i| i % 7).sum();
    assert_eq!(run.call(&mut store, ()).unwrap().0, expected);
    run.post_return(&mut store).unwrap();
}

#[test]
fn known_double_core_results_stay_raw_and_execute() {
    let source = "(defn ^:export bound [] (loop [i 0 acc 0.0]
      (if (< i 1234579)
        (recur (inc i) (unchecked-add acc
          (abs (rem (unchecked-multiply -0.5 (double i)) 7.0))))
        acc)))";
    let bytes = Compiler::new().compile(source,
        "package test:raw-core; world api { export bound: func() -> f64; }").unwrap();
    for operators in functions_with(&bytes, 1234579) {
        assert!(operators.iter().any(|op| matches!(op, Operator::F64Div)));
        assert!(operators.iter().any(|op| matches!(op, Operator::F64Abs)));
        let calls = (0..operators.len()).filter(|&i| matches!(operators[i], Operator::Call { .. })).collect::<Vec<_>>();
        assert!(calls.iter().all(|&i| throwing_call(&operators, i)), "only throwing paths call: {operators:?}");
        for (index, op) in operators.iter().enumerate() {
            if !matches!(op, Operator::StructNew { .. }) { continue; }
            let end = operators[index..].iter().position(|op| matches!(op, Operator::Call { .. } | Operator::Return))
                .map(|offset| index + offset).unwrap();
            assert!(!operators[index..end].iter().any(|op| matches!(op, Operator::Br { .. } | Operator::BrIf { .. })));
            assert!(matches!(operators[end], Operator::Return) || throwing_call(&operators, end));
        }
    }
    let (mut store, instance) = extrema_component(&bytes);
    let run = instance.get_typed_func::<(), (f64,)>(&mut store, "bound").unwrap();
    let expected: f64 = (0..1234579_i64).map(|i| (i % 14) as f64 * 0.5).sum();
    assert_eq!(run.call(&mut store, ()).unwrap().0.to_bits(), expected.to_bits());
    run.post_return(&mut store).unwrap();
}

mod raw_bits_and_casts {
    use super::*;
    fn check(operators: &[Operator]) {
        for (i, op) in operators.iter().enumerate() {
            match op {
                Operator::Call { .. } => assert!(throwing_call(operators, i), "normal path calls: {operators:?}"),
                Operator::CallRef { .. } | Operator::ReturnCall { .. } | Operator::ReturnCallRef { .. } =>
                    panic!("normal path calls: {operators:?}"),
                Operator::StructNew { .. } => {
                    let end = operators[i..].iter().position(|op| matches!(op, Operator::Call { .. } | Operator::Return))
                        .map(|offset| i + offset).unwrap();
                    assert!(!operators[i..end].iter().any(|op| matches!(op, Operator::Br { .. } | Operator::BrIf { .. })));
                    assert!(matches!(operators[end], Operator::Return) || throwing_call(operators, end));
                }
                _ => {}
            }
        }
    }
    #[test]
    fn long_bits_execute_without_normal_calls_or_boxes() {
        let source = "(defn ^:export bound [] (loop [i 0 acc 0]
          (if (< i 1234581)
            (recur (inc i) (unchecked-add acc
              (bit-xor (bit-and-not (bit-not i) 255)
                (bit-clear (bit-flip (bit-set 0 i) (+ i 65)) (+ i 128))
                (bit-shift-right (bit-shift-left i i) i)
                (unsigned-bit-shift-right -1 i)))) acc)))";
        let bytes = Compiler::new().compile(source,
            "package test:raw-bits; world api { export bound: func() -> s64; }").unwrap();
        for operators in functions_with(&bytes, 1234581) {
            check(&operators);
            assert!(operators.iter().any(|op| matches!(op, Operator::I64Shl)));
            assert!(operators.iter().any(|op| matches!(op, Operator::I64ShrS)));
            assert!(operators.iter().any(|op| matches!(op, Operator::I64ShrU)));
        }
        let expected = (0..1234581_i64).fold(0_i64, |sum, i| {
            let shift = i as u32;
            let a = !i & !255_i64;
            let b = ((1_i64.wrapping_shl(shift)) ^ 1_i64.wrapping_shl(shift + 65)) & !1_i64.wrapping_shl(shift + 128);
            let c = i.wrapping_shl(shift).wrapping_shr(shift);
            let d = ((-1_i64 as u64).wrapping_shr(shift)) as i64;
            sum.wrapping_add(a ^ b ^ c ^ d)
        });
        let (mut store, instance) = extrema_component(&bytes);
        let run = instance.get_typed_func::<(), (i64,)>(&mut store, "bound").unwrap();
        assert_eq!(run.call(&mut store, ()).unwrap().0, expected);
        run.post_return(&mut store).unwrap();
    }
    #[test]
    fn narrow_integer_casts_execute_without_normal_calls_or_boxes() {
        let source = "(defn ^:export bound [] (loop [i 0 acc 0]
          (if (< i 1234583) (recur (inc i) (unchecked-add acc
            (+ (byte (bit-and i 63)) (short (bit-and i 1023)) (int i)
              (unchecked-byte i) (unchecked-short i) (unchecked-int i)))) acc)))";
        let bytes = Compiler::new().compile(source,
            "package test:raw-casts; world api { export bound: func() -> s64; }").unwrap();
        for operators in functions_with(&bytes, 1234583) { check(&operators); }
        let expected: i64 = (0..1234583_i64).map(|i|
            (i & 63) + (i & 1023) + i + i as i8 as i64 + i as i16 as i64 + i as i32 as i64).sum();
        let (mut store, instance) = extrema_component(&bytes);
        let run = instance.get_typed_func::<(), (i64,)>(&mut store, "bound").unwrap();
        assert_eq!(run.call(&mut store, ()).unwrap().0, expected);
        run.post_return(&mut store).unwrap();
    }
    #[test]
    fn float_casts_execute_without_normal_calls_or_boxes() {
        let source = "(defn ^:export bound [] (loop [i 0 acc 0.0]
          (if (< i 1234585) (recur (inc i)
            (unchecked-add acc (double (float (unchecked-float i))))) acc)))";
        let bytes = Compiler::new().compile(source,
            "package test:raw-floats; world api { export bound: func() -> f64; }").unwrap();
        for operators in functions_with(&bytes, 1234585) {
            check(&operators);
            assert!(operators.iter().any(|op| matches!(op, Operator::F32ConvertI64S)));
            assert!(operators.iter().any(|op| matches!(op, Operator::F32DemoteF64)));
        }
        let expected = (1234585_i64 * 1234584 / 2) as f64;
        let (mut store, instance) = extrema_component(&bytes);
        let run = instance.get_typed_func::<(), (f64,)>(&mut store, "bound").unwrap();
        assert_eq!(run.call(&mut store, ()).unwrap().0.to_bits(), expected.to_bits());
        run.post_return(&mut store).unwrap();
    }
}
