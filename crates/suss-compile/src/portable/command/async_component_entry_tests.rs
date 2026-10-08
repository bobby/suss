//! Production driver tests with actual compiled source/runtime and core host
//! shims. These do not claim canonical late-callback or allocation acceptance.
use super::driver;
use crate::{
    portable::{
        self, core_bindings,
        resolve::{Environment, Global, Phase},
    },
    runtime_abi,
};
use std::sync::OnceLock;
use wasmtime::{Config, Engine, Instance, Linker, Module, Store, Val};

#[derive(Default)]
struct Host {
    registrations: Vec<(i32, i32)>,
    retirements: Vec<i32>,
    returned: Vec<i32>,
}
struct Fixture {
    engine: Engine,
    runtime: Module,
    bindings: Module,
    source: Module,
    driver: Module,
    cells: Vec<Global>,
    dependency: Global,
}
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let mut config = Config::new();
        config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true)
            .wasm_exceptions(true).consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        let engine = Engine::new(&config).unwrap();
        let prepared = portable::prepare_fragment(
            "(ns app) (def dependency (suss.internal.async/pending)) (def run (fn [x] (suss.async/future* (suss.async/await* dependency))))",
            &Environment::default(), Phase::Runtime).unwrap();
        let resolve = |name| prepared.environment.resolve(Phase::Runtime,
            &suss_reader::Symbol::namespaced("app", name), 0..0).unwrap().global().clone();
        let main = resolve("run");
        let dependency = resolve("dependency");
        let bindings = core_bindings::compile_with_cells(Phase::Runtime, &prepared.cells).unwrap();
        Fixture {
            runtime: Module::new(&engine, runtime_abi::module()).unwrap(),
            bindings: Module::new(&engine, bindings.wasm).unwrap(),
            source: Module::new(&engine, prepared.wasm).unwrap(),
            driver: Module::new(&engine, driver(&main, 1)).unwrap(),
            cells: bindings.cells, dependency, engine,
        }
    })
}
struct Harness {
    store: Store<Host>,
    runtime: Instance,
    bindings: Instance,
    driver: Instance,
}
impl Harness {
    fn new() -> Self {
        let f = fixture();
        let mut store = Store::new(&f.engine, Host::default());
        store.set_fuel(2_000_000).unwrap();
        let mut linker = Linker::new(&f.engine);
        let runtime = linker.instantiate(&mut store, &f.runtime).unwrap();
        linker
            .instance(&mut store, "suss.runtime", runtime)
            .unwrap();
        let bindings = linker.instantiate(&mut store, &f.bindings).unwrap();
        for cell in &f.cells {
            let global = bindings
                .get_global(&mut store, &cell.import_name())
                .unwrap();
            linker
                .define(&store, cell.import_module(), &cell.import_name(), global)
                .unwrap();
        }
        let source = linker.instantiate(&mut store, &f.source).unwrap();
        linker
            .instance(&mut store, "suss.fragment.0", source)
            .unwrap();
        linker.func_wrap("suss.bridge", "install", || {}).unwrap();
        for name in [
            "poll-invocation",
            "resume-invocation",
            "cancel-invocation",
            "pending-invocation",
        ] {
            linker
                .func_wrap("suss.bridge", name, |_key: i32| -> i32 { 0 })
                .unwrap();
        }
        linker
            .func_wrap(
                "suss.bridge",
                "complete-event",
                |_: i32, _: i32, _: i32| -> i32 { 0 },
            )
            .unwrap();
        linker
            .func_wrap("suss.canonical", "set-new", || -> i32 { 1 })
            .unwrap();
        linker
            .func_wrap("suss.canonical", "set-drop", |_: i32| {})
            .unwrap();
        linker
            .func_wrap("suss.canonical", "context-set", |_: i32| {})
            .unwrap();
        linker
            .func_wrap(
                "suss.context",
                "register",
                |mut caller: wasmtime::Caller<'_, Host>, key: i32, set: i32| {
                    caller.data_mut().registrations.push((key, set));
                },
            )
            .unwrap();
        linker
            .func_wrap(
                "suss.context",
                "retire",
                |mut caller: wasmtime::Caller<'_, Host>, key: i32| {
                    caller.data_mut().retirements.push(key);
                },
            )
            .unwrap();
        linker
            .func_wrap(
                "suss.canonical",
                "return",
                |mut caller: wasmtime::Caller<'_, Host>, value: i32| {
                    caller.data_mut().returned.push(value);
                },
            )
            .unwrap();
        let driver = linker.instantiate(&mut store, &f.driver).unwrap();
        Self {
            store,
            runtime,
            bindings,
            driver,
        }
    }
    fn scope(&mut self, key: i32) -> i32 {
        self.runtime
            .get_typed_func::<i32, i32>(&mut self.store, "async-enter-invocation")
            .unwrap()
            .call(&mut self.store, key)
            .unwrap()
    }
    fn counts(&mut self, key: i32) -> (i32, i32, i32) {
        self.runtime
            .get_typed_func::<i32, (i32, i32, i32)>(&mut self.store, "async-invocation-counts")
            .unwrap()
            .call(&mut self.store, key)
            .unwrap()
    }
    fn recover(&mut self) {
        self.driver
            .get_typed_func::<(), ()>(&mut self.store, "recover")
            .unwrap()
            .call(&mut self.store, ())
            .unwrap();
    }
}

