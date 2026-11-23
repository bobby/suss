//! Source namespaces must prepare local transformers in the compiled Macro phase.
use suss_cli::{portable_macros::CompiledMacros, portable_session::{Session, SessionOptions}};
use suss_compile::portable::resolve::Phase;
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};

fn value(session: &mut Session, macros: &mut CompiledMacros, source: &str) -> String {
    let value = session.eval_with_macros(source, macros).unwrap();
    suss_cli::portable_repl::NativeDisplay::default().display(session, &value).unwrap()
}

#[test]
fn source_namespace_local_macros_use_phase_imports_and_expand_following_forms() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("local")).unwrap();
    std::fs::write(root.path().join("local/reader.sus"), r##"(ns local.reader
  (:require [clojure.edn :as edn]))
(defmacro data [text] (list 'quote (edn/read-string text)))
(defmacro wrapped [text] `(vector (data ~text) :ok))
(def answer (wrapped "[1.25M #uuid \"1-2-3-4-5\"]"))
"##).unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let options = SessionOptions {
            source_paths: vec![root.path().to_owned()],
            ..SessionOptions::default()
        };
        let mut session = match phase {
            Phase::Runtime => Session::new_repl_with_options(options),
            Phase::Macro => Session::new_macro_with_options(options),
        }.unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        session.load_namespace_with_macros("local.reader", &mut macros).unwrap();
        assert_eq!(session.current_namespace(), "user");
        let value = session.eval("local.reader/answer").unwrap();
        session.collect().unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Vector(vec![
            Observation::Vector(vec![
                Observation::Decimal { unscaled: "125".into(), scale: 2 },
                Observation::Uuid { most: 0x100020003, least: 0x4000000000005 },
            ]),
            Observation::Keyword(None, "ok".encode_utf16().collect()),
        ]), "{phase:?}");
    }
}

#[test]
fn source_namespace_macro_replacements_roll_back_after_failed_reload_and_recover() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("app.sus");
    std::fs::write(&file, "(ns app) (defmacro answer [] 17) (def value (answer))").unwrap();
    let mut session = Session::new_repl_with_options(SessionOptions {
        source_paths: vec![root.path().to_owned()], ..SessionOptions::default()
    }).unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    session.load_namespace_with_macros("app", &mut macros).unwrap();
    assert_eq!(value(&mut session, &mut macros, "[app/value (app/answer)]"), "[17 17]");
    std::fs::write(&file, "(ns app) (defmacro answer [] 99) (def value (answer)) unresolved").unwrap();
    let failure = session.reload_namespace_with_macros("app", false, &mut macros).unwrap_err();
    assert!(failure.to_string().contains("unresolved"), "{failure}");
    session.collect().unwrap();
    assert_eq!(session.current_namespace(), "user");
    assert_eq!(value(&mut session, &mut macros, "[app/value (app/answer)]"), "[17 17]");
    std::fs::write(&file, "(ns app) (defmacro answer [] 23) (def value (answer))").unwrap();
    session.reload_namespace_with_macros("app", false, &mut macros).unwrap();
    assert_eq!(value(&mut session, &mut macros, "[app/value (app/answer)]"), "[23 23]");
}

#[test]
fn source_namespace_macros_do_not_leak_from_a_later_script_compile_failure() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.sus"), "(ns app) (defmacro answer [] 17) (def value (answer))").unwrap();
    let mut session = Session::new_repl_with_options(SessionOptions {
        source_paths: vec![root.path().to_owned()], ..SessionOptions::default()
    }).unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let failure = suss_cli::portable_repl::evaluate_script_compiled(&mut session, &mut macros,
        "(ns caller (:require [app])) unresolved", None).unwrap_err();
    assert!(failure.to_string().contains("unresolved"), "{failure}");
    assert!(session.eval_with_macros("app/value", &mut macros).is_err());
    assert!(session.eval_with_macros("(app/answer)", &mut macros).is_err());
    assert_eq!(session.current_namespace(), "user");
    session.load_namespace_with_macros("app", &mut macros).unwrap();
    assert_eq!(value(&mut session, &mut macros, "[app/value (app/answer)]"), "[17 17]");
}

#[test]
fn source_script_local_macros_keep_each_namespace_import_scope() {
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let result = suss_cli::portable_repl::evaluate_script_compiled(&mut session, &mut macros, r#"
(ns first (:require [clojure.edn :as edn]))
(defmacro data [text] (list 'quote (edn/read-string text)))
(def answer (data "1.25M"))
(ns second (:require [clojure.edn :as other]))
(defmacro data [text] (list 'quote (other/read-string text)))
(def answer (data "[7]"))
[first/answer second/answer]
"#, None).unwrap();
    assert_eq!(result, "[1.25M [7]]");
    assert_eq!(session.current_namespace(), "second");
}

#[test]
fn namespace_local_macros_preserve_successfully_loaded_macro_dependencies() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("tools.sus"),
        "(ns tools) (def effects (atom 0)) (swap! effects inc) (defmacro imported [] @effects)").unwrap();
    let file = root.path().join("app.sus");
    let prefix = "(ns app (:require-macros [tools :refer [imported]])) (defmacro answer [] (imported)) (def value (answer))";
    std::fs::write(&file, format!("{prefix} unresolved")).unwrap();
    let mut session = Session::new_repl_with_options(SessionOptions {
        source_paths: vec![root.path().to_owned()], ..SessionOptions::default()
    }).unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    assert!(session.load_namespace_with_macros("app", &mut macros).is_err());
    assert!(session.eval_with_macros("(app/answer)", &mut macros).is_err());
    assert_eq!(value(&mut session, &mut macros, "(tools/imported)"), "1");
    std::fs::write(&file, prefix).unwrap();
    session.load_namespace_with_macros("app", &mut macros).unwrap();
    assert_eq!(value(&mut session, &mut macros, "[app/value (app/answer) (tools/imported)]"), "[1 1 1]");
}

#[test]
fn source_namespace_local_macros_execute_in_actual_aot_with_original_positions() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("app.sus");
    let wit = root.path().join("probe.wit");
    let wasm = root.path().join("probe.wasm");
    std::fs::write(root.path().join("tools.sus"), "(ns tools) (defmacro same [x] x)").unwrap();
    std::fs::write(root.path().join("state.sus"), "(ns state) (def effects (atom 0))").unwrap();
    let source = concat!(
        "(ns app (:require [clojure.edn :as edn] [state]) (:require-macros [tools :refer [same]]))\n",
        "(swap! state/effects inc)\n",
        "(defmacro data [text] (same (list 'quote (edn/read-string text))))\n",
        "(defmacro where [] (list 'quote [(get (meta &form) :line) (get (meta &form) :column) (get (meta &form) :file)]))\n",
        "(def location (where))\n",
        r##"(def result (data "[1.25M #uuid \"1-2-3-4-5\"]"))
"##,
    );
    // Write first so the exact filename asserted by guest data is canonical.
    std::fs::write(&file, source).unwrap();
    let path = serde_json::to_string(file.canonicalize().unwrap().to_str().unwrap()).unwrap();
    let check = format!(r#"(def check (fn [unused] (if (and
      (= result [1.25M #uuid "1-2-3-4-5"])
      (= @state/effects 1) (= location [5 15 {path}])) 1 0)))"#);
    std::fs::write(&file, format!("{source}{check}\n")).unwrap();
    std::fs::write(&wit, "package test:inline; world w { export check: func(unused: u32) -> u32; }").unwrap();
    let compiled = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(root.path())
        .args(["--no-cache", "compile"]).arg(&file).args(["--src"]).arg(root.path())
        .args(["-w"]).arg(&wit).args(["--export", "check=app/check", "-o"]).arg(&wasm)
        .output().unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let executed = Command::new(env!("CARGO_BIN_EXE_suss")).current_dir(root.path())
        .args(["--no-cache", "run"]).arg(&wasm).args(["--invoke", "check", "0"]).output().unwrap();
    assert!(executed.status.success(), "{executed:?}");
    assert_eq!(String::from_utf8(executed.stdout).unwrap(), "1\n");
}
