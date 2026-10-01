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
    // Host-created raw __proto__ schemas must not bypass prototype boundaries.
    let proto_name = key(&mut store, runtime, "__proto__");
    let proto_schema = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    proto_schema.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap()
        .set(&mut store, 0, proto_name.clone()).unwrap();
    let proto_descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[proto_schema.clone()]);
    let proto_object = nominal_value(&mut store, runtime, "object-new", &[proto_descriptor, proto_schema]);
    language_error(&mut store, runtime, "named-property-get", &[proto_object.clone(), proto_name.clone()]);
    language_error(&mut store, runtime, "named-property-set", &[proto_object, proto_name.clone(), proto_name]);
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
    language_error(
        &mut store,
        runtime,
        "named-property-get",
        &[owner.clone(), name.clone()],
    );
    // A matching first key must not hide malformed later storage.
    let matching_then_opaque = nominal_value(&mut store, runtime, "args-new", &[Val::I32(4)]);
    let keys = matching_then_opaque
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    for (index, entry) in [name.clone(), nil.clone(), owner.clone(), nil.clone()]
        .into_iter()
        .enumerate()
    {
        keys.set(&mut store, index as u32, entry).unwrap();
    }
    language_error(
        &mut store,
        runtime,
        "property-find",
        &[matching_then_opaque.clone(), name.clone(), Val::I32(2)],
    );
    language_error(
        &mut store,
        runtime,
        "property-find",
        &[matching_then_opaque.clone(), name.clone(), Val::I32(1)],
    );
    let fields = payload
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    fields.set(&mut store, 1, matching_then_opaque).unwrap();
    language_error(
        &mut store,
        runtime,
        "named-property-get",
        &[owner.clone(), name.clone()],
    );
    language_error(
        &mut store,
        runtime,
        "named-property-set",
        &[owner, name, nil],
    );
}

#[test]
fn runtime_abi_object_method_tables_and_wrappers_reject_corruption_without_trapping() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let name = nominal_value(&mut store, runtime, "string-new", &[Val::I32(1)]);
    name.unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, Val::I32('m' as i32))
        .unwrap();
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[empty.clone()]);
    let class = nominal_value(
        &mut store,
        runtime,
        "constructor-new",
        &[descriptor.clone()],
    );
    let implementation = nominal_value(&mut store, runtime, "predicate-nil", &[]);
    let method = nominal_value(
        &mut store,
        runtime,
        "object-method-set",
        &[class.clone(), name.clone(), implementation],
    );
    store.gc(None).unwrap();
    let key = nominal_value(
        &mut store,
        runtime,
        "object-method-key",
        &[descriptor.clone(), name.clone()],
    );
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let other_name = nominal_value(&mut store, runtime, "string-new", &[Val::I32(1)]);
    other_name.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap()
        .set(&mut store, 0, Val::I32('n' as i32)).unwrap();
    nominal_value(&mut store, runtime, "object-method-set", &[class.clone(), other_name, method.clone()]);
    let table = descriptor.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap()
        .field(&mut store, 2).unwrap();
    let table_ref = table.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    assert_eq!(table_ref.len(&store).unwrap(), 4);
    table_ref.set(&mut store, 2, nil.clone()).unwrap();
    fn error(store: &mut Store<()>, runtime: Instance, function: &str, args: &[Val]) {
        let error = runtime
            .get_func(&mut *store, function)
            .unwrap()
            .call(&mut *store, args, &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(
            error.is::<wasmtime::ThrownException>(),
            "{function}: {error:#}"
        );
        assert!(!error.is::<wasmtime::Trap>());
        let exception = store.take_pending_exception().unwrap();
        let tag = exception.tag(&mut *store).unwrap();
        assert!(wasmtime::Tag::eq(
            &tag,
            &runtime.get_tag(&mut *store, "language-exception").unwrap(),
            &*store
        ));
    }
    // Copy the private callback's typed function reference into a foreign
    // closure with a different environment. A private detached callback must
    // reject the forgery rather than fall back into invoking itself recursively.
    let mut types = runtime_abi::prelude();
    let value = wasm_encoder::ValType::Ref(wasm_encoder::RefType::EQREF);
    types.ty().function([value, value], [value]);
    let mut functions = wasm_encoder::FunctionSection::new();
    functions.function(10);
    let mut exports = wasm_encoder::ExportSection::new();
    exports.export("copy-callback", wasm_encoder::ExportKind::Func, 0);
    let mut code = wasm_encoder::CodeSection::new();
    let mut forge = wasm_encoder::Function::new([]);
    use wasm_encoder::{Instruction, HeapType};
    forge.instruction(&Instruction::LocalGet(1))
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(4)))
        .instruction(&Instruction::StructGet { struct_type_index: 4, field_index: 1 })
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::I32Const(-1))
        .instruction(&Instruction::StructNew(4))
        .instruction(&Instruction::End);
    code.function(&forge);
    let mut module = wasm_encoder::Module::new();
    module.section(&types).section(&functions).section(&exports).section(&code);
    let foreign = Instance::new(&mut store, &Module::new(&engine, module.finish()).unwrap(), &[]).unwrap();
    let wrong_tag = nominal_value(&mut store, runtime, "object-new", &[descriptor.clone(), empty.clone()]);
    for environment in [nil.clone(), wrong_tag] {
        let forged = nominal_value(&mut store, foreign, "copy-callback", &[method.clone(), environment]);
        error(&mut store, runtime, "invoke", &[forged, empty.clone()]);
    }
    // A matching prefix must not conceal an opaque later key on lookup or update.
    error(
        &mut store,
        runtime,
        "object-method-key",
        &[descriptor.clone(), name.clone()],
    );
    error(
        &mut store,
        runtime,
        "object-method-set",
        &[class, name.clone(), method.clone()],
    );
    // A tagged method key must hold exactly one UTF-16 name.
    table_ref.set(&mut store, 2, key.clone()).unwrap();
    let schema = key
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 1)
        .unwrap();
    schema
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, nil.clone())
        .unwrap();
    error(
        &mut store,
        runtime,
        "object-method-key",
        &[descriptor, name],
    );
    // Invocation validates the tagged wrapper payload before calling it.
    let environment = nominal_value(
        &mut store,
        runtime,
        "closure-environment",
        &[method.clone()],
    );
    let payload = environment
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 1)
        .unwrap();
    payload
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, nil.clone())
        .unwrap();
    error(
        &mut store,
        runtime,
        "object-method-invoke",
        &[method, nil, empty],
    );
}

