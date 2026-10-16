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
use std::sync::{Arc, Mutex};
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
    root.join(format!(
        "{}.cljc",
        namespace.replace('.', "/").replace('-', "_")
    ))
}

/// Print the forms a namespace declaration may contain; None for anything else.
fn print(form: &Form) -> Option<String> {
    if !form.metadata.is_empty() {
        return None;
    }
    let join = |items: &[Form]| -> Option<String> {
        Some(
            items
                .iter()
                .map(print)
                .collect::<Option<Vec<_>>>()?
                .join(" "),
        )
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
                let Kind::Keyword(key) = &key.kind else {
                    return None;
                };
                match key.name.as_str() {
                    "as" => runtime_spec.extend([":as".into(), print(value)?]),
                    "refer" | "refer-macros" => {
                        let Kind::Vector(names) = &value.kind else {
                            return None;
                        };
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
        SessionError::ResetPending => "reset-pending",
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
    decoder: Arc<Mutex<Decoder>>,
    snapshots: Arc<Mutex<Vec<Vec<Value>>>>,
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
        let mut decoder = Decoder::capture(&mut session, DECODE_BUDGET).unwrap();
        decoder.capture_sequences(&mut session).unwrap();
        let decoder = Arc::new(Mutex::new(decoder));
        let snapshots = Arc::new(Mutex::new(Vec::<Vec<Value>>::new()));
        let observer_decoder = decoder.clone();
        let observer_snapshots = snapshots.clone();
        session
            .install_observation_hook(move |mut store, payload| {
                let mut decoder = observer_decoder.lock().unwrap();
                decoder.refill(DECODE_BUDGET);
                let captured = decoder.snapshot_payload(&mut store, &payload)?;
                let captured = captured
                    .into_iter()
                    .map(|value| match value {
                        Ok(value) => tagged(&value),
                        Err(reason) => json!({"tag":"opaque", "reason":reason}),
                    })
                    .collect::<Vec<_>>();
                let mut snapshots = observer_snapshots.lock().unwrap();
                if snapshots.len() >= 1_000_000 {
                    return Err(wasmtime::Error::msg("Assertion snapshot count limit"));
                }
                let token = u32::try_from(snapshots.len())
                    .map_err(|_| wasmtime::Error::msg("Snapshot token overflow"))?;
                snapshots.push(captured);
                Ok(token)
            })
            .unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(OPERATION_FUEL);
        session
            .load_namespace_with_macros(HARNESS, &mut macros)
            .unwrap();
        Self {
            session,
            macros,
            decoder,
            snapshots,
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
        self.decoder
            .lock()
            .unwrap()
            .decode_session(&mut self.session, value)
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

    /// Assertion metadata is immutable; payloads were independently captured
    /// on the host before later assertions or fixtures could mutate aliases.
    fn assertions(&mut self, position: usize, test: &str) -> Vec<Value> {
        self.decoder.lock().unwrap().refill(DECODE_BUDGET);
        let field = format!("({HARNESS}/result-field {position} 2)");
        let records = items(self.query(&field)).expect("assertion metadata vector");
        records.into_iter().enumerate().map(|(index, record)| {
            let fields = items(record).expect("assertion metadata record");
            assert_eq!(fields.len(),4,"assertion metadata width");
            let kind = name(&fields[0]).expect("assertion kind");
            let verdict = name(&fields[1]).expect("assertion verdict");
            let snapshot = count(&fields[2]).expect("assertion snapshot index");
            let head = name(&fields[3]);
            let payload = self.snapshots.lock().unwrap().get(snapshot).cloned().expect("assertion snapshot exists");
            let mut entry = json!({"test":test,"ordinal":index+1,"kind":kind,"verdict":verdict,"head":head});
            match kind.as_str() {
                "predicate" => { entry["result"]=payload[0].clone(); entry["operands"]=json!(payload[1..]); }
                "value" => entry["value"]=payload[0].clone(),
                "thrown" => entry["threw"]=payload[0]["value"].clone(),
                "error" => entry["thrown"]=payload[0].clone(),
                other => panic!("unknown assertion kind {other}"),
            }
            entry
        }).collect()
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
            if let Some(routed) = std::fs::read_to_string(&path)
                .ok()
                .as_deref()
                .and_then(route_namespace)
            {
                std::fs::write(&path, routed).unwrap();
            }
            let before = self.test_count();
            let result = self
                .session
                .load_namespace_with_macros(&namespace, &mut self.macros);
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
                    let name =
                        name(&self.query(&format!("({HARNESS}/test-name {index})"))).unwrap();
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
                .filter_map(|test| {
                    tests
                        .iter()
                        .find(|(name, _)| name == test.as_str().unwrap())
                        .cloned()
                })
                .collect();
            for test in tests {
                if !group.contains(test) {
                    group.push(test.clone());
                }
            }
            if group.is_empty() {
                continue;
            }
            let indices = group
                .iter()
                .map(|(_, index)| index.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            if let Err(error) = self.eval(&format!(
                "({HARNESS}/run-group! [{indices}] {namespace_index})"
            )) {
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
                if self.query(&format!("({HARNESS}/result-field {position} 0)"))
                    == Observation::Bool(true)
                {
                    let thrown =
                        self.opaque_or_tagged(&format!("({HARNESS}/result-field {position} 1)"));
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
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let observations = Harness::new(&source_root).run(&reference, &source_root);
    let output = workspace.path().join("observations.json");
    std::fs::write(&output, serde_json::to_vec_pretty(&observations).unwrap()).unwrap();
    // Retain actual raw evidence before the temporary workspace is dropped,
    // including when comparison fails. This does not change expected results.
    if let Some(directory) = std::env::var_os("SUSS_CLOJURE_TEST_SUITE_CAPTURE") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(
            &output,
            directory.join(format!("{suite}-observations.json")),
        )
        .unwrap();
    }
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
    assert!(
        result.status.success(),
        "{suite} differs from its reviewed baseline"
    );
}

#[test]
fn clojure_test_suite_routes_suite_namespace_requires_to_the_harness() {
    let source = "(ns clojure.core-test.conj\n  (:require [clojure.test :as t :refer [deftest is testing]]\n            [clojure.core-test.portability #?(:cljs :refer-macros :default :refer) [when-var-exists] :as p]\n            [clojure.string :as str]))\n(deftest x)\n";
    assert_eq!(
        route_namespace(source).unwrap(),
        "(ns clojure.core-test.conj (:require [suss.harness.test :as t] [clojure.core-test.portability :as p] [clojure.string :as str]) (:require-macros [suss.harness.portability-macros :refer [when-var-exists]] [suss.harness.test-macros :refer [deftest is testing]]))\n(deftest x)\n"
    );
    let functions =
        "(ns a (:require [clojure.core-test.portability :refer-macros [when-var-exists sleep]]))";
    assert_eq!(
        route_namespace(functions).unwrap(),
        "(ns a (:require [clojure.core-test.portability :refer [sleep]]) (:require-macros [suss.harness.portability-macros :refer [when-var-exists]]))"
    );
    // Splicing conditionals now resolve through the production reader.
    assert_eq!(
        route_namespace("(ns a #?@(:cljs [(:require [b])]))"),
        Some("(ns a (:require [b]))".into())
    );
    // Genuinely unreadable sources stay failures for the namespace loader.
    assert_eq!(route_namespace("(ns a #?@(:cljs 1))"), None);
}

#[test]
fn clojure_test_suite_fixture_matches_reviewed_baseline() {
    check("fixture");
}

#[test]
fn observation_capture_and_host_decoding_do_not_force_pending_guest_values() {
    let workspace = tempfile::tempdir().unwrap();
    let source_root = workspace.path().join("src");
    let generated = Command::new("python3")
        .arg(root().join("scripts/clojure_test_suite.py"))
        .args(["suss-sources", "--suite", "fixture", "--output"])
        .arg(&source_root)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let mut harness = Harness::new(&source_root);
    harness
        .eval(
            r#"
      (ns user)
      (def capture-effects 0)
      (def capture-pending
        (new suss.core/LazySeq nil
          (fn [] (set! capture-effects (+ capture-effects 1))
                 (throw (suss.bootstrap/error "observation forced pending code")))
          nil nil))
      (suss.harness.test/register! 'pending
        (fn [] (suss.harness.test/record! :predicate :pass [true capture-pending] 'lazy-seq?)))
      (suss.harness.test/register! 'later
        (fn [] (if (= capture-effects 0) nil
                   (throw (suss.bootstrap/error "later assertion saw observation effects")))))
      (suss.harness.test/run-group! [0 1] 0)
    "#,
        )
        .unwrap();
    assert_eq!(
        harness.query("(suss.harness.test/group-field 0)"),
        Observation::Bool(false)
    );
    for position in 0..2 {
        assert_eq!(
            harness.query(&format!("(suss.harness.test/result-field {position} 0)")),
            Observation::Bool(false)
        );
    }
    let captured = harness.assertions(0, "pending");
    assert_eq!(captured[0]["operands"][0]["tag"], "opaque");
    assert_eq!(
        harness.query("capture-effects"),
        Observation::Number(0.0_f64.to_bits())
    );
}

#[test]
fn clojure_test_suite_matches_reviewed_baseline() {
    check("suite");
}

#[test]
fn observation_snapshots_preserve_alias_values_before_later_mutation() {
    let workspace = tempfile::tempdir().unwrap();
    let source_root = workspace.path().join("src");
    let generated = Command::new("python3")
        .arg(root().join("scripts/clojure_test_suite.py"))
        .args(["suss-sources", "--suite", "fixture", "--output"])
        .arg(&source_root)
        .output()
        .unwrap();
    assert!(generated.status.success());
    let mut harness = Harness::new(&source_root);
    harness
        .eval(
            r#"
      (ns user)
      (def alias-source (array 1 2 3)) (def alias-vector (vec alias-source))
      (suss.harness.test/register! 'alias
        (fn []
          (suss.harness.test/record! :predicate :pass [true [1 2 3] alias-vector] '=)
          (aset alias-source 0 -1)
          (suss.harness.test/record! :predicate :pass [true [-1 2 3] alias-vector] '=)))
      (suss.harness.test/run-group! [0] 0)
    "#,
        )
        .unwrap();
    let captured = harness.assertions(0, "alias");
    assert_eq!(captured.len(), 2);
    let first = &captured[0]["operands"][1]["items"];
    let second = &captured[1]["operands"][1]["items"];
    assert_eq!(first[0]["bits"], "3ff0000000000000");
    assert_eq!(second[0]["bits"], "bff0000000000000");
    assert_eq!(
        harness.query("(first alias-vector)"),
        Observation::Number((-1.0_f64).to_bits()),
        "Snapshotting must preserve source aliasing"
    );
}

#[test]
fn observation_tokens_do_not_drift_after_capture_then_fuel_trap() {
    let workspace = tempfile::tempdir().unwrap();
    let source_root = workspace.path().join("src");
    let generated = Command::new("python3")
        .arg(root().join("scripts/clojure_test_suite.py"))
        .args(["suss-sources", "--suite", "fixture", "--output"])
        .arg(&source_root)
        .output()
        .unwrap();
    assert!(generated.status.success());
    let mut harness = Harness::new(&source_root);
    let error = harness
        .eval("(suss.harness.observation/capture [true [99] [99]]) (loop [] (recur))")
        .unwrap_err();
    assert!(
        matches!(error, SessionError::Trap(_)),
        "Expected capture followed by actual fuel trap"
    );
    assert_eq!(harness.snapshots.lock().unwrap().len(), 1);
    harness
        .eval(
            r#"(ns user)
      (suss.harness.test/register! 'recovered
        (fn [] (suss.harness.test/record! :predicate :pass [true [7] [7]] '=)))
      (suss.harness.test/run-group! [0] 0)"#,
        )
        .unwrap();
    let captured = harness.assertions(0, "recovered");
    assert_eq!(
        captured[0]["operands"][1]["items"][0]["bits"], "401c000000000000",
        "An orphan capture must not replace the next assertion's payload"
    );
}

/// #260 supplemental cases execute unchanged pinned identifier name and namespace cases.
#[test]
fn pinned_reader_identifier_names_keep_values() {
    let repository = root();
    let workspace = tempfile::tempdir().unwrap();
    let source_root = workspace.path().join("src");
    let generated = Command::new("python3")
        .arg(repository.join("scripts/clojure_test_suite.py"))
        .args(["suss-sources", "--suite", "suite", "--output"])
        .arg(&source_root).output().unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    let names = ["clojure.core-test.name", "clojure.core-test.namespace"];
    for namespace in names {
        let original = repository.join("vendor/clojure-test-suite/upstream/test")
            .join(namespace.replace('.', "/").replace('-', "_") + ".cljc");
        assert_eq!(std::fs::read(original).unwrap(),
            std::fs::read(source_file(&source_root, namespace)).unwrap(),
            "the original identifier declaration must remain intact");
    }
    let tests = ["clojure.core-test.name/test-name",
                 "clojure.core-test.namespace/test-namespace"];
    let selected = json!({"namespaces": names, "tests": tests});
    let observations = Harness::new(&source_root).run(&selected, &source_root);
    let captured = workspace.path().join("identifier-observations.json");
    std::fs::write(&captured, serde_json::to_vec_pretty(&observations).unwrap()).unwrap();
    if let Some(directory) = std::env::var_os("SUSS_READER_IDENTIFIER_CAPTURE") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("identifier-observations.json"),
            serde_json::to_vec_pretty(&observations).unwrap()).unwrap();
    }
    // Use the same independent operand comparator as the suite. Never accept
    // guest equality, an executed prefix, or a reviewed failure as a pass here.
    let compared = Command::new("python3").arg("-c").arg(r#"
import json,sys
from pathlib import Path
root=Path(sys.argv[1]);sys.path.insert(0,str(root/'scripts'))
import clojure_test_suite as suite
reference=json.loads((root/'tests/oracle/clojure-test-suite-observations.json').read_text())
names=['clojure.core-test.name','clojure.core-test.namespace']
tests=['clojure.core-test.name/test-name','clojure.core-test.namespace/test-namespace']
reference.update(namespaces=names,tests=tests,
 assertions=[x for x in reference['assertions'] if x['test'] in tests],
 skips=[x for x in reference['skips'] if x['test'] in tests])
observed=json.loads(Path(sys.argv[2]).read_text())
expected=[(x['test'],x['ordinal']) for x in reference['assertions']]
actual=[(x['test'],x['ordinal']) for x in observed['assertions']]
counts,failures=suite.compare(reference,observed)
assert not observed['test-failures']
assert not failures['tests'] and not failures['namespaces']
print(json.dumps({'counts':counts,'failures':failures,'ordered_ids_match':actual==expected}))
assert expected and actual==expected
assert counts['pass']==len(expected) and counts['guest-judged']==0
assert counts['fail']==0 and counts['not-executed']==0 and counts['skipped']==0
"#).arg(&repository).arg(&captured).output().unwrap();
    assert!(compared.status.success(), "{}\n{}", String::from_utf8_lossy(&compared.stdout),
        String::from_utf8_lossy(&compared.stderr));
    println!("PINNED_IDENTIFIER_FIELDS {}", String::from_utf8_lossy(&compared.stdout));
}
