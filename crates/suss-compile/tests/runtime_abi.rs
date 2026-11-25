//! Execute production ABI v3 runtime bytes, independently of Suss printing/equality.
mod support;
/// First type index a fragment appends after the shared ABI v3 prelude.
/// Fragment types follow the shared group.
const APPENDED: u32 = suss_compile::runtime_abi::TYPE_COUNT;
use suss_compile::runtime_abi;
use suss_compile::portable_session::{RuntimeNumber, runtime_number};
use wasmtime::{Instance, Module, Store, Val};

/// A runtime value that must be a long.
#[track_caller]
fn long_of(store: &mut Store<()>, value: &Val) -> i64 {
    match runtime_number(&mut *store, value).unwrap() {
        Some(RuntimeNumber::Long(value)) => value,
        other => panic!("expected a long, observed {other:?}"),
    }
}

/// A runtime value that must be a double.
#[track_caller]
fn double_of(store: &mut Store<()>, value: &Val) -> f64 {
    match runtime_number(&mut *store, value).unwrap() {
        Some(RuntimeNumber::Double(value)) => value,
        other => panic!("expected a double, observed {other:?}"),
    }
}

/// The ABI v3 sentinel tag of a language value: 0 nil, 2 false, 4 true, 6 undefined.
#[track_caller]
fn sentinel_of(store: &mut Store<()>, value: &Val) -> u32 {
    suss_compile::portable_session::sentinel_tag(&mut *store, value)
        .unwrap()
        .expect("nil, Boolean or undefined sentinel")
}

/// A runtime sentinel value: nil is a null eqref; the others are runtime globals.
fn sentinel_value(store: &mut Store<()>, runtime: Instance, tag: i32) -> Val {
    if tag == runtime_abi::NIL {
        return Val::AnyRef(None);
    }
    let (name, _) = runtime_abi::SENTINEL_EXPORTS
        .into_iter()
        .find(|(_, candidate)| *candidate == tag)
        .expect("sentinel tag");
    runtime.get_global(&mut *store, name).unwrap().get(&mut *store)
}

/// A host-built SENTINEL struct with an unknown tag: the ABI v3 shape of a
/// forged sentinel, distinct from every runtime singleton.
fn forged_sentinel(store: &mut Store<()>, runtime: Instance, tag: i32) -> Val {
    let genuine = sentinel_value(store, runtime, runtime_abi::FALSE);
    let ty = genuine
        .unwrap_anyref()
        .unwrap()
        .as_struct(&*store)
        .unwrap()
        .unwrap()
        .ty(&*store)
        .unwrap();
    let pre = wasmtime::StructRefPre::new(&mut *store, ty);
    let forged = wasmtime::StructRef::new(&mut *store, &pre, &[Val::I32(tag)]).unwrap();
    Val::AnyRef(Some(forged.to_anyref()))
}

#[test]
fn runtime_abi_owned_prototype_call_refs_use_typed_globals() {
    let engine = support::engine();
    let module = Module::new(&engine, runtime_abi::module())
        .expect("owned prototype wrappers must call typed function-reference globals");
    Instance::new(&mut Store::new(&engine, ()), &module, &[])
        .expect("owned prototype globals must initialize");
}

#[test]
fn runtime_abi_error_constructor_can_initialize_its_protocol_cache() {
    let engine = support::engine();
    let module = Module::new(&engine, runtime_abi::module()).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let constructor = runtime.get_func(&mut store, "error-constructor").unwrap();
    let mut output = [Val::null_any_ref()];
    constructor
        .call(&mut store, &[], &mut output)
        .expect("the Error descriptor must have an initialized protocol table");
    output[0]
        .unwrap_anyref()
        .expect("Error constructor result must be a reference");
}

#[test]
fn runtime_abi_object_constructor_accepts_reserved_closure_property_slots() {
    let engine = support::engine();
    let module = Module::new(&engine, runtime_abi::module()).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let constructor = runtime.get_func(&mut store, "object-constructor").unwrap();
    let mut output = [Val::null_any_ref()];
    constructor
        .call(&mut store, &[], &mut output)
        .expect("Object static initialization must preserve private closure keys 8 and 9");
    output[0]
        .unwrap_anyref()
        .expect("Object constructor result must be a reference");
}

/// Java's `Double.toString` (JDK 19+): the shortest digits that round-trip,
/// at least two, in plain notation for
/// magnitudes in [1e-3, 1e7) and computerized scientific notation otherwise.
fn java_double_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    if value == 0.0 {
        return format!("{sign}0.0");
    }
    let magnitude = value.abs();
    let split = |text: String| {
        let (mantissa, exponent) = text.split_once('e').unwrap();
        (mantissa.replace('.', ""), exponent.parse::<i32>().unwrap())
    };
    // Of the decimals with the shortest length (at least two digits) that
    // round-trip, Java picks the closest, ties to an even digit: exactly
    // Rust's correctly rounded fixed-precision formatting at that length.
    let shortest = split(format!("{magnitude:e}")).0.len().max(2);
    let (digits, exponent) = split(format!("{magnitude:.*e}", shortest - 1));
    let digits = match digits.trim_end_matches('0') {
        "" => "0".to_owned(),
        trimmed => trimmed.to_owned(),
    };
    if (1e-3..1e7).contains(&magnitude) {
        let point = exponent + 1;
        if point <= 0 {
            format!("{sign}0.{}{digits}", "0".repeat((-point) as usize))
        } else if point as usize >= digits.len() {
            format!("{sign}{digits}{}.0", "0".repeat(point as usize - digits.len()))
        } else {
            let (whole, fraction) = digits.split_at(point as usize);
            format!("{sign}{whole}.{fraction}")
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        format!("{sign}{first}.{rest}E{exponent}")
    }
}

