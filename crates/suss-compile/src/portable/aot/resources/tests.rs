use super::*;
use std::sync::OnceLock;
use wasmtime::{Config, Engine, Instance, Linker, Module, Store, Val};

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    })
}
fn module() -> Vec<u8> {
    use Instruction::*;
    let mut types = runtime_abi::prelude();
    let carrier_types = Types::declare(&mut types);
    let mut imports = ImportSection::new();
    for (name, params, results) in [
        ("descriptor-new-core", vec![VALUE; 3], vec![VALUE]),
        ("object-new", vec![VALUE; 2], vec![VALUE]),
        ("string-new", vec![ValType::I32], vec![VALUE]),
        (
            "string-set-unit",
            vec![VALUE, ValType::I32, ValType::I32],
            vec![ValType::I32],
        ),
        ("language-error-new", vec![VALUE], vec![VALUE]),
    ] {
        let index = next_type(&types);
        types.ty().function(params, results);
        imports.import("r", name, EntityType::Function(index));
    }
    imports.import("host", "drop", EntityType::Function(carrier_types.drop));
    let tag_type = next_type(&types);
    types.ty().function([VALUE], []);
    imports.import(
        "r",
        "language-exception",
        EntityType::Tag(TagType {
            kind: TagKind::Exception,
            func_type_idx: tag_type,
        }),
    );
    let init_type = next_type(&types);
    types.ty().function([], []);
    let invoke_wrapper_type = next_type(&types);
    types.ty().function([VALUE], [VALUE]);
    let helpers = Helpers {
        object_new: 1,
        string_new: 2,
        string_set: 3,
        error_new: 4,
        error_tag: 0,
    };
    let carrier = Carrier {
        types: carrier_types,
        helpers,
        descriptor: 0,
        scope: 1,
        check_function: 7,
        close_function: 8,
    };
    let mut functions = FunctionSection::new();
    for ty in [
        carrier_types.make,
        carrier_types.check,
        carrier_types.close,
        runtime_abi::INVOKE,
        init_type,
        invoke_wrapper_type,
    ] {
        functions.function(ty);
    }
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }),
    );
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([runtime_abi::sentinel_value(runtime_abi::NIL)]),
    );
    let mut exports = ExportSection::new();
    for (name, index) in [("make", 6), ("check", 7), ("close", 8), ("invoke", 11)] {
        exports.export(name, ExportKind::Func, index);
    }
    let mut code = CodeSection::new();
    code.function(&carrier.make())
        .function(&carrier.check())
        .function(&carrier.close())
        .function(&carrier.invoke_close());
    let mut init = Function::new([(1, VALUE)]);
    string(&mut init, helpers, "resource-state", 0);
    init.instruction(&ArrayNewFixed {
        array_type_index: runtime_abi::ARGS,
        array_size: 1,
    });
    string(&mut init, helpers, "suss.resource.Handle", 0);
    emit(
        &mut init,
        &[I32Const(0), RefI31, Call(0), GlobalSet(0), End],
    );
    code.function(&init);
    let mut invoke = Function::new([]);
    emit(
        &mut invoke,
        &[
            runtime_abi::sentinel_value(runtime_abi::NIL),
            LocalGet(0),
            ArrayNewFixed {
                array_type_index: runtime_abi::ARGS,
                array_size: 1,
            },
            Call(9),
            End,
        ],
    );
    code.function(&invoke);
    let mut module = wasm_encoder::Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&globals)
        .section(&exports)
        .section(&StartSection { function_index: 10 })
        .section(&code);
    module.finish()
}
fn value(store: &mut Store<Vec<u32>>, instance: Instance, name: &str, params: &[Val]) -> Val {
    let mut result = [Val::null_any_ref()];
    instance
        .get_func(&mut *store, name)
        .unwrap()
        .call(store, params, &mut result)
        .unwrap();
    result[0].clone()
}
#[test]
fn opaque_resource_carriers_reject_numeric_wrong_type_and_closed_aliases_after_gc() {
    let mut store = Store::new(engine(), Vec::<u32>::new());
    store.set_fuel(100_000_000).unwrap();
    let runtime = Instance::new(
        &mut store,
        &Module::new(engine(), runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut linker = Linker::new(engine());
    linker.instance(&mut store, "r", runtime).unwrap();
    linker
        .func_wrap(
            "host",
            "drop",
            |mut context: wasmtime::Caller<'_, Vec<u32>>, handle: u32| {
                context.data_mut().push(handle);
            },
        )
        .unwrap();
    let instance = linker
        .instantiate(&mut store, &Module::new(engine(), module()).unwrap())
        .unwrap();
    let drop_function = linker
        .get(&mut store, "host", "drop")
        .unwrap()
        .into_func()
        .unwrap();
    let check = instance.get_func(&mut store, "check").unwrap();
    let close = instance.get_func(&mut store, "close").unwrap();
    let numeric = Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
        &mut store,
        wasmtime::I31::wrapping_u32(42),
    )));
    assert!(
        check
            .call(
                &mut store,
                &[numeric, Val::I32(0), Val::I32(0)],
                &mut [Val::null_any_ref()]
            )
            .is_err()
    );
    for (index, raw) in [0_u32, 1, u32::MAX].into_iter().enumerate() {
        let handle = value(
            &mut store,
            instance,
            "make",
            &[
                Val::I32(raw as i32),
                Val::I32(17),
                Val::FuncRef(Some(drop_function)),
            ],
        );
        let alias = handle.clone();
        assert!(
            check
                .call(
                    &mut store,
                    &[handle.clone(), Val::I32(18), Val::I32(0)],
                    &mut [Val::null_any_ref()]
                )
                .is_err()
        );
        assert_eq!(store.data().len(), index);
        store.gc(None).unwrap();
        check
            .call(
                &mut store,
                &[handle.clone(), Val::I32(17), Val::I32(1)],
                &mut [Val::null_any_ref()],
            )
            .unwrap();
        let nil = value(&mut store, instance, "invoke", &[handle]);
        assert!(nil.unwrap_anyref().is_none());
        store.gc(None).unwrap();
        assert_eq!(store.data().len(), index + 1);
        assert!(close.call(&mut store, &[alias.clone()], &mut []).is_err());
        assert!(
            check
                .call(
                    &mut store,
                    &[alias, Val::I32(17), Val::I32(0)],
                    &mut [Val::null_any_ref()]
                )
                .is_err()
        );
        assert_eq!(store.data().len(), index + 1);
    }
    assert_eq!(store.data(), &[0, 1, u32::MAX]);
}
