//! Execute production ABI v1 runtime bytes, independently of Suss printing/equality.
mod support;
use suss_compile::runtime_abi;
use wasmtime::{Instance, Module, Store, Val};

#[test]
fn runtime_abi_numeric_samples_match_pinned_formatting_and_parsing() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/numeric-cases.json")).unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let boxed = runtime.get_func(&mut store, "number-box").unwrap();
    let formatted = runtime.get_func(&mut store, "coerce-string").unwrap();
    let parsed = runtime.get_func(&mut store, "coerce-number").unwrap();
    let memory = runtime
        .get_memory(&mut store, "numeric-scratch-memory")
        .unwrap();
    let mut identities = std::collections::HashSet::new();
    let mut capacity = 0;
    for (index, case) in corpus["cases"].as_array().unwrap().iter().enumerate() {
        let input = case["bits"].as_str().unwrap();
        assert_eq!(case["id"], input);
        assert!(identities.insert(input));
        {
            let mut scope = wasmtime::RootScope::new(&mut store);
            let mut value = [Val::null_any_ref()];
            let bits = u64::from_str_radix(input, 16).unwrap();
            boxed
                .call(&mut scope, &[Val::F64(bits)], &mut value)
                .unwrap();
            let object = value[0]
                .unwrap_anyref()
                .unwrap()
                .as_struct(&scope)
                .unwrap()
                .unwrap();
            assert_eq!(
                object.field(&mut scope, 0).unwrap().unwrap_f64().to_bits(),
                bits
            );
            let mut text = [Val::null_any_ref()];
            formatted.call(&mut scope, &value, &mut text).unwrap();
            let array = text[0]
                .unwrap_anyref()
                .unwrap()
                .as_array(&scope)
                .unwrap()
                .unwrap();
            let units = array
                .elems(&mut scope)
                .unwrap()
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            assert_eq!(
                serde_json::json!({"tag":"string","units":units}),
                case["formatted"],
                "{input}"
            );
            let mut number = [Val::F64(0)];
            parsed.call(&mut scope, &text, &mut number).unwrap();
            let bits = number[0].unwrap_f64().to_bits();
            assert_eq!(
                serde_json::json!({"tag":"f64","bits":format!("{bits:016x}")}),
                case["parsed"],
                "{input}"
            );
        }
        if index == 0 {
            capacity = memory.data_size(&store);
        }
        assert_eq!(memory.data_size(&store), capacity);
        if index % 64 == 0 {
            store.gc(None).unwrap();
        }
    }
    assert_eq!(identities.len(), 1024);
}

#[test]
fn runtime_abi_numeric_allocation_failure_is_typed_and_not_nan_or_a_trap() {
    let engine = support::engine();
    let module = Module::new(&engine, runtime_abi::module()).unwrap();
    let minimum = module
        .exports()
        .find_map(|export| match export.ty() {
            wasmtime::ExternType::Memory(memory) => Some(memory.minimum() as usize * 65536),
            _ => None,
        })
        .unwrap();
    let limits = wasmtime::StoreLimitsBuilder::new()
        .memory_size(minimum)
        .build();
    let mut store = Store::new(&engine, limits);
    store.limiter(|limits| limits);
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let memory = runtime
        .get_memory(&mut store, "numeric-scratch-memory")
        .unwrap();
    let function = runtime.get_func(&mut store, "numeric-reserve").unwrap();
    let mut output = [Val::I32(0)];
    let error = function
        .call(&mut store, &[Val::I64(32)], &mut output)
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>());
    let exception = store.take_pending_exception().unwrap();
    let tag = exception.tag(&mut store).unwrap();
    let expected = runtime.get_tag(&mut store, "language-exception").unwrap();
    assert!(wasmtime::Tag::eq(&tag, &expected, &store));
    let value = exception.field(&mut store, 0).unwrap();
    let value = value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    let fields = value.fields(&mut store).unwrap().collect::<Vec<_>>();
    assert_eq!(fields.len(), 4);
    let descriptor = fields[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(descriptor.field(&mut store, 0).unwrap().unwrap_i64(), 6);
    let message = fields[1]
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    let units = message
        .elems(&mut store)
        .unwrap()
        .map(|unit| unit.unwrap_i32() as u16)
        .collect::<Vec<_>>();
    assert_eq!(
        String::from_utf16(&units).unwrap(),
        "Numeric conversion scratch allocation failed"
    );
    assert_eq!(memory.data_size(&store), minimum);
    assert!(!store.has_pending_exception());
    let mut result = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "number-box")
        .unwrap()
        .call(&mut store, &[Val::F64(42.0f64.to_bits())], &mut result)
        .unwrap();
    let object = result[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(
        object.field(&mut store, 0).unwrap().unwrap_f64().to_bits(),
        42.0f64.to_bits()
    );
}

#[test]
fn runtime_abi_numbers_keep_binary64_bits_and_rounding() {
    let engine = support::engine();
    let module = Module::new(&engine, runtime_abi::module()).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let boxed = runtime
        .get_func(&mut store, "number-box")
        .expect("numeric runtime export");
    let unbox = runtime.get_func(&mut store, "number-unbox").unwrap();
    for bits in [
        0,
        (-0.0f64).to_bits(),
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        0x7ff8_0000_0000_0123,
        9007199254740992.0f64.to_bits(),
    ] {
        let mut result = [Val::null_any_ref()];
        boxed
            .call(&mut store, &[Val::F64(bits)], &mut result)
            .unwrap();
        let Val::AnyRef(Some(reference)) = &result[0] else {
            panic!("expected boxed number")
        };
        let object = reference.as_struct(&store).unwrap().unwrap();
        let fields: Vec<_> = object.fields(&mut store).unwrap().collect();
        assert!(matches!(fields.as_slice(), [Val::F64(raw)] if *raw == bits));
        let mut decoded = [Val::F64(0)];
        unbox.call(&mut store, &result, &mut decoded).unwrap();
        assert_eq!(decoded[0].f64().unwrap().to_bits(), bits);
    }
    let mut operands = Vec::new();
    for value in [9007199254740992.0f64, 1.0] {
        let mut result = [Val::null_any_ref()];
        boxed
            .call(&mut store, &[Val::F64(value.to_bits())], &mut result)
            .unwrap();
        operands.push(result[0].clone());
    }
    let add = runtime.get_func(&mut store, "number-add").unwrap();
    let mut sum = [Val::null_any_ref()];
    add.call(&mut store, &operands, &mut sum).unwrap();
    let mut decoded = [Val::F64(0)];
    unbox.call(&mut store, &sum, &mut decoded).unwrap();
    assert_eq!(decoded[0].f64().unwrap(), 9007199254740992.0);
}