#[test]
fn runtime_abi_int32_coercion_wraps_finite_values_and_zeroes_nonfinite() {
    let engine = support::engine();
    let module = Module::new(&engine, runtime_abi::module()).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let boxed = runtime.get_func(&mut store, "number-box").unwrap();
    let convert = runtime
        .get_func(&mut store, "coerce-int32")
        .expect("bitwise coercion export");
    let cases = [
        (0.0f64, 0),
        (-0.0, 0),
        (-3.9, -3),
        (3.9, 3),
        (4294967295.0, -1),
        (4294967296.0, 0),
        (4294967297.0, 1),
        (-4294967297.0, -1),
        (2147483648.0, i32::MIN),
        (9007199254740991.0, -1),
        (9007199254740992.0, 0),
        (1e100, 0),
        (-1e100, 0),
        (f64::MAX, 0),
        (f64::INFINITY, 0),
        (f64::NEG_INFINITY, 0),
        (f64::from_bits(0x7ff8_0000_0000_0123), 0),
        (f64::from_bits(0xfff8_0000_0000_0123), 0),
        (f64::from_bits(1), 0),
        (-f64::from_bits(1), 0),
    ];
    for (number, expected) in cases {
        let mut scope = wasmtime::RootScope::new(&mut store);
        let mut value = [Val::null_any_ref()];
        boxed
            .call(&mut scope, &[Val::F64(number.to_bits())], &mut value)
            .unwrap();
        let mut result = [Val::I32(0)];
        convert.call(&mut scope, &value, &mut result).unwrap();
        assert_eq!(
            result[0].unwrap_i32(),
            expected,
            "{:016x}",
            number.to_bits()
        );
    }
    for (name, expected) in [("nil", 0), ("false", 0), ("true", 1)] {
        let mut scope = wasmtime::RootScope::new(&mut store);
        let mut value = [Val::null_any_ref()];
        runtime
            .get_func(&mut scope, name)
            .unwrap()
            .call(&mut scope, &[], &mut value)
            .unwrap();
        let mut result = [Val::I32(0)];
        convert.call(&mut scope, &value, &mut result).unwrap();
        assert_eq!(result[0].unwrap_i32(), expected);
    }
    let new = runtime.get_func(&mut store, "string-new").unwrap();
    let set = runtime.get_func(&mut store, "string-set-unit").unwrap();
    for (units, expected) in [
        ("4294967297".encode_utf16().collect::<Vec<_>>(), 1),
        (" -3.9 ".encode_utf16().collect(), -3),
        ("Infinity".encode_utf16().collect(), 0),
        (vec![0xd800], 0),
        (vec![], 0),
    ] {
        let mut scope = &mut store;
        let mut value = [Val::null_any_ref()];
        new.call(&mut scope, &[Val::I32(units.len() as i32)], &mut value)
            .unwrap();
        for (index, unit) in units.iter().enumerate() {
            let mut accepted = [Val::I32(0)];
            set.call(
                &mut scope,
                &[
                    value[0].clone(),
                    Val::I32(index as i32),
                    Val::I32(*unit as i32),
                ],
                &mut accepted,
            )
            .unwrap();
            assert_eq!(accepted[0].unwrap_i32(), 1);
        }
        scope.gc(None).unwrap();
        let mut result = [Val::I32(0)];
        convert.call(&mut scope, &value, &mut result).unwrap();
        assert_eq!(result[0].unwrap_i32(), expected);
    }
    let mut opaque = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "args-new")
        .unwrap()
        .call(&mut store, &[Val::I32(0)], &mut opaque)
        .unwrap();
    let error = convert
        .call(&mut store, &opaque, &mut [Val::I32(0)])
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>());
    let exception = store.take_pending_exception().unwrap();
    assert!(wasmtime::Tag::eq(
        &exception.tag(&mut store).unwrap(),
        &runtime.get_tag(&mut store, "language-exception").unwrap(),
        &store
    ));
    let mut state = 0xd734_68ef_1279_4321u64;
    for _ in 0..2048 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let number = f64::from_bits(state);
        let expected = if number.is_finite() {
            number.trunc().rem_euclid(4294967296.0) as u32 as i32
        } else {
            0
        };
        let mut scope = wasmtime::RootScope::new(&mut store);
        let mut value = [Val::null_any_ref()];
        boxed
            .call(&mut scope, &[Val::F64(state)], &mut value)
            .unwrap();
        let mut result = [Val::I32(0)];
        convert.call(&mut scope, &value, &mut result).unwrap();
        assert_eq!(result[0].unwrap_i32(), expected, "{state:016x}");
    }
}

