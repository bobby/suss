//! Acceptance regressions for one cumulative Macro-phase fuel budget across a
//! complete source-fragment expansion. Dedicated accounting cases use small
//! test-only fuel limits. The metadata-authority semantic regression uses a
//! separate compilation allowance; production defaults and Runtime are unchanged.
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
// Semantic compilation calibration authorized by #252; this is not an
// exhaustion/default-budget control. Keep caller Runtime limits unchanged.
const METADATA_AUTHORITY_COMPILATION_FUEL: u64 = 100_000_000_000;

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
fn source_metadata_keeps_identity_after_witnessed_hash_memo_warming() {
    fn booleans(caller: &mut Session, value: &suss_compile::portable_session::SessionValue) -> Vec<bool> {
        caller.collect().unwrap();
        let bridge = suss_compile::portable_macro_data::FormBridge::new(caller).unwrap();
        let form = bridge.read(caller, value, 0..1).unwrap();
        let Kind::Vector(items) = &form.kind else { panic!("Boolean observation vector") };
        items.iter().map(|item| {
            let Kind::Bool(value) = &item.kind else { panic!("actual Boolean observation") };
            *value
        }).collect()
    }
    const OBSERVE: &str = r#"(defmacro inspect-memo [warm]
      (let [info (get (get (get &env :ns) :defs) 'review-memo-target)
            metadata (get info :meta)]
        (defonce saved-memo-metadata metadata)
        (if warm
          (let [before (.-__hash metadata)
                result (hash metadata)
                after (.-__hash metadata)]
            (list 'quote [(identical? saved-memo-metadata metadata)
                          (nil? before) (number? after) (= result after)]))
          (list 'quote [(identical? saved-memo-metadata metadata)
                        (number? (.-__hash metadata))]))))"#;
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        caller.eval("(def ^{:doc \"memo identity\"} review-memo-target 0)").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(OBSERVE).unwrap();
        let warm = caller.eval_with_macros("(inspect-memo true)", &mut macros).unwrap();
        assert_eq!(booleans(&mut caller, &warm), [true, true, true, true],
            "witness the nil-to-number memo transition, not just a hash call");
        let again = caller.eval_with_macros("(do (inspect-memo false))", &mut macros).unwrap();
        assert_eq!(booleans(&mut caller, &again), [true, true],
            "ordinary hash warming must preserve the captured declaration's metadata identity and memo");
    }
}


// Pinned reference: tests/oracle/metadata-custom-hash-observations.json.
// This supplemental boundary uses an equivalent user IHash implementation;
// CLJS delegates its original method, while Suss executes the public core body.
#[test]
fn source_metadata_keeps_identity_with_a_replaced_hash_method() {
    const OBSERVE: &str = r#"(defmacro inspect-custom-hash [warm]
      (let [metadata (get (get (get (get &env :ns) :defs) 'custom-hash-target) :meta)
            audit (get metadata :audit)]
        (defonce saved-custom-metadata metadata)
        (defonce saved-custom-audit audit)
        (let [before (.-__hash audit)
              result (if warm (hash audit) nil)]
          (list 'quote [(identical? saved-custom-metadata metadata)
                        (identical? saved-custom-audit audit)
                        (= 17 (get audit :x))
                        (if warm (nil? before) true)
                        (if warm (number? (.-__hash audit)) true)
                        (if warm (= result (.-__hash audit)) true)]))))"#;
    let mut after_observations = Vec::new();
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        caller.eval("(def ^{:audit {:x 17}} custom-hash-target 17)").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.define(OBSERVE).unwrap();
        let first = caller.eval_with_macros("(inspect-custom-hash false)", &mut macros).unwrap();
        assert_eq!(suss_cli::portable_repl::display(&mut caller, &first).unwrap(),
                   "[true true true true true true]");
        macros.define(r#"(defmacro install-custom-hash []
          (extend-type PersistentArrayMap IHash
          (-hash [self]
            (let [memo (.-__hash self)]
              (if (nil? memo)
                (let [value (hash-unordered-coll self)]
                  (set! (.-__hash self) value) value)
                memo)))) nil)"#).unwrap();
        caller.eval_with_macros("(install-custom-hash)", &mut macros).unwrap();
        let before = caller.eval_with_macros("(do (inspect-custom-hash false))", &mut macros).unwrap();
        assert_eq!(suss_cli::portable_repl::display(&mut caller, &before).unwrap(),
                   "[true true true true true true]", "identity before custom memo warming");
        caller.collect().unwrap();
        let warm = caller.eval_with_macros("(inspect-custom-hash true)", &mut macros).unwrap();
        assert_eq!(suss_cli::portable_repl::display(&mut caller, &warm).unwrap(),
                   "[true true true true true true]", "custom memo miss must preserve identity");
        caller.collect().unwrap();
        let after = caller.eval_with_macros("(let [] (inspect-custom-hash false))", &mut macros).unwrap();
        after_observations.push(suss_cli::portable_repl::display(&mut caller, &after).unwrap());
    }
    assert_eq!(after_observations, vec!["[true true true true true true]"; 2],
               "identity after custom memo warming in Runtime and Macro callers");
}

