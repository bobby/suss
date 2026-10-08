//! Portable stream journals through real ABI2 runtime and compiled source.
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
        static RUNTIME: OnceLock<Module> = OnceLock::new();
        let runtime = Instance::new(
            &mut store,
            RUNTIME.get_or_init(|| Module::new(engine, bytes).unwrap()),
            &[],
        )
        .unwrap();
        let mut linker = Linker::new(engine);
        linker
            .instance(&mut store, "suss.runtime", runtime)
            .unwrap();
        let bindings = portable::core_bindings::compile(Phase::Runtime).unwrap();
        runtime_abi::verify_artifact(&bindings.wasm, &runtime_abi::Manifest::default()).unwrap();
        static BINDINGS: OnceLock<Module> = OnceLock::new();
        let instance = linker
            .instantiate(
                &mut store,
                BINDINGS.get_or_init(|| Module::new(engine, bindings.wasm).unwrap()),
            )
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

impl Harness {
    fn void(&mut self, name: &str, args: &[Val]) {
        self.store.set_fuel(1_000_000).unwrap();
        self.runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut [])
            .unwrap();
    }
    fn pending(&mut self) -> i32 {
        self.integer("stream-pending-count", &[])
    }
    fn setup(&mut self) {
        self.enter(1);
        self.source("(def pair (suss.internal.async/stream-pair 1)) (def reader (suss.internal.async/stream-pair-reader pair)) (def writer (suss.internal.async/stream-pair-writer pair)) (def op nil) (def trace 0) (def cleanup 0) (def gate (suss.internal.async/pending))");
    }
    fn write_task(&mut self, value: i32) {
        self.source(&format!("(def chunk (suss.internal.async/stream-chunk-new 1)) (suss.internal.async/stream-chunk-set chunk 0 {value}) (def task (suss.async/future* (set! op (suss.internal.async/stream-begin-write writer chunk)) (suss.async/await* (suss.internal.async/stream-operation-future op))))"));
    }
    fn drain(&mut self) {
        for _ in 0..64 {
            if !self.run(1) {
                self.void("stream-service", &[]);
                return;
            }
        }
        panic!("stream cleanup failed to quiesce within 64 source turns");
    }
}
#[test]
fn stream_trapped_owner_orphan_withdraws_write_without_running_finally_or_accepting_data() {
    let mut h = Harness::new();
    h.setup();
    h.source("(def chunk (suss.internal.async/stream-chunk-new 1)) (suss.internal.async/stream-chunk-set chunk 0 17) (def task (suss.async/future* (try (set! trace (+ trace 1)) (set! op (suss.internal.async/stream-begin-write writer chunk)) ((fn [] (loop [] (recur)))) (finally (set! cleanup (+ cleanup 1))))))");
    h.store.set_fuel(50_000).unwrap();
    let mut output = [Val::I32(-1)];
    let error = h
        .runtime
        .get_func(&mut h.store, "async-run-invocation")
        .unwrap()
        .call(&mut h.store, &[Val::I32(1)], &mut output)
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&wasmtime::Trap::OutOfFuel)
    );
    h.void("async-scheduler-recover", &[]);
    assert_eq!(h.pending(), 0);
    assert_eq!(h.status("task"), 2);
    assert_eq!(
        h.status("(suss.internal.async/stream-operation-future op)"),
        3
    );
    assert_eq!(h.number("trace"), 1.0);
    assert_eq!(
        h.number("cleanup"),
        0.0,
        "engine trap bypasses source finally"
    );
    assert!(!h.run(1), "consumed trapped callback cannot replay");
    h.source("(def read-op nil) (def read-task (suss.async/future* (set! read-op (suss.internal.async/stream-begin-read reader 1)) (suss.async/await* (suss.internal.async/stream-operation-future read-op))))");
    assert!(h.run(1));
    assert_eq!(
        h.status("read-task"),
        0,
        "orphan write must not enter buffer"
    );
    h.write_task(23);
    h.drain();
    assert_eq!(
        h.number("(suss.internal.async/stream-chunk-nth (suss.internal.async/result read-task) 0)"),
        23.0
    );
    assert_eq!(h.pending(), 0);
}
#[test]
fn stream_consumed_cancel_latch_withdraws_preexisting_operation_before_consumption() {
    let mut h = Harness::new();
    h.setup();
    h.source("(def task (suss.async/future* (try (set! op (suss.internal.async/stream-begin-read reader 1)) (suss.async/await* gate) (finally (set! cleanup (+ cleanup 1)) (suss.async/await* gate)))))");
    assert!(h.run(1));
    let task = h.source("task");
    assert_eq!(h.integer("async-task-cancel", &[task.clone()]), 1);
    assert!(h.run(1));
    assert_eq!(
        h.status("task"),
        0,
        "cleanup still waits, owner is not terminal"
    );
    assert_eq!(
        h.integer("async-task-cancel-requested", &[task.clone()]),
        1,
        "must execute consumed cancellation phase"
    );
    assert_eq!(
        h.status("(suss.internal.async/stream-operation-future op)"),
        3
    );
    assert_eq!(h.pending(), 0);
    h.write_task(17);
    h.drain();
    h.source("(def read-task (suss.async/future* (suss.async/await* (suss.internal.async/stream-operation-future (suss.internal.async/stream-begin-read reader 1)))))");
    h.drain();
    assert_eq!(
        h.number("(suss.internal.async/stream-chunk-nth (suss.internal.async/result read-task) 0)"),
        17.0,
        "cancelled read must not consume accepted data"
    );
    let gate = h.source("gate");
    let nil = h.value("nil", &[]);
    h.integer("future-resolve", &[gate, nil]);
    h.drain();
    assert_eq!(h.number("cleanup"), 1.0);
    assert_eq!(h.integer("future-status", &[task]), 3);
    assert_eq!(h.integer("async-scheduler-pending-task-count", &[]), 0);
}
#[test]
fn stream_private_future_source_settlement_guards_preserve_pending_and_reset_roots() {
    let mut h = Harness::new();
    h.setup();
    h.source("(def task (suss.async/future* (set! op (suss.internal.async/stream-begin-read reader 1)) (suss.async/await* (suss.internal.async/stream-operation-future op))))");
    assert!(h.run(1));
    let future = h.source("(suss.internal.async/stream-operation-future op)");
    assert_eq!(h.integer("stream-operation-owned", &[future]), 1);
    for operation in ["resolve!", "reject!", "cancel!"] {
        let payload = if operation == "cancel!" { "" } else { " 99" };
        assert_eq!(h.number(&format!("(try (suss.internal.async/{operation} (suss.internal.async/stream-operation-future op){payload}) 0 (catch :default error 1))")),1.0);
        assert_eq!(
            h.status("(suss.internal.async/stream-operation-future op)"),
            0,
            "{operation} must not change private operation storage"
        );
    }
    assert_eq!(
        h.status("(suss.internal.async/stream-operation-future op)"),
        0
    );
    assert_eq!(h.integer("async-scheduler-cancel-all", &[]), 1);
    h.drain();
    assert_eq!(h.pending(), 0);
    assert_eq!(h.integer("async-scheduler-pending-task-count", &[]), 0);
    h.source("(set! op nil) (set! task nil) (set! pair nil) (set! reader nil) (set! writer nil)");
    h.store.gc(None).unwrap();
    assert_eq!(h.pending(), 0, "GC is not the cancellation mechanism");
}

