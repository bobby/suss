use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError},
};
use suss_reader::forms::Kind;
use wasmtime::Val;
fn number(runtime: &mut Session, value: &suss_cli::portable_session::SessionValue) -> f64 {
    runtime
        .inspect(value, |mut store, value| {
            let data = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            let [Val::F64(bits)] = data.as_slice() else {
                panic!("Number")
            };
            Ok(f64::from_bits(*bits))
        })
        .unwrap()
}
#[test]
fn compiled_macro_indexed_data_returns_actual_variadic_arguments_as_code() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro call [& xs] xs)").unwrap();
    let mut runtime = Session::new_repl().unwrap();
    let value = runtime
        .eval_with_macros("(call + 20 22)", &mut macros)
        .unwrap();
    assert_eq!(number(&mut runtime, &value), 42.0);
}
#[test]
fn compiled_macro_indexed_data_decodes_array_string_and_cons_tails_after_gc() {
    let mut macros = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut macros).unwrap();
    for source in [
        "(seq (array '+ 20 22))",
        "(new suss.core/IndexedSeq (array 'ignored '+ 20 22) 1 nil)",
        "(cons '+ (next (seq (array 0 20 22))))",
    ] {
        let value = macros.eval(source).unwrap();
        macros.collect().unwrap();
        let form = bridge.read(&mut macros, &value, 101..110).unwrap();
        assert_eq!(form.span, 101..110);
        let mut runtime = Session::new_repl().unwrap();
        let value = runtime.eval_forms(vec![form], 100..120).unwrap();
        assert_eq!(number(&mut runtime, &value), 42.0);
    }
    let value = macros.eval("(seq \"a😀\")").unwrap();
    let form = bridge.read(&mut macros, &value, 20..30).unwrap();
    let Kind::List(items) = form.kind else {
        panic!("Sequence form")
    };
    let actual = items
        .into_iter()
        .map(|f| {
            let Kind::String(s) = f.kind else {
                panic!("UTF16 unit")
            };
            s
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, vec![vec![0x61], vec![0xd83d], vec![0xde00]]);
    let value = macros
        .eval("(new suss.core/IndexedSeq (array) 0 nil)")
        .unwrap();
    assert!(
        matches!(bridge.read(&mut macros, &value, 0..1).unwrap().kind, Kind::List(items) if items.is_empty())
    );
    macros.eval("(def saved-seq suss.core/IndexedSeq)").unwrap();
    macros.eval("(ns suss.core) (def IndexedSeq nil)").unwrap();
    macros.enter_namespace("user").unwrap();
    macros.collect().unwrap();
    let value = macros.eval("(new saved-seq (array 42) 0 nil)").unwrap();
    assert!(
        matches!(bridge.read(&mut macros, &value, 0..1).unwrap().kind, Kind::List(items) if items.len()==1)
    );
}
#[test]
fn compiled_macro_indexed_data_rejects_bad_storage_indices_metadata_and_limits() {
    let mut macros = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut macros).unwrap();
    for source in [
        "(new suss.core/IndexedSeq (array 1) -1 nil)",
        "(new suss.core/IndexedSeq (array 1) 0.5 nil)",
        "(new suss.core/IndexedSeq (array 1) ##NaN nil)",
        "(new suss.core/IndexedSeq (array 1) 2 nil)",
        "(new suss.core/IndexedSeq (array 1) nil nil)",
        "(new suss.core/IndexedSeq (list 1) 0 nil)",
        "(new suss.core/IndexedSeq (array 1) 0 42)",
        "(new suss.core/IndexedSeq (make-array 4096) 0 nil)",
        "(let [a (array nil)] (let [s (new suss.core/IndexedSeq a 0 nil)] (aset a 0 s) s))",
    ] {
        let value = macros.eval(source).unwrap();
        let Err(SessionError::Compile(error)) = bridge.read(&mut macros, &value, 91..99) else {
            panic!("Reject {source}")
        };
        assert_eq!(error.span, 91..99);
    }
    macros.eval("(deftype Spoof [arr i meta])").unwrap();
    let value = macros.eval("(new Spoof (array 1) 0 nil)").unwrap();
    assert!(matches!(
        bridge.read(&mut macros, &value, 0..1),
        Err(SessionError::Compile(_))
    ));
    let value = macros
        .eval("(new suss.core/IndexedSeq (make-array 4094) 0 nil)")
        .unwrap();
    assert!(
        matches!(bridge.read(&mut macros, &value, 0..1).unwrap().kind, Kind::List(items) if items.len()==4094)
    );
    let value = macros
        .eval("(new suss.core/IndexedSeq (make-array 4095) 0 nil)")
        .unwrap();
    assert!(matches!(
        bridge.read(&mut macros, &value, 0..1),
        Err(SessionError::Compile(_))
    ));
    let value = macros.eval("(seq (array 42))").unwrap();
    assert!(bridge.read(&mut macros, &value, 0..1).is_ok());
}
