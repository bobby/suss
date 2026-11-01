//! Runtime namespaces (#309): `*ns*`, namespace objects and a runtime symbol
//! resolution that agrees with the compiler's syntax quote. Clojure is the
//! oracle: loading binds `*ns*` to the namespace being loaded, macros see the
//! namespace being compiled, the REPL and scripts start in `user`, and code
//! called from outside any load sees the root value, here `suss.core`.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions, SessionValue},
};
use suss_reader::forms::Kind;

fn assert_true(session: &mut Session, value: &SessionValue, source: &str) {
    session.collect().unwrap();
    session
        .inspect(value, |store, value| {
            let bits = value
                .unwrap_anyref()
                .and_then(|value| value.as_i31(&store).ok().flatten())
                .map(|value| value.get_u32());
            assert_eq!(bits, Some(4), "{source} is not true");
            Ok(())
        })
        .unwrap();
}

fn truthy(session: &mut Session, macros: &mut CompiledMacros, source: &str) {
    let value = session
        .eval_with_macros(source, macros)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    assert_true(session, &value, source);
}

fn text(session: &mut Session, value: &SessionValue) -> String {
    session.collect().unwrap();
    let bridge = FormBridge::new(session).unwrap();
    match &bridge.read(session, value, 0..1).unwrap().kind {
        Kind::String(units) => String::from_utf16(units).unwrap(),
        kind => panic!("expected a string: {kind:?}"),
    }
}

const SOURCES: &[(&str, &str)] = &[
    (
        "demo/a.sus",
        "(ns demo.a)\n(def x 1)\n(def y 2)\n(def z 3)\n(def loaded (ns-name *ns*))\n(def called (fn [] (ns-name *ns*)))\n",
    ),
    ("demo/m.sus", "(ns demo.m)\n(defmacro mm [] 1)\n(defmacro current [] (list 'quote (ns-name *ns*)))\n"),
];

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (path, source) in SOURCES {
        let path = root.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    root
}

/// A Runtime and a Macro session over `root`, each with a macro host.
fn sessions(root: &Path) -> Vec<(Session, CompiledMacros)> {
    let options = SessionOptions {
        source_paths: vec![root.to_owned()],
        fuel_per_operation: 1_000_000_000,
    };
    [Session::new_repl_with_options(options.clone()), Session::new_macro_with_options(options)]
        .into_iter()
        .map(|session| {
            let mut macros = CompiledMacros::new().unwrap();
            macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
            (session.unwrap(), macros)
        })
        .collect()
}

#[test]
fn ns_is_bound_while_loading_and_rebindable() {
    let root = project();
    for (mut session, mut macros) in sessions(root.path()) {
        for source in [
            "(= 'user (ns-name *ns*))",
            "(= \"user\" (str *ns*))",
            "(ns demo.b (:require [demo.a]))",
            "(= 'demo.b (ns-name *ns*))",
            // A namespace's top-level forms run with *ns* bound to it.
            "(= 'demo.a demo.a/loaded)",
            // Called later, a function sees the caller's *ns*, as in Clojure.
            "(= 'demo.b (demo.a/called))",
            "(= 'suss.core (binding [*ns* (the-ns 'suss.core)] (demo.a/called)))",
            "(= 'demo.b (ns-name *ns*))",
        ] {
            if source.starts_with("(ns ") {
                session.eval_with_macros(source, &mut macros).unwrap();
            } else {
                truthy(&mut session, &mut macros, source);
            }
        }
        // Code called from the host, outside any load, sees the root value.
        let called = session.eval("demo.a/called").unwrap();
        let name = session.invoke(&called, &[]).unwrap();
        session.collect().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = bridge.read(&mut session, &name, 0..1).unwrap();
        assert!(
            matches!(&form.kind, Kind::Symbol(symbol) if symbol.namespace.is_none() && symbol.name == "suss.core"),
            "{form:?}"
        );
    }
}

