use std::io::Write;
use std::process::{Command, Stdio};

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
fn namespace_session_command_loads_once_and_keeps_live_globals_and_old_captures() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("src")).unwrap();
    std::fs::write(project.path().join("src/counter.sus"), "(ns counter) (defonce calls 0) (def calls (+ calls 1)) (def value 17) (def f (fn [] value)) 0").unwrap();
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
                ":load counter\n",
                "counter/calls\n",
                "(do (def old counter/f) (def read-live (fn [] (counter/f))) 0)\n",
                ":load counter\n",
                "counter/calls\n",
                "(ns counter)\n",
                "(def f (fn [] 23))\n",
                "(ns user)\n",
                "(old)\n",
                "(read-live)\n",
                ":reload counter\n",
                "counter/calls\n",
                "(old)\n",
                "(read-live)\n",
                ":quit\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    // Loading preserves user scope; a definition displays its var.
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "nil\n1\n0\nnil\n1\nnil\n#'counter/f\nnil\n17\n23\nnil\n2\n17\n17\n"
    );
}

fn session_project() -> (tempfile::TempDir, suss_cli::portable_session::Session) {
    let project = tempfile::tempdir().unwrap();
    let session = suss_cli::portable_session::Session::with_options(
        suss_cli::portable_session::SessionOptions {
            source_paths: vec![project.path().to_owned()],
            ..Default::default()
        },
    )
    .unwrap();
    (project, session)
}
fn number(session: &mut suss_cli::portable_session::Session, source: &str) -> Number {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            exact_number(&mut store, &value)
        })
        .unwrap()
}

#[test]
fn namespace_session_reload_reads_changed_source_and_preserves_bindings_on_failure() {
    use suss_cli::portable_session::SessionError;
    let (project, mut session) = session_project();
    let path = project.path().join("counter.sus");
    std::fs::write(
        &path,
        "(ns counter) (def value 17) (def f (let [v value] (fn [] v)))",
    )
    .unwrap();
    session.load_namespace("counter").unwrap();
    session
        .eval("(def old counter/f) (def live (fn [] (counter/f))) (def effects 0)")
        .unwrap();
    std::fs::write(
        &path,
        "(ns counter) (def value 23) (def f (let [v value] (fn [] v)))",
    )
    .unwrap();
    assert!(session.load_namespace("counter").unwrap().is_none());
    assert_eq!(number(&mut session, "(live)"), num(17.0));
    session.reload_namespace("counter", false).unwrap();
    session.collect().unwrap();
    assert_eq!(number(&mut session, "(live)"), num(23.0));
    assert_eq!(number(&mut session, "(old)"), num(17.0));
    let stats = session.stats();
    std::fs::write(
        &path,
        "(ns counter) (set! user/effects 99) (def value missing)",
    )
    .unwrap();
    assert!(matches!(
        session.reload_namespace("counter", false),
        Err(SessionError::Module(_))
    ));
    assert_eq!(session.stats(), stats);
    assert_eq!(number(&mut session, "effects"), num(0.0));
    assert_eq!(number(&mut session, "counter/value"), num(23.0));
    std::fs::write(
        &path,
        "(ns counter) (set! user/effects 1) (def value (throw 31))",
    )
    .unwrap();
    assert!(matches!(
        session.reload_namespace("counter", false),
        Err(SessionError::Language(_))
    ));
    assert_eq!(number(&mut session, "effects"), num(1.0));
    assert_eq!(number(&mut session, "counter/value"), num(23.0));
    assert_eq!(number(&mut session, "(live)"), num(23.0));
    // Failed initialization clears the loaded identity; ordinary load retries it.
    std::fs::write(&path, "(ns counter) (def value 41) (def f (fn [] 41))").unwrap();
    assert!(session.load_namespace("counter").unwrap().is_some());
    assert_eq!(number(&mut session, "(live)"), num(41.0));
    assert_eq!(number(&mut session, "(old)"), num(17.0));
    assert_eq!(session.current_namespace(), "user");
}

