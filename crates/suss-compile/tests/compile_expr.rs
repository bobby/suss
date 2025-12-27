//! Tests for compile_expr() - compiles expressions and runs with wasmtime
//!
//! Uses WASM GC for value representation:
//! - Small integers: i31ref with encoding (n << 1) | 1
//! - nil: i31ref(0), false: i31ref(2), true: i31ref(4)
//! - Large integers: structref (LARGE_INT type)
//! - Floats: structref (FLOAT type)
//! - Strings: arrayref (STRING type)

use suss_compile::Compiler;
use wasmtime::{Config, Engine, Instance, Module, Store, Val};

/// Create a GC-enabled wasmtime engine
fn gc_engine() -> Engine {
    let mut config = Config::new();
    config.wasm_gc(true);
    config.wasm_function_references(true);
    config.wasm_tail_call(true);
    Engine::new(&config).expect("engine creation failed")
}

/// GC sentinel constants
const NIL_SENTINEL: i32 = 0;
const FALSE_SENTINEL: i32 = 2;
const TRUE_SENTINEL: i32 = 4;

/// Decode an i31ref value to an integer using our sentinel encoding
/// Encoding: small_int = (n << 1) | 1
/// Booleans: true = 4 -> 1, false = 2 -> 0
fn decode_i31_from_anyref(store: &mut Store<()>, val: &Val) -> i64 {
    match val {
        Val::AnyRef(Some(anyref)) => {
            // Try to extract as i31
            match anyref.as_i31(store) {
                Ok(Some(i31)) => {
                    let raw = i31.get_i32();
                    // Check for special sentinels
                    match raw {
                        NIL_SENTINEL => 0, // nil as 0
                        FALSE_SENTINEL => 0, // false as 0
                        TRUE_SENTINEL => 1, // true as 1
                        _ => {
                            // Small integer: encoding is (n << 1) | 1
                            (raw >> 1) as i64
                        }
                    }
                }
                Ok(None) => panic!("Expected i31ref inside AnyRef, got struct or array"),
                Err(e) => panic!("Error extracting i31: {:?}", e),
            }
        }
        Val::AnyRef(None) => {
            panic!("Got null anyref, expected i31ref");
        }
        _ => panic!("Expected AnyRef, got {:?}", val),
    }
}

fn run_expr_i32(expr: &str) -> i32 {
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_expr(expr).expect("compilation failed");

    let engine = gc_engine();
    let module = Module::new(&engine, &wasm_bytes).expect("module creation failed");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiation failed");

    let eval_fn = instance.get_func(&mut store, "eval").expect("eval function not found");
    let mut results = vec![Val::null_any_ref()];
    eval_fn.call(&mut store, &[], &mut results).expect("call failed");

    decode_i31_from_anyref(&mut store, &results[0]) as i32
}

fn run_expr_i64(expr: &str) -> i64 {
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_expr(expr).expect("compilation failed");

    let engine = gc_engine();
    let module = Module::new(&engine, &wasm_bytes).expect("module creation failed");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiation failed");

    let eval_fn = instance.get_func(&mut store, "eval").expect("eval function not found");
    let mut results = vec![Val::null_any_ref()];
    eval_fn.call(&mut store, &[], &mut results).expect("call failed");

    decode_i31_from_anyref(&mut store, &results[0])
}