#[test]
fn production_runtime_stream_exports_validate_before_any_source_execution() {
    let mut h = Harness::new(); // Module::new validates the actual shared runtime.
    for export in [
        "stream-pair-new",
        "stream-pair-reader",
        "stream-pair-writer",
        "stream-read-limit",
        "stream-write-limit",
        "stream-begin-read",
        "stream-begin-write",
        "stream-operation-future",
        "stream-operation-retire",
        "stream-chunk-new",
        "stream-chunk-set",
        "stream-chunk-count",
        "stream-chunk-nth",
        "stream-eof-is",
        "stream-close",
        "stream-fail",
        "stream-value-kind",
        "stream-operation-owned",
        "stream-service",
        "stream-pending-count",
    ] {
        assert!(
            h.runtime.get_func(&mut h.store, export).is_some(),
            "missing {export}"
        );
    }
}

#[test]
fn stream_service_fuel_sweep_retains_live_owner_and_delivers_eof_once() {
    let mut traps = 0;
    let mut finished = 0;
    for fuel in [0, 1, 8, 32, 128, 512, 2048, 8192, 32768, 131072] {
        let mut h = Harness::new();
        h.setup();
        h.source("(def task (suss.async/future* (set! op (suss.internal.async/stream-begin-read reader 1)) (suss.async/await* (suss.internal.async/stream-operation-future op)) (set! trace (+ trace 1))))");
        assert!(h.run(1));
        assert_eq!(h.status("task"), 0);
        assert_eq!(h.pending(), 1);
        let writer = h.source("writer");
        assert_eq!(h.integer("stream-close", &[writer]), 1);
        h.store.set_fuel(fuel).unwrap();
        let result = h
            .runtime
            .get_func(&mut h.store, "stream-service")
            .unwrap()
            .call(&mut h.store, &[], &mut []);
        match result {
            Err(error) => {
                assert_eq!(
                    error.downcast_ref::<wasmtime::Trap>(),
                    Some(&wasmtime::Trap::OutOfFuel),
                    "fuel={fuel}: {error:#}"
                );
                traps += 1;
                // No source callback is active: recover the producer journal
                // without failing or replaying the still-awaiting owner.
                h.void("async-scheduler-recover", &[]);
            }
            Ok(()) => finished += 1,
        }
        h.void("stream-service", &[]);
        h.drain();
        assert_eq!(
            h.status("task"),
            1,
            "fuel={fuel}: owner must resume successfully"
        );
        assert_eq!(
            h.number("trace"),
            1.0,
            "fuel={fuel}: once-only resumed effects"
        );
        assert_eq!(h.pending(), 0);
        let payload = h.source(
            "(suss.internal.async/result (suss.internal.async/stream-operation-future op))",
        );
        assert_eq!(h.integer("stream-eof-is", &[payload]), 1);
        h.void("stream-service", &[]);
        assert!(!h.run(1));
        assert_eq!(h.number("trace"), 1.0);
    }
    assert!(
        traps > 0 && finished > 0,
        "must execute interrupted and completed service paths"
    );
}