#[test]
fn namespace_session_reload_all_visits_only_reachable_dependencies_in_require_order() {
    let (project, mut session) = session_project();
    session.eval("(def trace 0)").unwrap();
    std::fs::write(
        project.path().join("dep.sus"),
        "(ns dep) (set! user/trace (+ (* user/trace 10) 1)) (defonce calls 0) (def calls (+ calls 1))",
    )
    .unwrap();
    std::fs::write(
        project.path().join("target.sus"),
        "(ns target (:require [zed] [dep])) (defonce calls 0) (def calls (+ calls 1))",
    )
    .unwrap();
    std::fs::write(
        project.path().join("other.sus"),
        "(ns other) (defonce calls 0) (def calls (+ calls 1))",
    )
    .unwrap();
    std::fs::write(
        project.path().join("zed.sus"),
        "(ns zed) (set! user/trace (+ (* user/trace 10) 2))",
    )
    .unwrap();
    session.load_namespace("target").unwrap();
    assert_eq!(number(&mut session, "trace"), num(21.0));
    session.load_namespace("other").unwrap();
    session.reload_namespace("target", false).unwrap();
    assert_eq!(number(&mut session, "dep/calls"), num(1.0));
    assert_eq!(number(&mut session, "target/calls"), num(2.0));
    assert_eq!(number(&mut session, "trace"), num(21.0));
    session.reload_namespace("target", true).unwrap();
    assert_eq!(number(&mut session, "trace"), num(2121.0));
    assert_eq!(number(&mut session, "dep/calls"), num(2.0));
    assert_eq!(number(&mut session, "target/calls"), num(3.0));
    assert_eq!(number(&mut session, "other/calls"), num(1.0));
    assert!(session.load_namespace("other").unwrap().is_none());
    let stats = session.stats();
    assert!(session.reload_namespace("cljs.core", true).is_err());
    assert_eq!(session.stats(), stats);
}

#[test]
fn namespace_session_errors_are_deterministic_and_reload_failure_keeps_dependencies_provided() {
    use suss_cli::portable_session::SessionError;
    let (project, mut session) = session_project();
    let before = session.stats();
    let first = session.load_namespace("missing").unwrap_err().to_string();
    let second = session.load_namespace("missing").unwrap_err().to_string();
    assert_eq!(first, second);
    assert_eq!(session.stats(), before);
    std::fs::write(
        project.path().join("dep.sus"),
        "(ns dep) (defonce calls 0) (def calls (+ calls 1))",
    )
    .unwrap();
    let path = project.path().join("target.sus");
    std::fs::write(&path, "(ns target (:require [dep])) (def value 17)").unwrap();
    session.load_namespace("target").unwrap();
    std::fs::write(&path, "(ns target (:require [dep])) (def value (throw 31))").unwrap();
    assert!(matches!(
        session.reload_namespace("target", true),
        Err(SessionError::Language(_))
    ));
    assert_eq!(number(&mut session, "dep/calls"), num(2.0));
    assert_eq!(number(&mut session, "target/value"), num(17.0));
    std::fs::write(&path, "(ns target (:require [dep])) (def value 23)").unwrap();
    assert!(session.load_namespace("target").unwrap().is_some());
    assert_eq!(number(&mut session, "dep/calls"), num(2.0));
    assert_eq!(number(&mut session, "target/value"), num(23.0));
    std::fs::write(&path, "(ns wrong) (def value 99)").unwrap();
    let before = session.stats();
    let first = session
        .reload_namespace("target", false)
        .unwrap_err()
        .to_string();
    let second = session
        .reload_namespace("target", false)
        .unwrap_err()
        .to_string();
    assert_eq!(first, second);
    assert_eq!(session.stats(), before);
    assert_eq!(number(&mut session, "target/value"), num(23.0));
}

#[test]
fn namespace_session_shipped_core_loads_from_supplied_source_roots() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("app")).unwrap();
    // Core collection functions come from the shipped compiled core; the
    // namespace comes from the caller's root, not the default ./src.
    std::fs::write(
        project.path().join("app/data.cljc"),
        "(ns app.data) (def total (count (conj [1 2] #?(:suss 3 :default 4))))",
    )
    .unwrap();
    let mut session = suss_cli::portable_session::Session::new_repl_with_options(
        suss_cli::portable_session::SessionOptions {
            source_paths: vec![project.path().to_owned()],
            ..Default::default()
        },
    )
    .unwrap();
    session.load_namespace("app.data").unwrap();
    assert_eq!(number(&mut session, "app.data/total"), num(3.0));
    let missing = suss_cli::portable_session::Session::new_repl()
        .unwrap()
        .load_namespace("app.data");
    assert!(missing.is_err(), "the default root must not see the supplied project");
}
