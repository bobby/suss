//! Execute the production runtime's entropy boundary without source printing.
mod support;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use suss_compile::runtime_abi;
use wasmtime::{Func, Instance, Module, Store, Val};

#[test]
fn random_words_are_consumed_once_and_mapped_to_binary64() {
    let engine = support::engine();
    let bytes = runtime_abi::module();
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ImportSection(section) = payload.unwrap() {
            assert_eq!(section.count(), 0, "the runtime has no hidden host imports");
        }
    }
    let module = Module::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let install = runtime
        .get_func(&mut store, "random-source-install")
        .unwrap();
    let sample = runtime
        .get_typed_func::<(), f64>(&mut store, "random-double-raw")
        .unwrap();
    let words = [0_u64, 2048, 1_u64 << 63, u64::MAX];
    let expected = [
        0,
        0x3ca0000000000000,
        0x3fe0000000000000,
        0x3fefffffffffffff,
    ];
    let draws = Arc::new(AtomicUsize::new(0));
    let observed = draws.clone();
    let provider = Func::wrap(&mut store, move || {
        words[observed.fetch_add(1, Ordering::SeqCst)] as i64
    });
    install
        .call(&mut store, &[Val::FuncRef(Some(provider))], &mut [])
        .unwrap();
    // The typed global must root the callback even after a collection.
    store.gc(None);
    for (index, expected) in expected.into_iter().enumerate() {
        assert_eq!(sample.call(&mut store, ()).unwrap().to_bits(), expected);
        assert_eq!(draws.load(Ordering::SeqCst), index + 1);
    }
    install
        .call(&mut store, &[Val::FuncRef(None)], &mut [])
        .unwrap();
    let error = sample
        .call(&mut store, ())
        .expect_err("absence cannot produce a constant fallback");
    assert!(error.is::<wasmtime::ThrownException>(), "{error:?}");
    assert_eq!(draws.load(Ordering::SeqCst), 4);
}
