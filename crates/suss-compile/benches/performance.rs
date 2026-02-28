//! Performance benchmarks for Suss compilation and execution
//!
//! Measures:
//! 1. Compilation time - How long to compile expressions to WASM
//! 2. Execution time - How long WASM operations take
//! 3. WASM size - Binary size of compiled output

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use suss_compile::Compiler;
use wasmtime::{Config, Engine, Instance, Module, Store, Val};

/// Create a GC-enabled wasmtime engine
fn gc_engine() -> Engine {
    let mut config = Config::new();
    config.wasm_gc(true);
    config.wasm_function_references(true);
    config.wasm_tail_call(true);
    config.wasm_exceptions(true);
    Engine::new(&config).expect("engine creation failed")
}

/// Compile an expression and return the WASM bytes
fn compile_expr(expr: &str) -> Vec<u8> {
    let mut compiler = Compiler::new();
    compiler.compile_expr(expr).expect("compilation failed")
}

/// Compile, load, and run an expression
fn run_expr(engine: &Engine, expr: &str) -> Val {
    let wasm_bytes = compile_expr(expr);
    let module = Module::new(engine, &wasm_bytes).expect("module failed");
    let mut store = Store::new(engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiation failed");
    let eval_fn = instance.get_func(&mut store, "eval").expect("eval not found");
    let mut results = vec![Val::null_any_ref()];
    eval_fn.call(&mut store, &[], &mut results).expect("call failed");
    results.remove(0)
}

// =============================================================================
// Compilation Benchmarks
// =============================================================================

fn bench_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("compilation");

    // Simple expressions
    group.bench_function("literal_int", |b| {
        b.iter(|| compile_expr(black_box("42")))
    });

    group.bench_function("add_2", |b| {
        b.iter(|| compile_expr(black_box("(+ 1 2)")))
    });

    group.bench_function("nested_arithmetic", |b| {
        b.iter(|| compile_expr(black_box("(+ (* 3 4) (- 10 5))")))
    });

    // Collections
    group.bench_function("vector_3", |b| {
        b.iter(|| compile_expr(black_box("[1 2 3]")))
    });

    group.bench_function("map_3", |b| {
        b.iter(|| compile_expr(black_box("{1 2 3 4 5 6}")))
    });

    group.bench_function("set_3", |b| {
        b.iter(|| compile_expr(black_box("#{1 2 3}")))
    });

    // Control flow
    group.bench_function("if_simple", |b| {
        b.iter(|| compile_expr(black_box("(if true 1 2)")))
    });

    group.bench_function("cond_3", |b| {
        b.iter(|| compile_expr(black_box("(cond false 1 false 2 true 3)")))
    });

    // Functions
    group.bench_function("fn_identity", |b| {
        b.iter(|| compile_expr(black_box("((fn [x] x) 42)")))
    });

    group.bench_function("fn_add", |b| {
        b.iter(|| compile_expr(black_box("((fn [a b] (+ a b)) 1 2)")))
    });

    // Loop
    group.bench_function("loop_simple", |b| {
        b.iter(|| compile_expr(black_box("(loop [x 0] (if (< x 5) (recur (+ x 1)) x))")))
    });

    group.finish();
}

// =============================================================================
// Execution Benchmarks
// =============================================================================