#[test]
fn runtime_abi_utf16_units_survive_gc_and_reject_truncation() {
    let engine = support::engine();
    let bytes = runtime_abi::module();
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
    let module = Module::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let new = runtime.get_func(&mut store, "string-new").unwrap();
    let set = runtime.get_func(&mut store, "string-set-unit").unwrap();
    let len = runtime.get_func(&mut store, "string-length").unwrap();
    let get = runtime.get_func(&mut store, "string-unit").unwrap();
    let units = [0, 0xd800, 0xd83d, 0xde00, 0xffff];
    let mut value = [Val::null_any_ref()];
    new.call(&mut store, &[Val::I32(units.len() as i32)], &mut value)
        .unwrap();
    for (index, unit) in units.iter().enumerate() {
        let mut accepted = [Val::I32(0)];
        set.call(
            &mut store,
            &[value[0].clone(), Val::I32(index as i32), Val::I32(*unit)],
            &mut accepted,
        )
        .unwrap();
        assert_eq!(accepted[0].i32(), Some(1));
    }
    for (index, unit) in [(1, 65536), (1, -1), (-1, 1), (5, 1)] {
        let mut accepted = [Val::I32(1)];
        set.call(
            &mut store,
            &[value[0].clone(), Val::I32(index), Val::I32(unit)],
            &mut accepted,
        )
        .unwrap();
        assert_eq!(accepted[0].i32(), Some(0));
    }
    store.gc(None).expect("forced collection");
    let mut length = [Val::I32(0)];
    len.call(&mut store, &value, &mut length).unwrap();
    assert_eq!(length[0].i32(), Some(5));
    for (index, expected) in units.iter().enumerate() {
        let mut unit = [Val::I32(0)];
        get.call(
            &mut store,
            &[value[0].clone(), Val::I32(index as i32)],
            &mut unit,
        )
        .unwrap();
        assert_eq!(unit[0].i32(), Some(*expected));
    }
    let Val::AnyRef(Some(reference)) = &value[0] else {
        panic!("expected UTF-16 array")
    };
    let array = reference.as_array(&store).unwrap().unwrap();
    assert!(matches!(
        array.ty(&store).unwrap().element_type(),
        wasmtime::StorageType::I16
    ));
    assert_eq!(array.len(&store).unwrap(), 5);
}

fn fragment(producer: bool) -> Vec<u8> {
    use wasm_encoder::*;
    let value = ValType::Ref(RefType::EQREF);
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    if producer {
        types.ty().function([ValType::F64], [value]);
        types.ty().function([ValType::I32], [value]);
        imports.import("rt", "number-box", EntityType::Function(10));
        imports.import("rt", "string-new", EntityType::Function(11));
        for (i, name) in ["number", "string"].iter().enumerate() {
            functions.function(10 + i as u32);
            exports.export(name, ExportKind::Func, 2 + i as u32);
            let mut function = Function::new([]);
            function
                .instruction(&Instruction::LocalGet(0))
                .instruction(&Instruction::Call(i as u32))
                .instruction(&Instruction::End);
            code.function(&function);
        }
    } else {
        types.ty().function([value], [ValType::F64]);
        types.ty().function([value, ValType::I32], [ValType::I32]);
        functions.function(10).function(11);
        exports
            .export("number", ExportKind::Func, 0)
            .export("unit", ExportKind::Func, 1);
        let mut number = Function::new([]);
        number
            .instruction(&Instruction::LocalGet(0))
            .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(0)))
            .instruction(&Instruction::StructGet {
                struct_type_index: 0,
                field_index: 0,
            })
            .instruction(&Instruction::End);
        code.function(&number);
        let mut unit = Function::new([]);
        unit.instruction(&Instruction::LocalGet(0))
            .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(1)))
            .instruction(&Instruction::LocalGet(1))
            .instruction(&Instruction::ArrayGetU(1))
            .instruction(&Instruction::End);
        code.function(&unit);
    }
    let mut module = Module::new();
    module.section(&types);
    if producer {
        module.section(&imports);
    }
    module
        .section(&functions)
        .section(&exports)
        .section(&code)
        .section(&runtime_abi::Manifest::default().section());
    module.finish()
}

#[test]
fn runtime_abi_generated_fragments_share_scalar_types_and_gc_roots() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut linker = wasmtime::Linker::new(&engine);
    linker.instance(&mut store, "rt", runtime).unwrap();
    let a = fragment(true);
    let b = fragment(false);
    for bytes in [&a, &b] {
        runtime_abi::verify_artifact(bytes, &runtime_abi::Manifest::default()).unwrap();
    }
    let a = linker
        .instantiate(&mut store, &Module::new(&engine, a).unwrap())
        .unwrap();
    let b = linker
        .instantiate(&mut store, &Module::new(&engine, b).unwrap())
        .unwrap();
    let mut number = [Val::null_any_ref()];
    a.get_func(&mut store, "number")
        .unwrap()
        .call(&mut store, &[Val::F64((-0.0f64).to_bits())], &mut number)
        .unwrap();
    let mut string = [Val::null_any_ref()];
    a.get_func(&mut store, "string")
        .unwrap()
        .call(&mut store, &[Val::I32(1)], &mut string)
        .unwrap();
    runtime
        .get_func(&mut store, "string-set-unit")
        .unwrap()
        .call(
            &mut store,
            &[string[0].clone(), Val::I32(0), Val::I32(0xd800)],
            &mut [Val::I32(0)],
        )
        .unwrap();
    store.gc(None).expect("forced collection");
    let mut bits = [Val::F64(0)];
    b.get_func(&mut store, "number")
        .unwrap()
        .call(&mut store, &number, &mut bits)
        .unwrap();
    assert_eq!(bits[0].f64().unwrap().to_bits(), (-0.0f64).to_bits());
    let mut unit = [Val::I32(0)];
    b.get_func(&mut store, "unit")
        .unwrap()
        .call(&mut store, &[string[0].clone(), Val::I32(0)], &mut unit)
        .unwrap();
    assert_eq!(unit[0].i32(), Some(0xd800));
}

fn initializer(manifest: &runtime_abi::Manifest) -> Vec<u8> {
    use wasm_encoder::*;
    let mut types = runtime_abi::prelude();
    types.ty().function([], []);
    let mut imports = ImportSection::new();
    imports.import("host", "mark", EntityType::Function(10));
    let mut functions = FunctionSection::new();
    functions.function(10);
    let mut function = Function::new([]);
    function
        .instruction(&Instruction::Call(0))
        .instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&StartSection { function_index: 1 })
        .section(&code)
        .section(&manifest.section());
    module.finish()
}

#[test]
fn runtime_abi_manifest_gate_precedes_initializer_effects() {
    let engine = support::engine();
    let mut store = Store::new(&engine, 0u32);
    let mut linker = wasmtime::Linker::new(&engine);
    linker
        .func_wrap("host", "mark", |mut caller: wasmtime::Caller<'_, u32>| {
            *caller.data_mut() += 1
        })
        .unwrap();
    let expected = runtime_abi::Manifest::default();
    let load = |bytes: &[u8], store: &mut Store<u32>| -> Result<(), String> {
        runtime_abi::verify_artifact(bytes, &expected)?;
        let module = Module::new(&engine, bytes).map_err(|e| format!("{e:#}"))?;
        linker
            .instantiate(store, &module)
            .map_err(|e| format!("{e:#}"))?;
        Ok(())
    };
    for changed in [
        runtime_abi::Manifest {
            runtime_abi: 2,
            ..expected.clone()
        },
        runtime_abi::Manifest {
            compiler: env!("CARGO_PKG_VERSION").into(), // pre-undefined portable format
            ..expected.clone()
        },
        runtime_abi::Manifest {
            compiler: "different".into(),
            ..expected.clone()
        },
        runtime_abi::Manifest {
            wasm_tools: "different".into(),
            ..expected.clone()
        },
    ] {
        assert!(
            load(&initializer(&changed), &mut store)
                .unwrap_err()
                .contains("version mismatch")
        );
        assert_eq!(*store.data(), 0);
    }
    let mut altered = initializer(&expected);
    let marker = [0x4e, 10, 0x5f, 1, 0x7c, 0]; // rec(10), struct, one f64 field
    let offset = altered
        .windows(marker.len())
        .position(|bytes| bytes == marker)
        .unwrap();
    altered[offset + 4] = 0x7e; // still valid Wasm, incompatible i64 Number layout
    assert!(load(&altered, &mut store).unwrap_err().contains("prelude"));
    assert_eq!(*store.data(), 0);
    load(&initializer(&expected), &mut store).unwrap();
    assert_eq!(*store.data(), 1);
}