#[test]
fn stream_active_owner_fuel_sweep_recovers_orphan_without_duplicate_acceptance() {
    let mut traps = 0;
    let mut trapped_after_begin = 0;
    for fuel in [0, 1, 32, 128, 512, 2048, 8192, 32768, 131072] {
        let mut h = Harness::new();
        h.setup();
        h.source("(def chunk (suss.internal.async/stream-chunk-new 1)) (suss.internal.async/stream-chunk-set chunk 0 17) (def task (suss.async/future* (set! trace (+ trace 1)) (set! op (suss.internal.async/stream-begin-write writer chunk)) ((fn [] (loop [] (recur))))))");
        h.store.set_fuel(fuel).unwrap();
        let mut output = [Val::I32(-1)];
        let result = h
            .runtime
            .get_func(&mut h.store, "async-run-invocation")
            .unwrap()
            .call(&mut h.store, &[Val::I32(1)], &mut output);
        if let Err(error) = result {
            assert_eq!(
                error.downcast_ref::<wasmtime::Trap>(),
                Some(&wasmtime::Trap::OutOfFuel),
                "fuel={fuel}: {error:#}"
            );
            traps += 1;
            if h.pending() > 0 {
                trapped_after_begin += 1;
            }
            h.void("async-scheduler-recover", &[]);
        }
        // Entry exhaustion may leave the callback unconsumed. Force that same
        // callback to trap after begin with a bounded, sufficient retry budget.
        if h.status("task") == 0 {
            h.store.set_fuel(200_000).unwrap();
            let error = h
                .runtime
                .get_func(&mut h.store, "async-run-invocation")
                .unwrap()
                .call(&mut h.store, &[Val::I32(1)], &mut output)
                .unwrap_err();
            assert_eq!(
                error.downcast_ref::<wasmtime::Trap>(),
                Some(&wasmtime::Trap::OutOfFuel)
            );
            if h.pending() > 0 {
                trapped_after_begin += 1;
            }
            h.void("async-scheduler-recover", &[]);
        }
        h.void("stream-service", &[]);
        assert_eq!(h.status("task"), 2);
        assert!(h.number("trace") <= 1.0, "consumed callback cannot replay");
        assert_eq!(
            h.pending(),
            0,
            "fuel={fuel}: failed owner operation must retire"
        );
        h.source("(def read-task (suss.async/future* (suss.async/await* (suss.internal.async/stream-operation-future (suss.internal.async/stream-begin-read reader 1)))))");
        assert!(h.run(1));
        assert_eq!(
            h.status("read-task"),
            0,
            "uncommitted orphan write cannot appear in buffer"
        );
        h.write_task(23);
        h.drain();
        assert_eq!(
            h.number(
                "(suss.internal.async/stream-chunk-nth (suss.internal.async/result read-task) 0)"
            ),
            23.0
        );
        assert_eq!(h.pending(), 0);
    }
    assert!(
        traps > 0 && trapped_after_begin > 0,
        "must trap with a genuinely rooted operation and active owner"
    );
}

