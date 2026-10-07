//! Execute the vendored jank-lang/clojure-test-suite through the compiled host.
//!
//! Every suite namespace is loaded through the production Session and compiled
//! macros, then each namespace's tests run as one group under its fixtures in
//! the oracle's test order. A reviewed harness adaptation routes the suite's
//! `clojure.test` and portability requires to the harness namespaces; nothing
//! else in the vendored sources changes. Each assertion's raw values are decoded
//! independently on the host, and scripts/clojure_test_suite.py compares them
//! with the pinned ClojureScript oracle reference and the reviewed known-failure
//! baseline. Guest equality or printing never decides a result. A namespace that
//! cannot load is a recorded failure for all of its assertions, never a skip.
#![cfg(not(target_family = "wasm"))]
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use suss_compile::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError, SessionOptions, SessionValue},
};
use suss_reader::forms::{Form, Kind, read_forms, resolve_conditionals};

const SUITE_COMMIT: &str = "95d4a9112cfc5fe6629be3d14bb99080ea00af2f";
const HARNESS: &str = "suss.harness.test";
const TEST_MACROS: &str = "suss.harness.test-macros";
const PORTABILITY: &str = "clojure.core-test.portability";
const PORTABILITY_MACROS: &str = "suss.harness.portability-macros";
// Names the suite refers that are macros upstream: cljs.test's assertion and
// definition macros (async included, though the harness lacks it) and the
// portability helper's when-var-exists. Other referred names are functions.
const TEST_MACRO_NAMES: &[&str] = &["deftest", "is", "are", "testing", "use-fixtures", "async"];
const PORTABILITY_MACRO_NAMES: &[&str] = &["when-var-exists"];
// Bounded so a runaway assertion becomes a recorded trap, not a hung suite.
const OPERATION_FUEL: u64 = 200_000_000;
const DECODE_BUDGET: usize = 10_000_000;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn source_file(root: &Path, namespace: &str) -> PathBuf {
    root.join(format!("{}.cljc", namespace.replace('.', "/").replace('-', "_")))
}

/// Print the forms a namespace declaration may contain; None for anything else.
fn print(form: &Form) -> Option<String> {
    if !form.metadata.is_empty() {
        return None;
    }
    let join = |items: &[Form]| -> Option<String> {
        Some(items.iter().map(print).collect::<Option<Vec<_>>>()?.join(" "))
    };
    Some(match &form.kind {
        Kind::Nil => "nil".into(),
        Kind::Bool(value) => value.to_string(),
        Kind::Symbol(symbol) => symbol.to_string(),
        Kind::Keyword(keyword) => match &keyword.namespace {
            Some(namespace) => format!(":{namespace}/{}", keyword.name),
            None => format!(":{}", keyword.name),
        },
        Kind::String(units) => serde_json::to_string(&String::from_utf16(units).ok()?).ok()?,
        Kind::List(items) => format!("({})", join(items)?),
        Kind::Vector(items) => format!("[{}]", join(items)?),
        Kind::Map(items) => format!("{{{}}}", join(items)?),
        _ => return None,
    })
}

fn symbol_name(form: &Form) -> Option<String> {
    match &form.kind {
        Kind::Symbol(symbol) => Some(symbol.to_string()),
        _ => None,
    }
}

