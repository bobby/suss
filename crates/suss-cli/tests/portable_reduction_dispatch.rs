//! Durable public reduction dispatch acceptance for #245, plus source-order supplement.
//! Raw values are decoded from ABI storage; representation is verified independently.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;
use compilation_fuel::COMPILATION_FUEL;

use portable_decode::{Decoder, Observation};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
use suss_cli::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionValue},
};
use wasmtime::{AnyRef, OwnedRooted, Rooted};

const CORPUS: &str = include_str!("../../../tests/oracle/reduction-dispatch-cases.json");
const FIXTURE: &str = include_str!("../../../tests/oracle/fixtures/reduction-dispatch-fixture.sus");
const DECODER: &str = "portable_decode::Decoder raw ABI observations";

struct CanonicalClass {
    _anchor: SessionValue,
    descriptor: OwnedRooted<AnyRef>,
    owner_type: wasmtime::StructType,
    owner_fields: usize,
}

fn canonical_from_value(
    session: &mut Session,
    anchor: SessionValue,
    label: &str,
) -> CanonicalClass {
    let (descriptor, owner_type, owner_fields) = session
        .inspect(&anchor, |mut store, value| {
            let owner = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .ok_or_else(|| wasmtime::Error::msg("canonical anchor is not a struct"))?;
            let descriptor = owner
                .field(&mut store, 0)?
                .unwrap_anyref()
                .unwrap()
                .to_owned_rooted(&mut store)?;
            let owner_type = owner.ty(&store)?;
            let owner_fields = owner.fields(&mut store)?.count();
            Ok((descriptor, owner_type, owner_fields))
        })
        .unwrap_or_else(|error| panic!("{label}: cannot capture canonical descriptor: {error}"));
    CanonicalClass {
        _anchor: anchor,
        descriptor,
        owner_type,
        owner_fields,
    }
}

// Host-side anchors read core class properties, which are private to
// suss.core; this probe uses the host's privileged evaluation.
fn canonical(session: &mut Session, source: &str, label: &str) -> CanonicalClass {
    let anchor = session
        .eval_privileged(source)
        .unwrap_or_else(|error| panic!("{label} anchor {source}: {error:?}"));
    canonical_from_value(session, anchor, label)
}

fn assert_canonical(session: &mut Session, source: &str, class: &CanonicalClass, label: &str) {
    let value = session
        .eval(source)
        .unwrap_or_else(|error| panic!("{label} eval {source}: {error:?}"));
    session
        .collect()
        .unwrap_or_else(|error| panic!("{label} collection: {error:?}"));
    session
        .inspect(&value, |mut store, value| {
            let owner = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .ok_or_else(|| wasmtime::Error::msg("actual value is not a nominal struct"))?;
            let actual_type = owner.ty(&store)?;
            if !wasmtime::StructType::eq(&actual_type, &class.owner_type) {
                return Err(wasmtime::Error::msg(
                    "owner StructType differs from captured canonical class",
                ));
            }
            if owner.fields(&mut store)?.count() != class.owner_fields {
                return Err(wasmtime::Error::msg(
                    "owner field count differs from captured canonical class",
                ));
            }
            let descriptor_value = owner.field(&mut store, 0)?;
            let actual_descriptor = descriptor_value.unwrap_anyref().unwrap();
            if !Rooted::ref_eq(&store, &class.descriptor, actual_descriptor)? {
                return Err(wasmtime::Error::msg(
                    "field-0 descriptor is not the captured canonical descriptor",
                ));
            }
            Ok(())
        })
        .unwrap_or_else(|error| panic!("{label} canonical identity check failed: {error}"));
}

fn raw(value: Observation) -> Value {
    fn string(units: Vec<u16>) -> Value {
        json!({"tag":"string", "units":units})
    }
    match value {
        Observation::Nil => json!({"tag":"nil"}),
        Observation::Bool(value) => json!({"tag":"bool", "value":value}),
        Observation::Number(bits) => json!({"tag":"f64", "bits":format!("{bits:016x}")}),
        Observation::String(units) => string(units),
        Observation::Keyword(namespace, name) => {
            json!({"tag":"keyword", "namespace":namespace.map(string).unwrap_or(json!({"tag":"nil"})), "name":string(name)})
        }
        Observation::Symbol(namespace, name) => {
            json!({"tag":"symbol", "namespace":namespace.map(string).unwrap_or(json!({"tag":"nil"})), "name":string(name)})
        }
        Observation::Vector(items) => {
            json!({"tag":"vector", "items":items.into_iter().map(raw).collect::<Vec<_>>() })
        }
        Observation::List(items) => {
            json!({"tag":"seq", "items":items.into_iter().map(raw).collect::<Vec<_>>() })
        }
        Observation::Map(entries) => {
            json!({"tag":"map", "entries":entries.into_iter().map(|(key, value)| json!([raw(key), raw(value)])).collect::<Vec<_>>() })
        }
        Observation::Set(items) => {
            json!({"tag":"set", "items":items.into_iter().map(raw).collect::<Vec<_>>() })
        }
        Observation::ExceptionInfo {
            message,
            data,
            cause,
        } => {
            json!({"tag":"exception-info", "message":raw(*message), "data":raw(*data), "cause":raw(*cause)})
        }
    }
}

