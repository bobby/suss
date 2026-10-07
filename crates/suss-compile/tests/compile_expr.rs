//! Expression tests migrated from the removed prototype route. Each expression
//! compiles through the compiled Macro/Runtime pipeline, executes as a complete
//! bundle in a fresh Session and is decoded independently on the host.

use suss_compile::{Compiler, portable_session::Session};

use wasmtime::{Engine, Store};

mod support;
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};

/// Compile through the shared compiled Macro/Runtime pipeline, execute the
/// complete bundle in a fresh Runtime session and decode the value on the host.
fn observe(expr: &str) -> Observation {
    let compiled = Compiler::new()
        .compile_expr_with_info(expr)
        .unwrap_or_else(|error| panic!("compilation failed for {expr}: {error:?}"));
    let mut session = Session::new().expect("runtime session");
    session.set_operation_fuel(1_000_000_000);
    let (value, mut decoder) = compiled
        .execute_with_core(&mut session, |session| Decoder::capture(session, 1_000_000))
        .unwrap_or_else(|error| panic!("execution failed for {expr}: {error}"));
    let value = value.unwrap_or_else(|| panic!("no value for {expr}"));
    session.collect().expect("collect");
    decoder
        .decode_session(&mut session, &value)
        .unwrap_or_else(|error| panic!("decode failed for {expr}: {error}"))
}

fn run_expr_f64(expr: &str) -> f64 {
    match observe(expr) {
        Observation::Number(bits) => f64::from_bits(bits),
        other => panic!("expected a number from {expr}, observed {other:?}"),
    }
}

/// ClojureScript numbers are binary64; an integer result must be integral.
fn run_expr_i64(expr: &str) -> i64 {
    let value = run_expr_f64(expr);
    assert!(value.fract() == 0.0 && value.abs() <= 9007199254740992.0,
            "expected an integral number from {expr}, observed {value}");
    value as i64
}

fn run_expr_i32(expr: &str) -> i32 {
    i32::try_from(run_expr_i64(expr)).unwrap_or_else(|_| panic!("{expr} exceeds i32"))
}

fn run_expr_bool(expr: &str) -> bool {
    match observe(expr) {
        Observation::Bool(value) => value,
        other => panic!("expected a boolean from {expr}, observed {other:?}"),
    }
}

#[test]
fn test_integer_literal() {
    assert_eq!(run_expr_i32("42"), 42);
}

#[test]
fn test_addition() {
    assert_eq!(run_expr_i32("(+ 1 2)"), 3);
}

#[test]
fn test_subtraction() {
    assert_eq!(run_expr_i32("(- 10 3)"), 7);
}

#[test]
fn test_multiplication() {
    assert_eq!(run_expr_i32("(* 6 7)"), 42);
}

#[test]
fn test_division() {
    // Division always returns float (Clojure semantics)
    assert_eq!(run_expr_f64("(/ 21 3)"), 7.0);
}

#[test]
fn test_nested_arithmetic() {
    assert_eq!(run_expr_i32("(+ (* 3 4) (- 10 5))"), 17);
}

#[test]
fn test_comparison_less_than() {
    assert_eq!(observe("(< 1 2)"), Observation::Bool(true)); // true = 1
    assert_eq!(observe("(< 2 1)"), Observation::Bool(false)); // false = 0
}

#[test]
fn test_comparison_greater_than() {
    assert_eq!(observe("(> 2 1)"), Observation::Bool(true));
    assert_eq!(observe("(> 1 2)"), Observation::Bool(false));
}

#[test]
fn test_comparison_equals() {
    assert_eq!(observe("(= 5 5)"), Observation::Bool(true));
    assert_eq!(observe("(= 5 3)"), Observation::Bool(false));
}

#[test]
fn test_if_true_branch() {
    assert_eq!(run_expr_i32("(if true 1 0)"), 1);
}

#[test]
fn test_if_false_branch() {
    assert_eq!(run_expr_i32("(if false 1 0)"), 0);
}

#[test]
fn test_if_with_condition() {
    assert_eq!(run_expr_i32("(if (> 5 3) 100 200)"), 100);
    assert_eq!(run_expr_i32("(if (< 5 3) 100 200)"), 200);
}

#[test]
fn test_let_simple() {
    assert_eq!(run_expr_i32("(let [x 5] x)"), 5);
}

#[test]
fn test_let_with_expression() {
    assert_eq!(run_expr_i32("(let [x 5 y 3] (+ x y))"), 8);
}

#[test]
fn test_do_block() {
    assert_eq!(run_expr_i32("(do 1 2 3)"), 3);
}

#[test]
fn test_boolean_true() {
    assert_eq!(observe("true"), Observation::Bool(true));
}

#[test]
fn test_boolean_false() {
    assert_eq!(observe("false"), Observation::Bool(false));
}

#[test]
fn test_float_literal() {
    let result = run_expr_f64("3.14");
    assert!((result - 3.14).abs() < 0.001);
}

#[test]
fn test_float_addition() {
    let result = run_expr_f64("(+ 1.5 2.5)");
    assert!((result - 4.0).abs() < 0.001);
}

fn run_expr_string(expr: &str) -> String {
    match observe(expr) {
        Observation::String(units) => String::from_utf16(&units).expect("valid UTF-16"),
        other => panic!("expected a string from {expr}, observed {other:?}"),
    }
}

#[test]
fn test_vector_literal() {
    // Vector literals should compile to PERSISTENT_VECTOR structs
    assert_eq!(observe("[1 2 3]"), Observation::Vector(vec![Observation::Number(1.0f64.to_bits()), Observation::Number(2.0f64.to_bits()), Observation::Number(3.0f64.to_bits())]));
}

#[test]
fn test_empty_vector() {
    assert_eq!(observe("[]"), Observation::Vector(vec![]));
}

#[test]
fn test_map_literal() {
    // Map literals should compile to PERSISTENT_MAP structs
    // Using integers as keys since keywords aren't supported yet
    assert_eq!(observe("{1 2}"), Observation::Map(vec![(Observation::Number(1.0f64.to_bits()), Observation::Number(2.0f64.to_bits()))]));
}

#[test]
fn test_empty_map() {
    assert_eq!(observe("{}"), Observation::Map(vec![]));
}

#[test]
fn test_map_get() {
    // Get should return the value for a key
    assert_eq!(run_expr_i32("(get {1 2} 1)"), 2);
}

#[test]
fn test_map_get_empty() {
    // Get on empty map returns nil
    assert_eq!(observe("(get {} 1)"), Observation::Nil);
}

#[test]
fn test_set_literal() {
    // Set literals should compile to PERSISTENT_SET structs
    let Observation::Set(mut items) = observe("#{1 2 3}") else { panic!("expected a set") };
    items.sort_by_key(|item| match item { Observation::Number(bits) => f64::from_bits(*bits) as i64, _ => i64::MAX });
    assert_eq!(items, vec![Observation::Number(1.0f64.to_bits()), Observation::Number(2.0f64.to_bits()), Observation::Number(3.0f64.to_bits())]);
}

#[test]
fn test_empty_set() {
    assert_eq!(observe("#{}"), Observation::Set(vec![]));
}

// Control flow forms tests

#[test]
fn test_cond_first_true() {
    assert_eq!(run_expr_i32("(cond (> 5 3) 1 :else 0)"), 1);
}

#[test]
fn test_cond_second_true() {
    assert_eq!(run_expr_i32("(cond (< 5 3) 1 (= 5 5) 2 :else 0)"), 2);
}

#[test]
fn test_cond_else() {
    assert_eq!(run_expr_i32("(cond (< 5 3) 1 (< 5 4) 2 :else 99)"), 99);
}

#[test]
fn test_cond_no_else() {
    // Without :else, cond returns nil
    assert_eq!(observe("(cond (< 5 3) 1 (< 5 4) 2)"), Observation::Nil);
}

#[test]
fn test_when_true() {
    assert_eq!(run_expr_i32("(when (> 5 3) 42)"), 42);
}

#[test]
fn test_when_false() {
    assert_eq!(observe("(when (< 5 3) 42)"), Observation::Nil);
}

#[test]
fn test_when_multiple_body() {
    assert_eq!(run_expr_i32("(when true 1 2 3)"), 3);
}

#[test]
fn test_when_not_true() {
    // when-not with true condition returns nil/0
    assert_eq!(observe("(when-not (> 5 3) 42)"), Observation::Nil);
}

#[test]
fn test_when_not_false() {
    // when-not with false condition executes body
    assert_eq!(run_expr_i32("(when-not (< 5 3) 42)"), 42);
}

#[test]
fn test_case_match_first() {
    assert_eq!(run_expr_i32("(case 1 1 10 2 20 3 30 0)"), 10);
}

#[test]
fn test_case_match_second() {
    assert_eq!(run_expr_i32("(case 2 1 10 2 20 3 30 0)"), 20);
}

#[test]
fn test_case_match_third() {
    assert_eq!(run_expr_i32("(case 3 1 10 2 20 3 30 0)"), 30);
}

#[test]
fn test_case_default() {
    assert_eq!(run_expr_i32("(case 99 1 10 2 20 3 30 0)"), 0);
}

#[test]
fn test_case_with_expression() {
    // Test that the expression is only evaluated once
    assert_eq!(run_expr_i32("(case (+ 1 1) 1 10 2 20 3 30 0)"), 20);
}

// Short-circuit and/or tests

#[test]
fn test_and_all_true() {
    assert_eq!(observe("(and true true)"), Observation::Bool(true));
}

#[test]
fn test_and_one_false() {
    assert_eq!(observe("(and true false)"), Observation::Bool(false));
    assert_eq!(observe("(and false true)"), Observation::Bool(false));
}

#[test]
fn test_and_returns_last_value() {
    // (and 1 2 3) returns 3 (last value when all truthy)
    assert_eq!(run_expr_i32("(and 1 2 3)"), 3);
}

#[test]
fn test_and_returns_first_falsy() {
    // (and 1 false 3) returns false (first falsy value)
    // Note: In Clojure, 0 is truthy, so we use false as the falsy value
    assert_eq!(observe("(and 1 false 3)"), Observation::Bool(false));
}

#[test]
fn test_and_empty() {
    // (and) returns true
    assert_eq!(observe("(and)"), Observation::Bool(true));
}

#[test]
fn test_and_single() {
    assert_eq!(run_expr_i32("(and 42)"), 42);
}

#[test]
fn test_or_all_false() {
    assert_eq!(observe("(or false false)"), Observation::Bool(false));
}

#[test]
fn test_or_one_true() {
    assert_eq!(observe("(or true false)"), Observation::Bool(true));
    assert_eq!(observe("(or false true)"), Observation::Bool(true));
}

#[test]
fn test_or_returns_first_truthy() {
    // In Clojure, 0 is truthy! So (or 0 0 5) returns 0 (first value is truthy)
    assert_eq!(run_expr_i32("(or 0 0 5)"), 0);
    // (or 3 5) returns 3 (first truthy)
    assert_eq!(run_expr_i32("(or 3 5)"), 3);
}

#[test]
fn test_or_returns_last_if_all_falsy() {
    // (or false false) returns false because false is falsy
    // With Clojure semantics, only nil and false are falsy
    assert_eq!(observe("(or false false)"), Observation::Bool(false));
}

#[test]
fn test_or_empty() {
    // (or) returns nil
    assert_eq!(observe("(or)"), Observation::Nil);
}

#[test]
fn test_or_single() {
    assert_eq!(run_expr_i32("(or 42)"), 42);
}

#[test]
fn test_not_true() {
    assert_eq!(observe("(not true)"), Observation::Bool(false));
}

#[test]
fn test_not_false() {
    assert_eq!(observe("(not false)"), Observation::Bool(true));
}

// ============================================================================
// Clojure Truthiness Tests
// ============================================================================
// In Clojure, only nil and false are falsy. Everything else is truthy,
// including 0, "", empty collections, etc.

#[test]
fn test_truthiness_zero_is_truthy() {
    // In Clojure, 0 is truthy (unlike WASM/JavaScript)
    assert_eq!(run_expr_i32("(if 0 1 2)"), 1);
}