#[test]
fn java_double_string_matches_reviewed_jvm_samples() {
    for (value, text) in [
        (1.0, "1.0"),
        (-0.0, "-0.0"),
        (0.5, "0.5"),
        (0.001, "0.001"),
        (1234567.0, "1234567.0"),
        (1e7, "1.0E7"),
        (1e21, "1.0E21"),
        (1e-7, "1.0E-7"),
        (1.5e300, "1.5E300"),
        (f64::MIN_POSITIVE * f64::EPSILON, "4.9E-324"),
        (2.0 * f64::MIN_POSITIVE * f64::EPSILON, "9.9E-324"),
        (3e-323, "3.0E-323"),
        (1.25e-322, "1.24E-322"),
        (f64::MAX, "1.7976931348623157E308"),
        (0.1 + 0.2, "0.30000000000000004"),
        (f64::from_bits(0x42df575484f5b3e8), "1.3783976246241562E14"),
        (f64::from_bits(0x43119d557fc60531), "1.2395162863865082E15"),
    ] {
        assert_eq!(java_double_string(value), text, "{value:e}");
    }
}

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
    let mut mismatches = Vec::new();
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
            // The corpus pins ClojureScript's formatting; Clojure prints a
            // double as Java's Double.toString does.
            let expected = java_double_string(f64::from_bits(bits));
            let found = String::from_utf16(&units).unwrap();
            if found != expected {
                mismatches.push(format!("{input}: {found} vs Java {expected}"));
            }
            let mut number = [Val::F64(0)];
            parsed.call(&mut scope, &text, &mut number).unwrap();
            let parsed_bits = number[0].unwrap_f64().to_bits();
            // Java's text round-trips the double (every NaN reads as NaN).
            assert!(
                parsed_bits == bits || (f64::from_bits(bits).is_nan() && f64::from_bits(parsed_bits).is_nan()),
                "{input}: {parsed_bits:016x}"
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
    assert!(mismatches.is_empty(), "{} formatting mismatches: {mismatches:#?}", mismatches.len());
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
        imports.import("rt", "number-box", EntityType::Function(APPENDED));
        imports.import("rt", "string-new", EntityType::Function(APPENDED + 1));
        for (i, name) in ["number", "string"].iter().enumerate() {
            functions.function(APPENDED + i as u32);
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
        functions.function(APPENDED).function(APPENDED + 1);
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
    imports.import("host", "mark", EntityType::Function(APPENDED));
    let mut functions = FunctionSection::new();
    functions.function(APPENDED);
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
    // rec(TYPE_COUNT), struct, one f64 field.
    let marker = [0x4e, runtime_abi::TYPE_COUNT as u8, 0x5f, 1, 0x7c, 0];
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
        types.ty().function([value, value], [value]); // APPENDED
        types.ty().function([value], []); // APPENDED + 1
        imports.import("rt", "invoke", EntityType::Function(APPENDED));
        imports.import(
            "rt",
            "language-exception",
            EntityType::Tag(TagType {
                kind: TagKind::Exception,
                func_type_idx: APPENDED + 1,
            }),
        );
        functions.function(APPENDED);
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
        ); // APPENDED
        types
            .ty()
            .function([value, ValType::I32, ValType::I32], [value]); // APPENDED + 1
        imports.import("rt", "closure-new", EntityType::Function(APPENDED));
        functions.function(3).function(APPENDED + 1);
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
    // Clojure's arity error (ADR-0004); a runtime-made closure is named fn.
    assert_eq!(String::from_utf16(&units).unwrap(), "Wrong number of args (0) passed to: fn");
    for field in &fields[2..4] {
        assert!(matches!(field, Val::AnyRef(None)), "expected nil");
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
        assert_eq!(sentinel_of(&mut store, &result[0]), expected);
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
        // (+) and (*) are the longs 0 and 1; double operands give doubles.
        ("add", vec![], RuntimeNumber::Long(0)),
        ("multiply", vec![], RuntimeNumber::Long(1)),
        ("add", vec![2.0], RuntimeNumber::Double(2.0)),
        ("multiply", vec![2.0], RuntimeNumber::Double(2.0)),
        ("subtract", vec![0.0], RuntimeNumber::Double(-0.0)),
        ("divide", vec![4.0], RuntimeNumber::Double(0.25)),
        ("add", vec![1.0, 2.0, 3.0], RuntimeNumber::Double(6.0)),
        ("multiply", vec![2.0, 3.0, 4.0], RuntimeNumber::Double(24.0)),
        ("subtract", vec![10.0, 2.0, 3.0], RuntimeNumber::Double(5.0)),
        ("divide", vec![24.0, 3.0, 2.0], RuntimeNumber::Double(4.0)),
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
        // A universal closure's arity range (ABI 4): fields 5 and 6.
        assert_eq!(
            object.field(&mut store, 5).unwrap().unwrap_i32(),
            if matches!(name, "add" | "multiply") {
                0
            } else {
                1
            }
        );
        assert_eq!(object.field(&mut store, 6).unwrap().unwrap_i32(), -1);
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
        match (runtime_number(&mut store, &result[0]).unwrap(), expected) {
            (Some(RuntimeNumber::Double(found)), RuntimeNumber::Double(expected)) => {
                assert_eq!(found.to_bits(), expected.to_bits(), "{name}")
            }
            (found, expected) => assert_eq!(found, Some(expected), "{name}"),
        }
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
        ("descriptor-new", vec![value, value, value], vec![value]),
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
        imports.import("rt", name, EntityType::Function(APPENDED + index));
        functions.function(APPENDED + index);
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
    functions.function(APPENDED + signatures.len() as u32);
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

#[track_caller]
fn nominal_value(store: &mut Store<()>, instance: Instance, name: &str, args: &[Val]) -> Val {
    let mut result = [Val::null_any_ref()];
    // Descriptors built here declare no mutable field names (#278) and no
    // type name (#304).
    let mut args = args.to_vec();
    while name == "descriptor-new" && args.len() < 3 {
        args.push(Val::AnyRef(Some(wasmtime::AnyRef::from_i31(&mut *store, wasmtime::I31::wrapping_u32(0)))));
    }
    instance
        .get_func(&mut *store, name)
        .unwrap()
        .call(store, &args, &mut result)
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
        sentinel_of(&mut store, &missing),
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
        sentinel_of(&mut store, &original),
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
        ("descriptor-new", vec![nil.clone(), nil.clone(), nil.clone()], true),
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
        ("descriptor-new", vec![Val::null_any_ref(), nil.clone(), nil.clone()], true),
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
        sentinel_of(&mut store, &field),
        0
    );
    let missing = nominal_value(
        &mut store,
        runtime,
        "protocol-method-get",
        &[descriptor.clone(), descriptor],
    );
    assert_eq!(
        sentinel_of(&mut store, &missing),
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
        (constructor, none.clone(), 1, "Wrong number of args (0) passed to: fn"),
        (dispatcher.clone(), none, 1, "Wrong number of args (0) passed to: fn"),
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
        sentinel_of(&mut store, &environment[0]),
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
        sentinel_of(&mut store, &result[0]),
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
fn runtime_abi_calls_foreign_closures_through_their_own_code_struct() {
    // Under ABI 4 (ADR-0004) any module can define a closure: a `$closure`
    // subtype holding its captures, and a code struct whose entries and spread
    // entry are its own functions. Here every entry returns the one capture.
    use std::borrow::Cow;
    use wasm_encoder::*;
    let value = ValType::Ref(RefType::EQREF);
    let mut types = runtime_abi::prelude();
    let capture = APPENDED;
    types.ty().subtype(&SubType {
        is_final: true,
        supertype_idx: Some(runtime_abi::CLOSURE),
        composite_type: CompositeType {
            inner: CompositeInnerType::Struct(StructType {
                fields: vec![
                    FieldType { element_type: StorageType::Val(ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::CODE) })), mutable: false },
                    FieldType { element_type: StorageType::Val(value), mutable: true },
                    FieldType { element_type: StorageType::Val(value), mutable: true },
                    FieldType { element_type: StorageType::Val(value), mutable: false },
                ]
                .into_boxed_slice(),
            }),
            shared: false,
            descriptor: None,
            describes: None,
        },
    });
    types.ty().function([value], [value]);
    let mut functions = FunctionSection::new();
    // Entries 0 to 8, the spread entry, then make.
    for k in 0..=runtime_abi::MAX_FIXED {
        functions.function(runtime_abi::CALL + k);
    }
    functions.function(runtime_abi::SPREAD).function(APPENDED + 1);
    let make_index = runtime_abi::MAX_FIXED + 2;
    let mut exports = ExportSection::new();
    exports.export("make", ExportKind::Func, make_index);
    let mut globals = GlobalSection::new();
    let mut init = vec![Instruction::RefNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq })];
    init.extend((0..=runtime_abi::MAX_FIXED + 1).map(Instruction::RefFunc));
    init.extend([Instruction::I32Const(0), Instruction::I32Const(-1), Instruction::StructNew(runtime_abi::CODE)]);
    globals.global(
        GlobalType {
            val_type: ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(runtime_abi::CODE) }),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended(init),
    );
    let mut elements = ElementSection::new();
    elements.declared(Elements::Functions(Cow::Owned((0..=runtime_abi::MAX_FIXED + 1).collect())));
    let mut code = CodeSection::new();
    for _ in 0..=runtime_abi::MAX_FIXED + 1 {
        let mut entry = Function::new([]);
        entry
            .instruction(&Instruction::LocalGet(0))
            .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(capture)))
            .instruction(&Instruction::StructGet { struct_type_index: capture, field_index: 3 })
            .instruction(&Instruction::End);
        code.function(&entry);
    }
    let mut make = Function::new([]);
    make.instruction(&Instruction::GlobalGet(0))
        .instruction(&Instruction::RefNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::RefI31)
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::StructNew(capture))
        .instruction(&Instruction::End);
    code.function(&make);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&functions)
        .section(&globals)
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
        sentinel_of(&mut store, &result[0]),
        0
    );
    // Its properties are created on first use, as for any closure.
    runtime
        .get_func(&mut store, "closure-property-set")
        .unwrap()
        .call(
            &mut store,
            &[function[0].clone(), Val::I32(0), environment[0].clone()],
            &mut result,
        )
        .unwrap();
    runtime
        .get_func(&mut store, "closure-property-get")
        .unwrap()
        .call(&mut store, &[function[0].clone(), Val::I32(0)], &mut result)
        .unwrap();
    assert!(wasmtime::Rooted::ref_eq(
        &store,
        result[0].unwrap_anyref().unwrap(),
        environment[0].unwrap_anyref().unwrap()
    )
    .unwrap());
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
        sentinel_of(&mut store, &actual),
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
    functions.function(APPENDED);
    let mut exports = wasm_encoder::ExportSection::new();
    exports.export("copy-callback", wasm_encoder::ExportKind::Func, 0);
    let mut code = wasm_encoder::CodeSection::new();
    let mut forge = wasm_encoder::Function::new([]);
    use wasm_encoder::{HeapType, Instruction};
    forge
        // A universal closure (ABI 4): the callback's code struct, no
        // properties, an unassigned UID, the new environment, the callback's
        // function and an open arity range.
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(runtime_abi::UNIVERSAL)))
        .instruction(&Instruction::StructGet {
            struct_type_index: runtime_abi::UNIVERSAL,
            field_index: 0,
        })
        .instruction(&Instruction::RefNull(HeapType::Abstract { shared: false, ty: wasm_encoder::AbstractHeapType::Eq }))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::RefI31)
        .instruction(&Instruction::LocalGet(1))
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(runtime_abi::UNIVERSAL)))
        .instruction(&Instruction::StructGet {
            struct_type_index: runtime_abi::UNIVERSAL,
            field_index: 4,
        })
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::I32Const(-1))
        .instruction(&Instruction::StructNew(runtime_abi::UNIVERSAL))
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
    let nil = Val::AnyRef(None);
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
        // A code unit is a long; a missing unit is the double NaN.
        let actual = runtime_number(&mut store, &output[0]).unwrap();
        if expected.is_nan() {
            assert!(matches!(actual, Some(RuntimeNumber::Double(value)) if value.is_nan()), "{index:?}: {actual:?}");
        } else {
            assert_eq!(actual, Some(RuntimeNumber::Long(expected as i64)), "{index:?}");
        }
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
    assert_eq!(long_of(&mut store, &value), 55296);
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
    functions.function(APPENDED);
    let mut exports = wasm_encoder::ExportSection::new();
    exports.export("copy-callback", wasm_encoder::ExportKind::Func, 0);
    let mut code = wasm_encoder::CodeSection::new();
    let mut forge = wasm_encoder::Function::new([]);
    forge
        // A universal closure (ABI 4): the callback's code struct, no
        // properties, an unassigned UID, the new environment, the callback's
        // function and an open arity range.
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(runtime_abi::UNIVERSAL)))
        .instruction(&Instruction::StructGet {
            struct_type_index: runtime_abi::UNIVERSAL,
            field_index: 0,
        })
        .instruction(&Instruction::RefNull(HeapType::Abstract { shared: false, ty: wasm_encoder::AbstractHeapType::Eq }))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::RefI31)
        .instruction(&Instruction::LocalGet(1))
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(runtime_abi::UNIVERSAL)))
        .instruction(&Instruction::StructGet {
            struct_type_index: runtime_abi::UNIVERSAL,
            field_index: 4,
        })
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::I32Const(-1))
        .instruction(&Instruction::StructNew(runtime_abi::UNIVERSAL))
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
        assert_eq!(long_of(&mut store, &value), 57343);
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
    assert_eq!(long_of(&mut store, &recovered), 57343);
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
            assert_eq!(
                long_of(&mut store, &value),
                i64::from(word.swap_bytes() as i32),
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
    assert_eq!(long_of(&mut store, &output), 0);
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
    let invalid_sentinel = forged_sentinel(&mut store, runtime, 99);
    // A null eqref is nil in ABI v3, which coerces (below); it is no longer foreign.
    for input in [opaque, invalid_sentinel] {
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
                .expect_err(&format!("accepted {input:?}"));
            assert!(error.is::<wasmtime::ThrownException>(), "{name}: {error:#}");
            assert!(!error.is::<wasmtime::Trap>());
            assert!(store.take_pending_exception().is_some());
        }
        let normalized = nominal_value(&mut store, runtime, "primitive-f64-coerce", &[nil.clone()]);
        for name in ["primitive-f64-word0", "primitive-f64-word4"] {
            let output = nominal_value(&mut store, runtime, name, &[normalized.clone()]);
            assert_eq!(long_of(&mut store, &output), 0);
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
            sentinel_of(&mut store, &missing),
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
    // ABI v3: a null eqref is nil, an ordinary storable value, not a corrupt one.
    nominal_value(
        &mut store,
        runtime,
        "native-object-own-set",
        &[owner.clone(), key.clone(), Val::null_any_ref()],
    );
    let stored = nominal_value(
        &mut store,
        runtime,
        "native-object-own-get",
        &[owner.clone(), key.clone()],
    );
    assert_eq!(sentinel_of(&mut store, &stored), 0, "nil is stored as nil");
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
    let undefined = sentinel_value(&mut store, runtime, runtime_abi::UNDEFINED);
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
        sentinel_of(&mut store, &shadow),
        6
    );
    for (owner, prototype) in [
        (child.clone(), child.clone()),
        (grandparent.clone(), child.clone()),
        (child.clone(), number),
        // ABI v3: a null eqref is nil, a valid prototype; forge a sentinel instead.
        (child.clone(), forged_sentinel(&mut store, runtime, 99)),
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
        sentinel_of(&mut store, &detached),
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
    // ABI v3: a null eqref is nil, the valid empty prototype; forge a sentinel instead.
    let forged = forged_sentinel(&mut store, runtime, 99);
    for corrupt in [owner.clone(), forged] {
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
            sentinel_of(&mut store, &result),
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
        sentinel_of(&mut store, &result),
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
        sentinel_of(&mut store, &result),
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
        sentinel_of(&mut store, &result),
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
        sentinel_of(&mut store, &present),
        4
    );
    let absent = nominal_value(
        &mut store,
        runtime,
        "object-method-invoke",
        &[methods[2].clone(), owner.clone(), one.clone()],
    );
    assert_eq!(
        sentinel_of(&mut store, &absent),
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
        sentinel_of(&mut store, &present),
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
        sentinel_value(&mut store, runtime, runtime_abi::TRUE),
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
            sentinel_value(&mut store, runtime, sentinel),
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
    // ABI v3: a null eqref is nil, a valid key and value; forge a sentinel instead.
    let forged = forged_sentinel(&mut store, runtime, 99);
    for value in [foreign.clone(), forged] {
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
        sentinel_of(&mut store, &result),
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
        sentinel_of(&mut store, &missing),
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
        sentinel_of(&mut store, &after),
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
        sentinel_of(&mut store, &result),
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
        sentinel_of(&mut store, &result),
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
        sentinel_of(&mut store, &result),
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
        sentinel_of(&mut store, &result),
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
        sentinel_of(&mut store, &detached),
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
        sentinel_of(&mut store, &no_getter),
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
        sentinel_of(&mut store, &shadow),
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
    let forged = forged_sentinel(&mut store, runtime, 99);
    // A null eqref is nil in ABI v3, a valid numeric operand; it is no longer foreign.
    for input in [opaque, forged] {
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
                .expect_err(&format!("accepted {args:?}"));
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
            assert_eq!(long_of(&mut store, &recovered), 0);
        }
        let output = nominal_value(&mut store, runtime, "primitive-f64-safe-integer", &[input]);
        assert_eq!(
            sentinel_of(&mut store, &output),
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
    for (owner, slot) in [(descriptor, 4), (object, 3), (closure, 2), (error, 4)] {
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
        let id = long_of(&mut store, &cached);
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
    assert_eq!(long_of(&mut store, &cached), LIMIT);
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
        sentinel_of(&mut store, &result),
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
    functions.function(APPENDED);
    let mut exports = wasm_encoder::ExportSection::new();
    exports.export("copy-callback", wasm_encoder::ExportKind::Func, 0);
    let mut code = wasm_encoder::CodeSection::new();
    let mut forge = wasm_encoder::Function::new([]);
    forge
        // A universal closure (ABI 4): the callback's code struct, no
        // properties, an unassigned UID, the new environment, the callback's
        // function and an open arity range.
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(runtime_abi::UNIVERSAL)))
        .instruction(&Instruction::StructGet {
            struct_type_index: runtime_abi::UNIVERSAL,
            field_index: 0,
        })
        .instruction(&Instruction::RefNull(HeapType::Abstract { shared: false, ty: wasm_encoder::AbstractHeapType::Eq }))
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::RefI31)
        .instruction(&Instruction::LocalGet(1))
        .instruction(&Instruction::LocalGet(0))
        .instruction(&Instruction::RefCastNonNull(HeapType::Concrete(runtime_abi::UNIVERSAL)))
        .instruction(&Instruction::StructGet {
            struct_type_index: runtime_abi::UNIVERSAL,
            field_index: 4,
        })
        .instruction(&Instruction::I32Const(0))
        .instruction(&Instruction::I32Const(-1))
        .instruction(&Instruction::StructNew(runtime_abi::UNIVERSAL))
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
        sentinel_of(&mut store, &result),
        2
    );
}


#[test]
fn runtime_abi_nominal_type_identity_rejects_equal_id_descriptor_copy_after_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let schema = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[schema.clone()]);
    let descriptor_ref = descriptor.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
    let fields = descriptor_ref.fields(&mut store).unwrap().collect::<Vec<_>>();
    let ty = descriptor_ref.ty(&store).unwrap();
    let allocator = wasmtime::StructRefPre::new(&mut store, ty);
    let copy = wasmtime::StructRef::new(&mut store, &allocator, &fields).unwrap();
    let a = nominal_value(&mut store, runtime, "object-new", &[descriptor.clone(), schema.clone()]);
    let b = nominal_value(&mut store, runtime, "object-new", &[descriptor, schema.clone()]);
    let impostor = nominal_value(&mut store, runtime, "object-new", &[Val::AnyRef(Some(copy.to_anyref())), schema]);
    store.gc(None).unwrap();
    let predicate = runtime.get_func(&mut store, "object-same-descriptor").expect("nominal identity export");
    for (other, expected) in [(b, 1), (impostor, 0), (Val::null_any_ref(), 0)] {
        for (left, right) in [(a.clone(), other.clone()), (other, a.clone())] {
            let mut result = [Val::I32(-1)];
            predicate.call(&mut store, &[left, right], &mut result).unwrap();
            assert_eq!(result[0].unwrap_i32(), expected);
        }
    }
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
fn runtime_abi_schema_field_character_guard_checks_every_utf16_unit() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let name = nominal_value(&mut store, runtime, "string-new", &[Val::I32(1)]);
    let set = runtime.get_func(&mut store, "string-set-unit").unwrap();
    let supported = runtime
        .get_func(&mut store, "property-field-name-supported")
        .unwrap();
    // Independent explicit whitelist, including boundaries, surrogate units and
    // non-ASCII units whose low bits could alias an ASCII lookup-table entry.
    let allowed = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_$";
    store.gc(None).unwrap();
    for unit in 0..=u16::MAX {
        set.call(
            &mut store,
            &[name.clone(), Val::I32(0), Val::I32(i32::from(unit))],
            &mut [Val::I32(0)],
        )
        .unwrap();
        let mut result = [Val::I32(0)];
        supported
            .call(&mut store, &[name.clone()], &mut result)
            .unwrap();
        let expected = allowed.iter().any(|ascii| u16::from(*ascii) == unit);
        assert_eq!(result[0].unwrap_i32(), i32::from(expected), "unit {unit:#06x}");
    }
}

