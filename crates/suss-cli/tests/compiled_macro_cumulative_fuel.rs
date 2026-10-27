//! Acceptance regressions for one cumulative Macro-phase fuel budget across a
//! complete source-fragment expansion. These use public Session APIs and small
//! test-only fuel limits; production defaults are not changed.
use std::{
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use suss_compile::{
    portable::{self, SourceOrigin, resolve::Phase},
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError},
};
use suss_reader::{Symbol, forms::{Kind, read_forms}};
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Linker},
};

const SMALL_OPERATION_FUEL: u64 = 100_000;

fn fuel_failure(error: SessionError) {
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("out of fuel") || message.contains("all fuel consumed by webassembly"),
        "expected a real fuel trap: {message}"
    );
    assert!(
        message.contains("compiled macro expansion failed during "),
        "failure must be attributed to a source macro expansion stage: {message}"
    );
}

fn number(session: &mut Session, value: &suss_compile::portable_session::SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [wasmtime::Val::F64(bits)] = fields.as_slice() else {
                panic!("expected the portable Number representation")
            };
            Ok(*bits)
        })
        .unwrap()
}

fn boolean(session: &mut Session, value: &suss_compile::portable_session::SessionValue) -> bool {
    session
        .inspect(value, |store, value| {
            Ok(
                match value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32()
                {
                    2 => false,
                    4 => true,
                    other => panic!("unexpected Boolean representation {other}"),
                },
            )
        })
        .unwrap()
}

#[test]
fn macros_cannot_mutate_transported_metadata_storage() {
    // Built-in internals are private to suss.core (design decision 2026-10-25),
    // so transported compiler data is immutable: the write fails and later
    // invocations still observe the source metadata.
    const OBSERVE: &str = r#"(defmacro inspect-audit [replacement]
      (let [info (get (get (get &env :ns) :defs) 'review-metadata-target)
            audit (get (get info :meta) :audit)]
        (if replacement (aset (.-arr audit) 1 replacement) nil)
        (get audit :x)))"#;
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        caller.eval("(def ^{:audit {:x 17}} review-metadata-target 0)").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(OBSERVE).unwrap();
        let first = caller.eval_with_macros("(inspect-audit nil)", &mut macros).unwrap();
        assert_eq!(number(&mut caller, &first), 17.0f64.to_bits());
        assert!(caller.eval_with_macros("(inspect-audit 99)", &mut macros).is_err(),
            "a macro must not reach the backing storage of transported metadata");
        caller.collect().unwrap();
        let target = caller.eval("review-metadata-target").unwrap();
        assert_eq!(number(&mut caller, &target), 0.0f64.to_bits());
        let again = caller.eval_with_macros("(do (inspect-audit nil))", &mut macros).unwrap();
        assert_eq!(number(&mut caller, &again), 17.0f64.to_bits());
    }
}

#[test]
fn independent_macro_calls_in_one_fragment_share_fuel_and_next_repl_input_recovers() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro cheap [x] x)").unwrap();
    macros.set_operation_fuel(SMALL_OPERATION_FUEL);
    let mut repl = Session::new_repl().unwrap();

    // A single call establishes that each expansion is comfortably below the
    // configured budget. Keep that exact setting for the multi-call input.
    let one = repl.eval_with_macros("(cheap 7)", &mut macros).unwrap();
    assert_eq!(number(&mut repl, &one), 7.0f64.to_bits());

    // Every macro call is cheap on its own, while their aggregate work exceeds
    // one operation budget. Resetting fuel at each transformer root would let
    // this whole input through, which is the regression this test prevents.
    let calls = (0..512).map(|_| "(cheap 1)").collect::<Vec<_>>().join(" ");
    let source = format!("(do (def must-not-publish 99) {calls})");
    fuel_failure(repl.eval_with_macros(&source, &mut macros).unwrap_err());
    let missing = repl
        .eval_with_macros("must-not-publish", &mut macros)
        .unwrap_err();
    assert!(
        missing.to_string().contains("Unresolved"),
        "partial declaration was published: {missing}"
    );
    // A later REPL input receives the same configured budget and remains usable.
    let recovered = repl.eval_with_macros("(cheap 42)", &mut macros).unwrap();
    assert_eq!(number(&mut repl, &recovered), 42.0f64.to_bits());
}