#[test]
fn production_entry_fuel_boundaries_restore_scope_without_replaying_entry() {
    let mut successes = 0;
    let mut traps = 0;
    let mut trapped_with_owner = 0;
    let mut quarantined = Vec::new();
    // Fresh stores prevent an interrupted entry from contaminating the next
    // sample. This finite sweep tests pinned checkpoints, not every instruction.
    for fuel in (0..=256).chain((288..=8192).step_by(32)).chain([32768]) {
        let mut h = Harness::new();
        assert_eq!(h.scope(77), 0);
        h.store.set_fuel(fuel).unwrap();
        let result = h
            .driver
            .get_typed_func::<i32, i32>(&mut h.store, "entry")
            .unwrap()
            .call(&mut h.store, 0);
        h.store.set_fuel(2_000_000).unwrap();
        if let Err(error) = result {
            assert_eq!(
                error.downcast_ref::<wasmtime::Trap>(),
                Some(&wasmtime::Trap::OutOfFuel)
            );
            traps += 1;
            if h.counts(i32::MIN).0 > 0 {
                trapped_with_owner += 1;
            }
            h.recover();
            let restored = h.scope(77);
            if restored != 77 {
                // No retry/replay or subsequent invocation is authorized on an
                // interrupted component Store. Record and drop this Store.
                quarantined.push((fuel, restored));
            }
            assert!(h.store.data().returned.is_empty());
        } else {
            successes += 1;
            assert_eq!(h.scope(77), 77);
            assert_eq!(h.store.data().registrations, [(i32::MIN, 1)]);
            assert_eq!(h.counts(i32::MIN).0, 1);
        }
    }
    assert!(
        traps > 0 && successes > 0,
        "sweep must cross actual entry completion"
    );
    assert!(
        trapped_with_owner > 0,
        "sweep must interrupt after a source owner exists"
    );
    eprintln!(
        "entry fuel sweep: traps={traps}, successes={successes}, trapped_with_owner={trapped_with_owner}, quarantined={quarantined:?}"
    );
    assert!(
        quarantined.is_empty(),
        "scope restoration failed at actual fuel checkpoints: {quarantined:?}; these Stores were quarantined, never replayed"
    );
}

#[test]
fn rejected_second_entry_preserves_first_context_and_source_owner() {
    let mut h = Harness::new();
    h.scope(77);
    let entry = h
        .driver
        .get_typed_func::<i32, i32>(&mut h.store, "entry")
        .unwrap();
    assert_eq!(entry.call(&mut h.store, 1).unwrap(), 18); // set1 WAIT
    let before = h.counts(i32::MIN);
    assert_eq!(before.0, 1);
    let error = entry.call(&mut h.store, 2).unwrap_err();
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&wasmtime::Trap::UnreachableCodeReached)
    );
    assert_eq!(h.scope(77), 77);
    assert_eq!(h.counts(i32::MIN), before);
    assert_eq!(h.store.data().registrations, [(i32::MIN, 1)]);
    assert!(h.store.data().retirements.is_empty());
    assert!(h.store.data().returned.is_empty());
    let global = h
        .bindings
        .get_global(&mut h.store, &fixture().dependency.import_name())
        .unwrap();
    let cell = global.get(&mut h.store);
    let mut dependency = [Val::null_any_ref()];
    h.runtime
        .get_func(&mut h.store, "binding-get")
        .unwrap()
        .call(&mut h.store, &[cell], &mut dependency)
        .unwrap();
    let mut number = [Val::null_any_ref()];
    h.runtime
        .get_func(&mut h.store, "number-box")
        .unwrap()
        .call(&mut h.store, &[Val::F64(42f64.to_bits())], &mut number)
        .unwrap();
    let mut accepted = [Val::I32(-1)];
    h.runtime
        .get_func(&mut h.store, "future-resolve")
        .unwrap()
        .call(
            &mut h.store,
            &[dependency[0].clone(), number[0].clone()],
            &mut accepted,
        )
        .unwrap();
    assert_eq!(accepted[0].unwrap_i32(), 1);
    let callback = h
        .driver
        .get_typed_func::<(i32, i32, i32), i32>(&mut h.store, "callback")
        .unwrap();
    let mut completed = false;
    for _ in 0..16 {
        if callback.call(&mut h.store, (0, 0, 0)).unwrap() == 0 {
            completed = true;
            break;
        }
    }
    assert!(completed);
    assert_eq!(h.store.data().returned, [42]);
    assert_eq!(h.store.data().retirements, [i32::MIN]);
    assert_eq!(h.counts(i32::MIN), (0, 0, 0));
    assert_eq!(h.scope(77), 77);
}
