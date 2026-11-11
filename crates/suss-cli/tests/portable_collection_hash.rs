use suss_cli::portable_session::Session;

/// An exact runtime number: a long, or a double by its bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Number {
    Long(i64),
    Double(u64),
}
/// The expected number for `value`: integral values are longs, as in Clojure.
fn num(value: f64) -> Number {
    if value.is_finite()
        && value.fract() == 0.0
        && !(value == 0.0 && value.is_sign_negative())
        && value.abs() < 9_223_372_036_854_775_808.0
    {
        Number::Long(value as i64)
    } else {
        Number::Double(value.to_bits())
    }
}
fn exact_number(store: impl wasmtime::AsContextMut, value: &wasmtime::Val) -> wasmtime::Result<Number> {
    Ok(
        match suss_compile::portable_session::runtime_number(store, value)?
            .expect("expected a runtime number")
        {
            suss_compile::portable_session::RuntimeNumber::Long(value) => Number::Long(value),
            suss_compile::portable_session::RuntimeNumber::Double(value) => {
                Number::Double(value.to_bits())
            }
        },
    )
}

/// Whether `actual` matches a pinned `f64` observation. The ClojureScript
/// corpus cannot distinguish longs: an integral observation is a Clojure long,
/// or an integral double when the source has a double operand.
fn matches_pinned(actual: Number, expected: &serde_json::Value, source: &str) -> bool {
    let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
    match actual {
        Number::Long(_) => actual == num(f64::from_bits(bits)),
        Number::Double(actual) => {
            actual == bits
                && (num(f64::from_bits(bits)) == Number::Double(bits)
                    || source.contains("##")
                    || source.as_bytes().windows(3).any(|w| {
                        w[0].is_ascii_digit()
                            && (w[1] == b'.' || w[1] == b'e')
                            && (w[2].is_ascii_digit() || w[2] == b'-')
                    }))
        }
    }
}
/// The pinned observation when `actual` matches it, otherwise a distinct value.
fn number_observation(actual: Number, expected: &serde_json::Value, source: &str) -> serde_json::Value {
    if matches_pinned(actual, expected, source) {
        expected.clone()
    } else {
        serde_json::json!({ "tag": "number", "actual": format!("{actual:?}") })
    }
}

fn boolean(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
            {
                2 => Ok(false),
                4 => Ok(true),
                other => panic!("Boolean layout {other}"),
            }
        })
        .unwrap()
}

#[test]
fn sequential_hash_agrees_across_retained_types_after_gc() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def hash-list (list 1 2)) (def hash-cons (cons 1 (list 2))) (def hash-indexed (array-seq (array 1 2)))").unwrap();
    session.collect().unwrap();
    assert!(boolean(
        &mut session,
        "(= (hash hash-list) (hash hash-cons) (hash hash-indexed))"
    ));
    assert!(boolean(
        &mut session,
        "(= (hash (list)) (hash-ordered-coll nil))"
    ));
}

#[test]
fn unordered_helper_hash_is_independent_of_input_order() {
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    assert!(boolean(
        &mut session,
        "(= (hash-unordered-coll (list 1 2)) (hash-unordered-coll (list 2 1)))"
    ));
}

