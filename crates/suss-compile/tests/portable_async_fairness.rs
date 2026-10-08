//! Execute cooperative compiler backedges alongside genuinely pending I/O.
//! Fuel bounds failures; normal FIFO yields, not fuel traps, provide fairness.
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

impl Harness {
    fn source_number(&mut self, source: &str) -> f64 {
        let value = self.source(source);
        value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap()
            .field(&mut self.store, 0)
            .unwrap()
            .unwrap_f64()
    }
    fn counts(&mut self) -> (i32, i32) {
        self.store.set_fuel(1_000_000).unwrap();
        let mut result = [Val::I32(-1), Val::I32(-1)];
        self.runtime
            .get_func(&mut self.store, "async-scheduler-counts")
            .unwrap()
            .call(&mut self.store, &[], &mut result)
            .unwrap();
        (result[0].unwrap_i32(), result[1].unwrap_i32())
    }
    fn bind_number(&mut self, name: &str, value: f64) {
        let boxed = self.value("number-box", &[Val::F64(value.to_bits())]);
        self.bind(name, boxed);
    }
}

#[test]
fn computing_loop_yields_to_completed_pending_io_before_cpu_finishes() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    h.bind_number("counter", 0.0);
    let io = h.source("(suss.async/future* (+ 1 (suss.async/await* test/dependency)))");
    h.run();
    assert_eq!(h.integer("future-status", &[io.clone()]), 0);
    let cpu = h.source("(suss.async/future* (loop [n 0] (if (< n 128) (do (set! test/counter (+ test/counter 1)) (recur (+ n 1))) n)))");
    let completed = h.value("number-box", &[Val::F64(41f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[dependency, completed]), 1);
    // CPU was queued first. It must return normally at its first backedge,
    // placing its next quantum behind the now completed I/O continuation.
    h.run();
    assert_eq!(h.source_number("test/counter"), 1.0);
    assert_eq!(h.integer("future-status", &[cpu.clone()]), 0);
    h.store.gc(None).unwrap();
    h.run();
    assert_eq!(h.ready_number(&io), 42.0);
    assert_eq!(h.integer("future-status", &[cpu.clone()]), 0);
    assert_eq!(h.source_number("test/counter"), 1.0);
    for _ in 0..128 {
        h.store.gc(None).unwrap();
        h.run();
        let (rows, queue) = h.counts();
        assert!(
            rows <= 4 && queue <= 2,
            "yield roots must stay bounded: {rows}/{queue}"
        );
    }
    assert_eq!(h.ready_number(&cpu), 128.0);
    assert_eq!(h.source_number("test/counter"), 128.0);
    assert_eq!(h.counts(), (0, 0));
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn loop_yield_preserves_parallel_parameters_capture_dynamic_and_finally_across_gc() {
    let mut h = Harness::new();
    h.bind_number("v", 1.0);
    h.bind_number("trace", 0.0);
    h.bind_number("cleanup", 0.0);
    let owner = h.source("(let [captured 7] (suss.async/future* (binding [test/v 5] (try (loop [n 0 a 1 b 2] (if (< n 4) (do (set! test/trace (+ test/trace test/v)) (recur (+ n 1) b a)) (+ captured (+ (* a 10) b)))) (finally (set! test/cleanup (+ test/cleanup 1)))))))");
    for quantum in 1..=4 {
        h.run();
        assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
        assert_eq!(h.source_number("test/trace"), quantum as f64 * 5.0);
        assert_eq!(h.source_number("test/v"), 1.0);
        assert_eq!(h.source_number("test/cleanup"), 0.0);
        h.store.gc(None).unwrap();
    }
    h.run();
    assert_eq!(h.ready_number(&owner), 19.0);
    assert_eq!(h.source_number("test/v"), 1.0);
    assert_eq!(h.source_number("test/cleanup"), 1.0);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn cancellation_at_yield_does_not_replay_loop_and_waits_for_rooted_cleanup() {
    let mut h = Harness::new();
    let cleanup = h.value("future-pending-new", &[]);
    h.bind("cleanup", cleanup.clone());
    h.bind_number("counter", 0.0);
    h.bind_number("caught", 0.0);
    h.bind_number("cleaned", 0.0);
    let owner = h.source("(suss.async/future* (try (loop [n 0] (if (< n 128) (do (set! test/counter (+ test/counter 1)) (recur (+ n 1))) n)) (catch :default e (set! test/caught 100)) (finally (suss.async/await* test/cleanup) (set! test/cleaned (+ test/cleaned 1)))))");
    h.run();
    assert_eq!(h.source_number("test/counter"), 1.0);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    assert_eq!(h.source_number("test/counter"), 1.0);
    assert_eq!(h.source_number("test/caught"), 0.0);
    assert_eq!(h.source_number("test/cleaned"), 0.0);
    h.store.gc(None).unwrap();
    let value = h.value("number-box", &[Val::F64(0f64.to_bits())]);
    assert_eq!(h.integer("future-resolve", &[cleanup, value]), 1);
    h.run();
    assert_eq!(h.integer("future-status", &[owner.clone()]), 3);
    h.checked_language_error("future-result", &[owner]);
    assert_eq!(h.source_number("test/counter"), 1.0);
    assert_eq!(h.source_number("test/caught"), 0.0);
    assert_eq!(h.source_number("test/cleaned"), 1.0);
    assert_eq!(h.counts(), (0, 0));
}
