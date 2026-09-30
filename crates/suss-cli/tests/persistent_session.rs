use suss_cli::portable_session::{Session, SessionError, SessionValue};
use wasmtime::Val;

fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("expected exact Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
}
fn eval_number(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}
fn units(session: &mut Session, value: &SessionValue) -> Vec<u16> {
    session
        .inspect(value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect())
        })
        .unwrap()
}

#[test]
fn persistent_session_arithmetic_coerces_live_cells_and_evaluates_operands_once() {
    let mut session = Session::new().unwrap();
    session
        .eval("(def counter 0) (def next (fn [] (def counter (+ counter 1))))")
        .unwrap();
    assert_eq!(
        eval_number(&mut session, "(+ (next) (* (next) 10) (next))"),
        24.0f64.to_bits()
    );
    assert_eq!(eval_number(&mut session, "counter"), 3.0f64.to_bits());
    session
        .eval("(def value 3) (def read (fn [] (+ value 1)))")
        .unwrap();
    assert_eq!(eval_number(&mut session, "(read)"), 4.0f64.to_bits());
    session.eval("(def value \"4\")").unwrap();
    let text = session.eval("(read)").unwrap();
    session.collect().unwrap();
    assert_eq!(units(&mut session, &text), [52, 49]);
    assert_eq!(
        eval_number(&mut session, "((fn [x] (* x 3)) \"2\")"),
        6.0f64.to_bits()
    );
    assert_eq!(eval_number(&mut session, "(- nil)"), (-0.0f64).to_bits());
    session.eval("(def old 7) (def object (fn [] 1))").unwrap();
    let error = session
        .eval("(def old (do (next) (+ object 1)))")
        .unwrap_err();
    let SessionError::Language(payload) = error else {
        panic!("expected explicit unsupported object coercion")
    };
    let message = session
        .inspect(&payload, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            assert_eq!(fields.len(), 4);
            let descriptor = fields[0]
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            assert_eq!(descriptor.field(&mut store, 0)?.unwrap_i64(), 5);
            let message = fields[1]
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            Ok(message
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(
        String::from_utf16(&message).unwrap(),
        "Unsupported arithmetic object coercion"
    );
    assert_eq!(eval_number(&mut session, "old"), 7.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "counter"), 4.0f64.to_bits());
}

#[test]
fn session_lifecycle_numeric_memory_has_bounded_high_water_capacity_and_reset() {
    let mut session = Session::new().unwrap();
    let initial = session.stats().numeric_memory_capacity;
    let text = format!("0.{}1", "0".repeat(40000));
    session.eval(&format!("(def text {text:?})")).unwrap();
    assert_eq!(eval_number(&mut session, "(* text 1)"), 0.0f64.to_bits());
    let capacity = session.stats().numeric_memory_capacity;
    assert!(capacity > initial);
    for _ in 0..20 {
        assert_eq!(eval_number(&mut session, "(* text 1)"), 0.0f64.to_bits());
    }
    session.collect().unwrap();
    assert_eq!(session.stats().numeric_memory_capacity, capacity);
    let value = session.eval("text").unwrap();
    assert_eq!(
        units(&mut session, &value),
        text.encode_utf16().collect::<Vec<_>>()
    );
    session.set_operation_fuel(500);
    assert!(matches!(
        session.eval("(* text 1)"),
        Err(SessionError::Trap(_))
    ));
    session.set_operation_fuel(100000);
    let output = session.eval("(+ 42 \"\")").unwrap();
    assert_eq!(units(&mut session, &output), [52, 50]);
    session.reset().unwrap();
    assert_eq!(session.stats().numeric_memory_capacity, initial);
    assert_eq!(session.stats().external_value_handles, 0);
}

#[test]
fn persistent_session_unary_arithmetic_retains_dynamic_values_and_old_closures() {
    let mut session = Session::new().unwrap();
    session
        .eval("(def effect 0) (def f (let [x 7] (fn [] x))) (def identity (fn [x] (* (+ x))))")
        .unwrap();
    let old = session.eval("(+ (do (def effect 1) (* f)))").unwrap();
    session.eval("(def f (fn [] 9))").unwrap();
    session.collect().unwrap();
    let returned = session.invoke(&old, &[]).unwrap();
    assert_eq!(number(&mut session, &returned), 7.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "effect"), 1.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "((* f))"), 9.0f64.to_bits());
    for (source, sentinel) in [
        ("(identity nil)", 0),
        ("(identity false)", 2),
        ("(identity true)", 4),
    ] {
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        let actual = session
            .inspect(&value, |store, value| {
                Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32())
            })
            .unwrap();
        assert_eq!(actual, sentinel);
    }
    let value = session.eval("(identity \"\\uD800😀\")").unwrap();
    session.collect().unwrap();
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(units, [0xd800, 0xd83d, 0xde00]);
}
#[test]
fn persistent_session_initializers_execute_once_and_defonce_skips_effects() {
    let mut session = Session::new().unwrap();
    session
        .eval("(def counter 1) (def next (fn [] (def counter 2))) (def value (next))")
        .unwrap();
    assert_eq!(eval_number(&mut session, "counter"), 2.0f64.to_bits());
    session.eval("(def counter 9)").unwrap();
    session.eval("(+ 20 22)").unwrap();
    session.eval("(defonce value (next))").unwrap();
    assert_eq!(eval_number(&mut session, "counter"), 9.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "value"), 2.0f64.to_bits());
}

