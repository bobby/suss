//! Actual guest progress, supplementing—not replacing—the original512-call negative.
use super::*;

#[test]
fn cumulative_exhaustion_follows_multiple_completed_guest_helpers_and_recovers() {
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let caller_fuel = caller.options().fuel_per_operation;
        let mut macros = CompiledMacros::new().unwrap();
        macros.define("(defmacro tick [] (do (defonce completed-helper-count (atom 0)) (reset! completed-helper-count (+ @completed-helper-count 1)) 1))").unwrap();
        macros
            .define("(defmacro completed-count [] @completed-helper-count)")
            .unwrap();
        macros
            .define("(defmacro clear-count [] (reset! completed-helper-count 0) nil)")
            .unwrap();
        macros.define("(defmacro counted-recover [] 42)").unwrap();
        // Dedicated cumulative negative; shared semantic10B calibration is not used.
        macros.set_operation_fuel(SMALL_OPERATION_FUEL);
        let value = caller
            .eval_with_macros("(do (tick) (tick))", &mut macros)
            .unwrap();
        assert_eq!(number(&mut caller, &value), 1.0f64.to_bits());
        drop(value);
        let value = caller
            .eval_with_macros("(completed-count)", &mut macros)
            .unwrap();
        assert_eq!(number(&mut caller, &value), 2.0f64.to_bits());
        drop(value);
        caller
            .eval_with_macros("(clear-count)", &mut macros)
            .unwrap();
        let zero = caller
            .eval_with_macros("(completed-count)", &mut macros)
            .unwrap();
        assert_eq!(number(&mut caller, &zero), 0.0f64.to_bits());
        drop(zero);
        caller.eval("(def counted-stable 7)").unwrap();
        let calls = std::iter::repeat_n("(tick)", 512)
            .collect::<Vec<_>>()
            .join(" ");
        let source = format!("(do (def counted-unpublished 99) {calls})");
        let before = caller.stats();
        let error = caller.eval_with_macros(&source, &mut macros).unwrap_err();
        let SessionError::Compile(diagnostic) = &error else {
            panic!("expected located expansion exhaustion: {error}");
        };
        assert!(diagnostic.span.start < diagnostic.span.end);
        assert!(diagnostic.span.end <= source.len());
        // Existing helper checks actual fuel text and a reached expansion-stage
        // diagnostic. No timer, guessed callback count or generic Err credit.
        fuel_failure(error);
        assert_eq!(caller.stats(), before);
        let missing = caller.eval("counted-unpublished").unwrap_err();
        assert!(missing.to_string().contains("Unresolved"), "{missing}");
        let stable = caller.eval("counted-stable").unwrap();
        assert_eq!(number(&mut caller, &stable), 7.0f64.to_bits());
        drop(stable);
        let value = caller
            .eval_with_macros("(completed-count)", &mut macros)
            .unwrap();
        let bits = number(&mut caller, &value);
        let increments = f64::from_bits(bits);
        assert!(increments.is_finite() && increments.fract() == 0.0);
        assert!(increments <= 512.0);
        // The final transformer may trap after incrementing but before return.
        // Sequential expansion permits at most one such in-flight call. Deduct
        // it rather than falsely labeling every observed effect a completed call.
        let completed_helpers_lower_bound = increments - 1.0;
        assert!(
            completed_helpers_lower_bound > 1.0,
            "need >1 completed guest helpers even allowing one partial call; increments={increments}"
        );
        drop(value);
        // Reading the surviving effect again must neither replay nor erase it.
        caller.collect().unwrap();
        let value = caller
            .eval_with_macros("(completed-count)", &mut macros)
            .unwrap();
        assert_eq!(number(&mut caller, &value), bits);
        drop(value);
        let value = caller
            .eval_with_macros("(counted-recover)", &mut macros)
            .unwrap();
        assert_eq!(number(&mut caller, &value), 42.0f64.to_bits());
        assert_eq!(caller.options().fuel_per_operation, caller_fuel);
        assert_eq!(
            portable::ExpansionHost::expansion_work_fuel(&macros),
            SMALL_OPERATION_FUEL
        );
    }
}
