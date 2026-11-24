//! Private capability transport; public eval must still use the host compiler.
mod support;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use suss_compile::{
    portable_session::{RuntimeNumber, runtime_number},
    runtime_abi,
};
use wasmtime::{Func, Instance, Module, Rooted, Store, Val};

#[test]
fn evaluation_capability_is_explicit_and_its_gc_payload_stays_in_one_store() {
    let engine = support::engine();
    let bytes = runtime_abi::module();
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ImportSection(section) = payload.unwrap() {
            assert_eq!(
                section.count(),
                0,
                "the core runtime has no hidden compiler import"
            );
        }
    }
    let module = Module::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let install = runtime
        .get_func(&mut store, "evaluation-source-install")
        .unwrap();
    let evaluate = runtime.get_func(&mut store, "evaluate-form").unwrap();
    let mut result = [Val::null_any_ref()];
    let error = evaluate
        .call(&mut store, &[Val::null_any_ref()], &mut result)
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>());

    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let signature = evaluate.ty(&store);
    // An echo callback tests only the typed capability boundary and roots. It
    // supplies no language eval implementation or special-case numeric result.
    let callback = Func::try_new(&mut store, signature, move |_, args, results| {
        count.fetch_add(1, Ordering::SeqCst);
        results[0] = args[0].clone();
        Ok(())
    })
    .unwrap();
    install
        .call(&mut store, &[Val::FuncRef(Some(callback))], &mut [])
        .unwrap();
    runtime
        .get_func(&mut store, "number-box")
        .unwrap()
        .call(&mut store, &[Val::F64(0.5)], &mut result)
        .unwrap();
    let held = result[0]
        .unwrap_anyref()
        .unwrap()
        .to_owned_rooted(&mut store)
        .unwrap();
    store.gc(None).unwrap();
    let input = held.to_rooted(&mut store);
    evaluate
        .call(&mut store, &[Val::AnyRef(Some(input))], &mut result)
        .unwrap();
    assert!(Rooted::ref_eq(&store, &input, &result[0].unwrap_anyref().unwrap()).unwrap());
    assert!(
        matches!(runtime_number(&mut store, &result[0]).unwrap(), Some(RuntimeNumber::Double(value)) if value.to_bits()==0.5_f64.to_bits())
    );
    evaluate
        .call(&mut store, &[Val::null_any_ref()], &mut result)
        .unwrap();
    assert!(result[0].unwrap_anyref().is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    install
        .call(&mut store, &[Val::FuncRef(None)], &mut [])
        .unwrap();
    assert!(
        evaluate
            .call(&mut store, &[Val::null_any_ref()], &mut result)
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