#[test]
fn public_reduce_dispatch_and_source_order_match_pinned_cljs_after_gc() {
    let corpus: Value = serde_json::from_str(CORPUS).expect("valid pinned reduction corpus");
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["portable_reference"]["upstream_commit"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let original = corpus["cases"].as_array().expect("original cases");
    let supplement = corpus["source_order_supplement"]["cases"]
        .as_array()
        .expect("source-order cases");
    assert_eq!(original.len(), 7);
    assert_eq!(supplement.len(), 3);

    let mut phases = Vec::new();
    for (phase, mut session) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(COMPILATION_FUEL);
        let mut decoder = Decoder::capture(&mut session, 65536).unwrap();

        // Root known core descriptors before loading the user fixture; these
        // anchors and descriptors remain rooted through fixture GC and checks.
        let array_map_class = canonical(
            &mut session,
            "(.-EMPTY PersistentArrayMap)",
            "PersistentArrayMap",
        );
        let hash_map_class = canonical(
            &mut session,
            "(.-EMPTY PersistentHashMap)",
            "PersistentHashMap",
        );
        let hash_set_class = canonical(
            &mut session,
            "(.-EMPTY PersistentHashSet)",
            "PersistentHashSet",
        );
        let queue_class = canonical(&mut session, "(.-EMPTY PersistentQueue)", "PersistentQueue");
        session.collect().unwrap();

        session
            .eval_with_macros(FIXTURE, &mut macros)
            .unwrap_or_else(|error| {
                panic!(
                    "{phase}/fixture: {}",
                    suss_cli::portable_repl::error_display(&mut session, &error)
                )
            });
        session.collect().unwrap();
        decoder.refill(65536);

        assert_canonical(
            &mut session,
            "array-map-source",
            &array_map_class,
            "array-map-source",
        );
        assert_canonical(
            &mut session,
            "array-map-empty",
            &array_map_class,
            "array-map-empty",
        );
        assert_canonical(&mut session, "hamt-source", &hash_map_class, "hamt-source");
        assert_canonical(&mut session, "hamt-empty", &hash_map_class, "hamt-empty");
        assert_canonical(&mut session, "set-source", &hash_set_class, "set-source");
        assert_canonical(&mut session, "queue-source", &queue_class, "queue-source");

        // Capture the actual generated record descriptor only after declaration,
        // then prove both the fixture global and a fresh constructor share it.
        let record_anchor = session.eval("record-source").unwrap();
        session.collect().unwrap();
        let record_class =
            canonical_from_value(&mut session, record_anchor, "ReductionDispatchRecord");
        assert_canonical(
            &mut session,
            "record-source",
            &record_class,
            "record-source",
        );
        assert_canonical(
            &mut session,
            "(->ReductionDispatchRecord 11 13 17)",
            &record_class,
            "fresh ReductionDispatchRecord",
        );

        let mut observations = Vec::with_capacity(10);
        for case in original.iter().chain(supplement.iter()) {
            let id = case["id"].as_str().expect("case id");
            let source = case["source"].as_str().expect("case source");
            let result = session
                .eval_with_macros(source, &mut macros)
                .unwrap_or_else(|error| {
                    panic!(
                        "{phase}/{id}: {}",
                        suss_cli::portable_repl::error_display(&mut session, &error)
                    )
                });
            session.collect().unwrap();
            decoder.refill(65536);
            observations.push(json!({"id":id, "value":raw(decoder.decode_session(&mut session, &result).unwrap())}));
        }
        println!(
            "REDUCTION_DISPATCH_REPRESENTATION_PROOF phase={phase} core_classes=4 record_class=1 checks=8"
        );
        phases.push(json!({"phase":phase, "cases":observations}));
    }

    let report = json!({"decoder":DECODER, "phases":phases});
    println!("REDUCTION_DISPATCH_RAW_CAPTURE={report}");
    let mut child = Command::new("python3")
        .arg("-B")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/reduction_dispatch_oracle.py"
        ))
        .arg("--compare-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start strict host raw comparator");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(report.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "strict raw comparison failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!(
        "REDUCTION_DISPATCH_STRICT_COMPARISON={}",
        String::from_utf8_lossy(&output.stdout).trim()
    );
}
