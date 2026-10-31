//! Execute the versioned bootstrap in real, separate phase Stores.
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_compile::portable::{bootstrap, resolve::Phase};
use suss_reader::{
    edn::{self, Value},
    forms::Kind,
};

#[test]
fn compiled_bootstrap_images_are_reproducible_and_phase_specific() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let shipped = bootstrap::shipped(phase).unwrap();
        assert!(std::ptr::eq(shipped, bootstrap::shipped(phase).unwrap()));
        let (generated, manifest) = bootstrap::generate(phase).unwrap();
        assert_eq!(generated, shipped.wasm);
        let encoded = edn::to_string(&manifest).unwrap().into_bytes();
        let restored = bootstrap::restore(phase, &generated, &encoded).unwrap();
        assert_eq!(restored.cells, shipped.cells);
        let other = match phase {
            Phase::Runtime => Phase::Macro,
            Phase::Macro => Phase::Runtime,
        };
        assert!(bootstrap::restore(other, &generated, &encoded).is_err());
    }
}

#[test]
fn compiled_bootstrap_rejects_changed_identity_and_corruption() {
    let phase = Phase::Macro;
    let (wasm, manifest) = bootstrap::generate(phase).unwrap();
    let Value::Map(base) = edn::to_value(&manifest).unwrap() else { panic!() };
    let encode = |map: &std::collections::BTreeMap<Value, Value>| {
        edn::print(&Value::Map(map.clone())).unwrap().into_bytes()
    };
    // The canonical text restores; the test edits below are what fail it.
    assert!(bootstrap::restore(phase, &wasm, &encode(&base)).is_ok());
    for key in [
        "format-version",
        "phase",
        "source-sha256",
        "compiler-source-sha256",
        "dependency-graph-sha256",
        "runtime-abi",
        "compiler",
        "wasm-tools",
        "target",
        "flags",
        "wasm-sha256",
        "cells",
    ] {
        let mut changed = base.clone();
        let value = changed.get_mut(&Value::keyword(key)).unwrap_or_else(|| panic!("{key}"));
        *value = match value {
            Value::Number(number) => Value::Number(*number + 1.0),
            Value::Vector(_) => Value::Vector(vec![Value::String("changed".into())]),
            _ => Value::String("changed".into()),
        };
        assert!(bootstrap::restore(phase, &wasm, &encode(&changed)).is_err(), "{key}");
    }
    let encoded = encode(&base);
    let mut corrupt = wasm.clone();
    corrupt[0] ^= 1;
    assert!(bootstrap::restore(phase, &corrupt, &encoded).is_err());
    assert!(bootstrap::restore(phase, &wasm, b"{}").is_err());
    // JSON manifests from before #299 are rejected.
    assert!(bootstrap::restore(phase, &wasm, br#"{"format_version": 1}"#).is_err());
    let mut unknown = base;
    unknown.insert(Value::keyword("unexpected"), Value::Bool(true));
    assert!(bootstrap::restore(phase, &wasm, &encode(&unknown)).is_err());
}

#[test]
fn compiled_bootstrap_executes_macro_syntax_quote_without_host_evaluation() {
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define("(defmacro bootstrap-add [x] `(+ ~x 2))")
        .unwrap();
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let value = caller
            .eval_with_macros("(bootstrap-add 40)", &mut macros)
            .unwrap();
        caller.collect().unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        assert_eq!(
            bridge.read(&mut caller, &value, 0..1).unwrap().kind,
            Kind::Number(42.0)
        );
    }
}

#[test]
fn compiled_bootstrap_catalog_cache_never_shares_guest_state() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let new_session = || match phase {
            Phase::Runtime => Session::new_repl().unwrap(),
            Phase::Macro => Session::new_macro().unwrap(),
        };
        let mut first = new_session();
        first.eval("(ns cljs.core)").unwrap();
        first.eval("(def identity (fn [x] 99))").unwrap();
        first.eval("(ns user)").unwrap();
        let value = first.eval("(identity 42)").unwrap();
        let bridge = FormBridge::new(&mut first).unwrap();
        assert_eq!(
            bridge.read(&mut first, &value, 0..1).unwrap().kind,
            Kind::Number(99.0)
        );
        first
            .eval("(def bootstrap-owned (atom 41)) (swap! bootstrap-owned inc)")
            .unwrap();
        let mut second = new_session();
        assert!(second.eval("@bootstrap-owned").is_err());
        let value = second.eval("(identity 42)").unwrap();
        let bridge = FormBridge::new(&mut second).unwrap();
        second.collect().unwrap();
        assert_eq!(
            bridge.read(&mut second, &value, 0..1).unwrap().kind,
            Kind::Number(42.0)
        );
        first.reset().unwrap();
        assert!(first.eval("@bootstrap-owned").is_err());
        let value = first.eval("(identity 42)").unwrap();
        let bridge = FormBridge::new(&mut first).unwrap();
        first.collect().unwrap();
        assert_eq!(
            bridge.read(&mut first, &value, 0..1).unwrap().kind,
            Kind::Number(42.0)
        );
        let value = second.eval("(get (hash-map :answer 42) :answer)").unwrap();
        let bridge = FormBridge::new(&mut second).unwrap();
        second.collect().unwrap();
        assert_eq!(
            bridge.read(&mut second, &value, 0..1).unwrap().kind,
            Kind::Number(42.0)
        );
    }
}
