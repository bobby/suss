//! Strict prototype baseline. Known failures are tracked, never counted as passes.
mod support;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;
use support::decode::{Decoder, values_match};
use suss_compile::Compiler;
use suss_core::Edn;
use suss_reader::{ParserState, parse_all};
use wasmtime::{Config, Engine, Linker, Module, Store, Val};

type Failure = (String, String);
type Baseline = BTreeMap<String, Failure>;

// serde_json's ordinary map decoding overwrites duplicate keys. Baseline and
// catalog evidence must reject them rather than discard an earlier observation.
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

fn compare_failures(expected: &Baseline, actual: &Baseline) -> Result<(), String> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "conformance changed: expected {expected:?}; actual {actual:?}"
        ))
    }
}

#[derive(Debug)]
struct Case {
    id: String,
    expr: String,
    expected: Edn,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn engine() -> Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut cfg = Config::new();
            cfg.wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&cfg).expect("conformance engine")
        })
        .clone()
}

fn failure(kind: &str, err: impl std::fmt::Display) -> Failure {
    (kind.into(), err.to_string())
}

fn run(compiler: &mut Compiler, engine: &Engine, case: &Case) -> Result<(), Failure> {
    let compiled = compiler
        .compile_expr_cached(&case.expr)
        .map_err(|e| failure("compile", e))?;
    if compiled.is_component {
        return Err(failure("artifact", "expected core module"));
    }
    let module =
        Module::new(engine, &compiled.wasm).map_err(|e| failure("validation", format!("{e:#}")))?;
    let mut store = Store::new(engine, ());
    store.set_fuel(20_000_000).unwrap();
    let mut linker = Linker::new(engine);
    linker
        .func_wrap("suss", "print_str", |_: i32, _: i32| {})
        .unwrap();
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| failure("instantiate", format!("{e:#}")))?;
    let eval = instance
        .get_func(&mut store, "eval")
        .ok_or_else(|| failure("artifact", "missing eval"))?;
    let mut results = [Val::null_any_ref()];
    eval.call(&mut store, &[], &mut results).map_err(|e| {
        let stage = if e.downcast_ref::<wasmtime::Trap>().is_some() {
            "trap"
        } else {
            "execution"
        };
        failure(stage, format!("{e:#}"))
    })?;
    let actual = Decoder::new(compiler)
        .decode(&mut store, &results[0])
        .map_err(|e| failure("decode", e))?;
    if values_match(&case.expected, &actual) {
        Ok(())
    } else {
        Err(failure(
            "value",
            format!("expected {:?}; actual {:?}", case.expected, actual),
        ))
    }
}

fn observe() -> (Baseline, usize) {
    let engine = engine();
    let mut compiler = Compiler::new();
    let cases = cases();
    let mut failures = Baseline::new();
    for case in &cases {
        if let Err(failure) = run(&mut compiler, &engine, case) {
            println!("KNOWN/OBSERVED {}: {}: {}", case.id, failure.0, failure.1);
            failures.insert(case.id.clone(), failure);
        }
    }
    println!(
        "{} cases: {} passing, {} failing, 0 skipped",
        cases.len(),
        cases.len() - failures.len(),
        failures.len()
    );
    (failures, cases.len())
}

#[test]
fn conformance_matches_reviewed_baseline() {
    let expected: Baseline = unique_json_map(
        &std::fs::read_to_string(root().join("docs/compatibility/known-failures.json"))
            .expect("required known-failure baseline missing"),
    )
    .expect("malformed baseline");
    let (actual, _) = observe();
    // Exact outcomes catch regressions, changed failures, stale entries and
    // unexpected passes. Updating this file is an explicit reviewed operation.
    compare_failures(&expected, &actual)
        .expect("review differences, fix regressions and remove resolved failures");
}

#[test]
#[ignore = "manual evidence capture; review the diff before accepting this baseline"]
fn record_known_failures() {
    let (failures, _) = observe();
    std::fs::write(
        root().join("docs/compatibility/known-failures.json"),
        serde_json::to_string_pretty(&failures).unwrap() + "\n",
    )
    .unwrap();
}

