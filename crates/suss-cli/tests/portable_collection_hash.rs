use suss_cli::portable_session::Session;

fn boolean(session: &mut Session, source: &str) -> bool {
    let value = session.eval(source).unwrap();
    session
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
    use wasmtime::Val;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/collection-hash-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 49);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    for case in cases {
        let value = session
            .eval(case["source"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
        let observed = session
            .inspect(&value, |mut store, value| {
                Ok(match case["expected"]["tag"].as_str().unwrap() {
                    "f64" => {
                        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                        let [Val::F64(bits)] = fields.as_slice() else {
                            panic!("Number layout")
                        };
                        serde_json::json!({"tag":"f64","bits":format!("{bits:016x}")})
                    }
                    "bool" => {
                        let boolean = match value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .unwrap()
                            .get_u32()
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
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [wasmtime::Val::F64(bits)] = fields.as_slice() else {
                panic!("Number exception layout")
            };
            assert_eq!(*bits, 17.0f64.to_bits());
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
            assert_eq!(units, "Wrong arity".encode_utf16().collect::<Vec<_>>());
            Ok(())
        })
        .unwrap();
    session.collect().unwrap();
    assert!(boolean(
        &mut session,
        "(= (hash guarded-list) guarded-hash)"
    ));
}
