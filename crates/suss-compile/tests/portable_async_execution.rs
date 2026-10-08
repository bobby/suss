//! Execute compiler-generated source continuations against the shared GC runtime.
//! These focused gates do not constitute M3 acceptance.
mod support;
use std::collections::BTreeSet;
use suss_compile::{
    portable::{
        self,
        resolve::{Environment, Phase},
    },
    runtime_abi,
};
use wasmtime::{
    Global, GlobalType, Instance, Linker, Module, Mutability, RefType, Store, Val, ValType,
};

struct Harness {
    store: Store<()>,
    runtime: Instance,
    linker: Linker<()>,
    environment: Environment,
    cells: BTreeSet<(String, String)>,
}
impl Harness {
    fn new() -> Self {
        let mut config = wasmtime::Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        let engine = wasmtime::Engine::new(&config).unwrap();
        let mut store = Store::new(&engine, ());
        store.set_fuel(1_000_000).unwrap();
        let runtime = Instance::new(
            &mut store,
            &Module::new(&engine, runtime_abi::module()).unwrap(),
            &[],
        )
        .unwrap();
        let mut linker = Linker::new(&engine);
        linker
            .instance(&mut store, "suss.runtime", runtime)
            .unwrap();
        Self {
            store,
            runtime,
            linker,
            environment: Environment::default(),
            cells: BTreeSet::new(),
        }
    }
    fn value(&mut self, name: &str, args: &[Val]) -> Val {
        self.store.set_fuel(1_000_000).unwrap();
        let mut result = [Val::null_any_ref()];
        self.runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut result)
            .unwrap_or_else(|error| panic!("runtime API {name} failed: {error:?}"));
        result[0].clone()
    }
    fn integer(&mut self, name: &str, args: &[Val]) -> i32 {
        self.store.set_fuel(1_000_000).unwrap();
        let mut result = [Val::I32(-1)];
        self.runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut result)
            .unwrap_or_else(|error| panic!("runtime API {name} failed: {error:?}"));
        result[0].unwrap_i32()
    }
    fn checked_language_error(&mut self, name: &str, args: &[Val]) {
        self.store.set_fuel(1_000_000).unwrap();
        let error = self
            .runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut [Val::null_any_ref()])
            .expect_err("runtime operation must reject this state");
        assert!(
            error.is::<wasmtime::ThrownException>(),
            "runtime API {name}: {error:#}"
        );
        assert!(
            !error.is::<wasmtime::Trap>(),
            "runtime API {name}: {error:#}"
        );
        let exception = self
            .store
            .take_pending_exception()
            .expect("checked language exception payload");
        let tag = exception.tag(&mut self.store).unwrap();
        let expected = self
            .runtime
            .get_tag(&mut self.store, "language-exception")
            .unwrap();
        assert!(wasmtime::Tag::eq(&tag, &expected, &self.store));
        let payload = exception.field(&mut self.store, 0).unwrap();
        assert_eq!(self.integer("language-error-is", &[payload]), 1);
        assert!(
            self.store.take_pending_exception().is_none(),
            "exception must be consumed before later scheduler calls"
        );
    }
    fn bind(&mut self, name: &str, value: Val) {
        let identity = self
            .environment
            .declare_cell(Phase::Runtime, "test", name)
            .unwrap();
        let cell = self.value("binding-new", &[value]);
        self.link_cell(&identity, cell);
    }
    fn link_cell(&mut self, identity: &portable::resolve::Global, cell: Val) {
        let ty = cell
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap()
            .ty(&self.store)
            .unwrap();
        let global = Global::new(
            &mut self.store,
            GlobalType::new(
                ValType::Ref(RefType::new(false, ty.into())),
                Mutability::Const,
            ),
            cell,
        )
        .unwrap();
        self.linker
            .define(
                &self.store,
                identity.import_module(),
                &identity.import_name(),
                global,
            )
            .unwrap();
        self.cells
            .insert((identity.import_module().to_owned(), identity.import_name()));
    }
    fn source(&mut self, source: &str) -> Val {
        self.store.set_fuel(1_000_000).unwrap();
        let prepared =
            portable::prepare_fragment(source, &self.environment, Phase::Runtime).unwrap();
        for identity in &prepared.cells {
            let key = (identity.import_module().to_owned(), identity.import_name());
            if !self.cells.contains(&key) {
                let cell = self.value("binding-unbound", &[]);
                self.link_cell(identity, cell);
            }
        }
        let module = Module::new(self.store.engine(), prepared.wasm).unwrap();
        let instance = self.linker.instantiate(&mut self.store, &module).unwrap();
        let mut result = [Val::null_any_ref()];
        instance
            .get_func(&mut self.store, "eval")
            .unwrap()
            .call(&mut self.store, &[], &mut result)
            .unwrap();
        self.environment = prepared.environment;
        result[0].clone()
    }
    fn scheduler_counts(&mut self) -> (i32, i32) {
        let mut result = [Val::I32(-1), Val::I32(-1)];
        self.runtime.get_func(&mut self.store, "async-scheduler-counts").unwrap()
            .call(&mut self.store, &[], &mut result).unwrap();
        (result[0].unwrap_i32(), result[1].unwrap_i32())
    }
    fn run(&mut self) {
        self.store.set_fuel(1_000_000).unwrap();
        assert_eq!(self.integer("async-scheduler-run-one", &[]), 1);
    }
    fn ready_number(&mut self, future: &Val) -> f64 {
        assert_eq!(self.integer("future-status", &[future.clone()]), 1);
        let result = self.value("future-result", &[future.clone()]);
        result
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap()
            .field(&mut self.store, 0)
            .unwrap()
            .unwrap_f64()
    }
}