#[test]
fn runtime_abi_reserved_schema_names_keep_all_spellings_and_near_misses() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let reserved = [
        "arguments",
        "abstract",
        "await",
        "boolean",
        "break",
        "byte",
        "case",
        "catch",
        "char",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "double",
        "else",
        "enum",
        "export",
        "extends",
        "final",
        "finally",
        "float",
        "for",
        "function",
        "goto",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "int",
        "interface",
        "let",
        "long",
        "native",
        "new",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "short",
        "static",
        "super",
        "switch",
        "synchronized",
        "this",
        "throw",
        "throws",
        "transient",
        "try",
        "typeof",
        "var",
        "void",
        "volatile",
        "while",
        "with",
        "yield",
        "methods",
        "null",
        "constructor",
        "__proto__",
    ];
    let empty = nominal_value(&mut store, runtime, "string-new", &[Val::I32(0)]);
    store.gc(None).unwrap();
    assert_eq!(nominal_value(&mut store, runtime, "property-field-name-supported", &[empty]).unwrap_i32(), 1);
    for spelling in reserved {
        for (candidate, expected) in [(spelling.to_owned(), 0), (format!("{spelling}_"), 1)] {
            let units = candidate.encode_utf16().collect::<Vec<_>>();
            let name = nominal_value(&mut store, runtime, "string-new", &[Val::I32(units.len() as i32)]);
            for (index, unit) in units.into_iter().enumerate() {
                runtime.get_func(&mut store, "string-set-unit").unwrap()
                    .call(&mut store, &[name.clone(), Val::I32(index as i32), Val::I32(i32::from(unit))], &mut [Val::I32(0)]).unwrap();
            }
            store.gc(None).unwrap();
            assert_eq!(nominal_value(&mut store, runtime, "property-field-name-supported", &[name]).unwrap_i32(), expected, "{candidate}");
        }
    }
}

