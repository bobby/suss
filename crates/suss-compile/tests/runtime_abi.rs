//! Execute production ABI v2 runtime bytes, independently of Suss printing/equality.
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
    assert_eq!(fields.len(), 5);
    assert_eq!(
        fields[4]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0,
        "fresh UID slot"
    );
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
            runtime_abi: 1, // ABI1 must fail before initializer effects.
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
        assert!(load(&initializer(&changed), &mut store)
            .unwrap_err()
            .contains("version mismatch"));
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
    assert!(runtime_abi::verify_artifact(&duplicate.finish(), &expected)
        .unwrap_err()
        .contains("duplicate"));
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
    assert_eq!(fields.len(), 5);
    assert_eq!(
        fields[4]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0,
        "fresh UID slot"
    );
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
        5
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
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        retained_descriptor.unwrap_anyref().unwrap(),
        a.unwrap_anyref().unwrap()
    )
    .unwrap());
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
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        updated.unwrap_anyref().unwrap(),
        replacement_object.unwrap_anyref().unwrap()
    )
    .unwrap());
    let old = nominal_value(
        &mut store,
        runtime,
        "invoke",
        &[old_method, receiver_args.clone()],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        old.unwrap_anyref().unwrap(),
        object.unwrap_anyref().unwrap()
    )
    .unwrap());
    let other = nominal_value(
        &mut store,
        consumer,
        "protocol-method-get",
        &[a.clone(), other_key],
    );
    let other = nominal_value(&mut store, runtime, "invoke", &[other, receiver_args]);
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        other.unwrap_anyref().unwrap(),
        object.unwrap_anyref().unwrap()
    )
    .unwrap());
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
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        result.unwrap_anyref().unwrap(),
        number.unwrap_anyref().unwrap()
    )
    .unwrap());
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
fn runtime_abi_source_closure_names_survive_gc_and_reject_unset_or_invalid_owners() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let function = nominal_value(&mut store, runtime, "predicate-nil", &[]);
    let get = runtime.get_func(&mut store, "closure-source-name").unwrap();
    let initialize = runtime
        .get_func(&mut store, "closure-source-name-initialize")
        .unwrap();
    // A kernel closure has no known source name. Never turn that unknown into
    // the successful empty name of a genuine anonymous source function.
    let error = get
        .call(&mut store, &[function.clone()], &mut [Val::null_any_ref()])
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>());
    assert!(!error.is::<wasmtime::Trap>());
    let expected = [0x61, 0xd800, 0xd83d, 0xde42];
    {
        let mut scope = wasmtime::RootScope::new(&mut store);
        let mut name = [Val::null_any_ref()];
        runtime
            .get_func(&mut scope, "string-new")
            .unwrap()
            .call(&mut scope, &[Val::I32(expected.len() as i32)], &mut name)
            .unwrap();
        let array = name[0]
            .unwrap_anyref()
            .unwrap()
            .as_array(&scope)
            .unwrap()
            .unwrap();
        for (index, unit) in expected.iter().enumerate() {
            array
                .set(&mut scope, index as u32, Val::I32(*unit))
                .unwrap();
        }
        initialize
            .call(&mut scope, &[function.clone(), name[0].clone()], &mut [])
            .unwrap();
    }
    // The owning closure is the only surviving root for this UTF16 label.
    store.gc(None).unwrap();
    let mut result = [Val::null_any_ref()];
    get.call(&mut store, &[function.clone()], &mut result)
        .unwrap();
    let name = result[0]
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    assert_eq!(name.len(&store).unwrap(), expected.len() as u32);
    for (index, unit) in expected.iter().enumerate() {
        assert_eq!(
            name.get(&mut store, index as u32).unwrap().unwrap_i32(),
            *unit
        );
    }
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let number = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(7f64.to_bits())],
    );
    for owner in [nil.clone(), number.clone()] {
        let error = get
            .call(&mut store, &[owner], &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>());
        assert!(!error.is::<wasmtime::Trap>());
    }
    for invalid_name in [nil, number] {
        let error = initialize
            .call(&mut store, &[function.clone(), invalid_name], &mut [])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>());
        assert!(!error.is::<wasmtime::Trap>());
    }
    let unchanged = nominal_value(&mut store, runtime, "closure-source-name", &[function]);
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        result[0].unwrap_anyref().unwrap(),
        unchanged.unwrap_anyref().unwrap()
    )
    .unwrap());
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
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::RefI31)
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
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        result[0].unwrap_anyref().unwrap(),
        environment[0].unwrap_anyref().unwrap()
    )
    .unwrap());
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
    assert!(!wasmtime::Rooted::ref_eq(
        &store,
        value.unwrap_anyref().unwrap(),
        clone.unwrap_anyref().unwrap()
    )
    .unwrap());
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
    // Sparse indexed growth is valid; only dense argument materialization is bounded.
    nominal_value(
        &mut store,
        runtime,
        "source-array-set",
        &[value.clone(), huge.clone(), nil.clone()],
    );
    let length = nominal_value(&mut store, runtime, "source-array-length", &[value.clone()]);
    assert_eq!(
        length
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        1_000_002.0
    );
    for (export, args) in [
        ("source-array-new", vec![nil.clone()]),
        ("source-array-fields", vec![nil.clone()]),
        ("source-array-storage", vec![nil.clone()]),
        ("source-array-length", vec![nil.clone()]),
        ("source-array-clone", vec![nil.clone()]),
        ("source-array-index", vec![nil.clone()]),
        ("source-array-get", vec![nil.clone(), huge.clone()]),
        ("source-array-storage", vec![value.clone()]),
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
    proto_schema
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, proto_name.clone())
        .unwrap();
    let proto_descriptor = nominal_value(
        &mut store,
        runtime,
        "descriptor-new",
        &[proto_schema.clone()],
    );
    let proto_object = nominal_value(
        &mut store,
        runtime,
        "object-new",
        &[proto_descriptor, proto_schema],
    );
    language_error(
        &mut store,
        runtime,
        "named-property-get",
        &[proto_object.clone(), proto_name.clone()],
    );
    language_error(
        &mut store,
        runtime,
        "named-property-set",
        &[proto_object, proto_name.clone(), proto_name],
    );
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
    other_name
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, Val::I32('n' as i32))
        .unwrap();
    nominal_value(
        &mut store,
        runtime,
        "object-method-set",
        &[class.clone(), other_name, method.clone()],
    );
    let table = descriptor
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 2)
        .unwrap();
    let table_ref = table
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
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
    use wasm_encoder::{HeapType, Instruction};
    forge
        .instruction(&Instruction::LocalGet(1))
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(4)))
        .instruction(&Instruction::StructGet {
            struct_type_index: 4,
            field_index: 1,
        })
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::I32Const(-1))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::RefI31)
        .instruction(&Instruction::StructNew(4))
        .instruction(&Instruction::End);
    code.function(&forge);
    let mut module = wasm_encoder::Module::new();
    module
        .section(&types)
        .section(&functions)
        .section(&exports)
        .section(&code);
    let foreign = Instance::new(
        &mut store,
        &Module::new(&engine, module.finish()).unwrap(),
        &[],
    )
    .unwrap();
    let wrong_tag = nominal_value(
        &mut store,
        runtime,
        "object-new",
        &[descriptor.clone(), empty.clone()],
    );
    for environment in [nil.clone(), wrong_tag] {
        let forged = nominal_value(
            &mut store,
            foreign,
            "copy-callback",
            &[method.clone(), environment],
        );
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
    let array = args[0]
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
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
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let text = nominal_value(&mut store, runtime, "string-new", &[Val::I32(2)]);
    for (i, unit) in [0xd800, 0xffff].into_iter().enumerate() {
        runtime
            .get_func(&mut store, "string-set-unit")
            .unwrap()
            .call(
                &mut store,
                &[text.clone(), Val::I32(i as i32), Val::I32(unit)],
                &mut [Val::I32(0)],
            )
            .unwrap();
    }
    store.gc(None).unwrap();
    let get = runtime.get_func(&mut store, "string-char-code-at").unwrap();
    for (index, expected) in [
        (0.0, 55296.0),
        (-0.9, 55296.0),
        (f64::NAN, 55296.0),
        (1.9, 65535.0),
        (-1.0, f64::NAN),
        (2.0, f64::NAN),
        (4294967296.0, f64::NAN),
        (f64::MAX, f64::NAN),
        (f64::INFINITY, f64::NAN),
        (f64::NEG_INFINITY, f64::NAN),
    ] {
        let input = nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64(index.to_bits())],
        );
        let mut output = [Val::null_any_ref()];
        get.call(&mut store, &[text.clone(), input], &mut output)
            .unwrap();
        let object = output[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        let actual = object.field(&mut store, 0).unwrap().unwrap_f64();
        assert_eq!(actual.to_bits(), expected.to_bits(), "{index:?}");
    }
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let opaque = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    for args in [
        [nil.clone(), nil.clone()],
        [text.clone(), opaque],
        [Val::null_any_ref(), nil.clone()],
    ] {
        let error = get
            .call(&mut store, &args, &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    let value = nominal_value(&mut store, runtime, "string-char-code-at", &[text, nil]);
    let object = value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), 55296.0);
}

#[test]
fn runtime_abi_string_method_copied_callbacks_and_corrupt_environments_do_not_trap() {
    use wasm_encoder::{HeapType, Instruction};
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let method = nominal_value(&mut store, runtime, "string-char-code-at-method", &[]);
    let env = nominal_value(
        &mut store,
        runtime,
        "closure-environment",
        &[method.clone()],
    );
    let payload = env
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 1)
        .unwrap();
    let array = payload
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    let anchored = array.get(&mut store, 0).unwrap();
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let text = nominal_value(&mut store, runtime, "string-new", &[Val::I32(1)]);
    text.unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, Val::I32(0xdfff))
        .unwrap();
    let pair = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    let args = pair
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
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
    forge
        .instruction(&Instruction::LocalGet(1))
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(4)))
        .instruction(&Instruction::StructGet {
            struct_type_index: 4,
            field_index: 1,
        })
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::I32Const(-1))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::RefI31)
        .instruction(&Instruction::StructNew(4))
        .instruction(&Instruction::End);
    code.function(&forge);
    let mut module = wasm_encoder::Module::new();
    module
        .section(&types)
        .section(&functions)
        .section(&exports)
        .section(&code);
    let foreign = Instance::new(
        &mut store,
        &Module::new(&engine, module.finish()).unwrap(),
        &[],
    )
    .unwrap();
    fn language_error(store: &mut Store<()>, runtime: Instance, export: &str, args: &[Val]) {
        let error = runtime
            .get_func(&mut *store, export)
            .unwrap()
            .call(&mut *store, args, &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        let exception = store.take_pending_exception().unwrap();
        assert!(wasmtime::Tag::eq(
            &exception.tag(&mut *store).unwrap(),
            &runtime.get_tag(&mut *store, "language-exception").unwrap(),
            &*store
        ));
    }
    for malformed in [nil.clone(), empty.clone(), Val::null_any_ref()] {
        let copied_detached = nominal_value(
            &mut store,
            foreign,
            "copy-callback",
            &[method.clone(), malformed.clone()],
        );
        let copied_anchored = nominal_value(
            &mut store,
            foreign,
            "copy-callback",
            &[anchored.clone(), malformed],
        );
        store.gc(None).unwrap();
        language_error(
            &mut store,
            runtime,
            "invoke",
            &[copied_detached, empty.clone()],
        );
        language_error(
            &mut store,
            runtime,
            "invoke",
            &[copied_anchored.clone(), empty.clone()],
        );
        // The anchored body is deliberately stateless. Its environment is not
        // interpreted, and valid physical receiver/index arguments still work.
        let value = nominal_value(
            &mut store,
            runtime,
            "invoke",
            &[copied_anchored, pair.clone()],
        );
        assert_eq!(
            value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap()
                .field(&mut store, 0)
                .unwrap()
                .unwrap_f64(),
            57343.0
        );
    }
    // A malformed tagged payload must fail before dereference or callback.
    array.set(&mut store, 0, nil.clone()).unwrap();
    store.gc(None).unwrap();
    language_error(
        &mut store,
        runtime,
        "object-method-invoke",
        &[method, text.clone(), empty.clone()],
    );
    let recovered = nominal_value(&mut store, runtime, "string-char-code-at", &[text, nil]);
    assert_eq!(
        recovered
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        57343.0
    );
}