#[test]
fn runtime_abi_manifest_rejects_missing_duplicate_and_malformed_data() {
    use std::borrow::Cow;
    use wasm_encoder::{CustomSection, Module};
    let expected = runtime_abi::Manifest::default();
    assert!(
        runtime_abi::verify_artifact(&Module::new().finish(), &expected)
            .unwrap_err()
            .contains("missing")
    );
    let mut duplicate = Module::new();
    duplicate
        .section(&expected.section())
        .section(&expected.section());
    assert!(
        runtime_abi::verify_artifact(&duplicate.finish(), &expected)
            .unwrap_err()
            .contains("duplicate")
    );
    for data in [
        vec![],
        vec![1, 0, 0, 0],
        vec![1, 0, 0, 0, 255],
        vec![1, 0, 0, 0, b'a', 0, b'b', 0, b'c', 0],
    ] {
        let mut malformed = Module::new();
        malformed.section(&CustomSection {
            name: Cow::Borrowed("suss.runtime-abi"),
            data: Cow::Owned(data),
        });
        assert!(runtime_abi::verify_artifact(&malformed.finish(), &expected).is_err());
    }
    assert!(runtime_abi::verify_artifact(&[0, 97, 115, 109], &expected).is_err());
}

fn closure_fragment(catcher: bool) -> Vec<u8> {
    use std::borrow::Cow;
    use wasm_encoder::*;
    let value = ValType::Ref(RefType::EQREF);
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    let mut elements = ElementSection::new();
    if catcher {
        types.ty().function([value, value], [value]); // 10
        types.ty().function([value], []); // 11
        imports.import("rt", "invoke", EntityType::Function(10));
        imports.import(
            "rt",
            "language-exception",
            EntityType::Tag(TagType {
                kind: TagKind::Exception,
                func_type_idx: 11,
            }),
        );
        functions.function(10);
        exports.export("call", ExportKind::Func, 1);
        let mut function = Function::new([]);
        function
            .instruction(&Instruction::Block(BlockType::Result(value)))
            .instruction(&Instruction::TryTable(
                BlockType::Result(value),
                Cow::Borrowed(&[Catch::One { tag: 0, label: 0 }]),
            ))
            .instruction(&Instruction::LocalGet(0))
            .instruction(&Instruction::LocalGet(1))
            .instruction(&Instruction::Call(0))
            .instruction(&Instruction::End)
            .instruction(&Instruction::End)
            .instruction(&Instruction::End);
        code.function(&function);
    } else {
        types.ty().function(
            [
                value,
                ValType::Ref(RefType {
                    nullable: false,
                    heap_type: HeapType::Concrete(3),
                }),
                ValType::I32,
                ValType::I32,
            ],
            [value],
        ); // 10
        types
            .ty()
            .function([value, ValType::I32, ValType::I32], [value]); // 11
        imports.import("rt", "closure-new", EntityType::Function(10));
        functions.function(3).function(11);
        exports.export("make", ExportKind::Func, 2);
        elements.declared(Elements::Functions(Cow::Borrowed(&[1])));
        let mut callback = Function::new([]);
        callback
            .instruction(&Instruction::LocalGet(0))
            .instruction(&Instruction::End);
        code.function(&callback);
        let mut make = Function::new([]);
        make.instruction(&Instruction::LocalGet(0))
            .instruction(&Instruction::RefFunc(1))
            .instruction(&Instruction::LocalGet(1))
            .instruction(&Instruction::LocalGet(2))
            .instruction(&Instruction::Call(0))
            .instruction(&Instruction::End);
        code.function(&make);
    }
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&exports);
    if !catcher {
        module.section(&elements);
    }
    module
        .section(&code)
        .section(&runtime_abi::Manifest::default().section());
    module.finish()
}

#[test]
fn runtime_abi_closures_check_arity_and_keep_old_captures_after_rebinding() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut linker = wasmtime::Linker::new(&engine);
    linker.instance(&mut store, "rt", runtime).unwrap();
    let producer = linker
        .instantiate(
            &mut store,
            &Module::new(&engine, closure_fragment(false)).unwrap(),
        )
        .unwrap();
    let consumer = linker
        .instantiate(
            &mut store,
            &Module::new(&engine, closure_fragment(true)).unwrap(),
        )
        .unwrap();
    let boxed = runtime.get_func(&mut store, "number-box").unwrap();
    let make = producer.get_func(&mut store, "make").unwrap();
    let invoke = consumer.get_func(&mut store, "call").unwrap();
    let mut closures = Vec::new();
    for n in [40.0f64, 100.0] {
        let mut environment = [Val::null_any_ref()];
        boxed
            .call(&mut store, &[Val::F64(n.to_bits())], &mut environment)
            .unwrap();
        let mut closure = [Val::null_any_ref()];
        make.call(
            &mut store,
            &[environment[0].clone(), Val::I32(1), Val::I32(1)],
            &mut closure,
        )
        .unwrap();
        closures.push(closure[0].clone());
    }
    let mut binding = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "binding-new")
        .unwrap()
        .call(&mut store, &[closures[0].clone()], &mut binding)
        .unwrap();
    runtime
        .get_func(&mut store, "binding-set")
        .unwrap()
        .call(
            &mut store,
            &[binding[0].clone(), closures[1].clone()],
            &mut [],
        )
        .unwrap();
    store.gc(None).expect("forced collection");
    let mut current = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "binding-get")
        .unwrap()
        .call(&mut store, &binding, &mut current)
        .unwrap();
    let mut args = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "args-new")
        .unwrap()
        .call(&mut store, &[Val::I32(1)], &mut args)
        .unwrap();
    for (closure, expected) in [(closures[0].clone(), 40.0), (current[0].clone(), 100.0)] {
        let mut result = [Val::null_any_ref()];
        invoke
            .call(&mut store, &[closure, args[0].clone()], &mut result)
            .unwrap();
        let Val::AnyRef(Some(reference)) = &result[0] else {
            panic!("expected captured box")
        };
        let fields: Vec<_> = reference
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .fields(&mut store)
            .unwrap()
            .collect();
        assert_eq!(fields[0].f64(), Some(expected));
    }
    let mut bad_args = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "args-new")
        .unwrap()
        .call(&mut store, &[Val::I32(0)], &mut bad_args)
        .unwrap();
    let mut exception = [Val::null_any_ref()];
    invoke
        .call(
            &mut store,
            &[closures[0].clone(), bad_args[0].clone()],
            &mut exception,
        )
        .unwrap();
    let Val::AnyRef(Some(reference)) = &exception[0] else {
        panic!("expected arity exception")
    };
    let fields: Vec<_> = reference
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .fields(&mut store)
        .unwrap()
        .collect();
    assert_eq!(fields.len(), 4);
    let Val::AnyRef(Some(message)) = &fields[1] else {
        panic!("expected diagnostic")
    };
    let message = message.as_array(&store).unwrap().unwrap();
    let units: Vec<_> = (0..message.len(&store).unwrap())
        .map(|i| message.get(&mut store, i).unwrap().i32().unwrap() as u16)
        .collect();
    assert_eq!(String::from_utf16(&units).unwrap(), "Wrong arity");
    for field in &fields[2..] {
        let Val::AnyRef(Some(nil)) = field else {
            panic!("expected nil sentinel")
        };
        assert_eq!(nil.as_i31(&store).unwrap().unwrap().get_i32(), 0);
    }
    runtime
        .get_func(&mut store, "args-new")
        .unwrap()
        .call(&mut store, &[Val::I32(2)], &mut bad_args)
        .unwrap();
    invoke
        .call(
            &mut store,
            &[closures[0].clone(), bad_args[0].clone()],
            &mut exception,
        )
        .unwrap();
    let Val::AnyRef(Some(reference)) = &exception[0] else {
        panic!("expected upper-bound exception")
    };
    assert_eq!(
        reference
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .fields(&mut store)
            .unwrap()
            .count(),
        4
    );
    let mut environment = [Val::null_any_ref()];
    boxed
        .call(&mut store, &[Val::F64(40.0f64.to_bits())], &mut environment)
        .unwrap();
    let mut variadic = [Val::null_any_ref()];
    make.call(
        &mut store,
        &[environment[0].clone(), Val::I32(1), Val::I32(-1)],
        &mut variadic,
    )
    .unwrap();
    let mut result = [Val::null_any_ref()];
    invoke
        .call(
            &mut store,
            &[variadic[0].clone(), bad_args[0].clone()],
            &mut result,
        )
        .unwrap();
    let Val::AnyRef(Some(reference)) = &result[0] else {
        panic!("expected variadic result")
    };
    let fields: Vec<_> = reference
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .fields(&mut store)
        .unwrap()
        .collect();
    assert_eq!(fields[0].f64(), Some(40.0));
}

