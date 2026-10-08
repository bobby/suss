//! Cancellation-wave metric regressions using real compiled source.
//! Raw runtime/core bindings only: no bootstrap or Session private handles.
use std::{collections::BTreeSet, sync::OnceLock};

use suss_compile::{
    portable::{
        self,
        resolve::{Environment, Phase},
    },
    runtime_abi,
};
use wasmtime::{
    Config, Engine, Global, GlobalType, Instance, Linker, Module, Mutability, RefType, Store, Val,
    ValType,
};

struct RetiringHarness {
    store: Store<()>,
    runtime: Instance,
    linker: Linker<()>,
    environment: Environment,
    cells: BTreeSet<(String, String)>,
}
impl RetiringHarness {
    fn new() -> Self {
        static ENGINE: OnceLock<Engine> = OnceLock::new();
        let engine = ENGINE.get_or_init(|| {
            let mut config = Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).unwrap()
        });
        let mut store = Store::new(engine, ());
        store.set_fuel(1_000_000).unwrap();
        let bytes = runtime_abi::module();
        runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
        let runtime = Instance::new(&mut store, &Module::new(engine, bytes).unwrap(), &[]).unwrap();
        let mut linker = Linker::new(engine);
        linker
            .instance(&mut store, "suss.runtime", runtime)
            .unwrap();
        let bindings = portable::core_bindings::compile(Phase::Runtime).unwrap();
        runtime_abi::verify_artifact(&bindings.wasm, &runtime_abi::Manifest::default()).unwrap();
        let instance = linker
            .instantiate(&mut store, &Module::new(engine, bindings.wasm).unwrap())
            .unwrap();
        let mut cells = BTreeSet::new();
        for identity in bindings.cells {
            let global = instance
                .get_global(&mut store, &identity.import_name())
                .unwrap();
            linker
                .define(
                    &store,
                    identity.import_module(),
                    &identity.import_name(),
                    global,
                )
                .unwrap();
            cells.insert((identity.import_module().to_owned(), identity.import_name()));
        }
        Self {
            store,
            runtime,
            linker,
            environment: Environment::default(),
            cells,
        }
    }
    fn value(&mut self, name: &str, args: &[Val]) -> Val {
        self.store.set_fuel(1_000_000).unwrap();
        let mut result = [Val::null_any_ref()];
        self.runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut result)
            .unwrap();
        result[0].clone()
    }
    fn integer(&mut self, name: &str, args: &[Val]) -> i32 {
        self.store.set_fuel(1_000_000).unwrap();
        let mut result = [Val::I32(-1)];
        self.runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut result)
            .unwrap();
        result[0].unwrap_i32()
    }
    fn enter(&mut self, key: i32) -> i32 {
        self.integer("async-enter-invocation", &[Val::I32(key)])
    }
    fn run(&mut self, key: i32) -> bool {
        match self.integer("async-run-invocation", &[Val::I32(key)]) {
            0 => false,
            1 => true,
            other => panic!("invalid scheduler result {other}"),
        }
    }
    fn cancel(&mut self, key: i32) -> i32 {
        self.integer("async-cancel-invocation", &[Val::I32(key)])
    }
    fn counts(&mut self, key: i32) -> (i32, i32, i32) {
        self.store.set_fuel(1_000_000).unwrap();
        let mut result = [Val::I32(-1), Val::I32(-1), Val::I32(-1)];
        self.runtime
            .get_func(&mut self.store, "async-invocation-counts")
            .unwrap()
            .call(&mut self.store, &[Val::I32(key)], &mut result)
            .unwrap();
        (
            result[0].unwrap_i32(),
            result[1].unwrap_i32(),
            result[2].unwrap_i32(),
        )
    }
    fn source(&mut self, source: &str) -> Val {
        self.store.set_fuel(1_000_000).unwrap();
        let prepared =
            portable::prepare_fragment(source, &self.environment, Phase::Runtime).unwrap();
        for identity in &prepared.cells {
            let key = (identity.import_module().to_owned(), identity.import_name());
            if self.cells.contains(&key) {
                continue;
            }
            let cell = self.value("binding-unbound", &[]);
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
            self.cells.insert(key);
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
    fn number(&mut self, source: &str) -> f64 {
        let value = self.source(source);
        let object = value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap();
        object.field(&mut self.store, 0).unwrap().unwrap_f64()
    }
    fn status(&mut self, source: &str) -> i32 {
        let value = self.source(source);
        self.integer("future-status", &[value])
    }
}

fn retiring(h: &mut RetiringHarness, key: i32) -> i32 {
    h.integer("async-invocation-retiring-count", &[Val::I32(key)])
}

#[test]
fn consumed_latch_counts_once_while_cleanup_awaits_an_uncancelled_child_across_gc() {
    let mut h = RetiringHarness::new();
    h.source("(def trace 0) (def cleanup-child nil) (def dependency (suss.internal.async/pending)) (def cleanup-input (suss.internal.async/pending)) (def initializer (suss.async/future* 88))");
    assert_eq!(h.enter(-1), 0);
    h.source("(def parent (suss.async/future* (try (suss.async/await* dependency) (finally (set! cleanup-child (suss.async/future* (suss.async/await* cleanup-input) (set! trace (+ trace 1)))) (suss.async/await* cleanup-child) (set! trace (+ trace 10))))))");
    assert_eq!(h.enter(2), -1);
    assert!(h.run(-1));
    assert_eq!(retiring(&mut h, -1), 0);
    assert_eq!(h.cancel(-1), 1);
    assert_eq!(retiring(&mut h, -1), 1);
    assert!(h.run(-1)); // Consumes cancellation; finally awaits its new child.
    assert_eq!(h.status("parent"), 0);
    assert_eq!(h.status("cleanup-child"), 0);
    assert_eq!(h.counts(-1).0, 2);
    assert_eq!(
        retiring(&mut h, -1),
        1,
        "consumed latch plus ordinary cleanup waiter identify one retiring owner"
    );
    assert_eq!(retiring(&mut h, 0), 0);
    assert_eq!(retiring(&mut h, 2), 0);
    assert!(h.run(-1)); // Child suspends normally; it has no cancellation latch.
    h.store.gc(None).unwrap();
    assert_eq!(retiring(&mut h, -1), 1);
    assert!(!h.run(-1));
    let input = h.source("cleanup-input");
    let nil = h.value("nil", &[]);
    assert_eq!(h.integer("future-resolve", &[input, nil]), 1);
    assert!(h.run(-1));
    assert_eq!(h.status("cleanup-child"), 1);
    assert_eq!(retiring(&mut h, -1), 1);
    assert!(h.run(-1));
    assert_eq!(h.status("parent"), 3);
    assert_eq!(h.number("trace"), 11.0);
    assert_eq!(retiring(&mut h, -1), 0);
    assert_eq!(h.counts(-1), (0, 0, 0));
    assert_eq!(
        h.status("initializer"),
        0,
        "scope0 never ran or received cancellation"
    );
    assert_eq!(h.enter(0), 2, "caller selection survived cleanup turns");
}

#[test]
fn active_consumed_cancellation_latch_counts_until_trap_recovery_publishes_terminal() {
    let mut h = RetiringHarness::new();
    h.source("(def trace 0) (def dependency (suss.internal.async/pending))");
    assert_eq!(h.enter(i32::MIN), 0);
    h.source("(def parent (suss.async/future* (try (suss.async/await* dependency) (finally (set! trace (+ trace 1)) ((fn [] (loop [] (recur))))))))");
    assert_eq!(h.enter(2), i32::MIN);
    assert!(h.run(i32::MIN));
    assert_eq!(h.cancel(i32::MIN), 1);
    h.store.set_fuel(100_000).unwrap();
    let error = h
        .runtime
        .get_func(&mut h.store, "async-run-invocation")
        .unwrap()
        .call(&mut h.store, &[Val::I32(i32::MIN)], &mut [Val::I32(0)])
        .unwrap_err();
    assert!(
        error.downcast_ref::<wasmtime::Trap>().is_some(),
        "preserve native trap: {error:?}"
    );
    assert_eq!(h.counts(i32::MIN), (1, 0, 1));
    assert_eq!(
        retiring(&mut h, i32::MIN),
        1,
        "active owner still has its consumed phase2 cancellation latch"
    );
    assert_eq!(
        h.cancel(i32::MIN),
        0,
        "consumed latch prevents duplicate acceptance"
    );
    h.store.gc(None).unwrap();
    assert_eq!(retiring(&mut h, i32::MIN), 1);
    h.store.set_fuel(1_000_000).unwrap();
    h.runtime
        .get_func(&mut h.store, "async-scheduler-recover")
        .unwrap()
        .call(&mut h.store, &[], &mut [])
        .unwrap();
    assert_eq!(h.status("parent"), 2);
    let payload = h.source("parent");
    let payload = h.value("future-result", &[payload]);
    assert_eq!(h.integer("async-runtime-trap-is", &[payload]), 1);
    assert_eq!(retiring(&mut h, i32::MIN), 0);
    assert_eq!(h.counts(i32::MIN), (0, 0, 0));
    assert_eq!(h.enter(0), 2, "trap recovery restored caller selection");
    assert_eq!(h.number("trace"), 1.0);
    assert!(!h.run(i32::MIN));
    assert_eq!(
        h.number("trace"),
        1.0,
        "recovery never replays cleanup effects"
    );
}

#[test]
fn active_request_is_counted_before_dispatch_and_deduplicates_ordinary_owner_rows() {
    let mut h = RetiringHarness::new();
    h.source("(def trace 0)");
    assert_eq!(h.enter(1), 0);
    h.source("(def parent (suss.async/future* (set! trace (+ trace 1)) (suss.async/cancel! parent) ((fn [] (loop [] (recur))))))");
    assert_eq!(h.enter(2), 1);
    h.store.set_fuel(100_000).unwrap();
    let error = h
        .runtime
        .get_func(&mut h.store, "async-run-invocation")
        .unwrap()
        .call(&mut h.store, &[Val::I32(1)], &mut [Val::I32(0)])
        .unwrap_err();
    assert!(error.downcast_ref::<wasmtime::Trap>().is_some());
    assert_eq!(h.counts(1), (1, 1, 1));
    assert_eq!(
        retiring(&mut h, 1),
        1,
        "active ordinary token and queued cancellation identify one owner"
    );
    assert_eq!(h.cancel(1), 0);
    assert_eq!(retiring(&mut h, 1), 1);
    h.store.gc(None).unwrap();
    assert_eq!(retiring(&mut h, 1), 1);
    h.store.set_fuel(1_000_000).unwrap();
    h.runtime
        .get_func(&mut h.store, "async-scheduler-recover")
        .unwrap()
        .call(&mut h.store, &[], &mut [])
        .unwrap();
    assert_eq!(retiring(&mut h, 1), 0);
    assert_eq!(h.counts(1), (0, 0, 0));
    assert_eq!(h.enter(0), 2);
    assert_eq!(h.number("trace"), 1.0);
    assert!(!h.run(1));
}

#[test]
fn interrupted_scoped_cancellation_publishes_whole_latch_and_retries_without_scope_aliasing() {
    let mut saw_interrupted = false;
    let mut saw_published = false;
    for budget in (0..1200).step_by(20).chain(std::iter::once(10_000)) {
        let mut h = RetiringHarness::new();
        h.source("(def trace 0) (def dependency (suss.internal.async/pending)) (def initializer (suss.async/future* 77))");
        assert_eq!(h.enter(-1), 0);
        h.source("(def parent (suss.async/future* (try (suss.async/await* dependency) (finally (set! trace (+ trace 1))))))");
        assert_eq!(h.enter(2), -1);
        assert!(h.run(-1));
        h.store.set_fuel(budget).unwrap();
        let outcome = h
            .runtime
            .get_func(&mut h.store, "async-cancel-invocation")
            .unwrap()
            .call(&mut h.store, &[Val::I32(-1)], &mut [Val::I32(0)]);
        if outcome.is_err() {
            saw_interrupted = true;
        }
        h.store.gc(None).unwrap();
        let latched = retiring(&mut h, -1);
        assert!(latched == 0 || latched == 1, "budget={budget}");
        saw_published |= latched == 1;
        assert_eq!(h.counts(-1).0, 1);
        assert_eq!(retiring(&mut h, 0), 0);
        assert_eq!(retiring(&mut h, 2), 0);
        assert_eq!(
            h.cancel(-1),
            1 - latched,
            "partial publication cannot accept a second request, budget={budget}"
        );
        assert_eq!(retiring(&mut h, -1), 1);
        assert!(h.run(-1));
        assert_eq!(h.status("parent"), 3);
        assert_eq!(h.number("trace"), 1.0);
        assert_eq!(retiring(&mut h, -1), 0);
        assert_eq!(h.counts(-1), (0, 0, 0));
        assert_eq!(h.status("initializer"), 0);
        assert_eq!(h.enter(0), 2);
        assert!(!h.run(-1));
    }
    assert!(saw_interrupted && saw_published);
}
