//! Public IPending inspection must not demand or replay lazy computation.
use suss_compile::{
    portable::{self, SourceOrigin, resolve::Phase},
    portable_session::{Session, SessionError},
};
use suss_reader::{Symbol, forms::read_forms};
use wasmtime::{
    Config, Engine, Store, Val,
    component::{Component, Linker},
};

fn number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    session
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let Val::F64(bits) = object.field(&mut store, 0)? else {
                return Err(wasmtime::Error::msg("Expected portable Number result"));
            };
            Ok(f64::from_bits(bits))
        })
        .unwrap()
}

fn status(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    session
        .inspect(&value, |store, value| {
            let raw = value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .ok_or_else(|| wasmtime::Error::msg("Expected portable Boolean sentinel"))?;
            match raw.get_u32() {
                2 => Ok(false),
                4 => Ok(true),
                _ => Err(wasmtime::Error::msg("Unexpected language sentinel")),
            }
        })
        .unwrap()
}

#[test]
fn public_realized_inspects_lazy_values_without_forcing_in_both_phases_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def effects 0)").unwrap();
        session.eval("(def pending (new cljs.core/LazySeq nil (fn [] (do (set! effects (+ effects 1)) (list 42))) nil nil))").unwrap();
        session
            .eval("(def decorated (with-meta pending {:marker 73}))")
            .unwrap();
        for _ in 0..2 {
            assert!(!status(&mut session, "(realized? decorated)"));
            assert_eq!(number(&mut session, "(if (realized? decorated) 1 0)"), 0.0);
            assert_eq!(number(&mut session, "effects"), 0.0);
        }
        assert_eq!(number(&mut session, "(get (meta decorated) :marker)"), 73.0);
        assert_eq!(number(&mut session, "(first decorated)"), 42.0);
        for _ in 0..2 {
            assert!(status(&mut session, "(realized? decorated)"));
            assert_eq!(
                number(&mut session, "(if (cljs.core/realized? decorated) 1 0)"),
                1.0
            );
            assert_eq!(number(&mut session, "(first decorated)"), 42.0);
            assert_eq!(number(&mut session, "effects"), 1.0);
        }
        session.eval("(def empty-pending (new cljs.core/LazySeq nil (fn [] (do (set! effects (+ effects 1)) nil)) nil nil))").unwrap();
        assert_eq!(
            number(&mut session, "(if (realized? empty-pending) 1 0)"),
            0.0
        );
        assert_eq!(number(&mut session, "effects"), 1.0);
        assert_eq!(
            number(&mut session, "(if (nil? (first empty-pending)) 1 0)"),
            1.0
        );
        assert_eq!(
            number(&mut session, "(if (realized? empty-pending) 1 0)"),
            1.0
        );
        assert_eq!(number(&mut session, "effects"), 2.0);
        for input in [
            "nil", "true", "42", ":marker", "\"text\"", "'ordinary-symbol",
            "'()", "'(1 2)", "[]", "{}", "#{}",
        ] {
            let source = format!("(realized? {input})");
            let error = session.eval(&source).unwrap_err();
            assert!(
                matches!(error, SessionError::Language(_)),
                "non-IPending input {input} must retain protocol failure: {error:?}"
            );
            // Each failed input must leave the next REPL evaluation usable.
            assert_eq!(number(&mut session, "42"), 42.0);
        }
        assert_eq!(
            number(&mut session, "(if (realized? decorated) 42 0)"),
            42.0
        );
    }
}

#[test]
fn public_realization_matches_actual_pinned_value_and_effect_observations() {
    use suss_compile::portable_macro_data::FormBridge;
    use suss_reader::forms::Kind;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/realized-observations.json"
    ))
    .unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["schema"], 1);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(include_str!("../../../tests/oracle/realized-fixture.sus"))
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for (case, (label, width, source)) in cases.iter().zip([
            ("lazy", 8, "(realized-lazy-observations)"),
            ("empty", 5, "(realized-empty-observations)"),
            ("sequence", 9, "(realized-sequence-observations)"),
            ("protocol", 3, "(realized-protocol-observations)"),
        ]) {
            assert_eq!(case[0], label);
            let expected = case[1].as_array().unwrap();
            assert_eq!(expected.len(), width);
            let value = session.eval(source).unwrap();
            session.collect().unwrap();
            let form = bridge.read(&mut session, &value, 0..source.len()).unwrap();
            let Kind::List(items) = form.into_parts().2 else {
                panic!("Expected actual sequence observations")
            };
            assert_eq!(items.len(), width);
            for (item, expected) in items.iter().zip(expected) {
                let Kind::Number(actual) = item.kind else {
                    panic!("Expected numeric observation")
                };
                assert_eq!(actual.to_bits(), expected.as_f64().unwrap().to_bits());
            }
        }
    }
}

