//! Core host/Macro and live Var controls, independent of the public eval binding.
use suss_compile::{portable_macros::CompiledMacros, portable_session::Session};
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use suss_reader::forms::{ExactValue, Form, Kind};


fn expected(form: &Form) -> Observation {
    match &form.kind {
        Kind::Nil => Observation::Nil,
        Kind::Bool(v) => Observation::Bool(*v),
        Kind::Long(v) => Observation::Long(*v),
        Kind::Number(v) => Observation::Number(v.to_bits()),
        Kind::Exact(v) => match &v.value {
            ExactValue::Integer { value, big: true } => Observation::BigInt(value.to_string()),
            ExactValue::Ratio {
                numerator,
                denominator,
            } => Observation::Ratio(numerator.to_string(), denominator.to_string()),
            ExactValue::Decimal { unscaled, scale } => Observation::Decimal {
                unscaled: unscaled.to_string(),
                scale: *scale,
            },
            other => panic!("unassessed exact primary {other:?}"),
        },
        Kind::String(v) => Observation::String(v.clone()),
        Kind::Keyword(v) => Observation::Keyword(
            v.namespace.as_ref().map(|s| s.encode_utf16().collect()),
            v.name.encode_utf16().collect(),
        ),
        Kind::Symbol(v) => Observation::Symbol(
            v.namespace.as_ref().map(|s| s.encode_utf16().collect()),
            v.name.encode_utf16().collect(),
        ),
        Kind::Vector(v) => Observation::Vector(v.iter().map(expected).collect()),
        Kind::List(v) => Observation::List(v.iter().map(expected).collect()),
        Kind::Set(v) => Observation::Set(v.iter().map(expected).collect()),
        Kind::Map(v) => {
            assert_eq!(v.len() % 2, 0);
            Observation::Map(
                v.chunks_exact(2)
                    .map(|pair| (expected(&pair[0]), expected(&pair[1])))
                    .collect(),
            )
        }
        other => panic!("unassessed primary eval value {other:?}"),
    }
}

fn canonical(value: Observation) -> Observation {
    match value {
        Observation::Vector(v) => Observation::Vector(v.into_iter().map(canonical).collect()),
        Observation::List(v) => Observation::List(v.into_iter().map(canonical).collect()),
        Observation::Set(v) => {
            let mut v: Vec<_> = v.into_iter().map(canonical).collect();
            v.sort_by_cached_key(|value| format!("{value:?}"));
            Observation::Set(v)
        }
        Observation::Map(v) => {
            let mut v: Vec<_> = v
                .into_iter()
                .map(|(k, v)| (canonical(k), canonical(v)))
                .collect();
            v.sort_by_cached_key(|(key, _)| format!("{key:?}"));
            Observation::Map(v)
        }
        other => other,
    }
}

fn aot_missing_capability_report(program: &str, required: &str) {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.sus"), program).unwrap();
    std::fs::write(
        root.path().join("app.wit"),
        "package test:eval-capability; world w { export report: func() -> string; }",
    )
    .unwrap();
    let compiled = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "--no-cache",
            "compile",
            "app.sus",
            "-w",
            "app.wit",
            "--export",
            "report=eval-capability/report",
            "-o",
            "app.wasm",
        ])
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let bytes = std::fs::read(root.path().join("app.wasm")).unwrap();
    let mut depth = 0;
    let mut component = false;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        match payload.unwrap() {
            wasmparser::Payload::Version { encoding, .. } => {
                if depth == 0 {
                    component = encoding == wasmparser::Encoding::Component;
                }
                depth += 1;
            }
            wasmparser::Payload::End(_) => depth -= 1,
            wasmparser::Payload::ComponentImportSection(section) if depth == 1 => {
                assert_eq!(
                    section.count(),
                    0,
                    "the selected world has no compiler or other host imports"
                );
            }
            _ => {}
        }
    }
    assert!(component);
    assert_eq!(depth, 0);
    let executed = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["--no-cache", "run", "app.wasm", "--invoke", "report"])
        .output()
        .unwrap();
    assert!(
        executed.status.success(),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    assert_eq!(
        String::from_utf8(executed.stdout).unwrap().trim_end(),
        required
    );
}