#[test]
fn runtime_abi_binary64_words_preserve_payloads_byte_order_and_typed_errors() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut samples = vec![
        0u64,
        0x8000000000000000,
        0x3ff0000000000000,
        0x0000000000000001,
        0x7ff0000000000000,
        0xfff0000000000000,
        0x7ff8000000000123,
        0xfff8000000000123,
        0x7ff0000000000001,
        0xfff0000000000001,
        0x0123456789abcdef,
    ];
    let mut sample_bits = 0x1397abcde0123456u64;
    for _ in 0..256 {
        sample_bits ^= sample_bits << 13;
        sample_bits ^= sample_bits >> 7;
        sample_bits ^= sample_bits << 17;
        samples.push(sample_bits);
    }
    for bits in samples {
        let input = nominal_value(&mut store, runtime, "number-box", &[Val::F64(bits)]);
        let normalized = nominal_value(
            &mut store,
            runtime,
            "primitive-f64-coerce",
            &[input.clone()],
        );
        let object = normalized
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(
            object.field(&mut store, 0).unwrap().unwrap_f64().to_bits(),
            bits
        );
        for (name, word) in [
            ("primitive-f64-word0", bits as u32),
            ("primitive-f64-word4", (bits >> 32) as u32),
        ] {
            let value = nominal_value(&mut store, runtime, name, &[input.clone()]);
            let object = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap();
            assert_eq!(
                object.field(&mut store, 0).unwrap().unwrap_f64(),
                f64::from(word.swap_bytes() as i32),
                "{name}: {bits:016x}"
            );
        }
    }
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let opaque = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    store.gc(None).unwrap();
    for name in ["primitive-f64-word0", "primitive-f64-word4"] {
        for input in [nil.clone(), opaque.clone(), Val::null_any_ref()] {
            let error = runtime
                .get_func(&mut store, name)
                .unwrap()
                .call(&mut store, &[input], &mut [Val::null_any_ref()])
                .unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
        }
    }
    let normalized = nominal_value(&mut store, runtime, "primitive-f64-coerce", &[nil]);
    let output = nominal_value(&mut store, runtime, "primitive-f64-word0", &[normalized]);
    let object = output
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
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
    )
    .unwrap();
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let opaque = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    let invalid_sentinel = Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
        &mut store,
        wasmtime::I31::new_u32(99).unwrap(),
    )));
    for input in [opaque, invalid_sentinel, Val::null_any_ref()] {
        store.gc(None).unwrap();
        for name in [
            "primitive-f64-coerce",
            "primitive-f64-word0",
            "primitive-f64-word4",
        ] {
            let error = runtime
                .get_func(&mut store, name)
                .unwrap()
                .call(&mut store, &[input.clone()], &mut [Val::null_any_ref()])
                .unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>(), "{name}: {error:#}");
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
        }
        let normalized = nominal_value(&mut store, runtime, "primitive-f64-coerce", &[nil.clone()]);
        for name in ["primitive-f64-word0", "primitive-f64-word4"] {
            let output = nominal_value(&mut store, runtime, name, &[normalized.clone()]);
            assert_eq!(
                output
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)
                    .unwrap()
                    .unwrap()
                    .field(&mut store, 0)
                    .unwrap()
                    .unwrap_f64(),
                0.0
            );
        }
    }
}

#[test]
fn runtime_abi_owned_dynamic_properties_preserve_keys_values_and_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let other = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let spellings = ["x", "object", "array", "constructor", "__proto__", "", "😀"];
    let mut keys = vec![];
    for (i, spelling) in spellings.into_iter().enumerate() {
        let units = spelling.encode_utf16().collect::<Vec<_>>();
        let key = nominal_value(
            &mut store,
            runtime,
            "string-new",
            &[Val::I32(units.len() as i32)],
        );
        for (j, unit) in units.into_iter().enumerate() {
            runtime
                .get_func(&mut store, "string-set-unit")
                .unwrap()
                .call(
                    &mut store,
                    &[key.clone(), Val::I32(j as i32), Val::I32(unit as i32)],
                    &mut [Val::I32(0)],
                )
                .unwrap();
        }
        let value = nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64((i as f64).to_bits())],
        );
        nominal_value(
            &mut store,
            runtime,
            "native-object-own-set",
            &[owner.clone(), key.clone(), value],
        );
        keys.push(key);
    }
    store.gc(None).unwrap();
    for (i, key) in keys.iter().enumerate() {
        let value = nominal_value(
            &mut store,
            runtime,
            "native-object-own-get",
            &[owner.clone(), key.clone()],
        );
        let object = value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), i as f64);
        let missing = nominal_value(
            &mut store,
            runtime,
            "native-object-own-get",
            &[other.clone(), key.clone()],
        );
        assert_eq!(
            missing
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)
                .unwrap()
                .unwrap()
                .get_u32(),
            6
        );
    }
    let replacement = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(77.0f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-set",
        &[owner.clone(), keys[0].clone(), replacement],
    );
    store.gc(None).unwrap();
    let value = nominal_value(
        &mut store,
        runtime,
        "native-object-own-get",
        &[owner.clone(), keys[0].clone()],
    );
    let object = value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), 77.0);
    for args in [
        [Val::null_any_ref(), keys[0].clone()],
        [owner, Val::null_any_ref()],
    ] {
        let error = runtime
            .get_func(&mut store, "native-object-own-get")
            .unwrap()
            .call(&mut store, &args, &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
}

#[test]
fn runtime_abi_owned_dynamic_properties_reject_corrupt_storage_and_null_values() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(1)]);
    runtime
        .get_func(&mut store, "string-set-unit")
        .unwrap()
        .call(
            &mut store,
            &[key.clone(), Val::I32(0), Val::I32(0xd800)],
            &mut [Val::I32(0)],
        )
        .unwrap();
    let fields = nominal_value(
        &mut store,
        runtime,
        "native-object-fields",
        &[owner.clone()],
    );
    let array = fields
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    let original = array.get(&mut store, 0).unwrap();
    for count in [1, 2] {
        let bad = nominal_value(&mut store, runtime, "args-new", &[Val::I32(count)]);
        array.set(&mut store, 0, bad).unwrap();
        store.gc(None).unwrap();
        let error = runtime
            .get_func(&mut store, "native-object-own-get")
            .unwrap()
            .call(
                &mut store,
                &[owner.clone(), key.clone()],
                &mut [Val::null_any_ref()],
            )
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    let bad = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    let bad_array = bad
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    bad_array.set(&mut store, 0, key.clone()).unwrap();
    bad_array.set(&mut store, 1, Val::null_any_ref()).unwrap();
    array.set(&mut store, 0, bad).unwrap();
    let error = runtime
        .get_func(&mut store, "native-object-own-get")
        .unwrap()
        .call(
            &mut store,
            &[owner.clone(), key.clone()],
            &mut [Val::null_any_ref()],
        )
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    array.set(&mut store, 0, original).unwrap();
    let error = runtime
        .get_func(&mut store, "native-object-own-set")
        .unwrap()
        .call(
            &mut store,
            &[owner.clone(), key.clone(), Val::null_any_ref()],
            &mut [Val::null_any_ref()],
        )
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    let value = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(7.0f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-set",
        &[owner.clone(), key.clone(), value],
    );
    store.gc(None).unwrap();
    let value = nominal_value(&mut store, runtime, "native-object-own-get", &[owner, key]);
    let object = value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(object.field(&mut store, 0).unwrap().unwrap_f64(), 7.0);
}

#[test]
fn runtime_abi_native_prototype_chain_preserves_shadowing_and_rejects_cycles_atomically() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let child = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let parent = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let grandparent = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(1)]);
    runtime
        .get_func(&mut store, "string-set-unit")
        .unwrap()
        .call(
            &mut store,
            &[key.clone(), Val::I32(0), Val::I32(120)],
            &mut [Val::I32(0)],
        )
        .unwrap();
    let number = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(42.0f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-set",
        &[grandparent.clone(), key.clone(), number.clone()],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-prototype-set",
        &[parent.clone(), grandparent.clone()],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-prototype-set",
        &[child.clone(), parent.clone()],
    );
    store.gc(None).unwrap();
    let inherited = nominal_value(
        &mut store,
        runtime,
        "native-object-chain-get",
        &[child.clone(), key.clone()],
    );
    assert_eq!(
        inherited
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        42.0
    );
    // Undefined is a present own value and must suppress a farther inherited value.
    let undefined = Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
        &mut store,
        wasmtime::I31::new_u32(6).unwrap(),
    )));
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-set",
        &[parent.clone(), key.clone(), undefined],
    );
    let shadow = nominal_value(
        &mut store,
        runtime,
        "native-object-chain-get",
        &[child.clone(), key.clone()],
    );
    assert_eq!(
        shadow
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        6
    );
    for (owner, prototype) in [
        (child.clone(), child.clone()),
        (grandparent.clone(), child.clone()),
        (child.clone(), number),
        (child.clone(), Val::null_any_ref()),
    ] {
        let error = runtime
            .get_func(&mut store, "native-object-prototype-set")
            .unwrap()
            .call(&mut store, &[owner, prototype], &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    // Failed cycle writes leave the former chain intact.
    let old = nominal_value(
        &mut store,
        runtime,
        "native-object-prototype",
        &[child.clone()],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        old.unwrap_anyref().unwrap(),
        parent.unwrap_anyref().unwrap()
    )
    .unwrap());
    nominal_value(
        &mut store,
        runtime,
        "native-object-prototype-set",
        &[child.clone(), nil],
    );
    store.gc(None).unwrap();
    let detached = nominal_value(
        &mut store,
        runtime,
        "native-object-chain-get",
        &[child, key],
    );
    assert_eq!(
        detached
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        6
    );
}

#[test]
fn runtime_abi_native_prototype_chain_rejects_forged_cycles_and_recovers() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let other = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let fields = nominal_value(
        &mut store,
        runtime,
        "native-object-fields",
        &[owner.clone()],
    );
    let fields = fields
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(0)]);
    for corrupt in [owner.clone(), Val::null_any_ref()] {
        fields.set(&mut store, 1, corrupt).unwrap();
        store.gc(None).unwrap();
        for (name, arguments) in [
            ("native-object-chain-get", vec![owner.clone(), key.clone()]),
            (
                "native-object-prototype-set",
                vec![other.clone(), owner.clone()],
            ),
        ] {
            let error = runtime
                .get_func(&mut store, name)
                .unwrap()
                .call(&mut store, &arguments, &mut [Val::null_any_ref()])
                .unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>(), "{name}: {error:#}");
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
        }
        fields.set(&mut store, 1, nil.clone()).unwrap();
        store.gc(None).unwrap();
        nominal_value(
            &mut store,
            runtime,
            "native-object-prototype-set",
            &[other.clone(), owner.clone()],
        );
        nominal_value(
            &mut store,
            runtime,
            "native-object-chain-get",
            &[other.clone(), key.clone()],
        );
        nominal_value(
            &mut store,
            runtime,
            "native-object-prototype-set",
            &[other.clone(), nil.clone()],
        );
    }
}

#[test]
fn runtime_abi_native_prototype_long_chain_and_forged_tail_cycle_survive_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let tail = nominal_value(&mut store, runtime, "native-object-new", &[]);
    let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(0)]);
    let value = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(19.0f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-set",
        &[tail.clone(), key.clone(), value],
    );
    let mut head = tail.clone();
    for _ in 0..129 {
        let next = nominal_value(&mut store, runtime, "native-object-new", &[]);
        nominal_value(
            &mut store,
            runtime,
            "native-object-prototype-set",
            &[next.clone(), head],
        );
        head = next;
    }
    store.gc(None).unwrap();
    let inherited = nominal_value(
        &mut store,
        runtime,
        "native-object-chain-get",
        &[head.clone(), key.clone()],
    );
    assert_eq!(
        inherited
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        19.0
    );
    let error = runtime
        .get_func(&mut store, "native-object-prototype-set")
        .unwrap()
        .call(
            &mut store,
            &[tail.clone(), head.clone()],
            &mut [Val::null_any_ref()],
        )
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    let fields = nominal_value(&mut store, runtime, "native-object-fields", &[tail]);
    let fields = fields
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    fields.set(&mut store, 1, head.clone()).unwrap();
    store.gc(None).unwrap();
    let error = runtime
        .get_func(&mut store, "native-object-chain-get")
        .unwrap()
        .call(
            &mut store,
            &[head.clone(), key.clone()],
            &mut [Val::null_any_ref()],
        )
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    fields.set(&mut store, 1, nil).unwrap();
    store.gc(None).unwrap();
    let inherited = nominal_value(&mut store, runtime, "native-object-chain-get", &[head, key]);
    assert_eq!(
        inherited
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        19.0
    );
}