#[test]
fn runtime_abi_numeric_operations_and_sentinels_execute() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    for (name, expected) in [("nil", 0), ("false", 2), ("true", 4)] {
        let mut result = [Val::null_any_ref()];
        runtime
            .get_func(&mut store, name)
            .unwrap()
            .call(&mut store, &[], &mut result)
            .unwrap();
        let Val::AnyRef(Some(reference)) = &result[0] else {
            panic!("expected sentinel")
        };
        assert_eq!(
            reference.as_i31(&store).unwrap().unwrap().get_i32(),
            expected
        );
    }
    for (name, a, b, expected) in [
        ("number-subtract", 3.0f64, 5.0f64, -2.0f64),
        ("number-multiply", 2.5, -4.0, -10.0),
        ("number-divide", 1.0, 0.0, f64::INFINITY),
        ("number-divide", 0.0, 0.0, f64::NAN),
    ] {
        let mut args = Vec::new();
        for number in [a, b] {
            let mut value = [Val::null_any_ref()];
            runtime
                .get_func(&mut store, "number-box")
                .unwrap()
                .call(&mut store, &[Val::F64(number.to_bits())], &mut value)
                .unwrap();
            args.push(value[0].clone());
        }
        let mut value = [Val::null_any_ref()];
        runtime
            .get_func(&mut store, name)
            .unwrap()
            .call(&mut store, &args, &mut value)
            .unwrap();
        let Val::AnyRef(Some(reference)) = &value[0] else {
            panic!("expected number")
        };
        let fields: Vec<_> = reference
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .fields(&mut store)
            .unwrap()
            .collect();
        let actual = fields[0].f64().unwrap();
        if expected.is_nan() {
            assert!(actual.is_nan());
        } else {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    }
    let mut zero = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "number-box")
        .unwrap()
        .call(&mut store, &[Val::F64(0)], &mut zero)
        .unwrap();
    let mut negative = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "number-negate")
        .unwrap()
        .call(&mut store, &zero, &mut negative)
        .unwrap();
    let Val::AnyRef(Some(reference)) = &negative[0] else {
        panic!("expected number")
    };
    let fields: Vec<_> = reference
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .fields(&mut store)
        .unwrap()
        .collect();
    assert_eq!(fields[0].f64().unwrap().to_bits(), (-0.0f64).to_bits());
}

#[test]
fn runtime_abi_arithmetic_function_values_check_arity_and_fold_in_order() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let invoke = runtime.get_func(&mut store, "invoke").unwrap();
    for (name, inputs, expected) in [
        ("add", vec![], 0.0),
        ("multiply", vec![], 1.0),
        ("add", vec![2.0], 2.0),
        ("multiply", vec![2.0], 2.0),
        ("subtract", vec![0.0], -0.0),
        ("divide", vec![4.0], 0.25),
        ("add", vec![1.0, 2.0, 3.0], 6.0),
        ("multiply", vec![2.0, 3.0, 4.0], 24.0),
        ("subtract", vec![10.0, 2.0, 3.0], 5.0),
        ("divide", vec![24.0, 3.0, 2.0], 4.0),
    ] {
        let mut closure = [Val::null_any_ref()];
        runtime
            .get_func(&mut store, &format!("arithmetic-{name}"))
            .unwrap()
            .call(&mut store, &[], &mut closure)
            .unwrap();
        let object = closure[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(
            object.field(&mut store, 2).unwrap().unwrap_i32(),
            if matches!(name, "add" | "multiply") {
                0
            } else {
                1
            }
        );
        assert_eq!(object.field(&mut store, 3).unwrap().unwrap_i32(), -1);
        let mut args = [Val::null_any_ref()];
        runtime
            .get_func(&mut store, "args-new")
            .unwrap()
            .call(&mut store, &[Val::I32(inputs.len() as i32)], &mut args)
            .unwrap();
        let array = args[0]
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        for (index, input) in inputs.into_iter().enumerate() {
            let mut value = [Val::null_any_ref()];
            runtime
                .get_func(&mut store, "number-box")
                .unwrap()
                .call(&mut store, &[Val::F64(f64::to_bits(input))], &mut value)
                .unwrap();
            array
                .set(&mut store, index as u32, value[0].clone())
                .unwrap();
        }
        store.gc(None).unwrap();
        let mut result = [Val::null_any_ref()];
        invoke
            .call(
                &mut store,
                &[closure[0].clone(), args[0].clone()],
                &mut result,
            )
            .unwrap();
        let object = result[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(
            object.field(&mut store, 0).unwrap().unwrap_f64().to_bits(),
            f64::to_bits(expected),
            "{name}"
        );
    }
    for name in ["subtract", "divide"] {
        let mut closure = [Val::null_any_ref()];
        runtime
            .get_func(&mut store, &format!("arithmetic-{name}"))
            .unwrap()
            .call(&mut store, &[], &mut closure)
            .unwrap();
        let mut args = [Val::null_any_ref()];
        runtime
            .get_func(&mut store, "args-new")
            .unwrap()
            .call(&mut store, &[Val::I32(0)], &mut args)
            .unwrap();
        let error = invoke
            .call(
                &mut store,
                &[closure[0].clone(), args[0].clone()],
                &mut [Val::null_any_ref()],
            )
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>());
        let exception = store.take_pending_exception().unwrap();
        assert!(wasmtime::Tag::eq(
            &exception.tag(&mut store).unwrap(),
            &runtime.get_tag(&mut store, "language-exception").unwrap(),
            &store
        ));
        let value = exception.field(&mut store, 0).unwrap();
        let object = value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        let descriptor = object
            .field(&mut store, 0)
            .unwrap()
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(descriptor.field(&mut store, 0).unwrap().unwrap_i64(), 1);
    }
}

// Real independently emitted callers; all nominal values use the shared prelude.
fn nominal_fragment() -> Vec<u8> {
    use wasm_encoder::*;
    let value = ValType::Ref(RefType::EQREF);
    let signatures = [
        ("descriptor-new", vec![value], vec![value]),
        ("constructor-new", vec![value], vec![value]),
        ("constructor-descriptor", vec![value], vec![value]),
        ("protocol-dispatcher-new", vec![value], vec![value]),
        ("object-new", vec![value, value], vec![value]),
        ("object-instance", vec![value, value], vec![ValType::I32]),
        ("object-descriptor", vec![value], vec![value]),
        ("object-field-get", vec![value, ValType::I32], vec![value]),
        ("object-field-set", vec![value, ValType::I32, value], vec![]),
        ("protocol-method-get", vec![value, value], vec![value]),
        ("protocol-method-set", vec![value, value, value], vec![]),
    ];
    let mut types = runtime_abi::prelude();
    let mut imports = ImportSection::new();
    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    for (i, (name, params, results)) in signatures.iter().enumerate() {
        let index = i as u32;
        types
            .ty()
            .function(params.iter().copied(), results.iter().copied());
        imports.import("rt", name, EntityType::Function(10 + index));
        functions.function(10 + index);
        exports.export(name, ExportKind::Func, signatures.len() as u32 + index);
        let mut function = Function::new([]);
        for argument in 0..params.len() {
            function.instruction(&Instruction::LocalGet(argument as u32));
        }
        function
            .instruction(&Instruction::Call(index))
            .instruction(&Instruction::End);
        code.function(&function);
    }
    // Deliberately malformed descriptors remain valid Wasm with the exact prelude.
    // They probe guards and prove numeric identity alone cannot forge a type.
    types.ty().function([value, value, ValType::I64], [value]);
    functions.function(10 + signatures.len() as u32);
    exports.export(
        "forge-descriptor",
        ExportKind::Func,
        2 * signatures.len() as u32,
    );
    let mut forge = Function::new([]);
    forge
        .instruction(&Instruction::LocalGet(2))
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::LocalGet(1))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::RefI31)
        .instruction(&Instruction::StructNew(6))
        .instruction(&Instruction::End);
    code.function(&forge);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&exports)
        .section(&code)
        .section(&runtime_abi::Manifest::default().section());
    module.finish()
}

