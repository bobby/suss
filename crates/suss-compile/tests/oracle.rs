//! Independent prototype observations of the shared development oracle corpus.
mod support;

use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, process::Command, sync::OnceLock};
use support::decode::Decoder;
use suss_compile::Compiler;
use suss_core::{Edn, Number};
use wasmtime::{Config, Engine, Linker, Module, Store, Val};

const PIN: &str = "c4295f303100bbf5afac449242d30bca1126f1a1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    schema: u32,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    expr: String,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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
            Engine::new(&cfg).unwrap()
        })
        .clone()
}

fn string(value: &str) -> Value {
    json!({"tag": "string", "units": value.encode_utf16().collect::<Vec<_>>()})
}

// Encode host-decoded values without guest equality, printer or numeric coercion.
fn tagged(value: &Edn) -> Result<Value, String> {
    Ok(match value {
        Edn::Nil => json!({"tag": "nil"}),
        Edn::Bool(b) => json!({"tag": "bool", "value": b}),
        Edn::String(s) => string(s),
        Edn::Number(n) => {
            let f = match n {
                Number::Float(f) => *f,
                Number::Integer(n) => {
                    let f = Number::Integer(n.clone()).to_f64();
                    if !f.is_finite() || format!("{f:.0}") != n.to_string() {
                        return Err(format!(
                            "prototype integer cannot be represented losslessly as binary64: {n}"
                        ));
                    }
                    f
                }
                Number::Ratio(_) => return Err("prototype ratio is not a binary64 value".into()),
            };
            json!({"tag": "f64", "bits": format!("{:016x}", f.to_bits())})
        }
        Edn::Keyword(k) => {
            json!({"tag": "keyword", "namespace": k.namespace.as_deref().map(string).unwrap_or(json!({"tag":"nil"})), "name": string(&k.name)})
        }
        Edn::Symbol(s) => {
            json!({"tag": "symbol", "namespace": s.namespace.as_deref().map(string).unwrap_or(json!({"tag":"nil"})), "name": string(&s.name)})
        }
        Edn::Vector(items) | Edn::List(items) | Edn::Set(items) => {
            let tag = match value {
                Edn::Vector(_) => "vector",
                Edn::List(_) => "seq",
                _ => "set",
            };
            json!({"tag": tag, "items": items.iter().map(tagged).collect::<Result<Vec<_>,_>>()?})
        }
        Edn::Map(entries) => {
            json!({"tag": "map", "entries": entries.iter().map(|(k,v)| Ok(vec![tagged(k)?, tagged(v)?])).collect::<Result<Vec<_>,String>>()?})
        }
        _ => return Err(format!("unsupported host-decoded value {value:?}")),
    })
}

type Failure = (String, String);
fn failure(stage: &str, error: impl std::fmt::Display) -> Failure {
    (stage.into(), error.to_string())
}

fn observe(compiler: &mut Compiler, engine: &Engine, case: &Case) -> Result<Value, Failure> {
    // The body is identical to ClojureScript; only the test runner wrapper differs.
    let source = format!(
        "(let [trace (atom [])] (let [outcome (try [false {}] (catch error [true error]))] [outcome (deref trace)]))",
        case.expr
    );
    let artifact = compiler
        .compile_expr_cached(&source)
        .map_err(|e| failure("compile", e))?;
    if artifact.is_component {
        return Err(failure("artifact", "expected core module"));
    }
    let module =
        Module::new(engine, &artifact.wasm).map_err(|e| failure("validation", format!("{e:#}")))?;
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
    let mut result = [Val::null_any_ref()];
    eval.call(&mut store, &[], &mut result).map_err(|e| {
        let stage = if e.downcast_ref::<wasmtime::Trap>().is_some() {
            "trap"
        } else {
            "execution"
        };
        failure(stage, format!("{e:#}"))
    })?;
    let decoded = Decoder::new(compiler)
        .decode(&mut store, &result[0])
        .map_err(|e| failure("decode", e))?;
    let Edn::Vector(parts) = decoded else {
        return Err(failure("decode", "malformed observation envelope"));
    };
    if parts.len() != 2 {
        return Err(failure("decode", "wrong observation envelope length"));
    }
    let Edn::Vector(effects) = &parts[1] else {
        return Err(failure("decode", "malformed effect trace"));
    };
    let Edn::Vector(outcome) = &parts[0] else {
        return Err(failure("decode", "malformed outcome"));
    };
    if outcome.len() != 2 {
        return Err(failure("decode", "wrong outcome length"));
    }
    let effects = effects
        .iter()
        .map(tagged)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| failure("decode", e))?;
    let payload = tagged(&outcome[1]).map_err(|e| failure("decode", e))?;
    match outcome[0] {
        Edn::Bool(false) => {
            Ok(json!({"id":case.id,"status":"value","value":payload,"effects":effects}))
        }
        // These independently decoded prototype values are not Error or
        // ExceptionInfo objects. Pinned cljs ex-data/ex-message return nil for
        // non-Errors; preserve the exact thrown value instead of discarding it.
        Edn::Bool(true) => Ok(json!({"id":case.id,"status":"exception","thrown":payload,
            "data":{"tag":"nil"},"message":{"tag":"nil"},"effects":effects})),
        _ => Err(failure("decode", "invalid outcome discriminator")),
    }
}