#[test]
fn runtime_abi_default_object_methods_are_callable_shared_and_survive_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let other = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let root = nominal_value(
        &mut store,
        runtime,
        "native-object-prototype",
        &[owner.clone()],
    );
    let root2 = nominal_value(
        &mut store,
        runtime,
        "native-object-prototype",
        &[other.clone()],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        root.unwrap_anyref().unwrap(),
        root2.unwrap_anyref().unwrap()
    )
    .unwrap());
    let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let mut methods = vec![];
    for name in [
        "toString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "constructor",
    ] {
        let key = nominal_value(
            &mut store,
            runtime,
            "string-new",
            &[Val::I32(name.len() as i32)],
        );
        for (i, byte) in name.bytes().enumerate() {
            runtime
                .get_func(&mut store, "string-set-unit")
                .unwrap()
                .call(
                    &mut store,
                    &[key.clone(), Val::I32(i as i32), Val::I32(byte as i32)],
                    &mut [Val::I32(0)],
                )
                .unwrap();
        }
        let method = nominal_value(
            &mut store,
            runtime,
            "native-object-chain-get",
            &[owner.clone(), key],
        );
        assert!(
            method
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .is_some(),
            "{name}"
        );
        methods.push(method);
    }
    store.gc(None).unwrap();
    let string = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[methods[0].clone(), owner.clone(), args.clone()],
    );
    let units = string
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    let actual = (0..units.len(&store).unwrap())
        .map(|i| units.get(&mut store, i).unwrap().unwrap_i32() as u16)
        .collect::<Vec<_>>();
    assert_eq!(actual, "[object Object]".encode_utf16().collect::<Vec<_>>());
    let returned = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[methods[1].clone(), owner.clone(), args.clone()],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        returned.unwrap_anyref().unwrap(),
        owner.unwrap_anyref().unwrap()
    )
    .unwrap());
    for method in [&methods[2], &methods[3]] {
        let result = nominal_value(
            &mut store,
            runtime,
            "object-method-invoke",
            &[method.clone(), owner.clone(), args.clone()],
        );
        assert_eq!(
            result
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)
                .unwrap()
                .unwrap()
                .get_u32(),
            2
        );
    }
    let one = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let one_array = one
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    one_array.set(&mut store, 0, owner.clone()).unwrap();
    let result = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[methods[3].clone(), root, one.clone()],
    );
    assert_eq!(
        result
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        4
    );
    let same = nominal_value(
        &mut store,
        runtime,
        "protocol-native-invoke",
        &[methods[4].clone(), one],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        same.unwrap_anyref().unwrap(),
        owner.unwrap_anyref().unwrap()
    )
    .unwrap());
    let created = nominal_value(
        &mut store,
        runtime,
        "protocol-native-invoke",
        &[methods[4].clone(), args.clone()],
    );
    assert!(!wasmtime::Rooted::ref_eq(
        &store,
        created.unwrap_anyref().unwrap(),
        owner.unwrap_anyref().unwrap()
    )
    .unwrap());
    let string = nominal_value(
        &mut store,
        runtime,
        "protocol-native-invoke",
        &[methods[0].clone(), args.clone()],
    );
    let units = string
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    let actual = (0..units.len(&store).unwrap())
        .map(|i| units.get(&mut store, i).unwrap().unwrap_i32() as u16)
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        "[object Undefined]".encode_utf16().collect::<Vec<_>>()
    );
    for method in &methods[1..3] {
        let error = runtime
            .get_func(&mut store, "protocol-native-invoke")
            .unwrap()
            .call(
                &mut store,
                &[method.clone(), args.clone()],
                &mut [Val::null_any_ref()],
            )
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    let result = nominal_value(
        &mut store,
        runtime,
        "protocol-native-invoke",
        &[methods[3].clone(), args.clone()],
    );
    assert_eq!(
        result
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        2
    );
    let scalar = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(7.0f64.to_bits())],
    );
    one_array.set(&mut store, 0, scalar).unwrap();
    let one = Val::AnyRef(Some(one_array.into()));
    let result = nominal_value(
        &mut store,
        runtime,
        "protocol-native-invoke",
        &[methods[3].clone(), one.clone()],
    );
    assert_eq!(
        result
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        2
    );
    let own_key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(11)]);
    for (i, byte) in "constructor".bytes().enumerate() {
        runtime
            .get_func(&mut store, "string-set-unit")
            .unwrap()
            .call(
                &mut store,
                &[own_key.clone(), Val::I32(i as i32), Val::I32(byte as i32)],
                &mut [Val::I32(0)],
            )
            .unwrap();
    }
    one_array.set(&mut store, 0, own_key.clone()).unwrap();
    let present = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[methods[2].clone(), root2, one.clone()],
    );
    assert_eq!(
        present
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        4
    );
    let absent = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[methods[2].clone(), owner.clone(), one.clone()],
    );
    assert_eq!(
        absent
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        2
    );
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-set",
        &[owner.clone(), own_key, nil],
    );
    store.gc(None).unwrap();
    let present = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[methods[2].clone(), owner.clone(), one.clone()],
    );
    assert_eq!(
        present
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        4
    );
    one_array.set(&mut store, 0, owner.clone()).unwrap();
    let error = runtime
        .get_func(&mut store, "protocol-native-invoke")
        .unwrap()
        .call(
            &mut store,
            &[methods[3].clone(), one],
            &mut [Val::null_any_ref()],
        )
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    store.gc(None).unwrap();
    nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[methods[1].clone(), other, args],
    );
}

fn native_property_key(store: &mut Store<()>, runtime: Instance, units: &[u16]) -> Val {
    let key = nominal_value(
        store,
        runtime,
        "string-new",
        &[Val::I32(units.len() as i32)],
    );
    for (i, unit) in units.iter().enumerate() {
        runtime
            .get_func(&mut *store, "string-set-unit")
            .unwrap()
            .call(
                &mut *store,
                &[key.clone(), Val::I32(i as i32), Val::I32(i32::from(*unit))],
                &mut [Val::I32(0)],
            )
            .unwrap();
    }
    key
}

#[test]
fn runtime_abi_native_property_accessor_preserves_proto_writes_shadows_and_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let parent = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let root = nominal_value(&mut store, runtime, "native-object-default-prototype", &[]);
    let key = native_property_key(
        &mut store,
        runtime,
        &"__proto__".encode_utf16().collect::<Vec<_>>(),
    );
    let answer = native_property_key(
        &mut store,
        runtime,
        &"answer".encode_utf16().collect::<Vec<_>>(),
    );
    let seven = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(7.0f64.to_bits())],
    );
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let old = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), key.clone()],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        old.unwrap_anyref().unwrap(),
        root.unwrap_anyref().unwrap()
    )
    .unwrap());
    for value in [
        seven.clone(),
        Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
            &mut store,
            wasmtime::I31::new_u32(4).unwrap(),
        ))),
    ] {
        nominal_value(
            &mut store,
            runtime,
            "native-object-property-set",
            &[owner.clone(), key.clone(), value],
        );
        let unchanged = nominal_value(
            &mut store,
            runtime,
            "native-object-property-get",
            &[owner.clone(), key.clone()],
        );
        assert!(wasmtime::Rooted::ref_eq(
            &store,
            unchanged.unwrap_anyref().unwrap(),
            root.unwrap_anyref().unwrap()
        )
        .unwrap());
    }
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[parent.clone(), answer.clone(), seven.clone()],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[owner.clone(), key.clone(), parent.clone()],
    );
    store.gc(None).unwrap();
    let value = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), answer],
    );
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        7.0
    );
    for (receiver, value) in [
        (parent.clone(), owner.clone()),
        (root.clone(), parent.clone()),
    ] {
        let error = runtime
            .get_func(&mut store, "native-object-property-set")
            .unwrap()
            .call(
                &mut store,
                &[receiver, key.clone(), value],
                &mut [Val::null_any_ref()],
            )
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[root, key.clone(), nil.clone()],
    );
    // Removing the chain removes the accessor; the next write becomes own data.
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[owner.clone(), key.clone(), nil],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[owner.clone(), key.clone(), seven.clone()],
    );
    store.gc(None).unwrap();
    let value = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), key.clone()],
    );
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        7.0
    );
    // A nearer inherited data __proto__ suppresses the farther default accessor.
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-set",
        &[parent.clone(), key.clone(), seven],
    );
    let child = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[child.clone(), key.clone(), parent.clone()],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[child.clone(), key.clone(), owner.clone()],
    );
    let actual_parent = nominal_value(
        &mut store,
        runtime,
        "native-object-prototype",
        &[child.clone()],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        actual_parent.unwrap_anyref().unwrap(),
        parent.unwrap_anyref().unwrap()
    )
    .unwrap());
    let own = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[child, key],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        own.unwrap_anyref().unwrap(),
        owner.unwrap_anyref().unwrap()
    )
    .unwrap());
}

#[test]
fn runtime_abi_native_property_scalar_keys_and_malformed_values_recover() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let mut cases = vec![];
    for (sentinel, spelling) in [(0, "null"), (2, "false"), (4, "true"), (6, "undefined")] {
        cases.push((
            Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
                &mut store,
                wasmtime::I31::new_u32(sentinel).unwrap(),
            ))),
            spelling.encode_utf16().collect::<Vec<_>>(),
        ));
    }
    for (number, spelling) in [(-0.0f64, "0"), (f64::NAN, "NaN"), (1.5, "1.5")] {
        cases.push((
            nominal_value(
                &mut store,
                runtime,
                "number-box",
                &[Val::F64(number.to_bits())],
            ),
            spelling.encode_utf16().collect(),
        ));
    }
    for units in [vec![0xd800], vec![0xd83d, 0xde00], vec![]] {
        cases.push((native_property_key(&mut store, runtime, &units), units));
    }
    for (index, (key, units)) in cases.into_iter().enumerate() {
        let number = nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64((index as f64).to_bits())],
        );
        nominal_value(
            &mut store,
            runtime,
            "native-object-property-set",
            &[owner.clone(), key, number],
        );
        store.gc(None).unwrap();
        let expected_key = native_property_key(&mut store, runtime, &units);
        let value = nominal_value(
            &mut store,
            runtime,
            "native-object-property-get",
            &[owner.clone(), expected_key],
        );
        assert_eq!(
            value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap()
                .field(&mut store, 0)
                .unwrap()
                .unwrap_f64(),
            index as f64
        );
    }
    let proto = native_property_key(
        &mut store,
        runtime,
        &"__proto__".encode_utf16().collect::<Vec<_>>(),
    );
    let foreign = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    for value in [foreign.clone(), Val::null_any_ref()] {
        for (name, arguments) in [
            (
                "native-object-property-get",
                vec![owner.clone(), value.clone()],
            ),
            (
                "native-object-proto-accessor",
                vec![owner.clone(), value.clone()],
            ),
            (
                "native-object-property-set",
                vec![owner.clone(), proto.clone(), value],
            ),
        ] {
            let error = runtime
                .get_func(&mut store, name)
                .unwrap()
                .call(
                    &mut store,
                    &arguments,
                    &mut [if name.ends_with("accessor") {
                        Val::I32(0)
                    } else {
                        Val::null_any_ref()
                    }],
                )
                .unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>(), "{name}: {error:#}");
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
        }
    }
    store.gc(None).unwrap();
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner, proto],
    );
}

#[test]
fn runtime_abi_default_proto_accessor_is_an_own_property() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let root = nominal_value(&mut store, runtime, "native-object-default-prototype", &[]);
    let name = native_property_key(
        &mut store,
        runtime,
        &"hasOwnProperty".encode_utf16().collect::<Vec<_>>(),
    );
    let method = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[root.clone(), name],
    );
    let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let key = native_property_key(
        &mut store,
        runtime,
        &"__proto__".encode_utf16().collect::<Vec<_>>(),
    );
    args.unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, key)
        .unwrap();
    store.gc(None).unwrap();
    let result = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[method, root, args],
    );
    assert_eq!(
        result
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        4,
        "the default accessor must be a real own property, not only a lookup special case"
    );
}

#[test]
fn runtime_abi_owned_descriptors_preserve_attributes_and_invoke_accessors() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let parent = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let owner = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    nominal_value(
        &mut store,
        runtime,
        "native-object-prototype-set",
        &[owner.clone(), parent.clone()],
    );
    let root = nominal_value(&mut store, runtime, "native-object-default-prototype", &[]);
    let key = native_property_key(&mut store, runtime, &[120]);
    let seven = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(7.0f64.to_bits())],
    );
    let nine = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(9.0f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-define",
        &[parent.clone(), key.clone(), seven.clone(), Val::I32(0)],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[owner.clone(), key.clone(), nine.clone()],
    );
    let value = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), key.clone()],
    );
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        7.0
    );
    let missing = nominal_value(
        &mut store,
        runtime,
        "native-object-own-descriptor",
        &[owner.clone(), key.clone()],
    );
    assert_eq!(
        missing
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        6
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-define",
        &[parent, key.clone(), seven, Val::I32(1)],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[owner.clone(), key.clone(), nine.clone()],
    );
    let descriptor = nominal_value(
        &mut store,
        runtime,
        "native-object-own-descriptor",
        &[owner.clone(), key.clone()],
    );
    let flags = descriptor
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .get(&mut store, 0)
        .unwrap();
    assert_eq!(
        flags
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        7
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-define",
        &[owner.clone(), key.clone(), nine.clone(), Val::I32(0)],
    );
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[owner.clone(), key.clone(), nil.clone()],
    );
    let value = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), key.clone()],
    );
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        9.0
    );
    let proto = native_property_key(
        &mut store,
        runtime,
        &"__proto__".encode_utf16().collect::<Vec<_>>(),
    );
    let pair = nominal_value(
        &mut store,
        runtime,
        "native-object-own-get",
        &[root, proto.clone()],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-define",
        &[owner.clone(), key.clone(), pair.clone(), Val::I32(12)],
    );
    let before = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), key.clone()],
    );
    let actual_proto = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), proto],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        before.unwrap_anyref().unwrap(),
        actual_proto.unwrap_anyref().unwrap()
    )
    .unwrap());
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[owner.clone(), key.clone(), nil],
    );
    store.gc(None).unwrap();
    let after = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), key.clone()],
    );
    assert_eq!(
        after
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0
    );
    for flags in [-1, 16, i32::MAX, i32::MIN, 9] {
        let error = runtime
            .get_func(&mut store, "native-object-own-define")
            .unwrap()
            .call(
                &mut store,
                &[owner.clone(), key.clone(), pair.clone(), Val::I32(flags)],
                &mut [Val::null_any_ref()],
            )
            .unwrap_err();
        assert!(
            error.is::<wasmtime::ThrownException>(),
            "{flags}: {error:#}"
        );
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    let opaque = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let error = runtime
        .get_func(&mut store, "native-object-own-define")
        .unwrap()
        .call(
            &mut store,
            &[owner.clone(), key.clone(), opaque, Val::I32(12)],
            &mut [Val::null_any_ref()],
        )
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    store.gc(None).unwrap();
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner, key],
    );
}