/// Route the suite's `clojure.test` and portability requires to the harness.
///
/// cljs.test and the upstream portability namespace define macros beside
/// runtime functions and are referred with `:refer-macros` or `:refer`. A Suss
/// runtime namespace cannot yet define macros, so each such libspec becomes a
/// runtime require plus an explicit `:require-macros` of the harness macros.
/// Other libspecs and clauses are unchanged. Returns None when the source
/// cannot be read, which the namespace load then reports exactly.
fn route_namespace(source: &str) -> Option<String> {
    let forms = resolve_conditionals(read_forms(source).ok()?).ok()?;
    let declaration = forms.first()?;
    let Kind::List(items) = &declaration.kind else {
        return None;
    };
    if symbol_name(items.first()?).as_deref() != Some("ns") {
        return None;
    }
    let mut parts = vec!["ns".to_string(), print(items.get(1)?)?];
    let mut macros: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for clause in &items[2..] {
        let Kind::List(clause_items) = &clause.kind else {
            parts.push(print(clause)?);
            continue;
        };
        let is_require = matches!(clause_items.first().map(|f| &f.kind),
            Some(Kind::Keyword(k)) if k.namespace.is_none() && k.name == "require");
        if !is_require {
            parts.push(print(clause)?);
            continue;
        }
        let mut specs = vec![":require".to_string()];
        for spec in &clause_items[1..] {
            let Kind::Vector(libspec) = &spec.kind else {
                specs.push(print(spec)?);
                continue;
            };
            let library = libspec.first().and_then(symbol_name);
            let (runtime, macro_namespace, macro_names) = match library.as_deref() {
                Some("clojure.test" | "cljs.test") => (HARNESS, TEST_MACROS, TEST_MACRO_NAMES),
                Some(PORTABILITY) => (PORTABILITY, PORTABILITY_MACROS, PORTABILITY_MACRO_NAMES),
                _ => {
                    specs.push(print(spec)?);
                    continue;
                }
            };
            let mut runtime_spec = vec![runtime.to_string()];
            let mut runtime_refers = Vec::new();
            for option in libspec[1..].chunks(2) {
                let [key, value] = option else { return None };
                let Kind::Keyword(key) = &key.kind else { return None };
                match key.name.as_str() {
                    "as" => runtime_spec.extend([":as".into(), print(value)?]),
                    "refer" | "refer-macros" => {
                        let Kind::Vector(names) = &value.kind else { return None };
                        for name in names {
                            let printed = print(name)?;
                            if macro_names.contains(&printed.as_str()) {
                                macros.entry(macro_namespace).or_default().push(printed);
                            } else {
                                runtime_refers.push(printed);
                            }
                        }
                    }
                    "include-macros" => {}
                    _ => return None,
                }
            }
            if !runtime_refers.is_empty() {
                runtime_spec.push(format!(":refer [{}]", runtime_refers.join(" ")));
            }
            specs.push(format!("[{}]", runtime_spec.join(" ")));
        }
        parts.push(format!("({})", specs.join(" ")));
    }
    if !macros.is_empty() {
        let specs = macros
            .iter()
            .map(|(namespace, names)| format!("[{namespace} :refer [{}]]", names.join(" ")))
            .collect::<Vec<_>>();
        parts.push(format!("(:require-macros {})", specs.join(" ")));
    }
    let routed = format!("({})", parts.join(" "));
    Some(format!(
        "{}{routed}{}",
        &source[..declaration.span.start],
        &source[declaration.span.end..]
    ))
}

/// Namespaces in the same deterministic order as the oracle reference.
fn namespaces(reference: &Value) -> Vec<String> {
    reference["namespaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_owned())
        .collect()
}

fn session_stage(error: &SessionError) -> &'static str {
    match error {
        SessionError::Compile(error)
            if error
                .message
                .starts_with("Generated fragment failed validation:") =>
        {
            "validation"
        }
        SessionError::Compile(_) => "compile",
        SessionError::Module(_) => "dependency",
        SessionError::Language(_) => "language-exception",
        SessionError::Trap(_) => "trap",
        SessionError::Host(_) => "host",
        SessionError::ForeignValue => "ownership",
    }
}

/// Distinguish read failures from macro and namespace compilation failures:
/// a module diagnostic is a read failure when the Suss reader reproduces it.
fn load_stage(error: &SessionError) -> String {
    let SessionError::Module(diagnostic) = error else {
        return session_stage(error).into();
    };
    let reread = diagnostic
        .source_path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|source| read_forms(&source).and_then(resolve_conditionals));
    match reread {
        Some(Err(read)) if read.message == diagnostic.message => "read".into(),
        _ if diagnostic.message.starts_with("Compiled macro") => "macro".into(),
        _ => "namespace".into(),
    }
}

fn tagged(value: &Observation) -> Value {
    let units = |units: &[u16]| json!({"tag": "string", "units": units});
    match value {
        Observation::Nil => json!({"tag": "nil"}),
        Observation::Bool(value) => json!({"tag": "bool", "value": value}),
        Observation::Number(bits) => json!({"tag": "f64", "bits": format!("{bits:016x}")}),
        Observation::String(value) => units(value),
        Observation::Keyword(namespace, name) | Observation::Symbol(namespace, name) => {
            let tag = if matches!(value, Observation::Keyword(..)) {
                "keyword"
            } else {
                "symbol"
            };
            json!({"tag": tag, "namespace": namespace.as_ref().map(|value| units(value))
                .unwrap_or(json!({"tag": "nil"})), "name": units(name)})
        }
        Observation::Vector(items) | Observation::List(items) | Observation::Set(items) => {
            let tag = match value {
                Observation::Vector(_) => "vector",
                Observation::List(_) => "seq",
                _ => "set",
            };
            json!({"tag": tag, "items": items.iter().map(tagged).collect::<Vec<_>>()})
        }
        Observation::Map(entries) => json!({"tag": "map", "entries": entries.iter()
            .map(|(key, value)| vec![tagged(key), tagged(value)])
            .collect::<Vec<_>>()}),
        Observation::ExceptionInfo {
            message,
            data,
            cause,
        } => json!({
            "tag": "exception-info", "message": tagged(message),
            "data": tagged(data), "cause": tagged(cause),
        }),
    }
}

