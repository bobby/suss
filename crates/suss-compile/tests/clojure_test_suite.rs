//! Execute the vendored jank-lang/clojure-test-suite through the compiled host.
//!
//! Every suite namespace is loaded through the production Session and compiled
//! macros, with the conformance harness's clojure.test replacement. Each
//! assertion's raw values are decoded independently on the host; the pinned
//! ClojureScript oracle reference and the reviewed known-failure baseline are
//! compared by scripts/clojure_test_suite.py. Guest equality or printing never
//! decides a result. A namespace that cannot load is a recorded failure for all
//! of its assertions, never a skip.
#![cfg(not(target_family = "wasm"))]
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use suss_compile::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError, SessionOptions, SessionValue},
};

const SUITE_COMMIT: &str = "95d4a9112cfc5fe6629be3d14bb99080ea00af2f";
const HARNESS: &str = "suss.harness.test";
// Bounded so a runaway assertion becomes a recorded trap, not a hung suite.
const OPERATION_FUEL: u64 = 200_000_000;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
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

fn stage(error: &SessionError) -> &'static str {
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
    String::from_utf16(units).unwrap()
}

fn keyword(value: &Observation) -> String {
    match value {
        Observation::Keyword(None, name) => text(name),
        other => panic!("expected an unqualified harness keyword, observed {other:?}"),
    }
}

fn symbol(value: &Observation) -> String {
    match value {
        Observation::Symbol(None, name) => text(name),
        other => panic!("expected an unqualified symbol, observed {other:?}"),
    }
}