#[test]
fn persistent_session_old_closures_and_owned_values_survive_rebinding_and_gc() {
    let mut session = Session::new().unwrap();
    session.eval("(def f (let [x 1] (fn [y] x)))").unwrap();
    let old = session.eval("f").unwrap();
    let string = session.eval("\"\\uD800😀\"").unwrap();
    let arg = session.eval("7").unwrap();
    session.eval("(def f (fn [y] 10))").unwrap();
    for _ in 0..20 {
        session.eval("123").unwrap();
    }
    session.collect().unwrap();
    let result = session.invoke(&old, &[&arg]).unwrap();
    assert_eq!(number(&mut session, &result), 1.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "(f 7)"), 10.0f64.to_bits());
    let units = session
        .inspect(&string, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(units, [0xd800, 0xd83d, 0xde00]);
}
#[test]
fn persistent_session_compile_failure_is_atomic_and_language_failure_recovers() {
    let mut session = Session::new().unwrap();
    session.eval("(def old 7) (def effect 1)").unwrap();
    let before = session.stats();
    assert!(matches!(
        session.eval("(def ghost 8) unresolved"),
        Err(SessionError::Compile(_))
    ));
    assert_eq!(session.stats(), before);
    assert!(matches!(
        session.eval("ghost"),
        Err(SessionError::Compile(_))
    ));
    let error = session
        .eval("(def old (do (def effect 9) (1)))")
        .unwrap_err();
    let SessionError::Language(payload) = error else {
        panic!("expected language exception")
    };
    session.collect().unwrap();
    let descriptor = session
        .inspect(&payload, |mut store, value| {
            let exception = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = exception.fields(&mut store)?.collect::<Vec<_>>();
            assert_eq!(fields.len(), 4);
            let descriptor = fields[0]
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            Ok(descriptor.field(&mut store, 0)?.unwrap_i64())
        })
        .unwrap();
    assert_eq!(descriptor, 3);
    session.collect().unwrap();
    assert_eq!(eval_number(&mut session, "old"), 7.0f64.to_bits());
    assert_eq!(eval_number(&mut session, "effect"), 9.0f64.to_bits());
}

#[test]
fn session_lifecycle_reset_rejects_old_values_and_distinguishes_code_from_roots() {
    let mut session = Session::new().unwrap();
    let bootstrap_cells = session.stats().binding_cells;
    assert_eq!(
        bootstrap_cells, 4,
        "canonical arithmetic cells are resident"
    );
    let value = session.eval("(def old 7) old").unwrap();
    let clone = value.clone();
    assert_eq!(session.stats().external_value_handles, 2);
    drop(clone);
    assert_eq!(session.stats().external_value_handles, 1);
    let before = session.stats();
    session.collect().unwrap();
    assert_eq!(
        session.stats().resident_fragments,
        before.resident_fragments
    );
    session.reset().unwrap();
    assert_eq!(session.stats().resident_fragments, 0);
    assert_eq!(session.stats().binding_cells, bootstrap_cells);
    assert_eq!(session.stats().external_value_handles, 0);
    assert!(matches!(
        session.inspect(&value, |_, _| Ok(())),
        Err(SessionError::ForeignValue)
    ));
    assert!(matches!(session.eval("old"), Err(SessionError::Compile(_))));
    assert_eq!(eval_number(&mut session, "42"), 42.0f64.to_bits());
}

fn sources() -> (tempfile::TempDir, Session) {
    let root = tempfile::tempdir().unwrap();
    let session = Session::with_options(suss_cli::portable_session::SessionOptions {
        source_paths: vec![root.path().into()],
        ..Default::default()
    })
    .unwrap();
    (root, session)
}

