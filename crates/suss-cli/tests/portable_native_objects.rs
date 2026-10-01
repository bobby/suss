use suss_cli::portable_session::{Session, SessionError};

fn number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap();
    session
        .inspect(&value, |mut store, value| {
            store.gc(None)?;
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(object.field(&mut store, 0)?.unwrap_f64())
        })
        .unwrap()
}

#[test]
fn private_object_factory_and_properties_execute_ordered_source_after_gc() {
    let mut session = Session::new().unwrap();
    assert_eq!(number(&mut session, "(let [f (suss.bootstrap/object-factory) o (f \"x\" 7)] (suss.bootstrap/object-get o \"x\"))"), 7.0);
    assert_eq!(number(&mut session, "(let [f (suss.bootstrap/object-factory) o (f (array \"x\" 9))] (suss.bootstrap/object-get o \"x\"))"), 9.0);
    assert_eq!(number(&mut session, "(let [f (suss.bootstrap/object-factory) o (f (array (array \"x\" 11)))] (suss.bootstrap/object-get o \"x\"))"), 11.0);
    assert_eq!(number(&mut session, "(do (def object_effects 0) (def object_factory (suss.bootstrap/object-factory)) (let [o (object_factory (do (set! object_effects 1) \"x\") (do (set! object_effects (+ (* object_effects 10) 2)) 7))] (suss.bootstrap/object-set (do (set! object_effects (+ (* object_effects 10) 3)) o) (do (set! object_effects (+ (* object_effects 10) 4)) \"x\") (do (set! object_effects (+ (* object_effects 10) 5)) 9))) object_effects)"), 12345.0);
    assert_eq!(number(&mut session, "(let [f object_factory o (f) p (f \"answer\" 13)] (suss.bootstrap/object-set o \"__proto__\" p) (suss.bootstrap/object-get o \"answer\"))"), 13.0);
    assert_eq!(number(&mut session, "(let [f object_factory o (f)] (suss.bootstrap/object-set o -0.0 17) (suss.bootstrap/object-get o 0))"), 17.0);
}

#[test]
fn private_object_errors_are_atomic_or_runtime_and_recover() {
    let mut session = Session::new().unwrap();
    session
        .eval("(def factory (suss.bootstrap/object-factory)) (def effects 0)")
        .unwrap();
    let error = session
        .eval("(factory (do (set! effects 7) \"odd\"))")
        .unwrap_err();
    assert!(matches!(error, SessionError::Language(_)), "{error:?}");
    assert_eq!(number(&mut session, "effects"), 7.0);
    for source in [
        "(do (def unpublished_object 1) (suss.bootstrap/object-factory 7))",
        "(do (def unpublished_object 1) (suss.bootstrap/object-get nil))",
        "(do (def unpublished_object 1) (suss.bootstrap/object-set-strict nil))",
        "(do (def unpublished_object 1) (suss.bootstrap/object-set nil \"x\"))",
    ] {
        let error = session.eval(source).unwrap_err();
        assert!(matches!(error, SessionError::Compile(_)), "{error:?}");
        assert!(session.eval("unpublished_object").is_err());
    }
    for source in [
        "(suss.bootstrap/object-get nil \"x\")",
        "(suss.bootstrap/object-set (factory) (factory) 7)",
        "(let [a (array nil)] (aset a 0 a) (factory a))",
        "(let [a (array nil) b (array nil)] (aset a 0 b) (aset b 0 a) (factory a))",
    ] {
        let error = session.eval(source).unwrap_err();
        assert!(
            matches!(error, SessionError::Language(_)),
            "{source}: {error:?}"
        );
    }
    assert_eq!(
        number(
            &mut session,
            "(suss.bootstrap/object-get (factory \"x\" 19) \"x\")"
        ),
        19.0
    );
    assert_eq!(number(&mut session, "(do (def captured_factory factory) (set! factory (fn [] 7)) (suss.bootstrap/object-get (captured_factory \"x\" 23) \"x\"))"), 23.0);
    assert_eq!(number(&mut session, "(factory)"), 7.0);
}
