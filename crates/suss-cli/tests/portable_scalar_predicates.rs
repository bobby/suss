//! Execute portable primitive predicates through canonical live core cells.
use suss_cli::portable_session::{Session, SessionError, SessionValue};
use wasmtime::Val;

fn boolean(session: &mut Session, value: &SessionValue) -> bool {
    session
        .inspect(value, |store, value| {
            let sentinel = value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32();
            match sentinel {
                2 => Ok(false),
                4 => Ok(true),
                other => panic!("unexpected Boolean {other}"),
            }
        })
        .unwrap()
}
fn check(session: &mut Session, source: &str, expected: bool) {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    assert_eq!(boolean(session, &value), expected, "{source}");
}
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
fn primitive_predicates_preserve_nil_boolean_numeric_and_utf16_distinctions() {
    let mut s = Session::new().unwrap();
    for (source, expected) in [
        ("(nil? nil)", true),
        ("(nil? false)", false),
        ("(false? false)", true),
        ("(false? nil)", false),
        ("(false? 0)", false),
        ("(true? true)", true),
        ("(true? 1)", false),
        ("(number? ##NaN)", true),
        ("(number? ##Inf)", true),
        ("(number? -0.0)", true),
        ("(number? true)", false),
        ("(number? \"7\")", false),
        ("(string? \"\")", true),
        ("(string? \"\\uD800😀\")", true),
        ("(string? 7)", false),
        ("(string? (fn [] 7))", false),
        ("(undefined? nil)", false),
        ("(nil? (ex-data (new ExceptionInfo \"m\")))", true),
        ("(undefined? (ex-data (new ExceptionInfo \"m\")))", true),
        ("(undefined? (ex-cause (ex-info \"m\" 7)))", false),
    ] {
        check(&mut s, source, expected);
    }
}
#[test]
fn identical_compares_primitive_values_and_nominal_references() {
    let mut s = Session::new().unwrap();
    s.eval("(deftype Item [value]) (def item (Item. 7)) (def f (fn [] 7))")
        .unwrap();
    for (source, expected) in [
        ("(identical? 7 7)", true),
        ("(identical? 7 8)", false),
        ("(identical? 0 -0.0)", true),
        ("(identical? ##NaN ##NaN)", false),
        ("(let [n ##NaN] (identical? n n))", false),
        ("(identical? ##Inf ##Inf)", true),
        ("(identical? 0 false)", false),
        ("(identical? nil false)", false),
        ("(identical? false false)", true),
        ("(identical? \"same\" (+ \"sa\" \"me\"))", true),
        ("(identical? \"\\uD800😀\" (+ \"\\uD800\" \"😀\"))", true),
        ("(identical? \"ab\" \"a\")", false),
        ("(identical? \"ab\" \"ac\")", false),
        ("(identical? \"7\" 7)", false),
        ("(identical? item item)", true),
        ("(identical? item (Item. 7))", false),
        ("(identical? f f)", true),
        ("(identical? f (fn [] 7))", false),
        (
            "(identical? nil (ex-data (new ExceptionInfo \"m\")))",
            false,
        ),
    ] {
        check(&mut s, source, expected);
    }
}
#[test]
fn first_class_predicates_use_live_aliases_and_retained_original_closures() {
    let mut s = Session::new().unwrap();
    let original = s.eval("nil?").unwrap();
    let root = s
        .inspect(&original, |store, value| {
            value.unwrap_anyref().unwrap().to_owned_rooted(store)
        })
        .unwrap();
    for name in ["cljs.core/nil?", "suss.core/nil?"] {
        let alias = s.eval(name).unwrap();
        assert!(
            s.inspect(&alias, |store, value| wasmtime::Rooted::ref_eq(
                &store,
                &root,
                value.unwrap_anyref().unwrap()
            ))
            .unwrap()
        );
    }
    s.eval("(def read (fn [x] (nil? x)))").unwrap();
    s.enter_namespace("suss.core").unwrap();
    s.eval("(def nil? (fn [x] false))").unwrap();
    s.enter_namespace("user").unwrap();
    s.collect().unwrap();
    check(&mut s, "(read nil)", false);
    let nil = s.eval("nil").unwrap();
    let value = s.invoke(&original, &[&nil]).unwrap();
    assert!(boolean(&mut s, &value));
    check(&mut s, "(let [nil? (fn [x] true)] (nil? 42))", true);
}
#[test]
fn predicate_calls_evaluate_callee_and_arguments_once_and_recover_after_arity_errors() {
    let mut s = Session::new().unwrap();
    s.eval("(def trace 0) (def next (fn [] (do (def trace (+ trace 1)) trace)))")
        .unwrap();
    check(&mut s, "((do (next) identical?) (next) (next))", false);
    let trace = s.eval("trace").unwrap();
    assert_eq!(number(&mut s, &trace), 3.0f64.to_bits());
    assert!(matches!(
        s.eval("(nil? (next) (next))"),
        Err(SessionError::Language(_))
    ));
    let trace = s.eval("trace").unwrap();
    assert_eq!(number(&mut s, &trace), 5.0f64.to_bits());
    for source in [
        "(nil?)",
        "(identical? 1)",
        "(identical? 1 1 1)",
        "(number?)",
    ] {
        assert!(
            matches!(s.eval(source), Err(SessionError::Language(_))),
            "{source}"
        );
    }
    check(&mut s, "(number? 42)", true);
}

#[test]
fn every_predicate_factory_is_first_class_and_retained_across_collection() {
    let mut s = Session::new().unwrap();
    for (name, source, expected) in [
        ("nil?", "nil", true),
        ("false?", "false", true),
        ("true?", "true", true),
        ("undefined?", "(ex-data (new ExceptionInfo \"m\"))", true),
        ("number?", "##NaN", true),
        ("string?", "\"\\uD800😀\"", true),
    ] {
        let function = s.eval(name).unwrap();
        let value = s.eval(source).unwrap();
        s.collect().unwrap();
        let answer = s.invoke(&function, &[&value]).unwrap();
        assert_eq!(boolean(&mut s, &answer), expected, "{name}");
        assert!(matches!(
            s.invoke(&function, &[]),
            Err(SessionError::Language(_))
        ));
        assert!(matches!(
            s.invoke(&function, &[&value, &value]),
            Err(SessionError::Language(_))
        ));
    }
}

#[test]
fn utf16_identity_loop_obeys_fuel_and_recovers_with_rooted_inputs() {
    let mut s = Session::new().unwrap();
    let function = s.eval("identical?").unwrap();
    let text = "😀\\uD800".repeat(4096);
    let left = s.eval(&format!("\"{text}\"")).unwrap();
    let right = s.eval(&format!("\"{text}\"")).unwrap();
    let different = s.eval(&format!("\"{text}x\"")).unwrap();
    s.collect().unwrap();
    s.set_operation_fuel(5000);
    let SessionError::Trap(error) = s.invoke(&function, &[&left, &right]).unwrap_err() else {
        panic!("expected fuel trap inside UTF-16 identity loop")
    };
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&wasmtime::Trap::OutOfFuel)
    );
    s.collect().unwrap();
    s.set_operation_fuel(1_000_000);
    let answer = s.invoke(&function, &[&left, &right]).unwrap();
    assert!(boolean(&mut s, &answer));
    let answer = s.invoke(&function, &[&left, &different]).unwrap();
    assert!(!boolean(&mut s, &answer));
    check(&mut s, "(true? true)", true);
}
