//! Execute the selected WIT randomness capability through real AOT artifacts.
mod support;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use suss_compile::portable::{
    self,
    resolve::{Environment, Phase},
};
use suss_reader::Symbol;
use wasmtime::{
    Store,
    component::{Component, Linker},
};

fn component(import: bool, optimize: bool) -> Vec<u8> {
    let mut resolve = wit_parser::Resolve::new();
    resolve
        .push_str(
            "random.wit",
            r#"package wasi:random@0.3.1;
        interface random {
            get-random-bytes: func(len: u64) -> list<u8>;
            get-random-u64: func() -> u64;
        }"#,
        )
        .unwrap();
    let source = format!(
        "package test:random; world api {{ {} export sample: func() -> f64; export discarded: func() -> f64; }}",
        if import {
            "import wasi:random/random@0.3.1;"
        } else {
            ""
        }
    );
    let package = resolve.push_str("api.wit", &source).unwrap();
    let world = resolve.packages[package].worlds["api"];
    let fragment = portable::prepare_fragment(
        "(def sample (fn [] (suss.bootstrap/random-double)))
         (def discarded (fn [] (do (suss.bootstrap/random-double)
                                   (suss.bootstrap/random-double)
                                   (suss.bootstrap/random-double))))",
        &Environment::default(),
        Phase::Runtime,
    )
    .unwrap();
    portable::aot::component_with(
        &[fragment],
        &resolve,
        world,
        &[
            ("sample".into(), Symbol::new("sample")),
            ("discarded".into(), Symbol::new("discarded")),
        ],
        optimize,
    )
    .unwrap()
}

fn execute(import: bool, optimize: bool) {
    let bytes = component(import, optimize);
    let engine = support::engine();
    let compiled = Component::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let mut linker = Linker::new(&engine);
    let draws = Arc::new(AtomicUsize::new(0));
    if import {
        let mut provider = linker.instance("wasi:random/random@0.3.1").unwrap();
        let observed = draws.clone();
        provider
            .func_wrap("get-random-u64", move |_, ()| {
                let words = [0_u64, 1_u64 << 63, u64::MAX];
                Ok((words[observed.fetch_add(1, Ordering::SeqCst) % words.len()],))
            })
            .unwrap();
        provider
            .func_wrap(
                "get-random-bytes",
                |_, (_len,): (u64,)| -> wasmtime::Result<(Vec<u8>,)> {
                    panic!("scalar sampling must consume one word, never request bytes");
                },
            )
            .unwrap();
    }
    let instance = linker.instantiate(&mut store, &compiled).unwrap();
    let sample = instance
        .get_typed_func::<(), (f64,)>(&mut store, "sample")
        .unwrap();
    assert_eq!(
        draws.load(Ordering::SeqCst),
        0,
        "compilation/initialization must not precompute draws"
    );
    if import {
        for expected in [0, 0x3fe0000000000000, 0x3fefffffffffffff] {
            assert_eq!(sample.call(&mut store, ()).unwrap().0.to_bits(), expected);
        }
        assert_eq!(draws.load(Ordering::SeqCst), 3);
        let discarded = instance
            .get_typed_func::<(), (f64,)>(&mut store, "discarded")
            .unwrap();
        assert_eq!(
            discarded.call(&mut store, ()).unwrap().0.to_bits(),
            0x3fefffffffffffff
        );
        assert_eq!(
            draws.load(Ordering::SeqCst),
            6,
            "unused random results must still consume provider words"
        );
    } else {
        assert!(
            sample.call(&mut store, ()).is_err(),
            "no hidden provider or constant fallback"
        );
        assert_eq!(draws.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn selected_random_import_is_lowered_and_installed() {
    execute(true, false);
}

#[test]
fn pure_world_keeps_its_import_contract_and_reports_missing_entropy() {
    execute(false, false);
}

#[test]
#[ignore = "requires Binaryen133 or later; run explicitly with --include-ignored and SUSS_WASM_OPT"]
fn optimized_selected_random_import_preserves_draws() {
    execute(true, true);
}
