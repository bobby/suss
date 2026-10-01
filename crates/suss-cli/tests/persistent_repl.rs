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
fn persistent_session_command_repl_keeps_effects_and_captured_functions() {
    let output = repl(concat!(
        "(def counter 0)\n",
        "(def stamp (do (set! counter (+ counter 1)) counter))\n",
        "(set! counter 9)\n",
        "counter\n",
        "(do (def f (fn [] stamp)) (def old f) 0)\n",
        "(do (def f (fn [] 42)) 0)\n",
        "(old)\n",
        "(f)\n",
        "(defonce stamp (do (set! counter 99) 99))\n",
        "counter\n",
    ));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "0\n1\n9\n9\n0\n0\n1\n42\nnil\n9\n"
    );
}

#[test]
fn persistent_session_command_repl_recovers_without_publishing_failed_definitions() {
    let output = repl(concat!(
        "(def keep 17)\n",
        "(def keep missing)\n",
        "keep\n",
        "(def keep (do (def effect 23) (throw 31)))\n",
        "keep\n",
        "effect\n",
        "(+ 20 22)\n",
    ));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "17\n17\n17\n23\n42\n"
    );
    let errors = String::from_utf8(output.stderr).unwrap();
    assert_eq!(errors.lines().count(), 2, "{errors}");
    assert!(errors.contains("missing"), "{errors}");
    assert!(
        errors.contains("Uncaught language exception: 31"),
        "{errors}"
    );
}

#[test]
fn persistent_session_command_repl_displays_utf16_and_binary64_without_reexecution() {
    let output = repl(concat!(
        "(def calls 0)\n",
        "(do (set! calls (+ calls 1)) 1e21)\n",
        "calls\n",
        "\"\\uD800😀\\n\\\"\\\\\"\n",
        "##NaN\n##Inf\n##-Inf\nfalse\ntrue\nnil\n",
    ));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "0\n1e+21\n1\n\"\\ud800😀\\n\\\"\\\\\"\n##NaN\n##Inf\n##-Inf\nfalse\ntrue\nnil\n"
    );
}

#[test]
fn session_lifecycle_command_repl_reset_discards_bindings_and_allows_new_inputs() {
    let output = repl("(def keep 17)\n:reset\nkeep\n(+ 20 22)\n:quit\n99\n");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "17\nnil\n42\n");
    let errors = String::from_utf8(output.stderr).unwrap();
    assert_eq!(errors.lines().count(), 1, "{errors}");
    assert!(errors.contains("keep"), "{errors}");
}

#[test]
fn persistent_session_command_repl_reads_multiline_inputs_before_executing() {
    let output = repl(
        "; comment only\n(def calls 0)\n(def answer\n  (do (set! calls (+ calls 1))\n      42))\ncalls\nanswer\n(throw\n  31)\n(+ 20 22)\n",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "0\n42\n1\n42\n42\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Error: Uncaught language exception: 31\n"
    );
}

#[test]
fn persistent_session_command_repl_reports_malformed_and_incomplete_input() {
    let output = repl(")\n(+ 20 22)\n(do (throw 99)\n");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "42\n");
    let errors = String::from_utf8(output.stderr).unwrap();
    assert_eq!(errors.lines().count(), 2, "{errors}");
    assert!(errors.contains("Unexpected closing delimiter"), "{errors}");
    assert!(errors.contains("Unclosed collection"), "{errors}");
    assert!(!errors.contains("Uncaught language exception"), "{errors}");
}

#[test]
fn persistent_session_command_repl_reports_unsupported_objects_without_fabricating_strings() {
    let output = repl("(array)\n42\n");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "42\n");
    let errors = String::from_utf8(output.stderr).unwrap();
    assert_eq!(errors.lines().count(), 1, "{errors}");
    assert!(
        errors.contains("Value display does not yet support"),
        "{errors}"
    );
}

#[test]
fn persistent_session_command_repl_keeps_atoms_and_their_captured_values() {
    let output = repl(concat!(
        "(do (def state (atom 17)) 0)\n",
        "(do (def read-state (fn [] @state)) 0)\n",
        "(swap! state (fn [x] (+ x 1)))\n",
        "(read-state)\n",
        "(do (def holder (atom (fn [] 23))) (def old @holder) 0)\n",
        "(do (reset! holder (fn [] 31)) 0)\n",
        "(old)\n",
        "(@holder)\n",
        "(defonce state (atom 99))\n",
        "@state\n",
    ));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "0\n0\n18\n18\n0\n0\n23\n31\nnil\n18\n"
    );
}
