//! Dynamic scope executes in the persistent ABI runtime; no source replay.
use suss_cli::portable_session::{Session, SessionError, SessionValue};
use wasmtime::Val;
fn number(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let fields = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
}
fn eval(session: &mut Session, source: &str) -> u64 {
    let value = session.eval(source).unwrap();
    number(session, &value)
}

#[test]
fn binding_initializers_are_parallel_and_nested_scopes_restore_current_values() {
    let mut s = Session::new().unwrap();
    s.eval("(def ^:dynamic *x* 1) (def ^{:dynamic true} *y* 2)")
        .unwrap();
    assert_eq!(
        eval(&mut s, "(binding [*x* 7 *y* *x*] (+ (* *x* 10) *y*))"),
        71.0f64.to_bits()
    );
    assert_eq!(eval(&mut s, "*x*"), 1.0f64.to_bits());
    assert_eq!(eval(&mut s, "*y*"), 2.0f64.to_bits());
    assert_eq!(
        eval(
            &mut s,
            "(binding [*x* 2] (+ (* (binding [*x* 7] *x*) 10) *x*))"
        ),
        72.0f64.to_bits()
    );
    assert_eq!(
        eval(&mut s, "(binding [*x* 7 *x* 9] *x*)"),
        9.0f64.to_bits()
    );
    assert_eq!(eval(&mut s, "*x*"), 1.0f64.to_bits());
    assert_eq!(
        eval(
            &mut s,
            "(binding [*x* nil *y* false] (+ (if *x* 1 10) (if *y* 2 20)))"
        ),
        30.0f64.to_bits()
    );
}

#[test]
fn binding_is_visible_to_old_functions_but_escaped_readers_do_not_capture_scope() {
    let mut s = Session::new().unwrap();
    s.eval("(def ^:dynamic *value* 1) (def read-value (fn [] *value*))")
        .unwrap();
    assert_eq!(
        eval(&mut s, "(binding [*value* 7] (read-value))"),
        7.0f64.to_bits()
    );
    let escaped = s.eval("(binding [*value* 7] (fn [] *value*))").unwrap();
    let captured = s
        .eval("(binding [*value* 7] (let [saved *value*] (fn [] saved)))")
        .unwrap();
    s.collect().unwrap();
    let result = s.invoke(&escaped, &[]).unwrap();
    assert_eq!(number(&mut s, &result), 1.0f64.to_bits());
    let result = s.invoke(&captured, &[]).unwrap();
    assert_eq!(number(&mut s, &result), 7.0f64.to_bits());
    s.eval("(def ^:dynamic *value* 42)").unwrap();
    s.collect().unwrap();
    let result = s.invoke(&escaped, &[]).unwrap();
    assert_eq!(number(&mut s, &result), 42.0f64.to_bits());
}

#[test]
fn original_values_are_snapshotted_before_initializers_and_failed_initializers_keep_effects() {
    let mut s = Session::new().unwrap();
    s.eval("(def ^:dynamic *value* 1)").unwrap();
    assert_eq!(
        eval(
            &mut s,
            "(binding [*value* (do (set! *value* 2) 7)] *value*)"
        ),
        7.0f64.to_bits()
    );
    assert_eq!(eval(&mut s, "*value*"), 1.0f64.to_bits());
    assert_eq!(
        eval(
            &mut s,
            "(try (binding [*value* (do (set! *value* 2) (throw 7))] 42) (catch :default error *value*))"
        ),
        2.0f64.to_bits()
    );
    assert_eq!(eval(&mut s, "(set! *value* 1)"), 1.0f64.to_bits());
    assert_eq!(
        eval(
            &mut s,
            "(binding [*value* 7] (try (binding [*value* (do (set! *value* 8) (throw 9))] 42) (catch :default error *value*)))"
        ),
        8.0f64.to_bits()
    );
    assert_eq!(eval(&mut s, "*value*"), 1.0f64.to_bits());
    s.eval("(binding [*value* 7] (set! *value* 9))").unwrap();
    assert_eq!(eval(&mut s, "*value*"), 1.0f64.to_bits());
    s.eval("(binding [*value* 7] (def *value* 9))").unwrap();
    assert_eq!(eval(&mut s, "*value*"), 1.0f64.to_bits());
}

