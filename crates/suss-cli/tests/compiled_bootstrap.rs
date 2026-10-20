//! Execute the versioned bootstrap in real, separate phase Stores.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;

use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_compile::portable::{bootstrap, resolve::Phase};
use suss_reader::forms::Kind;

// Cache isolation is a semantic test, not a default compilation-budget control.
// Use the ordinary no-expansion pipeline with one cumulative compilation-only
// allowance; Session execution and inspection retain their default Runtime fuel.
fn eval_isolation(
    session: &mut Session,
    source: &str,
) -> Result<suss_cli::portable_session::SessionValue, suss_cli::portable_session::SessionError> {
    struct Compilation(u64);
    impl suss_compile::portable::ExpansionHost for Compilation {
        fn expansion_work_fuel(&self) -> u64 {
            self.0
        }
        fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> {
            Some(vec![])
        }
        fn charge_expansion_work(
            &mut self,
            work: u64,
            span: std::ops::Range<usize>,
        ) -> Result<(), suss_compile::portable::Diagnostic> {
            self.0 =
                self.0
                    .checked_sub(work)
                    .ok_or_else(|| suss_compile::portable::Diagnostic {
                        span,
                        message: "Bootstrap isolation compilation fuel exhausted".into(),
                    })?;
            Ok(())
        }
        fn expand(
            &mut self,
            _: &suss_reader::forms::Form,
            _: suss_compile::portable::ExpansionContext<'_>,
        ) -> Result<Option<suss_reader::forms::Form>, suss_compile::portable::Diagnostic> {
            Ok(None)
        }
    }
    session.eval_with_macros(source, &mut Compilation(compilation_fuel::COMPILATION_FUEL))
}

#[test]
fn compiled_bootstrap_images_are_reproducible_and_phase_specific() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let shipped = bootstrap::shipped(phase).unwrap();
        assert!(std::ptr::eq(shipped, bootstrap::shipped(phase).unwrap()));
        let (generated, manifest) = bootstrap::generate(phase).unwrap();
        assert_eq!(generated, shipped.wasm);
        let encoded = serde_json::to_vec(&manifest).unwrap();
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
    let base = serde_json::to_value(&manifest).unwrap();
    for key in [
        "format_version",
        "phase",
        "source_sha256",
        "compiler_source_sha256",
        "dependency_graph_sha256",
        "runtime_abi",
        "compiler",
        "wasm_tools",
        "target",
        "flags",
        "wasm_sha256",
        "cells",
    ] {
        let mut changed = base.clone();
        changed[key] = match &base[key] {
            serde_json::Value::Number(number) => serde_json::json!(number.as_u64().unwrap() + 1),
            serde_json::Value::Array(_) => serde_json::json!(["changed"]),
            _ => serde_json::json!("changed"),
        };
        assert!(
            bootstrap::restore(phase, &wasm, &serde_json::to_vec(&changed).unwrap()).is_err(),
            "{key}"
        );
    }
    let encoded = serde_json::to_vec(&manifest).unwrap();
    let mut corrupt = wasm.clone();
    corrupt[0] ^= 1;
    assert!(bootstrap::restore(phase, &corrupt, &encoded).is_err());
    assert!(bootstrap::restore(phase, &wasm, b"{}").is_err());
    let mut unknown = base;
    unknown["unexpected"] = serde_json::json!(true);
    assert!(bootstrap::restore(phase, &wasm, &serde_json::to_vec(&unknown).unwrap()).is_err());
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
        eval_isolation(&mut first, "(ns cljs.core)").unwrap();
        eval_isolation(&mut first, "(def identity (fn [x] 99))").unwrap();
        eval_isolation(&mut first, "(ns user)").unwrap();
        let value = eval_isolation(&mut first, "(identity 42)").unwrap();
        let bridge = FormBridge::new(&mut first).unwrap();
        assert_eq!(
            bridge.read(&mut first, &value, 0..1).unwrap().kind,
            Kind::Number(99.0)
        );
        eval_isolation(
            &mut first,
            "(def bootstrap-owned (atom 41)) (swap! bootstrap-owned inc)",
        )
        .unwrap();
        let mut second = new_session();
        assert!(eval_isolation(&mut second, "@bootstrap-owned").is_err());
        let value = eval_isolation(&mut second, "(identity 42)").unwrap();
        let bridge = FormBridge::new(&mut second).unwrap();
        second.collect().unwrap();
        assert_eq!(
            bridge.read(&mut second, &value, 0..1).unwrap().kind,
            Kind::Number(42.0)
        );
        first.reset().unwrap();
        assert!(eval_isolation(&mut first, "@bootstrap-owned").is_err());
        let value = eval_isolation(&mut first, "(identity 42)").unwrap();
        let bridge = FormBridge::new(&mut first).unwrap();
        first.collect().unwrap();
        assert_eq!(
            bridge.read(&mut first, &value, 0..1).unwrap().kind,
            Kind::Number(42.0)
        );
        let value = eval_isolation(&mut second, "(get (hash-map :answer 42) :answer)").unwrap();
        let bridge = FormBridge::new(&mut second).unwrap();
        second.collect().unwrap();
        assert_eq!(
            bridge.read(&mut second, &value, 0..1).unwrap().kind,
            Kind::Number(42.0)
        );
        for session in [&first, &second] {
            assert_eq!(
                session.options().fuel_per_operation,
                suss_compile::portable::DEFAULT_OPERATION_FUEL
            );
        }
    }
}