#[test]
fn test_truthiness_negative_is_truthy() {
    assert_eq!(run_expr_i32("(if -1 1 2)"), 1);
}

#[test]
fn test_truthiness_positive_is_truthy() {
    assert_eq!(run_expr_i32("(if 42 1 2)"), 1);
}

#[test]
fn test_truthiness_float_zero_is_truthy() {
    // Even 0.0 is truthy in Clojure
    assert_eq!(run_expr_i32("(if 0.0 1 2)"), 1);
}

#[test]
fn test_truthiness_false_is_falsy() {
    assert_eq!(run_expr_i32("(if false 1 2)"), 2);
}

#[test]
fn test_truthiness_true_is_truthy() {
    assert_eq!(run_expr_i32("(if true 1 2)"), 1);
}

#[test]
fn test_truthiness_comparison_true() {
    // Comparison results (booleans) should work correctly
    assert_eq!(run_expr_i32("(if (> 5 3) 1 2)"), 1);
}

#[test]
fn test_truthiness_comparison_false() {
    assert_eq!(run_expr_i32("(if (< 5 3) 1 2)"), 2);
}

#[test]
fn test_truthiness_when_with_zero() {
    // (when 0 42) should return 42 since 0 is truthy
    assert_eq!(run_expr_i32("(when 0 42)"), 42);
}

#[test]
fn test_truthiness_cond_with_zero() {
    // (cond 0 1 :else 2) should return 1 since 0 is truthy
    assert_eq!(run_expr_i32("(cond 0 1 :else 2)"), 1);
}

#[test]
fn test_truthiness_and_with_zero() {
    // (and 0 1) - 0 is truthy so should return 1
    // Note: This works because `and` desugars to nested `if` where the condition
    // is a literal `0` (Type::I32), which we correctly treat as truthy.
    assert_eq!(run_expr_i32("(and 0 1)"), 1);
}

// Phase 8.6: Type propagation through locals now works.
// (or 0 1) correctly returns 0 (Clojure-style) because 0 is truthy.
#[test]
fn test_or_with_zero_is_truthy() {
    // In Clojure, 0 is truthy, so (or 0 1) returns 0
    assert_eq!(run_expr_i32("(or 0 1)"), 0);
    // (or 0 false) returns 0
    assert_eq!(run_expr_i32("(or 0 false)"), 0);
    // (or false 0) returns 0 (false is falsy, 0 is truthy)
    assert_eq!(run_expr_i32("(or false 0)"), 0);
}

// ============================================================================
// Comparison Operations Tests (8c)
// ============================================================================

#[test]
fn test_less_than_or_equal() {
    assert_eq!(observe("(<= 1 2)"), Observation::Bool(true));
    assert_eq!(observe("(<= 2 2)"), Observation::Bool(true));
    assert_eq!(observe("(<= 3 2)"), Observation::Bool(false));
}

#[test]
fn test_greater_than_or_equal() {
    assert_eq!(observe("(>= 3 2)"), Observation::Bool(true));
    assert_eq!(observe("(>= 2 2)"), Observation::Bool(true));
    assert_eq!(observe("(>= 1 2)"), Observation::Bool(false));
}

#[test]
fn test_not_equal() {
    assert_eq!(observe("(not= 1 2)"), Observation::Bool(true));
    assert_eq!(observe("(not= 2 2)"), Observation::Bool(false));
}

// ============================================================================
// Numeric Operations Tests (8d)
// ============================================================================

#[test]
fn test_inc() {
    assert_eq!(run_expr_i32("(inc 5)"), 6);
    assert_eq!(run_expr_i32("(inc 0)"), 1);
    assert_eq!(run_expr_i32("(inc -1)"), 0);
}

#[test]
fn test_dec() {
    assert_eq!(run_expr_i32("(dec 5)"), 4);
    assert_eq!(run_expr_i32("(dec 1)"), 0);
    assert_eq!(run_expr_i32("(dec 0)"), -1);
}

#[test]
fn test_abs() {
    assert_eq!(run_expr_i32("(abs 5)"), 5);
    assert_eq!(run_expr_i32("(abs -5)"), 5);
    assert_eq!(run_expr_i32("(abs 0)"), 0);
}

#[test]
fn test_min_two_args() {
    assert_eq!(run_expr_i32("(min 3 1)"), 1);
    assert_eq!(run_expr_i32("(min 1 3)"), 1);
    assert_eq!(run_expr_i32("(min 2 2)"), 2);
}

#[test]
fn test_min_multiple_args() {
    assert_eq!(run_expr_i32("(min 3 1 4)"), 1);
    assert_eq!(run_expr_i32("(min 5 2 8 1)"), 1);
}

#[test]
fn test_min_single_arg() {
    assert_eq!(run_expr_i32("(min 42)"), 42);
}

#[test]
fn test_max_two_args() {
    assert_eq!(run_expr_i32("(max 3 1)"), 3);
    assert_eq!(run_expr_i32("(max 1 3)"), 3);
    assert_eq!(run_expr_i32("(max 2 2)"), 2);
}

#[test]
fn test_max_multiple_args() {
    assert_eq!(run_expr_i32("(max 3 1 4)"), 4);
    assert_eq!(run_expr_i32("(max 5 2 8 1)"), 8);
}

#[test]
fn test_max_single_arg() {
    assert_eq!(run_expr_i32("(max 42)"), 42);
}

#[test]
fn test_mod() {
    assert_eq!(run_expr_i32("(mod 10 3)"), 1);
    assert_eq!(run_expr_i32("(mod 9 3)"), 0);
    assert_eq!(run_expr_i32("(mod 7 4)"), 3);
}

// ============================================================================
// Tail Call Optimization (TCO) Tests (Phase 9.2)
// ============================================================================
// These tests verify that tail-recursive functions are properly optimized
// with WASM's return_call instruction.

