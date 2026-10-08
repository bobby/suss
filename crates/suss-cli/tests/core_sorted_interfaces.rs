//! Retained sorted protocols on original nominal probes; no tree implementation.
use suss_cli::portable_session::{Session, SessionError, SessionOptions, SessionValue};
use suss_compile::portable::resolve::Phase;
use wasmtime::Val;

const CORE: &str = include_str!("../../../runtime/core-import/suss/core.sus");
const FIXTURE: &str = include_str!("../../../tests/oracle/fixtures/sorted-interface-probe.sus");
const PIN: &str = "c4295f303100bbf5afac449242d30bca1126f1a1";

fn observation(session: &mut Session, value: &SessionValue) -> serde_json::Value {
    session
        .inspect(value, |mut store, value| {
            let value = value.unwrap_anyref().unwrap();
            if let Some(boolean) = value.as_i31(&store)? {
                return Ok(match boolean.get_u32() {
                    2 => serde_json::json!({"tag":"bool", "value":false}),
                    4 => serde_json::json!({"tag":"bool", "value":true}),
                    other => panic!("unexpected scalar sentinel {other}"),
                });
            }
            let object = value.as_struct(&store)?.expect("number struct");
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("number layout")
            };
            Ok(serde_json::json!({"tag":"f64", "bits":format!("{bits:016x}")}))
        })
        .unwrap()
}

// Inspect the actual exception payload, not merely the SessionError variant.
// Arity descriptor ID 1 follows persistent_session.rs; nominal ID 7 is
// numeric::ERROR_GLOBALS (6) + 1, initialized in runtime_abi.rs.
fn expect_language_error(
    session: &mut Session,
    error: SessionError,
    expected_descriptor: i64,
    expected_message: &str,
    source: &str,
) {
    let SessionError::Language(payload) = error else {
        panic!("expected language payload for {source}: {error}")
    };
    session.collect().unwrap();
    let (descriptor_id, message) = session
        .inspect(&payload, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let descriptor = object
                .field(&mut store, 0)?
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            let descriptor_id = descriptor.field(&mut store, 0)?.unwrap_i64();
            let text = object
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let message = text
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok((descriptor_id, message))
        })
        .unwrap();
    assert_eq!(descriptor_id, expected_descriptor, "{source}");
    assert_eq!(
        message,
        expected_message.encode_utf16().collect::<Vec<_>>(),
        "{source}"
    );
}

fn setup(phase: Phase) -> Session {
    let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
    session.eval(CORE).unwrap();
    session.enter_namespace("user").unwrap();
    session.eval(FIXTURE).unwrap();
    session.collect().unwrap();
    session
}

fn expect_number(session: &mut Session, source: &str, expected: f64) {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    assert_eq!(
        observation(session, &value),
        serde_json::json!({"tag":"f64", "bits":format!("{:016x}",expected.to_bits())}),
        "{source}"
    );
}

#[test]
fn sorted_protocol_shared_nominal_observations_match_both_phases_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/sorted-interface-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(corpus["upstream"], PIN);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 27);
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = setup(phase);
        let mut ids = std::collections::BTreeSet::new();
        for case in cases {
            let id = case["id"].as_str().unwrap();
            assert!(ids.insert(id));
            let source = case["source"].as_str().unwrap();
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("{phase:?}, {id}: {error}"));
            session.collect().unwrap();
            assert_eq!(
                observation(&mut session, &value),
                case["expected"],
                "{phase:?}, {id}"
            );
        }
    }
}