#[test]
fn runtime_abi_munged_field_aliases_copy_last_constructor_value_and_use_last_named_slot() {
    // Authored regression; no execution claimed in the source author checkout.
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    fn key(store: &mut Store<()>, runtime: Instance) -> Val {
        let value = nominal_value(store, runtime, "string-new", &[Val::I32(3)]);
        let array = value
            .unwrap_anyref()
            .unwrap()
            .as_array(&*store)
            .unwrap()
            .unwrap();
        for (i, unit) in "x_y".encode_utf16().enumerate() {
            array
                .set(&mut *store, i as u32, Val::I32(unit as i32))
                .unwrap();
        }
        value
    }
    fn set(store: &mut Store<()>, args: &Val, index: u32, value: Val) {
        args.unwrap_anyref()
            .unwrap()
            .as_array(&*store)
            .unwrap()
            .unwrap()
            .set(store, index, value)
            .unwrap();
    }
    fn bits(store: &mut Store<()>, value: &Val) -> u64 {
        let value = value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&*store)
            .unwrap()
            .unwrap();
        let Val::F64(bits) = value.field(store, 0).unwrap() else {
            panic!("Number ABI")
        };
        bits
    }
    let first_key = key(&mut store, runtime);
    let second_key = key(&mut store, runtime);
    let schema = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    set(&mut store, &schema, 0, first_key.clone());
    set(&mut store, &schema, 1, second_key.clone());
    let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[schema.clone()]);
    let first = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(7.0f64.to_bits())],
    );
    let last = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(9.0f64.to_bits())],
    );
    let input = nominal_value(&mut store, runtime, "args-new", &[Val::I32(2)]);
    set(&mut store, &input, 0, first);
    set(&mut store, &input, 1, last);
    let object = nominal_value(
        &mut store,
        runtime,
        "object-new",
        &[descriptor, input.clone()],
    );
    store.gc(None).unwrap();
    for index in [0, 1] {
        let field = nominal_value(
            &mut store,
            runtime,
            "object-field-get",
            &[object.clone(), Val::I32(index)],
        );
        assert_eq!(bits(&mut store, &field), 9.0f64.to_bits());
    }
    let original = input
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .get(&mut store, 0)
        .unwrap();
    assert_eq!(
        bits(&mut store, &original),
        7.0f64.to_bits(),
        "constructor input was not mutated"
    );
    let replacement = nominal_value(
        &mut store,
        runtime,
        "number-box",
        &[Val::F64(17.0f64.to_bits())],
    );
    nominal_value(
        &mut store,
        runtime,
        "named-property-set",
        &[object.clone(), first_key.clone(), replacement],
    );
    store.gc(None).unwrap();
    let named = nominal_value(
        &mut store,
        runtime,
        "named-property-get",
        &[object.clone(), second_key.clone()],
    );
    let canonical = nominal_value(
        &mut store,
        runtime,
        "object-field-get",
        &[object, Val::I32(1)],
    );
    assert_eq!(bits(&mut store, &named), 17.0f64.to_bits());
    assert_eq!(bits(&mut store, &canonical), 17.0f64.to_bits());
    let table = nominal_value(&mut store, runtime, "args-new", &[Val::I32(4)]);
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    set(&mut store, &table, 0, first_key.clone());
    set(&mut store, &table, 1, nil.clone());
    set(&mut store, &table, 2, second_key);
    set(&mut store, &table, 3, nil);
    store.gc(None).unwrap();
    for (table, stride, expected) in [(schema, 1, 1), (table, 2, 0)] {
        let mut result = [Val::I32(-1)];
        runtime
            .get_func(&mut store, "property-find")
            .unwrap()
            .call(
                &mut store,
                &[table, first_key.clone(), Val::I32(stride)],
                &mut result,
            )
            .unwrap();
        assert!(matches!(result[0], Val::I32(actual) if actual == expected));
    }
}