#[test]
fn persistent_session_input_dependencies_compile_atomically_and_initialize_once() {
    let (root, mut session) = sources();
    std::fs::write(
        root.path().join("dependency.sus"),
        "(ns dependency) (def value 1)",
    )
    .unwrap();
    let before = session.stats();
    assert!(matches!(
        session.eval("(ns app (:require [dependency :as d])) (def ghost unresolved)"),
        Err(SessionError::Compile(_))
    ));
    assert_eq!(session.stats(), before);
    assert_eq!(session.current_namespace(), "user");
    assert!(matches!(
        session.eval("dependency/value"),
        Err(SessionError::Compile(_))
    ));
    assert_eq!(
        eval_number(
            &mut session,
            "(ns app (:require [dependency :as d])) (def value d/value) value"
        ),
        1.0f64.to_bits()
    );
    assert_eq!(session.current_namespace(), "app");
    assert_eq!(session.stats().loaded_modules, 1);
    std::fs::write(root.path().join("dependency.sus"), "no longer valid source").unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(ns app (:require [dependency :as d])) d/value"
        ),
        1.0f64.to_bits()
    );
    assert_eq!(eval_number(&mut session, "value"), 1.0f64.to_bits());
    assert_eq!(session.stats().loaded_modules, 1);
}

#[test]
fn persistent_session_failed_module_retries_without_replaying_successful_dependencies() {
    let (root, mut session) = sources();
    std::fs::write(
        root.path().join("dependency.sus"),
        "(ns dependency) (def value 1)",
    )
    .unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        "(ns app (:require dependency)) (def value 2) (def value (1))",
    )
    .unwrap();
    assert!(matches!(
        session.load_namespace("app"),
        Err(SessionError::Language(_))
    ));
    assert_eq!(session.current_namespace(), "user");
    assert_eq!(session.stats().loaded_modules, 1);
    assert_eq!(eval_number(&mut session, "app/value"), 2.0f64.to_bits());
    std::fs::write(root.path().join("dependency.sus"), "must never replay this").unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        "(ns app (:require dependency)) (def value 3) value",
    )
    .unwrap();
    let value = session.load_namespace("app").unwrap().unwrap();
    assert_eq!(number(&mut session, &value), 3.0f64.to_bits());
    assert_eq!(session.current_namespace(), "user");
    assert_eq!(session.stats().loaded_modules, 2);
    assert!(session.load_namespace("app").unwrap().is_none());
}

#[test]
fn persistent_session_catalog_scopes_cannot_skip_loading_and_core_alias_is_canonical() {
    let (_, mut session) = sources();
    session.eval("(ns missing)").unwrap();
    let error = session.eval("(ns app (:require missing))").unwrap_err();
    let SessionError::Module(error) = error else {
        panic!("expected missing source")
    };
    assert!(error.message.contains("No source"));
    assert_eq!(error.source_path, None);
    assert_eq!(&"(ns app (:require missing))"[error.span], "missing");
    assert_eq!(session.current_namespace(), "missing");
    assert_eq!(
        eval_number(
            &mut session,
            "(ns app (:require [cljs.core :as core])) (core/+ 1 2)"
        ),
        3.0f64.to_bits()
    );
    assert_eq!(session.stats().loaded_modules, 0);
}

#[test]
fn persistent_session_foreign_values_are_rejected_before_runtime_use() {
    let mut left = Session::new().unwrap();
    let mut right = Session::new().unwrap();
    let function = left.eval("(fn [x] x)").unwrap();
    let foreign = right.eval("7").unwrap();
    let before = left.stats();
    assert!(matches!(
        left.invoke(&function, &[&foreign]),
        Err(SessionError::ForeignValue)
    ));
    assert_eq!(left.stats(), before);
    assert!(matches!(
        right.invoke(&function, &[]),
        Err(SessionError::ForeignValue)
    ));
}

#[test]
fn session_lifecycle_fuel_trap_is_distinct_and_the_next_input_recovers() {
    let mut session = Session::new().unwrap();
    session.eval("(def spin (fn [] (spin)))").unwrap();
    session.set_operation_fuel(500);
    let SessionError::Trap(error) = session.eval("(spin)").unwrap_err() else {
        panic!("expected fuel trap")
    };
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&wasmtime::Trap::OutOfFuel)
    );
    session.set_operation_fuel(10_000);
    assert_eq!(eval_number(&mut session, "42"), 42.0f64.to_bits());
    assert_eq!(session.stats().external_value_handles, 0);
}

