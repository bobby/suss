//! Fuel spent resolving a transformer still belongs to macro expansion.
use suss_compile::{
    portable::ExpansionHost,
    portable_macros::CompiledMacros,
    portable_session::{RuntimeNumber, Session, SessionError},
};

fn long(session: &mut Session, value: &suss_compile::portable_session::SessionValue) -> i64 {
    session.inspect(value, |store, value| {
        match suss_compile::portable_session::runtime_number(store, &value)? {
            Some(RuntimeNumber::Long(value)) => Ok(value),
            other => panic!("expected a real Long, got {other:?}"),
        }
    }).unwrap()
}

#[test]
fn exhausted_transformer_lookup_retains_expansion_stage_effects_and_recovery() {
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let phase = caller.phase();
        let caller_allowance = caller.options().fuel_per_operation;
        let mut macros = CompiledMacros::new().unwrap();
        // Preserve the original failing source, guest workload, 100K allowance
        // and 128-call batch. Faster guest code can finish one transformer and
        // exhaust the last allowance in the next real binding accessor.
        macros.define("(defmacro record-effect [x] (do (defonce expansion-effects (atom 0)) (reset! expansion-effects x) (loop [n 20] (if (= n 0) x (recur (dec n))))))").unwrap();
        macros.define("(defmacro observed-expansion-effect [] @expansion-effects)").unwrap();
        macros.set_operation_fuel(100_000);
        let first = caller.eval_with_macros("(record-effect 7)", &mut macros).unwrap();
        assert_eq!(long(&mut caller, &first), 7);
        drop(first);

        let calls = std::iter::repeat_n("(record-effect 1)", 128).collect::<Vec<_>>().join(" ");
        let source = format!("(do {calls})");
        let before = caller.stats();
        let error = caller.eval_with_macros(&source, &mut macros).unwrap_err();
        let SessionError::Compile(diagnostic) = &error else {
            panic!("{phase:?}: expected located expansion failure: {error}");
        };
        assert!(diagnostic.message.contains("Compiled macro expansion failed during "),
            "{phase:?}: actual expansion stage was lost: {diagnostic:?}");
        assert!(diagnostic.message.to_lowercase().contains("all fuel consumed by webassembly")
            || diagnostic.message.to_lowercase().contains("out of fuel"), "{diagnostic:?}");
        assert!(diagnostic.span.start < diagnostic.span.end && diagnostic.span.end <= source.len());
        assert_eq!(&source[diagnostic.span.clone()], "(record-effect 1)");
        assert!(!error.is_interrupt(), "fuel exhaustion must not become cancellation");
        assert_eq!(caller.stats(), before, "failed source preparation must not publish code");

        let observed = caller.eval_with_macros("(observed-expansion-effect)", &mut macros).unwrap();
        assert_eq!(long(&mut caller, &observed), 1, "completed Macro effects must survive");
        drop(observed);
        caller.collect().unwrap();
        let observed = caller.eval_with_macros("(observed-expansion-effect)", &mut macros).unwrap();
        assert_eq!(long(&mut caller, &observed), 1, "inspection must not replay or erase effects");
        drop(observed);
        let recovered = caller.eval_with_macros("(record-effect 42)", &mut macros).unwrap();
        assert_eq!(long(&mut caller, &recovered), 42);
        assert_eq!(caller.options().fuel_per_operation, caller_allowance);
        assert_eq!(ExpansionHost::expansion_work_fuel(&macros), 100_000);
    }
}
