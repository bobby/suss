use suss_cli::portable_session::{Session, SessionError, SessionValue};
use suss_compile::portable::resolve::Phase;
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Expected Number layout");
            };
            Ok(*bits)
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}

#[test]
fn compiled_phase_session_uses_separate_store_cells_core_and_roots() {
    let mut runtime = Session::new_repl().unwrap();
    let mut macros = Session::new_macro().unwrap();
    assert_eq!(runtime.phase(), Phase::Runtime);
    assert_eq!(macros.phase(), Phase::Macro);
    runtime
        .eval("(ns shared) (def value 17) (def a (atom 1))")
        .unwrap();
    macros
        .eval("(ns shared) (def value 23) (def a (atom 2)) (def old (fn [] value))")
        .unwrap();
    let closure = macros.eval("old").unwrap();
    assert!(matches!(
        runtime.invoke(&closure, &[]),
        Err(SessionError::ForeignValue)
    ));
    assert!(matches!(
        runtime.inspect(&closure, |_, _| Ok(())),
        Err(SessionError::ForeignValue)
    ));
    runtime.eval("(reset! a 31)").unwrap();
    assert_eq!(eval_number(&mut macros, "@a"), 2f64.to_bits());
    macros.eval("(def value 29) (reset! a 37)").unwrap();
    macros.collect().unwrap();
    let result = macros.invoke(&closure, &[]).unwrap();
    assert_eq!(number(&mut macros, &result), 29f64.to_bits());
    assert_eq!(eval_number(&mut runtime, "value"), 17f64.to_bits());
    assert_eq!(eval_number(&mut runtime, "@a"), 31f64.to_bits());
    let name = macros.eval("(name (first '(macro/data)))").unwrap();
    let units = macros
        .inspect(&name, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(units, "data".encode_utf16().collect::<Vec<_>>());
    macros.reset().unwrap();
    assert_eq!(macros.phase(), Phase::Macro);
    assert_eq!(macros.current_namespace(), "user");
    assert!(matches!(
        macros.invoke(&closure, &[]),
        Err(SessionError::ForeignValue)
    ));
    assert!(matches!(
        macros.eval("shared/value"),
        Err(SessionError::Compile(_))
    ));
    assert_eq!(eval_number(&mut macros, "@(atom 41)"), 41f64.to_bits());
    assert_eq!(eval_number(&mut runtime, "@a"), 31f64.to_bits());
}

#[test]
fn compiled_phase_session_load_reload_failure_and_reset_preserve_phase() {
    use suss_cli::portable_session::SessionOptions;
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("phase")).unwrap();
    let dependency = root.path().join("phase/dep.sus");
    let target = root.path().join("phase/target.sus");
    std::fs::write(&dependency, "(ns phase.dep) (def value 7)").unwrap();
    std::fs::write(
        &target,
        "(ns phase.target (:require [phase.dep :as d])) (def read (fn [] d/value))",
    )
    .unwrap();
    let options = SessionOptions {
        source_paths: vec![root.path().to_owned()],
        ..Default::default()
    };
    let mut macros = Session::with_options_in(options.clone(), Phase::Macro).unwrap();
    let mut runtime = Session::with_options(options).unwrap();
    macros.load_namespace("phase.target").unwrap();
    runtime.load_namespace("phase.target").unwrap();
    assert_eq!(macros.current_namespace(), "user");
    let captured = macros.eval("phase.target/read").unwrap();
    std::fs::write(&dependency, "(ns phase.dep) (def value 11)").unwrap();
    macros.reload_namespace("phase.target", true).unwrap();
    assert_eq!(
        eval_number(&mut macros, "(phase.target/read)"),
        11f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut runtime, "(phase.target/read)"),
        7f64.to_bits()
    );
    macros.collect().unwrap();
    let result = macros.invoke(&captured, &[]).unwrap();
    assert_eq!(number(&mut macros, &result), 11f64.to_bits());
    let before = macros.stats();
    std::fs::write(&dependency, "(ns phase.dep) (def value absent)").unwrap();
    assert!(matches!(
        macros.reload_namespace("phase.target", true),
        Err(SessionError::Module(_))
    ));
    assert_eq!(macros.stats(), before);
    assert_eq!(
        eval_number(&mut macros, "(phase.target/read)"),
        11f64.to_bits()
    );
    std::fs::write(&dependency, "(ns phase.dep) (def value (throw 19))").unwrap();
    assert!(matches!(
        macros.reload_namespace("phase.target", true),
        Err(SessionError::Language(_))
    ));
    assert_eq!(
        eval_number(&mut macros, "(phase.target/read)"),
        11f64.to_bits()
    );
    std::fs::write(&dependency, "(ns phase.dep) (def value 23)").unwrap();
    macros.load_namespace("phase.target").unwrap();
    assert_eq!(
        eval_number(&mut macros, "(phase.target/read)"),
        23f64.to_bits()
    );
    macros.reset().unwrap();
    assert_eq!(macros.phase(), Phase::Macro);
    assert_eq!(macros.stats().loaded_modules, 0);
    assert!(matches!(
        macros.invoke(&captured, &[]),
        Err(SessionError::ForeignValue)
    ));
    assert_eq!(
        eval_number(&mut runtime, "(phase.target/read)"),
        7f64.to_bits()
    );
}

#[test]
fn compiled_phase_session_invokes_compiled_transformer_and_failed_reset_keeps_state() {
    let mut macros = Session::new_macro().unwrap();
    macros
        .eval("(def count 0) (def expand (fn [x] (set! count (+ count 1)) (list '+ x 1)))")
        .unwrap();
    let transformer = macros.eval("expand").unwrap();
    let argument = macros.eval("'x").unwrap();
    let expansion = macros.invoke(&transformer, &[&argument]).unwrap();
    macros.collect().unwrap();
    let name = macros.eval("(fn [form] (name (first form)))").unwrap();
    let operator = macros.invoke(&name, &[&expansion]).unwrap();
    let units = macros
        .inspect(&operator, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(units, vec![u16::from(b'+')]);
    let constant = macros
        .eval("(fn [form] (first (next (next form))))")
        .unwrap();
    let value = macros.invoke(&constant, &[&expansion]).unwrap();
    assert_eq!(number(&mut macros, &value), 1f64.to_bits());
    assert_eq!(eval_number(&mut macros, "count"), 1f64.to_bits());
    macros.set_operation_fuel(1);
    let before = macros.stats();
    assert!(macros.reset().is_err());
    assert_eq!(macros.phase(), Phase::Macro);
    assert_eq!(macros.stats(), before);
    macros.set_operation_fuel(10_000_000);
    assert_eq!(eval_number(&mut macros, "count"), 1f64.to_bits());
    let another = macros.invoke(&transformer, &[&argument]).unwrap();
    let value = macros.invoke(&constant, &[&another]).unwrap();
    assert_eq!(number(&mut macros, &value), 1f64.to_bits());
    assert_eq!(eval_number(&mut macros, "count"), 2f64.to_bits());
}
