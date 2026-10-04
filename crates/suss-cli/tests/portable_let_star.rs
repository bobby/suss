//! Primitive binding syntax emitted by portable source macros must execute.
//! Semantic reference: pinned cljs/analyzer.cljc parse let*2594 (EPL-1.0).
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::Kind;

#[test]
fn primitive_let_star_executes_directly_and_from_compiled_macros_in_both_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                "(defmacro primitive-copy [value] (list 'let* ['copy value] (list '+ 'copy 1)))",
            )
            .unwrap();
        macros
            .define("(defmacro primitive-hygienic [value] `(let* [copy# ~value] (+ copy# 1)))")
            .unwrap();
        for source in [
            "(let* [base 40 copy (+ base 2)] copy)",
            "(primitive-copy 41)",
            "(primitive-hygienic 41)",
        ] {
            let value = session.eval_with_macros(source, &mut macros).unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            session.collect().unwrap();
            let actual = bridge.read(&mut session, &value, 0..source.len()).unwrap();
            assert!(
                matches!(actual.kind, Kind::Number(value) if value.to_bits() == 42.0_f64.to_bits()),
                "{source}: {actual:?}"
            );
        }
        let source = "(do (def primitive-visits 0) (let* [base (do (set! primitive-visits (+ (* primitive-visits 10) 1)) 40) copy (do (set! primitive-visits (+ (* primitive-visits 10) 2)) (+ base 2))] copy))";
        let value = session.eval_with_macros(source, &mut macros).unwrap();
        let visits = session
            .eval_with_macros("primitive-visits", &mut macros)
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        for (value, expected) in [(&value, 42.0_f64), (&visits, 12.0_f64)] {
            let actual = bridge.read(&mut session, value, 0..source.len()).unwrap();
            assert!(
                matches!(actual.kind, Kind::Number(value) if value.to_bits() == expected.to_bits()),
                "ordered primitive bindings: {actual:?}, expected {expected}"
            );
        }
    }
}
