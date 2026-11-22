//! Execute production GC/helper/dispatch multiplication and inspect owned storage.
mod support;
use suss_compile::runtime_abi;
use wasmtime::{Instance, Module, Store, Val};

fn call<T>(store: &mut Store<T>, runtime: &Instance, name: &str, args: &[Val]) -> Val {
    let mut output = [Val::null_any_ref()];
    runtime.get_func(&mut *store, name).unwrap()
        .call(&mut *store, args, &mut output).unwrap();
    output[0].clone()
}
fn integer<T>(store: &mut Store<T>, runtime: &Instance, words: &[u32], sign: i32) -> Val {
    // Poison unused capacity: the bridge must copy logical limbs, not capacity.
    let value = call(store, runtime, "bigint-limbs-new", &[Val::I32(words.len() as i32 + 2)]);
    let array = value.unwrap_anyref().unwrap().as_array(&*store).unwrap().unwrap();
    for (i, &word) in words.iter().chain([0xdeadbeef, 0xdeadbeef].iter()).enumerate() {
        array.set(&mut *store, i as u32, Val::I32(word as i32)).unwrap();
    }
    call(store, runtime, "bigint-new", &[Val::I32(sign), value, Val::I32(words.len() as i32)])
}
fn storage<T>(store: &mut Store<T>, value: &Val) -> (i32, usize, Vec<u32>, i32) {
    let object = value.unwrap_anyref().unwrap().as_struct(&*store).unwrap().unwrap();
    let sign = object.field(&mut *store, 0).unwrap().unwrap_i32();
    let value = object.field(&mut *store, 1).unwrap();
    let array = value.unwrap_anyref().unwrap().as_array(&*store).unwrap().unwrap();
    let words = array.elems(&mut *store).unwrap().map(|v| v.unwrap_i32() as u32).collect();
    let used = object.field(&mut *store, 2).unwrap().unwrap_i32() as usize;
    let display = object.field(&mut *store, 3).unwrap().unwrap_i32();
    (sign, used, words, display)
}
fn words(value: &serde_json::Value) -> Vec<u32> {
    value.as_array().unwrap().iter().map(|v| u32::try_from(v.as_u64().unwrap()).unwrap()).collect()
}

#[test]
fn production_bigint_multiply_paths_match_primary_limbs_and_keep_owned_values() {
    let engine = support::engine();
    let module = Module::new(&engine, runtime_abi::module()).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let cases: serde_json::Value = serde_json::from_str(include_str!("../../../results/m12-helper-multiply/cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let (a, b, expected) = (words(&case["a"]), words(&case["b"]), words(&case["product"]));
        for (sa, sb) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
            let a = integer(&mut store, &runtime, &a, sa);
            let b = integer(&mut store, &runtime, &b, sb);
            let (before_a, before_b) = (storage(&mut store, &a), storage(&mut store, &b));
            for name in ["bigint-multiply-gc", "bigint-multiply-helper", "bigint-multiply"] {
                let product = call(&mut store, &runtime, name, &[a.clone(), b.clone()]);
                // A subsequent formatter overwrites shared scratch before GC.
                let number = call(&mut store, &runtime, "number-box", &[Val::F64(1.5f64.to_bits())]);
                let text = call(&mut store, &runtime, "coerce-string", &[number]);
                let text = text.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
                assert_eq!(text.elems(&mut store).unwrap().map(|v| v.unwrap_i32() as u16).collect::<Vec<_>>(), vec![49, 46, 53]);
                store.gc(None).unwrap();
                let mut padded = expected.clone();
                padded.resize(before_a.1 + before_b.1, 0);
                assert_eq!(storage(&mut store, &product), (if expected.is_empty() { 0 } else { sa * sb }, expected.len(), padded, 0), "{name}: {}/{} limbs, signs {sa}/{sb}", before_a.1, before_b.1);
                assert_eq!(storage(&mut store, &a), before_a, "left input mutated");
                assert_eq!(storage(&mut store, &b), before_b, "right input mutated");
            }
        }
    }
}

#[test]
fn helper_scratch_growth_failure_is_a_language_error_and_preserves_inputs() {
    let engine = support::engine();
    let module = Module::new(&engine, runtime_abi::module()).unwrap();
    let minimum = module.exports().find_map(|export| match export.ty() {
        wasmtime::ExternType::Memory(memory) => Some(memory.minimum() as usize * 65536),
        _ => None,
    }).unwrap();
    let limits = wasmtime::StoreLimitsBuilder::new().memory_size(minimum).build();
    let mut store = Store::new(&engine, limits);
    store.limiter(|limits| limits);
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let a = integer(&mut store, &runtime, &[u32::MAX; 4], 1);
    let before = storage(&mut store, &a);
    let mut output = [Val::null_any_ref()];
    let error = runtime.get_func(&mut store, "bigint-multiply").unwrap()
        .call(&mut store, &[a.clone(), a.clone()], &mut output).unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
    let exception = store.take_pending_exception().unwrap();
    let value = exception.field(&mut store, 0).unwrap();
    let value = value.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
    let message = value.field(&mut store, 1).unwrap();
    let message = message.unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
    let units = message.elems(&mut store).unwrap().map(|v| v.unwrap_i32() as u16).collect::<Vec<_>>();
    assert_eq!(String::from_utf16(&units).unwrap(), "Numeric conversion scratch allocation failed");
    assert_eq!(storage(&mut store, &a), before);
}