#[test]
fn recursive_macro_reexpansion_does_not_reset_the_fragment_budget() {
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define("(defmacro descend [n] (if (= n 0) 42 (list 'descend (dec n))))")
        .unwrap();
    macros.set_operation_fuel(SMALL_OPERATION_FUEL);
    let mut repl = Session::new_repl().unwrap();

    // A shallow chain is below the limit; one deeper chain contains many
    // individually small re-expansions in the same source operation.
    let shallow = repl.eval_with_macros("(descend 1)", &mut macros).unwrap();
    assert_eq!(number(&mut repl, &shallow), 42.0f64.to_bits());
    fuel_failure(
        repl.eval_with_macros("(descend 512)", &mut macros)
            .unwrap_err(),
    );
    let recovered = repl.eval_with_macros("(descend 1)", &mut macros).unwrap();
    assert_eq!(number(&mut repl, &recovered), 42.0f64.to_bits());
}

#[test]
fn source_metadata_observer_and_nonobserver_survive_macro_input_and_result_transport() {
    for mut repl in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define("(defmacro observe-call [x] (list 'quote (= (:marker (meta &form)) 73)))")
            .unwrap();
        macros.define("(defmacro ignore-call [x] x)").unwrap();
        macros
            .define("(defmacro observe-argument [x] (list 'quote (:payload (meta x))))")
            .unwrap();
        macros.set_operation_fuel(2_000_000);

        // The first macro observes call-site metadata through &form and its
        // quoted result crosses the expanded-result transport. The second
        // deliberately ignores &form while returning an explicit child form.
        let observed = repl
            .eval_with_macros("^{:marker 73} (observe-call (+ 40 2))", &mut macros)
            .unwrap();
        assert!(boolean(&mut repl, &observed));

        let ignored = repl
            .eval_with_macros(
                "^{:marker 73} (ignore-call ^{:payload 19} (+ 40 2))",
                &mut macros,
            )
            .unwrap();
        assert_eq!(number(&mut repl, &ignored), 42.0f64.to_bits());

        // Explicit nested argument metadata remains observable to a macro that
        // does not inspect the implicit caller form.
        let child_metadata = repl
            .eval_with_macros("(observe-argument ^{:payload 19} (+ 40 2))", &mut macros)
            .unwrap();
        assert_eq!(number(&mut repl, &child_metadata), 19.0f64.to_bits());
    }
}

fn component_engine() -> Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .wasm_component_model(true)
                .wasm_component_model_implements(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).unwrap()
        })
        .clone()
}

#[test]
fn interrupted_macro_expansion_recovers_for_repl_and_aot_with_same_fuel_setting() {
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define("(defmacro spin [] (loop [] (recur)))")
        .unwrap();
    macros
        .define("(defmacro recover-value [x] (list '+ x 2))")
        .unwrap();
    let operation_fuel = 2_000_000;
    macros.set_operation_fuel(operation_fuel);
    let mut repl = Session::new_repl().unwrap();
    let warm = repl
        .eval_with_macros("(recover-value 0)", &mut macros)
        .unwrap();
    assert_eq!(number(&mut repl, &warm), 2.0f64.to_bits());

    let interrupt = macros.interrupt_handle();
    let stop_interrupts = std::sync::Arc::new(AtomicBool::new(false));
    let stop_thread = stop_interrupts.clone();
    let interrupt_thread = thread::spawn(move || {
        // Repeated requests avoid a timing race with operation setup, which
        // intentionally discards an interrupt received while the session is idle.
        while !stop_thread.load(Ordering::Acquire) {
            interrupt.interrupt();
            thread::sleep(Duration::from_millis(1));
        }
    });
    let error = repl.eval_with_macros("(spin)", &mut macros).unwrap_err();
    stop_interrupts.store(true, Ordering::Release);
    interrupt_thread.join().unwrap();
    assert!(
        error.to_string().to_lowercase().contains("interrupted"),
        "expected explicit interruption, got {error}"
    );

    // Do not increase fuel after the interruption. The next public REPL
    // operation should succeed with the same test budget.
    let recovered = repl
        .eval_with_macros("(recover-value 40)", &mut macros)
        .unwrap();
    assert_eq!(number(&mut repl, &recovered), 42.0f64.to_bits());
    // Exercise the recovered Macro session through the public fragment->AOT
    // path and execute the resulting component, again without changing fuel.
    let source = "(ns user) (def calculate (fn [] (recover-value 40)))";
    let core = portable::bootstrap::shipped(Phase::Runtime).unwrap();
    let fragment = portable::prepare_fragment_forms_with_origin(
        read_forms(source).unwrap(),
        0..source.len(),
        &core.environment,
        Phase::Runtime,
        &mut macros,
        Some(&SourceOrigin::new(source, Some("recovery.sus".into()))),
    )
    .unwrap();
    let mut resolve = portable::aot::Resolve::new();
    let package = resolve
        .push_str(
            "api.wit",
            "package test:macro-recovery; world api { export calculate: func() -> f64; }",
        )
        .unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    let bytes = portable::aot::component(
        &[core.clone(), fragment],
        &resolve,
        world,
        &[("calculate".into(), Symbol::parse("user/calculate"))],
    )
    .unwrap();
    let engine = component_engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(20_000_000).unwrap();
    let instance = Linker::<()>::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let calculate = instance
        .get_typed_func::<(), (f64,)>(&mut store, "calculate")
        .unwrap();
    assert_eq!(
        calculate.call(&mut store, ()).unwrap().0.to_bits(),
        42.0f64.to_bits()
    );
}