fn nominal_value(store: &mut Store<()>, instance: Instance, name: &str, args: &[Val]) -> Val {
    let mut result = [Val::null_any_ref()];
    instance
        .get_func(&mut *store, name)
        .unwrap()
        .call(store, args, &mut result)
        .unwrap();
    result[0].clone()
}

#[test]
fn runtime_abi_nominal_identity_fields_and_live_protocols_cross_fragments_after_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut linker = wasmtime::Linker::new(&engine);
    linker.instance(&mut store, "rt", runtime).unwrap();
    let bytes = nominal_fragment();
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
    let module = Module::new(&engine, &bytes).unwrap();
    let producer = linker.instantiate(&mut store, &module).unwrap();
    let consumer = linker.instantiate(&mut store, &module).unwrap();
    let closures = linker
        .instantiate(
            &mut store,
            &Module::new(&engine, closure_fragment(false)).unwrap(),
        )
        .unwrap();
    let schema = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let a = nominal_value(&mut store, producer, "descriptor-new", &[schema.clone()]);
    let b = nominal_value(&mut store, producer, "descriptor-new", &[schema.clone()]);
    let key = nominal_value(&mut store, producer, "descriptor-new", &[schema.clone()]);
    let other_key = nominal_value(&mut store, producer, "descriptor-new", &[schema.clone()]);
    let object = nominal_value(
        &mut store,
        producer,
        "object-new",
        &[a.clone(), schema.clone()],
    );
    store.gc(None).unwrap();
    for (descriptor, expected) in [(&a, 1), (&b, 0)] {
        let mut result = [Val::I32(-1)];
        consumer
            .get_func(&mut store, "object-instance")
            .unwrap()
            .call(
                &mut store,
                &[descriptor.clone(), object.clone()],
                &mut result,
            )
            .unwrap();
        assert_eq!(result[0].unwrap_i32(), expected);
    }
    let a_identity = a
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 0)
        .unwrap()
        .unwrap_i64();
    let b_identity = b
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 0)
        .unwrap()
        .unwrap_i64();
    assert!(a_identity >= 8 && b_identity > a_identity);
    let forged = nominal_value(
        &mut store,
        producer,
        "forge-descriptor",
        &[schema.clone(), schema.clone(), Val::I64(a_identity)],
    );
    assert_eq!(
        forged
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_i64(),
        a_identity
    );
    let mut forged_instance = [Val::I32(-1)];
    consumer
        .get_func(&mut store, "object-instance")
        .unwrap()
        .call(&mut store, &[forged, object.clone()], &mut forged_instance)
        .unwrap();
    assert_eq!(forged_instance[0].unwrap_i32(), 0);

    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    for value in [
        nil.clone(),
        nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64(7.0f64.to_bits())],
        ),
    ] {
        let mut result = [Val::I32(-1)];
        consumer
            .get_func(&mut store, "object-instance")
            .unwrap()
            .call(&mut store, &[a.clone(), value], &mut result)
            .unwrap();
        assert_eq!(result[0].unwrap_i32(), 0);
    }
    let constructor = nominal_value(&mut store, producer, "constructor-new", &[a.clone()]);
    let through_invoke = nominal_value(
        &mut store,
        runtime,
        "invoke",
        &[constructor.clone(), schema.clone()],
    );
    let mut instance = [Val::I32(0)];
    consumer
        .get_func(&mut store, "object-instance")
        .unwrap()
        .call(&mut store, &[a.clone(), through_invoke], &mut instance)
        .unwrap();
    assert_eq!(instance[0].unwrap_i32(), 1);
    let retained_descriptor = nominal_value(
        &mut store,
        consumer,
        "constructor-descriptor",
        &[constructor],
    );
    assert!(
        wasmtime::Rooted::ref_eq(
            &store,
            retained_descriptor.unwrap_anyref().unwrap(),
            a.unwrap_anyref().unwrap()
        )
        .unwrap()
    );
    let dispatcher = nominal_value(
        &mut store,
        producer,
        "protocol-dispatcher-new",
        &[key.clone()],
    );
    let mut expected = 0;
    for iteration in 0..3 {
        let method = nominal_value(
            &mut store,
            closures,
            "make",
            &[object.clone(), Val::I32(1), Val::I32(1)],
        );
        // Add two keys, then replace the first method without losing the second.
        let chosen = if iteration == 1 { &other_key } else { &key };
        consumer
            .get_func(&mut store, "protocol-method-set")
            .unwrap()
            .call(&mut store, &[a.clone(), chosen.clone(), method], &mut [])
            .unwrap();
        let field = nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64((7.0 + iteration as f64).to_bits())],
        );
        consumer
            .get_func(&mut store, "object-field-set")
            .unwrap()
            .call(&mut store, &[object.clone(), Val::I32(0), field], &mut [])
            .unwrap();
        expected = 7 + iteration;
        store.gc(None).unwrap();
        let descriptor =
            nominal_value(&mut store, producer, "object-descriptor", &[object.clone()]);
        let method = nominal_value(
            &mut store,
            producer,
            "protocol-method-get",
            &[descriptor, key.clone()],
        );
        let result = nominal_value(&mut store, runtime, "invoke", &[method, schema.clone()]);
        let retained_field = nominal_value(
            &mut store,
            producer,
            "object-field-get",
            &[result, Val::I32(0)],
        );
        let mut decoded = [Val::F64(0)];
        runtime
            .get_func(&mut store, "number-unbox")
            .unwrap()
            .call(&mut store, &[retained_field], &mut decoded)
            .unwrap();
        assert_eq!(decoded[0].unwrap_f64(), expected as f64);
    }
    assert_eq!(expected, 9);
    let receiver_args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    receiver_args
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, object.clone())
        .unwrap();
    let old_method = nominal_value(
        &mut store,
        consumer,
        "protocol-method-get",
        &[a.clone(), key.clone()],
    );
    let replacement_object = nominal_value(
        &mut store,
        producer,
        "object-new",
        &[b.clone(), schema.clone()],
    );
    let replacement_method = nominal_value(
        &mut store,
        closures,
        "make",
        &[replacement_object.clone(), Val::I32(1), Val::I32(1)],
    );
    consumer
        .get_func(&mut store, "protocol-method-set")
        .unwrap()
        .call(
            &mut store,
            &[a.clone(), key.clone(), replacement_method],
            &mut [],
        )
        .unwrap();
    store.gc(None).unwrap();
    let updated = nominal_value(
        &mut store,
        runtime,
        "invoke",
        &[dispatcher, receiver_args.clone()],
    );
    assert!(
        wasmtime::Rooted::ref_eq(
            &store,
            updated.unwrap_anyref().unwrap(),
            replacement_object.unwrap_anyref().unwrap()
        )
        .unwrap()
    );
    let old = nominal_value(
        &mut store,
        runtime,
        "invoke",
        &[old_method, receiver_args.clone()],
    );
    assert!(
        wasmtime::Rooted::ref_eq(
            &store,
            old.unwrap_anyref().unwrap(),
            object.unwrap_anyref().unwrap()
        )
        .unwrap()
    );
    let other = nominal_value(
        &mut store,
        consumer,
        "protocol-method-get",
        &[a.clone(), other_key],
    );
    let other = nominal_value(&mut store, runtime, "invoke", &[other, receiver_args]);
    assert!(
        wasmtime::Rooted::ref_eq(
            &store,
            other.unwrap_anyref().unwrap(),
            object.unwrap_anyref().unwrap()
        )
        .unwrap()
    );
    let missing = nominal_value(&mut store, consumer, "protocol-method-get", &[b, key]);
    assert_eq!(
        missing
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_i32(),
        0
    );
    // Field construction copied the argument storage; mutation did not rewrite it.
    let original = schema
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .get(&mut store, 0)
        .unwrap();
    assert_eq!(
        original
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_i32(),
        0
    );
}