// Reference: tests/oracle/metadata-custom-hash-order-observations.json.
#[test]
fn source_metadata_keeps_canonically_warmed_identity_after_hash_replacement() {
    const OBSERVE: &str = r#"
      (defmacro prime-before-replacement []
        (def warm-before-dispatches 0)
        (let [m (:meta (get (:defs (:ns &env)) 'warm-before-target))
              a (:audit m)
              before (.-__hash a)
              h (hash a)
              after (.-__hash a)]
          (def saved-warm-before-metadata m)
          (def saved-warm-before-audit a)
          (def saved-warm-before-memo after)
          (list 'quote [(nil? before) (number? after)
                        (= h after) (= 17 (:x a))])))
      (defmacro inspect-warm-before [hit expected-hash]
        (let [m (:meta (get (:defs (:ns &env)) 'warm-before-target))
              a (:audit m)
              old-before (.-__hash saved-warm-before-audit)
              actual-before (.-__hash a)
              calls-before warm-before-dispatches
              h (if hit (hash a) nil)
              calls-after warm-before-dispatches
              old-after (.-__hash saved-warm-before-audit)
              actual-after (.-__hash a)]
          (list 'quote
            [(identical? saved-warm-before-metadata m)
             (identical? saved-warm-before-audit a)
             (number? old-before)
             (number? actual-before)
             (identical? old-before saved-warm-before-memo)
             (identical? actual-before saved-warm-before-memo)
             (= 17 (:x saved-warm-before-audit))
             (= 17 (:x a))
             (if hit (= h (if (nil? expected-hash) saved-warm-before-memo expected-hash)) true)
             (identical? old-after old-before)
             (identical? actual-after actual-before)
             (= calls-after (if hit (inc calls-before) calls-before))
             (if hit (if (nil? expected-hash) (identical? h actual-before) (= h expected-hash)) true)])))
    "#;
    const INSTALL: &str = r#"
      (defmacro install-warm-before [constant]
        (extend-type PersistentArrayMap IHash
          (-hash [self]
            (do
              (if (identical? self saved-warm-before-audit)
                (set! warm-before-dispatches (inc warm-before-dispatches)) nil)
              (if constant 91
                (let [memo (.-__hash self)]
                  (if (nil? memo)
                    (let [h (hash-unordered-coll self)]
                      (set! (.-__hash self) h) h)
                    memo)))))) nil)
    "#;
    let all_true = |n| format!("[{}]", vec!["true"; n].join(" "));
    // Preserve completed rows even if a later setup/evaluation stage errors.
    let mut observations: Vec<(String, String, String, String)> = Vec::new();
    let mut errors = Vec::new();
    for constant in [false, true] {
      for macro_caller in [false, true] {
        let phase = format!("{} / {}",
            if macro_caller { "Macro" } else { "Runtime" },
            if constant { "constant91" } else { "manual-warm-hit" });
        let mut stage = "create";
        let run = (|| -> Result<(), String> {
            let mut caller = if macro_caller { Session::new_macro() }
                             else { Session::new_repl() }
                .map_err(|e| e.to_string())?;
            stage = "declare";
            caller.eval("(def ^{:audit {:x 17}} warm-before-target 17)")
                .map_err(|e| e.to_string())?;
            let mut macros = CompiledMacros::new().map_err(|e| e.to_string())?;
            let observer_start = OBSERVE.find("(defmacro inspect-warm-before").unwrap();
            stage = "define-prime";
            macros.define(&OBSERVE[..observer_start]).map_err(|e| e.to_string())?;
            let expected_hash = if constant { "91" } else { "nil" };
            let before = format!("(do (inspect-warm-before false {expected_hash}))");
            let hit = format!("(inspect-warm-before true {expected_hash})");
            let after = format!("(let [] (inspect-warm-before false {expected_hash}))");
            for (label, source, count) in [
                ("canonical-warm", "(prime-before-replacement)", 4),
                ("canonical-next-call", "(inspect-warm-before false nil)", 13),
                ("custom-before-hit", before.as_str(), 13),
                ("custom-hit", hit.as_str(), 13),
                ("custom-after-hit", after.as_str(), 13),
            ] {
                stage = label;
                if label == "canonical-next-call" {
                    stage = "define-observer";
                    macros.define(&OBSERVE[observer_start..]).map_err(|e| e.to_string())?;
                    stage = label;
                }
                if label == "custom-before-hit" {
                    stage = "install";
                    macros.define(INSTALL).map_err(|e| e.to_string())?;
                    caller.eval_with_macros(
                        if constant { "(install-warm-before true)" }
                        else { "(install-warm-before false)" }, &mut macros)
                        .map_err(|e| e.to_string())?;
                    stage = label;
                }
                caller.collect().map_err(|e| e.to_string())?;
                let value = caller.eval_with_macros(source, &mut macros)
                    .map_err(|e| e.to_string())?;
                let actual = suss_cli::portable_repl::display(&mut caller, &value)
                    .map_err(|e| e.to_string())?;
                observations.push((phase.clone(), label.into(), actual, all_true(count)));
            }
            Ok(())
        })();
        if let Err(error) = run { errors.push(format!("{phase} {stage}: {error}")); }
      }
    }
    // No identity assertion stops remaining observations or the other phase.
    for (phase, stage, actual, expected) in &observations {
        if actual != expected {
            errors.push(format!("{phase} {stage}: {actual}; expected {expected}"));
        }
    }
    assert!(errors.is_empty(), "observations={observations:?}\n{}", errors.join("\n"));
}

// Interpretation:
// canonical-next-call isolates preexisting canonical receipt retention before
// any replacement. custom-before-hit isolates invalidation by installation alone.
// custom-hit hashes the CURRENT audit: rebuilt roots cannot fake saved-receiver
// counter increments, and a miss cannot masquerade as preserving the old memo.
// The miss branch remains legitimate executable user behavior but the intended
// successful path is exclusively an already-warm hit, avoiding new custom-write
// authorization as a prerequisite for this separate regression.
// Counter differences bracket only explicit hash inside macro execution; do not
// infer a zero-transport-callback policy. Both phases use independent macrohosts.

// Constant91 checks current hash result91 without treating the old memo as the
// hash result. The saved and current memo boxes must remain identical throughout.
// Each (variant, caller phase) creates a fresh Session and CompiledMacros host.


#[test]
fn source_metadata_is_fresh_after_a_macro_mutates_nested_guest_storage() {
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
        let mutated = caller.eval_with_macros("(inspect-audit 99)", &mut macros).unwrap();
        assert_eq!(number(&mut caller, &mutated), 99.0f64.to_bits(),
            "the macro must actually mutate the nested guest backing array");
        caller.collect().unwrap();
        // Fresh-host control uses the unchanged caller definition. Different
        // source fragments also prevent an identical-input artifact cache hit
        // from substituting for a new macro invocation in this regression.
        let mut fresh = CompiledMacros::new().unwrap();
        fresh.define(OBSERVE).unwrap();
        let control = caller.eval_with_macros("(+ 0 (inspect-audit nil))", &mut fresh).unwrap();
        assert_eq!(number(&mut caller, &control), 17.0f64.to_bits());
        let target = caller.eval("review-metadata-target").unwrap();
        assert_eq!(number(&mut caller, &target), 0.0f64.to_bits());
        let again = caller.eval_with_macros("(do (inspect-audit nil))", &mut macros).unwrap();
        assert_eq!(number(&mut caller, &again), 17.0f64.to_bits(),
            "a previous macro's mutation must not replace source-derived metadata");
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
fn source_metadata_poisoned_at_memo_write_is_not_certified_by_later_restoration() {
    fn observed(caller: &mut Session, value: &suss_compile::portable_session::SessionValue) -> Vec<bool> {
        caller.collect().unwrap();
        let bridge = suss_compile::portable_macro_data::FormBridge::new(caller).unwrap();
        let form = bridge.read(caller, value, 0..1).unwrap();
        let Kind::Vector(items) = &form.kind else { panic!("Boolean observation vector") };
        items.iter().map(|item| match &item.kind { Kind::Bool(value) => *value, _ => panic!("Boolean observation") }).collect()
    }
    const OBSERVE: &str = r#"(defmacro inspect-write-state [warm]
      (let [metadata (:meta (get (get (get &env :ns) :defs) 'write-state-target))
            audit (:audit metadata)
            original (aget (.-arr audit) 1)]
        (defonce saved-write-state metadata)
        (if warm
          (do
            (def write-state-calls 0)
            (set! suss.core/hash-unordered-coll
              (fn [m] (do (set! write-state-calls (+ write-state-calls 1))
                           (aset (.-arr (:audit m)) 1 99) 719)))
            (hash metadata)
            (let [poisoned (= 99 (:x audit))]
              (aset (.-arr audit) 1 original)
              (list 'quote [(= write-state-calls 1) poisoned
                            (= 719 (.-__hash metadata)) (= 17 (:x audit))])))
          (list 'quote [(not (identical? saved-write-state metadata))
                        (= 719 (.-__hash saved-write-state))
                        (nil? (.-__hash metadata)) (= 17 (:x audit))]))))"#;
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        caller.eval("(def ^{:audit {:x 17}} write-state-target 0)").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(METADATA_AUTHORITY_COMPILATION_FUEL);
        macros.define(OBSERVE).unwrap();
        let first = caller.eval_with_macros("(inspect-write-state true)", &mut macros).unwrap();
        assert_eq!(observed(&mut caller, &first), [true,true,true,true],
            "witness poison during the hash callback, the actual memo, and later source restoration");
        let again = caller.eval_with_macros("(do (inspect-write-state false))", &mut macros).unwrap();
        assert_eq!(observed(&mut caller, &again), [true,true,true,true],
            "restored data cannot retroactively authorize a memo written while poisoned");
    }
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