/// Helper to compile and run modules with function definitions
/// Compiles suss source with defn statements and runs the specified function
fn run_module_i32(source: &str, func_name: &str, args: &[i32]) -> i32 {
    use std::io::Write;

    // Create temp files
    let mut suss_file = tempfile::Builder::new()
        .suffix(".sus")
        .tempfile()
        .expect("failed to create temp suss file");
    suss_file.write_all(source.as_bytes()).expect("failed to write suss");

    // Create WIT with the function signature
    // For simplicity, assume all functions take and return s32
    let wit = match args.len() {
        0 => format!(r#"
package test:tco;
world tco {{
    export {}: func() -> s32;
}}
"#, func_name),
        1 => format!(r#"
package test:tco;
world tco {{
    export {}: func(a: s32) -> s32;
}}
"#, func_name),
        2 => format!(r#"
package test:tco;
world tco {{
    export {}: func(a: s32, b: s32) -> s32;
}}
"#, func_name),
        _ => panic!("run_module_i32 only supports 0-2 args"),
    };

    let mut wit_file = tempfile::Builder::new()
        .suffix(".wit")
        .tempfile()
        .expect("failed to create temp wit file");
    wit_file.write_all(wit.as_bytes()).expect("failed to write wit");

    // Compile
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler
        .compile_files(suss_file.path().to_str().unwrap(), wit_file.path().to_str().unwrap())
        .expect("compilation failed");

    // Create engine with tail call, GC, and exceptions support
    let mut config = wasmtime::Config::new();
    config.wasm_tail_call(true);
    config.wasm_component_model(true);
    config.wasm_gc(true);
    config.wasm_function_references(true);
    config.wasm_exceptions(true);
    let engine = Engine::new(&config).expect("engine creation failed");

    // Load as component
    let component = wasmtime::component::Component::new(&engine, &wasm_bytes)
        .expect("component creation failed");
    let mut linker = wasmtime::component::Linker::<()>::new(&engine);

    // Register the suss runtime interface (print-str is a no-op in tests)
    linker.instance("test:tco/suss")
        .expect("failed to create suss instance in linker")
        .func_wrap("print-str", |_store: wasmtime::StoreContextMut<'_, ()>, (_ptr, _len): (u32, u32)| -> wasmtime::Result<()> {
            Ok(())
        })
        .expect("failed to register print-str");

    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component)
        .expect("instantiation failed");

    // Get and call the function
    match args.len() {
        0 => {
            let func = instance
                .get_typed_func::<(), (i32,)>(&mut store, func_name)
                .expect("function not found");
            func.call(&mut store, ()).expect("call failed").0
        }
        1 => {
            let func = instance
                .get_typed_func::<(i32,), (i32,)>(&mut store, func_name)
                .expect("function not found");
            func.call(&mut store, (args[0],)).expect("call failed").0
        }
        2 => {
            let func = instance
                .get_typed_func::<(i32, i32), (i32,)>(&mut store, func_name)
                .expect("function not found");
            func.call(&mut store, (args[0], args[1])).expect("call failed").0
        }
        _ => unreachable!(),
    }
}

// TCO tests: WIT boundary marshaling converts between GC refs (eqref) and WIT primitives (s32)

#[test]
fn test_tco_tail_recursive_factorial() {
    // Tail-recursive factorial: factorial(n, acc)
    // The recursive call is in tail position
    let source = r#"
(defn ^:export factorial [n acc]
  (if (<= n 1)
    acc
    (factorial (dec n) (* n acc))))
"#;

    // factorial(5, 1) = 120
    assert_eq!(run_module_i32(source, "factorial", &[5, 1]), 120);
    // factorial(1, 1) = 1
    assert_eq!(run_module_i32(source, "factorial", &[1, 1]), 1);
    // factorial(0, 1) = 1
    assert_eq!(run_module_i32(source, "factorial", &[0, 1]), 1);
    // factorial(10, 1) = 3628800
    assert_eq!(run_module_i32(source, "factorial", &[10, 1]), 3628800);
}

#[test]
fn test_tco_tail_recursive_sum() {
    // Tail-recursive sum: sum(n, acc)
    // Sums 1 + 2 + ... + n
    let source = r#"
(defn ^:export sum [n acc]
  (if (<= n 0)
    acc
    (sum (dec n) (+ acc n))))
"#;

    // sum(5, 0) = 1+2+3+4+5 = 15
    assert_eq!(run_module_i32(source, "sum", &[5, 0]), 15);
    // sum(10, 0) = 55
    assert_eq!(run_module_i32(source, "sum", &[10, 0]), 55);
    // sum(100, 0) = 5050
    assert_eq!(run_module_i32(source, "sum", &[100, 0]), 5050);
}

#[test]
fn test_tco_deep_recursion() {
    // Test deep recursion that would stack overflow without TCO
    // This function just counts down to 0
    let source = r#"
(defn ^:export countdown [n]
  (if (<= n 0)
    0
    (countdown (dec n))))
"#;

    // With TCO, this should complete without stack overflow
    // Without TCO, this would fail with stack overflow
    assert_eq!(run_module_i32(source, "countdown", &[10000]), 0);
}

#[test]
fn test_tco_mutual_recursion_style() {
    // Test that the recursive call is properly detected in if branches
    let source = r#"
(defn ^:export gcd [a b]
  (if (= b 0)
    a
    (gcd b (mod a b))))
"#;

    // gcd(48, 18) = 6
    assert_eq!(run_module_i32(source, "gcd", &[48, 18]), 6);
    // gcd(100, 25) = 25
    assert_eq!(run_module_i32(source, "gcd", &[100, 25]), 25);
    // gcd(17, 13) = 1
    assert_eq!(run_module_i32(source, "gcd", &[17, 13]), 1);
}

// ============================================================================
// Hash Tests
// ============================================================================

#[test]
fn test_hash_nil() {
    assert_eq!(run_expr_i32("(hash nil)"), 0);
}

#[test]
fn test_hash_true() {
    // Java convention: true.hashCode() = 1231
    assert_eq!(run_expr_i32("(hash true)"), 1231);
}

#[test]
fn test_hash_false() {
    // Java convention: false.hashCode() = 1237
    assert_eq!(run_expr_i32("(hash false)"), 1237);
}

#[test]
fn test_hash_small_int() {
    // Small integers are their own hash (good distribution for i31ref range)
    assert_eq!(run_expr_i32("(hash 42)"), 42);
    assert_eq!(run_expr_i32("(hash 1)"), 1);
    assert_eq!(run_expr_i32("(hash 100)"), 100);
}

#[test]
fn test_hash_zero() {
    assert_eq!(run_expr_i32("(hash 0)"), 0);
}

#[test]
fn test_hash_negative() {
    // Negative small integers
    assert_eq!(run_expr_i32("(hash -1)"), -1);
    assert_eq!(run_expr_i32("(hash -5)"), -5);
    assert_eq!(run_expr_i32("(hash -100)"), -100);
}

#[test]
fn test_hash_float_deterministic() {
    // Float hashes should be deterministic
    // (1.5 hashed twice should give same result)
    let result1 = run_expr_i32("(hash 1.5)");
    let result2 = run_expr_i32("(hash 1.5)");
    assert_eq!(result1, result2);
    // Should be non-zero for non-zero floats
    assert_ne!(result1, 0);
}

#[test]
fn test_hash_float_different_values() {
    // Different floats with different bit patterns should have different hashes
    // NOTE: Current implementation only hashes low 32 bits of f64 representation,
    // so floats like 1.0 (0x3FF0_0000_0000_0000) and 2.0 (0x4000_0000_0000_0000)
    // have the same low 32 bits (0x00000000) and hash the same.
    // This will be fixed when we implement proper i64 hashing with local variables.
    //
    // For now, test floats with different low 32 bits:
    let h1 = run_expr_i32("(hash 1.5)");  // 0x3FF8_0000_0000_0000
    let h2 = run_expr_i32("(hash 1.25)"); // 0x3FF4_0000_0000_0000
    // These have the same low 32 bits (0), so they might collide
    // Instead, test that we get deterministic non-zero results
    assert_ne!(h1, 0);
    assert_ne!(h2, 0);
}

#[test]
fn test_hash_string_basic() {
    // Strings now use xxHash32 over their bytes.
    let h1 = run_expr_i32("(hash \"hello\")");
    let h2 = run_expr_i32("(hash \"world\")");
    let h3 = run_expr_i32("(hash \"hello\")"); // Same as h1

    // Same string should produce same hash
    assert_eq!(h1, h3);
    // Different strings should produce different hashes
    assert_ne!(h1, h2);
    // Non-zero hashes
    assert_ne!(h1, 0);
    assert_ne!(h2, 0);
}

#[test]
fn test_hash_string_empty() {
    // The pinned ClojureScript string hash of "" is 0 (fresh pinned observation).
    assert_eq!(run_expr_i32("(hash \"\")"), 0);
}

#[test]
fn test_hash_string_deterministic() {
    // Hash should be deterministic
    let h1 = run_expr_i32("(hash \"test\")");
    let h2 = run_expr_i32("(hash \"test\")");
    assert_eq!(h1, h2);
}

// ==============================================================================
// Vector/Collection Operations
// ==============================================================================

#[test]
fn test_nth_vector() {
    // nth on a vector literal should use fast path
    assert_eq!(run_expr_i32("(nth [10 20 30] 0)"), 10);
    assert_eq!(run_expr_i32("(nth [10 20 30] 1)"), 20);
    assert_eq!(run_expr_i32("(nth [10 20 30] 2)"), 30);
}

#[test]
fn test_count_vector() {
    // count on a vector literal should use fast path
    assert_eq!(run_expr_i32("(count [])"), 0);
    assert_eq!(run_expr_i32("(count [1])"), 1);
    assert_eq!(run_expr_i32("(count [1 2 3])"), 3);
}

// ============================================================================
// List (Cons) Operations
// ============================================================================

#[test]
fn test_cons_creates_list() {
    // cons creates a new list with val at front
    // We build lists using cons and nil
    assert_eq!(run_expr_i32("(first (cons 99 nil))"), 99);
    assert_eq!(run_expr_i32("(first (cons 1 (cons 2 nil)))"), 1);
}

#[test]
fn test_first_on_cons() {
    // first on a cons cell returns the car
    assert_eq!(run_expr_i32("(first (cons 42 nil))"), 42);
    assert_eq!(run_expr_i32("(first (cons 10 (cons 20 nil)))"), 10);
}

#[test]
fn test_rest_on_cons() {
    // rest on a cons cell returns the cdr
    // Check first of rest
    assert_eq!(run_expr_i32("(first (rest (cons 10 (cons 20 nil))))"), 20);
    assert_eq!(run_expr_i32("(first (rest (rest (cons 10 (cons 20 (cons 30 nil))))))"), 30);
}

#[test]
fn test_conj_on_vector() {
    // conj on a vector adds to the end
    // We verify by checking nth on the new vector
    assert_eq!(run_expr_i32("(nth (conj [1 2] 3) 2)"), 3);
}

#[test]
fn test_vector_32_elements() {
    // Build a vector with exactly 32 elements (boundary case)
    // Using loop to build [0, 1, 2, ..., 31]
    let expr = "(count (loop [v [] i 0] (if (< i 32) (recur (conj v i) (+ i 1)) v)))";
    assert_eq!(run_expr_i32(expr), 32);
}

#[test]
fn test_simple_loop_with_vector() {
    // Simple loop that just returns a vector (no conj)
    let expr = "(count (loop [v [1 2 3]] v))";
    assert_eq!(run_expr_i32(expr), 3);
}

#[test]
fn test_get_on_vector() {
    // get on a vector acts like nth
    assert_eq!(run_expr_i32("(get [10 20 30] 1)"), 20);
}

// =========================================================
// Polymorphic Collection Operations
// =========================================================

#[test]
fn test_polymorphic_first_on_vector() {
    // first should work on vectors via protocol dispatch
    assert_eq!(run_expr_i32("(first [42 1 2])"), 42);
    assert_eq!(run_expr_i32("(first [99])"), 99);
}

#[test]
fn test_polymorphic_first_on_empty_vector() {
    // first on empty vector returns nil
    assert_eq!(observe("(first [])"), Observation::Nil);
}

#[test]
fn test_polymorphic_count_on_list() {
    // count should work on cons lists via protocol dispatch
    assert_eq!(run_expr_i32("(count (cons 1 nil))"), 1);
    assert_eq!(run_expr_i32("(count (cons 1 (cons 2 nil)))"), 2);
    assert_eq!(run_expr_i32("(count (cons 1 (cons 2 (cons 3 nil))))"), 3);
}

#[test]
fn test_polymorphic_nth_on_list() {
    // nth should work on cons lists via protocol dispatch (O(n) traversal)
    assert_eq!(run_expr_i32("(nth (cons 10 (cons 20 (cons 30 nil))) 0)"), 10);
    assert_eq!(run_expr_i32("(nth (cons 10 (cons 20 (cons 30 nil))) 1)"), 20);
    assert_eq!(run_expr_i32("(nth (cons 10 (cons 20 (cons 30 nil))) 2)"), 30);
}

#[test]
fn test_polymorphic_mixed_operations() {
    // Verify operations work correctly on both types in same expression
    // first on vector + first on list
    let expr = "(+ (first [100]) (first (cons 23 nil)))";
    assert_eq!(run_expr_i32(expr), 123);
}

#[test]
fn test_polymorphic_count_dispatches_correctly() {
    // Verify count dispatches to the right implementation
    // Vector: O(1) field access
    // List: O(n) traversal
    assert_eq!(run_expr_i32("(count [1 2 3 4 5])"), 5);
    assert_eq!(run_expr_i32("(count (cons 1 (cons 2 (cons 3 (cons 4 (cons 5 nil))))))"), 5);
}

// =========================================================
// Loop/Recur Tests
// =========================================================

#[test]
fn test_simple_loop_sum() {
    // Sum 1 to 5 using loop/recur
    // (loop [acc 0 i 1] (if (<= i 5) (recur (+ acc i) (+ i 1)) acc))
    assert_eq!(run_expr_i32("(loop [acc 0 i 1] (if (<= i 5) (recur (+ acc i) (+ i 1)) acc))"), 15);
}

#[test]
fn test_simple_loop_countdown() {
    // Count down from 10 to 0
    assert_eq!(run_expr_i32("(loop [i 10] (if (> i 0) (recur (- i 1)) i))"), 0);
}

// =========================================================
// Recur Validation Tests (ClojureScript semantics)
// =========================================================

#[test]
fn test_recur_not_in_tail_position_rejected() {
    // recur as argument to + is NOT in tail position - should be rejected
    let mut compiler = Compiler::new();
    let result = compiler.compile_expr_with_info("(loop [x 0] (+ 1 (recur x)))");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("tail position"), "Expected 'tail position' error, got: {}", err_msg);
}

#[test]
fn test_recur_outside_loop_rejected() {
    // recur outside of loop should be rejected
    let mut compiler = Compiler::new();
    let result = compiler.compile_expr_with_info("(recur 1)");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("requires an enclosing loop or function"), "Expected recur target error, got: {}", err_msg);
}

#[test]
fn test_recur_in_let_binding_rejected() {
    // recur in let binding (not tail position) should be rejected
    let mut compiler = Compiler::new();
    let result = compiler.compile_expr_with_info("(loop [x 0] (let [y (recur (+ x 1))] y))");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("tail position"), "Expected 'tail position' error, got: {}", err_msg);
}

#[test]
fn test_recur_in_if_branches_allowed() {
    // recur in if branch IS in tail position - should work
    assert_eq!(run_expr_i32("(loop [x 0] (if (< x 5) (recur (+ x 1)) x))"), 5);
}

#[test]
fn test_recur_in_do_last_expr_allowed() {
    // recur in last expression of do IS in tail position
    assert_eq!(run_expr_i32("(loop [x 0] (do (+ 1 1) (if (< x 3) (recur (+ x 1)) x)))"), 3);
}

#[test]
fn test_recur_in_do_non_last_rejected() {
    // recur NOT in last expression of do is NOT in tail position
    let mut compiler = Compiler::new();
    let result = compiler.compile_expr_with_info("(loop [x 0] (do (recur (+ x 1)) x))");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("tail position"), "Expected 'tail position' error, got: {}", err_msg);
}

#[test]
fn test_recur_in_let_body_allowed() {
    // recur in let body IS in tail position - should work
    assert_eq!(run_expr_i32("(loop [x 0] (let [y (+ x 1)] (if (< y 5) (recur y) y)))"), 5);
}

// =========================================================
// Large Vector Tests (>32 elements - trie required)
// =========================================================

/// Helper to build vector expression of size n: [0, 1, 2, ..., n-1]
fn build_vector_expr(n: usize) -> String {
    format!(
        "(loop [v [] i 0] (if (< i {}) (recur (conj v i) (+ i 1)) v))",
        n
    )
}

#[test]
fn test_large_vector_count_33() {
    // Vector with 33 elements crosses the 32-element boundary into trie territory
    let expr = format!("(count {})", build_vector_expr(33));
    assert_eq!(run_expr_i32(&expr), 33);
}

#[test]
fn test_large_vector_nth_first_element() {
    // Access first element in a vector with 33 elements
    let expr = format!("(nth {} 0)", build_vector_expr(33));
    assert_eq!(run_expr_i32(&expr), 0);
}

#[test]
fn test_large_vector_nth_32nd_element() {
    // Access element at index 32 (the 33rd element, first one that requires trie)
    let expr = format!("(nth {} 32)", build_vector_expr(33));
    assert_eq!(run_expr_i32(&expr), 32);
}

#[test]
fn test_large_vector_count_100() {
    // Larger vector
    let expr = format!("(count {})", build_vector_expr(100));
    assert_eq!(run_expr_i32(&expr), 100);
}

#[test]
fn test_large_vector_nth_middle() {
    // Access element in the middle of a large vector
    let expr = format!("(nth {} 50)", build_vector_expr(100));
    assert_eq!(run_expr_i32(&expr), 50);
}

#[test]
fn test_large_vector_nth_last() {
    // Access last element
    let expr = format!("(nth {} 99)", build_vector_expr(100));
    assert_eq!(run_expr_i32(&expr), 99);
}

#[test]
fn test_structural_sharing() {
    // Verify both old and new vector work after conj
    let expr = format!(
        "(let [v1 {} v2 (conj v1 33)] (+ (nth v1 32) (nth v2 33)))",
        build_vector_expr(33)
    );
    assert_eq!(run_expr_i32(&expr), 65); // 32 + 33
}

