use suss_cli::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError},
};
use wasmtime::Val;
fn number(session: &mut Session, source: &str, macros: &mut CompiledMacros) -> u64 {
    let value = session.eval_with_macros(source, macros).unwrap();
    session
        .inspect(&value, |mut store, value| {
            let fields = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number");
            };
            Ok(*bits)
        })
        .unwrap()
}
#[test]
fn compiled_source_macros_expand_nested_calls_in_actual_lexical_analysis() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro add1 [x] (list '+ x 1))").unwrap();
    macros
        .define("(defmacro pair [& xs] (list '+ (first xs) (first (next xs))))")
        .unwrap();
    let mut runtime = Session::new_repl().unwrap();
    assert_eq!(
        number(&mut runtime, "(let [x 40] (add1 (add1 x)))", &mut macros),
        42f64.to_bits()
    );
    assert_eq!(
        number(&mut runtime, "(pair 20 22)", &mut macros),
        42f64.to_bits()
    );
    assert_eq!(
        number(
            &mut runtime,
            "(let [add1 (fn [x] (+ x 3))] (add1 39))",
            &mut macros
        ),
        42f64.to_bits()
    );
    assert!(matches!(
        runtime.eval("add1"),
        Err(SessionError::Compile(_))
    ));
}
#[test]
fn compiled_source_macros_pass_real_form_and_preserve_quote_and_failures() {
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define("(defmacro form-name [] (list 'quote (first &form)))")
        .unwrap();
    let mut runtime = Session::new_repl().unwrap();
    let value = runtime
        .eval_with_macros("(name (form-name))", &mut macros)
        .unwrap();
    let units = runtime
        .inspect(&value, |mut store, value| {
            Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap()
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(units, "form-name".encode_utf16().collect::<Vec<_>>());
    runtime
        .eval_with_macros("'(form-name)", &mut macros)
        .unwrap();
    macros.define("(defmacro bad [] (atom 1))").unwrap();
    let before = runtime.stats();
    let Err(SessionError::Compile(error)) =
        runtime.eval_with_macros("(def ghost 1) (bad)", &mut macros)
    else {
        panic!("Unknown macro result must fail");
    };
    assert_eq!(error.span, 14..19);
    assert_eq!(runtime.stats(), before);
    assert!(matches!(
        runtime.eval("ghost"),
        Err(SessionError::Compile(_))
    ));
    macros.define("(defmacro forever [] &form)").unwrap();
    assert!(matches!(
        runtime.eval_with_macros("(forever)", &mut macros),
        Err(SessionError::Compile(_))
    ));
    macros
        .define("(defmacro needs-env [] (count (get &env :locals)))")
        .unwrap();
    assert_eq!(
        number(&mut runtime, "(let [x 40 y 2] (needs-env))", &mut macros),
        2f64.to_bits(),
        "Implicit environment contains the actual caller's lexical bindings"
    );
    assert_eq!(
        number(&mut runtime, "(+ 20 22)", &mut macros),
        42f64.to_bits()
    );
}

#[test]
fn compiled_source_macros_expand_dependency_artifacts_before_publication() {
    use suss_cli::portable_session::SessionOptions;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("leaf.sus"),
        "(ns leaf) (def read (fn [] (user/add1 41)))",
    )
    .unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro add1 [x] (list '+ x 1))").unwrap();
    let mut runtime = Session::with_options(SessionOptions {
        source_paths: vec![root.path().to_owned()],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        number(
            &mut runtime,
            "(ns user (:require [leaf])) (leaf/read)",
            &mut macros
        ),
        42f64.to_bits()
    );
    macros.define("(defmacro add1 [x] (list '+ x 2))").unwrap();
    assert_eq!(
        number(&mut runtime, "(user/add1 40)", &mut macros),
        42f64.to_bits()
    );
    assert_eq!(
        number(&mut runtime, "(leaf/read)", &mut macros),
        42f64.to_bits(),
        "Existing artifact keeps its prior expansion"
    );
    assert!(macros.define("(defmacro add1 [x] missing)").is_err());
    assert_eq!(
        number(&mut runtime, "(user/add1 40)", &mut macros),
        42f64.to_bits()
    );
}

#[test]
fn compiled_source_macros_preserve_true_loop_special_form_priority() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro loop* [& xs] 99)").unwrap();
    let mut runtime = Session::new_repl().unwrap();
    assert_eq!(
        number(
            &mut runtime,
            "(loop* [n 0] (if (< n 42) (recur (+ n 1)) n))",
            &mut macros
        ),
        42f64.to_bits()
    );
    assert_eq!(
        number(&mut runtime, "(user/loop*)", &mut macros),
        99f64.to_bits(),
        "Qualified macro name remains an ordinary macro lookup"
    );
}