#[test]
fn runtime_abi_descriptor_methods_observe_enumerability_and_live_tostring() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let root = nominal_value(&mut store, runtime, "native-object-default-prototype", &[]);
    let enum_key = native_property_key(
        &mut store,
        runtime,
        &"propertyIsEnumerable".encode_utf16().collect::<Vec<_>>(),
    );
    let enumerable = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), enum_key],
    );
    let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let array = args
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    let key = native_property_key(
        &mut store,
        runtime,
        &"__proto__".encode_utf16().collect::<Vec<_>>(),
    );
    array.set(&mut store, 0, key).unwrap();
    let result = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[enumerable.clone(), root, args.clone()],
    );
    assert_eq!(
        result
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        2
    );
    let key = native_property_key(&mut store, runtime, &[120]);
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[owner.clone(), key.clone(), nil.clone()],
    );
    array.set(&mut store, 0, key.clone()).unwrap();
    let result = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[enumerable.clone(), owner.clone(), args.clone()],
    );
    assert_eq!(
        result
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        4
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-define",
        &[owner.clone(), key, nil, Val::I32(5)],
    );
    let result = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[enumerable, owner.clone(), args],
    );
    assert_eq!(
        result
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        2
    );
    let key = native_property_key(
        &mut store,
        runtime,
        &"toLocaleString".encode_utf16().collect::<Vec<_>>(),
    );
    let locale = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), key],
    );
    let key = native_property_key(
        &mut store,
        runtime,
        &"valueOf".encode_utf16().collect::<Vec<_>>(),
    );
    let value_of = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), key],
    );
    let key = native_property_key(
        &mut store,
        runtime,
        &"toString".encode_utf16().collect::<Vec<_>>(),
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[owner.clone(), key, value_of],
    );
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    store.gc(None).unwrap();
    let result = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[locale, owner.clone(), empty],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        result.unwrap_anyref().unwrap(),
        owner.unwrap_anyref().unwrap()
    )
    .unwrap());
}

fn native_object_method(store: &mut Store<()>, runtime: Instance, owner: &Val, name: &str) -> Val {
    let key = native_property_key(store, runtime, &name.encode_utf16().collect::<Vec<_>>());
    nominal_value(
        store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), key],
    )
}

#[test]
fn runtime_abi_legacy_accessors_are_real_methods_and_preserve_counterparts() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let parent = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let child = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    nominal_value(
        &mut store,
        runtime,
        "native-object-prototype-set",
        &[child.clone(), parent.clone()],
    );
    let define_get = native_object_method(&mut store, runtime, &parent, "__defineGetter__");
    let define_set = native_object_method(&mut store, runtime, &parent, "__defineSetter__");
    let lookup_get = native_object_method(&mut store, runtime, &parent, "__lookupGetter__");
    let lookup_set = native_object_method(&mut store, runtime, &parent, "__lookupSetter__");
    let value_of = native_object_method(&mut store, runtime, &parent, "valueOf");
    let root = nominal_value(&mut store, runtime, "native-object-default-prototype", &[]);
    let proto = native_property_key(
        &mut store,
        runtime,
        &"__proto__".encode_utf16().collect::<Vec<_>>(),
    );
    let pair = nominal_value(&mut store, runtime, "native-object-own-get", &[root, proto]);
    let setter = pair
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .get(&mut store, 1)
        .unwrap();
    let key = native_property_key(&mut store, runtime, &[120]);
    let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    let array = args
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    array.set(&mut store, 0, key.clone()).unwrap();
    array.set(&mut store, 1, value_of.clone()).unwrap();
    let result = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[define_get.clone(), parent.clone(), args.clone()],
    );
    assert_eq!(
        result
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        6
    );
    array.set(&mut store, 1, setter.clone()).unwrap();
    nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[define_set.clone(), parent.clone(), args.clone()],
    );
    store.gc(None).unwrap();
    let found = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[lookup_get.clone(), child.clone(), args.clone()],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        found.unwrap_anyref().unwrap(),
        value_of.unwrap_anyref().unwrap()
    )
    .unwrap());
    let found = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[lookup_set.clone(), child.clone(), args.clone()],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        found.unwrap_anyref().unwrap(),
        setter.unwrap_anyref().unwrap()
    )
    .unwrap());
    let value = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[child.clone(), key.clone()],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        value.unwrap_anyref().unwrap(),
        child.unwrap_anyref().unwrap()
    )
    .unwrap());
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set",
        &[child.clone(), key.clone(), nil.clone()],
    );
    let detached = nominal_value(
        &mut store,
        runtime,
        "native-object-prototype",
        &[child.clone()],
    );
    assert_eq!(
        detached
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0
    );
    // Defining only a setter creates no getter; the later getter keeps that setter.
    nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[define_set.clone(), child.clone(), args.clone()],
    );
    let no_getter = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[child.clone(), key.clone()],
    );
    assert_eq!(
        no_getter
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        6
    );
    array.set(&mut store, 1, value_of.clone()).unwrap();
    nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[define_get.clone(), child.clone(), args.clone()],
    );
    let found = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[lookup_set, child.clone(), args.clone()],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        found.unwrap_anyref().unwrap(),
        setter.unwrap_anyref().unwrap()
    )
    .unwrap());
    let descriptor = nominal_value(
        &mut store,
        runtime,
        "native-object-own-descriptor",
        &[child.clone(), key.clone()],
    );
    let flags = descriptor
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .get(&mut store, 0)
        .unwrap();
    assert_eq!(
        flags
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        14
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-set",
        &[child.clone(), key.clone(), nil],
    );
    let shadow = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[lookup_get.clone(), child.clone(), args.clone()],
    );
    assert_eq!(
        shadow
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        6
    );
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    for method in [define_get, define_set, lookup_get] {
        let error = runtime
            .get_func(&mut store, "protocol-native-invoke")
            .unwrap()
            .call(
                &mut store,
                &[method, empty.clone()],
                &mut [Val::null_any_ref()],
            )
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
}

#[test]
fn runtime_abi_legacy_accessor_rejections_are_atomic_and_recover_after_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let owner = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let key = native_property_key(&mut store, runtime, &[120]);
    let callback = native_object_method(&mut store, runtime, &owner, "valueOf");
    let number = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(7.0f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-define",
        &[owner.clone(), key.clone(), number.clone(), Val::I32(0)],
    );
    for arguments in [
        vec![owner.clone(), key.clone(), callback.clone(), Val::I32(0)],
        vec![owner.clone(), key.clone(), number, Val::I32(0)],
        vec![owner.clone(), key.clone(), callback.clone(), Val::I32(-1)],
        vec![owner.clone(), key.clone(), callback.clone(), Val::I32(2)],
        vec![
            Val::null_any_ref(),
            key.clone(),
            callback.clone(),
            Val::I32(0),
        ],
    ] {
        let error = runtime
            .get_func(&mut store, "native-object-legacy-define")
            .unwrap()
            .call(&mut store, &arguments, &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
        store.gc(None).unwrap();
        let still = nominal_value(
            &mut store,
            runtime,
            "native-object-property-get",
            &[owner.clone(), key.clone()],
        );
        assert_eq!(
            still
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap()
                .field(&mut store, 0)
                .unwrap()
                .unwrap_f64(),
            7.0
        );
    }
    for selector in [-1, 2] {
        let error = runtime
            .get_func(&mut store, "native-object-legacy-lookup")
            .unwrap()
            .call(
                &mut store,
                &[owner.clone(), key.clone(), Val::I32(selector)],
                &mut [Val::null_any_ref()],
            )
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    // A configurable data property can become an accessor with normal flags14.
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-define",
        &[owner.clone(), key.clone(), nil, Val::I32(4)],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-legacy-define",
        &[owner.clone(), key.clone(), callback, Val::I32(0)],
    );
    store.gc(None).unwrap();
    let value = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner.clone(), key],
    );
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        value.unwrap_anyref().unwrap(),
        owner.unwrap_anyref().unwrap()
    )
    .unwrap());
}

#[test]
fn runtime_abi_native_factory_guards_foreign_buffers_and_recovers_after_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    for name in [
        "native-object-factory-next",
        "native-object-factory-flatten",
    ] {
        for input in [nil.clone(), Val::null_any_ref()] {
            let error = runtime
                .get_func(&mut store, name)
                .unwrap()
                .call(&mut store, &[input], &mut [Val::null_any_ref()])
                .unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>(), "{name}: {error:#}");
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
        }
    }
    let function = nominal_value(&mut store, runtime, "native-object-factory-function", &[]);
    let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    let array = args
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    array.set(&mut store, 0, Val::null_any_ref()).unwrap();
    let error = runtime
        .get_func(&mut store, "protocol-native-invoke")
        .unwrap()
        .call(
            &mut store,
            &[function.clone(), args.clone()],
            &mut [Val::null_any_ref()],
        )
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    let key = native_property_key(&mut store, runtime, &[120]);
    array.set(&mut store, 0, key.clone()).unwrap();
    store.gc(None).unwrap();
    let object = nominal_value(
        &mut store,
        runtime,
        "protocol-native-invoke",
        &[function, args],
    );
    let value = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[object, key],
    );
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0
    );
}

#[test]
fn runtime_abi_strict_owned_store_rejects_readonly_data_before_mutation() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let parent = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    let owner = nominal_value(&mut store, runtime, "native-object-default-new", &[]);
    nominal_value(
        &mut store,
        runtime,
        "native-object-prototype-set",
        &[owner.clone(), parent.clone()],
    );
    let key = native_property_key(&mut store, runtime, &[120]);
    let seven = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(7.0f64.to_bits())],
    );
    let nine = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(9.0f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-define",
        &[parent.clone(), key.clone(), seven.clone(), Val::I32(0)],
    );
    for target in [parent.clone(), owner.clone()] {
        let error = runtime
            .get_func(&mut store, "native-object-property-set-strict")
            .unwrap()
            .call(
                &mut store,
                &[target.clone(), key.clone(), nine.clone()],
                &mut [Val::null_any_ref()],
            )
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
        store.gc(None).unwrap();
        let value = nominal_value(
            &mut store,
            runtime,
            "native-object-property-get",
            &[target, key.clone()],
        );
        assert_eq!(
            value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap()
                .field(&mut store, 0)
                .unwrap()
                .unwrap_f64(),
            7.0
        );
    }
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-define",
        &[parent, key.clone(), seven, Val::I32(1)],
    );
    nominal_value(
        &mut store,
        runtime,
        "native-object-property-set-strict",
        &[owner.clone(), key.clone(), nine],
    );
    let value = nominal_value(
        &mut store,
        runtime,
        "native-object-property-get",
        &[owner, key],
    );
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        9.0
    );
}

#[test]
fn runtime_abi_numeric_hash_boundaries_reject_bad_values_without_traps() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let three = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(3.0f64.to_bits())],
    );
    let opaque = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let forged = Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
        &mut store,
        wasmtime::I31::new_u32(99).unwrap(),
    )));
    for input in [opaque, forged, Val::null_any_ref()] {
        for export in [
            "primitive-f64-floor",
            "primitive-f64-finite",
            "primitive-safe-integer-remainder",
        ] {
            let args = if export == "primitive-safe-integer-remainder" {
                vec![input.clone(), three.clone()]
            } else {
                vec![input.clone()]
            };
            let error = runtime
                .get_func(&mut store, export)
                .unwrap()
                .call(&mut store, &args, &mut [Val::null_any_ref()])
                .unwrap_err();
            assert!(
                error.is::<wasmtime::ThrownException>(),
                "{export}: {error:#}"
            );
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
            store.gc(None).unwrap();
            let recovered = nominal_value(
                &mut store,
                runtime,
                "primitive-safe-integer-remainder",
                &[three.clone(), three.clone()],
            );
            assert_eq!(
                recovered
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)
                    .unwrap()
                    .unwrap()
                    .field(&mut store, 0)
                    .unwrap()
                    .unwrap_f64()
                    .to_bits(),
                0.0f64.to_bits()
            );
        }
        let output = nominal_value(&mut store, runtime, "primitive-f64-safe-integer", &[input]);
        assert_eq!(
            output
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)
                .unwrap()
                .unwrap()
                .get_u32(),
            2
        );
    }
}