#[test]
fn binding_cleanup_restores_after_throw_and_runs_inside_finally_before_outer_handlers() {
    let mut s = Session::new().unwrap();
    s.eval("(def ^:dynamic *value* 1) (def seen 0)").unwrap();
    assert_eq!(
        eval(
            &mut s,
            "(try (binding [*value* 7] (throw *value*)) (catch :default error (+ (* error 10) *value*)))"
        ),
        71.0f64.to_bits()
    );
    assert_eq!(
        eval(
            &mut s,
            "(try (binding [*value* 7] (try (throw 42) (finally (def seen *value*)))) (catch :default error (+ seen *value*)))"
        ),
        8.0f64.to_bits()
    );
    assert_eq!(
        eval(
            &mut s,
            "(try (binding [*value* 7] (try (throw 9) (finally (throw *value*)))) (catch :default error (+ (* error 10) *value*)))"
        ),
        71.0f64.to_bits()
    );
    assert_eq!(eval(&mut s, "*value*"), 1.0f64.to_bits());
}

#[test]
fn with_redefs_keeps_macro_calls_and_rebinds_actual_function_values() {
    let mut s = Session::new().unwrap();
    s.eval("(def ordinary (fn [] 1)) (def plain 1)").unwrap();
    assert_eq!(
        eval(&mut s, "(with-redefs [ordinary (fn [] 42)] (ordinary))"),
        42.0f64.to_bits()
    );
    assert_eq!(eval(&mut s, "(ordinary)"), 1.0f64.to_bits());
    // The pin warns for plain vars but executes them; do not impose JVM rules.
    assert_eq!(eval(&mut s, "(binding [plain 7] plain)"), 7.0f64.to_bits());
    assert_eq!(eval(&mut s, "plain"), 1.0f64.to_bits());
    assert_eq!(
        eval(&mut s, "(with-redefs [+ (fn [x y] 42)] (+ 1 2))"),
        3.0f64.to_bits()
    );
    assert_eq!(
        eval(&mut s, "((with-redefs [+ (fn [x y] 42)] +) 1 2)"),
        42.0f64.to_bits()
    );
    assert_eq!(eval(&mut s, "(+ 1 2)"), 3.0f64.to_bits());
}

#[test]
fn malformed_binding_and_assignment_fail_locally_without_publishing_declarations() {
    let mut s = Session::new().unwrap();
    s.eval("(def ^:dynamic *value* 1)").unwrap();
    let before = s.stats().binding_cells;
    for source in [
        "(def leaked 7) (binding)",
        "(def leaked 7) (binding [*value*])",
        "(def leaked 7) (binding [missing 7] 42)",
        "(def leaked 7) (let [*value* 2] (binding [*value* 7] *value*))",
        "(def leaked 7) (let [local 2] (set! local 7))",
        "(def leaked 7) (set! missing 7)",
        "(def leaked 7) (set! *value*)",
        "(def leaked 7) (loop [x 0] (binding [*value* 7] (recur 1)))",
    ] {
        let SessionError::Compile(d) = s.eval(source).unwrap_err() else {
            panic!("located compile rejection: {source}")
        };
        assert!(d.span.start < d.span.end && d.span.end <= source.len());
        assert_eq!(s.stats().binding_cells, before, "{source}");
    }
    assert!(matches!(s.eval("leaked"), Err(SessionError::Compile(_))));
    assert_eq!(eval(&mut s, "*value*"), 1.0f64.to_bits());
}