#[test]
fn runtime_abi_variadic_bitwise_callback_rejects_bad_cells_without_trapping() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let nil = Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
        &mut store,
        wasmtime::I31::new_u32(0).unwrap(),
    )));
    let mut args = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "args-new")
        .unwrap()
        .call(&mut store, &[Val::I32(3)], &mut args)
        .unwrap();
    let mut boxed = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "number-box")
        .unwrap()
        .call(&mut store, &[Val::F64(1.0f64.to_bits())], &mut boxed)
        .unwrap();
    let array = args[0].unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    for index in 0..3 {
        array.set(&mut store, index, boxed[0].clone()).unwrap();
    }
    for export in [
        "primitive-bit-and-function",
        "primitive-bit-or-function",
        "primitive-bit-xor-function",
        "primitive-bit-and-not-function",
    ] {
        for bad_cell in [nil.clone(), args[0].clone()] {
            let mut closure = [Val::null_any_ref()];
            runtime
                .get_func(&mut store, export)
                .unwrap()
                .call(&mut store, &[bad_cell], &mut closure)
                .unwrap();
            store.gc(None).unwrap();
            let error = runtime
                .get_func(&mut store, "invoke")
                .unwrap()
                .call(
                    &mut store,
                    &[closure[0].clone(), args[0].clone()],
                    &mut [Val::null_any_ref()],
                )
                .unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>(), "{error:?}");
            let exception = store.take_pending_exception().unwrap();
            assert!(wasmtime::Tag::eq(
                &exception.tag(&mut store).unwrap(),
                &runtime.get_tag(&mut store, "language-exception").unwrap(),
                &store
            ));
        }
    }
}