#[test]
fn runtime_abi_identity_slots_keep_metadata_and_reject_corruption_without_traps() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[empty.clone()]);
    let object = nominal_value(
        &mut store,
        runtime,
        "object-new",
        &[descriptor.clone(), empty],
    );
    let closure = nominal_value(&mut store, runtime, "arithmetic-add", &[]);
    let text = nominal_value(&mut store, runtime, "string-new", &[Val::I32(0)]);
    let error = nominal_value(&mut store, runtime, "language-error-new", &[text]);
    let mut seen = std::collections::BTreeSet::new();
    for (owner, slot) in [(descriptor, 4), (object, 3), (closure, 4), (error, 4)] {
        let reference = owner
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(
            reference
                .field(&mut store, slot)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)
                .unwrap()
                .unwrap()
                .get_u32(),
            0
        );
        let cached = nominal_value(&mut store, runtime, "identity-uid", &[owner.clone()]);
        let id = cached
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64()
            .to_bits();
        assert!(seen.insert(id));
        store.gc(None).unwrap();
        let repeated = nominal_value(&mut store, runtime, "identity-uid", &[owner.clone()]);
        assert!(wasmtime::Rooted::ref_eq(
            &store,
            cached.unwrap_anyref().unwrap(),
            repeated.unwrap_anyref().unwrap()
        )
        .unwrap());
        for invalid in [
            Val::null_any_ref(),
            nominal_value(&mut store, runtime, "false", &[]),
            nominal_value(
                &mut store,
                runtime,
                "number-box",
                &[Val::F64(0.0f64.to_bits())],
            ),
            nominal_value(
                &mut store,
                runtime,
                "number-box",
                &[Val::F64((-1.0f64).to_bits())],
            ),
            nominal_value(
                &mut store,
                runtime,
                "number-box",
                &[Val::F64(1.5f64.to_bits())],
            ),
            nominal_value(
                &mut store,
                runtime,
                "number-box",
                &[Val::F64(f64::NAN.to_bits())],
            ),
            nominal_value(
                &mut store,
                runtime,
                "number-box",
                &[Val::F64(f64::INFINITY.to_bits())],
            ),
            nominal_value(
                &mut store,
                runtime,
                "number-box",
                &[Val::F64(9007199254740992.0f64.to_bits())],
            ),
        ] {
            reference.set_field(&mut store, slot, invalid).unwrap();
            store.gc(None).unwrap();
            let error = runtime
                .get_func(&mut store, "identity-uid")
                .unwrap()
                .call(&mut store, &[owner.clone()], &mut [Val::null_any_ref()])
                .unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
            reference
                .set_field(&mut store, slot, cached.clone())
                .unwrap();
            let restored = nominal_value(&mut store, runtime, "identity-uid", &[owner.clone()]);
            assert!(wasmtime::Rooted::ref_eq(
                &store,
                cached.unwrap_anyref().unwrap(),
                restored.unwrap_anyref().unwrap()
            )
            .unwrap());
        }
    }
}

#[test]
fn runtime_abi2_rejects_real_abi1_layouts_and_manifests_before_initializers() {
    #[path = "support/abi1_types.rs"]
    mod abi1;
    use wasm_encoder::{
        CodeSection, Function, FunctionSection, HeapType, ImportSection, Instruction,
        Module as EncodedModule, RefType, StartSection, ValType,
    };
    let engine = support::engine();
    let mut store = Store::new(&engine, 0u32);
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut linker = wasmtime::Linker::new(&engine);
    linker.instance(&mut store, "runtime", runtime).unwrap();
    linker
        .func_wrap(
            "fixture",
            "bump",
            |mut caller: wasmtime::Caller<'_, u32>| {
                *caller.data_mut() += 1;
            },
        )
        .unwrap();
    for version in [1, runtime_abi::VERSION] {
        let mut types = abi1::prelude();
        types.ty().function(
            [
                ValType::Ref(RefType::EQREF),
                ValType::Ref(RefType {
                    nullable: false,
                    heap_type: HeapType::Concrete(2),
                }),
            ],
            [ValType::Ref(RefType::EQREF)],
        );
        types.ty().function([], []);
        let mut imports = ImportSection::new();
        imports.import("runtime", "invoke", wasm_encoder::EntityType::Function(10));
        imports.import("fixture", "bump", wasm_encoder::EntityType::Function(11));
        let mut functions = FunctionSection::new();
        functions.function(11);
        let mut code = CodeSection::new();
        let mut function = Function::new([]);
        function
            .instruction(&Instruction::Call(1))
            .instruction(&Instruction::End);
        code.function(&function);
        let manifest = runtime_abi::Manifest {
            runtime_abi: version,
            ..runtime_abi::Manifest::default()
        };
        let mut encoded = EncodedModule::new();
        encoded
            .section(&manifest.section())
            .section(&types)
            .section(&imports)
            .section(&functions)
            .section(&StartSection { function_index: 2 })
            .section(&code);
        let bytes = encoded.finish();
        let error =
            runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap_err();
        assert!(
            error.contains(if version == 1 {
                "version mismatch"
            } else {
                "prelude"
            }),
            "{error}"
        );
        // Independent engine linking also rejects the old recursive group even
        // if a caller falsely labels it ABI2 or bypasses the manifest gate.
        let module = Module::new(&engine, bytes).unwrap();
        assert!(linker.instantiate(&mut store, &module).is_err());
        assert_eq!(*store.data(), 0);
    }
}

#[test]
fn runtime_abi_uid_safe_integer_exhaustion_preserves_cached_owners() {
    // Development fixture adds access to the scalar UID allocator global;
    // the production runtime deliberately exports no allocator mutation API.
    use wasm_encoder::{ExportKind, ExportSection, Module as EncodedModule, RawSection};
    let original = runtime_abi::module();
    let mut encoded = EncodedModule::new();
    let mut counter = None;
    for payload in wasmparser::Parser::new(0).parse_all(&original) {
        let payload = payload.unwrap();
        if let wasmparser::Payload::GlobalSection(ref globals) = payload {
            for (index, global) in globals.clone().into_iter().enumerate() {
                let global = global.unwrap();
                if global.ty.mutable && global.ty.content_type == wasmparser::ValType::I64 {
                    let mut initializer = global.init_expr.get_operators_reader();
                    if matches!(
                        initializer.read().unwrap(),
                        wasmparser::Operator::I64Const { value: 1 }
                    ) && matches!(initializer.read().unwrap(), wasmparser::Operator::End)
                        && initializer.eof()
                    {
                        assert!(
                            counter.replace(u32::try_from(index).unwrap()).is_none(),
                            "UID allocator fixture must identify exactly one seeded i64 counter"
                        );
                    }
                }
            }
        }
        if let wasmparser::Payload::ExportSection(ref exports) = payload {
            let mut section = ExportSection::new();
            for export in exports.clone() {
                let export = export.unwrap();
                let kind = match export.kind {
                    wasmparser::ExternalKind::Func => ExportKind::Func,
                    wasmparser::ExternalKind::Table => ExportKind::Table,
                    wasmparser::ExternalKind::Memory => ExportKind::Memory,
                    wasmparser::ExternalKind::Global => ExportKind::Global,
                    wasmparser::ExternalKind::Tag => ExportKind::Tag,
                    wasmparser::ExternalKind::FuncExact => {
                        panic!("unexpected exact function export")
                    }
                };
                section.export(export.name, kind, export.index);
            }
            section.export("review-uid-counter", ExportKind::Global, counter.unwrap());
            encoded.section(&section);
        } else if let Some((id, range)) = payload.as_section() {
            encoded.section(&RawSection {
                id,
                data: &original[range.start as usize..range.end as usize],
            });
        }
    }
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, encoded.finish()).unwrap(),
        &[],
    )
    .unwrap();
    let counter = runtime
        .get_global(&mut store, "review-uid-counter")
        .unwrap();
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let first = nominal_value(&mut store, runtime, "descriptor-new", &[empty.clone()]);
    let second = nominal_value(&mut store, runtime, "descriptor-new", &[empty]);
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let cell = nominal_value(&mut store, runtime, "binding-new", &[nil.clone()]);
    let schema = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let key = nominal_value(&mut store, runtime, "descriptor-new", &[schema]);
    let malformed = runtime
        .get_func(&mut store, "ifn-live-dispatcher-new")
        .unwrap()
        .call(&mut store, &[key, cell, nil], &mut [Val::null_any_ref()])
        .unwrap_err();
    assert!(malformed.is::<wasmtime::ThrownException>(), "{malformed:#}");
    assert!(!malformed.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    const LIMIT: i64 = 9_007_199_254_740_991;
    counter.set(&mut store, Val::I64(LIMIT)).unwrap();
    let cached = nominal_value(&mut store, runtime, "identity-uid", &[first.clone()]);
    let boxed = cached
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(
        boxed.field(&mut store, 0).unwrap().unwrap_f64(),
        LIMIT as f64
    );
    assert_eq!(counter.get(&mut store).unwrap_i64(), LIMIT + 1);
    store.gc(None).unwrap();
    let repeat = nominal_value(&mut store, runtime, "identity-uid", &[first.clone()]);
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        cached.unwrap_anyref().unwrap(),
        repeat.unwrap_anyref().unwrap()
    )
    .unwrap());
    let error = runtime
        .get_func(&mut store, "identity-uid")
        .unwrap()
        .call(&mut store, &[second.clone()], &mut [Val::null_any_ref()])
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    assert!(!error.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    assert_eq!(counter.get(&mut store).unwrap_i64(), LIMIT + 1);
    let fresh = second
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(
        fresh
            .field(&mut store, 4)
            .unwrap()
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0
    );
    // Exhaustion and its language error leave existing owner storage usable.
    let restored = nominal_value(&mut store, runtime, "identity-uid", &[first]);
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        cached.unwrap_anyref().unwrap(),
        restored.unwrap_anyref().unwrap()
    )
    .unwrap());
}

#[test]
fn runtime_abi_callable_adapter_roots_source_method_and_rejects_corrupt_environments() {
    use wasm_encoder::{HeapType, Instruction};
    fn language_error(store: &mut Store<()>, runtime: Instance, export: &str, args: &[Val]) {
        let error = runtime
            .get_func(&mut *store, export)
            .unwrap()
            .call(&mut *store, args, &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        let exception = store.take_pending_exception().unwrap();
        assert!(wasmtime::Tag::eq(
            &exception.tag(&mut *store).unwrap(),
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
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let one = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[empty.clone()]);
    let key = nominal_value(&mut store, runtime, "descriptor-new", &[one.clone()]);
    let owner = nominal_value(
        &mut store,
        runtime,
        "object-new",
        &[descriptor.clone(), empty.clone()],
    );
    let method = nominal_value(&mut store, runtime, "predicate-nil", &[]);
    runtime
        .get_func(&mut store, "protocol-method-set")
        .unwrap()
        .call(
            &mut store,
            &[descriptor.clone(), key.clone(), method],
            &mut [],
        )
        .unwrap();
    let bound = nominal_value(
        &mut store,
        runtime,
        "callable-bind",
        &[owner.clone(), key.clone()],
    );
    // Later table replacement cannot replace a method already captured as callee.
    // This valid replacement has arity2, so the newer bound callback errors.
    let replacement = nominal_value(&mut store, runtime, "predicate-identical", &[]);
    runtime
        .get_func(&mut store, "protocol-method-set")
        .unwrap()
        .call(&mut store, &[descriptor, key.clone(), replacement], &mut [])
        .unwrap();
    store.gc(None).unwrap();
    let result = nominal_value(
        &mut store,
        runtime,
        "invoke",
        &[bound.clone(), empty.clone()],
    );
    assert_eq!(
        result
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        2
    );
    let newer = nominal_value(
        &mut store,
        runtime,
        "callable-bind",
        &[owner.clone(), key.clone()],
    );
    language_error(&mut store, runtime, "invoke", &[newer, empty.clone()]);
    for malformed in [nil.clone(), empty.clone(), Val::null_any_ref()] {
        language_error(
            &mut store,
            runtime,
            "callable-bind",
            &[owner.clone(), malformed],
        );
    }
    // Independently copy the actual callback with malformed closure environments.
    let mut types = runtime_abi::prelude();
    let value = wasm_encoder::ValType::Ref(wasm_encoder::RefType::EQREF);
    types.ty().function([value, value], [value]);
    let mut functions = wasm_encoder::FunctionSection::new();
    functions.function(10);
    let mut exports = wasm_encoder::ExportSection::new();
    exports.export("copy-callback", wasm_encoder::ExportKind::Func, 0);
    let mut code = wasm_encoder::CodeSection::new();
    let mut forge = wasm_encoder::Function::new([]);
    forge
        .instruction(&Instruction::LocalGet(1))
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(4)))
        .instruction(&Instruction::StructGet {
            struct_type_index: 4,
            field_index: 1,
        })
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::I32Const(-1))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::RefI31)
        .instruction(&Instruction::StructNew(4))
        .instruction(&Instruction::End);
    code.function(&forge);
    let mut module = wasm_encoder::Module::new();
    module
        .section(&types)
        .section(&functions)
        .section(&exports)
        .section(&code);
    let foreign = Instance::new(
        &mut store,
        &Module::new(&engine, module.finish()).unwrap(),
        &[],
    )
    .unwrap();
    for malformed in [nil, empty.clone(), one, Val::null_any_ref()] {
        let copied = nominal_value(
            &mut store,
            foreign,
            "copy-callback",
            &[bound.clone(), malformed],
        );
        store.gc(None).unwrap();
        language_error(&mut store, runtime, "invoke", &[copied, empty.clone()]);
    }
    // Corruption did not poison the valid rooted callback or runtime exception tag.
    let result = nominal_value(&mut store, runtime, "invoke", &[bound, empty]);
    assert_eq!(
        result
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        2
    );
}