fn text(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

/// Harness keywords and symbols, as written (`p/thrown?` keeps its alias).
fn name(value: &Observation) -> Option<String> {
    match value {
        Observation::Keyword(namespace, name) | Observation::Symbol(namespace, name) => {
            Some(match namespace {
                Some(namespace) => format!("{}/{}", text(namespace), text(name)),
                None => text(name),
            })
        }
        _ => None,
    }
}

fn items(value: Observation) -> Option<Vec<Observation>> {
    match value {
        Observation::Vector(items) | Observation::List(items) => Some(items),
        _ => None,
    }
}

fn count(value: &Observation) -> Option<usize> {
    match value {
        Observation::Number(bits) => Some(f64::from_bits(*bits) as usize),
        _ => None,
    }
}

struct Harness {
    session: Session,
    macros: CompiledMacros,
    decoder: Decoder,
    root: String,
}

impl Harness {
    fn new(source_root: &Path) -> Self {
        let mut session = Session::new_repl_with_options(SessionOptions {
            source_paths: vec![source_root.to_owned()],
            ..SessionOptions::default()
        })
        .unwrap();
        session.set_operation_fuel(OPERATION_FUEL);
        let decoder = Decoder::capture(&mut session, DECODE_BUDGET).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(OPERATION_FUEL);
        session
            .load_namespace_with_macros(HARNESS, &mut macros)
            .unwrap();
        Self {
            session,
            macros,
            decoder,
            root: source_root.canonicalize().unwrap().display().to_string(),
        }
    }

    /// Diagnostics name the temporary source root; keep the baseline stable.
    fn diagnostic(&self, error: &SessionError) -> String {
        error
            .to_string()
            .replace(&self.root, "<root>")
            .replace(&std::env::temp_dir().display().to_string(), "<tmp>")
    }

    fn eval(&mut self, source: &str) -> Result<SessionValue, SessionError> {
        self.session.eval_with_macros(source, &mut self.macros)
    }

    fn decode(&mut self, value: &SessionValue) -> Result<Observation, SessionError> {
        self.session.collect()?;
        self.decoder.decode_session(&mut self.session, value)
    }

    /// A harness accessor whose result must decode; harness defects are fatal.
    fn query(&mut self, source: &str) -> Observation {
        let value = self
            .eval(source)
            .unwrap_or_else(|error| panic!("harness query {source}: {error}"));
        self.decode(&value)
            .unwrap_or_else(|error| panic!("harness query {source}: {error}"))
    }

    fn opaque_or_tagged(&mut self, source: &str) -> Value {
        match self.eval(source).and_then(|value| self.decode(&value)) {
            Ok(value) => tagged(&value),
            Err(error) => json!({"tag": "opaque", "reason": self.diagnostic(&error)}),
        }
    }

    fn test_count(&mut self) -> usize {
        count(&self.query(&format!("({HARNESS}/test-count)"))).expect("test count")
    }

    fn skips(&mut self, values: Observation, namespace: &str, test: Option<&str>) -> Vec<Value> {
        items(values)
            .expect("skip vector")
            .iter()
            .map(|value| {
                json!({"symbol": name(value).expect("skip symbol"),
                       "phase": if test.is_some() { "run" } else { "load" },
                       "namespace": namespace, "test": test})
            })
            .collect()
    }

    /// Decode the observations of group result `position`. Records are decoded
    /// together when possible, otherwise one at a time, and an undecodable
    /// payload element becomes opaque on its own; it never decides a pass.
    fn assertions(&mut self, position: usize, test: &str) -> Vec<Value> {
        self.decoder.refill(DECODE_BUDGET);
        let field = format!("({HARNESS}/result-field {position} 2)");
        let total = count(&self.query(&format!("(count {field})"))).expect("observation count");
        let mut all = self
            .eval(&field)
            .and_then(|value| self.decode(&value))
            .ok()
            .and_then(items);
        (0..total)
            .map(|index| {
                let record = format!("({HARNESS}/observation {position} {index})");
                let decoded = match &mut all {
                    Some(records) => items(std::mem::replace(&mut records[index], Observation::Nil)),
                    None => self.eval(&record).and_then(|value| self.decode(&value)).ok().and_then(items),
                };
                let (kind, verdict, payload, head) = match decoded {
                    Some(mut fields) => {
                        let head = name(&fields.pop().unwrap());
                        let payload = items(fields.pop().unwrap()).expect("payload vector");
                        (name(&fields[0]).unwrap(), name(&fields[1]).unwrap(),
                         payload.iter().map(tagged).collect::<Vec<_>>(), head)
                    }
                    None => {
                        let kind = name(&self.query(&format!("(nth {record} 0)"))).unwrap();
                        let verdict = name(&self.query(&format!("(nth {record} 1)"))).unwrap();
                        let head = name(&self.query(&format!("(nth {record} 3)")));
                        let width = count(&self.query(&format!("(count (nth {record} 2))"))).unwrap();
                        let payload = (0..width)
                            .map(|element| self.opaque_or_tagged(&format!(
                                "({HARNESS}/operand {position} {index} {element})")))
                            .collect::<Vec<_>>();
                        (kind, verdict, payload, head)
                    }
                };
                let mut entry = json!({"test": test, "ordinal": index + 1, "kind": kind,
                                       "verdict": verdict, "head": head});
                match kind.as_str() {
                    "predicate" => {
                        entry["result"] = payload[0].clone();
                        entry["operands"] = json!(payload[1..]);
                    }
                    "value" => entry["value"] = payload[0].clone(),
                    "thrown" => entry["threw"] = payload[0]["value"].clone(),
                    "error" => entry["thrown"] = payload[0].clone(),
                    other => panic!("unknown harness assertion kind {other}"),
                }
                entry
            })
            .collect()
    }

    fn run(&mut self, reference: &Value, source_root: &Path) -> Value {
        let mut loaded = Vec::new();
        let mut test_failures = Vec::new();
        let mut skips = Vec::new();
        let mut assertions = Vec::new();
        // Suss test indices by namespace and test name, in registration order.
        let mut registered: BTreeMap<String, (usize, Vec<(String, usize)>)> = BTreeMap::new();
        for (namespace_index, namespace) in namespaces(reference).into_iter().enumerate() {
            let path = source_file(source_root, &namespace);
            if let Some(routed) = std::fs::read_to_string(&path).ok().as_deref().and_then(route_namespace) {
                std::fs::write(&path, routed).unwrap();
            }
            let before = self.test_count();
            let result = self.session.load_namespace_with_macros(&namespace, &mut self.macros);
            let load_skips = self.query(&format!("({HARNESS}/drain-skips!)"));
            skips.extend(self.skips(load_skips, &namespace, None));
            if let Err(error) = result {
                loaded.push(json!({"namespace": namespace, "status": "failure",
                                   "stage": load_stage(&error), "diagnostic": self.diagnostic(&error)}));
                continue;
            }
            loaded.push(json!({"namespace": namespace, "status": "loaded"}));
            self.query(&format!("({HARNESS}/attach-fixtures! {namespace_index})"));
            let tests = (before..self.test_count())
                .map(|index| {
                    let name = name(&self.query(&format!("({HARNESS}/test-name {index})"))).unwrap();
                    (format!("{namespace}/{name}"), index)
                })
                .collect();
            registered.insert(namespace, (namespace_index, tests));
        }
        // Run each loaded namespace's tests as one group: oracle order first,
        // then tests the oracle does not have, in registration order.
        let order = reference["tests"].as_array().unwrap();
        let mut namespaces_in_order = Vec::new();
        for test in order {
            let namespace = test.as_str().unwrap().split('/').next().unwrap().to_owned();
            if !namespaces_in_order.contains(&namespace) {
                namespaces_in_order.push(namespace);
            }
        }
        for namespace in registered.keys() {
            if !namespaces_in_order.contains(namespace) {
                namespaces_in_order.push(namespace.clone());
            }
        }
        for namespace in namespaces_in_order {
            let Some((namespace_index, tests)) = registered.get(&namespace) else {
                continue;
            };
            let mut group: Vec<(String, usize)> = order
                .iter()
                .filter_map(|test| tests.iter().find(|(name, _)| name == test.as_str().unwrap()).cloned())
                .collect();
            for test in tests {
                if !group.contains(test) {
                    group.push(test.clone());
                }
            }
            if group.is_empty() {
                continue;
            }
            let indices = group.iter().map(|(_, index)| index.to_string()).collect::<Vec<_>>().join(" ");
            if let Err(error) = self.eval(&format!("({HARNESS}/run-group! [{indices}] {namespace_index})")) {
                for (test, _) in &group {
                    test_failures.push(json!({"test": test, "stage": session_stage(&error),
                                              "diagnostic": self.diagnostic(&error)}));
                }
                continue;
            }
            let completed = count(&self.query(&format!("({HARNESS}/result-count)"))).unwrap();
            if self.query(&format!("({HARNESS}/group-field 0)")) == Observation::Bool(true) {
                let thrown = self.opaque_or_tagged(&format!("({HARNESS}/group-field 1)"));
                for (test, _) in &group[completed..] {
                    test_failures.push(json!({"test": test, "stage": "fixture-exception",
                                              "diagnostic": thrown.to_string()}));
                }
            }
            for (position, (test, _)) in group.iter().enumerate().take(completed) {
                if self.query(&format!("({HARNESS}/result-field {position} 0)")) == Observation::Bool(true) {
                    let thrown = self.opaque_or_tagged(&format!("({HARNESS}/result-field {position} 1)"));
                    test_failures.push(json!({"test": test, "stage": "language-exception",
                                              "diagnostic": thrown.to_string()}));
                }
                let run_skips = self.query(&format!("({HARNESS}/result-field {position} 3)"));
                skips.extend(self.skips(run_skips, &namespace, Some(test)));
                assertions.extend(self.assertions(position, test));
            }
        }
        json!({"schema": 1, "suite": SUITE_COMMIT, "namespaces": loaded,
               "test-failures": test_failures, "skips": skips, "assertions": assertions})
    }
}

/// Run one suite and require it to equal its reviewed known-failure baseline.
/// Set SUSS_CLOJURE_TEST_SUITE_WRITE=1 to record the baseline for review instead.
fn check(suite: &str) {
    let repository = root();
    let script = repository.join("scripts/clojure_test_suite.py");
    let reference_path = repository.join(match suite {
        "suite" => "tests/oracle/clojure-test-suite-observations.json",
        _ => "tests/oracle/clojure-test-suite-fixture-observations.json",
    });
    let reference: Value =
        serde_json::from_str(&std::fs::read_to_string(reference_path).unwrap()).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let source_root = workspace.path().join("src");
    let generated = Command::new("python3")
        .arg(&script)
        .args(["suss-sources", "--suite", suite, "--output"])
        .arg(&source_root)
        .output()
        .unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    let observations = Harness::new(&source_root).run(&reference, &source_root);
    let output = workspace.path().join("observations.json");
    std::fs::write(&output, serde_json::to_vec_pretty(&observations).unwrap()).unwrap();
    let mut command = Command::new("python3");
    command
        .arg(&script)
        .args(["compare", "--suite", suite, "--observations"])
        .arg(&output);
    if std::env::var_os("SUSS_CLOJURE_TEST_SUITE_WRITE").is_some() {
        command.arg("--write");
    }
    let result = command.output().unwrap();
    eprintln!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.status.success(), "{suite} differs from its reviewed baseline");
}