fn bench_execution(c: &mut Criterion) {
    let engine = gc_engine();
    let mut group = c.benchmark_group("execution");

    // Pre-compile modules for execution benchmarks
    let add_wasm = compile_expr("(+ 1 2)");
    let add_module = Module::new(&engine, &add_wasm).unwrap();

    let nested_wasm = compile_expr("(+ (* 3 4) (- 10 5))");
    let nested_module = Module::new(&engine, &nested_wasm).unwrap();

    let vector_wasm = compile_expr("[1 2 3]");
    let vector_module = Module::new(&engine, &vector_wasm).unwrap();

    let conj_wasm = compile_expr("(conj [1 2] 3)");
    let conj_module = Module::new(&engine, &conj_wasm).unwrap();

    let if_wasm = compile_expr("(if true 1 2)");
    let if_module = Module::new(&engine, &if_wasm).unwrap();

    let loop_wasm = compile_expr("(loop [x 0] (if (< x 10) (recur (+ x 1)) x))");
    let loop_module = Module::new(&engine, &loop_wasm).unwrap();

    // Benchmark execution of pre-compiled modules
    group.bench_function("add_2_exec", |b| {
        b.iter(|| {
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &add_module, &[]).unwrap();
            let eval_fn = instance.get_func(&mut store, "eval").unwrap();
            let mut results = vec![Val::null_any_ref()];
            eval_fn.call(&mut store, &[], &mut results).unwrap();
            black_box(results)
        })
    });

    group.bench_function("nested_arithmetic_exec", |b| {
        b.iter(|| {
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &nested_module, &[]).unwrap();
            let eval_fn = instance.get_func(&mut store, "eval").unwrap();
            let mut results = vec![Val::null_any_ref()];
            eval_fn.call(&mut store, &[], &mut results).unwrap();
            black_box(results)
        })
    });

    group.bench_function("vector_create_exec", |b| {
        b.iter(|| {
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &vector_module, &[]).unwrap();
            let eval_fn = instance.get_func(&mut store, "eval").unwrap();
            let mut results = vec![Val::null_any_ref()];
            eval_fn.call(&mut store, &[], &mut results).unwrap();
            black_box(results)
        })
    });

    group.bench_function("vector_conj_exec", |b| {
        b.iter(|| {
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &conj_module, &[]).unwrap();
            let eval_fn = instance.get_func(&mut store, "eval").unwrap();
            let mut results = vec![Val::null_any_ref()];
            eval_fn.call(&mut store, &[], &mut results).unwrap();
            black_box(results)
        })
    });

    group.bench_function("if_exec", |b| {
        b.iter(|| {
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &if_module, &[]).unwrap();
            let eval_fn = instance.get_func(&mut store, "eval").unwrap();
            let mut results = vec![Val::null_any_ref()];
            eval_fn.call(&mut store, &[], &mut results).unwrap();
            black_box(results)
        })
    });

    group.bench_function("loop_10_exec", |b| {
        b.iter(|| {
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &loop_module, &[]).unwrap();
            let eval_fn = instance.get_func(&mut store, "eval").unwrap();
            let mut results = vec![Val::null_any_ref()];
            eval_fn.call(&mut store, &[], &mut results).unwrap();
            black_box(results)
        })
    });

    group.finish();
}

// =============================================================================
// WASM Size Benchmarks
// =============================================================================

fn bench_wasm_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasm_size");

    let expressions = [
        ("literal", "42"),
        ("add", "(+ 1 2)"),
        ("vector_3", "[1 2 3]"),
        ("map_3", "{1 2 3 4 5 6}"),
        ("set_3", "#{1 2 3}"),
        ("if", "(if true 1 2)"),
        ("cond_3", "(cond false 1 false 2 true 3)"),
        ("fn_id", "((fn [x] x) 42)"),
        ("loop", "(loop [x 0] (if (< x 5) (recur (+ x 1)) x))"),
    ];

    for (name, expr) in expressions {
        group.bench_with_input(BenchmarkId::new("size", name), &expr, |b, expr| {
            b.iter(|| {
                let wasm = compile_expr(black_box(expr));
                black_box(wasm.len())
            })
        });
    }

    group.finish();
}

// =============================================================================
// Scaling Benchmarks
// =============================================================================

fn bench_vector_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_scaling");

    for size in [10, 32, 64, 100].iter() {
        // Build expression to create vector of given size
        let expr = format!(
            "(loop [v [] i 0] (if (< i {}) (recur (conj v i) (+ i 1)) v))",
            size
        );

        group.bench_with_input(BenchmarkId::new("build", size), &expr, |b, expr| {
            b.iter(|| compile_expr(black_box(expr)))
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_compilation,
    bench_execution,
    bench_wasm_size,
    bench_vector_scaling,
);

criterion_main!(benches);