#[test]
fn runtime_abi_apply_callbacks_reject_foreign_environments_and_empty_push_buffers() {
    use wasm_encoder::{HeapType, Instruction};
    fn language_error(store: &mut Store<()>, runtime: Instance, callback: Val, args: Val) {
        let error = runtime
            .get_func(&mut *store, "invoke")
            .unwrap()
            .call(&mut *store, &[callback, args], &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        let exception = store.take_pending_exception().unwrap();
        assert!(wasmtime::Tag::eq(
            &exception.tag(&mut *store).unwrap(),
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
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let one = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    let keys = nominal_value(&mut store, runtime, "args-new", &[Val::I32(22)]);
    let owner = nominal_value(&mut store, runtime, "arithmetic-add", &[]);
    let mut types = runtime_abi::prelude();
    let value = wasm_encoder::ValType::Ref(wasm_encoder::RefType::EQREF);
    types.ty().function([value, value], [value]);
    let mut functions = wasm_encoder::FunctionSection::new();
    functions.function(10);
    let mut exports = wasm_encoder::ExportSection::new();
    exports.export("copy-callback", wasm_encoder::ExportKind::Func, 0);
    let mut code = wasm_encoder::CodeSection::new();
    let mut forge = wasm_encoder::Function::new([]);
    forge
        .instruction(&Instruction::LocalGet(1))
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(4)))
        .instruction(&Instruction::StructGet {
            struct_type_index: 4,
            field_index: 1,
        })
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::I32Const(-1))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::RefI31)
        .instruction(&Instruction::StructNew(4))
        .instruction(&Instruction::End);
    code.function(&forge);
    let mut module = wasm_encoder::Module::new();
    module
        .section(&types)
        .section(&functions)
        .section(&exports)
        .section(&code);
    let foreign = Instance::new(
        &mut store,
        &Module::new(&engine, module.finish()).unwrap(),
        &[],
    )
    .unwrap();
    for export in [
        "closure-call-method",
        "closure-apply-method",
        "ifn-call-method",
        "ifn-apply-method",
    ] {
        let arguments = if export.starts_with("ifn-") {
            vec![owner.clone(), keys.clone()]
        } else {
            vec![owner.clone()]
        };
        let valid = nominal_value(&mut store, runtime, export, &arguments);
        for environment in [nil.clone(), empty.clone(), one.clone(), Val::null_any_ref()] {
            let copied = nominal_value(
                &mut store,
                foreign,
                "copy-callback",
                &[valid.clone(), environment],
            );
            store.gc(None).unwrap();
            language_error(&mut store, runtime, copied, empty.clone());
        }
    }
    for method in ["source-array-push-method", "source-array-pop-method"] {
        let wrapper = nominal_value(&mut store, runtime, method, &[]);
        let tagged = nominal_value(&mut store, runtime, "closure-environment", &[wrapper]);
        let tagged = tagged
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        let fields = tagged.fields(&mut store).unwrap().collect::<Vec<_>>();
        let array = fields[1]
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        let anchored = array.get(&mut store, 0).unwrap();
        let copied = nominal_value(
            &mut store,
            foreign,
            "copy-callback",
            &[anchored, nil.clone()],
        );
        store.gc(None).unwrap();
        language_error(&mut store, runtime, copied.clone(), empty.clone());
        // A nonempty invocation still rejects a foreign physical receiver.
        language_error(&mut store, runtime, copied, one.clone());
    }
    let valid = nominal_value(&mut store, runtime, "closure-call-method", &[owner]);
    let value = nominal_value(&mut store, runtime, "invoke", &[valid, empty]);
    let bits = value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .fields(&mut store)
        .unwrap()
        .collect::<Vec<_>>();
    assert!(matches!(bits.as_slice(), [Val::F64(bits)] if *bits == 0.0f64.to_bits()));
}

#[test]
fn runtime_abi_language_error_predicate_uses_rooted_descriptor_identity_after_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let message = nominal_value(&mut store, runtime, "string-new", &[Val::I32(0)]);
    let error = nominal_value(&mut store, runtime, "language-error-new", &[message]);
    let payload = error
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    let mut payload_fields = payload.fields(&mut store).unwrap().collect::<Vec<_>>();
    let descriptor = payload_fields[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    let fields = descriptor.fields(&mut store).unwrap().collect::<Vec<_>>();
    let ty = descriptor.ty(&store).unwrap();
    let allocator = wasmtime::StructRefPre::new(&mut store, ty);
    let fake_descriptor = wasmtime::StructRef::new(&mut store, &allocator, &fields).unwrap();
    payload_fields[0] = Val::AnyRef(Some(fake_descriptor.to_anyref()));
    let ty = payload.ty(&store).unwrap();
    let allocator = wasmtime::StructRefPre::new(&mut store, ty);
    let fake = wasmtime::StructRef::new(&mut store, &allocator, &payload_fields).unwrap();
    store.gc(None).unwrap();
    let predicate = runtime.get_func(&mut store, "language-error-is").unwrap();
    for (value, expected) in [
        (error, 1),
        (Val::AnyRef(Some(fake.to_anyref())), 0),
        (Val::null_any_ref(), 0),
    ] {
        let mut result = [Val::I32(-1)];
        predicate.call(&mut store, &[value], &mut result).unwrap();
        assert_eq!(result[0].unwrap_i32(), expected);
    }
}

#[test]
fn runtime_abi_error_message_read_rejects_copied_nominal_descriptor_after_gc() {
    fn text(store: &mut Store<()>, runtime: Instance, units: &[u16]) -> Val {
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
        for (index, unit) in units.iter().enumerate() {
            array
                .set(&mut *store, index as u32, Val::I32(*unit as i32))
                .unwrap();
        }
        value
    }
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let expected = [0xd83d, 0x0000, 0xdc00];
    let message = text(&mut store, runtime, &expected);
    let key = text(
        &mut store,
        runtime,
        &"message".encode_utf16().collect::<Vec<_>>(),
    );
    let error = nominal_value(&mut store, runtime, "language-error-new", &[message]);
    let payload = error
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    let mut fields = payload.fields(&mut store).unwrap().collect::<Vec<_>>();
    let descriptor = fields[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    let descriptor_fields = descriptor.fields(&mut store).unwrap().collect::<Vec<_>>();
    let ty = descriptor.ty(&store).unwrap();
    let pre = wasmtime::StructRefPre::new(&mut store, ty);
    // Even every descriptor field, including the numeric nominal ID, is copied.
    let copied = wasmtime::StructRef::new(&mut store, &pre, &descriptor_fields).unwrap();
    fields[0] = Val::AnyRef(Some(copied.to_anyref()));
    let ty = payload.ty(&store).unwrap();
    let pre = wasmtime::StructRefPre::new(&mut store, ty);
    let forged = wasmtime::StructRef::new(&mut store, &pre, &fields).unwrap();
    store.gc(None).unwrap();
    let getter = runtime.get_func(&mut store, "named-property-get").unwrap();
    let failure = getter
        .call(
            &mut store,
            &[Val::AnyRef(Some(forged.to_anyref())), key.clone()],
            &mut [Val::null_any_ref()],
        )
        .unwrap_err();
    assert!(failure.is::<wasmtime::ThrownException>(), "{failure:#}");
    assert!(!failure.is::<wasmtime::Trap>());
    let exception = store.take_pending_exception().unwrap();
    assert!(wasmtime::Tag::eq(
        &exception.tag(&mut store).unwrap(),
        &runtime.get_tag(&mut store, "language-exception").unwrap(),
        &store
    ));
    let mut output = [Val::null_any_ref()];
    getter.call(&mut store, &[error, key], &mut output).unwrap();
    store.gc(None).unwrap();
    let array = output[0]
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    let units = array
        .elems(&mut store)
        .unwrap()
        .map(|value| value.unwrap_i32() as u16)
        .collect::<Vec<_>>();
    assert_eq!(units, expected);
    assert!(!store.has_pending_exception());
}

#[test]
fn runtime_abi_sparse_array_length_presence_and_uint32_boundary_survive_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    for length in [0_u32, 1_000_001, u32::MAX] {
        let storage = nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-new",
            &[Val::I32(length as i32)],
        );
        // A logical maximum-length array has constant-size backing before writes.
        assert_eq!(
            storage
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)
                .unwrap()
                .unwrap()
                .len(&store)
                .unwrap(),
            4
        );
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-length",
                &[storage.clone()]
            )
            .unwrap_i32() as u32,
            length
        );
        let absent = nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-get",
            &[storage.clone(), Val::I32(7)],
        );
        assert_eq!(
            absent
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)
                .unwrap()
                .unwrap()
                .get_u32(),
            6
        );
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-has",
                &[storage.clone(), Val::I32(7)]
            )
            .unwrap_i32(),
            0
        );
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-set",
            &[storage.clone(), Val::I32(7), absent],
        );
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-has",
                &[storage.clone(), Val::I32(7)]
            )
            .unwrap_i32(),
            1
        );
        {
            let mut scope = wasmtime::RootScope::new(&mut store);
            let mut boxed = [Val::null_any_ref()];
            runtime
                .get_func(&mut scope, "number-box")
                .unwrap()
                .call(&mut scope, &[Val::F64(23.0_f64.to_bits())], &mut boxed)
                .unwrap();
            let mut result = [Val::null_any_ref()];
            runtime
                .get_func(&mut scope, "source-array-sparse-set")
                .unwrap()
                .call(
                    &mut scope,
                    &[storage.clone(), Val::I32(-2), boxed[0].clone()],
                    &mut result,
                )
                .unwrap();
            runtime
                .get_func(&mut scope, "source-array-sparse-set")
                .unwrap()
                .call(
                    &mut scope,
                    &[storage.clone(), Val::I32(-1), boxed[0].clone()],
                    &mut result,
                )
                .unwrap();
        }
        store.gc(None).unwrap();
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-length",
                &[storage.clone()]
            )
            .unwrap_i32() as u32,
            u32::MAX
        );
        for index in [-2, -1] {
            let value = nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-get",
                &[storage.clone(), Val::I32(index)],
            );
            assert_eq!(
                value
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)
                    .unwrap()
                    .unwrap()
                    .field(&mut store, 0)
                    .unwrap()
                    .unwrap_f64(),
                23.0
            );
        }
    }
    let storage = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-new",
        &[Val::I32(0)],
    );
    let value = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(29.0_f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-set",
        &[storage.clone(), Val::I32(-1), value],
    );
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-length",
            &[storage]
        )
        .unwrap_i32(),
        0,
        "uint32-max property must not extend length"
    );
}

#[test]
fn runtime_abi_sparse_array_shrink_deletes_indices_without_resurrection_or_length_change_on_delete()
{
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let storage = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-new",
        &[Val::I32(-1)],
    );
    let value = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(17.0_f64.to_bits())],
    );
    for index in [0, 7, 3, -2, -1] {
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-set",
            &[storage.clone(), Val::I32(index), value.clone()],
        );
    }
    runtime
        .get_func(&mut store, "source-array-sparse-set-length")
        .unwrap()
        .call(&mut store, &[storage.clone(), Val::I32(4)], &mut [])
        .unwrap();
    store.gc(None).unwrap();
    for (index, present) in [(0, 1), (3, 1), (7, 0), (-2, 0), (-1, 1)] {
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-has",
                &[storage.clone(), Val::I32(index)]
            )
            .unwrap_i32(),
            present,
            "shrink index {index}"
        );
    }
    runtime
        .get_func(&mut store, "source-array-sparse-set-length")
        .unwrap()
        .call(&mut store, &[storage.clone(), Val::I32(-1)], &mut [])
        .unwrap();
    for index in [7, -2] {
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-has",
                &[storage.clone(), Val::I32(index)]
            )
            .unwrap_i32(),
            0,
            "grow must not restore deleted index {index}"
        );
    }
    // Remove head, middle/tail and absent properties; delete preserves length.
    for index in [-1, 3, 0, 123] {
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-delete",
                &[storage.clone(), Val::I32(index)]
            )
            .unwrap_i32(),
            1
        );
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-has",
                &[storage.clone(), Val::I32(index)]
            )
            .unwrap_i32(),
            0
        );
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-length",
                &[storage.clone()]
            )
            .unwrap_i32(),
            -1
        );
        store.gc(None).unwrap();
    }
}

#[test]
fn runtime_abi_sparse_array_rejects_malformed_length_instead_of_saturating() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    for bad in [-1.0, 0.5, f64::NAN, f64::INFINITY, 4294967296.0] {
        let storage = nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-new",
            &[Val::I32(0)],
        );
        let invalid = nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64(bad.to_bits())],
        );
        storage
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap()
            .set(&mut store, 0, invalid)
            .unwrap();
        let error = runtime
            .get_func(&mut store, "source-array-sparse-length")
            .unwrap()
            .call(&mut store, &[storage], &mut [Val::I32(0)])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{bad}: {error:#}");
    }
    let storage = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-new",
        &[Val::I32(0)],
    );
    for value in [17.0_f64, 23.0] {
        let boxed = nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64(value.to_bits())],
        );
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-set",
            &[storage.clone(), Val::I32(7), boxed],
        );
    }
    store.gc(None).unwrap();
    let value = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-get",
        &[storage.clone(), Val::I32(7)],
    );
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        23.0
    );
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-length",
            &[storage]
        )
        .unwrap_i32(),
        8
    );
}

#[test]
fn runtime_abi_sparse_array_rejects_corrupt_chains_before_shrink_mutates_length() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    for corruption in [
        "fractional-index",
        "nan-index",
        "bad-tail",
        "cycle",
        "bad-shape",
    ] {
        let storage = nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-new",
            &[Val::I32(9)],
        );
        let value = nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64(17.0_f64.to_bits())],
        );
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-set",
            &[storage.clone(), Val::I32(7), value.clone()],
        );
        let fields = storage
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        let node = fields.get(&mut store, 1).unwrap();
        let entry = node
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        match corruption {
            "fractional-index" | "nan-index" => {
                let index = if corruption == "fractional-index" {
                    0.5_f64
                } else {
                    f64::NAN
                };
                let bad = nominal_value(
                    &mut store,
                    runtime,
                    "number-box",
                    &[Val::F64(index.to_bits())],
                );
                entry.set(&mut store, 0, bad).unwrap();
            }
            "bad-tail" => entry.set(&mut store, 2, value).unwrap(),
            "cycle" => entry.set(&mut store, 2, node).unwrap(),
            "bad-shape" => fields.set(&mut store, 1, storage.clone()).unwrap(),
            _ => unreachable!(),
        }
        let error = runtime
            .get_func(&mut store, "source-array-sparse-set-length")
            .unwrap()
            .call(&mut store, &[storage, Val::I32(0)], &mut [])
            .unwrap_err();
        assert!(
            error.is::<wasmtime::ThrownException>(),
            "{corruption}: {error:#}"
        );
        let length = fields.get(&mut store, 0).unwrap();
        assert_eq!(
            length
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap()
                .field(&mut store, 0)
                .unwrap()
                .unwrap_f64(),
            9.0,
            "{corruption} mutated length"
        );
    }
}