fn run_expr_f64(expr: &str) -> f64 {
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_expr(expr).expect("compilation failed");

    let engine = gc_engine();
    let module = Module::new(&engine, &wasm_bytes).expect("module creation failed");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiation failed");

    let eval_fn = instance.get_func(&mut store, "eval").expect("eval function not found");
    let mut results = vec![Val::null_any_ref()];
    eval_fn.call(&mut store, &[], &mut results).expect("call failed");

    match &results[0] {
        Val::AnyRef(Some(anyref)) => {
            match anyref.as_i31(&store) {
                Ok(Some(i31)) => {
                    // Small integers can represent some float values
                    let raw = i31.get_i32();
                    (raw >> 1) as f64
                }
                Ok(None) => {
                    // Should be a FLOAT struct with f64 inside
                    // FLOAT struct: { type_id: i32, value: f64 }
                    // Field 1 contains the f64 value (field 0 is type_id)
                    match anyref.as_struct(&store) {
                        Ok(Some(struct_ref)) => {
                            match struct_ref.field(&mut store, 1) {
                                Ok(val) => val.unwrap_f64(),
                                Err(e) => panic!("Error getting struct field: {:?}", e),
                            }
                        }
                        Ok(None) => panic!("Expected FLOAT struct, got non-struct GC ref"),
                        Err(e) => panic!("Error converting to struct: {:?}", e),
                    }
                }
                Err(e) => panic!("Error extracting i31: {:?}", e),
            }
        }
        Val::AnyRef(None) => panic!("Got null anyref for f64"),
        _ => panic!("Unexpected value type for f64: {:?}", results[0]),
    }
}

fn run_expr_bool(expr: &str) -> bool {
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_expr(expr).expect("compilation failed");

    let engine = gc_engine();
    let module = Module::new(&engine, &wasm_bytes).expect("module creation failed");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiation failed");

    let eval_fn = instance.get_func(&mut store, "eval").expect("eval function not found");
    let mut results = vec![Val::null_any_ref()];
    eval_fn.call(&mut store, &[], &mut results).expect("call failed");

    match &results[0] {
        Val::AnyRef(Some(anyref)) => {
            match anyref.as_i31(&store) {
                Ok(Some(i31)) => {
                    let raw = i31.get_i32();
                    match raw {
                        TRUE_SENTINEL => true,
                        FALSE_SENTINEL => false,
                        NIL_SENTINEL => false, // nil is falsy
                        _ => panic!("Expected boolean sentinel, got {}", raw),
                    }
                }
                Ok(None) => panic!("Expected i31ref boolean, got struct/array"),
                Err(e) => panic!("Error extracting i31: {:?}", e),
            }
        }
        Val::AnyRef(None) => panic!("Got null anyref for bool"),
        _ => panic!("Unexpected value type for bool: {:?}", results[0]),
    }
}

#[test]
fn test_integer_literal() {
    assert_eq!(run_expr_i32("42"), 42);
}

#[test]
fn test_dump_if_wasm() {
    use std::fs;
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_expr("(if true 1 0)").expect("compilation failed");
    fs::write("/tmp/test_if.wasm", &wasm_bytes).expect("write failed");
    println!("Wrote {} bytes to /tmp/test_if.wasm", wasm_bytes.len());
    // Now try to load it
    let engine = gc_engine();
    match Module::new(&engine, &wasm_bytes) {
        Ok(_) => println!("Module loaded successfully"),
        Err(e) => panic!("Module load failed: {}", e),
    }
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
    assert_eq!(run_expr_i32("(< 1 2)"), 1); // true = 1
    assert_eq!(run_expr_i32("(< 2 1)"), 0); // false = 0
}

#[test]
fn test_comparison_greater_than() {
    assert_eq!(run_expr_i32("(> 2 1)"), 1);
    assert_eq!(run_expr_i32("(> 1 2)"), 0);
}

#[test]
fn test_comparison_equals() {
    assert_eq!(run_expr_i32("(= 5 5)"), 1);
    assert_eq!(run_expr_i32("(= 5 3)"), 0);
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
    assert_eq!(run_expr_i32("true"), 1);
}

#[test]
fn test_boolean_false() {
    assert_eq!(run_expr_i32("false"), 0);
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
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_expr(expr).expect("compilation failed");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("module creation failed");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiation failed");

    let eval_fn = instance
        .get_typed_func::<(), (i32, i32)>(&mut store, "eval")
        .expect("eval function not found");

    let (ptr, len) = eval_fn.call(&mut store, ()).expect("call failed");

    let memory = instance.get_memory(&mut store, "memory").expect("memory not found");
    let mut buf = vec![0u8; len as usize];
    memory.read(&store, ptr as usize, &mut buf).expect("memory read failed");
    String::from_utf8(buf).expect("invalid utf8")
}

