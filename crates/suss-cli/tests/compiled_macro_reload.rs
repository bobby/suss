use suss_cli::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions},
};
use wasmtime::Val;

fn number(runtime: &mut Session, macros: &mut CompiledMacros, source: &str) -> u64 {
    let value = runtime.eval_with_macros(source, macros).unwrap();
    runtime
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Expected ordinary number")
            };
            Ok(*bits)
        })
        .unwrap()
}
fn runtime(root: &std::path::Path) -> Session {
    Session::with_options(SessionOptions {
        source_paths: vec![root.to_owned()],
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn namespace_session_compiled_macro_reload_observes_changed_source_and_keeps_old_expansions() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("tools.sus");
    std::fs::write(&source, "(ns tools) (defmacro answer [] 42)").unwrap();
    let mut runtime = runtime(project.path());
    let mut macros = CompiledMacros::new().unwrap();
    runtime
        .eval_with_macros(
            "(ns user (:require-macros [tools :as t])) (def old (fn [] (t/answer)))",
            &mut macros,
        )
        .unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(old)"),
        42.0f64.to_bits()
    );
    std::fs::write(&source, "(ns tools) (defmacro answer [] 43)").unwrap();
    runtime
        .eval_with_macros("(ns user (:require-macros [tools :as t]))", &mut macros)
        .unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        42.0f64.to_bits()
    );
    runtime
        .eval_with_macros(
            "(ns user (:require-macros ^{:reload :reload} [tools :as t]))",
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
fn namespace_session_compiled_macro_reload_all_updates_reachable_macro_dependencies() {
    let project = tempfile::tempdir().unwrap();
    let helper = project.path().join("helper.sus");
    std::fs::write(&helper, "(ns helper) (def bias 2)").unwrap();
    std::fs::write(
        project.path().join("tools.sus"),
        "(ns tools (:require [helper :as h])) (defmacro add [x] (list '+ x h/bias))",
    )
    .unwrap();
    let mut runtime = runtime(project.path());
    let mut macros = CompiledMacros::new().unwrap();
    runtime
        .eval_with_macros(
            "(ns user (:require-macros [tools :as t])) (def old (fn [] (t/add 40)))",
            &mut macros,
        )
        .unwrap();
    std::fs::write(&helper, "(ns helper) (def bias 3)").unwrap();
    runtime
        .eval_with_macros(
            "(ns user (:require-macros ^{:reload :reload} [tools :as t]))",
            &mut macros,
        )
        .unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/add 40)"),
        42.0f64.to_bits()
    );
    runtime
        .eval_with_macros(
            "(ns user (:require-macros ^{:reload :reload-all} [tools :as t]))",
            &mut macros,
        )
        .unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/add 40)"),
        43.0f64.to_bits()
    );
    assert_eq!(
        number(&mut runtime, &mut macros, "(old)"),
        42.0f64.to_bits()
    );
}