#[test]
fn comparison_rejects_opaque_values_and_wrong_collections() {
    let read = |s| {
        parse_all(s, &mut ParserState::new("suss"))
            .unwrap()
            .remove(0)
    };
    assert!(!values_match(&read("42"), &read("<gc-struct>")));
    assert!(!values_match(&read("[1 2]"), &read("[1 3]")));
    assert!(!values_match(&read("{:a 1}"), &read("{:a 2}")));
    assert!(!values_match(&read("#{1 2}"), &read("#{1 3}")));
    assert!(values_match(&read("{:a 1 :b 2}"), &read("{:b 2 :a 1}")));
    assert!(values_match(&read("[1 2]"), &read("(1 2)")));
    assert!(!values_match(&read("false"), &read("nil")));
    assert!(!values_match(&read("1.0"), &read("1.00001")));
    assert!(!values_match(
        &read("9007199254740993"),
        &read("9007199254740992.0")
    ));
    assert!(!values_match(&read("0"), &read("-0.0")));
}

#[test]
fn decoder_observes_nested_values_and_trie_boundaries() {
    let engine = engine();
    let mut compiler = Compiler::new();
    for (expr, expected) in [
        (
            "{:a [1 nil false] :b #{2 3}}",
            "{:b #{3 2} :a [1 nil false]}",
        ),
        ("(cons 1 (cons 2 nil))", "(1 2)"),
        (
            "[536870912 2.5 \"hello\" :a/b (symbol \"c/d\")]",
            "[536870912 2.5 \"hello\" :a/b c/d]",
        ),
    ] {
        let expected = parse_all(expected, &mut ParserState::new("suss"))
            .unwrap()
            .remove(0);
        run(
            &mut compiler,
            &engine,
            &Case {
                id: expr.into(),
                expr: expr.into(),
                expected,
            },
        )
        .unwrap();
    }
    let items = (0..1057)
        .map(|n| Edn::Number(suss_core::Number::from_i64(n)))
        .collect();
    run(
        &mut compiler,
        &engine,
        &Case {
            id: "vector trie".into(),
            expr: "(loop [v [] i 0] (if (< i 1057) (recur (conj v i) (inc i)) v))".into(),
            expected: Edn::Vector(items),
        },
    )
    .unwrap();
}

fn case_catalog() -> BTreeMap<String, (String, String)> {
    cases()
        .into_iter()
        .map(|case| (case.id, (case.expr, format!("{:?}", case.expected))))
        .collect()
}

#[test]
fn case_catalog_matches_reviewed_inputs() {
    let expected: BTreeMap<String, (String, String)> = unique_json_map(
        &std::fs::read_to_string(root().join("docs/compatibility/cases.json"))
            .expect("required conformance catalog missing"),
    )
    .expect("malformed conformance catalog");
    // Also protect passing cases: an empty failure baseline alone cannot detect
    // removed tests or changed expectations. Input changes require review too.
    assert_eq!(
        case_catalog(),
        expected,
        "conformance inputs changed; review the catalog diff"
    );
}

#[test]
#[ignore = "manual input catalog update; review removed cases and changed expectations"]
fn record_case_catalog() {
    std::fs::write(
        root().join("docs/compatibility/cases.json"),
        serde_json::to_string_pretty(&case_catalog()).unwrap() + "\n",
    )
    .unwrap();
}