#[test]
fn actual_pending_await_preserves_capture_and_resumes_after_gc() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    let owner = h.source(
        "(let [captured 10] (suss.async/future* (+ captured (suss.async/await* test/dependency))))",
    );
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    h.store.gc(None).unwrap();
    let value = h.value("number-box", &[Val::F64(32f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[dependency, value]), 1);
    h.run();
    assert_eq!(h.ready_number(&owner), 42.0);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn nested_future_and_consecutive_awaits_execute_compiled_states() {
    let mut h = Harness::new();
    let owner = h.source("(suss.async/future* (let [first (suss.async/await* (suss.async/future* 20))] (+ first (suss.async/await* (suss.async/future* 22)))))");
    for _ in 0..5 {
        h.run();
        h.store.gc(None).unwrap();
    }
    assert_eq!(h.ready_number(&owner), 42.0);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn pending_nested_try_binding_and_finally_await_preserve_roots_and_caller() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    let cleanup = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    h.bind("cleanup", cleanup.clone());
    let one = h.value("number-box", &[Val::F64(1f64.to_bits())]);
    h.bind("v", one);
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    h.bind("trace", zero);
    let owner = h.source("(suss.async/future* (binding [test/v 7] (try (try (+ (suss.async/await* test/dependency) test/v) (finally (set! test/trace 5))) (finally (suss.async/await* test/cleanup) (set! test/trace test/v)))))");
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    // Runtime turn restoration must make the caller's binding observable now.
    let caller = h.source("test/v");
    assert_eq!(
        caller
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        1.0
    );
    h.store.gc(None).unwrap();
    let eleven = h.value("number-box", &[Val::F64(11f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[dependency, eleven]), 1);
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    let inner_cleanup = h.source("test/trace");
    assert_eq!(
        inner_cleanup
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        5.0
    );
    h.store.gc(None).unwrap();
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[cleanup, zero]), 1);
    h.run();
    assert_eq!(h.ready_number(&owner), 18.0);
    let outer_cleanup = h.source("test/trace");
    assert_eq!(
        outer_cleanup
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        7.0
    );
    let caller = h.source("test/v");
    assert_eq!(
        caller
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        1.0
    );
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn failed_dependency_enters_handler_then_pending_cleanup_and_keeps_payload() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    let cleanup = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    h.bind("cleanup", cleanup.clone());
    let owner = h.source("(let [captured 10] (suss.async/future* (try (suss.async/await* test/dependency) (catch :default payload (+ captured payload)) (finally (suss.async/await* test/cleanup)))))");
    h.run();
    let payload = h.value("number-box", &[Val::F64(32f64.to_bits())]);
    assert_eq!(h.integer("future-reject", &[dependency, payload]), 1);
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    h.store.gc(None).unwrap();
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[cleanup, zero]), 1);
    h.run();
    assert_eq!(h.ready_number(&owner), 42.0);
}

