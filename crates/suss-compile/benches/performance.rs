//! Performance benchmarks for Suss compilation and execution
//!
//! Measures:
//! 1. Compilation time - How long to compile expressions to WASM
//! 2. Artifact initialization - Preflight, core/dependencies and expression execution
//! 3. Bundle Wasm size - All emitted module bytes, including the core bootstrap

use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use suss_compile::{CompiledExpr, Compiler, portable_session::Session};

/// Compile the complete shared-runtime bundle, including isolated macro expansion.
fn compile_expr(expr: &str) -> CompiledExpr {
    Compiler::new()
        .compile_expr_with_info(expr)
        .expect("compilation failed")
}

// =============================================================================
// Compilation Benchmarks
// =============================================================================

fn bench_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("compilation");

    // Simple expressions
    group.bench_function("literal_int", |b| b.iter(|| compile_expr(black_box("42"))));

    group.bench_function("add_2", |b| b.iter(|| compile_expr(black_box("(+ 1 2)"))));

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

    group.bench_function("set_3", |b| b.iter(|| compile_expr(black_box("#{1 2 3}"))));

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
    let mut group = c.benchmark_group("artifact_initialization");
    // Preparation and empty-Store creation happen outside the measured routine.
    // The measurement includes bundle preflight, core/dependency initialization
    // execution and Store teardown. It is not comparable to the old standalone-
    // module benchmark.
    for (name, source) in [
        ("add_2_exec", "(+ 1 2)"),
        ("nested_arithmetic_exec", "(+ (* 3 4) (- 10 5))"),
        ("vector_create_exec", "[1 2 3]"),
        ("vector_conj_exec", "(conj [1 2] 3)"),
        ("if_exec", "(if true 1 2)"),
        (
            "loop_10_exec",
            "(loop [x 0] (if (< x 10) (recur (+ x 1)) x))",
        ),
    ] {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    (
                        compile_expr(source),
                        Session::new().expect("session creation failed"),
                    )
                },
                |(artifact, mut session)| {
                    let value = artifact
                        .execute(&mut session)
                        .expect("execution failed")
                        .expect("missing expression result");
                    // Keep the result's owning Store alive through observation.
                    black_box((&session, &value));
                },
                BatchSize::PerIteration,
            )
        });
    }
    group.finish();
}

// =============================================================================
// WASM Size Benchmarks
// =============================================================================

fn bench_wasm_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("bundle_wasm_size");

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
                let bytes = wasm
                    .prepared
                    .as_ref()
                    .expect("missing bundle")
                    .modules()
                    .map(<[u8]>::len)
                    .sum::<usize>();
                black_box(bytes)
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