#[test]
fn runtime_abi_sparse_array_bulk_ingress_copies_input_and_preserves_explicit_undefined() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let buffer = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1024)]);
    let input = buffer
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    let seventeen = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(17.0_f64.to_bits())],
    );
    input.set(&mut store, 0, seventeen.clone()).unwrap();
    input.set(&mut store, 1023, seventeen).unwrap();
    let empty = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-new",
        &[Val::I32(0)],
    );
    let undefined = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-get",
        &[empty, Val::I32(0)],
    );
    input.set(&mut store, 1, undefined).unwrap();
    let storage = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-from-args",
        &[buffer.clone()],
    );
    let other = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-from-args",
        &[buffer],
    );
    let twenty_three = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(23.0_f64.to_bits())],
    );
    input.set(&mut store, 0, twenty_three.clone()).unwrap();
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-set",
        &[other, Val::I32(1023), twenty_three],
    );
    store.gc(None).unwrap();
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-length",
            &[storage.clone()]
        )
        .unwrap_i32(),
        1024
    );
    for index in [0, 1023] {
        let value = nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-get",
            &[storage.clone(), Val::I32(index)],
        );
        assert_eq!(
            value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap()
                .field(&mut store, 0)
                .unwrap()
                .unwrap_f64(),
            17.0,
            "bulk storage must be independent"
        );
    }
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-has",
            &[storage.clone(), Val::I32(1)]
        )
        .unwrap_i32(),
        1
    );
    let value = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-get",
        &[storage, Val::I32(1)],
    );
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        6
    );
}

#[test]
fn runtime_abi_sparse_array_dense_prefix_presence_and_shrink_remain_independent() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let input = nominal_value(&mut store, runtime, "args-new", &[Val::I32(8)]);
    let storage = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-from-args",
        &[input],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-delete",
        &[storage.clone(), Val::I32(1)],
    );
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-has",
            &[storage.clone(), Val::I32(1)]
        )
        .unwrap_i32(),
        0
    );
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-length",
            &[storage.clone()]
        )
        .unwrap_i32(),
        8
    );
    let undefined = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-get",
        &[storage.clone(), Val::I32(1)],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-set",
        &[storage.clone(), Val::I32(1), undefined],
    );
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-has",
            &[storage.clone(), Val::I32(1)]
        )
        .unwrap_i32(),
        1
    );
    let set_length = runtime
        .get_func(&mut store, "source-array-sparse-set-length")
        .unwrap();
    set_length
        .call(&mut store, &[storage.clone(), Val::I32(4)], &mut [])
        .unwrap();
    set_length
        .call(&mut store, &[storage.clone(), Val::I32(8)], &mut [])
        .unwrap();
    store.gc(None).unwrap();
    for index in 0..8 {
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-has",
                &[storage.clone(), Val::I32(index)]
            )
            .unwrap_i32(),
            i32::from(index < 4),
            "dense shrink/grow index{index}"
        );
    }
    let value = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(23.0_f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-set",
        &[storage.clone(), Val::I32(7), value],
    );
    store.gc(None).unwrap();
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-has",
            &[storage.clone(), Val::I32(7)]
        )
        .unwrap_i32(),
        1
    );
}

#[test]
fn runtime_abi_sparse_array_rejects_duplicate_out_of_range_and_dense_overlap_keys() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    for mode in ["duplicate", "out-of-range", "unordered", "dense-overlap"] {
        let storage = if mode == "dense-overlap" {
            let input = nominal_value(&mut store, runtime, "args-new", &[Val::I32(8)]);
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-from-args",
                &[input],
            )
        } else {
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-new",
                &[Val::I32(10)],
            )
        };
        let value = nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64(17.0_f64.to_bits())],
        );
        for index in if mode == "dense-overlap" {
            vec![9]
        } else {
            vec![3, 7]
        } {
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-set",
                &[storage.clone(), Val::I32(index), value.clone()],
            );
        }
        let fields = storage
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        let head = fields
            .get(&mut store, 1)
            .unwrap()
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        let (entry, bad) = match mode {
            "duplicate" => (
                head.get(&mut store, 2)
                    .unwrap()
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)
                    .unwrap()
                    .unwrap(),
                3.0_f64,
            ),
            "out-of-range" => (head, 10.0),
            "unordered" => (head, 8.0),
            "dense-overlap" => (head, 2.0),
            _ => unreachable!(),
        };
        let key = nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64(bad.to_bits())],
        );
        entry.set(&mut store, 0, key).unwrap();
        let error = runtime
            .get_func(&mut store, "source-array-sparse-delete")
            .unwrap()
            .call(&mut store, &[storage, Val::I32(3)], &mut [Val::I32(0)])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{mode}: {error:#}");
        assert_eq!(
            fields
                .get(&mut store, 0)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap()
                .field(&mut store, 0)
                .unwrap()
                .unwrap_f64(),
            10.0
        );
    }
}

#[test]
fn runtime_abi_sparse_array_mutations_reject_invalid_presence_without_repairing_it() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    for operation in ["set", "delete", "set-length"] {
        let input = nominal_value(&mut store, runtime, "args-new", &[Val::I32(8)]);
        let storage = nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-from-args",
            &[input],
        );
        let fields = storage
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        let mask = fields
            .get(&mut store, 3)
            .unwrap()
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        mask.set(&mut store, 1, Val::I32(2)).unwrap();
        let mut arguments = vec![storage, Val::I32(1)];
        if operation == "set" {
            arguments.push(nominal_value(&mut store, runtime, "nil", &[]));
        }
        let mut result = if operation == "set-length" {
            vec![]
        } else {
            vec![Val::null_any_ref()]
        };
        let error = runtime
            .get_func(&mut store, &format!("source-array-sparse-{operation}"))
            .unwrap()
            .call(&mut store, &arguments, &mut result)
            .unwrap_err();
        assert!(
            error.is::<wasmtime::ThrownException>(),
            "{operation}: {error:#}"
        );
        assert_eq!(
            mask.get(&mut store, 1).unwrap().unwrap_i32(),
            2,
            "must not silently repair"
        );
        assert_eq!(
            fields
                .get(&mut store, 0)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap()
                .field(&mut store, 0)
                .unwrap()
                .unwrap_f64(),
            8.0
        );
    }
}

#[test]
fn runtime_abi_sparse_array_materialization_is_linear_snapshot_and_keeps_sparse_length_independent()
{
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let input = nominal_value(&mut store, runtime, "args-new", &[Val::I32(8)]);
    let storage = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-from-args",
        &[input],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-delete",
        &[storage.clone(), Val::I32(1)],
    );
    let number = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(23.0_f64.to_bits())],
    );
    for index in [12, -1] {
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-set",
            &[storage.clone(), Val::I32(index), number.clone()],
        );
    }
    let snapshot = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-to-args",
        &[storage.clone()],
    );
    let array = snapshot
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    assert_eq!(array.len(&store).unwrap(), 13, "ordinary max key excluded");
    for index in [1, 8, 9, 10, 11] {
        assert_eq!(
            array
                .get(&mut store, index)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)
                .unwrap()
                .unwrap()
                .get_u32(),
            6,
            "hole {index}"
        );
    }
    assert_eq!(
        array
            .get(&mut store, 12)
            .unwrap()
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        23.0
    );
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    array.set(&mut store, 12, nil).unwrap();
    store.gc(None).unwrap();
    let retained = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-get",
        &[storage, Val::I32(12)],
    );
    assert_eq!(
        retained
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        23.0,
        "snapshot is independent"
    );
    let large = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-new",
        &[Val::I32(-1)],
    );
    let error = runtime
        .get_func(&mut store, "source-array-sparse-to-args")
        .unwrap()
        .call(&mut store, &[large.clone()], &mut [Val::null_any_ref()])
        .unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>());
    assert_eq!(
        nominal_value(&mut store, runtime, "source-array-sparse-length", &[large]).unwrap_i32(),
        -1,
        "argument allocation failure does not invalidate logical array"
    );
}

#[test]
fn runtime_abi_source_array_sparse_owner_clone_preserves_holes_and_maximum_length_after_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let owner = nominal_value(
        &mut store,
        runtime,
        "source-array-holes-new",
        &[Val::I32(-1)],
    );
    let first = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(1.0_f64.to_bits())],
    );
    let last = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(4294967294.0_f64.to_bits())],
    );
    let ordinary = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(4294967295.0_f64.to_bits())],
    );
    let undefined = nominal_value(
        &mut store,
        runtime,
        "source-array-get",
        &[owner.clone(), first.clone()],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-set",
        &[owner.clone(), first, undefined],
    );
    {
        let mut scope = wasmtime::RootScope::new(&mut store);
        let mut value = [Val::null_any_ref()];
        runtime
            .get_func(&mut scope, "number-box")
            .unwrap()
            .call(&mut scope, &[Val::F64(23.0_f64.to_bits())], &mut value)
            .unwrap();
        for index in [last.clone(), ordinary] {
            runtime
                .get_func(&mut scope, "source-array-set")
                .unwrap()
                .call(
                    &mut scope,
                    &[owner.clone(), index, value[0].clone()],
                    &mut [Val::null_any_ref()],
                )
                .unwrap();
        }
    }
    let cloned = nominal_value(&mut store, runtime, "source-array-clone", &[owner.clone()]);
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    nominal_value(
        &mut store,
        runtime,
        "source-array-set",
        &[owner, last.clone(), nil],
    );
    store.gc(None).unwrap();
    let backing = nominal_value(
        &mut store,
        runtime,
        "source-array-backing",
        &[cloned.clone()],
    );
    for (index, present) in [(0, 0), (1, 1), (-2, 1), (-1, 0)] {
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-has",
                &[backing.clone(), Val::I32(index)]
            )
            .unwrap_i32(),
            present,
            "clone index{index}"
        );
    }
    let value = nominal_value(
        &mut store,
        runtime,
        "source-array-get",
        &[cloned.clone(), last],
    );
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        23.0
    );
    let length = nominal_value(&mut store, runtime, "source-array-length", &[cloned]);
    assert_eq!(
        length
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        4294967295.0
    );
}

#[test]
fn runtime_abi_sparse_array_slice_preserves_holes_dense_offsets_and_high_sparse_ranges() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let input = nominal_value(&mut store, runtime, "args-new", &[Val::I32(8)]);
    let source = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-from-args",
        &[input],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-delete",
        &[source.clone(), Val::I32(6)],
    );
    let number = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(23.0_f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-set",
        &[source.clone(), Val::I32(12), number.clone()],
    );
    let sliced = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-slice",
        &[source.clone(), Val::I32(4), Val::I32(13)],
    );
    for (index, present) in [(0, 1), (1, 1), (2, 0), (3, 1), (4, 0), (7, 0), (8, 1)] {
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-has",
                &[sliced.clone(), Val::I32(index)]
            )
            .unwrap_i32(),
            present,
            "dense/sparse slice {index}"
        );
    }
    let value = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-get",
        &[sliced, Val::I32(8)],
    );
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .field(&mut store, 0)
            .unwrap()
            .unwrap_f64(),
        23.0
    );
    let high = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-new",
        &[Val::I32(-1)],
    );
    let undefined = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-get",
        &[high.clone(), Val::I32(0)],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-set",
        &[high.clone(), Val::I32(-3), number.clone()],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-set",
        &[high.clone(), Val::I32(-2), undefined],
    );
    nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-set",
        &[high.clone(), Val::I32(-1), number],
    );
    let sliced = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-slice",
        &[high, Val::I32(-3), Val::I32(-1)],
    );
    store.gc(None).unwrap();
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-length",
            &[sliced.clone()]
        )
        .unwrap_i32(),
        2
    );
    for index in [0, 1] {
        assert_eq!(
            nominal_value(
                &mut store,
                runtime,
                "source-array-sparse-has",
                &[sliced.clone(), Val::I32(index)]
            )
            .unwrap_i32(),
            1
        );
    }
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-has",
            &[sliced, Val::I32(-1)]
        )
        .unwrap_i32(),
        0
    );
    // A captured end can survive shrinking during earlier bound conversion.
    runtime
        .get_func(&mut store, "source-array-sparse-set-length")
        .unwrap()
        .call(&mut store, &[source.clone(), Val::I32(0)], &mut [])
        .unwrap();
    let sliced = nominal_value(
        &mut store,
        runtime,
        "source-array-sparse-slice",
        &[source, Val::I32(4), Val::I32(13)],
    );
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-length",
            &[sliced.clone()]
        )
        .unwrap_i32(),
        9
    );
    assert_eq!(
        nominal_value(
            &mut store,
            runtime,
            "source-array-sparse-has",
            &[sliced, Val::I32(0)]
        )
        .unwrap_i32(),
        0
    );
}