#[test]
fn runtime_abi_native_name_normalization_keeps_exact_utf16_and_live_input_identity() {
    fn text(store: &mut Store<()>, runtime: Instance, units: &[u16]) -> Val {
        let value = nominal_value(store, runtime, "string-new", &[Val::I32(units.len() as i32)]);
        for (index, unit) in units.iter().enumerate() {
            runtime.get_func(&mut *store, "string-set-unit").unwrap().call(
                &mut *store,
                &[value.clone(), Val::I32(index as i32), Val::I32(i32::from(*unit))],
                &mut [Val::I32(0)],
            ).unwrap();
        }
        value
    }
    fn same(store: &Store<()>, input: &Val, output: &Val) {
        assert!(wasmtime::Rooted::<wasmtime::AnyRef>::ref_eq(
            store, input.unwrap_anyref().unwrap(), output.unwrap_anyref().unwrap(),
        ).unwrap(), "unrecognized names must retain the original string identity");
    }
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let normalize = runtime.get_func(&mut store, "property-native-name").unwrap();
    let setter = runtime.get_func(&mut store, "string-set-unit").unwrap();
    let mut output = [Val::null_any_ref()];
    for (kind, name) in ["null", "boolean", "number", "string", "function", "object", "array", "_"]
        .into_iter().enumerate()
    {
        let units = name.encode_utf16().collect::<Vec<_>>();
        let input = text(&mut store, runtime, &units);
        store.gc(None).unwrap();
        normalize.call(&mut store, &[input.clone()], &mut output).unwrap();
        assert_eq!(output[0].unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), kind as u32);
        for (index, unit) in units.iter().enumerate() {
            for replacement in [0, *unit ^ 0x20, *unit | 0x100, 0xd800] {
                setter.call(&mut store, &[input.clone(), Val::I32(index as i32), Val::I32(i32::from(replacement))], &mut [Val::I32(0)]).unwrap();
                if replacement == 0xd800 {
                    store.gc(None).unwrap();
                }
                normalize.call(&mut store, &[input.clone()], &mut output).unwrap();
                same(&store, &input, &output[0]);
                setter.call(&mut store, &[input.clone(), Val::I32(index as i32), Val::I32(i32::from(*unit))], &mut [Val::I32(0)]).unwrap();
                normalize.call(&mut store, &[input.clone()], &mut output).unwrap();
                assert_eq!(output[0].unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), kind as u32);
            }
        }
    }
    for units in [vec![], vec![0xd800, 0xdfff], vec![0], "Null".encode_utf16().collect(),
        "array_".encode_utf16().collect(), "string\0".encode_utf16().collect()]
    {
        let input = text(&mut store, runtime, &units);
        normalize.call(&mut store, &[input.clone()], &mut output).unwrap();
        same(&store, &input, &output[0]);
    }
    let input = text(&mut store, runtime, &[0]);
    for unit in 0..=u16::MAX {
        setter.call(&mut store, &[input.clone(), Val::I32(0), Val::I32(i32::from(unit))], &mut [Val::I32(0)]).unwrap();
        normalize.call(&mut store, &[input.clone()], &mut output).unwrap();
        if unit == u16::from(b'_') {
            assert_eq!(output[0].unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 7);
        } else {
            same(&store, &input, &output[0]);
        }
    }
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    for invalid in [nil, Val::null_any_ref()] {
        let error = normalize.call(&mut store, &[invalid], &mut output).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
    }
    let input = text(&mut store, runtime, &"number".encode_utf16().collect::<Vec<_>>());
    normalize.call(&mut store, &[input], &mut output).unwrap();
    assert_eq!(output[0].unwrap_anyref().unwrap().as_i31(&store).unwrap().unwrap().get_u32(), 2);
}

#[test]
fn runtime_abi_property_key_comparison_preserves_utf16_live_mutation_and_error_order() {
    fn text(store: &mut Store<()>, runtime: Instance, units: &[u16]) -> Val {
        let value = nominal_value(store, runtime, "string-new", &[Val::I32(units.len() as i32)]);
        let setter = runtime.get_func(&mut *store, "string-set-unit").unwrap();
        for (index, unit) in units.iter().enumerate() {
            let mut result = [Val::I32(0)];
            setter.call(&mut *store, &[value.clone(), Val::I32(index as i32), Val::I32(i32::from(*unit))], &mut result).unwrap();
            assert_eq!(result[0].unwrap_i32(), 1);
        }
        value
    }
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let equal = runtime.get_func(&mut store, "property-key-equal").unwrap();
    let mut result = [Val::I32(-1)];
    for units in [vec![], vec![0], vec![97, 98], vec![97, 98, 99], vec![97, 98, 99, 100], vec![0xd800, 0, 0xdfff, 0xffff, 97], vec![97; 9]] {
        let left = text(&mut store, runtime, &units);
        let right = text(&mut store, runtime, &units);
        let longer = text(&mut store, runtime, &[units.as_slice(), &[98]].concat());
        store.gc(None).unwrap();
        for (a, b, expected) in [(&left, &left, 1), (&left, &right, 1), (&left, &longer, 0)] {
            equal.call(&mut store, &[a.clone(), b.clone()], &mut result).unwrap();
            assert_eq!(result[0].unwrap_i32(), expected);
        }
        for (index, unit) in units.iter().enumerate() {
            runtime.get_func(&mut store, "string-set-unit").unwrap()
                .call(&mut store, &[right.clone(), Val::I32(index as i32), Val::I32(i32::from(*unit) ^ 1)], &mut result).unwrap();
            equal.call(&mut store, &[left.clone(), right.clone()], &mut result).unwrap();
            assert_eq!(result[0].unwrap_i32(), 0, "later calls must observe changed units");
            runtime.get_func(&mut store, "string-set-unit").unwrap()
                .call(&mut store, &[right.clone(), Val::I32(index as i32), Val::I32(i32::from(*unit))], &mut result).unwrap();
            equal.call(&mut store, &[left.clone(), right.clone()], &mut result).unwrap();
            assert_eq!(result[0].unwrap_i32(), 1);
        }
        let nil = nominal_value(&mut store, runtime, "nil", &[]);
        equal.call(&mut store, &[nil.clone(), nil.clone()], &mut result).unwrap();
        assert_eq!(result[0].unwrap_i32(), 0, "left type rejection precedes right validation");
        assert!(equal.call(&mut store, &[left.clone(), nil], &mut result).is_err());
        equal.call(&mut store, &[left.clone(), right], &mut result).unwrap();
        assert_eq!(result[0].unwrap_i32(), 1, "a valid call must recover after rejection");
    }
}