#[test]
fn interrupted_read_journal_is_recovered_before_new_close_mutates_snapshot() {
    let mut interrupted_owned = 0;
    // Sweep across source dispatch, operation creation, transfer preparation and
    // publication. Only genuinely rooted live-owner interruptions are proof.
    for fuel in (256..=32768).step_by(256) {
        let mut h = Harness::new();
        h.setup();
        h.write_task(17);
        h.drain();
        let writer = h.source("writer");
        h.source("(def task (suss.async/future* (set! op (suss.internal.async/stream-begin-read reader 1)) (suss.async/await* gate)))");
        h.store.set_fuel(fuel).unwrap();
        let mut output = [Val::I32(-1)];
        let result = h
            .runtime
            .get_func(&mut h.store, "async-run-invocation")
            .unwrap()
            .call(&mut h.store, &[Val::I32(1)], &mut output);
        let Err(error) = result else {
            continue;
        };
        assert_eq!(
            error.downcast_ref::<wasmtime::Trap>(),
            Some(&wasmtime::Trap::OutOfFuel),
            "fuel={fuel}: {error:#}"
        );
        if h.pending() == 0 {
            continue;
        }
        let operation = h.source("op");
        let future = h.value("stream-operation-future", &[operation]);
        if h.integer("future-status", &[future.clone()]) != 0 {
            continue;
        }
        interrupted_owned += 1;
        // Deliberately call a NEW public mutation before scheduler recovery or
        // explicit stream-service. Its barrier must preserve the old receipt.
        assert_eq!(h.integer("stream-close", &[writer]), 1);
        assert_eq!(h.integer("future-status", &[future.clone()]), 1);
        let chunk = h.value("future-result", &[future]);
        let zero = h.value("number-box", &[Val::F64(0.0f64.to_bits())]);
        let item = h.value("stream-chunk-nth", &[chunk, zero]);
        assert_eq!(
            item.unwrap_anyref()
                .unwrap()
                .as_struct(&h.store)
                .unwrap()
                .unwrap()
                .field(&mut h.store, 0)
                .unwrap()
                .unwrap_f64(),
            17.0
        );
        h.void("async-scheduler-recover", &[]);
        h.source("(def next-task (suss.async/future* (suss.async/await* (suss.internal.async/stream-operation-future (suss.internal.async/stream-begin-read reader 1)))))");
        h.drain();
        let next = h.source("(suss.internal.async/result next-task)");
        assert_eq!(
            h.integer("stream-eof-is", &[next]),
            1,
            "fuel={fuel}: consumption must not replay after close changes the snapshot"
        );
        assert_eq!(h.pending(), 0);
    }
    assert!(
        interrupted_owned > 0,
        "must execute a service interruption with a live rooted read operation; entry-only exhaustion is insufficient"
    );
}