fn items(value: Observation) -> Vec<Observation> {
    match value {
        Observation::Vector(items) | Observation::List(items) => items,
        other => panic!("expected a harness vector, observed {other:?}"),
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
        let decoder = Decoder::capture(&mut session, 100_000_000).unwrap();
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

    /// Evaluate a harness accessor that must succeed and decode.
    fn query(&mut self, source: &str) -> Observation {
        let value = self.eval(source).unwrap();
        self.decode(&value).unwrap()
    }

    fn opaque_or_tagged(&mut self, source: &str) -> Value {
        match self.eval(source).and_then(|value| self.decode(&value)) {
            Ok(value) => tagged(&value),
            Err(error) => json!({"tag": "opaque", "reason": self.diagnostic(&error)}),
        }
    }

    fn skips(&mut self, values: Vec<Observation>, namespace: &str, test: Option<&str>) -> Vec<Value> {
        values
            .iter()
            .map(|value| {
                json!({"symbol": symbol(value), "phase": if test.is_some() { "run" } else { "load" },
                       "namespace": namespace, "test": test})
            })
            .collect()
    }

    /// Decode the observations of the test that just ran. Undecodable payload
    /// elements become opaque individually; they never decide a pass.
    fn assertions(&mut self, test: &str) -> Vec<Value> {
        let count = match self.query(&format!("(count (deref {HARNESS}/observations))")) {
            Observation::Number(bits) => f64::from_bits(bits) as usize,
            other => panic!("observation count {other:?}"),
        };
        // Decode all records in one query when possible; otherwise decode each
        // record, and each undecodable payload element individually.
        let mut all = self
            .eval(&format!("(deref {HARNESS}/observations)"))
            .and_then(|value| self.decode(&value))
            .ok()
            .map(items);
        (0..count)
            .map(|index| {
                let record = format!("({HARNESS}/observation {index})");
                let decoded = match &mut all {
                    Some(records) => Some(items(std::mem::replace(&mut records[index], Observation::Nil))),
                    None => self
                        .eval(&record)
                        .and_then(|value| self.decode(&value))
                        .ok()
                        .map(items),
                };
                let (kind, verdict, payload) = match decoded {
                    Some(mut fields) => {
                        let payload = items(fields.pop().unwrap());
                        (keyword(&fields[0]), keyword(&fields[1]),
                         payload.iter().map(tagged).collect::<Vec<_>>())
                    }
                    None => {
                        let kind = keyword(&self.query(&format!("(nth {record} 0)")));
                        let verdict = keyword(&self.query(&format!("(nth {record} 1)")));
                        let width = match self.query(&format!("(count (nth {record} 2))")) {
                            Observation::Number(bits) => f64::from_bits(bits) as usize,
                            other => panic!("payload width {other:?}"),
                        };
                        let payload = (0..width)
                            .map(|position| {
                                self.opaque_or_tagged(&format!("({HARNESS}/operand {index} {position})"))
                            })
                            .collect::<Vec<_>>();
                        (kind, verdict, payload)
                    }
                };
                let mut entry = json!({"test": test, "ordinal": index + 1,
                                       "kind": kind, "verdict": verdict});
                match kind.as_str() {
                    "predicate" => {
                        entry["result"] = payload[0].clone();
                        entry["operands"] = json!(payload[1..]);
                    }
                    "value" => entry["value"] = payload[0].clone(),
                    "thrown" => {
                        entry["threw"] = match &payload[0] {
                            Value::Object(value) if value["tag"] == "bool" => value["value"].clone(),
                            other => panic!("thrown observation {other}"),
                        }
                    }
                    "error" => entry["thrown"] = payload[0].clone(),
                    other => panic!("unknown harness assertion kind {other}"),
                }
                entry
            })
            .collect()
    }

    fn run(&mut self, reference: &Value) -> Value {
        let mut loaded = Vec::new();
        let mut test_failures = Vec::new();
        let mut skips = Vec::new();
        let mut assertions = Vec::new();
        for namespace in namespaces(reference) {
            let before = self.test_count();
            if let Err(error) = self
                .session
                .load_namespace_with_macros(&namespace, &mut self.macros)
            {
                loaded.push(json!({"namespace": namespace, "status": "failure",
                                   "stage": stage(&error), "diagnostic": self.diagnostic(&error)}));
                continue;
            }
            loaded.push(json!({"namespace": namespace, "status": "loaded"}));
            let load_skips = items(self.query(&format!("({HARNESS}/drain-skips!)")));
            skips.extend(self.skips(load_skips, &namespace, None));
            for index in before..self.test_count() {
                let name = symbol(&self.query(&format!("({HARNESS}/test-name {index})")));
                let test = format!("{namespace}/{name}");
                match self.eval(&format!("({HARNESS}/run-test! {index})")) {
                    Err(error) => test_failures.push(json!({"test": test, "stage": stage(&error),
                        "diagnostic": self.diagnostic(&error)})),
                    Ok(_) => {
                        let threw = self.query(&format!("(nth ({HARNESS}/run-outcome) 0)"));
                        if threw == Observation::Bool(true) {
                            let thrown = self.opaque_or_tagged(&format!("(nth ({HARNESS}/run-outcome) 1)"));
                            test_failures.push(json!({"test": test, "stage": "language-exception",
                                "diagnostic": thrown.to_string()}));
                        }
                    }
                }
                let run_skips = items(self.query(&format!("(deref {HARNESS}/skips)")));
                skips.extend(self.skips(run_skips, &namespace, Some(&test)));
                assertions.extend(self.assertions(&test));
            }
        }
        json!({"schema": 1, "suite": SUITE_COMMIT, "namespaces": loaded,
               "test-failures": test_failures, "skips": skips, "assertions": assertions})
    }

    fn test_count(&mut self) -> usize {
        match self.query(&format!("({HARNESS}/test-count)")) {
            Observation::Number(bits) => f64::from_bits(bits) as usize,
            other => panic!("test count {other:?}"),
        }
    }
}

/// Run one suite and require it to equal its reviewed known-failure baseline.
/// Set SUSS_CLOJURE_TEST_SUITE_WRITE=1 to record the baseline for review instead.
fn check(suite: &str, sources: &Path) {
    let repository = root();
    let reference_path = repository.join(match suite {
        "suite" => "tests/oracle/clojure-test-suite-observations.json",
        _ => "tests/oracle/clojure-test-suite-fixture-observations.json",
    });
    let reference: Value =
        serde_json::from_str(&std::fs::read_to_string(reference_path).unwrap()).unwrap();
    let source_root = tempfile::tempdir().unwrap();
    copy_tree(sources, source_root.path());
    copy_tree(
        &repository.join("tests/clojure-test-suite/suss"),
        source_root.path(),
    );
    let observations = Harness::new(source_root.path()).run(&reference);
    let output = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(output.path(), serde_json::to_vec_pretty(&observations).unwrap()).unwrap();
    let mut command = Command::new("python3");
    command
        .arg(repository.join("scripts/clojure_test_suite.py"))
        .args(["compare", "--suite", suite, "--observations"])
        .arg(output.path());
    if std::env::var_os("SUSS_CLOJURE_TEST_SUITE_WRITE").is_some() {
        command.arg("--write");
    }
    let result = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    eprintln!("{stdout}{stderr}");
    assert!(result.status.success(), "{suite} differs from its reviewed baseline");
}

#[test]
fn clojure_test_suite_fixture_matches_reviewed_baseline() {
    check(
        "fixture",
        &root().join("tests/clojure-test-suite/fixture"),
    );
}

#[test]
fn clojure_test_suite_matches_reviewed_baseline() {
    check(
        "suite",
        &root().join("vendor/clojure-test-suite/upstream/test"),
    );
}
