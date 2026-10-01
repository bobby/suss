use suss_cli::portable_session::{Session, SessionValue};
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
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
    // Pinned core.cljs1054–1090 literals and safe-integer remainder branch.
    for (source, expected) in [
        ("(captured-hash nil)", 0.0f64),
        ("(captured-hash true)", 1231.0),
        ("(captured-hash false)", 1237.0),
        ("(captured-hash 0)", 0.0),
        ("(captured-hash 2147483648)", 1.0),
        ("(captured-hash ##Inf)", 2146435072.0),
        ("(captured-hash ##-Inf)", -1048576.0),
        ("(captured-hash ##NaN)", 2146959360.0),
        ("(captured-hash \"\")", 0.0),
    ] {
        let value = session.eval(source).unwrap();
        assert_eq!(number(&mut session, &value), expected.to_bits(), "{source}");
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
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
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
        ("-0.0", 0.0f64, 0.0f64),
        ("-0.9", 0.0, 0.0),
        ("17.9", 17.0, 17.0),
        ("-17.9", -17.0, -17.0),
        ("2147483648.9", 2147483648.0, -2147483648.0),
        ("4294967297", 4294967297.0, 1.0),
    ] {
        session
            .eval(&format!(
                "(def hash-date (suss.core/date-from-ms {milliseconds}))"
            ))
            .unwrap();
        session.collect().unwrap();
        let value = session.eval("(.valueOf hash-date)").unwrap();
        assert_eq!(number(&mut session, &value), stored.to_bits());
        let value = session.eval("(hash hash-date)").unwrap();
        assert_eq!(number(&mut session, &value), hash.to_bits());
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
        assert!(f64::from_bits(number(&mut session, &value)).is_nan());
        let value = session.eval("(hash invalid-hash-date)").unwrap();
        assert_eq!(number(&mut session, &value), 0.0f64.to_bits());
    }
    // Numeric-only construction rejects unsupported Date string/object inputs.
    assert!(matches!(
        session.eval("(suss.core/date-from-ms \"17\")"),
        Err(SessionError::Language(_))
    ));
    session.eval("(def date-method-effects 0) (extend-type suss.core/BootstrapDate Object (valueOf [_] (do (set! date-method-effects (+ date-method-effects 1)) 19)))").unwrap();
    let value = session.eval("(hash hash-date)").unwrap();
    assert_eq!(number(&mut session, &value), 19.0f64.to_bits());
    session
        .eval("(extend-type suss.core/BootstrapDate IHash (-hash [_] 17))")
        .unwrap();
    session.collect().unwrap();
    let value = session.eval("(hash hash-date)").unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
    let value = session.eval("date-method-effects").unwrap();
    assert_eq!(number(&mut session, &value), 1.0f64.to_bits());
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
                serde_json::json!({"tag":"f64","bits":format!("{:016x}",number(&mut session,&value))})
            }
            "bool" => {
                let boolean = session
                    .inspect(&value, |store, value| {
                        match value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .unwrap()
                            .get_u32()
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
        assert_eq!(observed, case["expected"], "{}", case["id"]);
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
    assert_eq!(number(&mut session, &value), 5.0f64.to_bits());
    let value = session.eval("(suss.bootstrap/f64-time-clip -0.9)").unwrap();
    assert_eq!(number(&mut session, &value), 0.0f64.to_bits());
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