#[test]
fn runtime_abi_utf16_char_code_bounds_and_errors_never_trap() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let text = nominal_value(&mut store, runtime, "string-new", &[Val::I32(2)]);
    for (i, unit) in [0xd800, 0xffff].into_iter().enumerate() {
        runtime.get_func(&mut store, "string-set-unit").unwrap().call(&mut store,
            &[text.clone(), Val::I32(i as i32), Val::I32(unit)], &mut [Val::I32(0)]).unwrap();
    }
    store.gc(None).unwrap();
    let get = runtime.get_func(&mut store, "string-char-code-at").unwrap();
    for (index, expected) in [(0.0, 55296.0), (-0.9, 55296.0), (f64::NAN, 55296.0), (1.9, 65535.0), (-1.0, f64::NAN), (2.0, f64::NAN), (4294967296.0, f64::NAN), (f64::MAX, f64::NAN), (f64::INFINITY, f64::NAN), (f64::NEG_INFINITY, f64::NAN)] {
        let input = nominal_value(&mut store, runtime, "number-box", &[Val::F64(index.to_bits())]);
        let mut output = [Val::null_any_ref()];
        get.call(&mut store, &[text.clone(), input], &mut output).unwrap();
        let object = output[0].unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
        let actual = object.field(&mut store, 0).unwrap().unwrap_f64();
        assert_eq!(actual.to_bits(), expected.to_bits(), "{index:?}");
    }
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let opaque = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    for args in [[nil.clone(), nil.clone()], [text.clone(), opaque], [Val::null_any_ref(), nil.clone()]] {
        let error = get.call(&mut store, &args, &mut [Val::null_any_ref()]).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    let value = nominal_value(&mut store, runtime, "string-char-code-at", &[text, nil]);
    let object = value.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
    assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), 55296.0);
}

#[test]
fn runtime_abi_string_method_copied_callbacks_and_corrupt_environments_do_not_trap() {
    use wasm_encoder::{HeapType, Instruction};
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let method = nominal_value(&mut store, runtime, "string-char-code-at-method", &[]);
    let env = nominal_value(&mut store, runtime, "closure-environment", &[method.clone()]);
    let payload = env.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 1).unwrap();
    let array = payload.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    let anchored = array.get(&mut store, 0).unwrap();
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let text = nominal_value(&mut store, runtime, "string-new", &[Val::I32(1)]);
    text.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap().set(&mut store, 0, Val::I32(0xdfff)).unwrap();
    let pair = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    let args = pair.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    args.set(&mut store, 0, text.clone()).unwrap();
    args.set(&mut store, 1, nil.clone()).unwrap();
    // Copy the actual typed callbacks into independently generated closures.
    let mut types = runtime_abi::prelude();
    let value = wasm_encoder::ValType::Ref(wasm_encoder::RefType::EQREF);
    types.ty().function([value, value], [value]);
    let mut functions = wasm_encoder::FunctionSection::new();
    functions.function(10);
    let mut exports = wasm_encoder::ExportSection::new();
    exports.export("copy-callback", wasm_encoder::ExportKind::Func, 0);
    let mut code = wasm_encoder::CodeSection::new();
    let mut forge = wasm_encoder::Function::new([]);
    forge.instruction(&Instruction::LocalGet(1))
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(4)))
        .instruction(&Instruction::StructGet { struct_type_index: 4, field_index: 1 })
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::I32Const(-1))
        .instruction(&Instruction::StructNew(4))
        .instruction(&Instruction::End);
    code.function(&forge);
    let mut module = wasm_encoder::Module::new();
    module.section(&types).section(&functions).section(&exports).section(&code);
    let foreign = Instance::new(&mut store, &Module::new(&engine, module.finish()).unwrap(), &[]).unwrap();
    fn language_error(store: &mut Store<()>, runtime: Instance, export: &str, args: &[Val]) {
        let error = runtime.get_func(&mut *store, export).unwrap().call(&mut *store, args, &mut [Val::null_any_ref()]).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        let exception = store.take_pending_exception().unwrap();
        assert!(wasmtime::Tag::eq(&exception.tag(&mut *store).unwrap(), &runtime.get_tag(&mut *store, "language-exception").unwrap(), &*store));
    }
    for malformed in [nil.clone(), empty.clone(), Val::null_any_ref()] {
        let copied_detached = nominal_value(&mut store, foreign, "copy-callback", &[method.clone(), malformed.clone()]);
        let copied_anchored = nominal_value(&mut store, foreign, "copy-callback", &[anchored.clone(), malformed]);
        store.gc(None).unwrap();
        language_error(&mut store, runtime, "invoke", &[copied_detached, empty.clone()]);
        language_error(&mut store, runtime, "invoke", &[copied_anchored.clone(), empty.clone()]);
        // The anchored body is deliberately stateless. Its environment is not
        // interpreted, and valid physical receiver/index arguments still work.
        let value = nominal_value(&mut store, runtime, "invoke", &[copied_anchored, pair.clone()]);
        assert_eq!(value.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 0).unwrap().unwrap_f64(), 57343.0);
    }
    // A malformed tagged payload must fail before dereference or callback.
    array.set(&mut store, 0, nil.clone()).unwrap();
    store.gc(None).unwrap();
    language_error(&mut store, runtime, "object-method-invoke", &[method, text.clone(), empty.clone()]);
    let recovered = nominal_value(&mut store, runtime, "string-char-code-at", &[text, nil]);
    assert_eq!(recovered.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 0).unwrap().unwrap_f64(), 57343.0);
}

