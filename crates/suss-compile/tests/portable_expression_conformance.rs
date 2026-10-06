//! Execute every unchanged legacy corpus case through compiled ABI2 sessions.
//! This is a replacement acceptance gate; failures are never baseline successes.
#![cfg(not(target_family = "wasm"))]
#[path = "support/portable_compare.rs"]
mod portable_compare;
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::Decoder;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
};
use suss_compile::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError},
};
use suss_core::Edn;
use suss_reader::{ParserState, parse_all};

#[derive(Debug)]
struct Case {
    id: String,
    expr: String,
    expected: Edn,
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
// The strict schema and duplicate-key checks below are retained from conformance.rs.
fn unique_json_map<T: serde::de::DeserializeOwned>(
    source: &str,
) -> Result<BTreeMap<String, T>, serde_json::Error> {
    struct Unique<T>(std::marker::PhantomData<T>);
    impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for Unique<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("unique-key evidence map")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut input: A,
        ) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = input.next_entry::<String, T>()? {
                if result.insert(key.clone(), value).is_some() {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate evidence key {key}"
                    )));
                }
            }
            Ok(result)
        }
    }
    use serde::Deserializer;
    let mut decoder = serde_json::Deserializer::from_str(source);
    let result = decoder.deserialize_map(Unique(std::marker::PhantomData))?;
    decoder.end()?;
    Ok(result)
}

fn parse_cases(file: &str, content: &str) -> Result<Vec<Case>, String> {
    let forms = parse_all(content, &mut ParserState::new("suss"))
        .map_err(|e| format!("malformed conformance file {file}: {e:?}"))?;
    if forms.len() != 1 {
        return Err("case file must contain one vector".into());
    }
    let Edn::Vector(entries) = &forms[0] else {
        return Err("expected test vector".into());
    };
    if entries.is_empty() {
        return Err("empty conformance file".into());
    }
    let mut cases = Vec::new();
    let mut names = HashSet::new();
    for entry in entries {
        let Edn::Map(pairs) = entry else {
            return Err("expected test map".into());
        };
        let mut fields = HashMap::new();
        for (key, val) in pairs {
            let Edn::Keyword(key) = key else {
                return Err("non-keyword test field".into());
            };
            if key.namespace.is_some()
                || !["name", "expr", "expected", "category", "skip"].contains(&key.name.as_str())
            {
                return Err(format!("unknown test field {key:?}"));
            }
            if fields.insert(key.name.as_str(), val).is_some() {
                return Err("duplicate test field".into());
            }
        }
        let Some(Edn::String(name)) = fields.get("name") else {
            return Err("missing or invalid test name".into());
        };
        let Some(Edn::String(expr)) = fields.get("expr") else {
            return Err("missing or invalid expression".into());
        };
        if name.trim().is_empty() || expr.trim().is_empty() {
            return Err("empty name or expression".into());
        }
        let expected = fields.get("expected").ok_or("missing expected value")?;
        if let Some(category) = fields.get("category") {
            if !matches!(category, Edn::Keyword(_)) {
                return Err("invalid category".into());
            }
        }
        match fields.get("skip") {
            None | Some(Edn::Bool(false)) => (),
            Some(Edn::Bool(true)) => {
                return Err("record a known failure instead of a blanket skip".into());
            }
            Some(_) => return Err("skip must be Boolean".into()),
        }
        let id = format!("{file}/{name}");
        if !names.insert(id.clone()) {
            return Err(format!("duplicate case {id}"));
        }
        cases.push(Case {
            id,
            expr: expr.clone(),
            expected: (*expected).clone(),
        });
    }
    Ok(cases)
}

fn load_case_file(path: &std::path::Path, group: &str) -> Result<Vec<Case>, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("required conformance file {}: {e}", path.display()))?;
    parse_cases(group, &source)
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for file in ["core", "collections"] {
        let path = root().join(format!("reference/cljs-tests/{file}.sus"));
        cases.extend(load_case_file(&path, file).expect("missing or malformed conformance cases"));
    }
    cases
}

fn stage(error: SessionError) -> (String, String) {
    let name = match &error {
        // Validation currently travels through the compiler Diagnostic type.
        // Match the emitter's explicit prefix, not a guessed Wasm backtrace.
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
    };
    (name.into(), error.to_string())
}

fn run(case: &Case) -> Result<portable_decode::Observation, (String, String)> {
    // Preserve the prototype suite's fresh execution state per case. Reusing
    // source histories or mutable Stores would introduce order dependencies.
    let mut session =
        Session::new_repl().map_err(|error| ("runtime-bootstrap".into(), error.to_string()))?;
    session.set_operation_fuel(1_000_000_000);
    let mut macros =
        CompiledMacros::new().map_err(|error| ("macro-bootstrap".into(), error.to_string()))?;
    let mut decoder = Decoder::capture(&mut session, 100_000)
        .map_err(|error| ("decoder-capture".into(), error.to_string()))?;
    let value = match session.eval_with_macros(&case.expr, &mut macros) {
        Ok(value) => value,
        Err(SessionError::Language(payload)) => {
            // Preserve the actual rooted exception payload rather than the
            // SessionError display string, which deliberately omits its value.
            session
                .collect()
                .map_err(|error| ("gc".into(), error.to_string()))?;
            let detail = match decoder.decode_session(&mut session, &payload) {
                Ok(actual) => format!("uncaught payload {actual:?}"),
                Err(error) => format!("uncaught payload could not be decoded: {error}"),
            };
            return Err(("language-exception".into(), detail));
        }
        Err(error) => return Err(stage(error)),
    };
    session
        .collect()
        .map_err(|error| ("gc".into(), error.to_string()))?;
    let actual = decoder
        .decode_session(&mut session, &value)
        .map_err(|error| ("decode".into(), error.to_string()))?;
    if portable_compare::matches(&case.expected, &actual) {
        Ok(actual)
    } else {
        Err((
            "value".into(),
            format!("expected {:?}; actual {actual:?}", case.expected),
        ))
    }
}