#[test]
fn runtime_abi_nominal_bad_inputs_throw_language_errors_before_storage_access() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let schema = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[schema.clone()]);
    let object = nominal_value(
        &mut store,
        runtime,
        "object-new",
        &[descriptor.clone(), schema.clone()],
    );
    let mut linker = wasmtime::Linker::new(&engine);
    linker.instance(&mut store, "rt", runtime).unwrap();
    let fragment = linker
        .instantiate(
            &mut store,
            &Module::new(&engine, nominal_fragment()).unwrap(),
        )
        .unwrap();
    let invalid_schema = nominal_value(
        &mut store,
        fragment,
        "forge-descriptor",
        &[nil.clone(), schema.clone(), Val::I64(8)],
    );
    let invalid_table = nominal_value(
        &mut store,
        fragment,
        "forge-descriptor",
        &[schema.clone(), nil.clone(), Val::I64(8)],
    );
    let odd_table = nominal_value(
        &mut store,
        fragment,
        "forge-descriptor",
        &[schema.clone(), schema.clone(), Val::I64(8)],
    );
    let dispatcher = nominal_value(
        &mut store,
        runtime,
        "protocol-dispatcher-new",
        &[descriptor.clone()],
    );
    let cell = nominal_value(&mut store, runtime, "binding-unbound", &[]);
    for (name, args, returns_value) in [
        (
            "protocol-live-dispatcher-new",
            vec![invalid_schema.clone(), cell.clone()],
            true,
        ),
        (
            "protocol-live-dispatcher-new",
            vec![descriptor.clone(), nil.clone()],
            true,
        ),
        (
            "protocol-native-invoke",
            vec![dispatcher.clone(), nil.clone()],
            true,
        ),
        (
            "closure-property-set",
            vec![nil.clone(), Val::I32(0), nil.clone()],
            true,
        ),
        ("constructor-descriptor", vec![dispatcher], true),
        ("descriptor-new", vec![nil.clone()], true),
        ("object-new", vec![invalid_schema, schema.clone()], true),
        (
            "protocol-method-get",
            vec![invalid_table, descriptor.clone()],
            true,
        ),
        (
            "protocol-method-get",
            vec![odd_table, descriptor.clone()],
            true,
        ),
        ("descriptor-new", vec![Val::null_any_ref()], true),
        ("constructor-new", vec![nil.clone()], true),
        ("constructor-descriptor", vec![nil.clone()], true),
        ("protocol-dispatcher-new", vec![nil.clone()], true),
        ("object-new", vec![nil.clone(), schema.clone()], true),
        ("object-new", vec![descriptor.clone(), nil.clone()], true),
        ("object-new", vec![descriptor.clone(), empty], true),
        ("object-instance", vec![nil.clone(), object.clone()], false),
        ("object-descriptor", vec![nil.clone()], true),
        ("object-field-get", vec![object.clone(), Val::I32(-1)], true),
        ("object-field-get", vec![object.clone(), Val::I32(1)], true),
        (
            "object-field-set",
            vec![object.clone(), Val::I32(1), nil.clone()],
            false,
        ),
        ("object-field-get", vec![nil.clone(), Val::I32(0)], true),
        (
            "protocol-method-get",
            vec![nil.clone(), descriptor.clone()],
            true,
        ),
        (
            "protocol-method-get",
            vec![descriptor.clone(), nil.clone()],
            true,
        ),
        (
            "protocol-method-set",
            vec![descriptor.clone(), descriptor.clone(), nil.clone()],
            false,
        ),
    ] {
        let mut result = if returns_value {
            vec![Val::null_any_ref()]
        } else if name == "object-instance" {
            vec![Val::I32(0)]
        } else {
            vec![]
        };
        let error = runtime
            .get_func(&mut store, name)
            .unwrap()
            .call(&mut store, &args, &mut result)
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{name}: {error:#}");
        let exception = store.take_pending_exception().unwrap();
        let tag = exception.tag(&mut store).unwrap();
        assert!(wasmtime::Tag::eq(
            &tag,
            &runtime.get_tag(&mut store, "language-exception").unwrap(),
            &store
        ));
        let payload = exception.field(&mut store, 0).unwrap();
        let payload = payload
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        let error_descriptor = payload.field(&mut store, 0).unwrap();
        let identity = error_descriptor
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_i64();
        assert_eq!(identity, 7, "{name}");
    }
    // Rejection did not change a field or protocol table, and prompt use resumes.
    let field = nominal_value(
        &mut store,
        runtime,
        "object-field-get",
        &[object, Val::I32(0)],
    );
    assert_eq!(
        field
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_i32(),
        0
    );
    let missing = nominal_value(
        &mut store,
        runtime,
        "protocol-method-get",
        &[descriptor.clone(), descriptor],
    );
    assert_eq!(
        missing
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_i32(),
        0
    );
}

#[test]
fn runtime_abi_nominal_callables_reject_arity_missing_methods_and_non_objects() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let one = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let none = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[one.clone()]);
    let key = nominal_value(&mut store, runtime, "descriptor-new", &[one.clone()]);
    let constructor = nominal_value(
        &mut store,
        runtime,
        "constructor-new",
        &[descriptor.clone()],
    );
    let dispatcher = nominal_value(
        &mut store,
        runtime,
        "protocol-dispatcher-new",
        &[key.clone()],
    );
    let object = nominal_value(
        &mut store,
        runtime,
        "invoke",
        &[constructor.clone(), one.clone()],
    );
    let receiver = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    receiver
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, object.clone())
        .unwrap();
    for (callable, args, identity, message) in [
        (constructor, none.clone(), 1, "Wrong arity"),
        (dispatcher.clone(), none, 1, "Wrong arity"),
        (
            dispatcher.clone(),
            one.clone(),
            7,
            "Invalid nominal operation",
        ),
        (
            dispatcher.clone(),
            receiver.clone(),
            7,
            "Invalid nominal operation",
        ),
    ] {
        let error = runtime
            .get_func(&mut store, "invoke")
            .unwrap()
            .call(&mut store, &[callable, args], &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>());
        let exception = store.take_pending_exception().unwrap();
        let payload = exception.field(&mut store, 0).unwrap();
        let payload = payload
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        let descriptor = payload.field(&mut store, 0).unwrap();
        let descriptor = descriptor
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(
            descriptor.field(&mut store, 0).unwrap().unwrap_i64(),
            identity
        );
        let text = payload.field(&mut store, 1).unwrap();
        let text = text
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        let units = text
            .elems(&mut store)
            .unwrap()
            .map(|unit| unit.unwrap_i32() as u16)
            .collect::<Vec<_>>();
        assert_eq!(String::from_utf16(&units).unwrap(), message);
    }
    // A newly installed method makes the same captured dispatcher callable.
    let mut linker = wasmtime::Linker::new(&engine);
    linker.instance(&mut store, "rt", runtime).unwrap();
    let closures = linker
        .instantiate(
            &mut store,
            &Module::new(&engine, closure_fragment(false)).unwrap(),
        )
        .unwrap();
    let number = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(42.0f64.to_bits())],
    );
    let method = nominal_value(
        &mut store,
        closures,
        "make",
        &[number.clone(), Val::I32(1), Val::I32(1)],
    );
    runtime
        .get_func(&mut store, "protocol-method-set")
        .unwrap()
        .call(&mut store, &[descriptor, key, method], &mut [])
        .unwrap();
    store.gc(None).unwrap();
    let result = nominal_value(&mut store, runtime, "invoke", &[dispatcher, receiver]);
    assert!(
        wasmtime::Rooted::ref_eq(
            &store,
            result.unwrap_anyref().unwrap(),
            number.unwrap_anyref().unwrap()
        )
        .unwrap()
    );
}

