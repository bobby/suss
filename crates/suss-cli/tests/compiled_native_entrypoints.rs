//! Native eval and file commands must execute the same portable pipeline as REPL.
use std::process::{Command, Output};

fn success(output: Output, expected: &str) {
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

#[test]
fn eval_and_file_commands_keep_compiled_macro_phases_and_initializer_effects() {
    let source = concat!(
        "(def counter 0)\n",
        "(defmacro bump [x] (list '+ x 2))\n",
        "(def old (fn [] (bump 40)))\n",
        "(defmacro bump [x] (list '+ x 3))\n",
        "(def value (do (set! counter (+ counter 1)) (bump 39)))\n",
        "[value (old) counter]\n",
    );
    success(
        Command::new(env!("CARGO_BIN_EXE_suss"))
            .args(["-e", source])
            .output()
            .unwrap(),
        "[42 42 1]\n",
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("script.sus");
    std::fs::write(&path, source).unwrap();
    success(
        Command::new(env!("CARGO_BIN_EXE_suss"))
            .arg(&path)
            .output()
            .unwrap(),
        "[42 42 1]\n",
    );
}

#[test]
fn eval_command_gives_compiled_macros_actual_lexical_environment() {
    let source = concat!(
        "(defmacro local-value [x]\n",
        "  (if (contains? (get &env :locals) x) x 99))\n",
        "(let [answer 42] (local-value answer))\n",
    );
    success(
        Command::new(env!("CARGO_BIN_EXE_suss"))
            .args(["-e", source])
            .output()
            .unwrap(),
        "42\n",
    );
}

#[test]
fn file_command_preserves_original_macro_call_positions_and_filename() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("locations.sus");
    std::fs::write(
        &path,
        concat!(
            "(defmacro location [] (list 'quote [(get (meta &form) :line) ",
            "(get (meta &form) :column) (get (meta &form) :file)]))\n",
            "\n",
            "(location)\n",
        ),
    )
    .unwrap();
    success(
        Command::new(env!("CARGO_BIN_EXE_suss"))
            .arg(&path)
            .output()
            .unwrap(),
        &format!("[3 1 {:?}]\n", path.to_str().unwrap()),
    );
}

#[test]
fn canonical_display_preserves_gc_data_numeric_spelling_and_utf16_without_growth() {
    use suss_cli::{portable_repl::NativeDisplay, portable_session::Session};
    let mut session = Session::new_repl().unwrap();
    let value = session
        .eval(r#"'(a :ns/k [1e21 1e-7 -0 ##NaN ##Inf ##-Inf "\ud800" "a\n\"\\"] {:a 2} #{3})"#)
        .unwrap();
    let expected = r#"(a :ns/k [1e+21 1e-7 0 ##NaN ##Inf ##-Inf "\ud800" "a\n\"\\"] {:a 2} #{3})"#;
    let mut display = NativeDisplay::default();
    assert_eq!(display.display(&mut session, &value).unwrap(), expected);
    let warmed = session.stats();
    for _ in 0..3 {
        session.collect().unwrap();
        assert_eq!(display.display(&mut session, &value).unwrap(), expected);
        let after = session.stats();
        assert_eq!(after.resident_fragments, warmed.resident_fragments);
        assert_eq!(after.binding_cells, warmed.binding_cells);
        assert_eq!(after.external_value_handles, warmed.external_value_handles);
    }
    session.reset().unwrap();
    let replacement = session.eval("[42]").unwrap();
    assert!(display.display(&mut session, &replacement).is_err());
    display.clear();
    assert_eq!(display.display(&mut session, &replacement).unwrap(), "[42]");
    display.clear();
    assert_eq!(session.stats().external_value_handles, 1);
}

#[test]
fn canonical_display_ignores_metadata_that_is_not_macro_data() {
    use suss_cli::{portable_repl::NativeDisplay, portable_session::Session};
    let mut session = Session::new_repl().unwrap();
    let value = session.eval("(with-meta [42] {:owner (atom 7)})").unwrap();
    session.collect().unwrap();
    let bridge = suss_cli::portable_macro_data::FormBridge::new(&mut session).unwrap();
    assert!(bridge.read(&mut session, &value, 0..0).is_err());
    assert_eq!(
        NativeDisplay::default()
            .display(&mut session, &value)
            .unwrap(),
        "[42]"
    );
}

#[test]
fn scripts_do_not_display_discarded_lazy_values() {
    success(
        Command::new(env!("CARGO_BIN_EXE_suss"))
            .args(["-e", concat!(
                "(def effects 0)\n",
                "(new cljs.core/LazySeq nil (fn [] (do (set! effects (+ effects 1)) (list 42))) nil nil)\n",
                "effects\n",
            )])
            .output().unwrap(),
        "0\n",
    );
}

#[test]
fn script_compile_failure_preserves_existing_runtime_bindings() {
    use suss_cli::{portable_macros::CompiledMacros, portable_repl, portable_session::Session};
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    session.eval("(def keep 17)").unwrap();
    let before = session.stats();
    assert!(
        portable_repl::evaluate_script_compiled(
            &mut session,
            &mut macros,
            "(def keep 99) unresolved",
            None,
        )
        .is_err()
    );
    assert_eq!(session.stats(), before);
    let value = session.eval("keep").unwrap();
    assert_eq!(portable_repl::display(&mut session, &value).unwrap(), "17");
}

#[test]
fn display_traversal_bound_reports_failure_and_recovers() {
    use suss_cli::{portable_repl::NativeDisplay, portable_session::Session};
    let mut session = Session::new_repl().unwrap();
    let source = format!("[{}]", vec!["1"; 4097].join(" "));
    // Construct the large fixture separately; exercise display at its ordinary budget.
    let fuel = session.options().fuel_per_operation;
    session.set_operation_fuel(100_000_000);
    let value = session.eval(&source).unwrap();
    session.set_operation_fuel(fuel);
    session.collect().unwrap();
    let mut display = NativeDisplay::default();
    let error = display.display(&mut session, &value).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Vector integer field exceeds transport bounds"),
        "{error}"
    );
    let value = session.eval("[42]").unwrap();
    assert_eq!(display.display(&mut session, &value).unwrap(), "[42]");
}