fn promotion_review_controls(name: &str, source: &str, primary: &str, count: usize) {
    let forms = suss_reader::forms::read_forms(primary.trim()).unwrap();
    assert_eq!(forms.len(), 1);
    let Kind::Vector(rows) = &forms[0].kind else { panic!("primary must retain all ordered controls") };
    assert_eq!(rows.len(), count);
    let wanted = canonical(expected(&forms[0]));
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 80_000).unwrap();
        let source = source.replace("(catch Throwable ", "(catch :default ");
        match session.eval_with_macros(&source, &mut macros) {
            Ok(value) => {
                session.collect().unwrap();
                let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
                if actual != wanted { failures.push(format!("{name} {phase:?}: expected {wanted:?}, actual {actual:?}")); }
            }
            Err(error) => failures.push(format!("{name} {phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn actual_aot_namespace_catalog_reports_the_missing_explicit_capability() {
    aot_missing_capability_report(
        "(ns eval-capability) (def report (fn [] (try (str (ns-interns *ns*)) (catch :default e (.-message e)))))",
        "Native namespace catalog capability is unavailable");
}

#[test]
fn public_alias_preserves_primary_state_errors_and_argument_order() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/alias-primary/cases.json"
    )).unwrap();
    assert_eq!(rows.len(), 15);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        // The primary creates these namespaces. Establish the same objects via
        // ordinary native ns forms; create-ns is a separate public API gap.
        suss_compile::portable_repl::evaluate_script_compiled(
            &mut session, &mut macros,
            "(ns alias-target) (ns alias-other) (ns alias-user) (ns user)", None,
        ).unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let mut retained = Vec::new();
        for row in &rows {
            let source = row["expression"].as_str().unwrap()
                .replace("(create-ns ", "(the-ns ")
                .replace("clojure.core", "suss.core")
                .replace("(catch Throwable ", "(catch :default ")
                .replace("(.getMessage e)", "(ex-message e)");
            let value = session.eval_with_macros(&source, &mut macros).unwrap_or_else(|error| {
                panic!("{}: {}", row["name"], suss_compile::portable_repl::error_display(&mut session, &error))
            });
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            assert_eq!(forms.len(), 1);
            decoder.refill(40_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(&forms[0])), "{} {:?}", row["name"], session.phase());
            eprintln!("alias-primary-match {:?} {}", session.phase(), row["name"]);
            retained.push(value);
        }
        session.collect().unwrap();
        for (row, value) in rows.iter().zip(&retained) {
            let forms = suss_reader::forms::read_forms(row["expected"].as_str().unwrap()).unwrap();
            decoder.refill(40_000);
            assert_eq!(canonical(decoder.decode_session(&mut session, value).unwrap()), canonical(expected(&forms[0])));
        }
    }
}

#[test]
fn live_var_metadata_and_roots_survive_replacing_the_core_indexed_seq_constructor() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        session
            .eval("(ns suss.core) (def IndexedSeq nil)")
            .unwrap();
        session.enter_namespace("user").unwrap();
        let original = session.eval("(def ^{:probe 17} retained 42)").unwrap();
        let observe = session
            .eval("(fn [v] [(var? v) @v (:probe (meta v)) (= v (var retained))])")
            .unwrap();
        for (root, metadata) in [(42, 17), (43, 18)] {
            if root == 43 {
                session.eval("(def ^{:probe 18} retained 43)").unwrap();
            }
            session.collect().unwrap();
            let value = session.invoke(&observe, &[&original]).unwrap();
            decoder.refill(40_000);
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::Vector(vec![
                    Observation::Bool(true),
                    Observation::Long(root),
                    Observation::Long(metadata),
                    Observation::Bool(true),
                ]),
                "{:?}", session.phase()
            );
        }
    }
}