#[test]
fn test_conj_past_32_boundary() {
    // Verify transition from tail-only to trie works correctly
    let expr = format!("(count (conj {} 32))", build_vector_expr(32));
    assert_eq!(run_expr_i32(&expr), 33);
}

// =========================================================
// HAMT Map Tests
// =========================================================

#[test]
fn test_map_get_multiple_entries() {
    // Get from map with multiple entries
    assert_eq!(run_expr_i32("(get {1 10 2 20 3 30} 2)"), 20);
}

#[test]
fn test_map_get_first_entry() {
    assert_eq!(run_expr_i32("(get {1 10 2 20 3 30} 1)"), 10);
}

#[test]
fn test_map_get_last_entry() {
    assert_eq!(run_expr_i32("(get {1 10 2 20 3 30} 3)"), 30);
}

#[test]
fn test_map_get_missing_key() {
    // Missing key returns nil
    assert_eq!(observe("(get {1 2 3 4} 5)"), Observation::Nil);
}

#[test]
fn test_map_assoc_new_key() {
    // Assoc adds a new key-value pair
    assert_eq!(run_expr_i32("(get (assoc {1 2} 3 4) 3)"), 4);
}

#[test]
fn test_map_assoc_preserves_existing() {
    // Assoc preserves existing entries
    assert_eq!(run_expr_i32("(get (assoc {1 2} 3 4) 1)"), 2);
}

#[test]
fn test_map_assoc_update_existing() {
    // Assoc updates existing key
    assert_eq!(run_expr_i32("(get (assoc {1 2} 1 99) 1)"), 99);
}

#[test]
fn test_map_count() {
    assert_eq!(run_expr_i32("(count {1 2 3 4 5 6})"), 3);
}

#[test]
fn test_map_count_empty() {
    assert_eq!(run_expr_i32("(count {})"), 0);
}

#[test]
fn test_map_let_bound_get() {
    // Tests protocol dispatch for map lookup
    assert_eq!(run_expr_i32("(let [m {1 2}] (get m 1))"), 2);
}

#[test]
fn test_map_let_bound_assoc() {
    // Tests protocol dispatch with assoc
    assert_eq!(run_expr_i32("(let [m {1 2}] (get (assoc m 3 4) 3))"), 4);
}

#[test]
fn test_map_chained_assoc() {
    // Multiple assoc operations
    assert_eq!(
        run_expr_i32("(get (assoc (assoc {1 2} 3 4) 5 6) 5)"),
        6
    );
}

// =========================================================
// HAMT Set Tests
// =========================================================

#[test]
fn test_set_contains_true() {
    // Element is in set
    assert_eq!(run_expr_bool("(contains? #{1 2 3} 2)"), true);
}

#[test]
fn test_set_contains_false() {
    // Element is not in set
    assert_eq!(run_expr_bool("(contains? #{1 2 3} 5)"), false);
}

#[test]
fn test_set_contains_empty() {
    // Empty set never contains anything
    assert_eq!(run_expr_bool("(contains? #{} 1)"), false);
}

#[test]
fn test_set_conj_new_element() {
    // Conj adds a new element
    assert_eq!(run_expr_bool("(contains? (conj #{1 2} 3) 3)"), true);
}

#[test]
fn test_set_conj_preserves_existing() {
    // Conj preserves existing elements
    assert_eq!(run_expr_bool("(contains? (conj #{1 2} 3) 1)"), true);
}

#[test]
fn test_set_count() {
    assert_eq!(run_expr_i32("(count #{1 2 3 4 5})"), 5);
}

#[test]
fn test_set_count_empty() {
    assert_eq!(run_expr_i32("(count #{})"), 0);
}

#[test]
fn test_set_let_bound_contains() {
    // Tests protocol dispatch for set contains
    assert_eq!(run_expr_bool("(let [s #{1 2 3}] (contains? s 2))"), true);
}

#[test]
fn test_set_let_bound_conj() {
    // Tests protocol dispatch with conj
    assert_eq!(
        run_expr_bool("(let [s #{1 2}] (contains? (conj s 3) 3))"),
        true
    );
}

#[test]
fn test_set_chained_conj() {
    // Multiple conj operations
    assert_eq!(
        run_expr_i32("(count (conj (conj (conj #{} 1) 2) 3))"),
        3
    );
}

// =========================================================
// Set disj Tests
// =========================================================

#[test]
fn test_set_disj_compiles() {
    // disj removes element from set, reducing count
    assert_eq!(run_expr_i32("(count (disj #{1 2 3} 2))"), 2);
}

#[test]
fn test_set_disj_on_empty() {
    // disj on empty set returns empty set
    assert_eq!(run_expr_i32("(count (disj #{} 1))"), 0);
}

#[test]
fn test_set_disj_nonexistent_element() {
    // Stub: disj on nonexistent element returns original (correct behavior)
    assert_eq!(run_expr_i32("(count (disj #{1 2 3} 99))"), 3);
}

// =========================================================
// Map dissoc Tests
// =========================================================

#[test]
fn test_map_dissoc_compiles() {
    // dissoc removes key from map, reducing count
    assert_eq!(run_expr_i32("(count (dissoc {1 2 3 4} 1))"), 1);
}

#[test]
fn test_map_dissoc_on_empty() {
    // dissoc on empty map returns empty map
    assert_eq!(run_expr_i32("(count (dissoc {} 1))"), 0);
}

#[test]
fn test_map_dissoc_nonexistent_key() {
    // Stub: dissoc on nonexistent key returns original (correct behavior)
    assert_eq!(run_expr_i32("(count (dissoc {1 2 3 4} 99))"), 2);
}

// =========================================================
// Large Map Tests (>16 entries - HAMT branching required)
// =========================================================

/// Helper to build map expression of size n: {0 0, 1 1, 2 2, ..., n-1 n-1}
fn build_map_expr(n: usize) -> String {
    format!(
        "(loop [m {{}} i 0] (if (< i {}) (recur (assoc m i i) (+ i 1)) m))",
        n
    )
}

#[test]
fn test_large_map_count_50() {
    // Map with 50 entries exercises HAMT branching
    let expr = format!("(count {})", build_map_expr(50));
    assert_eq!(run_expr_i32(&expr), 50);
}

#[test]
fn test_large_map_count_100() {
    // Map with 100 entries exercises deeper HAMT tree
    let expr = format!("(count {})", build_map_expr(100));
    assert_eq!(run_expr_i32(&expr), 100);
}

#[test]
fn test_large_map_get_first() {
    // Get first key from large map
    let expr = format!("(get {} 0)", build_map_expr(100));
    assert_eq!(run_expr_i32(&expr), 0);
}

#[test]
fn test_large_map_get_middle() {
    // Get key in middle of large map
    let expr = format!("(get {} 50)", build_map_expr(100));
    assert_eq!(run_expr_i32(&expr), 50);
}

#[test]
fn test_large_map_get_last() {
    // Get last key from large map
    let expr = format!("(get {} 99)", build_map_expr(100));
    assert_eq!(run_expr_i32(&expr), 99);
}

#[test]
fn test_large_map_get_missing() {
    // Get missing key from large map returns nil
    let expr = format!("(get {} 999)", build_map_expr(100));
    assert_eq!(observe(&expr), Observation::Nil);
}

#[test]
fn test_large_map_assoc_update() {
    // Update existing key in large map
    let expr = format!("(get (assoc {} 50 999) 50)", build_map_expr(100));
    assert_eq!(run_expr_i32(&expr), 999);
}

#[test]
fn test_large_map_assoc_preserves() {
    // Verify assoc on large map preserves other entries
    let expr = format!("(get (assoc {} 50 999) 49)", build_map_expr(100));
    assert_eq!(run_expr_i32(&expr), 49);
}

#[test]
fn test_map_structural_sharing() {
    // Verify original map is unchanged after assoc
    let expr = format!(
        "(let [m1 {} m2 (assoc m1 50 999)] (+ (get m1 50) (get m2 50)))",
        build_map_expr(100)
    );
    assert_eq!(run_expr_i32(&expr), 1049); // 50 + 999
}

// =========================================================
// Large Set Tests (>16 elements - HAMT branching required)
// =========================================================

/// Helper to build set expression of size n: #{0 1 2 ... n-1}
fn build_set_expr(n: usize) -> String {
    format!(
        "(loop [s #{{}} i 0] (if (< i {}) (recur (conj s i) (+ i 1)) s))",
        n
    )
}

#[test]
fn test_large_set_count_50() {
    // Set with 50 elements exercises HAMT branching
    let expr = format!("(count {})", build_set_expr(50));
    assert_eq!(run_expr_i32(&expr), 50);
}

#[test]
fn test_large_set_count_100() {
    // Set with 100 elements exercises deeper HAMT tree
    let expr = format!("(count {})", build_set_expr(100));
    assert_eq!(run_expr_i32(&expr), 100);
}

#[test]
fn test_large_set_contains_first() {
    // Check first element in large set
    let expr = format!("(contains? {} 0)", build_set_expr(100));
    assert_eq!(run_expr_bool(&expr), true);
}

#[test]
fn test_large_set_contains_middle() {
    // Check element in middle of large set
    let expr = format!("(contains? {} 50)", build_set_expr(100));
    assert_eq!(run_expr_bool(&expr), true);
}

#[test]
fn test_large_set_contains_last() {
    // Check last element in large set
    let expr = format!("(contains? {} 99)", build_set_expr(100));
    assert_eq!(run_expr_bool(&expr), true);
}

#[test]
fn test_large_set_contains_missing() {
    // Check missing element in large set
    let expr = format!("(contains? {} 999)", build_set_expr(100));
    assert_eq!(run_expr_bool(&expr), false);
}

#[test]
fn test_large_set_conj_new() {
    // Add new element to large set
    let expr = format!("(contains? (conj {} 999) 999)", build_set_expr(100));
    assert_eq!(run_expr_bool(&expr), true);
}

#[test]
fn test_large_set_conj_preserves() {
    // Verify conj preserves existing elements
    let expr = format!("(contains? (conj {} 999) 50)", build_set_expr(100));
    assert_eq!(run_expr_bool(&expr), true);
}

#[test]
fn test_set_structural_sharing() {
    // Verify original set is unchanged after conj
    let expr = format!(
        "(let [s1 {} s2 (conj s1 999)] (if (contains? s1 999) 1 (if (contains? s2 999) 2 0)))",
        build_set_expr(100)
    );
    assert_eq!(run_expr_i32(&expr), 2); // s1 doesn't have 999, s2 does
}

// =========================================================
// First-Class Functions (Closures) Tests
// =========================================================

#[test]
fn test_closure_simple() {
    // Basic immediately-invoked closure
    assert_eq!(run_expr_i32("((fn [x] (+ x 1)) 5)"), 6);
}

#[test]
fn test_closure_with_capture() {
    // Closure capturing a local variable
    assert_eq!(run_expr_i32("(let [x 10] ((fn [y] (+ x y)) 5))"), 15);
}

#[test]
fn test_closure_as_value() {
    // Closure stored in a let binding and then called
    assert_eq!(run_expr_i32("(let [f (fn [x] (* x 2))] (f 21))"), 42);
}

#[test]
fn test_closure_multiple_params() {
    // Closure with multiple parameters
    assert_eq!(run_expr_i32("((fn [a b c] (+ a (+ b c))) 1 2 3)"), 6);
}

#[test]
fn test_closure_nested_capture() {
    // Nested let with capture
    assert_eq!(
        run_expr_i32("(let [a 10] (let [b 20] ((fn [c] (+ a (+ b c))) 5)))"),
        35
    );
}

#[test]
fn test_closure_zero_arity() {
    // Closure with no parameters
    assert_eq!(run_expr_i32("((fn [] 42))"), 42);
}

#[test]
fn test_closure_zero_arity_with_capture() {
    // Zero-arity closure capturing a variable
    assert_eq!(run_expr_i32("(let [x 99] ((fn [] x)))"), 99);
}

#[test]
fn test_closure_call_twice() {
    // Same closure called multiple times
    assert_eq!(
        run_expr_i32("(let [f (fn [x] (+ x 1))] (+ (f 10) (f 20)))"),
        32
    );
}

