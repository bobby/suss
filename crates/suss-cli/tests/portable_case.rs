use suss_cli::portable_session::{Session, SessionValue};
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout");
            };
            Ok(*bits)
        })
        .unwrap()
}

#[test]
fn scalar_case_evaluates_selector_once_and_only_selected_result() {
    let mut session = Session::new().unwrap();
    session.eval("(def case-effects 0)").unwrap();
    let value = session.eval("(case (do (set! case-effects (+ case-effects 1)) 2) (1 2) 17 (do (set! case-effects 99) 19))").unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
    session.collect().unwrap();
    let value = session.eval("case-effects").unwrap();
    assert_eq!(number(&mut session, &value), 1.0f64.to_bits());
}

#[test]
fn scalar_case_matches_independently_decoded_pinned_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/case-cases.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 33);
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    for case in cases {
        let value = session
            .eval(case["source"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
        assert_eq!(
            serde_json::json!({"tag":"f64", "bits":format!("{:016x}",number(&mut session, &value))}),
            case["expected"],
            "{}",
            case["id"]
        );
        session.collect().unwrap();
    }
}

#[test]
fn empty_case_group_has_exact_located_boundary_without_publication() {
    use suss_cli::portable_session::SessionError;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/case-reference-boundaries.json"
    ))
    .unwrap();
    let case = &corpus["cases"][0];
    let mut session = Session::new().unwrap();
    let Err(SessionError::Compile(error)) = session.eval(&format!(
        "(def case-ghost 17) {}",
        case["source"].as_str().unwrap()
    )) else {
        panic!("exact compile boundary");
    };
    assert_eq!(error.message, case["native"]["message"].as_str().unwrap());
    assert!(error.span.end > error.span.start);
    assert!(matches!(
        session.eval("case-ghost"),
        Err(SessionError::Compile(_))
    ));
}

#[test]
fn case_errors_and_namespace_guards_keep_effects_and_recover_after_gc() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new().unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def case-effects 0)").unwrap();
    assert!(matches!(
        session.eval("(case (throw 17) 1 (set! case-effects 1) (set! case-effects 2))"),
        Err(SessionError::Language(_))
    ));
    session.collect().unwrap();
    let value = session.eval("case-effects").unwrap();
    assert_eq!(number(&mut session, &value), 0.0f64.to_bits());
    for source in [
        "(case)",
        "(case 1 1 17)",
        "(case 1 1 17 1 19 23)",
        "(case 1 (1 1) 17 19)",
        "(case 1 :a 17 19)",
        "(case 1 x 17 19)",
        "case",
    ] {
        let Err(SessionError::Compile(error)) =
            session.eval(&format!("(def case-ghost 7) {source}"))
        else {
            panic!("located case guard: {source}");
        };
        assert!(error.span.end > error.span.start);
        assert!(matches!(
            session.eval("case-ghost"),
            Err(SessionError::Compile(_))
        ));
    }
    let constants = (0..257)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let Err(SessionError::Compile(error)) = session.eval(&format!("(case 1 ({constants}) 17 19)"))
    else {
        panic!("bounded case expansion");
    };
    assert!(error.message.contains("expansion limit"));
    session
        .eval("(ns case.alias (:require [cljs.core :as c]))")
        .unwrap();
    let value = session.eval("(c/case 1 1 17 19)").unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
    session
        .eval("(ns case.excluded (:refer-clojure :exclude [case]))")
        .unwrap();
    assert!(matches!(
        session.eval("(case 1 1 17 19)"),
        Err(SessionError::Compile(_))
    ));
    let value = session.eval("(cljs.core/case 1 1 17 19)").unwrap();
    assert_eq!(number(&mut session, &value), 17.0f64.to_bits());
}