#[test]
fn public_realized_dispatches_to_user_defined_pending_values() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def checks 0) (deftype PendingProbe [value] cljs.core/IPending (-realized? [this] (do (set! checks (+ checks 1)) value)))").unwrap();
        assert_eq!(
            number(
                &mut session,
                "(if (realized? (new PendingProbe false)) 1 0)"
            ),
            0.0
        );
        assert_eq!(
            number(&mut session, "(if (realized? (new PendingProbe true)) 1 0)"),
            1.0
        );
        assert_eq!(number(&mut session, "checks"), 2.0);
    }
}

#[test]
fn public_realized_executes_in_an_aot_component() {
    let source = "(ns user) (def calculate (fn [] (let [pending (new cljs.core/LazySeq nil (fn [] (list 42)) nil nil)] (if (realized? pending) 0 (do (first pending) (if (realized? pending) 42 0))))))";
    let core = portable::bootstrap::shipped(Phase::Runtime).unwrap();
    let fragment = portable::prepare_fragment_forms_with_origin(
        read_forms(source).unwrap(),
        0..source.len(),
        &core.environment,
        Phase::Runtime,
        &mut NoMacros,
        Some(&SourceOrigin::new(source, Some("realized.sus".into()))),
    )
    .unwrap();
    let mut resolve = portable::aot::Resolve::new();
    let package = resolve
        .push_str(
            "api.wit",
            "package test:realized; world api { export calculate: func() -> f64; }",
        )
        .unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    let bytes = portable::aot::component(
        &[core.clone(), fragment],
        &resolve,
        world,
        &[("calculate".into(), Symbol::parse("user/calculate"))],
    )
    .unwrap();
    let mut config = Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .wasm_component_model(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(10_000_000).unwrap();
    let instance = Linker::<()>::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let calculate = instance
        .get_typed_func::<(), (f64,)>(&mut store, "calculate")
        .unwrap();
    assert_eq!(calculate.call(&mut store, ()).unwrap().0, 42.0);
}

struct NoMacros;

#[test]
fn pinned_jank_lazy_realization_block_executes_without_body_changes() {
    use suss_compile::portable_macros::CompiledMacros;
    use suss_reader::forms::{Form, Kind};
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock = std::process::Command::new("python3")
        .arg(repository.join("scripts/clojure_test_suite.py"))
        .arg("check")
        .output()
        .unwrap();
    assert!(lock.status.success(), "{}", String::from_utf8_lossy(&lock.stderr));
    let original = include_str!("../../../vendor/clojure-test-suite/upstream/test/clojure/core_test/realized_qmark.cljc");
    let forms = read_forms(original).unwrap();
    let mut pending: Vec<&Form> = forms.iter().collect();
    let mut selected = None;
    while let Some(form) = pending.pop() {
        if let Kind::List(items) = &form.kind {
            if matches!(items.first().map(|item| &item.kind), Some(Kind::Symbol(s)) if s.name == "testing")
                && matches!(items.get(1).map(|item| &item.kind), Some(Kind::String(label)) if label == &"lazy sequences".encode_utf16().collect::<Vec<_>>())
            {
                assert!(selected.replace(form).is_none(), "ambiguous pinned block");
            }
        }
        if let Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) = &form.kind {
            pending.extend(items);
        }
    }
    let selected = selected.expect("missing pinned lazy sequence block");
    let source = &original[selected.span.clone()];
    assert_eq!(source.matches("(is ").count(), 7);
    let mut macros = CompiledMacros::new().unwrap();
    // Test-only assertion plumbing: evaluate each original assertion exactly
    // once, retain ordered success count, and fail at its original expression.
    macros.define("(defmacro testing [_label & body] `(do ~@body))").unwrap();
    macros.define("(defmacro is [form] `(do (set! assertions (+ assertions 1)) (if ~form nil (throw \"pinned realization assertion failed\"))))").unwrap();
    // Match the existing pinned-suite harness's execution allowance.
    macros.set_operation_fuel(200_000_000);
    let mut session = Session::new_repl().unwrap();
    session.set_operation_fuel(200_000_000);
    session.eval("(def assertions 0)").unwrap();
    session.eval_with_macros(source, &mut macros).unwrap();
    assert_eq!(number(&mut session, "assertions"), 7.0);
}

impl portable::ExpansionHost for NoMacros {
    fn expand(
        &mut self,
        _: &suss_reader::forms::Form,
        _: portable::ExpansionContext<'_>,
    ) -> Result<Option<suss_reader::forms::Form>, portable::Diagnostic> {
        Ok(None)
    }
}
