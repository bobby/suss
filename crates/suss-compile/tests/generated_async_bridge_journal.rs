//! Production bridge journal with explicit injected canonical operations.
//! These shims exercise journal recovery, not Wasmtime canonical builtin behavior.
use suss_compile::{
    portable::{
        self,
        command::async_bridge,
        resolve::{Environment, Phase},
    },
    runtime_abi,
};
use wasmtime::{
    Config, Engine, Global, GlobalType, Instance, Linker, Memory, MemoryType, Module, Mutability,
    RefType, Store, Val, ValType,
};
#[derive(Default)]
struct Operations {
    drops: usize,
    releases: usize,
    cancels: usize,
    joined: bool,
    fail_release: bool,
}
struct Harness {
    store: Store<Operations>,
    bridge: Instance,
}
impl Harness {
    fn new(fail_release: bool) -> Self {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        static ENGINE: std::sync::OnceLock<Engine> = std::sync::OnceLock::new();
        let engine = ENGINE.get_or_init(|| Engine::new(&config).unwrap()).clone();
        let mut store = Store::new(
            &engine,
            Operations {
                fail_release,
                ..Default::default()
            },
        );
        store.set_fuel(2_000_000).unwrap();
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
        let memory = Memory::new(&mut store, MemoryType::new(1, Some(65536))).unwrap();
        memory.write(&mut store, 64, &99u32.to_le_bytes()).unwrap();
        linker
            .define(&store, "suss.memory", "memory", memory)
            .unwrap();
        linker
            .func_wrap(
                "suss.memory",
                "cabi_realloc",
                |_: i32, _: i32, _: i32, _: i32| -> i32 { 64 },
            )
            .unwrap();
        linker
            .func_wrap(
                "suss.memory",
                "release",
                |mut caller: wasmtime::Caller<'_, Operations>,
                 _: i32,
                 _: i32|
                 -> wasmtime::Result<()> {
                    caller.data_mut().releases += 1;
                    if caller.data().fail_release {
                        wasmtime::bail!("injected release error after side effect");
                    }
                    Ok(())
                },
            )
            .unwrap();
        linker
            .func_wrap("suss.canonical", "context", || -> i32 { 1 })
            .unwrap();
        linker
            .func_wrap("suss.context", "waitable-set", || -> i32 { 2 })
            .unwrap();
        linker
            .func_wrap("suss.canonical", "call", |_: i32, _: i32| -> i32 {
                (7 << 4) | 1
            })
            .unwrap();
        linker
            .func_wrap(
                "suss.canonical",
                "join",
                |mut caller: wasmtime::Caller<'_, Operations>, _: i32, set: i32| {
                    caller.data_mut().joined = set != 0;
                },
            )
            .unwrap();
        linker
            .func_wrap(
                "suss.canonical",
                "subtask-drop",
                |mut caller: wasmtime::Caller<'_, Operations>, _: i32| {
                    assert!(!caller.data().joined);
                    caller.data_mut().drops += 1;
                },
            )
            .unwrap();
        linker
            .func_wrap(
                "suss.canonical",
                "cancel",
                |mut caller: wasmtime::Caller<'_, Operations>, _: i32| -> i32 {
                    assert!(!caller.data().joined);
                    caller.data_mut().cancels += 1;
                    3
                },
            )
            .unwrap();
        let mut env = Environment::default();
        let cell = env
            .declare_cell(Phase::Runtime, "host", "increment")
            .unwrap();
        let prepared =
            portable::prepare_fragment("(host/increment 7)", &env, Phase::Runtime).unwrap();
        for identity in &prepared.cells {
            let mut result = [Val::null_any_ref()];
            runtime
                .get_func(&mut store, "binding-unbound")
                .unwrap()
                .call(&mut store, &[], &mut result)
                .unwrap();
            let ty = result[0]
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap()
                .ty(&store)
                .unwrap();
            let global = Global::new(
                &mut store,
                GlobalType::new(
                    ValType::Ref(RefType::new(false, ty.into())),
                    Mutability::Const,
                ),
                result[0].clone(),
            )
            .unwrap();
            linker
                .define(
                    &store,
                    identity.import_module(),
                    &identity.import_name(),
                    global,
                )
                .unwrap();
        }
        let mut resolve = wit_parser::Resolve::new();
        let package = resolve.push_str("journal.wit", "package test:journal; world api { import increment: async func(value: u32) -> u32; }").unwrap();
        let world = resolve.select_world(&[package], Some("api")).unwrap();
        let import = resolve.worlds[world]
            .imports
            .values()
            .find_map(|item| match item {
                wit_parser::WorldItem::Function(f) => Some(f),
                _ => None,
            })
            .unwrap();
        let bytes = async_bridge::module(&resolve, import, &cell).unwrap();
        let bridge = linker
            .instantiate(&mut store, &Module::new(&engine, bytes).unwrap())
            .unwrap();
        bridge
            .get_typed_func::<(), ()>(&mut store, "install")
            .unwrap()
            .call(&mut store, ())
            .unwrap();
        let source = linker
            .instantiate(&mut store, &Module::new(&engine, prepared.wasm).unwrap())
            .unwrap();
        source
            .get_func(&mut store, "eval")
            .unwrap()
            .call(&mut store, &[], &mut [Val::null_any_ref()])
            .unwrap();
        Self { store, bridge }
    }
    fn count(&mut self) -> i32 {
        self.store.set_fuel(1_000_000).unwrap();
        self.bridge
            .get_typed_func::<i32, i32>(&mut self.store, "pending-invocation")
            .unwrap()
            .call(&mut self.store, 1)
            .unwrap()
    }
    fn complete(&mut self) -> wasmtime::Result<i32> {
        self.bridge
            .get_typed_func::<(i32, i32, i32), i32>(&mut self.store, "complete-event")
            .unwrap()
            .call(&mut self.store, (1, 7, 2))
    }
}
#[test]
fn completed_record_rejects_late_event_without_repeating_resources() {
    let mut h = Harness::new(false);
    assert_eq!(h.complete().unwrap(), 1);
    assert_eq!(h.count(), 0);
    assert_eq!(h.complete().unwrap(), 0);
    assert_eq!((h.store.data().drops, h.store.data().releases), (1, 1));
}
#[test]
fn uncertain_release_error_is_quarantined_without_blind_retry() {
    let mut h = Harness::new(true);
    let error = h.complete().unwrap_err();
    assert!(format!("{error:#}").contains("injected release error after side effect"));
    h.store.set_fuel(1_000_000).unwrap();
    h.bridge
        .get_typed_func::<i32, ()>(&mut h.store, "quarantine-invocation")
        .unwrap()
        .call(&mut h.store, 1)
        .unwrap();
    assert_eq!(
        h.count(),
        1,
        "uncertain ownership stays rooted for Store retirement"
    );
    for name in ["resume-invocation", "cancel-invocation"] {
        let _ = h
            .bridge
            .get_typed_func::<i32, i32>(&mut h.store, name)
            .unwrap()
            .call(&mut h.store, 1);
    }
    assert!(h.complete().is_err());
    assert_eq!(
        (
            h.store.data().drops,
            h.store.data().releases,
            h.store.data().cancels
        ),
        (1, 1, 0)
    );
}
#[test]
fn actual_fuel_traps_resume_acknowledged_teardown_without_double_release() {
    let mut trapped = 0;
    for fuel in [0, 1, 4, 16, 64, 128, 256, 512, 1024] {
        let mut h = Harness::new(false);
        h.store.set_fuel(fuel).unwrap();
        if let Err(error) = h.complete() {
            assert_eq!(
                error.downcast_ref::<wasmtime::Trap>(),
                Some(&wasmtime::Trap::OutOfFuel),
                "unexpected error: {error:#}"
            );
            trapped += 1;
        }
        h.store.set_fuel(1_000_000).unwrap();
        h.bridge
            .get_typed_func::<i32, i32>(&mut h.store, "resume-invocation")
            .unwrap()
            .call(&mut h.store, 1)
            .unwrap();
        if h.count() != 0 {
            h.complete().unwrap();
        }
        assert_eq!(h.count(), 0);
        assert_eq!(
            (h.store.data().drops, h.store.data().releases),
            (1, 1),
            "fuel={fuel}"
        );
    }
    assert!(trapped > 0, "must execute real engine fuel traps");
}
