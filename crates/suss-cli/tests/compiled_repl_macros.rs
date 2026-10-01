use std::io::Write;
use std::process::{Command, Stdio};
fn repl(source: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn persistent_session_command_compiled_macros_keep_effects_and_old_expansions() {
    let output = repl(concat!(
        "(def counter 0)\n",
        "(defmacro add-two [x] (list '+ x 2))\n",
        "(def f (fn [] (add-two 40)))\n",
        "(add-two (do (set! counter (+ counter 1)) 40))\n",
        "counter\n",
        "(defmacro add-two [x] (list '+ x 3))\n",
        "(f)\n",
        "(add-two 39)\n",
        "(defmacro invoke [& xs] xs)\n",
        "(invoke + 20 22)\n",
    ));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "0\n#<function>\n#<function>\n42\n1\n#<function>\n42\n42\n#<function>\n42\n"
    );
}
#[test]
fn persistent_session_command_compiled_macros_namespace_errors_and_reset() {
    let output = repl(concat!(
        "(defmacro answer ([] 42))\n",
        ":in-ns other\n",
        "(defmacro answer [] 40)\n",
        "(+ (answer) 2)\n",
        "(user/answer)\n",
        "(defmacro answer [] unresolved)\n",
        "(+ (answer) 2)\n",
        "(defmacro bad [] (atom 1))\n",
        "(def ghost 0) (bad)\n",
        "ghost\n",
        ":reset\n",
        "(user/answer)\n",
        "(+ 20 22)\n",
    ));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "#<function>\nnil\n#<function>\n42\n42\n42\n#<function>\nnil\n42\n"
    );
    let errors = String::from_utf8(output.stderr).unwrap();
    assert_eq!(errors.lines().count(), 4, "{errors}");
    for expected in ["unresolved", "macro data", "ghost", "user/answer"] {
        assert!(errors.contains(expected), "{errors}");
    }
}

#[test]
fn namespace_session_command_compiled_macros_expand_load_and_reload() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("src/leaf.sus"),
        "(ns leaf) (def read (fn [] (user/add-two 40)))",
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
                "(defmacro add-two [x] (list '+ x 2))\n",
                ":load leaf\n",
                "(leaf/read)\n",
                "(defmacro add-two [x] (list '+ x 3))\n",
                "(leaf/read)\n",
                ":reload leaf\n",
                "(leaf/read)\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "#<function>\nnil\n42\n#<function>\n42\nnil\n43\n"
    );
}
#[test]
fn session_lifecycle_compiled_macro_reset_failure_preserves_both_phase_stores() {
    use suss_cli::{portable_macros::CompiledMacros, portable_repl, portable_session::Session};
    let mut runtime = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    portable_repl::evaluate_compiled(&mut runtime, &mut macros, "(def keep 17)").unwrap();
    portable_repl::evaluate_compiled(&mut runtime, &mut macros, "(defmacro answer [] 42)").unwrap();
    let old = runtime.eval("keep").unwrap();
    let before = runtime.stats();
    macros.set_operation_fuel(0);
    assert!(portable_repl::reset_compiled(&mut runtime, &mut macros).is_err());
    assert_eq!(runtime.stats(), before);
    assert!(runtime.inspect(&old, |_, _| Ok(())).is_ok());
    macros.set_operation_fuel(10_000_000);
    assert_eq!(
        portable_repl::evaluate_compiled(&mut runtime, &mut macros, "keep").unwrap(),
        "17"
    );
    assert_eq!(
        portable_repl::evaluate_compiled(&mut runtime, &mut macros, "(answer)").unwrap(),
        "42"
    );
    portable_repl::reset_compiled(&mut runtime, &mut macros).unwrap();
    assert!(runtime.inspect(&old, |_, _| Ok(())).is_err());
    assert!(portable_repl::evaluate_compiled(&mut runtime, &mut macros, "(answer)").is_err());
}
#[test]
fn persistent_session_command_compiled_macro_core_aliases_conditionals_and_shadowing() {
    let output = repl(concat!(
        "(cljs.core/defmacro a [] 42)\n",
        "(a)\n",
        "#?(:suss (suss.core/defmacro b [] 42) :cljs (throw 99))\n",
        "(b)\n",
        "(def defmacro (fn [a b c] (+ a (+ b c))))\n",
        "(defmacro 10 20 12)\n",
    ));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "#<function>\n42\n#<function>\n42\n#<function>\n42\n"
    );
}

#[test]
fn namespace_session_command_compiled_macro_definitions_respect_exclusions_and_aliases() {
    let output = repl(concat!(
        "(ns other (:refer-clojure :exclude [defmacro]) (:require [cljs.core :as c]))\n",
        "(defmacro forbidden [] 99)\n",
        "(c/defmacro allowed [] 42)\n",
        "(allowed)\n",
        "forbidden\n",
    ));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "nil\n#<function>\n42\n"
    );
    let errors = String::from_utf8(output.stderr).unwrap();
    assert_eq!(errors.lines().count(), 2, "{errors}");
    assert!(errors.contains("defmacro"), "{errors}");
    assert!(errors.contains("forbidden"), "{errors}");
}
