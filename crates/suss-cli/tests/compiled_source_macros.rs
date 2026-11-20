#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;

use suss_cli::{
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError},
};

/// An exact runtime number: a long, or a double by its bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Number {
    Long(i64),
    Double(u64),
}
/// The expected number for `value`: integral values are longs, as in Clojure.
fn num(value: f64) -> Number {
    if value.is_finite()
        && value.fract() == 0.0
        && !(value == 0.0 && value.is_sign_negative())
        && value.abs() < 9_223_372_036_854_775_808.0
    {
        Number::Long(value as i64)
    } else {
        Number::Double(value.to_bits())
    }
}
fn exact_number(store: impl wasmtime::AsContextMut, value: &wasmtime::Val) -> wasmtime::Result<Number> {
    Ok(
        match suss_compile::portable_session::runtime_number(store, value)?
            .expect("expected a runtime number")
        {
            suss_compile::portable_session::RuntimeNumber::Long(value) => Number::Long(value),
            suss_compile::portable_session::RuntimeNumber::Double(value) => {
                Number::Double(value.to_bits())
            }
        },
    )
}
fn number(session: &mut Session, source: &str, macros: &mut CompiledMacros) -> Number {
    let value = session.eval_with_macros(source, macros).unwrap();
    session
        .inspect(&value, |mut store, value| {
            exact_number(&mut store, &value)
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
        num(42.0)
    );
    assert_eq!(
        number(&mut runtime, "(pair 20 22)", &mut macros),
        num(42.0)
    );
    assert_eq!(
        number(
            &mut runtime,
            "(let [add1 (fn [x] (+ x 3))] (add1 39))",
            &mut macros
        ),
        num(42.0)
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
        num(2.0),
        "Implicit environment contains the actual caller's lexical bindings"
    );
    assert_eq!(
        number(&mut runtime, "(+ 20 22)", &mut macros),
        num(42.0)
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
        num(42.0)
    );
    macros.define("(defmacro add1 [x] (list '+ x 2))").unwrap();
    assert_eq!(
        number(&mut runtime, "(user/add1 40)", &mut macros),
        num(42.0)
    );
    assert_eq!(
        number(&mut runtime, "(leaf/read)", &mut macros),
        num(42.0),
        "Existing artifact keeps its prior expansion"
    );
    assert!(macros.define("(defmacro add1 [x] missing)").is_err());
    assert_eq!(
        number(&mut runtime, "(user/add1 40)", &mut macros),
        num(42.0)
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
        num(42.0)
    );
    assert_eq!(
        number(&mut runtime, "(user/loop*)", &mut macros),
        num(99.0),
        "Qualified macro name remains an ordinary macro lookup"
    );
}
#[test]
fn compiled_source_macros_materialize_env_only_for_macros_that_can_read_it() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
    // No &env reference: expansion never needs the caller's analysis graph.
    macros.define("(defmacro plain [x] (list '+ x 1))").unwrap();
    // A direct reference observes a genuine environment map.
    macros
        .define("(defmacro direct [] (if (map? &env) 1 0))")
        .unwrap();
    // A source macro expanded inside a body may supply the reference, so a body
    // calling one is conservatively treated as reading &env.
    macros.define("(defmacro env-ref [] '&env)").unwrap();
    macros
        .define("(defmacro indirect [] (if (map? (env-ref)) 1 0))")
        .unwrap();
    // Compiling indirect's body expanded env-ref, which reads &env: that one
    // materialization happened in the macro session. Count runtime uses after it.
    let defined = macros.environment_materializations();
    assert_eq!(defined, 1);
    let mut runtime = Session::new_repl().unwrap();
    assert_eq!(number(&mut runtime, "(plain 41)", &mut macros), num(42.0));
    assert_eq!(macros.environment_materializations(), defined);
    assert_eq!(number(&mut runtime, "(direct)", &mut macros), num(1.0));
    assert_eq!(macros.environment_materializations(), defined + 1);
    assert_eq!(number(&mut runtime, "(indirect)", &mut macros), num(1.0));
    assert_eq!(macros.environment_materializations(), defined + 2);
    // The compiled defn bootstrap macro cannot read &env: definitions in a
    // namespace no longer rebuild the growing caller environment per form.
    let definitions = (0..32)
        .map(|i| format!("(defn f{i} [x] (+ x {i}))"))
        .collect::<Vec<_>>()
        .join(" ");
    runtime
        .eval_with_macros(&format!("(do {definitions} nil)"), &mut macros)
        .unwrap();
    assert_eq!(macros.environment_materializations(), defined + 2);
    assert_eq!(number(&mut runtime, "(f31 11)", &mut macros), num(42.0));
}
#[test]
fn compiled_source_macros_pass_env_through_renamed_and_rewritten_helper_expansions() {
    // A helper macro yields the &env reference. Each caller reaches it in a way
    // a textual scan of the caller cannot see: a renamed refer, an alias, or a
    // threading step that the compiler rewrites into a call.
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("helper.sus"), "(ns helper) (defmacro env-ref [& _] '&env)").unwrap();
    std::fs::write(
        root.path().join("tools.sus"),
        "(ns tools (:require [helper :as h :refer [env-ref] :rename {env-ref e}]))\n\
         (defmacro direct [] (if (map? &env) 1 0))\n\
         (defmacro aliased [] (if (map? (h/env-ref)) 1 0))\n\
         (defmacro renamed [] (if (map? (e)) 1 0))\n\
         (defmacro threaded [] (if (map? (-> 1 h/env-ref)) 1 0))\n",
    )
    .unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
    let mut runtime = suss_cli::portable_session::Session::with_options(
        suss_cli::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()],
            ..Default::default()
        },
    )
    .unwrap();
    runtime
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    runtime
        .eval_with_macros("(ns user (:require-macros [tools :as t]))", &mut macros)
        .unwrap();
    for name in ["direct", "aliased", "renamed", "threaded"] {
        assert_eq!(
            number(&mut runtime, &format!("(t/{name})"), &mut macros),
            num(1.0),
            "{name} must receive a genuine &env"
        );
    }
    // The same rewrite in a REPL definition.
    macros.define("(defmacro env-ref2 [& _] '&env)").unwrap();
    macros
        .define("(defmacro threaded2 [] (if (map? (-> 1 env-ref2)) 1 0))")
        .unwrap();
    assert_eq!(number(&mut runtime, "(threaded2)", &mut macros), num(1.0));
}