#[test]
fn runtime_abi_native_factory_rejects_source_array_self_and_mutual_cycles_after_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    for mutual in [false, true] {
        let first = nominal_value(
            &mut store,
            runtime,
            "source-array-holes-new",
            &[Val::I32(1)],
        );
        let second = if mutual {
            nominal_value(
                &mut store,
                runtime,
                "source-array-holes-new",
                &[Val::I32(1)],
            )
        } else {
            first.clone()
        };
        let zero = nominal_value(
            &mut store,
            runtime,
            "number-box",
            &[Val::F64(0.0_f64.to_bits())],
        );
        nominal_value(
            &mut store,
            runtime,
            "source-array-set",
            &[first.clone(), zero.clone(), second.clone()],
        );
        if mutual {
            nominal_value(
                &mut store,
                runtime,
                "source-array-set",
                &[second, zero, first.clone()],
            );
        }
        let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
        args.unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap()
            .set(&mut store, 0, first)
            .unwrap();
        store.gc(None).unwrap();
        let error = runtime
            .get_func(&mut store, "native-object-factory-flatten")
            .unwrap()
            .call(&mut store, &[args], &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(
            error.is::<wasmtime::ThrownException>(),
            "mutual={mutual}: {error:#}"
        );
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
    }
    let leaf_args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    let leaf = nominal_value(&mut store, runtime, "source-array-new", &[leaf_args]);
    let wrapper_args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    wrapper_args
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, leaf)
        .unwrap();
    let wrapper = nominal_value(&mut store, runtime, "source-array-new", &[wrapper_args]);
    let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    args.unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .set(&mut store, 0, wrapper)
        .unwrap();
    let flattened = nominal_value(
        &mut store,
        runtime,
        "native-object-factory-flatten",
        &[args],
    );
    assert_eq!(
        flattened
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap()
            .len(&store)
            .unwrap(),
        2,
        "acyclic traversal recovers"
    );
}

#[test]
fn runtime_abi_array_constructor_lengths_and_range_error_identity_after_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let constructor = nominal_value(&mut store, runtime, "array-constructor", &[]);
    let range_constructor = nominal_value(&mut store, runtime, "range-error-constructor", &[]);
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let range = nominal_value(&mut store, runtime, "invoke", &[range_constructor.clone(), empty.clone()]);
    let descriptor = range.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 0).unwrap();
    assert_eq!(nominal_value(&mut store, runtime, "object-instance", &[descriptor.clone(), range.clone()]).unwrap_i32(), 1);
    store.gc(None).unwrap();
    let canonical = nominal_value(&mut store, runtime, "value-constructor", &[range]);
    assert!(wasmtime::Rooted::ref_eq(&store, canonical.unwrap_anyref().unwrap(), range_constructor.unwrap_anyref().unwrap()).unwrap());
    for length in [0.0_f64, 3.0, 1_000_001.0, 4_294_967_295.0] {
        let argument = nominal_value(&mut store, runtime, "number-box", &[Val::F64(length.to_bits())]);
        let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
        args.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap().set(&mut store, 0, argument).unwrap();
        let owner = nominal_value(&mut store, runtime, "invoke", &[constructor.clone(), args]);
        let backing = nominal_value(&mut store, runtime, "source-array-backing", &[owner]);
        assert_eq!(nominal_value(&mut store, runtime, "source-array-sparse-length", &[backing.clone()]).unwrap_i32() as u32, length as u32);
        if length != 0.0 {
            assert_eq!(nominal_value(&mut store, runtime, "source-array-sparse-has", &[backing, Val::I32(0)]).unwrap_i32(), 0);
        }
    }
    for length in [-1.0, 1.5, f64::NAN, f64::INFINITY, 4_294_967_296.0] {
        let argument = nominal_value(&mut store, runtime, "number-box", &[Val::F64(length.to_bits())]);
        let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
        args.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap().set(&mut store, 0, argument).unwrap();
        let error = runtime.get_func(&mut store, "invoke").unwrap().call(&mut store, &[constructor.clone(), args], &mut [Val::null_any_ref()]).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{length}: {error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        let exception = store.take_pending_exception().unwrap();
        let payload = exception.field(&mut store, 0).unwrap();
        assert_eq!(nominal_value(&mut store, runtime, "range-error?", &[payload.clone()]).unwrap_i32(), 1);
        assert_eq!(nominal_value(&mut store, runtime, "object-instance", &[descriptor.clone(), payload.clone()]).unwrap_i32(), 1);
        let message = payload.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 1).unwrap();
        let units: Vec<_> = message.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap().elems(&mut store).unwrap().map(|v| v.unwrap_i32() as u16).collect();
        assert_eq!(String::from_utf16(&units).unwrap(), "Invalid array length");
    }
    let undefined = Val::AnyRef(Some(wasmtime::AnyRef::from_i31(&mut store, wasmtime::I31::new_u32(6).unwrap())));
    let one = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    one.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap().set(&mut store, 0, undefined).unwrap();
    let range_constructor = nominal_value(&mut store, runtime, "range-error-constructor", &[]);
    let error = nominal_value(&mut store, runtime, "invoke", &[range_constructor.clone(), one.clone()]);
    let message = error.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 1).unwrap();
    assert_eq!(message.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap().len(&store).unwrap(), 0, "RangeError(undefined) has an empty message");
    let seven = nominal_value(&mut store, runtime, "number-box", &[Val::F64(7.0_f64.to_bits())]);
    let message_args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    message_args.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap().set(&mut store, 0, seven).unwrap();
    let error = nominal_value(&mut store, runtime, "invoke", &[range_constructor, message_args]);
    let message = error.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 1).unwrap();
    let units: Vec<_> = message.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap().elems(&mut store).unwrap().map(|v| v.unwrap_i32() as u16).collect();
    assert_eq!(String::from_utf16(&units).unwrap(), "7");
    let owner = nominal_value(&mut store, runtime, "invoke", &[constructor, one]);
    let backing = nominal_value(&mut store, runtime, "source-array-backing", &[owner]);
    assert_eq!(nominal_value(&mut store, runtime, "source-array-sparse-has", &[backing, Val::I32(0)]).unwrap_i32(), 1);
}

#[test]
fn runtime_abi_array_own_property_names_preserve_canonical_decimal_keys() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let method = nominal_value(&mut store, runtime, "source-array-has-own-method", &[]);
    store.gc(None).unwrap();
    let again = nominal_value(&mut store, runtime, "source-array-has-own-method", &[]);
    assert!(wasmtime::Rooted::ref_eq(&store, method.unwrap_anyref().unwrap(), again.unwrap_anyref().unwrap()).unwrap());
    let owner = nominal_value(&mut store, runtime, "source-array-holes-new", &[Val::I32(1)]);
    let empty = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let value = nominal_value(&mut store, runtime, "object-method-invoke", &[method.clone(), owner.clone(), empty.clone()]);
    assert_eq!(value.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 2);
    let zero = nominal_value(&mut store, runtime, "number-box", &[Val::F64(0.0_f64.to_bits())]);
    let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    args.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap().set(&mut store, 0, zero.clone()).unwrap();
    let hole = nominal_value(&mut store, runtime, "object-method-invoke", &[method.clone(), owner.clone(), args.clone()]);
    assert_eq!(hole.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 2);
    let undefined = nominal_value(&mut store, runtime, "source-array-get", &[owner.clone(), zero.clone()]);
    nominal_value(&mut store, runtime, "source-array-set", &[owner.clone(), zero, undefined]);
    let present = nominal_value(&mut store, runtime, "object-method-invoke", &[method.clone(), owner, args]);
    assert_eq!(present.unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 4);
    let failure = runtime.get_func(&mut store, "invoke").unwrap().call(&mut store, &[method, empty], &mut [Val::null_any_ref()]).unwrap_err();
    assert!(failure.is::<wasmtime::ThrownException>());
    assert!(!failure.is::<wasmtime::Trap>());
    assert!(store.take_pending_exception().is_some());
    for (name, expected) in [("0", 0), ("1", 1), ("4294967294", 4294967294), ("4294967295", 4294967295),
        ("", -1), ("00", -1), ("01", -1), ("-0", -1), (" 1", -1), ("1 ", -1),
        ("+1", -1), ("1.0", -1), ("1e0", -1), ("4294967296", -1), ("99999999999", -1), ("١", -1)] {
        let units: Vec<_> = name.encode_utf16().collect();
        let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(units.len() as i32)]);
        for (index, unit) in units.iter().enumerate() {
            assert_eq!(nominal_value(&mut store, runtime, "string-set-unit", &[key.clone(), Val::I32(index as i32), Val::I32(i32::from(*unit))]).unwrap_i32(), 1);
        }
        let mut result = [Val::I64(0)];
        runtime.get_func(&mut store, "source-array-own-key-index").unwrap().call(&mut store, &[key], &mut result).unwrap();
        assert_eq!(result[0].unwrap_i64(), expected, "{name:?}");
    }
}

#[test]
fn runtime_abi_array_push_overflow_preserves_all_writes_before_range_error() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    for initial in [4294967294_u32, 4294967295] {
        let owner = nominal_value(&mut store, runtime, "source-array-holes-new", &[Val::I32(initial as i32)]);
        let method = nominal_value(&mut store, runtime, "source-array-push-method", &[]);
        let args = nominal_value(&mut store, runtime, "args-new", &[Val::I32(3)]);
        for index in 0..3 {
            let value = nominal_value(&mut store, runtime, "number-box", &[Val::F64((17.0 + f64::from(index)).to_bits())]);
            args.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap().set(&mut store, index, value).unwrap();
        }
        let error = runtime.get_func(&mut store, "object-method-invoke").unwrap().call(&mut store, &[method, owner.clone(), args], &mut [Val::null_any_ref()]).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>());
        assert!(!error.is::<wasmtime::Trap>());
        let exception = store.take_pending_exception().unwrap();
        let payload = exception.field(&mut store, 0).unwrap();
        assert_eq!(nominal_value(&mut store, runtime, "range-error?", &[payload]).unwrap_i32(), 1);
        store.gc(None).unwrap();
        let backing = nominal_value(&mut store, runtime, "source-array-backing", &[owner.clone()]);
        assert_eq!(nominal_value(&mut store, runtime, "source-array-sparse-length", &[backing]).unwrap_i32() as u32, u32::MAX);
        for index in 0..3 {
            let key = nominal_value(&mut store, runtime, "number-box", &[Val::F64((f64::from(initial) + f64::from(index)).to_bits())]);
            let value = nominal_value(&mut store, runtime, "source-array-get", &[owner.clone(), key]);
            let number = value.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 0).unwrap().unwrap_f64();
            assert_eq!(number, 17.0 + f64::from(index));
        }
    }
}

#[test]
fn runtime_abi_array_ordinary_properties_reject_corrupt_chains_before_writes() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let key = nominal_value(&mut store, runtime, "string-new", &[Val::I32(1)]);
    assert_eq!(nominal_value(&mut store, runtime, "string-set-unit", &[key.clone(), Val::I32(0), Val::I32(120)]).unwrap_i32(), 1);
    let numeric = nominal_value(&mut store, runtime, "number-box", &[Val::F64(4294967296.0_f64.to_bits())]);
    for kind in 0..5 {
        let owner = nominal_value(&mut store, runtime, "source-array-holes-new", &[Val::I32(0)]);
        let fields = nominal_value(&mut store, runtime, "source-array-fields", &[owner.clone()]);
        let fields = fields.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
        let node = nominal_value(&mut store, runtime, "args-new", &[Val::I32(if kind == 0 { 2 } else { 3 })]);
        let array = node.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
        array.set(&mut store, 0, if kind == 1 { numeric.clone() } else { key.clone() }).unwrap();
        array.set(&mut store, 1, nil.clone()).unwrap();
        if kind != 0 {
            let tail = match kind {
                2 => numeric.clone(),
                3 => node.clone(),
                4 => {
                    let second = nominal_value(&mut store, runtime, "args-new", &[Val::I32(3)]);
                    let array = second.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
                    array.set(&mut store, 0, key.clone()).unwrap();
                    array.set(&mut store, 1, nil.clone()).unwrap();
                    array.set(&mut store, 2, node.clone()).unwrap();
                    second
                }
                _ => nil.clone(),
            };
            array.set(&mut store, 2, tail).unwrap();
        }
        fields.set(&mut store, 1, node).unwrap();
        store.gc(None).unwrap();
        let error = runtime.get_func(&mut store, "source-array-set").unwrap().call(&mut store, &[owner.clone(), numeric.clone(), numeric.clone()], &mut [Val::null_any_ref()]).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "kind={kind}: {error:#}");
        assert!(!error.is::<wasmtime::Trap>());
        assert!(store.take_pending_exception().is_some());
        let backing = nominal_value(&mut store, runtime, "source-array-backing", &[owner.clone()]);
        assert_eq!(nominal_value(&mut store, runtime, "source-array-sparse-length", &[backing]).unwrap_i32(), 0);
        fields.set(&mut store, 1, nil.clone()).unwrap();
        nominal_value(&mut store, runtime, "source-array-set", &[owner.clone(), numeric.clone(), numeric.clone()]);
        store.gc(None).unwrap();
        let recovered = nominal_value(&mut store, runtime, "source-array-get", &[owner, numeric.clone()]);
        assert!(wasmtime::Rooted::ref_eq(&store, recovered.unwrap_anyref().unwrap(), numeric.unwrap_anyref().unwrap()).unwrap());
    }
}