#[test]
fn var_metadata_uses_original_core_data_without_changing_user_resolution() {
    for constant in ["", "^:const "] {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
            session
                .eval("(def ^{:probe [0 [0]]} metadata-target 0)")
                .unwrap();
            let retained = session.eval("(var metadata-target)").unwrap();
            let observe = session.eval("(let [vector vector var? var? meta meta get get key :probe computed :computed] (fn [v] (vector (var? v) @v (get (meta v) key) (get (meta v) computed))))").unwrap();
            session.eval(&format!("(ns suss.core) (def count (fn [x] 99)) (def {constant}PersistentVector nil) (def {constant}Keyword nil)")).unwrap();
            session.enter_namespace("user").unwrap();
            for (root, metadata) in [(42, 17), (43, 18)] {
                session.eval(&format!("(def ^{{:probe [{metadata} [{}]] :computed (let [f count] (f 123))}} metadata-target {root}) (def public-count-result (let [f count] (f 123)))", metadata + 2)).unwrap();
                session.collect().unwrap();
                let value = session.invoke(&observe, &[&retained]).unwrap();
                decoder.refill(40_000);
                assert_eq!(
                    decoder.decode_session(&mut session, &value).unwrap(),
                    Observation::Vector(vec![
                        Observation::Bool(true),
                        Observation::Long(root),
                        Observation::Vector(vec![
                            Observation::Long(metadata),
                            Observation::Vector(vec![Observation::Long(metadata + 2)]),
                        ]),
                        Observation::Long(99),
                    ]),
                    "{:?} {constant}", session.phase()
                );
                let ordinary = session.eval("public-count-result").unwrap();
                assert_eq!(
                    decoder.decode_session(&mut session, &ordinary).unwrap(),
                    Observation::Long(99)
                );
            }
        }
    }
}

#[test]
fn namespace_catalog_requires_the_registered_namespace_identity() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let forged = session.eval_privileged("(Namespace. (symbol \"suss.core\") (array))").unwrap();
        let query = session.eval_with_macros("(fn [n] (ns-interns n))", &mut macros).unwrap();
        let error = session.invoke(&query, &[&forged]).unwrap_err();
        assert!(suss_compile::portable_repl::error_display(&mut session, &error)
            .contains("registered canonical Namespace"));
        let value = session.eval_with_macros("(let [n (:ns (meta #'identity))] (identical? n (the-ns (ns-name n))))", &mut macros).unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Bool(true));
    }
}

#[test]
fn namespace_catalog_core_functions_reuse_live_var_cells() {
    promotion_review_controls("core-function-catalog",
        include_str!("../../../results/m12-eval/namespace-core-functions-primary/expression.sus"),
        include_str!("../../../results/m12-eval/namespace-core-functions-primary/primary.stdout"), 3);
}