#[test]
fn runtime_abi_binary64_words_preserve_payloads_byte_order_and_typed_errors() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let mut samples = vec![0u64, 0x8000000000000000, 0x3ff0000000000000,
        0x0000000000000001, 0x7ff0000000000000, 0xfff0000000000000,
        0x7ff8000000000123, 0xfff8000000000123, 0x7ff0000000000001,
        0xfff0000000000001, 0x0123456789abcdef];
    let mut sample_bits = 0x1397abcde0123456u64;
    for _ in 0..256 {
        sample_bits ^= sample_bits << 13; sample_bits ^= sample_bits >> 7; sample_bits ^= sample_bits << 17;
        samples.push(sample_bits);
    }
    for bits in samples {
        let input = nominal_value(&mut store, runtime, "number-box", &[Val::F64(bits)]);
        let normalized = nominal_value(&mut store, runtime, "primitive-f64-coerce", &[input.clone()]);
        let object = normalized.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
        assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64().to_bits(), bits);
        for (name, word) in [("primitive-f64-word0", bits as u32), ("primitive-f64-word4", (bits >> 32) as u32)] {
            let value = nominal_value(&mut store, runtime, name, &[input.clone()]);
            let object = value.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
            assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), f64::from(word.swap_bytes() as i32), "{name}: {bits:016x}");
        }
    }
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let opaque = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    store.gc(None).unwrap();
    for name in ["primitive-f64-word0", "primitive-f64-word4"] {
        for input in [nil.clone(), opaque.clone(), Val::null_any_ref()] {
            let error = runtime.get_func(&mut store, name).unwrap().call(&mut store, &[input], &mut [Val::null_any_ref()]).unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
        }
    }
    let normalized = nominal_value(&mut store, runtime, "primitive-f64-coerce", &[nil]);
    let output = nominal_value(&mut store, runtime, "primitive-f64-word0", &[normalized]);
    let object = output.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
    assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), 0.0);
}

#[test]
fn runtime_abi_binary64_scalar_adapters_reject_foreign_values_and_recover_after_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    ).unwrap();
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let opaque = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    let invalid_sentinel = Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
        &mut store, wasmtime::I31::new_u32(99).unwrap(),
    )));
    for input in [opaque, invalid_sentinel, Val::null_any_ref()] {
        store.gc(None).unwrap();
        for name in ["primitive-f64-coerce", "primitive-f64-word0", "primitive-f64-word4"] {
            let error = runtime.get_func(&mut store, name).unwrap()
                .call(&mut store, &[input.clone()], &mut [Val::null_any_ref()]).unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>(), "{name}: {error:#}");
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
        }
        let normalized = nominal_value(&mut store, runtime, "primitive-f64-coerce", &[nil.clone()]);
        for name in ["primitive-f64-word0", "primitive-f64-word4"] {
            let output = nominal_value(&mut store, runtime, name, &[normalized.clone()]);
            assert_eq!(output.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap()
                .field(&mut store, 0).unwrap().unwrap_f64(), 0.0);
        }
    }
}

#[test]
fn runtime_abi_owned_dynamic_properties_preserve_keys_values_and_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let other = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let spellings = ["x", "object", "array", "constructor", "__proto__", "", "😀"];
    let mut keys = vec![];
    for (i, spelling) in spellings.into_iter().enumerate() {
        let units = spelling.encode_utf16().collect::<Vec<_>>();
        let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(units.len() as i32)]);
        for (j, unit) in units.into_iter().enumerate() {
            runtime.get_func(&mut store, "string-set-unit").unwrap().call(&mut store, &[key.clone(), Val::I32(j as i32), Val::I32(unit as i32)], &mut [Val::I32(0)]).unwrap();
        }
        let value = nominal_value(&mut store, runtime, "number-box", &[Val::F64((i as f64).to_bits())]);
        nominal_value(&mut store, runtime, "native-object-own-set", &[owner.clone(), key.clone(), value]);
        keys.push(key);
    }
    store.gc(None).unwrap();
    for (i, key) in keys.iter().enumerate() {
        let value = nominal_value(&mut store, runtime, "native-object-own-get", &[owner.clone(), key.clone()]);
        let object = value.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
        assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), i as f64);
        let missing = nominal_value(&mut store, runtime, "native-object-own-get", &[other.clone(), key.clone()]);
        assert_eq!(missing.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 6);
    }
    let replacement = nominal_value(&mut store, runtime, "number-box", &[Val::F64(77.0f64.to_bits())]);
    nominal_value(&mut store, runtime, "native-object-own-set", &[owner.clone(), keys[0].clone(), replacement]);
    store.gc(None).unwrap();
    let value = nominal_value(&mut store, runtime, "native-object-own-get", &[owner.clone(), keys[0].clone()]);
    let object = value.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
    assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), 77.0);
    for args in [[Val::null_any_ref(), keys[0].clone()], [owner, Val::null_any_ref()]] {
        let error = runtime.get_func(&mut store, "native-object-own-get").unwrap().call(&mut store, &args, &mut [Val::null_any_ref()]).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
}

