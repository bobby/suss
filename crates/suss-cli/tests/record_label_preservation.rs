//! Compare private label construction with its pre-optimization implementation.
//! Pinned ClojureScript contract checks remain in compiled_record_labels.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::Decoder;
use suss_cli::portable_session::Session;
#[test]
fn private_label_preprocessing_preserves_utf16_results_after_gc() {
    let mut old = Session::new_macro().unwrap();
    let mut new = Session::new_macro().unwrap();
    old.eval(include_str!(
        "../../../tests/oracle/fixtures/record-label-before-delimiter.sus"
    ))
    .unwrap();
    let current = include_str!("../../suss-compile/src/portable_record_label.rs")
        .split_once("r###\"")
        .unwrap()
        .1
        .rsplit_once("\"###;")
        .unwrap()
        .0;
    new.eval(current).unwrap();
    let mut od = Decoder::capture(&mut old, 4096).unwrap();
    let mut nd = Decoder::capture(&mut new, 4096).unwrap();
    let mut cases = vec![
        String::new(),
        ".".into(),
        "..".into(),
        "...".into(),
        "....".into(),
        ".class.".into(),
        "class..for".into(),
        "/".into(),
        "//".into(),
        "///".into(),
        "////".into(),
        "a/b/c".into(),
        "a//b/".into(),
        "a/\nfor".into(),
        "a/\rfor".into(),
        "a/\u{2028}for".into(),
        "a/\u{2029}for".into(),
        "a/😀".into(),
        "😀/for".into(),
        "suss.m4.reduction-dispatch-preparation/ReductionDispatchRecord".into(),
    ];
    for unit in 0u8..=127 {
        cases.push(format!("for/{}./class/", char::from(unit)));
    }
    for input in cases {
        let source = format!(
            "(suss.internal.record-label/type-label {})",
            serde_json::to_string(&input).unwrap()
        );
        let ov = old.eval(&source).unwrap();
        let nv = new.eval(&source).unwrap();
        old.collect().unwrap();
        new.collect().unwrap();
        od.refill(4096);
        nd.refill(4096);
        assert_eq!(
            od.decode_session(&mut old, &ov).unwrap(),
            nd.decode_session(&mut new, &nv).unwrap(),
            "{source}"
        );
    }
    for input in [
        "\\uD800",
        "a/\\uD800",
        "a/\\uDC00",
        "a/\\uD83D\\uDE00",
        "\\uD800./for",
    ] {
        let source = format!("(suss.internal.record-label/type-label \"{input}\")");
        let ov = old.eval(&source).unwrap();
        let nv = new.eval(&source).unwrap();
        old.collect().unwrap();
        new.collect().unwrap();
        od.refill(4096);
        nd.refill(4096);
        assert_eq!(
            od.decode_session(&mut old, &ov).unwrap(),
            nd.decode_session(&mut new, &nv).unwrap(),
            "{source}"
        );
    }
}