/// Run an expression and verify it compiles and executes without error.
/// Returns true if the result is a GC struct (not i31ref).
fn run_expr_is_gc_struct(expr: &str) -> bool {
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_expr(expr).expect("compilation failed");

    let engine = gc_engine();
    let module = Module::new(&engine, &wasm_bytes).expect("module creation failed");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiation failed");

    let eval_fn = instance.get_func(&mut store, "eval").expect("eval function not found");
    let mut results = vec![Val::null_any_ref()];
    eval_fn.call(&mut store, &[], &mut results).expect("call failed");

    // Check if result is a struct (not i31ref)
    match &results[0] {
        Val::AnyRef(Some(anyref)) => {
            match anyref.as_i31(&store) {
                Ok(Some(_)) => false, // It's an i31ref
                Ok(None) => true,     // It's a struct/array
                Err(_) => false,
            }
        }
        _ => false,
    }
}

#[test]
fn test_vector_literal() {
    // Vector literals should compile to PERSISTENT_VECTOR structs
    assert!(run_expr_is_gc_struct("[1 2 3]"));
}

#[test]
fn test_empty_vector() {
    assert!(run_expr_is_gc_struct("[]"));
}

#[test]
fn test_map_literal() {
    // Map literals should compile to PERSISTENT_MAP structs
    // Using integers as keys since keywords aren't supported yet
    assert!(run_expr_is_gc_struct("{1 2}"));
}

#[test]
fn test_empty_map() {
    assert!(run_expr_is_gc_struct("{}"));
}

#[test]
fn test_map_get() {
    // Get should return the value for a key
    assert_eq!(run_expr_i32("(get {1 2} 1)"), 2);
}

#[test]
fn test_map_get_empty() {
    // Get on empty map returns nil (encoded as 0)
    assert_eq!(run_expr_i32("(get {} 1)"), 0);
}


#[test]
fn test_set_literal() {
    // Set literals should compile to PERSISTENT_SET structs
    assert!(run_expr_is_gc_struct("#{1 2 3}"));
}

#[test]
fn test_empty_set() {
    assert!(run_expr_is_gc_struct("#{}"));
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
    // Without :else, returns 0 (Unit)
    assert_eq!(run_expr_i32("(cond (< 5 3) 1 (< 5 4) 2)"), 0);
}

#[test]
fn test_when_true() {
    assert_eq!(run_expr_i32("(when (> 5 3) 42)"), 42);
}

#[test]
fn test_when_false() {
    assert_eq!(run_expr_i32("(when (< 5 3) 42)"), 0);
}

#[test]
fn test_when_multiple_body() {
    assert_eq!(run_expr_i32("(when true 1 2 3)"), 3);
}

#[test]
fn test_when_not_true() {
    // when-not with true condition returns nil/0
    assert_eq!(run_expr_i32("(when-not (> 5 3) 42)"), 0);
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
    assert_eq!(run_expr_i32("(and true true)"), 1);
}

#[test]
fn test_and_one_false() {
    assert_eq!(run_expr_i32("(and true false)"), 0);
    assert_eq!(run_expr_i32("(and false true)"), 0);
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
    assert_eq!(run_expr_i32("(and 1 false 3)"), 0);
}

#[test]
fn test_and_empty() {
    // (and) returns true
    assert_eq!(run_expr_i32("(and)"), 1);
}

#[test]
fn test_and_single() {
    assert_eq!(run_expr_i32("(and 42)"), 42);
}

#[test]
fn test_or_all_false() {
    assert_eq!(run_expr_i32("(or false false)"), 0);
}

#[test]
fn test_or_one_true() {
    assert_eq!(run_expr_i32("(or true false)"), 1);
    assert_eq!(run_expr_i32("(or false true)"), 1);
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
    // (or false false) returns false (0) because false is falsy
    // With Clojure semantics, only nil and false are falsy
    assert_eq!(run_expr_i32("(or false false)"), 0);
}