#[test]
fn runtime_abi_owned_dynamic_properties_reject_corrupt_storage_and_null_values() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(1)]);
    runtime.get_func(&mut store, "string-set-unit").unwrap().call(&mut store, &[key.clone(), Val::I32(0), Val::I32(0xd800)], &mut [Val::I32(0)]).unwrap();
    let fields = nominal_value(&mut store, runtime, "native-object-fields", &[owner.clone()]);
    let array = fields.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    let original = array.get(&mut store, 0).unwrap();
    for count in [1, 2] {
        let bad = nominal_value(&mut store, runtime, "args-new", &[Val::I32(count)]);
        array.set(&mut store, 0, bad).unwrap();
        store.gc(None).unwrap();
        let error = runtime.get_func(&mut store, "native-object-own-get").unwrap().call(&mut store, &[owner.clone(), key.clone()], &mut [Val::null_any_ref()]).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    let bad = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    let bad_array = bad.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    bad_array.set(&mut store, 0, key.clone()).unwrap();
    bad_array.set(&mut store, 1, Val::null_any_ref()).unwrap();
    array.set(&mut store, 0, bad).unwrap();
    let error = runtime.get_func(&mut store, "native-object-own-get").unwrap().call(&mut store, &[owner.clone(), key.clone()], &mut [Val::null_any_ref()]).unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    array.set(&mut store, 0, original).unwrap();
    let error = runtime.get_func(&mut store, "native-object-own-set").unwrap().call(&mut store,
        &[owner.clone(), key.clone(), Val::null_any_ref()], &mut [Val::null_any_ref()]).unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    let value = nominal_value(&mut store, runtime, "number-box", &[Val::F64(7.0f64.to_bits())]);
    nominal_value(&mut store, runtime, "native-object-own-set", &[owner.clone(), key.clone(), value]);
    store.gc(None).unwrap();
    let value = nominal_value(&mut store, runtime, "native-object-own-get", &[owner, key]);
    let object = value.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
    assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), 7.0);
}

#[test]
fn runtime_abi_native_prototype_chain_preserves_shadowing_and_rejects_cycles_atomically() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let child = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let parent = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let grandparent = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(1)]);
    runtime.get_func(&mut store, "string-set-unit").unwrap().call(&mut store, &[key.clone(), Val::I32(0), Val::I32(120)], &mut [Val::I32(0)]).unwrap();
    let number = nominal_value(&mut store, runtime, "number-box", &[Val::F64(42.0f64.to_bits())]);
    nominal_value(&mut store, runtime, "native-object-own-set", &[grandparent.clone(), key.clone(), number.clone()]);
    nominal_value(&mut store, runtime, "native-object-prototype-set", &[parent.clone(), grandparent.clone()]);
    nominal_value(&mut store, runtime, "native-object-prototype-set", &[child.clone(), parent.clone()]);
    store.gc(None).unwrap();
    let inherited = nominal_value(&mut store, runtime, "native-object-chain-get", &[child.clone(), key.clone()]);
    assert_eq!(inherited.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 0).unwrap().unwrap_f64(), 42.0);
    // Undefined is a present own value and must suppress a farther inherited value.
    let undefined = Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
        &mut store, wasmtime::I31::new_u32(6).unwrap(),
    )));
    nominal_value(&mut store, runtime, "native-object-own-set", &[parent.clone(), key.clone(), undefined]);
    let shadow = nominal_value(&mut store, runtime, "native-object-chain-get", &[child.clone(), key.clone()]);
    assert_eq!(shadow.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 6);
    for (owner, prototype) in [(child.clone(), child.clone()), (grandparent.clone(), child.clone()), (child.clone(), number), (child.clone(), Val::null_any_ref())] {
        let error = runtime.get_func(&mut store, "native-object-prototype-set").unwrap().call(&mut store, &[owner, prototype], &mut [Val::null_any_ref()]).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    // Failed cycle writes leave the former chain intact.
    let old = nominal_value(&mut store, runtime, "native-object-prototype", &[child.clone()]);
    assert!(wasmtime::Rooted::ref_eq(&store, old.unwrap_anyref().unwrap(), parent.unwrap_anyref().unwrap()).unwrap());
    nominal_value(&mut store, runtime, "native-object-prototype-set", &[child.clone(), nil]);
    store.gc(None).unwrap();
    let detached = nominal_value(&mut store, runtime, "native-object-chain-get", &[child, key]);
    assert_eq!(detached.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 6);
}

