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
    let borrow_wrapper_type = next_type(&types);
    types.ty().function([ValType::I32, ValType::I32], [VALUE]);
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
        borrow_scope: 2,
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
        carrier_types.make,
        borrow_wrapper_type,
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
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([runtime_abi::sentinel_value(runtime_abi::NIL)]),
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
    for (name, index) in [
        ("make", 6),
        ("check", 7),
        ("close", 8),
        ("invoke", 11),
        ("borrow-call", 13),
    ] {
        exports.export(name, ExportKind::Func, index);
    }
    exports.export("retained-borrow", ExportKind::Global, 3);
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
    code.function(&carrier.make_borrow());
    let context = BorrowContext {
        previous: 2,
        token: 3,
        error: 4,
        cursor: 5,
        state: 6,
        returned: Some(7),
    };
    let mut borrow = Function::new([(7, VALUE), (1, ValType::I64)]);
    carrier.begin_borrow(&mut borrow, &context);
    emit(
        &mut borrow,
        &[
            LocalGet(0),
            I32Const(17),
            RefFunc(5),
            Call(12),
            LocalTee(8),
            GlobalSet(3),
            LocalGet(8),
            I32Const(17),
            I32Const(0),
            Call(7),
            Drop,
            LocalGet(1),
            If(BlockType::Empty),
            I64Const(17),
        ],
    );
    for instruction in runtime_abi::long_box_in(9) {
        borrow.instruction(&instruction);
    }
    emit(&mut borrow, &[Throw(0), End, LocalGet(8)]);
    carrier.finish_borrow(&mut borrow, &context);
    borrow.instruction(&End);
    code.function(&borrow);
    let mut elements = ElementSection::new();
    elements.declared(Elements::Functions(std::borrow::Cow::Owned(vec![5])));
    let mut module = wasm_encoder::Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&globals)
        .section(&exports)
        .section(&StartSection { function_index: 10 })
        .section(&elements)
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

#[test]
fn borrow_call_finally_invalidates_aliases_and_releases_entries_before_rethrowing_the_original_payload()
 {
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
            |mut cx: wasmtime::Caller<'_, Vec<u32>>, handle: u32| {
                cx.data_mut().push(handle);
            },
        )
        .unwrap();
    let instance = linker
        .instantiate(&mut store, &Module::new(engine(), module()).unwrap())
        .unwrap();
    let handle = value(
        &mut store,
        instance,
        "borrow-call",
        &[Val::I32(41), Val::I32(0)],
    );
    assert_eq!(store.data(), &[41]);
    let check = instance.get_func(&mut store, "check").unwrap();
    assert!(
        check
            .call(
                &mut store,
                &[handle, Val::I32(17), Val::I32(0)],
                &mut [Val::null_any_ref()]
            )
            .is_err()
    );
    assert!(store.take_pending_exception().is_some());
    store.gc(None).unwrap();
    let error = instance
        .get_func(&mut store, "borrow-call")
        .unwrap()
        .call(
            &mut store,
            &[Val::I32(42), Val::I32(1)],
            &mut [Val::null_any_ref()],
        )
        .unwrap_err();
    assert!(
        error.is::<wasmtime::ThrownException>() && !error.is::<wasmtime::Trap>(),
        "{error:#}"
    );
    let exception = store.take_pending_exception().unwrap();
    let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
    assert_eq!(
        crate::portable_session::runtime_number(&mut store, &fields[0]).unwrap(),
        Some(crate::portable_session::RuntimeNumber::Long(17))
    );
    assert_eq!(store.data(), &[41, 42]);
    store.gc(None).unwrap();
    let alias = instance
        .get_global(&mut store, "retained-borrow")
        .unwrap()
        .get(&mut store);
    assert!(
        check
            .call(
                &mut store,
                &[alias, Val::I32(17), Val::I32(0)],
                &mut [Val::null_any_ref()]
            )
            .is_err()
    );
    assert_eq!(store.data(), &[41, 42]);
}