#[test]
fn stream_operation_created_after_cancel_dispatch_finishes_awaited_finally() {
    let mut h = Harness::new();
    h.setup();
    h.source("(def task (suss.async/future* (try (suss.async/await* gate) (finally (set! op (suss.internal.async/stream-begin-read reader 1)) (let [chunk (suss.async/await* (suss.internal.async/stream-operation-future op))] (set! trace (suss.internal.async/stream-chunk-nth chunk 0))) (set! cleanup (+ cleanup 1))))))");
    assert!(h.run(1));
    let owner = h.source("task");
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
    assert!(h.run(1));
    assert_eq!(
        h.integer("future-status", &[owner.clone()]),
        0,
        "cancelled owner is still awaiting real stream cleanup"
    );
    assert_eq!(
        h.integer("async-task-cancel-requested", &[owner.clone()]),
        1
    );
    assert_eq!(
        h.integer("async-owner-cancellation-pending", &[owner.clone()]),
        0,
        "request is consumed, latch retained"
    );
    let op = h.source("op");
    let completion = h.value("stream-operation-future", &[op]);
    assert_eq!(
        h.integer("future-status", &[completion.clone()]),
        0,
        "new cleanup operation must not be withdrawn by the old latch"
    );
    assert_eq!(h.pending(), 1);
    assert_eq!(h.number("cleanup"), 0.0);
    h.write_task(17);
    h.drain();
    assert_eq!(h.integer("future-status", &[completion]), 1);
    assert_eq!(
        h.integer("future-status", &[owner]),
        3,
        "awaited finally finishes before owner cancellation publishes"
    );
    assert_eq!(h.number("trace"), 17.0);
    assert_eq!(h.number("cleanup"), 1.0);
    assert_eq!(h.pending(), 0);
    assert_eq!(h.integer("async-scheduler-pending-task-count", &[]), 0);
    assert!(!h.run(1));
    assert_eq!(h.number("cleanup"), 1.0);
}

