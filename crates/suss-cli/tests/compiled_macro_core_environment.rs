//! The full retained core namespace must fit the bounded analysis graph.
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::Kind;

#[test]
fn compiled_macro_core_namespace_keeps_complete_declaration_records() {
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(
                r#"(defmacro declaration-facts [name]
          (let [info (get (get (get &env :ns) :defs) name)]
            (list 'quote [(get info :name)
                          (get info :suss/analysis-completed)
                          (get info :suss/defonce)])))"#,
            )
            .unwrap();
        caller.enter_namespace("cljs.core").unwrap();
        let value = caller
            .eval_with_macros("(user/declaration-facts identity)", &mut macros)
            .unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        caller.collect().unwrap();
        let Kind::Vector(items) = bridge.read(&mut caller, &value, 0..1).unwrap().kind else {
            panic!("Expected declaration projection");
        };
        assert_eq!(items.len(), 3);
        assert!(matches!(&items[0].kind, Kind::Symbol(symbol)
            if symbol.namespace.as_deref() == Some("suss.core") && symbol.name == "identity"));
        assert_eq!(items[1].kind, Kind::Bool(true));
        assert_eq!(items[2].kind, Kind::Bool(false));
        caller.eval("(defonce graph-owned 42)").unwrap();
        let value = caller
            .eval_with_macros("(user/declaration-facts graph-owned)", &mut macros)
            .unwrap();
        caller.collect().unwrap();
        let Kind::Vector(items) = bridge.read(&mut caller, &value, 0..1).unwrap().kind else {
            panic!("Expected fresh declaration projection");
        };
        assert_eq!(items[1].kind, Kind::Bool(true));
        assert_eq!(items[2].kind, Kind::Bool(true));
    }
}