#[test]
fn test_closure_capture_multiple_vars() {
    // Capture multiple variables
    assert_eq!(
        run_expr_i32("(let [a 1 b 2 c 3] ((fn [d] (+ a (+ b (+ c d)))) 4))"),
        10
    );
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name constantly; see prototype_cases_awaiting_compiled_support"]
fn test_variadic_closure_with_capture_zero_args() {
    // Variadic closure with capture, called with zero args
    // This was a bug: CLOSURE_1 (arity=1 for args array) was incorrectly
    // cast to CLOSURE_0 at call site. Fixed by VARIADIC_CAPTURE type_id.
    assert_eq!(run_expr_i32("((constantly 42))"), 42);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name constantly; see prototype_cases_awaiting_compiled_support"]
fn test_variadic_closure_with_capture_multiple_args() {
    // Variadic closure with capture, called with multiple args
    assert_eq!(run_expr_i32("((constantly 99) 1 2 3)"), 99);
}

#[test]
fn test_variadic_closure_inline_zero_args() {
    // Inline variadic closure with capture, called with zero args
    assert_eq!(run_expr_i32("(let [v 100] (let [f (fn [& xs] v)] (f)))"), 100);
}

#[test]
fn test_immediate_variadic_fn_call_count() {
    // Immediate variadic fn call: rest params are PersistentVectors
    assert_eq!(run_expr_i32("((fn [& xs] (count xs)) 1 2 3)"), 3);
    assert_eq!(run_expr_i32("((fn [& xs] (count xs)))"), 0);
    assert_eq!(run_expr_i32("((fn [& xs] (count xs)) 1)"), 1);
}

#[test]
fn test_immediate_variadic_fn_call_nth() {
    // Verify args are accessible with nth (rest params are PersistentVectors)
    assert_eq!(run_expr_i32("((fn [& xs] (nth xs 0)) 10 20 30)"), 10);
    assert_eq!(run_expr_i32("((fn [& xs] (nth xs 1)) 10 20 30)"), 20);
    assert_eq!(run_expr_i32("((fn [& xs] (nth xs 2)) 10 20 30)"), 30);
}

#[test]
fn test_immediate_variadic_fn_with_fixed_params() {
    // Variadic with fixed params: (fn [a b & rest] ...)
    assert_eq!(run_expr_i32("((fn [a b & rest] a) 1 2 3 4)"), 1);
    assert_eq!(run_expr_i32("((fn [a b & rest] b) 1 2 3 4)"), 2);
    assert_eq!(run_expr_i32("((fn [a b & rest] (count rest)) 1 2 3 4)"), 2);
}

// ============================================================================
// Built-in Function Values
// ============================================================================

#[test]
fn test_builtin_as_value_add() {
    // Use + as a value (not in call position)
    assert_eq!(run_expr_i32("(let [f +] (f 1 2))"), 3);
}

#[test]
fn test_builtin_as_value_mul() {
    // Use * as a value
    assert_eq!(run_expr_i32("(let [f *] (f 3 4))"), 12);
}

#[test]
fn test_builtin_as_value_sub() {
    // Use - as a value
    assert_eq!(run_expr_i32("(let [f -] (f 10 3))"), 7);
}

#[test]
fn test_builtin_as_value_div() {
    // Use / as a value (division always returns float)
    assert_eq!(run_expr_f64("(let [f /] (f 20 4))"), 5.0);
}

#[test]
fn test_builtin_as_value_inc() {
    // Use inc as a value
    assert_eq!(run_expr_i32("(let [f inc] (f 41))"), 42);
}

#[test]
fn test_builtin_as_value_dec() {
    // Use dec as a value
    assert_eq!(run_expr_i32("(let [f dec] (f 43))"), 42);
}

#[test]
fn test_builtin_conditional_selection() {
    // Select built-in based on condition
    assert_eq!(
        run_expr_i32("(let [op (if true + -)] (op 10 3))"),
        13
    );
    assert_eq!(
        run_expr_i32("(let [op (if false + -)] (op 10 3))"),
        7
    );
}

#[test]
fn test_builtin_same_function_twice() {
    // Using + twice in value position should reuse the wrapper
    assert_eq!(
        run_expr_i32("(let [f + g +] (f 1 (g 2 3)))"),
        6
    );
}

#[test]
fn test_builtin_comparison_as_value() {
    // Use comparison operator as a value
    assert_eq!(
        run_expr_i32("(let [cmp <] (if (cmp 3 5) 1 0))"),
        1
    );
    assert_eq!(
        run_expr_i32("(let [cmp >] (if (cmp 3 5) 1 0))"),
        0
    );
}

// ============================================================================
// Apply - Dynamic Function Invocation
// ============================================================================

#[test]
fn test_apply_basic_add() {
    // Apply with built-in +
    assert_eq!(run_expr_i32("(apply + [1 2])"), 3);
}

#[test]
fn test_apply_basic_mul() {
    // Apply with built-in *
    assert_eq!(run_expr_i32("(apply * [3 4])"), 12);
}

#[test]
fn test_apply_closure() {
    // Apply with user-defined closure
    assert_eq!(run_expr_i32("(apply (fn [a b] (+ a b)) [10 5])"), 15);
}

#[test]
fn test_apply_zero_arity() {
    // Apply with zero-arity closure
    assert_eq!(run_expr_i32("(apply (fn [] 42) [])"), 42);
}

#[test]
fn test_apply_one_arity() {
    // Apply with single-arg closure
    assert_eq!(run_expr_i32("(apply (fn [x] (+ x 10)) [32])"), 42);
}

#[test]
fn test_apply_captured_closure() {
    // Apply with closure that captures a variable
    assert_eq!(
        run_expr_i32("(let [x 10] (apply (fn [y] (+ x y)) [5]))"),
        15
    );
}

#[test]
fn test_apply_closure_as_value() {
    // Apply with closure stored in a let binding
    assert_eq!(
        run_expr_i32("(let [f (fn [a b] (* a b))] (apply f [6 7]))"),
        42
    );
}

#[test]
fn test_apply_vector_from_expr() {
    // Apply with vector constructed from expression
    assert_eq!(
        run_expr_i32("(let [v [3 4]] (apply + v))"),
        7
    );
}

#[test]
fn test_apply_three_args() {
    // Apply with 3-arity function
    assert_eq!(
        run_expr_i32("(apply (fn [a b c] (+ a (+ b c))) [1 2 3])"),
        6
    );
}

// =============================================================================
// Variadic Arithmetic Edge Cases (Part 1)
// =============================================================================

#[test]
fn test_add_zero_args() {
    // (+) returns identity element 0
    assert_eq!(run_expr_i32("(+)"), 0);
}

#[test]
fn test_mul_zero_args() {
    // (*) returns identity element 1
    assert_eq!(run_expr_i32("(*)"), 1);
}

#[test]
fn test_unary_negate() {
    // (- x) negates x
    assert_eq!(run_expr_i32("(- 5)"), -5);
    assert_eq!(run_expr_i32("(- 0)"), 0);
    assert_eq!(run_expr_i32("(- -3)"), 3);
}

#[test]
fn test_unary_reciprocal() {
    // (/ x) returns 1/x
    assert_eq!(run_expr_f64("(/ 2.0)"), 0.5);
    assert_eq!(run_expr_f64("(/ 4.0)"), 0.25);
    assert_eq!(run_expr_f64("(/ 0.5)"), 2.0);
}

#[test]
fn test_add_single_arg() {
    // (+ x) returns x unchanged
    assert_eq!(run_expr_i32("(+ 42)"), 42);
}

#[test]
fn test_mul_single_arg() {
    // (* x) returns x unchanged
    assert_eq!(run_expr_i32("(* 42)"), 42);
}

#[test]
fn test_variadic_add_chain() {
    // Verify multi-arg addition still works
    assert_eq!(run_expr_i32("(+ 1 2 3 4 5)"), 15);
    assert_eq!(run_expr_i32("(+ 10 20 30)"), 60);
}

#[test]
fn test_variadic_mul_chain() {
    // Verify multi-arg multiplication still works
    assert_eq!(run_expr_i32("(* 2 3 4)"), 24);
    assert_eq!(run_expr_i32("(* 1 2 3 4 5)"), 120);
}

#[test]
fn test_variadic_sub_chain() {
    // (- a b c) = a - b - c (left-fold)
    assert_eq!(run_expr_i32("(- 10 3 2)"), 5);
    assert_eq!(run_expr_i32("(- 100 10 20 30)"), 40);
}

#[test]
fn test_variadic_div_chain() {
    // (/ a b c) = a / b / c (left-fold)
    assert_eq!(run_expr_f64("(/ 24.0 2.0 3.0)"), 4.0);
    assert_eq!(run_expr_f64("(/ 100.0 2.0 5.0)"), 10.0);
}

// =============================================================================
// Variadic Apply Tests (Part 2)
// =============================================================================

#[test]
fn test_apply_variadic_add_zero_args() {
    // (apply + []) returns identity 0
    assert_eq!(run_expr_i32("(apply + [])"), 0);
}

#[test]
fn test_apply_variadic_add_one_arg() {
    // (apply + [x]) returns x
    assert_eq!(run_expr_i32("(apply + [42])"), 42);
}

#[test]
fn test_apply_variadic_add_many_args() {
    // (apply + [a b c ...]) sums all
    assert_eq!(run_expr_i32("(apply + [1 2 3])"), 6);
    assert_eq!(run_expr_i32("(apply + [1 2 3 4 5])"), 15);
    assert_eq!(run_expr_i32("(apply + [10 20 30 40])"), 100);
}

#[test]
fn test_apply_variadic_mul_zero_args() {
    // (apply * []) returns identity 1
    assert_eq!(run_expr_i32("(apply * [])"), 1);
}

#[test]
fn test_apply_variadic_mul_one_arg() {
    // (apply * [x]) returns x
    assert_eq!(run_expr_i32("(apply * [7])"), 7);
}

#[test]
fn test_apply_variadic_mul_many_args() {
    // (apply * [a b c ...]) multiplies all
    assert_eq!(run_expr_i32("(apply * [2 3 4])"), 24);
    assert_eq!(run_expr_i32("(apply * [1 2 3 4 5])"), 120);
}

#[test]
fn test_apply_variadic_sub_one_arg() {
    // (apply - [x]) negates x
    assert_eq!(run_expr_i32("(apply - [5])"), -5);
    assert_eq!(run_expr_i32("(apply - [-3])"), 3);
}

#[test]
fn test_apply_variadic_sub_two_args() {
    // (apply - [a b]) = a - b
    assert_eq!(run_expr_i32("(apply - [10 3])"), 7);
}

#[test]
fn test_apply_variadic_sub_many_args() {
    // (apply - [a b c ...]) = a - b - c - ... (left-fold)
    assert_eq!(run_expr_i32("(apply - [10 3 2])"), 5);
    assert_eq!(run_expr_i32("(apply - [100 10 20 30])"), 40);
}

#[test]
fn test_apply_variadic_div_one_arg() {
    // (apply / [x]) = 1/x (reciprocal)
    assert_eq!(run_expr_f64("(apply / [2.0])"), 0.5);
    assert_eq!(run_expr_f64("(apply / [4.0])"), 0.25);
}

#[test]
fn test_apply_variadic_div_two_args() {
    // (apply / [a b]) = a / b
    assert_eq!(run_expr_f64("(apply / [12.0 3.0])"), 4.0);
}

#[test]
fn test_apply_variadic_div_many_args() {
    // (apply / [a b c ...]) = a / b / c / ... (left-fold)
    assert_eq!(run_expr_f64("(apply / [24.0 2.0 3.0])"), 4.0);
    assert_eq!(run_expr_f64("(apply / [100.0 2.0 5.0])"), 10.0);
}

#[test]
fn test_apply_variadic_let_bound() {
    // Variadic builtin bound to variable then applied
    assert_eq!(run_expr_i32("(let [f +] (apply f [1 2 3 4]))"), 10);
    assert_eq!(run_expr_i32("(let [f *] (apply f [2 3 4]))"), 24);
}

// ============================================================
// Keyword 3-arg form tests
// ============================================================

#[test]
fn test_keyword_3arg_with_default() {
    // (:key map default) when key exists
    let result = run_expr_i32("(:a {:a 42} 0)");
    assert_eq!(result, 42);
}

#[test]
fn test_keyword_3arg_with_default_missing() {
    // (:key map default) when key is missing - should return default
    let result = run_expr_i32("(:b {:a 42} 99)");
    assert_eq!(result, 99);
}

// ============================================================
// Threading variant macro tests
// ============================================================

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name cond->; see prototype_cases_awaiting_compiled_support"]
fn test_cond_thread_first() {
    // (cond-> 1 true inc true inc) should be 3
    let result = run_expr_i32("(cond-> 1 true inc true inc)");
    assert_eq!(result, 3);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name cond->; see prototype_cases_awaiting_compiled_support"]
fn test_cond_thread_first_false() {
    // (cond-> 1 true inc false inc) should be 2 (second clause skipped)
    let result = run_expr_i32("(cond-> 1 true inc false inc)");
    assert_eq!(result, 2);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name some->; see prototype_cases_awaiting_compiled_support"]
fn test_some_thread_first() {
    // (some-> 1 inc inc) should be 3
    let result = run_expr_i32("(some-> 1 inc inc)");
    assert_eq!(result, 3);
}

#[test]
fn test_as_thread() {
    // (as-> 1 x (+ x 2) (+ x 10)) should be 13
    let result = run_expr_i32("(as-> 1 x (+ x 2) (+ x 10))");
    assert_eq!(result, 13);
}

// ============================================================
// Vector destructuring tests
// ============================================================

#[test]
#[ignore = "compiled pipeline: Binding destructuring is not lowered yet; see prototype_cases_awaiting_compiled_support"]
fn test_vector_destructuring_let() {
    let result = run_expr_i32("(let [[a b] [10 20]] (+ a b))");
    assert_eq!(result, 30);
}

#[test]
#[ignore = "compiled pipeline: Binding destructuring is not lowered yet; see prototype_cases_awaiting_compiled_support"]
fn test_vector_destructuring_rest() {
    // [a & rest] destructuring - count of rest
    let result = run_expr_i32("(let [[a & rest] [1 2 3]] (count rest))");
    assert_eq!(result, 2);
}

// ============================================================
// Map destructuring tests
// ============================================================

#[test]
#[ignore = "compiled pipeline: Binding destructuring is not lowered yet; see prototype_cases_awaiting_compiled_support"]
fn test_map_destructuring_keys() {
    // {:keys [a b]} destructuring
    let result = run_expr_i32("(let [{:keys [a b]} {:a 10 :b 20}] (+ a b))");
    assert_eq!(result, 30);
}

#[test]
#[ignore = "compiled pipeline: Binding destructuring is not lowered yet; see prototype_cases_awaiting_compiled_support"]
fn test_map_destructuring_direct() {
    // {sym :key} direct binding
    let result = run_expr_i32("(let [{x :a y :b} {:a 10 :b 20}] (+ x y))");
    assert_eq!(result, 30);
}

// ============================================================
// Fn map destructuring tests
// ============================================================

#[test]
#[ignore = "compiled pipeline: Parameter destructuring is not lowered yet; see prototype_cases_awaiting_compiled_support"]
fn test_fn_map_destructuring_keys() {
    let result = run_expr_i32("((fn [{:keys [a b]}] (+ a b)) {:a 10 :b 20})");
    assert_eq!(result, 30);
}

#[test]
#[ignore = "compiled pipeline: Parameter destructuring is not lowered yet; see prototype_cases_awaiting_compiled_support"]
fn test_defn_map_destructuring() {
    let result = run_expr_i32("(defn foo [{:keys [x y]}] (+ x y)) (foo {:x 3 :y 7})");
    assert_eq!(result, 10);
}

#[test]
#[ignore = "compiled pipeline: Parameter destructuring is not lowered yet; see prototype_cases_awaiting_compiled_support"]
fn test_fn_map_destructuring_as() {
    let result = run_expr_i32("((fn [{:keys [a] :as m}] (+ a (count m))) {:a 10 :b 20})");
    assert_eq!(result, 12);
}

#[test]
#[ignore = "compiled pipeline: Parameter destructuring is not lowered yet; see prototype_cases_awaiting_compiled_support"]
fn test_multi_arity_map_destructuring() {
    // Arity 0 returns constant, arity 1 uses map destructuring
    let result = run_expr_i32("(let [f (fn ([] 0) ([{:keys [a b]}] (+ a b)))] (f {:a 5 :b 15}))");
    assert_eq!(result, 20);
}

// ============================================================
// try/catch/throw tests
// ============================================================

#[test]
fn test_try_no_exception() {
    // try without throwing - should return body value
    let result = run_expr_i32("(try 42 (catch :default e 0))");
    assert_eq!(result, 42);
}

#[test]
fn test_try_catch_throw() {
    // throw inside try - should catch and return catch body value
    let result = run_expr_i32("(try (throw 99) (catch :default e e))");
    assert_eq!(result, 99);
}

#[test]
fn test_try_catch_with_expressions() {
    // Compute in both branches
    let result = run_expr_i32("(try (do (+ 1 2) (throw 10)) (catch :default e (+ e 5)))");
    assert_eq!(result, 15);
}

#[test]
fn test_finally_preserves_result_and_runs_on_throw() {
    assert_eq!(run_expr_i32("(try 42 (finally 7))"), 42);
    assert_eq!(run_expr_i32("(let [a (atom 0)] (try (try (throw 9) (finally (swap! a inc))) (catch :default e (+ e (deref a)))))"), 10);
    assert_eq!(run_expr_i32("(let [a (atom 0)] (try (try (throw 9) (catch :default e (throw 11)) (finally (swap! a inc))) (catch :default e (+ e (deref a)))))"), 12);
    assert_eq!(run_expr_i32("(try (try (throw 9) (finally (throw 13))) (catch :default e e))"), 13);
    assert_eq!(run_expr_i32("(let [a (atom 0)] (let [result (try 42 (catch :default e 0) (finally (swap! a inc)))] (+ result (deref a))))"), 43);
}

// ============================================================
// Atom tests
// ============================================================

#[test]
fn test_atom_basic() {
    let result = run_expr_i32("(let [a (atom 42)] (deref a))");
    assert_eq!(result, 42);
}

#[test]
fn test_atom_deref_syntax() {
    let result = run_expr_i32("(let [a (atom 42)] @a)");
    assert_eq!(result, 42);
}

#[test]
fn test_atom_reset() {
    let result = run_expr_i32("(let [a (atom 0)] (reset! a 42) @a)");
    assert_eq!(result, 42);
}

#[test]
fn test_atom_swap() {
    let result = run_expr_i32("(let [a (atom 10)] (swap! a inc) @a)");
    assert_eq!(result, 11);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name set-validator!; see prototype_cases_awaiting_compiled_support"]
fn test_atom_validator_accepts() {
    // Validator accepts value → reset! succeeds
    let result = run_expr_i32(
        "(let [a (atom 0)] (set-validator! a pos?) (reset! a 42) @a)"
    );
    assert_eq!(result, 42);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name set-validator!; see prototype_cases_awaiting_compiled_support"]
fn test_atom_validator_rejects() {
    // Validator rejects value → throw caught by try/catch
    let result = run_expr_i32(
        "(let [a (atom 1)] (set-validator! a pos?) (try (reset! a -1) (catch :default e 99)))"
    );
    assert_eq!(result, 99);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name set-validator!; see prototype_cases_awaiting_compiled_support"]
fn test_atom_validator_swap_rejects() {
    // Validator rejects swap! result → throw caught
    let result = run_expr_i32(
        "(let [a (atom 5)] (set-validator! a pos?) (try (swap! a (fn [x] (- 0 x))) (catch :default e 99)))"
    );
    assert_eq!(result, 99);
}

// ============================================================
// partial/comp/juxt tests
// ============================================================

#[test]
fn test_partial_debug_rest_count() {
    // Test that variadic rest params are usable PersistentVectors
    let result = run_expr_i32("((fn [& args] (count args)) 1 2 3)");
    assert_eq!(result, 3);
}

#[test]
fn test_partial_debug_rest_first() {
    let result = run_expr_i32("((fn [& args] (first args)) 42)");
    assert_eq!(result, 42);
}

#[test]
fn test_partial_debug_apply_rest() {
    let result = run_expr_i32("((fn [& args] (apply + args)) 3 4)");
    assert_eq!(result, 7);
}

#[test]
fn test_partial_debug_fixed_and_rest() {
    // 1 fixed param + rest
    let result = run_expr_i32("((fn [x & args] (+ x (first args))) 10 20)");
    assert_eq!(result, 30);
}

#[test]
fn test_partial_debug_concat() {
    let result = run_expr_i32("(first (concat [1 2] [3 4]))");
    assert_eq!(result, 1);
}

#[test]
fn test_partial_debug_concat_count() {
    let result = run_expr_i32("(count (concat [1 2] [3 4]))");
    assert_eq!(result, 4);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name lazy-seq; see prototype_cases_awaiting_compiled_support"]
fn test_partial_debug_lazy_seq() {
    let result = run_expr_i32("(first (lazy-seq [1 2 3]))");
    assert_eq!(result, 1);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name range; see prototype_cases_awaiting_compiled_support"]
fn test_partial_debug_range_alength() {
    // Test that range still works (uses alength on rest param)
    let result = run_expr_i32("(first (range 5))");
    assert_eq!(result, 0);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name comp; see prototype_cases_awaiting_compiled_support"]
fn test_debug_comp_simple() {
    // comp without rest params: (f (g x)) where g is inc
    let result = run_expr_i32("(let [f (comp inc inc)] (f 10))");
    assert_eq!(result, 12);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name lazy-seq; see prototype_cases_awaiting_compiled_support"]
fn test_debug_lazy_seq_simple() {
    // Simplest lazy-seq: a thunk that returns a cons
    let result = run_expr_i32("(first (lazy-seq (cons 42 nil)))");
    assert_eq!(result, 42);
}

#[test]
fn test_debug_set_mutable() {
    // Test mutable field set! on the pinned four-field Atom (state meta validator watches)
    let result = run_expr_i32("(let [a (->Atom 10 nil nil nil)] (set! (.-state a) 20) (.-state a))");
    assert_eq!(result, 20);
}

#[test]
fn test_debug_seq_vec() {
    // Test seq on a vector
    let result = run_expr_i32("(first (seq [1 2 3]))");
    assert_eq!(result, 1);
}

#[test]
fn test_debug_vec_nth() {
    // Test nth on a vector
    let result = run_expr_i32("(nth [10 20 30] 1)");
    assert_eq!(result, 20);
}

#[test]
fn test_partial_apply_vec() {
    let result = run_expr_i32("(apply + [5 10])");
    assert_eq!(result, 15);
}

#[test]
fn test_partial_apply_cons() {
    let result = run_expr_i32("(apply + (cons 5 (cons 10 nil)))");
    assert_eq!(result, 15);
}

#[test]
fn test_partial_apply_concat() {
    // Apply + to a concat result
    let result = run_expr_i32("(apply + (concat [5] [10]))");
    assert_eq!(result, 15);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name partial; see prototype_cases_awaiting_compiled_support"]
fn test_partial() {
    let result = run_expr_i32("(let [add5 (partial + 5)] (add5 10))");
    assert_eq!(result, 15);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name comp; see prototype_cases_awaiting_compiled_support"]
fn test_comp() {
    let result = run_expr_i32("(let [f (comp inc inc)] (f 10))");
    assert_eq!(result, 12);
}

// ============================================================
// doseq test
// ============================================================

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name doseq; see prototype_cases_awaiting_compiled_support"]
fn test_doseq_basic() {
    // doseq should return nil (0 as i31ref sentinel)
    let result = run_expr_i32("(let [a (atom 0)] (doseq [x [1 2 3]] (swap! a (fn [v] (+ v x)))) @a)");
    assert_eq!(result, 6);
}

// ============================================================
// when-first test
// ============================================================

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name when-first; see prototype_cases_awaiting_compiled_support"]
fn test_when_first() {
    let result = run_expr_i32("(when-first [x [42 1 2]] x)");
    assert_eq!(result, 42);
}

// ============================================================
// str multi-arg tests
// ============================================================

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name str; see prototype_cases_awaiting_compiled_support"]
fn test_str_multi_arg_literals() {
    // All-literal strings - compile-time optimization
    assert_eq!(run_expr_string(r#"(str "hello" " " "world")"#), "hello world");
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name str; see prototype_cases_awaiting_compiled_support"]
fn test_str_mixed_types() {
    // Mixed types - runtime via core.sus str function
    assert_eq!(run_expr_string(r#"(str "x=" 42)"#), "x=42");
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name str; see prototype_cases_awaiting_compiled_support"]
fn test_str_single_int() {
    assert_eq!(run_expr_string("(str 123)"), "123");
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name str; see prototype_cases_awaiting_compiled_support"]
fn test_str_with_nil() {
    assert_eq!(run_expr_string(r#"(str "a" nil "b")"#), "ab");
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name str; see prototype_cases_awaiting_compiled_support"]
fn test_str_empty() {
    assert_eq!(run_expr_string("(str)"), "");
}

/// Printing has no compiled-pipeline counterpart yet: println/print/prn are
/// unresolved and the session has no stdout capture. Execute the expression so
/// the failure is the real diagnostic, then refuse rather than guess output.
fn run_expr_capture_stdout(expr: &str) -> String {
    let _ = observe(expr);
    panic!("stdout capture is not available in the compiled pipeline")
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name println; see prototype_cases_awaiting_compiled_support"]
fn test_println_basic() {
    let output = run_expr_capture_stdout(r#"(println "hello")"#);
    assert_eq!(output, "hello\n");
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name println; see prototype_cases_awaiting_compiled_support"]
fn test_println_multiple_args() {
    let output = run_expr_capture_stdout(r#"(println "hello" "world")"#);
    assert_eq!(output, "hello world\n");
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name println; see prototype_cases_awaiting_compiled_support"]
fn test_println_no_args() {
    let output = run_expr_capture_stdout("(println)");
    assert_eq!(output, "\n");
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name println; see prototype_cases_awaiting_compiled_support"]
fn test_println_number() {
    let output = run_expr_capture_stdout("(println 42)");
    assert_eq!(output, "42\n");
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name print; see prototype_cases_awaiting_compiled_support"]
fn test_print_no_newline() {
    let output = run_expr_capture_stdout(r#"(print "hello")"#);
    assert_eq!(output, "hello");
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name prn; see prototype_cases_awaiting_compiled_support"]
fn test_prn_basic() {
    // prn uses pr-str which should print in readable form
    // For now, verify it outputs the value followed by newline
    let output = run_expr_capture_stdout("(prn 42)");
    assert_eq!(output, "42\n");
}

// ========================================================================
// Multi-arity functions
// ========================================================================

#[test]
fn test_multi_arity_basic() {
    // Non-variadic multi-arity: fixed dispatch
    let result = run_expr_i32(
        "(let [f (fn ([x] x) ([x y] (+ x y)))] (f 42))"
    );
    assert_eq!(result, 42);
}

#[test]
fn test_multi_arity_two_args() {
    let result = run_expr_i32(
        "(let [f (fn ([x] x) ([x y] (+ x y)))] (f 10 20))"
    );
    assert_eq!(result, 30);
}

// ========================================================================
// Multi-arity variadic rest params
// ========================================================================

#[test]
fn test_multi_arity_variadic_basic() {
    // Variadic clause should receive rest args as a vector
    let result = run_expr_i32(
        "(let [f (fn ([x] x) ([x & more] (count more)))] (f 1 2 3))"
    );
    assert_eq!(result, 2); // more = [2, 3]
}

#[test]
fn test_multi_arity_variadic_fixed_dispatch() {
    // Fixed arity clause should still work correctly
    let result = run_expr_i32(
        "(let [f (fn ([x] x) ([x & more] (count more)))] (f 42))"
    );
    assert_eq!(result, 42);
}

#[test]
fn test_multi_arity_variadic_rest_first() {
    // Should be able to use (first) on rest args
    let result = run_expr_i32(
        "(let [f (fn ([x] x) ([x y & more] (first more)))] (f 1 2 99))"
    );
    assert_eq!(result, 99);
}

#[test]
fn test_multi_arity_variadic_zero_fixed() {
    // Variadic clause with zero fixed params
    let result = run_expr_i32(
        "(let [f (fn ([] 0) ([& args] (count args)))] (f 1 2 3))"
    );
    assert_eq!(result, 3);
}

// ========================================================================
// Large vector apply
// ========================================================================

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name range; see prototype_cases_awaiting_compiled_support"]
fn test_apply_large_vector_variadic_capture() {
    // apply a user-defined variadic closure to a vector > 32 elements
    // This tests the VARIADIC_CAPTURE path with vec-to-array fallback
    let result = run_expr_i32(
        "(let [f (fn [& args] (count args))
               v (vec (range 50))]
           (apply f v))"
    );
    assert_eq!(result, 50);
}

#[test]
#[ignore = "compiled pipeline: Unresolved Runtime name take; see prototype_cases_awaiting_compiled_support"]
fn test_apply_large_vector_builtin() {
    // apply a builtin variadic (+) to a large vector (>32 elements)
    // Tests the VARIADIC_CLOSURE dispatch path with pre-normalized array
    let result = run_expr_i32(
        "(apply + (vec (take 4 (repeat 10))))"
    );
    assert_eq!(result, 40);
}

// ========================================================================
// String hashing in collections
// ========================================================================

#[test]
fn test_string_hash_map_count() {
    // Strings as map keys should hash correctly (not all collide at 0)
    let result = run_expr_i32("(count (assoc (assoc {} \"a\" 1) \"b\" 2))");
    assert_eq!(result, 2);
}

#[test]
fn test_string_hash_map_lookup() {
    let result = run_expr_i32("(get {\"hello\" 42} \"hello\")");
    assert_eq!(result, 42);
}

// ========================================================================
// Multi-arity variadic guards
// ========================================================================

#[test]
fn test_multi_arity_variadic_guard() {
    // Variadic clause with fixed params dispatches correctly
    let result = run_expr_i32(
        "(let [f (fn ([x] x) ([x y & more] (+ x y (count more))))] (f 10 20 30 40))"
    );
    assert_eq!(result, 32); // 10 + 20 + 2
}

// ========================================================================
// Keyword/symbol runtime construction
// ========================================================================

#[test]
fn test_keyword_from_string() {
    let result = run_expr_i32("(if (= (keyword \"a\") :a) 1 0)");
    assert_eq!(result, 1);
}

#[test]
fn test_keyword_as_map_key() {
    let result = run_expr_i32("(get {:a 42} (keyword \"a\"))");
    assert_eq!(result, 42);
}

#[test]
fn test_symbol_from_string() {
    let result = run_expr_i32("(if (= (name (symbol \"foo\")) \"foo\") 1 0)");
    assert_eq!(result, 1);
}

#[test]
fn test_keyword_ns_from_string() {
    let result = run_expr_i32("(if (= (keyword \"ns\" \"a\") :ns/a) 1 0)");
    assert_eq!(result, 1);
}
#[test]
fn condition_evaluates_effect_once() {
    assert_eq!(
        run_expr_i32("(let [a (atom 0)] (if (swap! a inc) 10 20) (deref a))"),
        1
    );
}

#[test]
fn condition_preserves_falsey_and_truthy_effects() {
    for condition in ["nil", "false", "true", "0", "[]", "1.5"] {
        let source =
            format!("(let [a (atom 0)] (if (do (swap! a inc) {condition}) 10 20) (deref a))");
        assert_eq!(run_expr_i32(&source), 1, "{condition}");
    }
}

#[test]
fn nested_conditions_preserve_effect_order() {
    assert_eq!(
        run_expr_i32(
            "(let [a (atom 0)] (if (if (do (swap! a inc) false) true (do (swap! a inc) 0)) (swap! a inc) 99) (deref a))"
        ),
        3
    );
}

#[test]
fn reduce_without_initial_value() {
    assert_eq!(run_expr_i32("(reduce + [1 2 3])"), 6);
    assert_eq!(run_expr_i32("(reduce + [])"), 0);
    assert_eq!(run_expr_i32("(reduce (fn [a b] (+ a b)) [8])"), 8);
    assert_eq!(run_expr_i32("(reduce + 10 [1 2 3])"), 16);
}

#[test]
fn comparison_arguments_follow_pinned_variadic_macro_expansion() {
    // The pinned cljs.core `<` macro expands (< a b c) to (and (< a b) (< b c)),
    // evaluating the middle argument twice and short-circuiting on false.
    // Fresh pinned ClojureScript observations: 4 and 2 (not JVM Clojure's 3).
    assert_eq!(
        run_expr_i32("(let [a (atom 0)] (< (swap! a inc) (swap! a inc) (swap! a inc)) (deref a))"),
        4
    );
    assert_eq!(
        run_expr_i32(
            "(let [a (atom 0)] (< (do (swap! a inc) 9) (do (swap! a inc) 2) (do (swap! a inc) 3)) (deref a))"
        ),
        2
    );
}

#[test]
fn arithmetic_and_equality_preserve_argument_effect_order() {
    assert_eq!(
        run_expr_i32("(let [a (atom 0)] (- (swap! a inc) (swap! a inc)))"),
        -1
    );
    assert_eq!(
        run_expr_i32("(let [a (atom 0)] (= (swap! a inc) (swap! a inc) (swap! a inc)) (deref a))"),
        3
    );
    assert!(run_expr_bool("(= [1 2] [1 2])"));
    assert!(!run_expr_bool("(= [1 2] [1 3])"));
    assert!(run_expr_bool("(< 1)"));
}

#[test]
fn known_function_arities_accept_every_declared_signature() {
    // Variadic minimums, multiple fixed signatures and the latest redefinition.
    assert_eq!(run_expr_i32("(defn f [x & more] (count more)) (f 1 2 3)"), 2);
    assert_eq!(run_expr_i32("(defn f [x & more] (count more)) (f 1)"), 0);
    assert_eq!(run_expr_i32("(defn f ([] 0) ([x] x) ([x y] (+ x y))) (+ (f) (f 4) (f 5 6))"), 15);
    assert_eq!(run_expr_i32("(defn f [x] x) (defn f [x y] (+ x y)) (f 20 22)"), 42);
    assert_eq!(run_expr_i32("(:a {:a 7})"), 7);
    assert_eq!(run_expr_i32("(:a {} 9)"), 9);
}

/// Pinned `parse-invoke*` :fn-arity, promoted to a located error (design section 4).
fn assert_wrong_arity(expression: &str, diagnostic: &str) {
    let error = Compiler::new()
        .compile_expr_with_info(expression)
        .expect_err(expression)
        .to_string();
    assert!(error.contains(diagnostic), "{expression}: {error}");
    assert!(error.contains(" at bytes "), "{expression}: unlocated {error}");
}

#[test]
fn known_function_arities_reject_calls_no_signature_accepts() {
    for (expression, diagnostic) in [
        ("(defn f [x & more] x) (f)", "Wrong number of args (0) passed to user/f"),
        ("(defn f ([] 0) ([x y] (+ x y))) (f 1)", "Wrong number of args (1) passed to user/f"),
        ("(defn f [x] x) (defn f [x y] (+ x y)) (f 1)", "Wrong number of args (1) passed to user/f"),
        ("(:a)", "Wrong number of args (0) passed to :a"),
        ("(:a {} 1 2)", "Wrong number of args (3) passed to :a"),
    ] {
        assert_wrong_arity(expression, diagnostic);
    }
}

#[test]
fn declared_arities_follow_the_published_declaration_fields() {
    // Accepted: unavailable method-params, disabled fn-var, provisional
    // self-recursion, non-fn initializers and the variadic minimum.
    for expression in [
        "(def ^{:top-fn {}} f (fn [x] x)) (f 1 2)",
        "(def ^{:top-fn {:fn-var false}} f (fn [x] x)) (f 1 2)",
        "(def ^{:declared true} f) (f 1 2)",
        "(defn f [x] (if (zero? x) 0 (f (dec x) 1))) 0",
        "(def f (let [] (fn [x] x))) (f 1 2)",
        "(defn f ([x] x) ([x y z & more] x)) (f 1 2 3 4)",
    ] {
        Compiler::new()
            .compile_expr_with_info(expression)
            .unwrap_or_else(|error| panic!("{expression}: {error}"));
    }
    for (expression, diagnostic) in [
        ("(def ^{:top-fn nil} f (fn [x] x)) (f 1 2)", "Wrong number of args (2) passed to user/f"),
        ("(def ^{:top-fn {:method-params ([a b])}} f (fn [x] x)) (f 1)", "Wrong number of args (1) passed to user/f"),
        ("(def ^{:declared true :arglists '([x])} f) (f 1 2)", "Wrong number of args (2) passed to user/f"),
        ("(declare ^{:arglists '([x])} f) (defn g [] (f 1 2))", "Wrong number of args (2) passed to user/f"),
        ("(defn f ([x] x) ([x y z & more] x)) (f 1 2)", "Wrong number of args (2) passed to user/f"),
        ("(def f (fn [x] x)) (f)", "Wrong number of args (0) passed to user/f"),
    ] {
        assert_wrong_arity(expression, diagnostic);
    }
}

#[test]
fn wrong_arity_diagnostics_locate_the_invoke_form() {
    let error = |source: &str| Compiler::new().compile_expr_with_info(source).expect_err(source).to_string();
    // The span is the invoke form itself, including a call rewritten by a
    // macro, which keeps its original source form's location.
    assert!(error("(defn f [x] x) (f)").ends_with("at bytes 15..18"), "{}", error("(defn f [x] x) (f)"));
    let threaded = error("(defn f [x] x) (-> 1 (f 2))");
    assert!(threaded.contains("Wrong number of args (2) passed to user/f"), "{threaded}");
    assert!(threaded.ends_with("at bytes 21..26"), "{threaded}");
}

#[test]
fn rebinding_a_function_var_keeps_its_declared_arity() {
    // Pinned ClojureScript only warns here and runs the rebound function.
    // Suss reports the declared arity (design section 4); this test records
    // that compatibility decision.
    for expression in [
        "(def ^:dynamic f (fn [x] x)) (binding [f (fn [x y] y)] (f 1 2))",
        "(defn f [x] x) (with-redefs [f (fn [x y] y)] (f 1 2))",
    ] {
        assert_wrong_arity(expression, "Wrong number of args (2) passed to user/f");
    }
}

#[test]
fn known_function_arity_errors_are_compile_diagnostics() {
    for (expression, diagnostic) in [
        ("(defn f [x] x) (f)", "Wrong number of args (0) passed to user/f"),
        ("(defn f [x] x) (f 1 2)", "Wrong number of args (2) passed to user/f"),
        ("(reduce +)", "Wrong number of args (1) passed to suss.core/reduce"),
    ] {
        assert_wrong_arity(expression, diagnostic);
    }
}

#[test]
fn lexical_callees_shadow_global_and_intrinsic_names() {
    assert_eq!(
        run_expr_i32("(defn f [x] x) (let [f (fn [x y] (+ x y))] (f 20 22))"),
        42
    );
    assert_eq!(run_expr_i32("(let [inc (fn [x] (+ x 10))] (inc 2))"), 12);
}

#[test]
fn legacy_dispatch_receiver_precedes_arguments_once() {
    assert_eq!(
        run_expr_i32("(let [a (atom 0)] (nth (do (swap! a inc) [17 19 23]) (swap! a inc)))"),
        23
    );
    // A nested dispatcher cannot overwrite the saved outer receiver/arguments.
    assert_eq!(run_expr_i32("(let [a (atom 0)] (nth (do (swap! a (fn [n] (+ (* n 10) 1))) [17 19]) (do (swap! a (fn [n] (+ (* n 10) 2))) (nth (do (swap! a (fn [n] (+ (* n 10) 3))) [1]) 0))) (deref a))"), 123);
    assert_eq!(run_expr_i32("(let [a (atom 0)] (assoc (do (swap! a (fn [n] (+ (* n 10) 1))) {}) (do (swap! a (fn [n] (+ (* n 10) 2))) 17) (do (swap! a (fn [n] (+ (* n 10) 3))) 19)) (deref a))"), 123);
}

#[test]
fn legacy_bitwise_operands_preserve_source_order_once() {
    let mut observations = Vec::new();
    for (operation, expected) in [
        ("bit-and", 1), ("bit-or", 3), ("bit-xor", 2),
        ("bit-shift-left", 6), ("bit-shift-right", 1),
        ("unsigned-bit-shift-right", 1),
    ] {
        let source = format!("(let [a (atom 0)] ({operation} (do (swap! a (fn [n] (+ (* n 10) 1))) 3) (do (swap! a (fn [n] (+ (* n 10) 2))) 1)) (deref a))");
        observations.push((operation, run_expr_i32(&source)));
        assert_eq!(run_expr_i32(&format!("({operation} 3 1)")), expected, "{operation} result");
    }
    assert_eq!(observations, vec![
        ("bit-and", 12), ("bit-or", 12), ("bit-xor", 12),
        ("bit-shift-left", 12), ("bit-shift-right", 12),
        ("unsigned-bit-shift-right", 12),
    ]);
}

#[test]
fn legacy_hash_inspects_each_evaluated_operand_once() {
    for value in ["7", "nil", "false", "true", "1.5", "\"text\"", ":key", "(do (hash 8) 7)"] {
        let source = format!("(let [a (atom 0)] (hash (do (swap! a inc) {value})) (deref a))");
        assert_eq!(run_expr_i32(&source), 1, "{value}");
    }
}

/// Former prototype-route tests whose features the compiled pipeline lacks.
/// Each row names the ignored test, its first expression and the exact current
/// diagnostic. When a feature lands the diagnostic changes and this test fails:
/// re-enable the named test and remove its row. Rows never count as passes.
const AWAITING_COMPILED_SUPPORT: &[(&str, &str, &str)] = &[
    ("test_apply_large_vector_builtin", "(apply + (vec (take 4 (repeat 10))))", "Unresolved Runtime name take at bytes 15..19"),
    ("test_apply_large_vector_variadic_capture", "(let [f (fn [& args] (count args))                v (vec (range 50))]            (apply f v))", "Unresolved Runtime name range at bytes 58..63"),
    ("test_atom_validator_accepts", "(let [a (atom 0)] (set-validator! a pos?) (reset! a 42) @a)", "Unresolved Runtime name set-validator! at bytes 19..33"),
    ("test_atom_validator_rejects", "(let [a (atom 1)] (set-validator! a pos?) (try (reset! a -1) (catch :default e 99)))", "Unresolved Runtime name set-validator! at bytes 19..33"),
    ("test_atom_validator_swap_rejects", "(let [a (atom 5)] (set-validator! a pos?) (try (swap! a (fn [x] (- 0 x))) (catch :default e 99)))", "Unresolved Runtime name set-validator! at bytes 19..33"),
    ("test_comp", "(let [f (comp inc inc)] (f 10))", "Unresolved Runtime name comp at bytes 9..13"),
    ("test_cond_thread_first", "(cond-> 1 true inc true inc)", "Unresolved Runtime name cond-> at bytes 1..7"),
    ("test_cond_thread_first_false", "(cond-> 1 true inc false inc)", "Unresolved Runtime name cond-> at bytes 1..7"),
    ("test_debug_comp_simple", "(let [f (comp inc inc)] (f 10))", "Unresolved Runtime name comp at bytes 9..13"),
    ("test_debug_lazy_seq_simple", "(first (lazy-seq (cons 42 nil)))", "Unresolved Runtime name lazy-seq at bytes 8..16"),
    ("test_defn_map_destructuring", "(defn foo [{:keys [x y]}] (+ x y)) (foo {:x 3 :y 7})", "Parameter destructuring is not lowered yet at bytes 0..34"),
    ("test_doseq_basic", "(let [a (atom 0)] (doseq [x [1 2 3]] (swap! a (fn [v] (+ v x)))) @a)", "Unresolved Runtime name doseq at bytes 19..24"),
    ("test_fn_map_destructuring_as", "((fn [{:keys [a] :as m}] (+ a (count m))) {:a 10 :b 20})", "Parameter destructuring is not lowered yet at bytes 6..23"),
    ("test_fn_map_destructuring_keys", "((fn [{:keys [a b]}] (+ a b)) {:a 10 :b 20})", "Parameter destructuring is not lowered yet at bytes 6..19"),
    ("test_map_destructuring_direct", "(let [{x :a y :b} {:a 10 :b 20}] (+ x y))", "Binding destructuring is not lowered yet at bytes 6..17"),
    ("test_map_destructuring_keys", "(let [{:keys [a b]} {:a 10 :b 20}] (+ a b))", "Binding destructuring is not lowered yet at bytes 6..19"),
    ("test_multi_arity_map_destructuring", "(let [f (fn ([] 0) ([{:keys [a b]}] (+ a b)))] (f {:a 5 :b 15}))", "Parameter destructuring is not lowered yet at bytes 21..34"),
    ("test_partial", "(let [add5 (partial + 5)] (add5 10))", "Unresolved Runtime name partial at bytes 12..19"),
    ("test_partial_debug_lazy_seq", "(first (lazy-seq [1 2 3]))", "Unresolved Runtime name lazy-seq at bytes 8..16"),
    ("test_partial_debug_range_alength", "(first (range 5))", "Unresolved Runtime name range at bytes 8..13"),
    ("test_print_no_newline", "(print \"hello\")", "Unresolved Runtime name print at bytes 1..6"),
    ("test_println_basic", "(println \"hello\")", "Unresolved Runtime name println at bytes 1..8"),
    ("test_println_multiple_args", "(println \"hello\" \"world\")", "Unresolved Runtime name println at bytes 1..8"),
    ("test_println_no_args", "(println)", "Unresolved Runtime name println at bytes 1..8"),
    ("test_println_number", "(println 42)", "Unresolved Runtime name println at bytes 1..8"),
    ("test_prn_basic", "(prn 42)", "Unresolved Runtime name prn at bytes 1..4"),
    ("test_some_thread_first", "(some-> 1 inc inc)", "Unresolved Runtime name some-> at bytes 1..7"),
    ("test_str_empty", "(str)", "Unresolved Runtime name str at bytes 1..4"),
    ("test_str_mixed_types", "(str \"x=\" 42)", "Unresolved Runtime name str at bytes 1..4"),
    ("test_str_multi_arg_literals", "(str \"hello\" \" \" \"world\")", "Unresolved Runtime name str at bytes 1..4"),
    ("test_str_single_int", "(str 123)", "Unresolved Runtime name str at bytes 1..4"),
    ("test_str_with_nil", "(str \"a\" nil \"b\")", "Unresolved Runtime name str at bytes 1..4"),
    ("test_variadic_closure_with_capture_multiple_args", "((constantly 99) 1 2 3)", "Unresolved Runtime name constantly at bytes 2..12"),
    ("test_variadic_closure_with_capture_zero_args", "((constantly 42))", "Unresolved Runtime name constantly at bytes 2..12"),
    ("test_vector_destructuring_let", "(let [[a b] [10 20]] (+ a b))", "Binding destructuring is not lowered yet at bytes 6..11"),
    ("test_vector_destructuring_rest", "(let [[a & rest] [1 2 3]] (count rest))", "Binding destructuring is not lowered yet at bytes 6..16"),
    ("test_when_first", "(when-first [x [42 1 2]] x)", "Unresolved Runtime name when-first at bytes 1..11"),
];

#[test]
fn prototype_cases_awaiting_compiled_support() {
    for (test, expr, diagnostic) in AWAITING_COMPILED_SUPPORT {
        // Whitespace inside multi-line test sources is collapsed in this table.
        match Compiler::new().compile_expr_with_info(expr) {
            Err(suss_compile::CompileError::Semantic(actual)) => assert_eq!(
                &actual, diagnostic,
                "{test} changed; re-enable it if {expr} now compiles correctly"
            ),
            other => panic!("{test}: {expr} no longer fails as recorded: {other:?}"),
        }
    }
}
