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

#[derive(Debug)]
struct Case {
    id: String,
    expr: String,
    expected: Edn,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut names = HashSet::new();
    for file in ["core", "collections"] {
        let path = root().join(format!("reference/cljs-tests/{file}.sus"));
        let content = std::fs::read_to_string(&path).expect("required conformance file missing");
        let forms =
            parse_all(&content, &mut ParserState::new("suss")).expect("malformed conformance file");
        assert_eq!(forms.len(), 1, "{} must contain one vector", path.display());
        let Edn::Vector(entries) = &forms[0] else {
            panic!("expected test vector")
        };
        assert!(!entries.is_empty(), "empty conformance file");
        for entry in entries {
            let Edn::Map(pairs) = entry else {
                panic!("expected test map: {entry:?}")
            };
            let mut fields = HashMap::new();
            for (key, val) in pairs {
                let Edn::Keyword(key) = key else {
                    panic!("non-keyword test field")
                };
                assert!(
                    fields.insert(key.name.as_str(), val).is_none(),
                    "duplicate test field"
                );
            }
            let Some(Edn::String(name)) = fields.get("name") else {
                panic!("missing test name")
            };
            let Some(Edn::String(expr)) = fields.get("expr") else {
                panic!("missing expression: {name}")
            };
            let expected = fields.get("expected").expect("missing expected value");
            assert!(
                !matches!(fields.get("skip"), Some(Edn::Bool(true))),
                "{name}: record a known failure instead of a blanket skip"
            );
            let id = format!("{file}/{name}");
            assert!(names.insert(id.clone()), "duplicate case {id}");
            cases.push(Case {
                id,
                expr: expr.clone(),
                expected: (*expected).clone(),
            });
        }
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
    eval.call(&mut store, &[], &mut results)
        .map_err(|e| failure("execution", format!("{e:#}")))?;
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
    let expected: Baseline = serde_json::from_str(
        &std::fs::read_to_string(root().join("docs/compatibility/known-failures.json"))
            .expect("required known-failure baseline missing"),
    )
    .expect("malformed baseline");
    let (actual, _) = observe();
    // Exact outcomes catch regressions, changed failures, stale entries and
    // unexpected passes. Updating this file is an explicit reviewed operation.
    assert_eq!(
        actual, expected,
        "conformance changed: review each difference, fix regressions and remove resolved failures"
    );
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
    let expected: BTreeMap<String, (String, String)> = serde_json::from_str(
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