#[test]
fn persistent_session_diamond_effects_follow_reverse_require_order() {
    let (root, mut session) = sources();
    for (name, source) in [
        (
            "shared",
            "(ns shared) (def value 0) (def record (fn [x] (def value x)))",
        ),
        (
            "right",
            "(ns right (:require [shared :as s])) (def value (s/record 3))",
        ),
        (
            "left",
            "(ns left (:require [shared :as s])) (def value (s/record 2))",
        ),
    ] {
        std::fs::write(root.path().join(format!("{name}.sus")), source).unwrap();
    }
    assert_eq!(
        eval_number(
            &mut session,
            "(ns app (:require right left [shared :as s])) s/value"
        ),
        2.0f64.to_bits()
    );
    assert_eq!(session.stats().loaded_modules, 3);
    assert_eq!(session.stats().resident_fragments, 4);
    assert_eq!(eval_number(&mut session, "s/value"), 2.0f64.to_bits());
}

#[test]
fn persistent_session_nil_and_false_are_initialized_defonce_bindings() {
    let mut session = Session::new().unwrap();
    session
        .eval("(def touched 7) (defonce held nil) (defonce other false)")
        .unwrap();
    session
        .eval("(defonce held (def touched 9)) (defonce other (def touched 10))")
        .unwrap();
    assert_eq!(eval_number(&mut session, "touched"), 7.0f64.to_bits());
    for (source, expected) in [("held", 0), ("other", 2)] {
        let value = session.eval(source).unwrap();
        let tag = session
            .inspect(&value, |store, value| {
                Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_i32())
            })
            .unwrap();
        assert_eq!(tag, expected);
    }
}

#[test]
fn persistent_session_arithmetic_function_values_use_universal_invocation() {
    let mut session = Session::new().unwrap();
    for (source, expected) in [
        ("(let [f +] (f))", 0.0),
        ("(let [f *] (f))", 1.0),
        ("(let [f +] (f 1 2))", 3.0),
        ("(let [f *] (f \"2\" 3 4))", 24.0),
        ("(let [f -] (f nil))", -0.0),
        ("(let [f /] (f \"4\"))", 0.25),
        ("((if false + *) \"2\" 3)", 6.0),
        ("((let [f +] (f (fn [] 42))))", 42.0),
    ] {
        assert_eq!(
            eval_number(&mut session, source),
            f64::to_bits(expected),
            "{source}"
        );
    }
    let value = session.eval("((fn [f x] (f x 3)) + \"2\")").unwrap();
    assert_eq!(units(&mut session, &value), vec![50, 51]);
    let subtract = session.eval("-").unwrap();
    assert!(matches!(
        session.invoke(&subtract, &[]),
        Err(SessionError::Language(_))
    ));
    assert_eq!(eval_number(&mut session, "(+ 40 2)"), 42.0f64.to_bits());
}

#[test]
fn persistent_session_arithmetic_values_share_cells_and_keep_old_functions() {
    let mut session = Session::new().unwrap();
    let original = session.eval("+").unwrap();
    let root = session
        .inspect(&original, |mut store, value| {
            value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
        })
        .unwrap();
    for source in ["+", "cljs.core/+", "suss.core/+", "(let [f +] (f +))"] {
        let alias = session.eval(source).unwrap();
        assert!(
            session
                .inspect(&alias, |store, value| wasmtime::Rooted::ref_eq(
                    &store,
                    &root,
                    value.unwrap_anyref().unwrap()
                ))
                .unwrap(),
            "{source}"
        );
    }
    session
        .eval("(def captured (let [f +] (fn [x y] (f x y))))")
        .unwrap();
    session.enter_namespace("suss.core").unwrap();
    session.eval("(def + (fn [x y] (- x y)))").unwrap();
    let replacement = session.eval("cljs.core/+").unwrap();
    assert!(
        !session
            .inspect(&replacement, |store, value| wasmtime::Rooted::ref_eq(
                &store,
                &root,
                value.unwrap_anyref().unwrap()
            ))
            .unwrap()
    );
    session.enter_namespace("user").unwrap();
    let one = session.eval("1").unwrap();
    let two = session.eval("2").unwrap();
    session.collect().unwrap();
    let result = session.invoke(&original, &[&one, &two]).unwrap();
    assert_eq!(number(&mut session, &result), 3.0f64.to_bits());
    let result = session.invoke(&replacement, &[&one, &two]).unwrap();
    assert_eq!(number(&mut session, &result), (-1.0f64).to_bits());
    assert_eq!(
        eval_number(&mut session, "(captured 1 2)"),
        3.0f64.to_bits()
    );
    assert_eq!(
        eval_number(&mut session, "(let [f +] (f 1 2))"),
        (-1.0f64).to_bits()
    );
    let before = session.stats();
    assert!(matches!(
        session.eval("(let [f +] missing)"),
        Err(SessionError::Compile(_))
    ));
    assert_eq!(session.stats(), before);
}