#[test]
fn runtime_abi_native_prototype_chain_rejects_forged_cycles_and_recovers() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let other = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let fields = nominal_value(&mut store, runtime, "native-object-fields", &[owner.clone()]);
    let fields = fields.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(0)]);
    for corrupt in [owner.clone(), Val::null_any_ref()] {
        fields.set(&mut store, 1, corrupt).unwrap();
        store.gc(None).unwrap();
        for (name, arguments) in [
            ("native-object-chain-get", vec![owner.clone(), key.clone()]),
            ("native-object-prototype-set", vec![other.clone(), owner.clone()]),
        ] {
            let error = runtime.get_func(&mut store, name).unwrap().call(&mut store, &arguments, &mut [Val::null_any_ref()]).unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>(), "{name}: {error:#}");
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
        }
        fields.set(&mut store, 1, nil.clone()).unwrap();
        store.gc(None).unwrap();
        nominal_value(&mut store, runtime, "native-object-prototype-set", &[other.clone(), owner.clone()]);
        nominal_value(&mut store, runtime, "native-object-chain-get", &[other.clone(), key.clone()]);
        nominal_value(&mut store, runtime, "native-object-prototype-set", &[other.clone(), nil.clone()]);
    }
}

#[test]
fn runtime_abi_native_prototype_long_chain_and_forged_tail_cycle_survive_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let tail = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(0)]);
    let value = nominal_value(&mut store, runtime, "number-box", &[Val::F64(19.0f64.to_bits())]);
    nominal_value(&mut store, runtime, "native-object-own-set", &[tail.clone(), key.clone(), value]);
    let mut head = tail.clone();
    for _ in 0..129 {
        let next = nominal_value(&mut store, runtime, "native-object-new", &[]);
        nominal_value(&mut store, runtime, "native-object-prototype-set", &[next.clone(), head]);
        head = next;
    }
    store.gc(None).unwrap();
    let inherited = nominal_value(&mut store, runtime, "native-object-chain-get", &[head.clone(), key.clone()]);
    assert_eq!(inherited.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 0).unwrap().unwrap_f64(), 19.0);
    let error = runtime.get_func(&mut store, "native-object-prototype-set").unwrap().call(&mut store, &[tail.clone(), head.clone()], &mut [Val::null_any_ref()]).unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    let fields = nominal_value(&mut store, runtime, "native-object-fields", &[tail]);
    let fields = fields.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    fields.set(&mut store, 1, head.clone()).unwrap();
    store.gc(None).unwrap();
    let error = runtime.get_func(&mut store, "native-object-chain-get").unwrap().call(&mut store, &[head.clone(), key.clone()], &mut [Val::null_any_ref()]).unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    fields.set(&mut store, 1, nil).unwrap();
    store.gc(None).unwrap();
    let inherited = nominal_value(&mut store, runtime, "native-object-chain-get", &[head, key]);
    assert_eq!(inherited.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 0).unwrap().unwrap_f64(), 19.0);
}