#[test]
fn namespace_session_compiled_macro_failed_reload_keeps_old_binding_and_retries_source() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("tools.sus");
    std::fs::write(&source, "(ns tools) (defmacro answer [] 42)").unwrap();
    let mut runtime = runtime(project.path());
    let mut macros = CompiledMacros::new().unwrap();
    runtime
        .eval_with_macros(
            "(ns user (:require-macros [tools :as t])) (def keep 17) (def old (fn [] (t/answer)))",
            &mut macros,
        )
        .unwrap();
    std::fs::write(&source, "(ns tools) (defmacro answer [] unresolved)").unwrap();
    let error = runtime
        .eval_with_macros(
            "(ns user (:require-macros ^{:reload :reload} [tools :as t])) (def ghost 99)",
            &mut macros,
        )
        .unwrap_err();
    assert!(error.to_string().contains("unresolved"), "{error}");
    assert_eq!(number(&mut runtime, &mut macros, "keep"), 17.0f64.to_bits());
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        42.0f64.to_bits()
    );
    assert!(runtime.eval_with_macros("ghost", &mut macros).is_err());
    std::fs::write(&source, "(ns tools) (defmacro answer [] 43)").unwrap();
    runtime
        .eval_with_macros("(ns user (:require-macros [tools :as t]))", &mut macros)
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
fn namespace_session_compiled_macro_reload_all_leaves_unreachable_modules_loaded() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("helper.sus"),
        "(ns helper) (def bias 2)",
    )
    .unwrap();
    std::fs::write(
        project.path().join("tools.sus"),
        "(ns tools (:require [helper :as h])) (defmacro answer [] (+ 40 h/bias))",
    )
    .unwrap();
    let unrelated = project.path().join("unrelated.sus");
    std::fs::write(
        &unrelated,
        "(ns unrelated) (def count 0) (set! count (+ count 1)) (defmacro calls [] count)",
    )
    .unwrap();
    let mut runtime = runtime(project.path());
    let mut macros = CompiledMacros::new().unwrap();
    runtime
        .eval_with_macros(
            "(ns user (:require-macros [tools :as t] [unrelated :as u]))",
            &mut macros,
        )
        .unwrap();
    std::fs::remove_file(&unrelated).unwrap();
    runtime
        .eval_with_macros(
            "(ns user (:require-macros ^{:reload :reload-all} [tools :as t] [unrelated :as u]))",
            &mut macros,
        )
        .unwrap();
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        42.0f64.to_bits()
    );
    assert_eq!(
        number(&mut runtime, &mut macros, "(u/calls)"),
        1.0f64.to_bits()
    );
}

#[test]
fn namespace_session_compiled_macro_reload_metadata_is_checked_before_publication() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("tools.sus");
    std::fs::write(&source, "(ns tools) (defmacro answer [] 42)").unwrap();
    let mut runtime = runtime(project.path());
    let mut macros = CompiledMacros::new().unwrap();
    runtime
        .eval_with_macros("(ns user (:require-macros [tools :as t]))", &mut macros)
        .unwrap();
    std::fs::write(&source, "(ns tools) (defmacro answer [] 43)").unwrap();
    for value in ["nil", "false"] {
        let input = format!("(ns user (:require-macros ^{{:reload {value}}} [tools :as t]))");
        runtime.eval_with_macros(&input, &mut macros).unwrap();
        assert_eq!(
            number(&mut runtime, &mut macros, "(t/answer)"),
            42.0f64.to_bits()
        );
    }
    let error = runtime
        .eval_with_macros(
            "(ns user (:require-macros ^{:reload 7} [tools :as t])) (def ghost 99)",
            &mut macros,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("reload metadata must"),
        "{error}"
    );
    assert!(runtime.eval_with_macros("ghost", &mut macros).is_err());
    let error = runtime
        .eval_with_macros(
            "(ns user (:require-macros ^{:reload :reload-all} [cljs.core :as c]))",
            &mut macros,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("bootstrap core cannot be source-reloaded"),
        "{error}"
    );
    assert_eq!(
        number(&mut runtime, &mut macros, "(t/answer)"),
        42.0f64.to_bits()
    );
}

#[test]
fn namespace_session_command_reloads_macro_source_from_libspec_metadata() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("src")).unwrap();
    std::fs::write(
        project.path().join("src/tools.sus"),
        "(ns tools) (defonce count 0) (set! count (+ count 1)) (defmacro answer [] count)",
    )
    .unwrap();
    std::fs::write(
        project.path().join("src/app.sus"),
        "(ns app (:require-macros ^{:reload :reload} [tools :as t])) (def read (fn [] (t/answer)))",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl")
        .current_dir(project.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            concat!(
                ":load app\n",
                "(app/read)\n",
                "(def old app/read)\n",
                ":load app\n",
                "(app/read)\n",
                ":reload app\n",
                "(app/read)\n",
                "(old)\n",
                ":reload-all app\n",
                "(app/read)\n",
                "(old)\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "nil\n1\n#<function>\nnil\n1\nnil\n2\n1\nnil\n3\n1\n"
    );
}