#[test]
fn runtime_abi_exception_info_getters_reject_malformed_named_schema_without_trapping() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    // Storage is well typed, but nil is not a field name. This is forged host
    // input; source constructors supply UTF-16 names.
    let schema = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[schema.clone()]);
    let class = nominal_value(
        &mut store,
        runtime,
        "class-value-new",
        &[descriptor.clone()],
    );
    let class_cell = nominal_value(&mut store, runtime, "binding-new", &[class]);
    let object = nominal_value(&mut store, runtime, "object-new", &[descriptor, schema]);
    let arguments = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    arguments
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, object)
        .unwrap();
    for export in ["core-ex-data", "core-ex-cause"] {
        let getter = nominal_value(&mut store, runtime, export, &[class_cell.clone()]);
        store.gc(None).unwrap();
        let error = runtime
            .get_func(&mut store, "invoke")
            .unwrap()
            .call(
                &mut store,
                &[getter, arguments.clone()],
                &mut [Val::null_any_ref()],
            )
            .unwrap_err();
        assert!(
            error.is::<wasmtime::ThrownException>(),
            "{export}: {error:#}"
        );
        assert!(!error.is::<wasmtime::Trap>());
        let exception = store.take_pending_exception().unwrap();
        let tag = exception.tag(&mut store).unwrap();
        assert!(wasmtime::Tag::eq(
            &tag,
            &runtime.get_tag(&mut store, "language-exception").unwrap(),
            &store
        ));
    }
}

#[test]
fn runtime_abi_closure_properties_keep_assigned_values_through_gc_and_reject_bad_keys() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut function = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "predicate-nil")
        .unwrap()
        .call(&mut store, &[], &mut function)
        .unwrap();
    let set = runtime
        .get_func(&mut store, "closure-property-set")
        .unwrap();
    let get = runtime
        .get_func(&mut store, "closure-property-get")
        .unwrap();
    let boxed = runtime.get_func(&mut store, "number-box").unwrap();
    // The only surviving root for each assigned Number is its owning closure.
    for kind in 0..8 {
        let mut scope = wasmtime::RootScope::new(&mut store);
        let mut value = [Val::null_any_ref()];
        boxed
            .call(
                &mut scope,
                &[Val::F64((40.0 + f64::from(kind)).to_bits())],
                &mut value,
            )
            .unwrap();
        let mut returned = [Val::null_any_ref()];
        set.call(
            &mut scope,
            &[function[0].clone(), Val::I32(kind), value[0].clone()],
            &mut returned,
        )
        .unwrap();
    }
    store.gc(None).unwrap();
    for kind in 0..8 {
        let mut value = [Val::null_any_ref()];
        get.call(
            &mut store,
            &[function[0].clone(), Val::I32(kind)],
            &mut value,
        )
        .unwrap();
        let object = value[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(
            object.field(&mut store, 0).unwrap().unwrap_f64(),
            40.0 + f64::from(kind)
        );
    }
    for kind in [-1, 8] {
        let error = set
            .call(
                &mut store,
                &[function[0].clone(), Val::I32(kind), function[0].clone()],
                &mut [Val::null_any_ref()],
            )
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>());
        assert!(!error.is::<wasmtime::Trap>());
    }
    // Property mutation does not replace the callback's original environment.
    let mut environment = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "closure-environment")
        .unwrap()
        .call(&mut store, &function, &mut environment)
        .unwrap();
    assert_eq!(
        environment[0]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0
    );
    let mut args = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "args-new")
        .unwrap()
        .call(&mut store, &[Val::I32(1)], &mut args)
        .unwrap();
    let mut result = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "invoke")
        .unwrap()
        .call(
            &mut store,
            &[function[0].clone(), args[0].clone()],
            &mut result,
        )
        .unwrap();
    assert_eq!(
        result[0]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        4
    );
}

#[test]
fn runtime_abi_accepts_foreign_raw_closure_environments_without_properties() {
    use std::borrow::Cow;
    use wasm_encoder::*;
    let value = ValType::Ref(RefType::EQREF);
    let mut types = runtime_abi::prelude();
    types.ty().function([value], [value]);
    let mut functions = FunctionSection::new();
    functions.function(3).function(10);
    let mut exports = ExportSection::new();
    exports.export("make", ExportKind::Func, 1);
    let mut elements = ElementSection::new();
    elements.declared(Elements::Functions(Cow::Borrowed(&[0])));
    let mut code = CodeSection::new();
    let mut callback = Function::new([]);
    callback
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::End);
    code.function(&callback);
    let mut make = Function::new([]);
    make.instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefFunc(0))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::StructNew(4))
        .instruction(&Instruction::End);
    code.function(&make);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&functions)
        .section(&exports)
        .section(&elements)
        .section(&code)
        .section(&runtime_abi::Manifest::default().section());
    let bytes = module.finish();
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &wasmtime::Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let foreign = Instance::new(
        &mut store,
        &wasmtime::Module::new(&engine, bytes).unwrap(),
        &[],
    )
    .unwrap();
    let mut environment = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "number-box")
        .unwrap()
        .call(&mut store, &[Val::F64(7.0f64.to_bits())], &mut environment)
        .unwrap();
    let mut function = [Val::null_any_ref()];
    foreign
        .get_func(&mut store, "make")
        .unwrap()
        .call(&mut store, &environment, &mut function)
        .unwrap();
    store.gc(None).unwrap();
    let mut args = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "args-new")
        .unwrap()
        .call(&mut store, &[Val::I32(0)], &mut args)
        .unwrap();
    let mut result = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "invoke")
        .unwrap()
        .call(
            &mut store,
            &[function[0].clone(), args[0].clone()],
            &mut result,
        )
        .unwrap();
    assert!(
        wasmtime::Rooted::ref_eq(
            &store,
            result[0].unwrap_anyref().unwrap(),
            environment[0].unwrap_anyref().unwrap()
        )
        .unwrap()
    );
    runtime
        .get_func(&mut store, "closure-property-get")
        .unwrap()
        .call(&mut store, &[function[0].clone(), Val::I32(0)], &mut result)
        .unwrap();
    assert_eq!(
        result[0]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0
    );
    let error = runtime
        .get_func(&mut store, "closure-property-set")
        .unwrap()
        .call(
            &mut store,
            &[function[0].clone(), Val::I32(0), environment[0].clone()],
            &mut result,
        )
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>());
    assert!(!error.is::<wasmtime::Trap>());
}