#[test]
fn runtime_abi_method_lookup_preserves_live_values_equal_id_keys_and_first_match_after_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &Module::new(&engine, runtime_abi::module()).unwrap(), &[]).unwrap();
    let mut linker = wasmtime::Linker::new(&engine);
    linker.instance(&mut store, "rt", runtime).unwrap();
    let producer = linker.instantiate(&mut store, &Module::new(&engine, nominal_fragment()).unwrap()).unwrap();
    let closures = linker.instantiate(&mut store, &Module::new(&engine, closure_fragment(false)).unwrap()).unwrap();
    let schema = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
    let owner = nominal_value(&mut store, producer, "descriptor-new", &[schema.clone()]);
    let key = nominal_value(&mut store, producer, "descriptor-new", &[schema.clone()]);
    let key_id = key.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap().field(&mut store, 0).unwrap().unwrap_i64();
    let other_key = nominal_value(&mut store, producer, "forge-descriptor", &[schema.clone(), schema, Val::I64(key_id)]);
    let nil = nominal_value(&mut store, runtime, "nil", &[]);
    let a = nominal_value(&mut store, closures, "make", &[nil.clone(), Val::I32(0), Val::I32(0)]);
    let b = nominal_value(&mut store, closures, "make", &[nil, Val::I32(0), Val::I32(0)]);
    let setter = runtime.get_func(&mut store, "protocol-method-set").unwrap();
    let getter = runtime.get_func(&mut store, "protocol-method-get").unwrap();
    setter.call(&mut store, &[owner.clone(), key.clone(), a.clone()], &mut []).unwrap();
    let mut result = [Val::null_any_ref()];
    getter.call(&mut store, &[owner.clone(), key.clone()], &mut result).unwrap();
    setter.call(&mut store, &[owner.clone(), other_key.clone(), b.clone()], &mut []).unwrap();
    store.gc(None).unwrap();
    for _ in 0..4 {
        for (key, expected) in [(&key, &a), (&other_key, &b)] {
            getter.call(&mut store, &[owner.clone(), key.clone()], &mut result).unwrap();
            assert!(wasmtime::Rooted::ref_eq(&store, result[0].unwrap_anyref().unwrap(), expected.unwrap_anyref().unwrap()).unwrap());
        }
    }
    // Reorder the table without changing IDs; lookup must use the live keys.
    // This also exercises replacement of the entire table reference.
    let table = nominal_value(&mut store, runtime, "args-new", &[Val::I32(4)]);
    let array = table.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    for (index, value) in [other_key.clone(), a.clone(), key.clone(), b.clone()].into_iter().enumerate() {
        array.set(&mut store, index as u32, value).unwrap();
    }
    let descriptor = owner.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
    descriptor.set_field(&mut store, 2, table.clone()).unwrap();
    store.gc(None).unwrap();
    for (key, expected) in [(&key, &b), (&other_key, &a)] {
        getter.call(&mut store, &[owner.clone(), key.clone()], &mut result).unwrap();
        assert!(wasmtime::Rooted::ref_eq(&store, result[0].unwrap_anyref().unwrap(), expected.unwrap_anyref().unwrap()).unwrap());
    }
    // Repeated lookup must read an in-place method replacement.
    array.set(&mut store, 1, b.clone()).unwrap();
    getter.call(&mut store, &[owner.clone(), other_key.clone()], &mut result).unwrap();
    assert!(wasmtime::Rooted::ref_eq(&store, result[0].unwrap_anyref().unwrap(), b.unwrap_anyref().unwrap()).unwrap());
    let odd = nominal_value(&mut store, runtime, "args-new", &[Val::I32(1)]);
    descriptor.set_field(&mut store, 2, odd).unwrap();
    assert!(getter.call(&mut store, &[owner.clone(), other_key.clone()], &mut result).is_err());
    descriptor.set_field(&mut store, 2, table).unwrap();
    getter.call(&mut store, &[owner.clone(), other_key.clone()], &mut result).unwrap();
    getter.call(&mut store, &[owner.clone(), key.clone()], &mut result).unwrap();
    // Even-length duplicate-key tables retain their existing first-match rule,
    // including after an earlier lookup returned the slot-2 method.
    for (index, value) in [key.clone(), a.clone(), key.clone(), b.clone()].into_iter().enumerate() {
        array.set(&mut store, index as u32, value).unwrap();
    }
    getter.call(&mut store, &[owner.clone(), key.clone()], &mut result).unwrap();
    assert!(wasmtime::Rooted::ref_eq(&store, result[0].unwrap_anyref().unwrap(), a.unwrap_anyref().unwrap()).unwrap());
    // Exercise the first fallback pair (slot 32) and a later duplicate at 34.
    let long = nominal_value(&mut store, runtime, "args-new", &[Val::I32(36)]);
    let long_array = long.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    for pair in 0..16 {
        let schema = nominal_value(&mut store, runtime, "args-new", &[Val::I32(0)]);
        let unrelated = nominal_value(&mut store, producer, "descriptor-new", &[schema]);
        long_array.set(&mut store, pair * 2, unrelated).unwrap();
        long_array.set(&mut store, pair * 2 + 1, a.clone()).unwrap();
    }
    for (index, value) in [key.clone(), a.clone(), key.clone(), b.clone()].into_iter().enumerate() {
        long_array.set(&mut store, 32 + index as u32, value).unwrap();
    }
    descriptor.set_field(&mut store, 2, long).unwrap();
    store.gc(None).unwrap();
    getter.call(&mut store, &[owner.clone(), key.clone()], &mut result).unwrap();
    assert!(wasmtime::Rooted::ref_eq(&store, result[0].unwrap_anyref().unwrap(), a.unwrap_anyref().unwrap()).unwrap());
    long_array.set(&mut store, 33, b.clone()).unwrap();
    getter.call(&mut store, &[owner.clone(), key.clone()], &mut result).unwrap();
    assert!(wasmtime::Rooted::ref_eq(&store, result[0].unwrap_anyref().unwrap(), b.unwrap_anyref().unwrap()).unwrap());
    // A duplicate at the final unrolled pair wins over both fallback matches.
    long_array.set(&mut store, 30, key.clone()).unwrap();
    long_array.set(&mut store, 31, a.clone()).unwrap();
    store.gc(None).unwrap();
    getter.call(&mut store, &[owner.clone(), key], &mut result).unwrap();
    assert!(wasmtime::Rooted::ref_eq(&store, result[0].unwrap_anyref().unwrap(), a.unwrap_anyref().unwrap()).unwrap());
    getter.call(&mut store, &[owner, other_key], &mut result).unwrap();
    assert_eq!(sentinel_of(&mut store, &result[0]), 0);
}

fn nominal_fueled_engine() -> wasmtime::Engine {
    static ENGINE: std::sync::OnceLock<wasmtime::Engine> = std::sync::OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = wasmtime::Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            wasmtime::Engine::new(&config).unwrap()
        })
        .clone()
}