#[test]
fn sorted_protocol_aliases_retained_values_reload_and_method_arities() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = setup(phase);
        // Reload the actual retained source, preserving old nominal instances and callees.
        session.eval(CORE).unwrap();
        session.enter_namespace("user").unwrap();
        session.collect().unwrap();
        for (protocol, saved) in [
            ("ISorted", "saved-sorted-protocol"),
            ("IComparable", "saved-comparable-protocol"),
        ] {
            for namespace in ["cljs.core", "suss.core"] {
                let source = format!(
                    "(and (identical? {namespace}/{protocol} {saved}) (implements? {namespace}/{protocol} sorted-interface-probe))"
                );
                let value = session.eval(&source).unwrap();
                session.collect().unwrap();
                assert_eq!(
                    observation(&mut session, &value),
                    serde_json::json!({"tag":"bool","value":true}),
                    "{phase:?}, {source}"
                );
            }
        }
        for method in [
            "-sorted-seq",
            "-sorted-seq-from",
            "-entry-key",
            "-comparator",
            "-compare",
        ] {
            let source = format!("(identical? cljs.core/{method} suss.core/{method})");
            let value = session.eval(&source).unwrap();
            session.collect().unwrap();
            assert_eq!(
                observation(&mut session, &value),
                serde_json::json!({"tag":"bool","value":true}),
                "{phase:?}, {source}"
            );
        }
        for (method, args, saved, expected) in [
            (
                "-sorted-seq",
                "sorted-interface-probe true",
                "saved-sorted-seq",
                17.0,
            ),
            (
                "-sorted-seq-from",
                "sorted-interface-probe 5 false",
                "saved-sorted-from",
                12.0,
            ),
            (
                "-entry-key",
                "sorted-interface-probe 8",
                "saved-entry-key",
                25.0,
            ),
            (
                "-compare",
                "sorted-interface-probe 20",
                "saved-compare",
                -3.0,
            ),
        ] {
            for namespace in ["cljs.core", "suss.core"] {
                expect_number(
                    &mut session,
                    &format!("({namespace}/{method} {args})"),
                    expected,
                );
            }
            expect_number(&mut session, &format!("({saved} {args})"), expected);
        }
        for callee in [
            "cljs.core/-comparator",
            "suss.core/-comparator",
            "saved-comparator",
        ] {
            expect_number(
                &mut session,
                &format!("(({callee} sorted-interface-probe) 11 4)"),
                7.0,
            );
        }
        // Test each published signature, including the receiver, one operand short/long.
        for (method, valid) in [
            ("-sorted-seq", vec!["sorted-interface-probe", "true"]),
            (
                "-sorted-seq-from",
                vec!["sorted-interface-probe", "5", "false"],
            ),
            ("-entry-key", vec!["sorted-interface-probe", "8"]),
            ("-comparator", vec!["sorted-interface-probe"]),
            ("-compare", vec!["sorted-interface-probe", "20"]),
        ] {
            let short = valid[..valid.len() - 1].join(" ");
            let long = format!("{} 99", valid.join(" "));
            for arguments in [short, long] {
                for namespace in ["cljs.core", "suss.core"] {
                    let source = format!("({namespace}/{method} {arguments})");
                    let error = session.eval(&source).unwrap_err();
                    expect_language_error(&mut session, error, 1, "Wrong arity", &source);
                    session.collect().unwrap();
                    expect_number(
                        &mut session,
                        "(saved-compare sorted-interface-probe 20)",
                        -3.0,
                    );
                }
            }
        }
        for source in [
            "(cljs.core/-sorted-seq unsorted-interface-probe true)",
            "(cljs.core/-sorted-seq-from unsorted-interface-probe 5 false)",
            "(cljs.core/-entry-key unsorted-interface-probe 8)",
            "(cljs.core/-comparator unsorted-interface-probe)",
            "(cljs.core/-compare unsorted-interface-probe 20)",
        ] {
            let error = session.eval(source).unwrap_err();
            expect_language_error(&mut session, error, 7, "Invalid nominal operation", source);
            session.collect().unwrap();
            expect_number(
                &mut session,
                "(saved-compare sorted-interface-probe 20)",
                -3.0,
            );
        }
        // An explicit public redefinition is visible through both namespace aliases;
        // the previously captured protocol function remains callable after GC.
        session.eval("(def old-sorted-method cljs.core/-sorted-seq) (set! cljs.core/-sorted-seq (fn [coll ascending?] 99))").unwrap();
        session.collect().unwrap();
        for namespace in ["cljs.core", "suss.core"] {
            expect_number(
                &mut session,
                &format!("({namespace}/-sorted-seq sorted-interface-probe true)"),
                99.0,
            );
        }
        expect_number(
            &mut session,
            "(saved-sorted-seq sorted-interface-probe true)",
            17.0,
        );
        session
            .eval("(set! suss.core/-sorted-seq old-sorted-method)")
            .unwrap();
        expect_number(
            &mut session,
            "(cljs.core/-sorted-seq sorted-interface-probe true)",
            17.0,
        );
    }
}