#[test]
fn namespace_functions_follow_clojure() {
    let root = project();
    for (mut session, mut macros) in sessions(root.path()) {
        session
            .eval_with_macros("(ns demo.c (:require [demo.a :as a :refer [x y] :rename {y yy}]))", &mut macros)
            .unwrap();
        for source in [
            "(nil? (find-ns 'demo.nowhere))",
            "(identical? (find-ns 'demo.a) (the-ns 'demo.a))",
            "(identical? (the-ns 'demo.a) (the-ns (the-ns 'demo.a)))",
            "(= 'demo.a (ns-name 'demo.a))",
            "(= 'demo.a (ns-name (the-ns 'demo.a)))",
            "(= \"demo.a\" (str (the-ns 'demo.a)))",
            "(= \"No namespace: demo.nowhere found\" (try (the-ns 'demo.nowhere) (catch :default e (ex-message e))))",
            "(every? (set (map ns-name (all-ns))) '[suss.core user demo.a demo.c])",
            "(= {'a (the-ns 'demo.a)} (ns-aliases *ns*))",
            "(= {} (ns-aliases 'demo.a))",
            // Suss has no runtime Vars: refers map to the qualified symbol.
            "(= {'x 'demo.a/x 'yy 'demo.a/y} (ns-refers 'demo.c))",
            "(= (the-ns 'demo.a) (the-ns 'demo.a))",
            "(not= (the-ns 'demo.a) (the-ns 'demo.c))",
        ] {
            truthy(&mut session, &mut macros, source);
        }
        let printed = session.eval("(pr-str (the-ns 'demo.a))").unwrap();
        let printed = text(&mut session, &printed);
        assert!(
            printed.starts_with("#object[suss.core.Namespace 0x") && printed.ends_with(" \"demo.a\"]"),
            "{printed}"
        );
    }
}

#[test]
fn macros_see_the_namespace_being_compiled() {
    let root = project();
    for (mut session, mut macros) in sessions(root.path()) {
        for (source, check) in [
            ("(ns demo.d (:require-macros [demo.m :as m]))", "(= 'demo.d (m/current))"),
            ("(ns demo.e (:require-macros [demo.m :as m]))", "(= 'demo.e (m/current))"),
            ("(ns demo.d)", "(= 'demo.d (demo.m/current))"),
        ] {
            session.eval_with_macros(source, &mut macros).unwrap();
            truthy(&mut session, &mut macros, check);
        }
    }
}

/// Symbols whose runtime resolution must equal the compiler's syntax quote.
const CORPUS: &[&str] = &[
    "own", "later", "map", "first", "filter", "when", "deftype", "aget", "foo", "x", "yy", "y", "z",
    "a/z", "a/nowhere", "demo.a/z", "nope/bar", "suss.core/map", "clojure.core/map", "cljs.core/map",
    "String.", "js/Date", "m/mm", "mm", "current", "*ns*", "ns-name", "the-ns", "ex-info",
];

fn agreement(session: &mut Session, macros: &mut CompiledMacros) {
    let quoted = CORPUS.join(" ");
    let resolved = CORPUS
        .iter()
        .map(|symbol| format!("(suss.core/reader-resolve-symbol '{symbol})"))
        .collect::<Vec<_>>()
        .join(" ");
    let source = format!("(= `[{quoted}] [{resolved}])");
    let value = session
        .eval_with_macros(&source, macros)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.collect().unwrap();
    let matches = session
        .inspect(&value, |store, value| {
            Ok(value
                .unwrap_anyref()
                .and_then(|value| value.as_i31(&store).ok().flatten())
                .map(|value| value.get_u32())
                == Some(4))
        })
        .unwrap();
    if !matches {
        let both = session
            .eval_with_macros(&format!("(pr-str [`[{quoted}] [{resolved}]])"), macros)
            .unwrap();
        panic!("runtime resolution differs from syntax quote: {}", text(session, &both));
    }
}

#[test]
fn runtime_resolution_agrees_with_syntax_quote() {
    let root = project();
    for (mut session, mut macros) in sessions(root.path()) {
        agreement(&mut session, &mut macros);
        session
            .eval_with_macros(
                "(ns demo.c (:refer-clojure :exclude [map]) (:require [demo.a :as a :refer [x y] :rename {y yy}]) (:require-macros [demo.m :as m :refer [mm]]))",
                &mut macros,
            )
            .unwrap();
        session.eval_with_macros("(def own 1)", &mut macros).unwrap();
        session.eval_with_macros("(def map 2)", &mut macros).unwrap();
        agreement(&mut session, &mut macros);
        // Redefinition and a reloaded ns form change the answers.
        session.eval_with_macros("(def later 3)", &mut macros).unwrap();
        session
            .eval_with_macros(
                "(ns demo.c (:refer-clojure :exclude [map first]) (:require [demo.a :as a :refer [z]]))",
                &mut macros,
            )
            .unwrap();
        agreement(&mut session, &mut macros);
        session.eval_with_macros("(def first 4)", &mut macros).unwrap();
        agreement(&mut session, &mut macros);
        session.eval_with_macros("(ns demo.f)", &mut macros).unwrap();
        agreement(&mut session, &mut macros);
    }
}

