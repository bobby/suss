//! Anonymous publication's genuine cell-state kernel; UNCOMPILED/UNEXECUTED.
//! This is not the whole exists? macro, owned path resolution, reify or records.
use suss_cli::{
    portable_macro_data::FormBridge,
    portable_session::{Session, SessionValue},
};
use suss_reader::forms::Kind;

fn boolean(session: &mut Session, value: &SessionValue, expected: bool) {
    session.collect().unwrap();
    let bridge = FormBridge::new(session).unwrap();
    let actual = bridge.read(session, value, 0..0).unwrap();
    assert!(matches!(actual.kind, Kind::Bool(value) if value == expected));
}

#[test]
fn publication_cell_distinguishes_unbound_undefined_nil_and_false_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let probe = session
            .eval("(fn [] (suss.compiler/cell-defined? user/publication-value))")
            .unwrap();
        let result = session.invoke(&probe, &[]).unwrap();
        boolean(&mut session, &result, false);
        for (source, expected) in [
            ("(def publication-value nil)", true),
            ("(def publication-value false)", true),
            ("(def publication-value 0)", true),
            ("(def publication-value (aget (make-array 1) 0))", false),
            (
                "(def publication-effects 0) (def publication-value (js-obj \"valueOf\" (fn [] (set! publication-effects 99))))",
                true,
            ),
        ] {
            session.eval(source).unwrap();
            session.collect().unwrap();
            let result = session.invoke(&probe, &[]).unwrap();
            boolean(&mut session, &result, expected);
        }
        let effects = session.eval("publication-effects").unwrap();
        session.collect().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let actual = bridge.read(&mut session, &effects, 0..0).unwrap();
        assert!(matches!(actual.kind, Kind::Number(value) if value.to_bits() == 0.0_f64.to_bits()));
        let result = session
            .eval("(binding [publication-value (aget (make-array 1) 0)] (suss.compiler/cell-defined? user/publication-value))");
        // Preserve the existing binding kernel's selected dynamic value.
        boolean(&mut session, &result.unwrap(), false);
        let result = session.invoke(&probe, &[]).unwrap();
        boolean(&mut session, &result, true);
    }
}

#[test]
fn publication_probe_sees_later_class_and_dynamic_overrides_without_replaying() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let probe = session
            .eval("(fn [] (suss.compiler/cell-defined? user/PublicationType))")
            .unwrap();
        let result = session.invoke(&probe, &[]).unwrap();
        boolean(&mut session, &result, false);
        session.eval("(deftype PublicationType [value])").unwrap();
        let result = session.invoke(&probe, &[]).unwrap();
        boolean(&mut session, &result, true);
        session
            .eval("(def ^:dynamic publication-dynamic nil)")
            .unwrap();
        let result = session.eval("(binding [publication-dynamic (aget (make-array 1) 0)] (suss.compiler/cell-defined? user/publication-dynamic))").unwrap();
        boolean(&mut session, &result, false);
        let result = session
            .eval("(suss.compiler/cell-defined? user/publication-dynamic)")
            .unwrap();
        boolean(&mut session, &result, true);
        session.eval("(def ^:dynamic publication-unbound)").unwrap();
        let result = session
            .eval("(suss.compiler/cell-defined? user/publication-unbound)")
            .unwrap();
        boolean(&mut session, &result, false);
        let result = session.eval("(binding [publication-unbound nil] (suss.compiler/cell-defined? user/publication-unbound))").unwrap();
        boolean(&mut session, &result, true);
        let result = session
            .eval("(suss.compiler/cell-defined? user/publication-unbound)")
            .unwrap();
        boolean(&mut session, &result, false);
        for source in [
            "(suss.compiler/cell-defined?)",
            "(suss.compiler/cell-defined? user/PublicationType user/PublicationType)",
            "(suss.compiler/cell-defined? PublicationType)",
            "(suss.compiler/cell-defined? (do (throw 73) user/PublicationType))",
            "(suss.compiler/cell-defined? missing.namespace/PublicationType)",
            "(suss.compiler/cell-defined? suss.core/+)",
        ] {
            assert!(session.eval(source).is_err(), "{source}");
            let result = session.invoke(&probe, &[]).unwrap();
            boolean(&mut session, &result, true);
        }
    }
}