#[test]
fn malformed_case_fields_do_not_become_passing_cases() {
    let valid = r#"[{:name "probe" :expr "42" :expected 42}]"#;
    assert_eq!(parse_cases("probe", valid).unwrap().len(), 1);
    for input in [
        "[]",
        "{}",
        "[42]",
        "[{:name",
        "[] []",
        r#"[{:name "p" :expr "42"}]"#,
        r#"[{:name "p" :expr "42" :expected 42 :expected 1}]"#,
        r#"[{:name "p" :expr "42" :expected 42 :skpi true}]"#,
        r#"[{:foreign/name "p" :expr "42" :expected 42}]"#,
        r#"[{:name "p" :expr "42" :expected 42 :skip "true"}]"#,
        r#"[{:name "p" :expr "42" :expected 42 :skip true}]"#,
        r#"[{:name " " :expr "42" :expected 42}]"#,
        r#"[{:name "p" :expr " " :expected 42}]"#,
        r#"[{:name "p" :expr "42" :expected 42 :category 1}]"#,
        r#"[{:name "p" :expr "42" :expected 42} {:name "p" :expr "42" :expected 42}]"#,
    ] {
        assert!(parse_cases("probe", input).is_err(), "accepted {input}");
    }
}

#[test]
fn decoder_rejects_executed_unknown_and_malformed_gc_layouts() {
    let engine = engine();
    let mut compiler = Compiler::new();
    for (body, expected) in [
        (
            "(func (export \"eval\") (result (ref null eq)) (ref.i31 (i32.const 6)))",
            "unknown i31 tag",
        ),
        (
            "(type $v (struct (field i32) (field i32))) (func (export \"eval\") (result (ref null eq)) (struct.new $v (i32.const 99999) (i32.const 42)))",
            "unsupported GC value tag",
        ),
        (
            "(type $v (struct (field i32) (field i32))) (func (export \"eval\") (result (ref null eq)) (struct.new $v (i32.const 1) (i32.const 42)))",
            "malformed Float64",
        ),
        (
            "(type $v (array i32)) (func (export \"eval\") (result (ref null eq)) (array.new_fixed $v 1 (i32.const 42)))",
            "raw non-string array",
        ),
    ] {
        let module = Module::new(&engine, format!("(module {body})")).unwrap();
        let mut store = Store::new(&engine, ());
        store.set_fuel(100_000).unwrap();
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
        let mut result = [Val::null_any_ref()];
        instance
            .get_func(&mut store, "eval")
            .unwrap()
            .call(&mut store, &[], &mut result)
            .unwrap();
        let error = Decoder::new(&mut compiler)
            .decode(&mut store, &result[0])
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn evidence_maps_reject_duplicates_and_changed_failures() {
    assert!(
        unique_json_map::<Failure>(r#"{"a":["value","wrong"],"a":["value","right"]}"#).is_err()
    );
    assert!(unique_json_map::<Failure>(r#"{} {}"#).is_err());
    assert!(unique_json_map::<Failure>(r#"{"a":true}"#).is_err());
    let known = Baseline::from([("a".into(), failure("value", "wrong"))]);
    assert!(compare_failures(&known, &known).is_ok());
    assert!(
        compare_failures(&known, &Baseline::new()).is_err(),
        "unexpected pass"
    );
    assert!(
        compare_failures(&Baseline::new(), &known).is_err(),
        "new failure"
    );
    let changed = Baseline::from([("a".into(), failure("decode", "unknown"))]);
    assert!(
        compare_failures(&known, &changed).is_err(),
        "changed stage/diagnostic"
    );
}

#[test]
fn missing_case_file_is_an_error_not_an_empty_suite() {
    let directory = tempfile::tempdir().unwrap();
    assert!(
        load_case_file(&directory.path().join("missing.sus"), "probe")
            .unwrap_err()
            .contains("required conformance file")
    );
}

#[test]
fn runtime_traps_are_distinct_from_uncaught_language_exceptions() {
    let engine = engine();
    let mut compiler = Compiler::new();
    for (expr, stage) in [
        ("(loop [i 0] (recur (inc i)))", "trap"),
        ("(throw \"expected\")", "execution"),
    ] {
        let case = Case {
            id: "stage-regression".into(),
            expr: expr.into(),
            expected: Edn::Nil,
        };
        let failure = run(&mut compiler, &engine, &case).unwrap_err();
        assert_eq!(failure.0, stage, "{failure:?}");
    }
}
