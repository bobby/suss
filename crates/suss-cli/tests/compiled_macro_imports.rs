use std::io::Write;
use std::process::{Command, Stdio};

/// Execute a source macro graph through the shipped prompt. No registration API
/// or development JVM/Node supplies the macro functions to this command.
#[test]
fn namespace_session_compiled_macro_imports_execute_phase_dependencies_and_aliases() {
    let root = tempfile::tempdir().unwrap();
    let sources = root.path().join("src");
    std::fs::create_dir(&sources).unwrap();
    std::fs::write(
        sources.join("helper.sus"),
        "(ns helper) (def bias 2) (def bump (fn [x] (+ x bias)))",
    )
    .unwrap();
    std::fs::write(
        sources.join("tools.sus"),
        concat!(
            "(ns tools (:require [helper :as h])) ",
            "(defmacro add-two [x] (list '+ x (h/bump 0)))"
        ),
    )
    .unwrap();
    std::fs::write(
        sources.join("app.sus"),
        concat!(
            "(ns app (:require-macros [tools :as t :refer [add-two] ",
            ":rename {add-two plus-two}])) ",
            "(def first (fn [] (t/add-two 40))) ",
            "(def second (fn [] (plus-two 40)))"
        ),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl")
        .current_dir(root.path())
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
                "(app/first)\n",
                "(app/second)\n",
                // Runtime helper has distinct cells from the loaded Macro helper.
                ":load helper\n",
                "(set! helper/bias 100)\n",
                ":reload app\n",
                "(app/first)\n",
                "(app/second)\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "nil\n42\n42\nnil\n100\nnil\n42\n42\n"
    );
}

fn run_sources(sources: &[(&str, &str)], input: &str) -> std::process::Output {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    for (name, source) in sources {
        std::fs::write(root.path().join("src").join(name), source).unwrap();
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl")
        .current_dir(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn namespace_session_compiled_macro_imports_preserve_caller_and_runtime_on_failure() {
    let output = run_sources(
        &[
            ("tools.sus", "(ns tools) (defmacro answer [] 42)"),
            (
                "bad.sus",
                "(ns bad (:require-macros [tools :as t])) (def ghost 99) (def failed unresolved)",
            ),
            (
                "unknown.sus",
                "(ns unknown (:require-macros [tools :refer [absent]])) (def ghost 99)",
            ),
        ],
        concat!(
            "(def keep 17)\n",
            ":load bad\n",
            "keep\n",
            "bad/ghost\n",
            ":load unknown\n",
            "unknown/ghost\n",
            "(ns user (:require-macros [tools :as t :refer [answer]]))\n",
            "(t/answer)\n",
            "(answer)\n",
            ":reset\n",
            "(answer)\n",
            "(tools/answer)\n",
            "(+ 20 22)\n"
        ),
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "17\n17\nnil\n42\n42\nnil\n42\n"
    );
    let errors = String::from_utf8(output.stderr).unwrap();
    assert_eq!(errors.lines().count(), 6, "{errors}");
    for expected in [
        "unresolved",
        "bad/ghost",
        "absent",
        "unknown/ghost",
        "answer",
        "tools/answer",
    ] {
        assert!(errors.contains(expected), "{errors}");
    }
}

#[test]
fn namespace_session_compiled_macro_source_definitions_resolve_core_and_execute_once() {
    let output = run_sources(
        &[
            (
                "tools.sus",
                concat!(
                    "(ns tools (:refer-clojure :exclude [defmacro]) (:require [cljs.core :as c])) ",
                    "(def count 0) ",
                    "(set! count (+ count 1)) ",
                    "(c/defmacro answer [] (+ 41 count)) ",
                    "(def defmacro (fn [x] (+ x 1))) ",
                    "(def ordinary (defmacro 41))"
                ),
            ),
            (
                "app.sus",
                "(ns app (:require-macros [tools :refer [answer]])) (def read (fn [] (answer)))",
            ),
        ],
        concat!(
            ":load app\n",
            "(app/read)\n",
            ":reload app\n",
            "(app/read)\n",
            "(ns other (:require-macros [tools :refer [answer]]))\n",
            "(answer)\n"
        ),
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "nil\n42\nnil\n42\nnil\n42\n"
    );
}

#[test]
fn namespace_session_compiled_macro_aliases_are_separate_and_lexical_locals_win() {
    let output = run_sources(
        &[
            ("tools.sus", "(ns tools) (defmacro answer [] 42)"),
            ("runtime.sus", "(ns runtime) (def value 17)"),
            (
                "app.sus",
                concat!(
                    "(ns app (:require [runtime :as t]) (:require-macros [tools :as t :refer [answer]])) ",
                    "(def runtime (fn [] t/value)) ",
                    "(def qualified (fn [] (t/answer))) ",
                    "(def referred (fn [] (answer))) ",
                    "(def shadow (fn [] (let [answer (fn [] 39)] (answer))))"
                ),
            ),
        ],
        ":load app\n(app/runtime)\n(app/qualified)\n(app/referred)\n(app/shadow)\n",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "nil\n17\n42\n42\n39\n"
    );
}