#[test]
fn namespace_catalog_returns_actual_user_core_and_macro_vars() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-eval/namespace-interns-primary/cases.json"
    )).unwrap();
    let primary = suss_reader::forms::read_forms(include_str!(
        "../../../results/m12-eval/namespace-interns-primary/primary.stdout"
    )).unwrap();
    let Kind::Vector(rows) = &primary[0].kind else { panic!("primary vector") };
    assert_eq!(cases.len(), 2);
    assert_eq!(rows.len(), cases.len());
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        for (case, required) in cases.iter().zip(rows) {
            let name = case["name"].as_str().unwrap();
            match session.eval_with_macros(case["expression"].as_str().unwrap(), &mut macros) {
                Ok(value) => {
                    session.collect().unwrap();
                    decoder.refill(40_000);
                    let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
                    let required = canonical(expected(required));
                    if actual == required { eprintln!("namespace-catalog-primary-match {phase:?} {name}"); }
                    else { failures.push(format!("{phase:?} {name}: {actual:?}, expected {required:?}")); }
                }
                Err(error) => failures.push(format!("{phase:?} {name}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn source_macro_function_namespace_preserves_unused_and_non_namespace_values() {
    let setup = include_str!("../../../results/m12-eval/function-namespace-boundaries-primary/setup.sus");
    let source = include_str!("../../../results/m12-eval/function-namespace-boundaries-primary/expression.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/function-namespace-boundaries-primary/primary.stdout")).unwrap();
    assert_eq!(forms.len(), 1);
    let wanted = canonical(expected(&forms[0]));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        for declaration in suss_reader::forms::read_forms(setup).unwrap() {
            macros.define(&setup[declaration.span.clone()]).unwrap();
        }
        let value = session.eval_with_macros(source, &mut macros).unwrap_or_else(|error| {
            panic!("{phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))
        });
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted, "{phase:?}");
    }
}

#[test]
fn namespace_catalog_includes_source_macro_vars_without_prior_var_references() {
    let setup = include_str!("../../../results/m12-eval/catalog-without-macro-reference-primary/setup.sus");
    let source = include_str!("../../../results/m12-eval/catalog-without-macro-reference-primary/expression.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/catalog-without-macro-reference-primary/primary.stdout")).unwrap();
    assert_eq!(forms.len(), 1);
    let wanted = canonical(expected(&forms[0]));
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(setup).unwrap();
        match session.eval_with_macros(source, &mut macros) {
            Ok(value) => {
                session.collect().unwrap();
                let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
                let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
                if actual != wanted { failures.push(format!("{phase:?}: {actual:?}, expected {wanted:?}")); }
            }
            Err(error) => failures.push(format!("{phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn staged_source_scripts_publish_real_macro_var_roots_after_linking() {
    let source = include_str!("../../../results/m12-eval/source-link-macro-vars-primary/script.sus");
    let primary = include_str!("../../../results/m12-eval/source-link-macro-vars-primary/primary.stdout");
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        match suss_compile::portable_repl::evaluate_script_compiled(&mut session, &mut macros, source, None) {
            Ok(actual) if actual.trim() == primary.trim() => {}
            Ok(actual) => failures.push(format!("{phase:?}: {actual}, expected {primary}")),
            Err(error) => failures.push(format!("{phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn staged_source_modules_publish_real_macro_var_roots_after_linking() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("staged_vars.sus"), include_str!("../../../results/m12-eval/source-link-macro-vars-primary/staged_vars.sus")).unwrap();
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/source-link-macro-vars-primary/primary.stdout")).unwrap();
    let wanted = canonical(expected(&forms[0]));
    let mut failures = Vec::new();
    for macro_phase in [false, true] {
        let options = suss_compile::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        };
        let mut session = if macro_phase { Session::new_macro_with_options(options) } else { Session::new_repl_with_options(options) }.unwrap();
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        match session.eval_with_macros("(ns user (:require [staged-vars])) staged-vars/result", &mut macros) {
            Ok(value) => {
                session.collect().unwrap();
                let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
                let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
                if actual != wanted { failures.push(format!("{phase:?}: {actual:?}, expected {wanted:?}")); }
            }
            Err(error) => failures.push(format!("{phase:?}: {}", suss_compile::portable_repl::error_display(&mut session, &error))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn forwarded_source_functions_retain_actual_current_namespace_identity() {
    let setup = include_str!("../../../results/m12-eval/function-namespace-identity-primary/setup.sus");
    let source = include_str!("../../../results/m12-eval/function-namespace-identity-primary/expression.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/function-namespace-identity-primary/primary.stdout")).unwrap();
    let wanted = canonical(expected(&forms[0]));
    let mut failures = Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        for declaration in suss_reader::forms::read_forms(setup).unwrap() {
            macros.define(&setup[declaration.span.clone()]).unwrap();
        }
        let value = session.eval_with_macros(source, &mut macros).unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
        if actual != wanted { failures.push(format!("{phase:?}: {actual:?}, expected {wanted:?}")); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn public_namespace_loaders_publish_canonical_source_macro_vars_and_reload_roots() {
    let root = tempfile::tempdir().unwrap();
    let first = include_str!("../../../results/m12-eval/public-loader-source-vars-primary/first.sus");
    let second = include_str!("../../../results/m12-eval/public-loader-source-vars-primary/second.sus");
    let forms = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/public-loader-source-vars-primary/primary.stdout")).unwrap();
    let Kind::Vector(rows) = &forms[0].kind else { panic!("primary vector") };
    assert_eq!(rows.len(), 3);
    let mut failures = Vec::new();
    for macro_phase in [false, true] {
        std::fs::write(root.path().join("staged_vars.sus"), first).unwrap();
        let options = suss_compile::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()], ..Default::default()
        };
        let mut session = if macro_phase { Session::new_macro_with_options(options) } else { Session::new_repl_with_options(options) }.unwrap();
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        if let Err(error) = session.load_namespace_with_macros("staged-vars", &mut macros) {
            failures.push(format!("{phase:?} load: {}", suss_compile::portable_repl::error_display(&mut session, &error)));
            continue;
        }
        let value = session.eval_with_macros("staged-vars/result", &mut macros).unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(&rows[0])), "{phase:?} first load");
        let held = session.eval_with_macros("(var staged-vars/staged-var)", &mut macros).unwrap();
        std::fs::write(root.path().join("staged_vars.sus"), second).unwrap();
        if let Err(error) = session.reload_namespace_with_macros("staged-vars", false, &mut macros) {
            failures.push(format!("{phase:?} reload: {}", suss_compile::portable_repl::error_display(&mut session, &error)));
            continue;
        }
        session.collect().unwrap();
        let value = session.eval_with_macros("staged-vars/result", &mut macros).unwrap();
        decoder.refill(40_000);
        assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), canonical(expected(&rows[1])), "{phase:?} reload");
        let current = session.eval_with_macros("(var staged-vars/staged-var)", &mut macros).unwrap();
        let identity = session.eval_with_macros("(fn [a b] (identical? a b))", &mut macros).unwrap();
        let same = session.invoke(&identity, &[&held, &current]).unwrap();
        decoder.refill(40_000);
        assert_eq!(decoder.decode_session(&mut session, &same).unwrap(), expected(&rows[2]), "{phase:?} reload preserves Var");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn root_audit_macro_reload_to_ordinary_definition_updates_retained_var_root_and_status() {
    let root = tempfile::tempdir().unwrap();
    let first = include_str!("../../../results/m12-eval/macro-to-ordinary-reload-primary/first.sus");
    let ordinary = include_str!("../../../results/m12-eval/macro-to-ordinary-reload-primary/ordinary.sus");
    let setup = include_str!("../../../results/m12-eval/macro-to-ordinary-reload-primary/setup.sus");
    let expression = include_str!("../../../results/m12-eval/macro-to-ordinary-reload-primary/expression.sus");
    let primary = suss_reader::forms::read_forms(include_str!("../../../results/m12-eval/macro-to-ordinary-reload-primary/primary.stdout")).unwrap();
    assert_eq!(primary.len(), 2);
    let mut failures = Vec::new();
    for macro_phase in [false, true] {
        let file = root.path().join("tools.sus");
        std::fs::write(&file, first).unwrap();
        let options = suss_compile::portable_session::SessionOptions { source_paths: vec![root.path().to_owned()], ..Default::default() };
        let mut session = if macro_phase { Session::new_macro_with_options(options) } else { Session::new_repl_with_options(options) }.unwrap();
        let phase = session.phase();
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros(setup, &mut macros).unwrap();
        for (index, expected_row) in primary.iter().enumerate() {
            if index == 1 {
                std::fs::write(&file, ordinary).unwrap();
                session.eval_with_macros("(ns user (:require-macros ^{:reload :reload} [tools :as t]))", &mut macros).unwrap();
            }
            let value = session.eval_with_macros(expression, &mut macros).unwrap();
            session.collect().unwrap();
            let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
            let actual = canonical(decoder.decode_session(&mut session, &value).unwrap());
            let wanted = canonical(expected(expected_row));
            if actual != wanted { failures.push(format!("{phase:?} stage{index}: {actual:?}, expected {wanted:?}")); }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn quoted_var_metadata_keeps_original_data_constructors_and_live_user_calls() {
    let primary = suss_reader::forms::read_forms("[([unbound-marker] {:key #{unbound-marker}}) ([x]) 99]").unwrap();
    let wanted = canonical(expected(&primary[0]));
    for constant in ["", "^:const "] {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
            let retained = session.eval("(def quoted-metadata-target 0)").unwrap();
            let observe = session.eval("(let [vector vector meta meta get get q :quoted a :arglists c :computed] (fn [v] (let [m (meta v)] (vector (get m q) (get m a) (get m c)))))").unwrap();
            session.enter_namespace("suss.core").unwrap();
            session.eval(&format!("(def count (fn [x] 99)) (def {constant}PersistentVector nil) (def {constant}Keyword nil)")).unwrap();
            session.enter_namespace("user").unwrap();
            session.eval("(def ^{:quoted '([unbound-marker] {:key #{unbound-marker}}) :arglists '([x]) :computed (let [f count] (f 123))} quoted-metadata-target 42)").unwrap();
            session.collect().unwrap();
            let value = session.invoke(&observe, &[&retained]).unwrap();
            assert_eq!(canonical(decoder.decode_session(&mut session, &value).unwrap()), wanted, "{:?} {constant}", session.phase());
        }
    }
}

#[test]
fn declaration_metadata_retains_quote_arity_errors_and_previous_roots() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let retained = session.eval("(def malformed-metadata-target 42)").unwrap();
        let observe = session.eval("(fn [v] @v)").unwrap();
        for quote in ["(quote)", "(quote 1 2)"] {
            let error = session.eval(&format!("(def ^{{:probe {quote}}} malformed-metadata-target 7)")).unwrap_err();
            let message = suss_compile::portable_repl::error_display(&mut session, &error);
            assert!(message.contains("quote requires exactly one operand"), "{message}");
            let value = session.invoke(&observe, &[&retained]).unwrap();
            let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
            assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Long(42));
        }
    }
}

#[test]
fn var_definition_orders_initializer_root_and_metadata_effects_like_primary() {
    let cases = [
        ("(def ^{:probe (do (swap! transition-events conj :metadata) :after)} transition-target (do (swap! transition-events conj :initializer) (throw (ex-info \"initializer stop\" {}))))", true, "[42 :before [:initializer]]"),
        ("(def ^{:probe (do (swap! transition-events conj :metadata) :after)} transition-target (do (swap! transition-events conj :initializer) 43))", false, "[43 :after [:initializer :metadata]]"),
        ("(def ^{:probe (do (swap! transition-events conj [:metadata @#'transition-target]) (throw (ex-info \"metadata stop\" {})))} transition-target (do (swap! transition-events conj :initializer) 44))", true, "[44 :after [:initializer [:metadata 44]]]"),
    ];
    assert_eq!(cases.iter().map(|row| row.2).collect::<Vec<_>>().join("\n") + "\n",
        include_str!("../../../results/native-host-foundation/definition-transition-primary/primary.stdout"));
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def transition-events (atom [])) (def ^{:probe :before} transition-target 42)").unwrap();
        let observe = session.eval("(fn [] [@#'transition-target (:probe (meta #'transition-target)) @transition-events])").unwrap();
        let mut decoder = Decoder::capture(&mut session, 40_000).unwrap();
        for (source, fails, primary) in cases {
            session.eval("(reset! transition-events [])").unwrap();
            assert_eq!(session.eval(source).is_err(), fails, "{:?}: {source}", session.phase());
            session.collect().unwrap();
            let value = session.invoke(&observe, &[]).unwrap();
            decoder.refill(40_000);
            let forms = suss_reader::forms::read_forms(primary).unwrap();
            assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), expected(&forms[0]), "{:?}: {source}", session.phase());
        }
    }
}
