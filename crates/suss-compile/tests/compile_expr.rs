//! Tests for compile_expr() - compiles expressions and runs with wasmtime

use suss_compile::Compiler;
use wasmtime::{Engine, Instance, Module, Store};

fn run_expr_i32(expr: &str) -> i32 {
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_expr(expr).expect("compilation failed");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("module creation failed");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiation failed");

    let eval_fn = instance
        .get_typed_func::<(), i32>(&mut store, "eval")
        .expect("eval function not found");

    eval_fn.call(&mut store, ()).expect("call failed")
}

fn run_expr_i64(expr: &str) -> i64 {
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_expr(expr).expect("compilation failed");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("module creation failed");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiation failed");

    let eval_fn = instance
        .get_typed_func::<(), i64>(&mut store, "eval")
        .expect("eval function not found");

    eval_fn.call(&mut store, ()).expect("call failed")
}

fn run_expr_f64(expr: &str) -> f64 {
    let mut compiler = Compiler::new();
    let wasm_bytes = compiler.compile_expr(expr).expect("compilation failed");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("module creation failed");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiation failed");

    let eval_fn = instance
        .get_typed_func::<(), f64>(&mut store, "eval")
        .expect("eval function not found");

    eval_fn.call(&mut store, ()).expect("call failed")
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
    assert_eq!(run_expr_i32("(/ 21 3)"), 7);
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

#[test]
fn test_vector_literal() {
    let result = run_expr_string("[1 2 3]");
    assert_eq!(result, "[1 2 3]");
}

#[test]
fn test_map_literal() {
    let result = run_expr_string("{:a 1}");
    assert_eq!(result, "{:a 1}");
}

#[test]
fn test_set_literal() {
    let result = run_expr_string("#{1 2 3}");
    assert_eq!(result, "#{1 2 3}");
}
