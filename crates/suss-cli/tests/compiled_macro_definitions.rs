use suss_cli::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError},
};
use wasmtime::Val;
fn number(runtime: &mut Session, source: &str, macros: &mut CompiledMacros) -> f64 {
    let value = runtime.eval_with_macros(source, macros).unwrap();
    runtime
        .inspect(&value, |mut store, value| {
            let fields = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number")
            };
            Ok(f64::from_bits(*bits))
        })
        .unwrap()
}
#[test]
fn compiled_macro_definitions_dispatch_all_signatures_and_keep_prior_definition_on_failure() {
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define("(defmacro choose \"two signatures\" {:added \"1\"} ([] 42) ([x] (list '+ x 2)))")
        .unwrap();
    assert!(macros
        .define("(defmacro private-one {:private true} [] 1)")
        .is_err());
    let mut runtime = Session::new_repl().unwrap();
    assert_eq!(number(&mut runtime, "(choose)", &mut macros), 42.0);
    assert_eq!(number(&mut runtime, "(choose 40)", &mut macros), 42.0);
    assert!(matches!(
        runtime.eval_with_macros("(choose 1 2)", &mut macros),
        Err(SessionError::Compile(_))
    ));
    assert!(macros
        .define("(defmacro choose ([] 1) ([x] absent))")
        .is_err());
    assert_eq!(number(&mut runtime, "(choose)", &mut macros), 42.0);
    assert_eq!(number(&mut runtime, "(choose 40)", &mut macros), 42.0);
    macros
        .define("(defmacro choose ([x] x) ([x & xs] (list '+ x (first xs))) {:added \"1\"})")
        .unwrap();
    assert_eq!(number(&mut runtime, "(choose 42)", &mut macros), 42.0);
    assert_eq!(number(&mut runtime, "(choose 20 22)", &mut macros), 42.0);
    assert!(macros
        .define("(defmacro choose \"doc\" {:private true})")
        .is_err());
    assert_eq!(number(&mut runtime, "(choose 20 22)", &mut macros), 42.0);
}
#[test]
fn compiled_macro_definitions_namespace_registration_keeps_owned_roots() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro answer [] 42)").unwrap();
    macros.enter_namespace("other").unwrap();
    assert_eq!(macros.current_namespace(), "other");
    macros.define("(defmacro answer [] 40)").unwrap();
    let mut runtime = Session::new_repl().unwrap();
    assert_eq!(number(&mut runtime, "(user/answer)", &mut macros), 42.0);
    assert_eq!(
        number(&mut runtime, "(+ (other/answer) 2)", &mut macros),
        42.0
    );
    macros.enter_namespace("user").unwrap();
    assert_eq!(number(&mut runtime, "(answer)", &mut macros), 42.0);
    assert!(macros.enter_namespace("invalid/name").is_err());
    assert_eq!(macros.current_namespace(), "user");
    assert_eq!(number(&mut runtime, "(answer)", &mut macros), 42.0);
}

#[test]
fn compiled_macro_definitions_expand_macro_bodies_in_the_same_phase_store() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro two ([] 2))").unwrap();
    macros
        .define("(defmacro add-two [x] (list '+ x (two)))")
        .unwrap();
    let mut runtime = Session::new_repl().unwrap();
    assert_eq!(number(&mut runtime, "(add-two 40)", &mut macros), 42.0);
    macros.define("(defmacro two [] 3)").unwrap();
    assert_eq!(
        number(&mut runtime, "(add-two 40)", &mut macros),
        42.0,
        "Already compiled macro body keeps its expansion"
    );
    macros.enter_namespace("other").unwrap();
    macros
        .define("(defmacro add-three [x] (list '+ x (user/two)))")
        .unwrap();
    assert_eq!(
        number(&mut runtime, "(other/add-three 39)", &mut macros),
        42.0
    );
    macros
        .define("(defmacro local [] (let [two (fn [] 42)] (two)))")
        .unwrap();
    assert_eq!(number(&mut runtime, "(other/local)", &mut macros), 42.0);
    assert!(macros
        .define("(defmacro add-three [x] (unresolved x))")
        .is_err());
    assert_eq!(
        number(&mut runtime, "(other/add-three 39)", &mut macros),
        42.0
    );
}