#[test]
fn invalid_await_registration_throws_to_catch_and_finally_executes_once() {
    let mut h = Harness::new();
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    h.bind("trace", zero);
    let owner = h.source("(suss.async/future* (try (suss.async/await* 42) (catch :default e 7) (finally (set! test/trace (+ test/trace 1)))))");
    h.run();
    assert_eq!(h.ready_number(&owner), 7.0);
    let trace = h.source("test/trace");
    assert_eq!(
        trace
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        1.0
    );
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn body_throw_is_caught_but_cleanup_throw_overrides_the_pending_result_once() {
    let mut h = Harness::new();
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    h.bind("trace", zero);
    let owner = h.source("(suss.async/future* (try (throw 2) (catch :default payload (+ payload 5)) (finally (set! test/trace (+ test/trace 1)) (throw 9))))");
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 2);
    let payload = h.value("future-result", &[owner]);
    assert_eq!(
        payload
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        9.0
    );
    let trace = h.source("test/trace");
    assert_eq!(
        trace
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        1.0
    );
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn cancelled_dependency_is_catchable_and_suspended_finally_completes_once() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    let cleanup = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    h.bind("cleanup", cleanup.clone());
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    h.bind("trace", zero);
    let owner = h.source("(suss.async/future* (try (suss.async/await* test/dependency) (catch :default payload (if (suss.bootstrap/nil? payload) (throw 99) 7)) (finally (suss.async/await* test/cleanup) (set! test/trace (+ test/trace 1)))))");
    h.run();
    assert_eq!(h.integer("future-cancel", &[dependency]), 1);
    h.run();
    // Cancelled dependency is an await failure, not a request to cancel owner.
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    let trace = h.source("test/trace");
    assert_eq!(
        trace
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        0.0
    );
    h.store.gc(None).unwrap();
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[cleanup, zero]), 1);
    h.run();
    assert_eq!(h.ready_number(&owner), 7.0);
    let trace = h.source("test/trace");
    assert_eq!(
        trace
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        1.0
    );
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn own_cancel_bypasses_catch_and_waits_for_finally_before_terminal_cancel() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    let cleanup = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    h.bind("cleanup", cleanup.clone());
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    h.bind("trace", zero);
    let owner = h.source("(suss.async/future* (try (suss.async/await* test/dependency) (catch :default payload (set! test/trace 100)) (finally (suss.async/await* test/cleanup) (set! test/trace (+ test/trace 1)))))");
    h.run();
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    let trace = h.source("test/trace");
    assert_eq!(
        trace
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        0.0
    );
    h.store.gc(None).unwrap();
    // The old body registration must not resume after cancellation replaced it.
    let stale = h.value("number-box", &[Val::F64(99f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[dependency, stale]), 1);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[cleanup, zero]), 1);
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 3);
    h.checked_language_error("future-result", &[owner.clone()]);
    assert_eq!(h.integer("future-status", &[owner.clone()]), 3);
    let trace = h.source("test/trace");
    assert_eq!(
        trace
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        1.0
    );
    assert_eq!(h.integer("async-task-cancel", &[owner]), 0);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn own_cancel_before_first_turn_does_not_execute_body_or_enter_finally() {
    let mut h = Harness::new();
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    h.bind("trace", zero);
    let owner =
        h.source("(suss.async/future* (try (set! test/trace 10) (finally (set! test/trace 100))))");
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    h.store.gc(None).unwrap();
    h.run();
    assert_eq!(h.integer("future-status", &[owner]), 3);
    let trace = h.source("test/trace");
    assert_eq!(
        trace
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap()
            .field(&mut h.store, 0)
            .unwrap()
            .unwrap_f64(),
        0.0
    );
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn owner_cancellation_waits_for_cleanup_and_retires_old_dependency() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    let cleanup = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    h.bind("cleanup", cleanup.clone());
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    h.bind("trace", zero);
    let owner = h.source("(suss.async/future* (try (suss.async/await* test/dependency) (catch :default e (set! test/trace 99)) (finally (suss.async/await* test/cleanup) (set! test/trace (+ test/trace 1)))))");
    h.run();
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
    h.store.gc(None).unwrap();
    let value = h.value("number-box", &[Val::F64(42f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[dependency, value]), 1);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    let value = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[cleanup, value]), 1);
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 3);
    assert_eq!(h.integer("async-task-cancel", &[owner]), 0);
    let trace = h.source("test/trace");
    let object = trace
        .unwrap_anyref()
        .unwrap()
        .as_struct(&h.store)
        .unwrap()
        .unwrap();
    assert_eq!(object.field(&mut h.store, 0).unwrap().unwrap_f64(), 1.0);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn cancellation_during_pending_finally_preserves_remaining_cleanup_and_override() {
    for throws in [false, true] {
        let mut h = Harness::new();
        let cleanup = h.value("future-pending-new", &[]);
        h.bind("cleanup", cleanup.clone());
        let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
        h.bind("trace", zero);
        let end = if throws { "(throw 9)" } else { "nil" };
        let owner = h.source(&format!("(suss.async/future* (try 42 (finally (suss.async/await* test/cleanup) (set! test/trace (+ test/trace 1)) {end})))"));
        h.run();
        assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
        assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
        assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
        // A cancellation notification turn may run, but may not finish cleanup.
        h.integer("async-scheduler-run-one", &[]);
        assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
        h.store.gc(None).unwrap();
        let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
        assert_eq!(h.integer("future-resolve", &[cleanup, zero]), 1);
        h.run();
        assert_eq!(h.integer("future-status", &[owner.clone()]), if throws { 2 } else { 3 });
        if throws {
            let payload = h.value("future-result", &[owner]);
            let object = payload.unwrap_anyref().unwrap().as_struct(&h.store).unwrap().unwrap();
            assert_eq!(object.field(&mut h.store, 0).unwrap().unwrap_f64(), 9.0);
        }
        let trace = h.source("test/trace");
        let object = trace.unwrap_anyref().unwrap().as_struct(&h.store).unwrap().unwrap();
        assert_eq!(object.field(&mut h.store, 0).unwrap().unwrap_f64(), 1.0);
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    }
}