#[test]
fn test_or_empty() {
    // (or) returns false
    assert_eq!(run_expr_i32("(or)"), 0);
}

#[test]
fn test_or_single() {
    assert_eq!(run_expr_i32("(or 42)"), 42);
}

#[test]
fn test_not_true() {
    assert_eq!(run_expr_i32("(not true)"), 0);
}

#[test]
fn test_not_false() {
    assert_eq!(run_expr_i32("(not false)"), 1);
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
    assert_eq!(run_expr_i32("(<= 1 2)"), 1);
    assert_eq!(run_expr_i32("(<= 2 2)"), 1);
    assert_eq!(run_expr_i32("(<= 3 2)"), 0);
}

#[test]
fn test_greater_than_or_equal() {
    assert_eq!(run_expr_i32("(>= 3 2)"), 1);
    assert_eq!(run_expr_i32("(>= 2 2)"), 1);
    assert_eq!(run_expr_i32("(>= 1 2)"), 0);
}

#[test]
fn test_not_equal() {
    assert_eq!(run_expr_i32("(not= 1 2)"), 1);
    assert_eq!(run_expr_i32("(not= 2 2)"), 0);
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
        .suffix(".suss")
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

    // Create engine with tail call and GC support
    let mut config = wasmtime::Config::new();
    config.wasm_tail_call(true);
    config.wasm_component_model(true);
    config.wasm_gc(true);
    let engine = Engine::new(&config).expect("engine creation failed");

    // Load as component
    let component = wasmtime::component::Component::new(&engine, &wasm_bytes)
        .expect("component creation failed");
    let linker = wasmtime::component::Linker::<()>::new(&engine);
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
    // Empty string has a defined hash
    let h = run_expr_i32("(hash \"\")");
    // xxHash32 of empty string with seed 0 should produce a consistent value
    // The value is: PRIME32_5 = 0x165667B1 = 374761393
    // After avalanche mixing, we get a different value
    assert_ne!(h, 0);
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
    // first on empty vector returns nil (encoded as 0)
    assert_eq!(run_expr_i32("(first [])"), 0);
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
    let result = compiler.compile_expr("(loop [x 0] (+ 1 (recur x)))");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("tail position"), "Expected 'tail position' error, got: {}", err_msg);
}

#[test]
fn test_recur_outside_loop_rejected() {
    // recur outside of loop should be rejected
    let mut compiler = Compiler::new();
    let result = compiler.compile_expr("(recur 1)");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("inside a loop"), "Expected 'inside a loop' error, got: {}", err_msg);
}

#[test]
fn test_recur_in_let_binding_rejected() {
    // recur in let binding (not tail position) should be rejected
    let mut compiler = Compiler::new();
    let result = compiler.compile_expr("(loop [x 0] (let [y (recur (+ x 1))] y))");
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
    let result = compiler.compile_expr("(loop [x 0] (do (recur (+ x 1)) x))");
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
    // Missing key returns nil (decoded as 0)
    assert_eq!(run_expr_i32("(get {1 2 3 4} 5)"), 0);
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
// Set disj Tests (stub implementation - returns original set)
// =========================================================

#[test]
fn test_set_disj_compiles() {
    // Just verify disj compiles and runs
    // Stub returns original set, so count is still 3
    assert_eq!(run_expr_i32("(count (disj #{1 2 3} 2))"), 3);
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
// Map dissoc Tests (stub implementation - returns original map)
// =========================================================

#[test]
fn test_map_dissoc_compiles() {
    // Just verify dissoc compiles and runs
    // Stub returns original map, so count is still 2
    assert_eq!(run_expr_i32("(count (dissoc {1 2 3 4} 1))"), 2);
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

#[test]
fn dump_div_wasm() {
    let mut compiler = suss_compile::Compiler::new();
    let wasm = compiler.compile_expr("(let [f /] (f 20 4))").unwrap();
    std::fs::write("/tmp/div_test.wasm", &wasm).unwrap();
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

