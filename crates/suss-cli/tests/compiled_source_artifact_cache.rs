use suss_cli::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions},
};

/// An exact runtime number: a long, or a double by its bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Number {
    Long(i64),
    Double(u64),
}
/// The expected number for `value`: integral values are longs, as in Clojure.
fn num(value: f64) -> Number {
    if value.is_finite()
        && value.fract() == 0.0
        && !(value == 0.0 && value.is_sign_negative())
        && value.abs() < 9_223_372_036_854_775_808.0
    {
        Number::Long(value as i64)
    } else {
        Number::Double(value.to_bits())
    }
}
fn exact_number(store: impl wasmtime::AsContextMut, value: &wasmtime::Val) -> wasmtime::Result<Number> {
    Ok(
        match suss_compile::portable_session::runtime_number(store, value)?
            .expect("expected a runtime number")
        {
            suss_compile::portable_session::RuntimeNumber::Long(value) => Number::Long(value),
            suss_compile::portable_session::RuntimeNumber::Double(value) => {
                Number::Double(value.to_bits())
            }
        },
    )
}

#[test]
fn source_artifact_cache_executes_distinct_nan_payloads_and_unknown_host_bypass() {
    use suss_compile::portable::{self, ExpansionHost, artifact_cache::ArtifactCache};
    use suss_reader::forms::{Form, Kind};
    struct Host {
        cache: ArtifactCache,
        bits: u64,
        known: bool,
    }
    impl ExpansionHost for Host {
        fn expand(
            &mut self,
            form: &Form,
            _: portable::ExpansionContext<'_>,
        ) -> Result<Option<Form>, portable::Diagnostic> {
            if matches!(&form.kind, Kind::List(_)) {
                Ok(Some(Form {
                    kind: Kind::Number(f64::from_bits(self.bits)),
                    metadata: vec![],
                    span: form.span.clone(),
                }))
            } else {
                Ok(None)
            }
        }
        fn emit_fragment(
            &mut self,
            function: &portable::ir::Function,
            phase: portable::resolve::Phase,
            forms: &[Form],
            origin: Option<&portable::SourceOrigin>,
        ) -> Result<Vec<u8>, portable::Diagnostic> {
            self.cache
                .emit(function, phase, forms, origin, self.known.then_some(&[]))
        }
    }
    let mut runtime = Session::new().unwrap();
    let mut host = Host {
        cache: Default::default(),
        bits: 0x7ff8000000000001,
        known: true,
    };
    for bits in [0x7ff8000000000001, 0x7ff8000000000002, 0x7ff8000000000002] {
        host.bits = bits;
        let value = runtime.eval_with_macros("(payload)", &mut host).unwrap();
        let actual = runtime
            .inspect(&value, |mut store, value| {
                exact_number(&mut store, &value)
            })
            .unwrap();
        assert_eq!(actual, Number::Double(bits));
    }
    assert_eq!(host.cache.stats().misses, 2);
    assert_eq!(host.cache.stats().hits, 1);
    host.known = false;
    for _ in 0..2 {
        runtime.eval_with_macros("(payload)", &mut host).unwrap();
    }
    assert_eq!(host.cache.stats().bypasses, 2);
    assert_eq!(host.cache.stats().hits, 1);
}

fn number(runtime: &mut Session, macros: &mut CompiledMacros, source: &str) -> Number {
    let value = runtime.eval_with_macros(source, macros).unwrap();
    runtime
        .inspect(&value, |mut store, value| {
            exact_number(&mut store, &value)
        })
        .unwrap()
}

#[test]
fn source_artifact_cache_hits_keep_macro_effects_and_runtime_initializers() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("tools.sus"),
        "(ns tools) (def calls 0) (defmacro answer [] (do (set! calls (+ calls 1)) 42)) (defmacro observed [] calls)").unwrap();
    let mut runtime = Session::with_options(SessionOptions {
        source_paths: vec![project.path().to_owned()],
        ..Default::default()
    })
    .unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    runtime
        .eval_with_macros(
            "(ns user (:require-macros [tools :as t])) (def counter 0)",
            &mut macros,
        )
        .unwrap();
    let source = "(do (set! counter (+ counter 1)) (t/answer))";
    assert_eq!(number(&mut runtime, &mut macros, source), num(42.0));
    let before = macros.artifact_cache_stats();
    assert_eq!(number(&mut runtime, &mut macros, source), num(42.0));
    let after = macros.artifact_cache_stats();
    assert_eq!(after.hits, before.hits + 1);
    assert_eq!(after.misses, before.misses);
    assert_eq!(
        number(&mut runtime, &mut macros, "counter"),
        num(2.0)
    );
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/observed)"),
        num(2.0)
    );
}

