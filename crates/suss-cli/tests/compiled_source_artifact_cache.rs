use suss_cli::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions},
};
use wasmtime::Val;

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
            if matches!(form.kind, Kind::List(_)) {
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
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                let [Val::F64(bits)] = fields.as_slice() else {
                    panic!("Expected number")
                };
                Ok(*bits)
            })
            .unwrap();
        assert_eq!(actual, bits);
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

fn number(runtime: &mut Session, macros: &mut CompiledMacros, source: &str) -> u64 {
    let value = runtime.eval_with_macros(source, macros).unwrap();
    runtime
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Expected number")
            };
            Ok(*bits)
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
    assert_eq!(number(&mut runtime, &mut macros, source), 42.0f64.to_bits());
    let before = macros.artifact_cache_stats();
    assert_eq!(number(&mut runtime, &mut macros, source), 42.0f64.to_bits());
    let after = macros.artifact_cache_stats();
    assert_eq!(after.hits, before.hits + 1);
    assert_eq!(after.misses, before.misses);
    assert_eq!(
        number(&mut runtime, &mut macros, "counter"),
        2.0f64.to_bits()
    );
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/observed)"),
        2.0f64.to_bits()
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
        42.0f64.to_bits()
    );
    std::fs::write(
        &helper,
        "(ns helper) (def bias 2) ; different dependency source, identical expansion",
    )
    .unwrap();
    let before = macros.artifact_cache_stats();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        42.0f64.to_bits()
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
        42.0f64.to_bits()
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
        43.0f64.to_bits()
    );
    assert_eq!(
        number(&mut runtime, &mut macros, "(old)"),
        42.0f64.to_bits()
    );
}

#[test]
fn source_artifact_cache_restores_declaration_provenance_after_script_compile_error() {
    let mut runtime = Session::new().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro keep [] 17)").unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(keep)"),
        17.0f64.to_bits()
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
        17.0f64.to_bits()
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
        42.0f64.to_bits()
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
        42.0f64.to_bits()
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
        42.0f64.to_bits()
    );
    assert_eq!(macros.artifact_cache_stats().misses, before.misses + 1);
    suss_cli::portable_repl::reset_compiled(&mut runtime, &mut macros).unwrap();
    assert_eq!(macros.artifact_cache_stats(), Default::default());
    assert!(runtime.eval_with_macros("(t/answer)", &mut macros).is_err());
    runtime.eval_with_macros(import, &mut macros).unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        42.0f64.to_bits()
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
        42.0f64.to_bits()
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
        42.0f64.to_bits()
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
        42.0f64.to_bits()
    );
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        42.0f64.to_bits()
    );
    assert_eq!(macros.artifact_cache_stats().hits, before.hits + 2);
    assert_eq!(macros.artifact_cache_stats().bypasses, before.bypasses);
}
