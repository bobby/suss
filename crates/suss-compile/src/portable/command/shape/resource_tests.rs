//! Execute resolved resource signatures through real canonical operations.
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicUsize, Ordering},
};
use wasmtime::{
    Config, Engine, Store, StoreContextMut,
    component::{Linker, Resource, ResourceTable, ResourceType},
};
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .gc_heap_initial_size(8 * 1024 * 1024)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .wasm_component_model(true)
            .wasm_component_model_implements(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    })
}
#[derive(Default)]
struct State {
    table: ResourceTable,
    live: Arc<AtomicUsize>,
    opened: usize,
    dropped: usize,
    order: Vec<&'static str>,
}
struct Counter {
    value: u64,
    live: Arc<AtomicUsize>,
}
impl Drop for Counter {
    fn drop(&mut self) {
        assert!(self.live.fetch_sub(1, Ordering::SeqCst) > 0);
    }
}
fn linker() -> Linker<State> {
    let mut linker = Linker::new(engine());
    let mut storage = linker.instance("storage").unwrap();
    storage
        .resource(
            "counter",
            ResourceType::host::<Counter>(),
            |mut context: StoreContextMut<'_, State>, rep| {
                context
                    .data_mut()
                    .table
                    .delete(Resource::<Counter>::new_own(rep))?;
                context.data_mut().dropped += 1;
                context.data_mut().order.push("drop");
                Ok(())
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "[constructor]counter",
            |mut context: StoreContextMut<'_, State>, (value,): (u64,)| {
                let live = context.data().live.clone();
                live.fetch_add(1, Ordering::SeqCst);
                let resource = context.data_mut().table.push(Counter { value, live })?;
                context.data_mut().opened += 1;
                context.data_mut().order.push("open");
                Ok((resource,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "[method]counter.read",
            |mut context: StoreContextMut<'_, State>, (counter,): (Resource<Counter>,)| {
                assert!(!counter.owned(), "method self must borrow the owner");
                let value = context.data().table.get(&counter)?.value;
                context.data_mut().order.push("read");
                Ok((value,))
            },
        )
        .unwrap();
    storage
        .func_wrap(
            "[static]counter.echo",
            |mut context: StoreContextMut<'_, State>, (value,): (u64,)| {
                context.data_mut().order.push("static");
                Ok((value,))
            },
        )
        .unwrap();
    linker
}
#[test]
fn resource_instance_type_executes_constructor_borrowed_method_static_and_drop() {
    use wasm_encoder::*;
    let mut resolve = wit_parser::Resolve::new();
    let package = resolve.push_str("storage.wit", "package test:resources; interface storage { resource counter { constructor(value: u64); read: func() -> u64; echo: static func(value: u64) -> u64; } } world api { import storage; }").unwrap();
    let id = resolve.packages[package].interfaces["storage"];
    let mut local = super::Types::<InstanceType>::new_instance(&resolve);
    for (name, ty) in &resolve.interfaces[id].types {
        local.export_named(name, *ty).unwrap();
    }
    for (name, function) in &resolve.interfaces[id].functions {
        let index = local.resolved_function(function).unwrap();
        local
            .section
            .export(name.as_str(), ComponentTypeRef::Func(index));
    }
    let mut c = wasm_encoder::Component::new();
    let mut types = ComponentTypeSection::new();
    types.instance(&local.section);
    c.section(&types);
    let mut imports = ComponentImportSection::new();
    imports.import("storage", ComponentTypeRef::Instance(0));
    c.section(&imports);
    let mut aliases = ComponentAliasSection::new();
    aliases.alias(Alias::InstanceExport {
        instance: 0,
        kind: ComponentExportKind::Type,
        name: "counter",
    });
    for name in [
        "[constructor]counter",
        "[method]counter.read",
        "[static]counter.echo",
    ] {
        aliases.alias(Alias::InstanceExport {
            instance: 0,
            kind: ComponentExportKind::Func,
            name,
        });
    }
    c.section(&aliases);
    let mut types = ComponentTypeSection::new();
    types
        .function()
        .params([("value", ComponentValType::Primitive(PrimitiveValType::U64))])
        .result(Some(ComponentValType::Primitive(PrimitiveValType::U64)));
    c.section(&types);
    let mut canon = CanonicalFunctionSection::new();
    for index in 0..3 {
        canon.lower(index, std::iter::empty::<CanonicalOption>());
    }
    canon.resource_drop(1);
    c.section(&canon);
    let mut m = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([ValType::I64], [ValType::I32]);
    types.ty().function([ValType::I32], [ValType::I64]);
    types.ty().function([ValType::I64], [ValType::I64]);
    types.ty().function([ValType::I32], []);
    m.section(&types);
    let mut imports = ImportSection::new();
    for (name, ty) in [("new", 0), ("read", 1), ("echo", 2), ("drop", 3)] {
        imports.import("s", name, EntityType::Function(ty));
    }
    m.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(2);
    m.section(&functions);
    let mut exports = ExportSection::new();
    exports.export("run", ExportKind::Func, 4);
    m.section(&exports);
    let mut body = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    for instruction in [
        Instruction::LocalGet(0),
        Instruction::Call(0),
        Instruction::LocalTee(1),
        Instruction::Call(1),
        Instruction::LocalSet(2),
        Instruction::LocalGet(1),
        Instruction::Call(3),
        Instruction::LocalGet(2),
        Instruction::Call(2),
        Instruction::End,
    ] {
        body.instruction(&instruction);
    }
    let mut code = CodeSection::new();
    code.function(&body);
    m.section(&code);
    c.section(&ModuleSection(&m));
    let mut instances = InstanceSection::new();
    instances.export_items([
        ("new", ExportKind::Func, 0),
        ("read", ExportKind::Func, 1),
        ("echo", ExportKind::Func, 2),
        ("drop", ExportKind::Func, 3),
    ]);
    instances.instantiate(0, [("s", ModuleArg::Instance(0))]);
    c.section(&instances);
    let mut aliases = ComponentAliasSection::new();
    aliases.alias(Alias::CoreInstanceExport {
        instance: 1,
        kind: ExportKind::Func,
        name: "run",
    });
    c.section(&aliases);
    let mut canon = CanonicalFunctionSection::new();
    canon.lift(4, 2, std::iter::empty::<CanonicalOption>());
    c.section(&canon);
    let mut exports = ComponentExportSection::new();
    exports.export("run", ComponentExportKind::Func, 3, None);
    c.section(&exports);
    let component = wasmtime::component::Component::new(engine(), c.finish()).unwrap();
    let mut store = Store::new(engine(), State::default());
    store.set_fuel(100_000_000).unwrap();
    let live = store.data().live.clone();
    let instance = linker().instantiate(&mut store, &component).unwrap();
    let run = instance
        .get_typed_func::<(u64,), (u64,)>(&mut store, "run")
        .unwrap();
    for value in [0, 9007199254740993, u64::MAX] {
        assert_eq!(run.call(&mut store, (value,)).unwrap().0, value);
        assert_eq!(live.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
    assert_eq!((store.data().opened, store.data().dropped), (3, 3));
    assert_eq!(
        store.data().order,
        ["open", "read", "drop", "static"].repeat(3)
    );
    println!(
        "canonical imported resources execute own constructor, borrowed self, static and drop; live=0, drops=3"
    );
}