#[test]
fn exhausted_expansion_preserves_effects_already_executed_in_the_macro_store() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro record-effect [x] (do (defonce expansion-effects (atom 0)) (reset! expansion-effects x) (loop [n 20] (if (= n 0) x (recur (dec n))))))").unwrap();
    macros
        .define("(defmacro observed-expansion-effect [] @expansion-effects)")
        .unwrap();
    macros.set_operation_fuel(SMALL_OPERATION_FUEL);
    let mut repl = Session::new_repl().unwrap();
    let first = repl
        .eval_with_macros("(record-effect 7)", &mut macros)
        .unwrap();
    assert_eq!(number(&mut repl, &first), 7.0f64.to_bits());
    let calls = (0..32)
        .map(|_| "(record-effect 1)")
        .collect::<Vec<_>>()
        .join(" ");
    let error = repl
        .eval_with_macros(&format!("(do {calls})"), &mut macros)
        .unwrap_err();
    fuel_failure(error);
    let observed = repl
        .eval_with_macros("(observed-expansion-effect)", &mut macros)
        .unwrap();
    assert_eq!(
        number(&mut repl, &observed),
        1.0f64.to_bits(),
        "the failed operation must actually execute an effect and retain it"
    );
    let recovered = repl
        .eval_with_macros("(record-effect 42)", &mut macros)
        .unwrap();
    assert_eq!(number(&mut repl, &recovered), 42.0f64.to_bits());
}



#[test]
fn infinite_lazy_expanded_result_exhausts_without_publication_and_preserves_effects() {
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.define("(defmacro lazy-spin [] (do (defonce lazy-realization-effects (atom 0)) (reset! lazy-realization-effects 1) (new cljs.core/LazySeq nil (fn [] (do (reset! lazy-realization-effects (+ (* @lazy-realization-effects 10) 2)) (loop [] (recur)))) nil nil)))").unwrap();
        macros.define("(defmacro lazy-effects [] @lazy-realization-effects)").unwrap();
        macros.define("(defmacro lazy-recover [] 42)").unwrap();
        // Dedicated negative allowance, not the semantic-corpus calibration.
        // Recovery and effect inspection retain this exact configured budget.
        macros.set_operation_fuel(2_000_000);
        caller.eval("(def lazy-stable 7)").unwrap();
        let warm = caller.eval_with_macros("(lazy-recover)", &mut macros).unwrap();
        assert_eq!(number(&mut caller, &warm), 42.0f64.to_bits());
        let source = "(def lazy-unpublished 99) (lazy-spin)";
        let start = source.find("(lazy-spin)").unwrap();
        let before = caller.stats();
        let error = caller.eval_with_macros(source, &mut macros).unwrap_err();
        let SessionError::Compile(diagnostic) = &error else {
            panic!("expected source-located compilation failure: {error}");
        };
        assert_eq!(diagnostic.span, start..source.len());
        assert!(diagnostic.message.contains("during expanded result transport"),
                "transformer must return before realization fails: {diagnostic:?}");
        fuel_failure(error);
        assert_eq!(caller.stats(), before, "failed result must not publish code/bindings");
        let missing = caller.eval("lazy-unpublished").unwrap_err();
        assert!(missing.to_string().contains("Unresolved"), "{missing}");
        let stable = caller.eval("lazy-stable").unwrap();
        assert_eq!(number(&mut caller, &stable), 7.0f64.to_bits());
        // 12 proves ordered transformer effect 1 then thunk effect (1*10)+2.
        // Prior Macro effects survive; Runtime compilation is not published.
        let effects = caller.eval_with_macros("(lazy-effects)", &mut macros).unwrap();
        assert_eq!(number(&mut caller, &effects), 12.0f64.to_bits());
        caller.collect().unwrap();
        let recovered = caller.eval_with_macros("(lazy-recover)", &mut macros).unwrap();
        assert_eq!(number(&mut caller, &recovered), 42.0f64.to_bits());
    }
}


#[path = "support/completed_macro_work.rs"]
mod completed_macro_work;