#[test]
fn runtime_abi_nominal_constructor_duplicate_fields_follow_live_utf16_schema() {
    let engine = nominal_fueled_engine();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    fn text(store: &mut Store<()>, rt: Instance, units: &[u16]) -> Val {
        let value = nominal_value(store, rt, "string-new", &[Val::I32(units.len() as i32)]);
        for (index, &unit) in units.iter().enumerate() {
            nominal_value(
                store,
                rt,
                "string-set-unit",
                &[
                    value.clone(),
                    Val::I32(index as i32),
                    Val::I32(i32::from(unit)),
                ],
            );
        }
        value
    }
    fn array(store: &mut Store<()>, rt: Instance, values: &[Val]) -> Val {
        let value = nominal_value(store, rt, "args-new", &[Val::I32(values.len() as i32)]);
        let array = value
            .unwrap_anyref()
            .unwrap()
            .as_array(&*store)
            .unwrap()
            .unwrap();
        for (index, item) in values.iter().enumerate() {
            array.set(&mut *store, index as u32, item.clone()).unwrap();
        }
        value
    }
    fn check(
        store: &mut Store<()>,
        rt: Instance,
        descriptor: &Val,
        args: &Val,
        values: &[Val],
        expected: &[usize],
    ) {
        let object = nominal_value(store, rt, "object-new", &[descriptor.clone(), args.clone()]);
        store.gc(None).unwrap();
        for (index, &source) in expected.iter().enumerate() {
            let value = nominal_value(
                store,
                rt,
                "object-field-get",
                &[object.clone(), Val::I32(index as i32)],
            );
            assert!(
                wasmtime::Rooted::ref_eq(
                    &*store,
                    value.unwrap_anyref().unwrap(),
                    values[source].unwrap_anyref().unwrap()
                )
                .unwrap()
            );
        }
        let source = args
            .unwrap_anyref()
            .unwrap()
            .as_array(&*store)
            .unwrap()
            .unwrap();
        for (index, expected) in values.iter().enumerate() {
            let actual = source.get(&mut *store, index as u32).unwrap();
            assert!(
                wasmtime::Rooted::ref_eq(
                    &*store,
                    actual.unwrap_anyref().unwrap(),
                    expected.unwrap_anyref().unwrap()
                )
                .unwrap(),
                "duplicate normalization must not overwrite constructor arguments"
            );
        }
    }
    let cases: &[&[&str]] = &[
        &["meta", "first", "rest", "__hash"],
        &["meta", "cnt", "shift", "root", "tail", "__hash"],
        &["meta", "first", "rest", "count", "__hash"],
    ];
    for (case, names) in cases.iter().enumerate() {
        let fields = names
            .iter()
            .map(|name| {
                text(
                    &mut store,
                    runtime,
                    &name.encode_utf16().collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        let schema = array(&mut store, runtime, &fields);
        let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[schema]);
        let values = (0..fields.len())
            .map(|i| {
                Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
                    &mut store,
                    wasmtime::I31::new_u32(20 + i as u32).unwrap(),
                )))
            })
            .collect::<Vec<_>>();
        let args = array(&mut store, runtime, &values);
        check(
            &mut store,
            runtime,
            &descriptor,
            &args,
            &values,
            &(0..fields.len()).collect::<Vec<_>>(),
        );
        let before = store.get_fuel().unwrap();
        for _ in 0..1000 {
            nominal_value(
                &mut store,
                runtime,
                "object-new",
                &[descriptor.clone(), args.clone()],
            );
        }
        eprintln!(
            "nominal-constructor case={case} calls=1000 fuel={}",
            before - store.get_fuel().unwrap()
        );
        let before = store.get_fuel().unwrap();
        for _ in 0..1000 {
            for field in &fields {
                assert_eq!(
                    nominal_value(
                        &mut store,
                        runtime,
                        "property-field-name-supported",
                        &[field.clone()]
                    )
                    .unwrap_i32(),
                    1
                );
            }
        }
        eprintln!(
            "nominal-field-validation case={case} calls={} fuel={}",
            1000 * fields.len(),
            before - store.get_fuel().unwrap()
        );
    }
    // Equal strings in distinct storage, an alias, an empty name, and exact
    // unpaired-surrogate/NUL units must all use the last matching argument.
    let names = [
        vec![0xd800, 0],
        vec![],
        vec![0xd800, 0],
        vec![0xd801, 0],
        vec![],
    ];
    let mut fields = names
        .iter()
        .map(|units| text(&mut store, runtime, units))
        .collect::<Vec<_>>();
    fields.push(fields[0].clone());
    let schema = array(&mut store, runtime, &fields);
    let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[schema]);
    let values = (0..fields.len())
        .map(|i| {
            Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
                &mut store,
                wasmtime::I31::new_u32(40 + i as u32).unwrap(),
            )))
        })
        .collect::<Vec<_>>();
    let args = array(&mut store, runtime, &values);
    check(
        &mut store,
        runtime,
        &descriptor,
        &args,
        &values,
        &[5, 4, 5, 3, 4, 5],
    );
    nominal_value(
        &mut store,
        runtime,
        "string-set-unit",
        &[fields[3].clone(), Val::I32(0), Val::I32(0xd800)],
    );
    check(
        &mut store,
        runtime,
        &descriptor,
        &args,
        &values,
        &[5, 4, 5, 5, 4, 5],
    );
    // Mutate the actual descriptor schema, not the array copied by descriptor-new.
    let actual = descriptor
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 1)
        .unwrap();
    let schema = actual
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    schema.set(&mut store, 5, fields[1].clone()).unwrap();
    check(
        &mut store,
        runtime,
        &descriptor,
        &args,
        &values,
        &[3, 5, 3, 3, 5, 5],
    );
    // Nonstring schema slots are never treated as duplicate property names.
    schema.set(&mut store, 0, values[0].clone()).unwrap();
    schema.set(&mut store, 2, values[0].clone()).unwrap();
    check(
        &mut store,
        runtime,
        &descriptor,
        &args,
        &values,
        &[0, 5, 2, 3, 5, 5],
    );
}