#[test]
fn the_repl_starts_in_user_and_follows_ns_and_in_ns() {
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
        .write_all(b"(ns-name *ns*)\n(ns other)\n(ns-name *ns*)\n(str *ns*)\n((fn [] (ns-name *ns*)))\n:in-ns third\n(ns-name *ns*)\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let out = String::from_utf8(output.stdout).unwrap();
    let errors = String::from_utf8(output.stderr).unwrap();
    let lines = out.lines().filter(|line| *line != "...").collect::<Vec<_>>();
    assert_eq!(lines, ["user", "nil", "other", "\"other\"", "other", "nil", "third"], "{out}\n{errors}");
}

#[test]
fn script_files_start_in_user() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("app.sus");
    let run = |source: &str| {
        std::fs::write(&file, source).unwrap();
        Command::new(env!("CARGO_BIN_EXE_suss")).arg(&file).output().unwrap()
    };
    let output = run(concat!(
        "(when-not (= 'user (ns-name *ns*)) (throw (ex-info \"before ns\" {})))\n",
        "(ns app)\n",
        "(when-not (= 'app (ns-name *ns*)) (throw (ex-info \"after ns\" {})))\n",
    ));
    assert!(output.status.success(), "{output:?}");
    // The check above can fail.
    let output = run("(ns app)\n(when-not (= 'user (ns-name *ns*)) (throw (ex-info \"app\" {})))\n");
    assert!(!output.status.success(), "{output:?}");
}

#[test]
fn component_exports_see_the_root_namespace() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src/app")).unwrap();
    std::fs::write(
        root.path().join("src/app/names.sus"),
        "(ns app.names)\n(def loaded (str *ns*))\n(defn at-load [] loaded)\n(defn at-call [] (str *ns*))\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("names.wit"),
        "package test:names;\nworld names {\n export at-load: func() -> string;\n export at-call: func() -> string;\n}\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["compile", "-n", "app.names", "--src", "src", "-w", "names.wit", "--wit-world", "names", "-o", "names.wasm"])
        .arg("--compile-fuel")
        .arg(compilation_fuel::COMPILATION_FUEL.to_string())
        .args(["--export", "at-load=app.names/at-load", "--export", "at-call=app.names/at-call"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for (export, expected) in [("at-load", "app.names"), ("at-call", "suss.core")] {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "names.wasm", "--invoke", export])
            .output()
            .unwrap();
        assert!(output.status.success(), "{export}: {}", String::from_utf8_lossy(&output.stderr));
        let out = String::from_utf8(output.stdout).unwrap();
        assert_eq!(out.trim().trim_matches('"'), expected, "{export}");
    }
}

#[test]
fn a_namespace_whose_dependency_failed_to_load_recovers() {
    let root = project();
    let broken = root.path().join("demo/broken.sus");
    for (mut session, mut macros) in sessions(root.path()) {
        std::fs::write(&broken, "(ns demo.broken)\n(throw (ex-info \"boom\" {}))\n").unwrap();
        // The ns form compiles, but its fragment never runs: loading its
        // dependency throws first.
        assert!(session.eval_with_macros("(ns demo.g (:require [demo.broken]))", &mut macros).is_err());
        std::fs::write(&broken, "(ns demo.broken)\n(def ok 1)\n").unwrap();
        session.eval_with_macros("(ns demo.g (:require [demo.broken]))", &mut macros).unwrap();
        truthy(&mut session, &mut macros, "(= 'demo.g (ns-name *ns*))");
        truthy(&mut session, &mut macros, "(= 1 demo.broken/ok)");
    }
}

#[test]
fn core_names_added_before_the_first_resolution_resolve_to_core() {
    let root = project();
    for (mut session, mut macros) in sessions(root.path()) {
        // Core publishes its own large name lists lazily; definitions added to
        // suss.core before the first resolution must survive that.
        session.eval_with_macros("(ns suss.core)", &mut macros).unwrap();
        session.eval_with_macros("(def later-core-name 1)", &mut macros).unwrap();
        session.eval_with_macros("(ns user)", &mut macros).unwrap();
        truthy(
            &mut session,
            &mut macros,
            "(= `later-core-name (suss.core/reader-resolve-symbol 'later-core-name) 'cljs.core/later-core-name)",
        );
    }
}
