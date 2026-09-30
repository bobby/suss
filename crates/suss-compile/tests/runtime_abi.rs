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
