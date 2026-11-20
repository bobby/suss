use suss_cli::portable_session::{RuntimeNumber, Session, SessionValue};
use suss_cli::portable_session::RuntimeNumber::{Double, Long};

fn number(session: &mut Session, value: &SessionValue) -> RuntimeNumber {
    session
        .inspect(value, |store, value| {
            Ok(suss_cli::portable_session::runtime_number(store, &value)?.expect("Number layout"))
        })
        .unwrap()
}

/// The observation of a runtime number, distinguishing a long from a double.
fn number_json(number: RuntimeNumber) -> serde_json::Value {
    match number {
        RuntimeNumber::Long(value) => serde_json::json!({"tag":"long", "value":value}),
        RuntimeNumber::Double(value) => {
            serde_json::json!({"tag":"f64", "bits":format!("{:016x}", value.to_bits())})
        }
    }
}
/// The ClojureScript oracle encodes every number as a double; JVM Clojure,
/// Suss's oracle, gives a long for these integral results, including the
/// ClojureScript `-0` hash of `-0.0`, which is `0` in Clojure.
fn clojure_number(expected: &serde_json::Value) -> serde_json::Value {
    if expected["tag"] == "f64" {
        let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
        let value = f64::from_bits(bits);
        if value.is_finite() && value.fract() == 0.0 {
            return serde_json::json!({"tag":"long", "value":value as i64});
        }
    }
    expected.clone()
}

#[test]
fn public_hash_retains_scalar_branches_and_captured_callable_after_gc() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def captured-hash hash)").unwrap();
    session.collect().unwrap();
    // Pinned core.cljs1054–1090 literals; numbers hash as Clojure's hasheq.
    for (source, expected) in [
        ("(captured-hash nil)", 0),
        ("(captured-hash true)", 1231),
        ("(captured-hash false)", 1237),
        ("(captured-hash 0)", 0),
        // A long hashes as Clojure's Murmur3 hashLong (#336).
        ("(captured-hash 2147483648)", 386683422),
        ("(captured-hash ##Inf)", 2146435072),
        ("(captured-hash ##-Inf)", -1048576),
        ("(captured-hash ##NaN)", 2146959360),
        ("(captured-hash \"\")", 0),
    ] {
        let value = session.eval(source).unwrap();
        assert_eq!(number(&mut session, &value), Long(expected), "{source}");
        session.collect().unwrap();
    }
}

#[test]
fn numeric_date_storage_executes_without_full_core() {
    let mut session = Session::new().unwrap();
    session
        .eval("(deftype DateProbe [milliseconds] Object (valueOf [this] milliseconds))")
        .unwrap();
    session
        .eval("(def date-probe (fn [ms] (DateProbe. (suss.bootstrap/f64-time-clip ms))))")
        .unwrap();
    let value = session.eval("(.valueOf (date-probe 17.9))").unwrap();
    assert_eq!(number(&mut session, &value), Long(17));
}

#[test]
fn date_hash_normalizes_numeric_milliseconds_and_keeps_protocol_priority() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    for (milliseconds, stored, hash) in [
        ("-0.0", 0, 0),
        ("-0.9", 0, 0),
        ("17.9", 17, 17),
        ("-17.9", -17, -17),
        ("2147483648.9", 2147483648, -2147483648),
        ("4294967297", 4294967297, 1),
    ] {
        session
            .eval(&format!(
                "(def hash-date (suss.core/date-from-ms {milliseconds}))"
            ))
            .unwrap();
        session.collect().unwrap();
        let value = session.eval("(.valueOf hash-date)").unwrap();
        // Time-clipped milliseconds are a long, as `Date.getTime` is in Clojure.
        assert_eq!(number(&mut session, &value), Long(stored));
        let value = session.eval("(hash hash-date)").unwrap();
        assert_eq!(number(&mut session, &value), Long(hash));
    }
    for milliseconds in [
        "8640000000000001",
        "-8640000000000001",
        "##Inf",
        "##-Inf",
        "##NaN",
    ] {
        session
            .eval(&format!(
                "(def invalid-hash-date (suss.core/date-from-ms {milliseconds}))"
            ))
            .unwrap();
        let value = session.eval("(.valueOf invalid-hash-date)").unwrap();
        assert!(matches!(number(&mut session, &value), Double(value) if value.is_nan()));
        let value = session.eval("(hash invalid-hash-date)").unwrap();
        assert_eq!(number(&mut session, &value), Long(0));
    }
    // Numeric-only construction rejects unsupported Date string/object inputs.
    assert!(matches!(
        session.eval("(suss.core/date-from-ms \"17\")"),
        Err(SessionError::Language(_))
    ));
    session.eval("(def date-method-effects 0) (extend-type suss.core/BootstrapDate Object (valueOf [_] (do (set! date-method-effects (+ date-method-effects 1)) 19)))").unwrap();
    let value = session.eval("(hash hash-date)").unwrap();
    assert_eq!(number(&mut session, &value), Long(19));
    session
        .eval("(extend-type suss.core/BootstrapDate IHash (-hash [_] 17))")
        .unwrap();
    session.collect().unwrap();
    let value = session.eval("(hash hash-date)").unwrap();
    assert_eq!(number(&mut session, &value), Long(17));
    let value = session.eval("date-method-effects").unwrap();
    assert_eq!(number(&mut session, &value), Long(1));
}