#[test]
fn collection_hash_matches_independently_decoded_primary_values() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/collection-hash-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 55);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    // Numbers hash as Clojure's hasheq (#336), so collections of numbers hash
    // as JVM Clojure 1.12's do where the ClojureScript observation differs.
    let hasheq: &[(&[&str], i64)] = &[
        (&["ordered-one", "public-one"], -1381383523),
        (&["unordered-one"], 1038464948),
        (
            &[
                "ordered-two",
                "public-two",
                "helper-cons",
                "hash-cons",
                "helper-indexed",
                "hash-indexed",
                "helper-reverse-seq",
                "hash-reverse-seq",
            ],
            156247261,
        ),
        (&["unordered-two", "unordered-reverse"], 460223544),
        (&["ordered-reverse", "public-reverse"], -1994590503),
        (&["ordered-mixed", "public-mixed"], 409217517),
        (&["unordered-mixed"], -1941226628),
        (&["ordered-nested", "public-nested"], 512463664),
        (&["unordered-nested"], -949793658),
        (&["ordered-duplicates"], 670845861),
        (&["unordered-duplicates"], -1628626703),
    ];
    let mut overridden = 0;
    for case in cases {
        let value = session
            .eval(case["source"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
        let id = case["id"].as_str().unwrap();
        if let Some((_, hash)) = hasheq.iter().find(|(ids, _)| ids.contains(&id)) {
            let actual = session
                .inspect(&value, |mut store, value| exact_number(&mut store, &value))
                .unwrap();
            assert_eq!(actual, Number::Long(*hash), "{id}");
            overridden += 1;
            session.collect().unwrap();
            continue;
        }
        let observed = session
            .inspect(&value, |mut store, value| {
                Ok(match case["expected"]["tag"].as_str().unwrap() {
                    "f64" => {
                        number_observation(
                        exact_number(&mut store, &value)?,
                        &case["expected"],
                        case["source"].as_str().unwrap(),
                    )
                    }
                    "bool" => {
                        let boolean = match suss_cli::portable_session::sentinel_tag(&mut store, &value)?.unwrap()
                        {
                            2 => false,
                            4 => true,
                            other => panic!("Boolean {other}"),
                        };
                        serde_json::json!({"tag":"bool","value":boolean})
                    }
                    tag => panic!("Unexpected corpus tag {tag}"),
                })
            })
            .unwrap();
        assert_eq!(observed, case["expected"], "{}", case["id"]);
        session.collect().unwrap();
    }
    assert_eq!(overridden, 23);
}

#[test]
fn cached_hash_suppresses_effects_and_recovers_after_a_throw() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval("(def guarded-list (list 1 2)) (def hash-effects 0)")
        .unwrap();
    let Err(SessionError::Language(thrown)) = session.eval("(with-redefs [hash-ordered-coll (fn [_] (do (set! hash-effects (+ hash-effects 1)) (throw 17)))] (hash guarded-list))") else { panic!("numeric language exception") };
    session
        .inspect(&thrown, |mut store, value| {
            assert_eq!(exact_number(&mut store, &value)?, num(17.0));
            Ok(())
        })
        .unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session, "(= hash-effects 1)"));
    session.eval("(def guarded-hash (hash guarded-list)) (def guarded-meta (with-meta guarded-list nil))").unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session,"(with-redefs [hash-ordered-coll (fn [_] (do (set! hash-effects (+ hash-effects 1)) 99))] (= (hash guarded-list) (hash guarded-meta) guarded-hash))"));
    assert!(boolean(&mut session, "(= hash-effects 1)"));
    let Err(SessionError::Language(error)) = session.eval("(hash-ordered-coll)") else {
        panic!("typed arity exception")
    };
    session
        .inspect(&error, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            assert_eq!(fields.len(), 5, "ABI2 Error");
            let text = fields[1]
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let units = text
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            // Clojure's arity error (ADR-0004).
            let message = String::from_utf16(&units).unwrap();
            assert!(message.starts_with("Wrong number of args ("), "{message}");
            Ok(())
        })
        .unwrap();
    session.collect().unwrap();
    assert!(boolean(
        &mut session,
        "(= (hash guarded-list) guarded-hash)"
    ));
}

#[test]
fn element_throw_stops_next_and_cons_cache_survives_gc() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval("(def element-trace 0) (def next-trace 0) (def original-next next)")
        .unwrap();
    for helper in ["hash-ordered-coll", "hash-unordered-coll"] {
        session
            .eval("(set! element-trace 0) (set! next-trace 0)")
            .unwrap();
        let source = format!("(with-redefs [hash (fn [x] (do (set! element-trace (+ (* element-trace 10) x)) (if (= x 2) (throw 17) x))) next (fn [x] (do (set! next-trace (+ next-trace 1)) (original-next x)))] ({helper} (list 1 2 3)))");
        let Err(SessionError::Language(thrown)) = session.eval(&source) else {
            panic!("exact element exception")
        };
        session
            .inspect(&thrown, |mut store, value| {
                assert_eq!(exact_number(&mut store, &value)?, num(17.0));
                Ok(())
            })
            .unwrap();
        session.collect().unwrap();
        assert!(boolean(&mut session, "(= element-trace 12)"));
        assert!(boolean(&mut session, "(= next-trace 1)"));
        assert!(boolean(
            &mut session,
            "(= (hash (list 1 2)) (hash-ordered-coll (list 1 2)))"
        ));
    }
    session.eval("(def cached-cons (cons 1 (list 2))) (def cached-cons-hash (hash cached-cons)) (def cached-cons-meta (with-meta cached-cons nil))").unwrap();
    session.collect().unwrap();
    assert!(boolean(&mut session, "(with-redefs [hash-ordered-coll (fn [_] (throw 17))] (= (hash cached-cons) (hash cached-cons-meta) cached-cons-hash))"));
}