fn observations() -> Value {
    let source = std::fs::read_to_string(root().join("tests/oracle/cases.json")).unwrap();
    let corpus: Corpus = serde_json::from_str(&source).expect("valid shared corpus");
    assert_eq!(corpus.schema, 1);
    assert!(!corpus.cases.is_empty());
    assert!(corpus.cases.iter().all(|case| !case.expr.trim().is_empty()));
    let engine = engine();
    let mut compiler = Compiler::new();
    let cases: Vec<_> = corpus
        .cases
        .iter()
        .map(|case| match observe(&mut compiler, &engine, case) {
            Ok(result) => result,
            Err((stage, diagnostic)) => {
                json!({"id":case.id,"status":"failure","stage":stage,"diagnostic":diagnostic})
            }
        })
        .collect();
    json!({"schema":2,"upstream":PIN,"cases":cases})
}

#[test]
fn shared_differential_baseline() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("observations.json");
    std::fs::write(&output, serde_json::to_vec_pretty(&observations()).unwrap()).unwrap();
    let reference = std::env::var_os("SUSS_ORACLE_REFERENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("tests/oracle/reference.json"));
    let status = Command::new("python3")
        .arg(root().join("scripts/oracle_compare.py"))
        .arg("--reference")
        .arg(reference)
        .arg("--observations")
        .arg(output)
        .arg("--baseline")
        .arg(root().join("tests/oracle/known-failures.json"))
        .status()
        .expect("Python development comparator");
    assert!(
        status.success(),
        "differential evidence changed; inspect exact diagnostics above"
    );
}

#[test]
#[ignore = "manual observation capture only; never updates expected failures"]
fn record_observations() {
    let output = std::env::var_os("SUSS_ORACLE_OUTPUT").expect("explicit output path required");
    std::fs::write(output, serde_json::to_vec_pretty(&observations()).unwrap()).unwrap();
}

#[test]
fn integer_transport_never_rounds_away_a_mismatch() {
    let exact = Number::parse_integer("9007199254740992").unwrap();
    let inexact = Number::parse_integer("9007199254740993").unwrap();
    assert_eq!(
        tagged(&Edn::Number(exact)).unwrap()["bits"],
        "4340000000000000"
    );
    assert!(
        tagged(&Edn::Number(inexact))
            .unwrap_err()
            .contains("losslessly")
    );
    assert_eq!(
        tagged(&Edn::Number(Number::Float(-0.0))).unwrap()["bits"],
        "8000000000000000"
    );
}

#[test]
fn caught_values_and_partial_effects_are_independently_decoded() {
    let engine = engine();
    let mut compiler = Compiler::new();
    for (source, expected) in [
        ("nil", json!({"tag":"nil"})),
        ("false", json!({"tag":"bool","value":false})),
        ("0", json!({"tag":"f64","bits":"0000000000000000"})),
        ("\"probe\"", string("probe")),
        (
            "{:reason :expected}",
            json!({"tag":"map","entries":[[
                {"tag":"keyword","namespace":{"tag":"nil"},"name":string("reason")},
                {"tag":"keyword","namespace":{"tag":"nil"},"name":string("expected")}
            ]]}),
        ),
    ] {
        let case = Case {
            id: "capture-regression".into(),
            expr: format!("(do (swap! trace conj :before) (throw {source}))"),
        };
        let result = observe(&mut compiler, &engine, &case).unwrap();
        assert_eq!(result["status"], "exception");
        assert_eq!(result["thrown"], expected);
        assert_eq!(result["data"], json!({"tag":"nil"}));
        assert_eq!(result["message"], json!({"tag":"nil"}));
        assert_eq!(
            result["effects"],
            json!([{"tag":"keyword","namespace":{"tag":"nil"},"name":string("before")}])
        );
    }
}

#[test]
fn fuel_trap_is_not_a_caught_language_exception() {
    let case = Case {
        id: "trap-regression".into(),
        expr: "(loop [i 0] (recur (inc i)))".into(),
    };
    let failure = observe(&mut Compiler::new(), &engine(), &case).unwrap_err();
    assert_eq!(failure.0, "trap", "{failure:?}");
    assert!(failure.1.contains("fuel"), "{}", failure.1);
}