#[test]
fn fuel_traps_do_not_leak_dynamic_values_into_the_next_prompt_or_saved_function() {
    let mut s = Session::new().unwrap();
    s.eval("(def ^:dynamic *value* 1)").unwrap();
    let saved = s
        .eval("(fn [] (binding [*value* 9] (loop [] (recur))))")
        .unwrap();
    s.set_operation_fuel(5_000);
    assert!(matches!(
        s.eval("(binding [*value* 7] (try (loop [] (recur)) (catch :default error 42)))"),
        Err(SessionError::Trap(_))
    ));
    s.set_operation_fuel(50_000);
    assert_eq!(eval(&mut s, "*value*"), 1.0f64.to_bits());
    s.set_operation_fuel(5_000);
    assert!(matches!(s.invoke(&saved, &[]), Err(SessionError::Trap(_))));
    s.set_operation_fuel(50_000);
    assert_eq!(eval(&mut s, "*value*"), 1.0f64.to_bits());
}

#[test]
fn fuel_interruption_during_frame_pop_restores_pre_initializer_snapshot() {
    let mut s = Session::new().unwrap();
    s.eval("(def ^:dynamic *value* 1) (def entered false)")
        .unwrap();
    let mut interrupted = 0;
    for fuel in (1..=1200).step_by(1) {
        s.set_operation_fuel(50_000);
        s.eval("(set! *value* 1) (set! entered false)").unwrap();
        s.set_operation_fuel(fuel);
        let result = s.eval("(binding [*value* (do (set! *value* 2) 7)] (set! entered true) 42)");
        s.set_operation_fuel(50_000);
        let entered = eval(&mut s, "(if entered 1 0)") == 1.0f64.to_bits();
        if entered {
            if matches!(result, Err(SessionError::Trap(_))) {
                interrupted += 1;
            }
            assert_eq!(eval(&mut s, "*value*"), 1.0f64.to_bits(), "fuel {fuel}");
        }
    }
    assert!(interrupted > 0, "actually interrupt after body entry");
}

#[test]
fn nested_multi_target_pop_is_restartable_at_every_fuel_boundary() {
    let mut s = Session::new().unwrap();
    s.eval("(def ^:dynamic *x* 1) (def ^:dynamic *y* 2) (def entered false)")
        .unwrap();
    let mut interrupted = 0;
    for fuel in 1..=2000 {
        s.set_operation_fuel(50_000);
        s.eval("(set! *x* 1) (set! *y* 2) (set! entered false)")
            .unwrap();
        s.set_operation_fuel(fuel);
        let result = s.eval("(binding [*x* (do (set! *x* 3) 7) *y* (do (set! *y* 4) 8)] (binding [*x* (do (set! *x* 9) 11) *y* (do (set! *y* 10) 12) *x* 13] (set! entered true) 42))");
        s.set_operation_fuel(50_000);
        if eval(&mut s, "(if entered 1 0)") == 1.0f64.to_bits() {
            if matches!(result, Err(SessionError::Trap(_))) {
                interrupted += 1;
            }
            assert_eq!(eval(&mut s, "*x*"), 1.0f64.to_bits(), "x at fuel {fuel}");
            assert_eq!(eval(&mut s, "*y*"), 2.0f64.to_bits(), "y at fuel {fuel}");
        }
    }
    assert!(interrupted > 0);
}

#[test]
fn closure_values_and_thrown_closures_survive_frame_restoration_and_gc() {
    let mut s = Session::new().unwrap();
    s.eval("(def ^:dynamic *f* (fn [] 1))").unwrap();
    let replacement = s.eval("(binding [*f* (fn [] 42)] *f*)").unwrap();
    let thrown = match s.eval("(binding [*f* (fn [] 7)] (throw *f*))").unwrap_err() {
        SessionError::Language(value) => value,
        error => panic!("language closure payload: {error:?}"),
    };
    s.collect().unwrap();
    let value = s.invoke(&replacement, &[]).unwrap();
    assert_eq!(number(&mut s, &value), 42.0f64.to_bits());
    let value = s.invoke(&thrown, &[]).unwrap();
    assert_eq!(number(&mut s, &value), 7.0f64.to_bits());
    assert_eq!(eval(&mut s, "(*f*)"), 1.0f64.to_bits());
}