#[test]
fn cpu_backedges_yield_to_completed_dependency_and_preserve_loop_state() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    let worker = h.source("(suss.async/future* (loop [n 0] (if (< n 20) (recur (+ n 1)) n)))");
    let waiter = h.source("(suss.async/future* (suss.async/await* test/dependency))");
    h.run(); // worker yields instead of finishing all iterations
    assert_eq!(h.integer("future-status", &[worker.clone()]), 0);
    h.run(); // waiter registers while worker stays queued
    let value = h.value("number-box", &[Val::F64(42f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[dependency, value]), 1);
    for _ in 0..3 {
        if h.integer("future-status", &[waiter.clone()]) == 1 { break; }
        h.run();
        h.store.gc(None).unwrap();
        let (registrations, queued) = h.scheduler_counts();
        assert!(registrations <= 4 && queued <= 2, "yield roots must remain bounded: {registrations}/{queued}");
    }
    assert_eq!(h.ready_number(&waiter), 42.0);
    assert_eq!(h.integer("future-status", &[worker.clone()]), 0);
    for _ in 0..24 {
        if h.integer("future-status", &[worker.clone()]) == 1 { break; }
        h.run();
        h.store.gc(None).unwrap();
        let (registrations, queued) = h.scheduler_counts();
        assert!(registrations <= 4 && queued <= 2, "yield roots must remain bounded: {registrations}/{queued}");
    }
    assert_eq!(h.ready_number(&worker), 20.0);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn yields_preserve_catch_payload_cleanup_outcome_and_dynamic_frame() {
    let mut h = Harness::new();
    let one = h.value("number-box", &[Val::F64(1f64.to_bits())]);
    h.bind("v", one);
    let zero = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    h.bind("trace", zero);
    let owner = h.source("(suss.async/future* (binding [test/v 7] (try (throw 22) (catch :default payload (loop [n 0] (if (< n 20) (recur (+ n 1)) (+ payload n)))) (finally (loop [n 0] (if (< n 10) (recur (+ n 1)) (set! test/trace test/v)))))))");
    for _ in 0..36 {
        if h.integer("future-status", &[owner.clone()]) == 1 { break; }
        h.run();
        h.store.gc(None).unwrap();
        let caller = h.source("test/v");
        let object = caller.unwrap_anyref().unwrap().as_struct(&h.store).unwrap().unwrap();
        assert_eq!(object.field(&mut h.store, 0).unwrap().unwrap_f64(), 1.0);
    }
    assert_eq!(h.ready_number(&owner), 42.0);
    let trace = h.source("test/trace");
    let object = trace.unwrap_anyref().unwrap().as_struct(&h.store).unwrap().unwrap();
    assert_eq!(object.field(&mut h.store, 0).unwrap().unwrap_f64(), 7.0);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}