#[test]
fn runtime_abi_default_object_methods_are_callable_shared_and_survive_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let other = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let root = nominal_value(&mut store, runtime, "native-object-prototype", &[owner.clone()]);
    let root2 = nominal_value(&mut store, runtime, "native-object-prototype", &[other.clone()]);
    assert!(wasmtime::Rooted::ref_eq(&store, root.unwrap_anyref().unwrap(), root2.unwrap_anyref().unwrap()).unwrap());
    let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let mut methods = vec![];
    for name in ["toString", "valueOf", "hasOwnProperty", "isPrototypeOf", "constructor"] {
        let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(name.len() as i32)]);
        for (i, byte) in name.bytes().enumerate() {
            runtime.get_func(&mut store, "string-set-unit").unwrap().call(&mut store, &[key.clone(), Val::I32(i as i32), Val::I32(byte as i32)], &mut [Val::I32(0)]).unwrap();
        }
        let method = nominal_value(&mut store, runtime, "native-object-chain-get", &[owner.clone(), key]);
        assert!(method.unwrap_anyref().unwrap().as_struct(&store).unwrap().is_some(), "{name}");
        methods.push(method);
    }
    store.gc(None).unwrap();
    let string = nominal_value(&mut store, runtime, "object-method-invoke", &[methods[0].clone(), owner.clone(), args.clone()]);
    let units = string.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    let actual = (0..units.len(&store).unwrap()).map(|i| units.get(&mut store, i).unwrap().unwrap_i32() as u16).collect::<Vec<_>>();
    assert_eq!(actual, "[object Object]".encode_utf16().collect::<Vec<_>>());
    let returned = nominal_value(&mut store, runtime, "object-method-invoke", &[methods[1].clone(), owner.clone(), args.clone()]);
    assert!(wasmtime::Rooted::ref_eq(&store, returned.unwrap_anyref().unwrap(), owner.unwrap_anyref().unwrap()).unwrap());
    for method in [&methods[2], &methods[3]] {
        let result = nominal_value(&mut store, runtime, "object-method-invoke", &[method.clone(), owner.clone(), args.clone()]);
        assert_eq!(result.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 2);
    }
    let one = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let one_array = one.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    one_array.set(&mut store, 0, owner.clone()).unwrap();
    let result = nominal_value(&mut store, runtime, "object-method-invoke", &[methods[3].clone(), root, one.clone()]);
    assert_eq!(result.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 4);
    let same = nominal_value(&mut store, runtime, "protocol-native-invoke", &[methods[4].clone(), one]);
    assert!(wasmtime::Rooted::ref_eq(&store, same.unwrap_anyref().unwrap(), owner.unwrap_anyref().unwrap()).unwrap());
    let created = nominal_value(&mut store, runtime, "protocol-native-invoke", &[methods[4].clone(), args.clone()]);
    assert!(!wasmtime::Rooted::ref_eq(&store, created.unwrap_anyref().unwrap(), owner.unwrap_anyref().unwrap()).unwrap());
    let string = nominal_value(&mut store, runtime, "protocol-native-invoke", &[methods[0].clone(), args.clone()]);
    let units = string.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    let actual = (0..units.len(&store).unwrap()).map(|i| units.get(&mut store, i).unwrap().unwrap_i32() as u16).collect::<Vec<_>>();
    assert_eq!(actual, "[object Undefined]".encode_utf16().collect::<Vec<_>>());
    for method in &methods[1..3] {
        let error = runtime.get_func(&mut store, "protocol-native-invoke").unwrap().call(&mut store, &[method.clone(), args.clone()], &mut [Val::null_any_ref()]).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    let result = nominal_value(&mut store, runtime, "protocol-native-invoke", &[methods[3].clone(), args.clone()]);
    assert_eq!(result.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 2);
    let scalar = nominal_value(&mut store, runtime, "number-box", &[Val::F64(7.0f64.to_bits())]);
    one_array.set(&mut store, 0, scalar).unwrap();
    let one = Val::AnyRef(Some(one_array.into()));
    let result = nominal_value(&mut store, runtime, "protocol-native-invoke", &[methods[3].clone(), one.clone()]);
    assert_eq!(result.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 2);
    let own_key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(11)]);
    for (i, byte) in "constructor".bytes().enumerate() {
        runtime.get_func(&mut store, "string-set-unit").unwrap().call(&mut store, &[own_key.clone(), Val::I32(i as i32), Val::I32(byte as i32)], &mut [Val::I32(0)]).unwrap();
    }
    one_array.set(&mut store, 0, own_key.clone()).unwrap();
    let present = nominal_value(&mut store, runtime, "object-method-invoke", &[methods[2].clone(), root2, one.clone()]);
    assert_eq!(present.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 4);
    let absent = nominal_value(&mut store, runtime, "object-method-invoke", &[methods[2].clone(), owner.clone(), one.clone()]);
    assert_eq!(absent.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 2);
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    nominal_value(&mut store, runtime, "native-object-own-set", &[owner.clone(), own_key, nil]);
    store.gc(None).unwrap();
    let present = nominal_value(&mut store, runtime, "object-method-invoke", &[methods[2].clone(), owner.clone(), one.clone()]);
    assert_eq!(present.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 4);
    one_array.set(&mut store, 0, owner.clone()).unwrap();
    let error = runtime.get_func(&mut store, "protocol-native-invoke").unwrap().call(&mut store, &[methods[3].clone(), one], &mut [Val::null_any_ref()]).unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    store.gc(None).unwrap();
    nominal_value(&mut store, runtime, "object-method-invoke", &[methods[1].clone(), other, args]);
}