#[test]
fn persistent_session_arithmetic_values_evaluate_callee_and_arguments_once() {
    let mut session = Session::new().unwrap();
    session
        .eval("(def counter 0) (def next (fn [] (def counter (+ counter 1)) counter))")
        .unwrap();
    assert_eq!(
        eval_number(&mut session, "((do (next) +) (next) (next))"),
        5.0f64.to_bits()
    );
    assert_eq!(eval_number(&mut session, "counter"), 3.0f64.to_bits());
    assert_eq!(
        eval_number(&mut session, "(let [f *] (f (next) (next)))"),
        20.0f64.to_bits()
    );
    assert_eq!(eval_number(&mut session, "counter"), 5.0f64.to_bits());
}

#[test]
fn persistent_session_loop_and_function_recur_replace_bindings_in_parallel() {
    let mut session = Session::new().unwrap();
    for (source, expected) in [
        (
            "(loop [flag true x 1 y 2] (if flag (recur false y x) (- x y)))",
            1.0,
        ),
        ("(loop [x 1 y (+ x 2)] y)", 3.0),
        ("(loop [flag true x nil] (if flag (recur false 9) x))", 9.0),
        (
            "((fn [flag x y] (if flag (recur false y x) (- x y))) true 1 2)",
            1.0,
        ),
        ("(loop [] 42)", 42.0),
    ] {
        assert_eq!(
            eval_number(&mut session, source),
            f64::to_bits(expected),
            "{source}"
        );
    }
    let value = session
        .eval("((fn [flag x] (if flag (recur false \"42\") x)) true nil)")
        .unwrap();
    assert_eq!(units(&mut session, &value), vec![52, 50]);
}

#[test]
fn recurrence_preserves_old_iteration_captures_effect_order_and_dynamic_types() {
    let mut session = Session::new().unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(loop [flag true x 1 f nil] (if flag (recur false 2 (fn [] x)) (f)))"
        ),
        1.0f64.to_bits()
    );
    assert_eq!(
        eval_number(
            &mut session,
            "(let [outer 10] ((fn [flag x] (if flag (recur false 2) (+ outer x))) true 1))"
        ),
        12.0f64.to_bits()
    );
    assert_eq!(
        eval_number(
            &mut session,
            "(loop [x 2] ((fn [flag y] (if flag (recur false (+ y 1)) y)) true x))"
        ),
        3.0f64.to_bits()
    );
    let text = session
        .eval("(loop [flag true x 100] (if flag (recur false \"2\") (+ x 1)))")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(units(&mut session, &text), [50, 49]);
    session
        .eval("(def counter 0) (def next (fn [] (def counter (+ counter 1))))")
        .unwrap();
    assert_eq!(
        eval_number(
            &mut session,
            "(loop [flag true x 0 y 0] (if flag (recur false (next) (next)) (+ (* x 10) y)))"
        ),
        12.0f64.to_bits()
    );
    assert_eq!(eval_number(&mut session, "counter"), 2.0f64.to_bits());
    let saved = session
        .eval("(loop [flag true x 1] (if flag (recur false (fn [] x)) x))")
        .unwrap();
    session.collect().unwrap();
    let result = session.invoke(&saved, &[]).unwrap();
    assert_eq!(number(&mut session, &result), 1.0f64.to_bits());
}

#[test]
fn recurrence_fuel_traps_and_compile_errors_leave_the_session_usable() {
    let mut session = Session::new().unwrap();
    session.eval("(def old 7)").unwrap();
    let before = session.stats();
    assert!(matches!(
        session.eval("(def old (loop [x 1] (+ (recur 2) x)))"),
        Err(SessionError::Compile(_))
    ));
    assert_eq!(session.stats(), before);
    session.set_operation_fuel(500);
    assert!(matches!(
        session.eval("(loop [] (if true (recur) (recur)))"),
        Err(SessionError::Trap(_))
    ));
    session.set_operation_fuel(100_000);
    assert_eq!(eval_number(&mut session, "(+ old 1)"), 8.0f64.to_bits());
    session.set_operation_fuel(500);
    assert!(matches!(
        session.eval("((fn [] (recur)))"),
        Err(SessionError::Trap(_))
    ));
    session.set_operation_fuel(100_000);
    assert_eq!(eval_number(&mut session, "old"), 7.0f64.to_bits());
}