#[test]
fn source_artifact_cache_uses_loaded_macro_graph_until_explicit_reload_all() {
    let project = tempfile::tempdir().unwrap();
    let helper = project.path().join("helper.sus");
    std::fs::write(&helper, "(ns helper) (def bias 2)").unwrap();
    std::fs::write(
        project.path().join("tools.sus"),
        "(ns tools (:require [helper :as h])) (defmacro answer [] (+ 40 h/bias))",
    )
    .unwrap();
    let mut runtime = Session::with_options(SessionOptions {
        source_paths: vec![project.path().to_owned()],
        ..Default::default()
    })
    .unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    runtime
        .eval_with_macros(
            "(ns user (:require-macros [tools :as t])) (def old (fn [] (t/answer)))",
            &mut macros,
        )
        .unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
    std::fs::write(
        &helper,
        "(ns helper) (def bias 2) ; different dependency source, identical expansion",
    )
    .unwrap();
    let before = macros.artifact_cache_stats();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
    assert_eq!(macros.artifact_cache_stats().hits, before.hits + 1);
    runtime
        .eval_with_macros(
            "(ns user (:require-macros ^{:reload :reload-all} [tools :as t]))",
            &mut macros,
        )
        .unwrap();
    let before = macros.artifact_cache_stats();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
    assert_eq!(macros.artifact_cache_stats().misses, before.misses + 1);
    std::fs::write(&helper, "(ns helper) (def bias 3)").unwrap();
    runtime
        .eval_with_macros(
            "(ns user (:require-macros ^{:reload :reload-all} [tools :as t]))",
            &mut macros,
        )
        .unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(43.0)
    );
    assert_eq!(
        number(&mut runtime, &mut macros, "(old)"),
        num(42.0)
    );
}

#[test]
fn source_artifact_cache_restores_declaration_provenance_after_script_compile_error() {
    let mut runtime = Session::new().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro keep [] 17)").unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(keep)"),
        num(17.0)
    );
    assert!(
        suss_cli::portable_repl::evaluate_script_compiled(
            &mut runtime,
            &mut macros,
            "(defmacro keep [] 99) unresolved",
            None
        )
        .is_err()
    );
    let before = macros.artifact_cache_stats();
    assert_eq!(
        number(&mut runtime, &mut macros, "(keep)"),
        num(17.0)
    );
    assert_eq!(macros.artifact_cache_stats().hits, before.hits + 1);
}

#[test]
fn source_artifact_cache_tracks_new_dependencies_failed_discovery_and_reset() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("tools.sus");
    std::fs::write(&source, "(ns tools) (defmacro answer [] 42)").unwrap();
    let mut runtime = Session::with_options(SessionOptions {
        source_paths: vec![project.path().to_owned()],
        ..Default::default()
    })
    .unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let import = "(ns user (:require-macros [tools :as t]))";
    let reload = "(ns user (:require-macros ^{:reload :reload-all} [tools :as t]))";
    runtime.eval_with_macros(import, &mut macros).unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
    std::fs::write(
        &source,
        "(ns tools (:require [new-helper :as h])) (defmacro answer [] (+ 40 h/bias))",
    )
    .unwrap();
    assert!(runtime.eval_with_macros(reload, &mut macros).is_err());
    let before = macros.artifact_cache_stats();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
    assert_eq!(macros.artifact_cache_stats().hits, before.hits + 1);
    std::fs::write(
        project.path().join("new_helper.sus"),
        "(ns new-helper) (def bias 2)",
    )
    .unwrap();
    runtime.eval_with_macros(reload, &mut macros).unwrap();
    let before = macros.artifact_cache_stats();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
    assert_eq!(macros.artifact_cache_stats().misses, before.misses + 1);
    suss_cli::portable_repl::reset_compiled(&mut runtime, &mut macros).unwrap();
    assert_eq!(macros.artifact_cache_stats(), Default::default());
    assert!(runtime.eval_with_macros("(t/answer)", &mut macros).is_err());
    runtime.eval_with_macros(import, &mut macros).unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
}