#[test]
fn clojure_test_suite_routes_suite_namespace_requires_to_the_harness() {
    let source = "(ns clojure.core-test.conj\n  (:require [clojure.test :as t :refer [deftest is testing]]\n            [clojure.core-test.portability #?(:cljs :refer-macros :default :refer) [when-var-exists] :as p]\n            [clojure.string :as str]))\n(deftest x)\n";
    assert_eq!(
        route_namespace(source).unwrap(),
        "(ns clojure.core-test.conj (:require [suss.harness.test :as t] [clojure.core-test.portability :as p] [clojure.string :as str]) (:require-macros [suss.harness.portability-macros :refer [when-var-exists]] [suss.harness.test-macros :refer [deftest is testing]]))\n(deftest x)\n"
    );
    let functions = "(ns a (:require [clojure.core-test.portability :refer-macros [when-var-exists sleep]]))";
    assert_eq!(
        route_namespace(functions).unwrap(),
        "(ns a (:require [clojure.core-test.portability :refer [sleep]]) (:require-macros [suss.harness.portability-macros :refer [when-var-exists]]))"
    );
    // Unreadable sources are left for the namespace load to report exactly.
    assert_eq!(route_namespace("(ns a #?@(:cljs [(:require [b])]))"), None);
}

#[test]
fn clojure_test_suite_fixture_matches_reviewed_baseline() {
    check("fixture");
}

#[test]
fn clojure_test_suite_matches_reviewed_baseline() {
    check("suite");
}