#[test]
fn runtime_abi_protocol_lookup_preserves_order_live_tables_and_group_boundaries() {
    let engine = nominal_fueled_engine();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    fn array(store: &mut Store<()>, rt: Instance, values: &[Val]) -> Val {
        let value = nominal_value(store, rt, "args-new", &[Val::I32(values.len() as i32)]);
        let array = value
            .unwrap_anyref()
            .unwrap()
            .as_array(&*store)
            .unwrap()
            .unwrap();
        for (index, item) in values.iter().enumerate() {
            array.set(&mut *store, index as u32, item.clone()).unwrap();
        }
        value
    }
    fn install(store: &mut Store<()>, descriptor: &Val, table: &Val) {
        descriptor
            .unwrap_anyref()
            .unwrap()
            .as_struct(&*store)
            .unwrap()
            .unwrap()
            .set_field(store, 2, table.clone())
            .unwrap();
    }
    fn expect(
        store: &mut Store<()>,
        rt: Instance,
        descriptor: &Val,
        key: &Val,
        expected: Option<&Val>,
    ) {
        let value = nominal_value(
            store,
            rt,
            "protocol-method-get",
            &[descriptor.clone(), key.clone()],
        );
        if let Some(expected) = expected {
            assert!(
                wasmtime::Rooted::ref_eq(
                    &*store,
                    value.unwrap_anyref().unwrap(),
                    expected.unwrap_anyref().unwrap()
                )
                .unwrap()
            );
        } else {
            assert_eq!(
                sentinel_of(&mut *store, &value),
                0
            );
        }
    }
    let empty = array(&mut store, runtime, &[]);
    let descriptor = nominal_value(&mut store, runtime, "descriptor-new", &[empty.clone()]);
    let missing = nominal_value(&mut store, runtime, "descriptor-new", &[empty.clone()]);
    let keys = (0..33)
        .map(|_| nominal_value(&mut store, runtime, "descriptor-new", &[empty.clone()]))
        .collect::<Vec<_>>();
    let values = (0..33)
        .map(|n| {
            nominal_value(
                &mut store,
                runtime,
                "number-box",
                &[Val::F64((n as f64).to_bits())],
            )
        })
        .collect::<Vec<_>>();
    for count in (0..=20).chain([31, 32, 33]) {
        let pairs = (0..count)
            .flat_map(|i| [keys[i].clone(), values[i].clone()])
            .collect::<Vec<_>>();
        let table = array(&mut store, runtime, &pairs);
        install(&mut store, &descriptor, &table);
        store.gc(None).unwrap();
        for i in 0..count {
            expect(&mut store, runtime, &descriptor, &keys[i], Some(&values[i]));
        }
        expect(&mut store, runtime, &descriptor, &missing, None);
        let before = store.get_fuel().unwrap();
        for _ in 0..1000 {
            nominal_value(
                &mut store,
                runtime,
                "protocol-method-get",
                &[descriptor.clone(), missing.clone()],
            );
        }
        let miss_cost = before - store.get_fuel().unwrap();
        let hit_cost = if count != 0 {
            let before = store.get_fuel().unwrap();
            for _ in 0..1000 {
                nominal_value(
                    &mut store,
                    runtime,
                    "protocol-method-get",
                    &[descriptor.clone(), keys[count - 1].clone()],
                );
            }
            Some(before - store.get_fuel().unwrap())
        } else {
            None
        };
        eprintln!(
            "protocol-lookup pairs={count} calls=1000 miss-fuel={miss_cost} last-hit-fuel={hit_cost:?}"
        );
        let slots = table
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        if count >= 2 {
            slots
                .set(&mut store, (2 * count - 1) as u32, values[0].clone())
                .unwrap();
            store.gc(None).unwrap();
            expect(
                &mut store,
                runtime,
                &descriptor,
                &keys[count - 1],
                Some(&values[0]),
            );
            slots
                .set(
                    &mut store,
                    (2 * count - 1) as u32,
                    values[count - 1].clone(),
                )
                .unwrap();
            // A late duplicate never supersedes the first matching key.
            slots
                .set(&mut store, (2 * count - 2) as u32, keys[0].clone())
                .unwrap();
            expect(&mut store, runtime, &descriptor, &keys[0], Some(&values[0]));
            expect(&mut store, runtime, &descriptor, &keys[count - 1], None);
            slots
                .set(&mut store, (2 * count - 2) as u32, keys[count - 1].clone())
                .unwrap();
        }
        if count != 0 {
            slots
                .set(&mut store, (2 * count - 2) as u32, missing.clone())
                .unwrap();
            expect(
                &mut store,
                runtime,
                &descriptor,
                &missing,
                Some(&values[count - 1]),
            );
            expect(&mut store, runtime, &descriptor, &keys[count - 1], None);
            slots
                .set(&mut store, (2 * count - 2) as u32, keys[count - 1].clone())
                .unwrap();
        }
        for (first, later) in [(3, 4), (7, 8), (15, 16)] {
            if later < count {
                slots
                    .set(&mut store, (2 * later) as u32, keys[first].clone())
                    .unwrap();
                expect(
                    &mut store,
                    runtime,
                    &descriptor,
                    &keys[first],
                    Some(&values[first]),
                );
                expect(&mut store, runtime, &descriptor, &keys[later], None);
                slots
                    .set(&mut store, (2 * later) as u32, keys[later].clone())
                    .unwrap();
            }
        }
        // An odd table is rejected before even an otherwise matching first key.
        let mut odd = pairs.clone();
        odd.push(missing.clone());
        let odd = array(&mut store, runtime, &odd);
        install(&mut store, &descriptor, &odd);
        let error = runtime
            .get_func(&mut store, "protocol-method-get")
            .unwrap()
            .call(
                &mut store,
                &[descriptor.clone(), keys[0].clone()],
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
        store.gc(None).unwrap();
        install(&mut store, &descriptor, &table);
        expect(&mut store, runtime, &descriptor, &missing, None);
    }
}

#[test]
fn private_deferred_binding_reports_on_read_survives_gc_and_preserves_ordinary_cells() {
    let engine = support::engine();
    let module = Module::new(&engine, runtime_abi::module()).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let units = "Unsupported dynamic context transport".encode_utf16().collect::<Vec<_>>();
    let mut output = [Val::null_any_ref()];
    runtime.get_func(&mut store, "string-new").unwrap().call(&mut store, &[Val::I32(units.len() as i32)], &mut output).unwrap();
    let string = output[0].unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    for (index, unit) in units.iter().enumerate() { string.set(&mut store, index as u32, Val::I32(i32::from(*unit))).unwrap(); }
    runtime.get_func(&mut store, "binding-delay-error").unwrap().call(&mut store, &[output[0]], &mut output).unwrap();
    let binding = runtime.get_func(&mut store, "binding-new").unwrap();
    binding.call(&mut store, &[output[0]], &mut output).unwrap();
    let cell = output[0];
    let getter = runtime.get_func(&mut store, "binding-get").unwrap();
    let reader = runtime.get_func(&mut store, "binding-read").unwrap();
    let snapshot = nominal_value(&mut store, runtime, "binding-snapshot", &[cell]);
    for get in [getter, reader] {
        store.gc(None).unwrap();
        let saved = nominal_value(&mut store, runtime, "binding-snapshot", &[cell]);
        assert!(wasmtime::Rooted::ref_eq(&store, snapshot.unwrap_anyref().unwrap(), saved.unwrap_anyref().unwrap()).unwrap());
        let error = get.call(&mut store, &[cell], &mut output).unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>());
        let exception = store.take_pending_exception().unwrap();
        let payload = exception.field(&mut store, 0).unwrap();
        let fields = payload.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
        let message = fields.field(&mut store, 1).unwrap().unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
        let observed = (0..message.len(&store).unwrap()).map(|i| message.get(&mut store, i).unwrap().unwrap_i32() as u16).collect::<Vec<_>>();
        assert_eq!(observed, units);
    }
    let value = Val::AnyRef(Some(wasmtime::AnyRef::from_i31(&mut store, wasmtime::I31::wrapping_u32(17))));
    binding.call(&mut store, &[value], &mut output).unwrap();
    let inner = output[0];
    binding.call(&mut store, &[inner], &mut output).unwrap();
    getter.call(&mut store, &[output[0]], &mut output).unwrap();
    assert!(wasmtime::Rooted::ref_eq(&store, inner.unwrap_anyref().unwrap(), output[0].unwrap_anyref().unwrap()).unwrap());
}


#[test]
fn unbound_dynamic_snapshots_preserve_root_state_and_nested_selected_values() {
    fn push(store: &mut Store<()>, runtime: Instance, cell: Val, value: Val) -> Val {
        let old = nominal_value(store, runtime, "binding-snapshot", &[cell]);
        let entries = nominal_value(store, runtime, "args-new", &[Val::I32(3)]);
        let array = entries.unwrap_anyref().unwrap().as_array(&*store).unwrap().unwrap();
        for (index, value) in [cell, old, value].into_iter().enumerate() {
            array.set(&mut *store, index as u32, value).unwrap();
        }
        nominal_value(store, runtime, "dynamic-push", &[entries])
    }
    fn pop(store: &mut Store<()>, runtime: Instance, frame: Val) {
        runtime.get_func(&mut *store, "dynamic-pop").unwrap().call(store, &[frame], &mut []).unwrap();
    }
    fn strict_unbound(store: &mut Store<()>, runtime: Instance, cell: Val) {
        for name in ["binding-get", "binding-get-unforced"] {
            let error = runtime.get_func(&mut *store, name).unwrap()
                .call(&mut *store, &[cell], &mut [Val::null_any_ref()]).unwrap_err();
            assert!(error.is::<wasmtime::ThrownException>());
            assert!(store.take_pending_exception().is_some());
        }
        for name in ["binding-snapshot", "binding-read"] {
            let value = nominal_value(store, runtime, name, &[cell]);
            assert_eq!(sentinel_of(store, &value), runtime_abi::UNDEFINED as u32);
        }
        let bound = nominal_value(store, runtime, "binding-bound", &[cell]);
        assert_eq!(sentinel_of(store, &bound), runtime_abi::FALSE as u32);
    }
    let engine = support::engine();
    let module = Module::new(&engine, runtime_abi::module()).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let cell = nominal_value(&mut store, runtime, "binding-unbound", &[]);
    strict_unbound(&mut store, runtime, cell);
    for (outer, inner) in [(runtime_abi::NIL, runtime_abi::FALSE), (runtime_abi::FALSE, runtime_abi::NIL)] {
        let value = sentinel_value(&mut store, runtime, outer);
        let frame = push(&mut store, runtime, cell, value);
        for name in ["binding-get", "binding-read", "binding-snapshot"] {
            let value = nominal_value(&mut store, runtime, name, &[cell]);
            assert_eq!(sentinel_of(&mut store, &value), outer as u32);
        }
        let value = sentinel_value(&mut store, runtime, inner);
        let nested = push(&mut store, runtime, cell, value);
        store.gc(None).unwrap();
        let value = nominal_value(&mut store, runtime, "binding-get", &[cell]);
        assert_eq!(sentinel_of(&mut store, &value), inner as u32);
        pop(&mut store, runtime, nested);
        let value = nominal_value(&mut store, runtime, "binding-read", &[cell]);
        assert_eq!(sentinel_of(&mut store, &value), outer as u32);
        pop(&mut store, runtime, frame);
        strict_unbound(&mut store, runtime, cell);
    }
    // An unrelated parent frame must not make the unbound root eligible for a
    // restore write either; lookup has to select the actual matching cell.
    let other = nominal_value(&mut store, runtime, "binding-new", &[Val::null_any_ref()]);
    let parent = push(&mut store, runtime, other, Val::null_any_ref());
    let nested = push(&mut store, runtime, cell, Val::null_any_ref());
    pop(&mut store, runtime, nested);
    strict_unbound(&mut store, runtime, cell);
    pop(&mut store, runtime, parent);
    strict_unbound(&mut store, runtime, cell);
}