#[test]
fn source_artifact_cache_bypasses_partial_load_until_successful_reload() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("tools.sus");
    std::fs::write(&source, "(ns tools) (defmacro answer [] 42)").unwrap();
    let mut runtime = Session::with_options(SessionOptions {
        source_paths: vec![project.path().to_owned()],
        ..Default::default()
    })
    .unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let import = "(ns user (:require-macros [tools :as t]))";
    let reload = "(ns user (:require-macros ^{:reload :reload} [tools :as t]))";
    runtime.eval_with_macros(import, &mut macros).unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
    std::fs::write(
        &source,
        "(ns tools) (def partial 17) (defmacro answer [] 42) (throw \"broken initializer\")",
    )
    .unwrap();
    assert!(runtime.eval_with_macros(reload, &mut macros).is_err());
    // The partially published macro still executes. Its unchanged expansion
    // cannot turn an incompletely loaded dependency graph into known provenance.
    let before = macros.artifact_cache_stats();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
    let after = macros.artifact_cache_stats();
    assert_eq!(after.bypasses, before.bypasses + 1);
    assert_eq!(after.hits, before.hits);
    assert_eq!(after.misses, before.misses);
    std::fs::write(&source, "(ns tools) (defmacro answer [] 42)").unwrap();
    runtime.eval_with_macros(reload, &mut macros).unwrap();
    let before = macros.artifact_cache_stats();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        num(42.0)
    );
    assert_eq!(macros.artifact_cache_stats().hits, before.hits + 2);
    assert_eq!(macros.artifact_cache_stats().bypasses, before.bypasses);
}

#[test]
fn source_artifact_key_admission_keeps_effects_and_executes_uncached_large_inputs() {
    let mut runtime = Session::new_repl().unwrap();
    runtime.eval("(def review-cache-runtime-calls 0)").unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro review-cache-large [] (defonce review-cache-macro-calls 0) (set! review-cache-macro-calls (+ review-cache-macro-calls 1)) review-cache-macro-calls)").unwrap();
    let source = format!(
        "(do (set! review-cache-runtime-calls (+ review-cache-runtime-calls 1)) (+ review-cache-runtime-calls (review-cache-large))) ;{}",
        "x".repeat(128 * 1024)
    );
    let before = macros.artifact_cache_stats();
    for expected in [2.0f64, 4.0] {
        assert_eq!(
            number(&mut runtime, &mut macros, &source),
            num(expected)
        );
        runtime.collect().unwrap();
        let now = macros.artifact_cache_stats();
        assert_eq!(now.entries, before.entries);
        assert_eq!(now.bytes, before.bytes);
        assert_eq!(now.hits, before.hits);
        assert_eq!(now.misses, before.misses);
        assert_eq!(now.bypasses, before.bypasses + (expected as usize / 2));
    }
    let next_macro_count = number(&mut runtime, &mut macros, "(review-cache-large)");
    assert_eq!(
        next_macro_count,
        num(3.0),
        "uncached emission must not suppress or replay macro effects"
    );
    let runtime_count = runtime.eval("review-cache-runtime-calls").unwrap();
    let actual = runtime
        .inspect(&runtime_count, |mut store, value| {
            exact_number(&mut store, &value)
        })
        .unwrap();
    assert_eq!(
        actual,
        num(2.0),
        "each runtime initializer executes exactly once"
    );
}

#[test]
fn large_ir_artifact_key_decline_executes_every_expansion() {
    use suss_compile::portable::{self, ExpansionHost, artifact_cache::ArtifactCache};
    use suss_reader::{
        Symbol,
        forms::{Form, Kind},
    };
    struct Host {
        cache: ArtifactCache,
        expansions: usize,
    }
    impl ExpansionHost for Host {
        fn expand(
            &mut self,
            form: &Form,
            _: portable::ExpansionContext<'_>,
        ) -> Result<Option<Form>, portable::Diagnostic> {
            let Kind::List(items) = &form.kind else {
                return Ok(None);
            };
            if !matches!(items.first().map(|f| &f.kind), Some(Kind::Symbol(s)) if s.name == "large-ir")
            {
                return Ok(None);
            }
            self.expansions += 1;
            let mut arguments = vec![Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::Symbol(Symbol {
                    namespace: None,
                    name: "+".into(),
                }),
            }];
            arguments.extend((0..2000).map(|_| Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::Number(1.0),
            }));
            Ok(Some(Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::List(arguments),
            }))
        }
        fn emit_fragment(
            &mut self,
            function: &portable::ir::Function,
            phase: portable::resolve::Phase,
            forms: &[Form],
            origin: Option<&portable::SourceOrigin>,
        ) -> Result<Vec<u8>, portable::Diagnostic> {
            assert!(function.values.len() > 2000);
            self.cache.emit(function, phase, forms, origin, Some(&[]))
        }
    }
    let mut runtime = Session::new().unwrap();
    let mut host = Host {
        cache: ArtifactCache::default(),
        expansions: 0,
    };
    for round in 1..=2 {
        let value = runtime.eval_with_macros("(large-ir)", &mut host).unwrap();
        runtime.collect().unwrap();
        runtime
            .inspect(&value, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                assert_eq!(object.field(&mut store, 0)?.unwrap_f64(), 2000.0);
                Ok(())
            })
            .unwrap();
        assert_eq!(host.expansions, round);
        assert_eq!(host.cache.stats().bypasses, round);
        assert_eq!(host.cache.stats().entries, 0);
        assert_eq!(host.cache.stats().hits, 0);
    }
}