#[test]
fn compiled_expression_corpus_retains_all_reviewed_inputs_and_observes_actual_values() {
    let cases = cases();
    let expected: BTreeMap<String, (String, String)> = unique_json_map(
        &std::fs::read_to_string(root().join("docs/compatibility/cases.json"))
            .expect("required catalog missing"),
    )
    .expect("malformed catalog");
    let actual = cases
        .iter()
        .map(|case| {
            (
                case.id.clone(),
                (case.expr.clone(), format!("{:?}", case.expected)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        actual, expected,
        "all reviewed corpus inputs must remain identical"
    );
    assert_eq!(cases.len(), 201, "retain every original case");
    let mut failures = BTreeMap::new();
    let mut observations = BTreeMap::new();
    for case in &cases {
        let result = run(case);
        let record = match &result {
            Ok(value) => serde_json::json!({
                "expr": case.expr, "expected": format!("{:?}", case.expected),
                "status": "pass", "actual": format!("{value:?}"),
            }),
            Err((stage, detail)) => serde_json::json!({
                "expr": case.expr, "expected": format!("{:?}", case.expected),
                "status": "failure", "stage": stage, "detail": detail,
            }),
        };
        observations.insert(case.id.clone(), record);
        if let Err(error) = result {
            eprintln!("OBSERVED {}: {}: {}", case.id, error.0, error.1);
            failures.insert(case.id.clone(), error);
        }
    }
    // Optional scratch evidence is written before the acceptance assertion so
    // a red run retains every real result. It is never read as a pass baseline.
    if let Some(path) = std::env::var_os("SUSS_COMPILED_CORPUS_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&observations).unwrap())
            .expect("write requested compiled corpus evidence");
    }
    eprintln!(
        "{} cases: {} passing, {} failing, 0 skipped",
        cases.len(),
        cases.len() - failures.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "compiled expression replacement has outstanding failures: {failures:#?}"
    );
}

#[test]
fn lossless_comparison_rejects_rounding_signed_zero_opaque_values_and_duplicate_reuse() {
    use portable_decode::Observation as O;
    let read = |source| {
        parse_all(source, &mut ParserState::new("suss"))
            .unwrap()
            .remove(0)
    };
    assert!(!portable_compare::matches(
        &read("9007199254740993"),
        &O::Number(9007199254740992.0_f64.to_bits())
    ));
    assert!(!portable_compare::matches(
        &read("0"),
        &O::Number((-0.0_f64).to_bits())
    ));
    assert!(!portable_compare::matches(&read("nil"), &O::Bool(false)));
    assert!(!portable_compare::matches(&read("42"), &O::Vector(vec![])));
    assert!(portable_compare::matches(
        &read("[1 2]"),
        &O::List(vec![
            O::Number(1.0_f64.to_bits()),
            O::Number(2.0_f64.to_bits())
        ])
    ));
    assert!(!portable_compare::matches(
        &read("#{1 2}"),
        &O::Set(vec![
            O::Number(1.0_f64.to_bits()),
            O::Number(1.0_f64.to_bits())
        ])
    ));
    assert!(portable_compare::matches(
        &read("##NaN"),
        &O::Number(0xfff8_0000_0000_0000)
    ));
    assert!(!portable_compare::matches(
        &read("##NaN"),
        &O::Number(0x7ff8_0000_0000_0001)
    ));
    assert!(portable_compare::matches(
        &read(r#""😀""#),
        &O::String(vec![0xd83d, 0xde00])
    ));
    assert!(!portable_compare::matches(
        &read(r#""😀""#),
        &O::String(vec![0xd83d])
    ));
}

#[test]
fn failure_stages_preserve_validation_compile_and_runtime_distinctions() {
    let diagnostic = |message: &str| {
        SessionError::Compile(suss_compile::portable::Diagnostic {
            span: 0..1,
            message: message.into(),
        })
    };
    assert_eq!(
        stage(diagnostic(
            "Generated fragment failed validation: bad operand"
        ))
        .0,
        "validation"
    );
    assert_eq!(stage(diagnostic("Unresolved name: missing")).0, "compile");
    assert_eq!(
        stage(SessionError::Trap(wasmtime::Error::msg("out of fuel"))).0,
        "trap"
    );
    assert_eq!(
        stage(SessionError::Host(wasmtime::Error::msg(
            "host callback failed"
        )))
        .0,
        "host"
    );
    assert_eq!(stage(SessionError::ForeignValue).0, "ownership");
}

#[test]
fn corpus_runner_records_actual_exception_payload_and_source_failure() {
    let make = |expr: &str| Case {
        id: "harness/negative".into(),
        expr: expr.into(),
        expected: Edn::Nil,
    };
    let exception = run(&make("(throw 42)")).unwrap_err();
    assert_eq!(exception.0, "language-exception");
    assert_eq!(
        exception.1,
        format!(
            "uncaught payload {:?}",
            portable_decode::Observation::Number(42.0_f64.to_bits())
        )
    );
    let unresolved = run(&make("definitely-missing-corpus-name")).unwrap_err();
    assert_eq!(unresolved.0, "compile");
    assert!(
        unresolved.1.contains("definitely-missing-corpus-name"),
        "{unresolved:?}"
    );
}