#[test]
fn public_hash_matches_independently_decoded_primary_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/public-hash-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 47);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval("(def date-ms suss.core/date-from-ms) (def rootobj (fn [] (suss.core/root-obj)))")
        .unwrap();
    for case in cases {
        let value = session
            .eval(case["source"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
        let observed = match case["expected"]["tag"].as_str().unwrap() {
            "f64" => {
                number_json(number(&mut session, &value))
            }
            "bool" => {
                let boolean = session
                    .inspect(&value, |mut store, value| {
                        match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
                        {
                            2 => Ok(false),
                            4 => Ok(true),
                            other => panic!("Boolean {other}"),
                        }
                    })
                    .unwrap();
                serde_json::json!({"tag":"bool","value":boolean})
            }
            tag => panic!("Unexpected corpus tag {tag}"),
        };
        // A number hashes as Clojure's hasheq (#336); these are JVM Clojure
        // 1.12's hashes where the ClojureScript observation differs.
        let hasheq = [
            ("one", 1392991556),
            ("minus-one", 1651860712),
            ("modulus", 1819228606),
            ("modulus-plus-one", 386683422),
            ("negative-modulus", 622954245),
            ("negative-modulus-minus-one", -2073034792),
            ("fraction", 1073217536),
            ("negative-fraction", -1074266112),
            ("subnormal", 1),
            ("large-nonsafe", -2093373915),
            ("review-safe-max", -1247966309),
            ("review-negative-safe-max", -19448363),
            // `hash` computes a string's hash in the runtime (#409), as JVM
            // Clojure's String.hashCode, instead of through the live
            // hash-string and m3-hash-int vars, so redefining them doesn't
            // change it: this is (hash "review") itself.
            ("review-captured-live-string-helpers", 2019226501),
        ];
        let expected = match hasheq.iter().find(|(id, _)| *id == case["id"]) {
            Some((_, hash)) => serde_json::json!({"tag":"long", "value":hash}),
            None => clojure_number(&case["expected"]),
        };
        assert_eq!(observed, expected, "{}", case["id"]);
        session.collect().unwrap();
    }
}

#[test]
fn time_clip_rejects_wrong_storage_and_preserves_effects_and_compile_atomicity() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session.eval("(def clip-effects 0)").unwrap();
    for source in ["nil", "false", "true", "\"17\"", "(fn [] 17)"] {
        assert!(matches!(session.eval(&format!("(suss.bootstrap/f64-time-clip (do (set! clip-effects (+ clip-effects 1)) {source}))")), Err(SessionError::Language(_))), "{source}");
        session.collect().unwrap();
    }
    let value = session.eval("clip-effects").unwrap();
    assert_eq!(number(&mut session, &value), Long(5));
    let value = session.eval("(suss.bootstrap/f64-time-clip -0.9)").unwrap();
    assert_eq!(number(&mut session, &value), Long(0));
    assert!(matches!(
        session.eval("(suss.bootstrap/f64-time-clip (throw 17))"),
        Err(SessionError::Language(_))
    ));
    for source in [
        "(suss.bootstrap/f64-time-clip)",
        "(suss.bootstrap/f64-time-clip 1 2)",
        "suss.bootstrap/f64-time-clip",
    ] {
        let Err(SessionError::Compile(error)) =
            session.eval(&format!("(def clip-ghost 7) {source}"))
        else {
            panic!("private unary adapter must reject {source}");
        };
        assert!(error.span.end > error.span.start);
        assert!(matches!(
            session.eval("clip-ghost"),
            Err(SessionError::Compile(_))
        ));
    }
}