#[test]
fn runtime_abi_source_arrays_copy_argument_buffers_and_own_values_through_growth_and_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let buffer = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let value = nominal_value(&mut store, runtime, "source-array-new", &[buffer.clone()]);
    let zero = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(0.0f64.to_bits())],
    );
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    {
        let mut scope = wasmtime::RootScope::new(&mut store);
        let mut number = [Val::null_any_ref()];
        runtime
            .get_func(&mut scope, "number-box")
            .unwrap()
            .call(&mut scope, &[Val::F64(7.0f64.to_bits())], &mut number)
            .unwrap();
        runtime
            .get_func(&mut scope, "source-array-set")
            .unwrap()
            .call(
                &mut scope,
                &[value.clone(), zero.clone(), number[0].clone()],
                &mut [Val::null_any_ref()],
            )
            .unwrap();
    }
    // Mutating the callback's argument array cannot mutate the source array.
    buffer
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, nil.clone())
        .unwrap();
    let clone = nominal_value(&mut store, runtime, "source-array-clone", &[value.clone()]);
    assert!(
        !wasmtime::Rooted::ref_eq(
            &store,
            value.unwrap_anyref().unwrap(),
            clone.unwrap_anyref().unwrap()
        )
        .unwrap()
    );
    let three = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(3.0f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-set",
        &[value.clone(), three, clone.clone()],
    );
    store.gc(None).unwrap();
    for array in [&value, &clone] {
        let item = nominal_value(
            &mut store,
            runtime,
            "source-array-get",
            &[array.clone(), zero.clone()],
        );
        let object = item
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), 7.0);
    }
    for (array, count) in [(value, 4.0), (clone, 1.0)] {
        let length = nominal_value(&mut store, runtime, "source-array-length", &[array]);
        let object = length
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), count);
    }
}

#[test]
fn runtime_abi_source_array_bad_inputs_and_capacity_fail_as_language_errors() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let value = nominal_value(&mut store, runtime, "source-array-new", &[empty.clone()]);
    let huge = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(1_000_001.0f64.to_bits())],
    );
    let dimensions = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    dimensions
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, huge.clone())
        .unwrap();
    for (export, args) in [
        ("source-array-new", vec![nil.clone()]),
        ("source-array-fields", vec![nil.clone()]),
        ("source-array-storage", vec![nil.clone()]),
        ("source-array-length", vec![nil.clone()]),
        ("source-array-clone", vec![nil.clone()]),
        ("source-array-index", vec![nil.clone()]),
        ("source-array-get", vec![nil.clone(), huge.clone()]),
        (
            "source-array-set",
            vec![value.clone(), huge.clone(), nil.clone()],
        ),
        ("source-array-make", vec![nil.clone()]),
        ("source-array-make-literal", vec![nil.clone()]),
        ("source-array-make-literal", vec![dimensions.clone()]),
        (
            "source-array-make-dimensions",
            vec![nil.clone(), Val::I32(0), Val::I32(6)],
        ),
        (
            "source-array-make-dimensions",
            vec![dimensions.clone(), Val::I32(-1), Val::I32(6)],
        ),
        (
            "source-array-make-dimensions",
            vec![dimensions, Val::I32(0), Val::I32(2)],
        ),
        ("source-array-length-args", vec![empty.clone()]),
        ("source-array-get-indices", vec![empty.clone()]),
        ("source-array-set-indices", vec![empty]),
    ] {
        let error = runtime
            .get_func(&mut store, export)
            .unwrap()
            .call(&mut store, &args, &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(
            error.is::<wasmtime::ThrownException>(),
            "{export}: {error:#}"
        );
        assert!(!error.is::<wasmtime::Trap>(), "{export}: {error:#}");
        assert!(store.take_pending_exception().is_some());
    }
    let dims = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    let size = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(2000.0f64.to_bits())],
    );
    let buffer = dims
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    buffer.set(&mut store, 0, size.clone()).unwrap();
    buffer.set(&mut store, 1, size).unwrap();
    let error = runtime
        .get_func(&mut store, "source-array-check-dimensions")
        .unwrap()
        .call(&mut store, &[dims, Val::I32(0)], &mut [])
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>());
    assert!(!error.is::<wasmtime::Trap>());
    store.take_pending_exception().unwrap();
    // Foreign ABI clients can corrupt private storage; guard it before ref.cast.
    let fields = value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 1)
        .unwrap();
    fields
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, nil)
        .unwrap();
    let error = runtime
        .get_func(&mut store, "source-array-length")
        .unwrap()
        .call(&mut store, &[value], &mut [Val::null_any_ref()])
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>());
    assert!(!error.is::<wasmtime::Trap>());
}

#[test]
fn runtime_abi_named_storage_preserves_native_keys_and_rejects_malformed_tables() {
    fn key(store: &mut Store<()>, runtime: Instance, text: &str) -> Val {
        let units = text.encode_utf16().collect::<Vec<_>>();
        let value = nominal_value(
            store,
            runtime,
            "string-new",
            &[Val::I32(units.len() as i32)],
        );
        let array = value
            .unwrap_anyref()
            .unwrap()
            .as_array(&*store)
            .unwrap()
            .unwrap();
        for (index, unit) in units.into_iter().enumerate() {
            array
                .set(&mut *store, index as u32, Val::I32(unit as i32))
                .unwrap();
        }
        value
    }
    fn language_error(store: &mut Store<()>, runtime: Instance, name: &str, args: &[Val]) {
        let mut results = if name == "property-find" {
            vec![Val::I32(0)]
        } else {
            vec![Val::null_any_ref()]
        };
        let error = runtime
            .get_func(&mut *store, name)
            .unwrap()
            .call(&mut *store, args, &mut results)
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{name}: {error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        let exception = store.take_pending_exception().unwrap();
        let tag = exception.tag(&mut *store).unwrap();
        assert!(wasmtime::Tag::eq(
            &tag,
            &runtime.get_tag(&mut *store, "language-exception").unwrap(),
            &*store
        ));
    }
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let owner = nominal_value(&mut store, runtime, "predicate-nil", &[]);
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let value = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(17.0f64.to_bits())],
    );
    let name = key(&mut store, runtime, "cache");
    let other_name = key(&mut store, runtime, "cache");
    nominal_value(
        &mut store,
        runtime,
        "closure-property-set",
        &[owner.clone(), Val::I32(0), value.clone()],
    );
    nominal_value(
        &mut store,
        runtime,
        "named-property-set",
        &[owner.clone(), name.clone(), nil.clone()],
    );
    store.gc(None).unwrap();
    let actual = nominal_value(
        &mut store,
        runtime,
        "named-property-get",
        &[owner.clone(), other_name.clone()],
    );
    assert_eq!(
        actual
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_i32(),
        0
    );
    let actual = nominal_value(
        &mut store,
        runtime,
        "closure-property-get",
        &[owner.clone(), Val::I32(0)],
    );
    let fields = actual
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .fields(&mut store)
        .unwrap()
        .collect::<Vec<_>>();
    assert!(matches!(fields.as_slice(), [Val::F64(bits)] if *bits == 17.0f64.to_bits()));
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    for stride in [0, 3] {
        language_error(
            &mut store,
            runtime,
            "property-find",
            &[empty.clone(), name.clone(), Val::I32(stride)],
        );
    }
    language_error(
        &mut store,
        runtime,
        "named-property-get",
        &[owner.clone(), nil.clone()],
    );
    language_error(
        &mut store,
        runtime,
        "named-property-set",
        &[nil.clone(), name.clone(), value.clone()],
    );
    // A host-forged opaque key is neither a native-kind key nor a UTF-16 name.
    let payload = nominal_value(
        &mut store,
        runtime,
        "closure-property-fields",
        &[owner.clone()],
    );
    let table = payload
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .get(&mut store, 1)
        .unwrap();
    table
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, value)
        .unwrap();
    language_error(&mut store, runtime, "named-property-get", &[owner, name]);
}
