//! Invocation-scoped scheduler isolation using real portable source fragments.
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

struct Harness {
    store: Store<()>,
    runtime: Instance,
    linker: Linker<()>,
    environment: Environment,
    cells: BTreeSet<(String, String)>,
}
impl Harness {
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

#[test]
fn invocation_scope_zero_initializer_is_untouched_by_pump_and_cancel_of_one_and_two() {
    let mut h = Harness::new();
    h.source("(def zero 0) (def one 0) (def two 0) (def clean-one 0) (def clean-two 0) (def initializer (suss.async/future* (set! zero (+ zero 1))))");
    let zero = h.counts(0);
    assert_eq!(zero, (1, 1, 0));
    assert_eq!(h.enter(1), 0);
    h.source("(def dep-one (suss.internal.async/pending)) (def task-one (suss.async/future* (try (set! one (+ one 1)) (suss.async/await* dep-one) (finally (set! clean-one (+ clean-one 1))))))");
    assert_eq!(h.enter(2), 1);
    h.source("(def dep-two (suss.internal.async/pending)) (def task-two (suss.async/future* (try (set! two (+ two 1)) (suss.async/await* dep-two) (finally (set! clean-two (+ clean-two 1))))))");
    assert_eq!(h.enter(0), 2);
    assert!(h.run(1));
    assert!(h.run(2));
    assert_eq!(h.counts(0), zero);
    assert_eq!(h.number("zero"), 0.0);
    assert_eq!(h.number("one"), 1.0);
    assert_eq!(h.number("two"), 1.0);
    assert_eq!(h.counts(1), (1, 0, 0));
    assert_eq!(h.counts(2), (1, 0, 0));
    assert_eq!(h.cancel(1), 1);
    assert_eq!(h.cancel(1), 0);
    assert!(h.run(1));
    assert!(!h.run(1));
    assert_eq!(h.status("task-one"), 3);
    assert_eq!(
        h.status("dep-one"),
        0,
        "cancellation must not globally settle a shared dependency"
    );
    assert_eq!(h.number("clean-one"), 1.0);
    assert_eq!(h.number("clean-two"), 0.0);
    assert_eq!(h.counts(1), (0, 0, 0));
    assert_eq!(h.counts(2), (1, 0, 0));
    assert_eq!(h.counts(0), zero);
    let dependency = h.source("dep-two");
    let payload = h.source("42");
    assert_eq!(h.integer("future-resolve", &[dependency, payload]), 1);
    h.store.gc(None).unwrap();
    assert!(!h.run(1));
    assert!(h.run(2));
    assert!(!h.run(2));
    assert_eq!(h.status("task-two"), 1);
    assert_eq!(h.number("clean-two"), 1.0);
    assert_eq!(h.counts(2), (0, 0, 0));
    assert_eq!(h.counts(0), zero);
    assert_eq!(h.number("zero"), 0.0);
    assert!(h.run(0));
    assert!(!h.run(0));
    assert_eq!(h.number("zero"), 1.0);
    assert_eq!(h.number("one"), 1.0, "consumed source body is not replayed");
    assert_eq!(h.number("two"), 1.0);
    assert_eq!(h.counts(0), (0, 0, 0));
}

#[test]
fn invocation_scope_keys_preserve_all_i32_bits_without_aliasing_scope_zero_or_one() {
    let mut h = Harness::new();
    h.source("(def trace 0) (def initializer (suss.async/future* (set! trace (+ trace 1000))))");
    let keys = [1, i32::MIN, i32::MIN + 1, -1];
    for (index, key) in keys.iter().copied().enumerate() {
        assert_eq!(h.enter(key), 0, "enter returns the exact previous key");
        h.source(&format!(
            "(def task-{index} (suss.async/future* (set! trace (+ trace {}))))",
            1 << index
        ));
        assert_eq!(h.enter(0), key, "previous key must retain its high bits");
        assert_eq!(h.counts(key), (1, 1, 0));
    }
    assert_eq!(h.number("trace"), 0.0);
    for (index, key) in keys.iter().copied().enumerate() {
        assert_eq!(
            h.cancel(key ^ 0x4000_0000),
            0,
            "an unrelated bit pattern cannot cancel this owner"
        );
        assert!(h.run(key));
        assert!(!h.run(key));
        assert_eq!(h.counts(key), (0, 0, 0));
        assert_eq!(h.status(&format!("task-{index}")), 1);
        assert_eq!(h.number("trace"), ((1 << (index + 1)) - 1) as f64);
        assert_eq!(h.counts(0), (1, 1, 0));
        for other in keys.iter().copied().skip(index + 1) {
            assert_eq!(
                h.counts(other),
                (1, 1, 0),
                "pumping one key cannot consume another key"
            );
        }
        assert_eq!(h.enter(key), 0, "run restores the caller's selection");
        assert_eq!(h.enter(0), key);
    }
    assert!(h.run(0));
    assert!(!h.run(0));
    assert_eq!(h.number("trace"), 1015.0);
}

#[test]
fn invocation_scope_descendants_cleanup_and_trap_recovery_restore_caller_without_replay() {
    let mut h = Harness::new();
    h.source("(def trace 0) (def child nil) (def cleanup-child nil)");
    assert_eq!(h.enter(1), 0);
    h.source("(def dependency (suss.internal.async/pending)) (def parent (suss.async/future* (try (set! trace (+ trace 1)) (set! child (suss.async/future* (set! trace (+ trace 10)))) (suss.async/await* dependency) (finally (set! cleanup-child (suss.async/future* (set! trace (+ trace 100))))))))");
    assert_eq!(h.enter(2), 1);
    assert!(h.run(1));
    assert_eq!(h.number("trace"), 1.0);
    assert_eq!(
        h.counts(1),
        (2, 1, 0),
        "descendant inherits executing owner, not caller scope2"
    );
    assert_eq!(h.counts(2), (0, 0, 0));
    assert_eq!(h.enter(3), 2);
    assert_eq!(h.enter(2), 3);
    assert!(h.run(1));
    assert_eq!(h.number("trace"), 11.0);
    assert_eq!(h.status("child"), 1);
    assert_eq!(h.counts(1), (1, 0, 0));
    assert_eq!(h.cancel(1), 1);
    assert!(h.run(1));
    assert_eq!(h.status("parent"), 3);
    assert_eq!(
        h.counts(1),
        (1, 1, 0),
        "cleanup-created child remains in the cancelled invocation"
    );
    assert_eq!(h.counts(2), (0, 0, 0));
    assert_eq!(h.enter(3), 2, "cleanup must restore caller selection");
    assert_eq!(h.enter(2), 3);
    h.store.gc(None).unwrap();
    assert!(h.run(1));
    assert!(!h.run(1));
    assert_eq!(h.status("cleanup-child"), 1);
    assert_eq!(h.number("trace"), 111.0);
    assert_eq!(h.counts(1), (0, 0, 0));

    assert_eq!(h.enter(i32::MIN), 2);
    h.source(
        "(def trapped (suss.async/future* (set! trace (+ trace 1)) ((fn [] (loop [] (recur))))))",
    );
    assert_eq!(h.enter(2), i32::MIN);
    h.store.set_fuel(50_000).unwrap();
    let error = h
        .runtime
        .get_func(&mut h.store, "async-run-invocation")
        .unwrap()
        .call(&mut h.store, &[Val::I32(i32::MIN)], &mut [Val::I32(-1)])
        .expect_err("ordinary synchronous loop must actually exhaust fuel");
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&wasmtime::Trap::OutOfFuel)
    );
    assert!(!h.store.has_pending_exception());
    let counts = h.counts(i32::MIN);
    assert_eq!(counts.0, 1);
    assert_eq!(
        counts.2, 1,
        "trapped invocation remains rooted until real host recovery"
    );
    h.store.set_fuel(1_000_000).unwrap();
    h.runtime
        .get_func(&mut h.store, "async-scheduler-recover")
        .unwrap()
        .call(&mut h.store, &[], &mut [])
        .unwrap();
    assert!(!h.store.has_pending_exception());
    assert_eq!(
        h.enter(3),
        2,
        "real runtime recovery must restore pre-turn caller selection"
    );
    assert_eq!(h.enter(2), 3);
    assert_eq!(h.status("trapped"), 2);
    assert!(!h.run(i32::MIN));
    assert_eq!(h.counts(i32::MIN), (0, 0, 0));
    assert_eq!(h.counts(2), (0, 0, 0));
    assert_eq!(
        h.number("trace"),
        112.0,
        "recovery cannot replay either trapped or cleanup source effects"
    );
    assert_eq!(h.enter(0), 2);
}