#[test]
fn later_published_write_receipt_survives_earlier_read_service_or_retirement() {
    for retire_read in [false, true] {
        let mut witnessed = 0;
        for fuel in (256..=49152).step_by(64) {
            let mut h = Harness::new();
            h.setup();
            h.source("(def read-op nil) (def reading (suss.async/future* (set! read-op (suss.internal.async/stream-begin-read reader 1)) (suss.async/await* gate)))");
            assert!(h.run(1));
            h.source("(def write-op nil) (def chunk (suss.internal.async/stream-chunk-new 1)) (suss.internal.async/stream-chunk-set chunk 0 17) (def writing (suss.async/future* (set! write-op (suss.internal.async/stream-begin-write writer chunk)) (suss.async/await* gate)))");
            h.store.set_fuel(fuel).unwrap();
            let result = h
                .runtime
                .get_func(&mut h.store, "async-run-invocation")
                .unwrap()
                .call(&mut h.store, &[Val::I32(1)], &mut [Val::I32(0)]);
            let Err(error) = result else { continue };
            assert_eq!(
                error.downcast_ref::<wasmtime::Trap>(),
                Some(&wasmtime::Trap::OutOfFuel)
            );
            // Read the pending endpoint token even if fuel ran out before the
            // source assignment following begin-write returned.
            let writer = h.source("writer");
            let endpoint = writer
                .unwrap_anyref()
                .unwrap()
                .as_struct(&h.store)
                .unwrap()
                .unwrap();
            let endpoint_fields = endpoint
                .field(&mut h.store, 1)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_array(&h.store)
                .unwrap()
                .unwrap();
            let state = endpoint_fields
                .get(&mut h.store, 0)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_struct(&h.store)
                .unwrap()
                .unwrap();
            let state_fields = state
                .field(&mut h.store, 1)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_array(&h.store)
                .unwrap()
                .unwrap();
            let snapshot = state_fields
                .get(&mut h.store, 0)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_array(&h.store)
                .unwrap()
                .unwrap();
            let operation = snapshot.get(&mut h.store, 6).unwrap();
            let Some(object) = operation
                .unwrap_anyref()
                .unwrap()
                .as_struct(&h.store)
                .unwrap()
            else {
                continue;
            };
            let fields = object
                .field(&mut h.store, 1)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_array(&h.store)
                .unwrap()
                .unwrap();
            let phase = fields
                .get(&mut h.store, 6)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_i31(&h.store)
                .unwrap()
                .unwrap()
                .get_u32();
            if phase != 1 {
                continue;
            }
            // Source genuinely exhausted fuel with a registered live owner.
            // Publish its transfer through the real runtime, then retain a
            // phase-two receipt as a deterministic interruption fixture. The
            // pinned engine's fuel sweep did not expose this exact instruction
            // window; this test proves recovery ordering, not that fuel window.
            h.void("stream-prepare-op", &[operation.clone()]);
            h.void("stream-commit-op", &[operation.clone()]);
            let phase = fields
                .get(&mut h.store, 6)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_i31(&h.store)
                .unwrap()
                .unwrap()
                .get_u32();
            assert_eq!(phase, 3);
            let phase_two =
                wasmtime::AnyRef::from_i31(&mut h.store, wasmtime::I31::new_u32(2).unwrap());
            fields
                .set(&mut h.store, 6, Val::AnyRef(Some(phase_two)))
                .unwrap();
            let completion = h.value("stream-operation-future", &[operation]);
            assert_eq!(h.integer("future-status", &[completion.clone()]), 0);
            witnessed += 1;
            if retire_read {
                let read = h.source("read-op");
                assert_eq!(h.integer("stream-operation-retire", &[read]), 1);
            } else {
                h.void("stream-service", &[]);
            }
            h.void("stream-service", &[]);
            assert_eq!(
                h.integer("future-status", &[completion]),
                1,
                "fuel={fuel} retire={retire_read}: accepted write must settle without replay"
            );
            h.void("async-scheduler-recover", &[]);
            if retire_read {
                h.source("(def read-after (suss.async/future* (set! read-op (suss.internal.async/stream-begin-read reader 1)) (suss.async/await* gate)))");
                assert!(h.run(1));
                h.void("stream-service", &[]);
            }
            let read = h.source("read-op");
            let read_future = h.value("stream-operation-future", &[read]);
            assert_eq!(h.integer("future-status", &[read_future.clone()]), 1);
            let data = h.value("future-result", &[read_future]);
            let zero = h.value("number-box", &[Val::F64(0.0f64.to_bits())]);
            let item = h.value("stream-chunk-nth", &[data, zero]);
            assert_eq!(
                item.unwrap_anyref()
                    .unwrap()
                    .as_struct(&h.store)
                    .unwrap()
                    .unwrap()
                    .field(&mut h.store, 0)
                    .unwrap()
                    .unwrap_f64(),
                17.0
            );
            h.source("(def probe-op nil) (def probe (suss.async/future* (set! probe-op (suss.internal.async/stream-begin-read reader 1)) (suss.async/await* gate)))");
            assert!(h.run(1));
            h.void("stream-service", &[]);
            let probe = h.source("probe-op");
            let probe_future = h.value("stream-operation-future", &[probe]);
            assert_eq!(
                h.integer("future-status", &[probe_future]),
                0,
                "fuel={fuel} retire={retire_read}: accepted value was delivered twice"
            );
            let writer = h.source("writer");
            h.integer("stream-close", &[writer]);
            h.integer("async-scheduler-cancel-all", &[]);
            h.drain();
            assert_eq!(h.pending(), 0);
            break;
        }
        assert!(witnessed > 0, "must create receipt fixture from a genuinely fuel-interrupted registered owner (retire={retire_read})");
    }
}
