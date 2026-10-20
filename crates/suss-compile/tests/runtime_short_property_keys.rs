use suss_compile::runtime_abi;
use wasmtime::{Config, Engine, Instance, Module, Store, Val};

fn call(s: &mut Store<()>, r: Instance, name: &str, args: &[Val]) -> Val {
    let mut out = [Val::null_any_ref()];
    r.get_func(&mut *s, name)
        .unwrap()
        .call(&mut *s, args, &mut out)
        .unwrap();
    out[0].clone()
}
fn key(s: &mut Store<()>, r: Instance, units: &[u16]) -> Val {
    let v = call(s, r, "string-new", &[Val::I32(units.len() as i32)]);
    let a = v.unwrap_anyref().unwrap().as_array(&*s).unwrap().unwrap();
    for (i, u) in units.iter().enumerate() {
        a.set(&mut *s, i as u32, Val::I32(i32::from(*u))).unwrap();
    }
    v
}
fn array(s: &mut Store<()>, r: Instance, values: &[Val]) -> Val {
    let v = call(s, r, "args-new", &[Val::I32(values.len() as i32)]);
    let a = v.unwrap_anyref().unwrap().as_array(&*s).unwrap().unwrap();
    for (i, x) in values.iter().enumerate() {
        a.set(&mut *s, i as u32, x.clone()).unwrap();
    }
    v
}
fn bits(units: &[u16]) -> i64 {
    units
        .iter()
        .enumerate()
        .fold(0u64, |v, (i, u)| v | (u64::from(*u) << (16 * i))) as i64
}
fn setup() -> (Store<()>, Instance) {
    let mut c = Config::new();
    c.consume_fuel(true)
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let e = Engine::new(&c).unwrap();
    let mut s = Store::new(&e, ());
    s.set_fuel(10_000_000).unwrap();
    let r = Instance::new(
        &mut s,
        &Module::new(&e, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    (s, r)
}
fn language_error(s: &mut Store<()>, r: Instance, name: &str, args: &[Val]) {
    let mut out = if name == "short-literal-property-probe" {
        vec![Val::I32(0), Val::null_any_ref()]
    } else {
        vec![Val::null_any_ref()]
    };
    let e = r
        .get_func(&mut *s, name)
        .unwrap()
        .call(&mut *s, args, &mut out)
        .unwrap_err();
    assert!(e.is::<wasmtime::ThrownException>(), "{name}: {e:#}");
    assert!(!e.is::<wasmtime::Trap>());
    let ex = s.take_pending_exception().unwrap();
    let tag = ex.tag(&mut *s).unwrap();
    let expected = r.get_tag(&mut *s, "language-exception").unwrap();
    assert!(wasmtime::Tag::eq(&tag, &expected, &*s));
}
fn number(s: &mut Store<()>, v: &Val) -> f64 {
    v.unwrap_anyref()
        .unwrap()
        .as_struct(&*s)
        .unwrap()
        .unwrap()
        .field(&mut *s, 0)
        .unwrap()
        .unwrap_f64()
}
#[test]
fn packed_helper_reads_live_last_slot_validates_later_schema_and_measures_exact_calls() {
    let (mut s, r) = setup();
    let ns = key(&mut s, r, &[110, 115]);
    let x = key(&mut s, r, &[120]);
    let schema = array(&mut s, r, &[ns.clone(), x.clone(), ns.clone()]);
    let a = call(&mut s, r, "number-box", &[Val::F64(7f64.to_bits())]);
    let b = call(&mut s, r, "number-box", &[Val::F64(9f64.to_bits())]);
    let payload = array(&mut s, r, &[a.clone(), a.clone(), b.clone()]);
    let descriptor = call(&mut s, r, "descriptor-new", &[schema.clone()]);
    let owner = call(&mut s, r, "object-new", &[descriptor, payload]);
    let packed = [owner.clone(), Val::I64(bits(&[110, 115])), Val::I32(2)];
    let ordinary = [owner.clone(), ns.clone()];
    // Only the exported function.call is inside each fuel interval: argument
    // setup and all return decoding/GC/printing are outside it.
    let old = r.get_func(&mut s, "named-property-get").unwrap();
    let new = r.get_func(&mut s, "short-literal-property-get").unwrap();
    let mut out = [Val::null_any_ref()];
    let before = s.get_fuel().unwrap();
    old.call(&mut s, &ordinary, &mut out).unwrap();
    let old_cost = before - s.get_fuel().unwrap();
    assert_eq!(number(&mut s, &out[0]), 9.0);
    let before = s.get_fuel().unwrap();
    new.call(&mut s, &packed, &mut out).unwrap();
    let new_cost = before - s.get_fuel().unwrap();
    assert_eq!(number(&mut s, &out[0]), 9.0);
    eprintln!("short-key nominal-hit existing={old_cost} packed={new_cost}");
    assert!(
        new_cost < old_cost,
        "actual nominal own-field hit should remove allocation/scan work"
    );
    // descriptor-new copies the input schema and object-new normalizes equal
    // names to the final constructor value. Mutate actual retained storage,
    // rather than the original input arrays, to distinguish the live slots.
    let object = owner
        .unwrap_anyref()
        .unwrap()
        .as_struct(&s)
        .unwrap()
        .unwrap();
    let physical_descriptor = object.field(&mut s, 0).unwrap();
    let physical_descriptor = physical_descriptor
        .unwrap_anyref()
        .unwrap()
        .as_struct(&s)
        .unwrap()
        .unwrap();
    let physical_schema = physical_descriptor.field(&mut s, 1).unwrap();
    let table = physical_schema
        .unwrap_anyref()
        .unwrap()
        .as_array(&s)
        .unwrap()
        .unwrap();
    let physical_payload = object.field(&mut s, 1).unwrap();
    let slots = physical_payload
        .unwrap_anyref()
        .unwrap()
        .as_array(&s)
        .unwrap()
        .unwrap();
    slots.set(&mut s, 0, a.clone()).unwrap();
    // Before schema mutation, slot2 remains the final equal-name winner.
    for (name, args) in [
        ("named-property-get", &ordinary[..]),
        ("short-literal-property-get", &packed[..]),
    ] {
        let live = call(&mut s, r, name, args);
        assert_eq!(number(&mut s, &live), 9.0);
    }
    table.set(&mut s, 2, x.clone()).unwrap();
    // Both routes observe slot0 once the later spelling no longer matches,
    // before and after GC. Keep the independently expected value7.
    for after_gc in [false, true] {
        if after_gc {
            s.gc(None).unwrap();
        }
        for (name, args) in [
            ("named-property-get", &ordinary[..]),
            ("short-literal-property-get", &packed[..]),
        ] {
            let live = call(&mut s, r, name, args);
            assert_eq!(number(&mut s, &live), 7.0, "{name}, after_gc={after_gc}");
        }
    }
    // An early match cannot hide any malformed later entry.
    for bad in [a.clone(), key(&mut s, r, &[100, 111])] {
        table.set(&mut s, 2, bad).unwrap();
        language_error(&mut s, r, "named-property-get", &ordinary);
        language_error(&mut s, r, "short-literal-property-get", &packed);
    }
    table.set(&mut s, 2, x).unwrap();
    for len in [-1, 5, i32::MAX] {
        language_error(
            &mut s,
            r,
            "short-literal-property-get",
            &[owner.clone(), Val::I64(0), Val::I32(len)],
        );
    }
}
#[test]
fn packed_raw_utf16_fallback_preserves_zero_surrogates_and_primitive_errors() {
    let (mut s, r) = setup();
    let owner = key(&mut s, r, &[65, 0xd800]);
    for units in [
        vec![],
        vec![0],
        vec![0xd800],
        vec![0, 0xdfff, 0, 0xd800],
        vec![110, 115],
    ] {
        let k = key(&mut s, r, &units);
        let ordinary = [owner.clone(), k];
        let packed = [
            owner.clone(),
            Val::I64(bits(&units)),
            Val::I32(units.len() as i32),
        ];
        let old = r.get_func(&mut s, "named-property-get").unwrap();
        let new = r.get_func(&mut s, "short-literal-property-get").unwrap();
        let mut a = [Val::null_any_ref()];
        let mut b = [Val::null_any_ref()];
        let before = s.get_fuel().unwrap();
        old.call(&mut s, &ordinary, &mut a).unwrap();
        let old_cost = before - s.get_fuel().unwrap();
        let before = s.get_fuel().unwrap();
        new.call(&mut s, &packed, &mut b).unwrap();
        let new_cost = before - s.get_fuel().unwrap();
        // Both unknown string properties are the same internal undefined, not
        // source nil. Raw zeros/surrogates must survive reconstruction exactly.
        let a = a[0]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&s)
            .unwrap()
            .unwrap()
            .get_i32();
        let b = b[0]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&s)
            .unwrap()
            .unwrap()
            .get_i32();
        assert_eq!(a, b);
        assert_ne!(a, 0);
        eprintln!("short-key fallback units={units:?} existing={old_cost} packed={new_cost} (no savings assertion)");
    }
    // Native own-property keys admit raw UTF16. Distinct values for every
    // spelling prove fallback reconstructs units, rather than merely returning
    // the same undefined result for all malformed/colliding query spellings.
    let native = call(&mut s, r, "native-object-new", &[]);
    let spellings = [
        vec![],
        vec![0],
        vec![0xd800],
        vec![0, 0xdfff, 0, 0xd800],
        vec![110, 115],
    ];
    for (i, units) in spellings.iter().enumerate() {
        let k = key(&mut s, r, units);
        let value = call(
            &mut s,
            r,
            "number-box",
            &[Val::F64(((i + 41) as f64).to_bits())],
        );
        call(&mut s, r, "named-property-set", &[native.clone(), k, value]);
    }
    s.gc(None).unwrap();
    for (i, units) in spellings.iter().enumerate() {
        let k = key(&mut s, r, units);
        let ordinary = call(&mut s, r, "named-property-get", &[native.clone(), k]);
        let packed = call(
            &mut s,
            r,
            "short-literal-property-get",
            &[
                native.clone(),
                Val::I64(bits(units)),
                Val::I32(units.len() as i32),
            ],
        );
        assert_eq!(number(&mut s, &ordinary), (i + 41) as f64);
        assert_eq!(number(&mut s, &packed), (i + 41) as f64);
    }
    let nil = call(&mut s, r, "nil", &[]);
    let k = key(&mut s, r, &[110, 115]);
    language_error(&mut s, r, "named-property-get", &[nil.clone(), k]);
    language_error(
        &mut s,
        r,
        "short-literal-property-get",
        &[nil, Val::I64(bits(&[110, 115])), Val::I32(2)],
    );
}

#[test]
fn packed_probe_distinguishes_nil_false_hits_from_excluded_and_nominal_misses() {
    let (mut s, r) = setup();
    let ns = key(&mut s, r, &[110, 115]);
    let x = key(&mut s, r, &[120]);
    let nil = call(&mut s, r, "nil", &[]);
    let false_value = call(&mut s, r, "false", &[]);
    let schema = array(&mut s, r, &[ns.clone(), x.clone(), ns.clone()]);
    let payload = array(&mut s, r, &[false_value.clone(), nil.clone(), nil.clone()]);
    let descriptor = call(&mut s, r, "descriptor-new", &[schema]);
    let owner = call(&mut s, r, "object-new", &[descriptor, payload]);
    let probe = r.get_func(&mut s, "short-literal-property-probe").unwrap();
    let args = [owner.clone(), Val::I64(bits(&[110, 115])), Val::I32(2)];
    let mut out = [Val::I32(-1), Val::null_any_ref()];
    let before = s.get_fuel().unwrap();
    probe.call(&mut s, &args, &mut out).unwrap();
    let cost = before - s.get_fuel().unwrap();
    eprintln!("short-key explicit probe nil-hit fuel={cost}");
    assert_eq!(out[0].unwrap_i32(), 1);
    assert_eq!(
        out[1]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&s)
            .unwrap()
            .unwrap()
            .get_i32(),
        0
    );
    let object = owner
        .unwrap_anyref()
        .unwrap()
        .as_struct(&s)
        .unwrap()
        .unwrap();
    let physical = object.field(&mut s, 1).unwrap();
    let physical = physical
        .unwrap_anyref()
        .unwrap()
        .as_array(&s)
        .unwrap()
        .unwrap();
    physical.set(&mut s, 2, false_value).unwrap();
    s.gc(None).unwrap();
    probe.call(&mut s, &args, &mut out).unwrap();
    assert_eq!(out[0].unwrap_i32(), 1);
    assert_eq!(
        out[1]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&s)
            .unwrap()
            .unwrap()
            .get_i32(),
        2
    );
    let descriptor = object.field(&mut s, 0).unwrap();
    let descriptor = descriptor
        .unwrap_anyref()
        .unwrap()
        .as_struct(&s)
        .unwrap()
        .unwrap();
    let schema = descriptor.field(&mut s, 1).unwrap();
    let schema = schema
        .unwrap_anyref()
        .unwrap()
        .as_array(&s)
        .unwrap()
        .unwrap();
    schema.set(&mut s, 2, x).unwrap();
    // Last-winner semantics stay live; slot0 was normalized to nil at creation.
    probe.call(&mut s, &args, &mut out).unwrap();
    assert_eq!(out[0].unwrap_i32(), 1);
    assert_eq!(
        out[1]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&s)
            .unwrap()
            .unwrap()
            .get_i32(),
        0
    );
    let native = call(&mut s, r, "native-object-new", &[]);
    let string = key(&mut s, r, &[65]);
    for (receiver, query) in [
        (owner.clone(), vec![122]),
        (native, vec![110, 115]),
        (string, vec![110, 115]),
        (nil, vec![110, 115]),
    ] {
        probe
            .call(
                &mut s,
                &[
                    receiver,
                    Val::I64(bits(&query)),
                    Val::I32(query.len() as i32),
                ],
                &mut out,
            )
            .unwrap();
        assert_eq!(out[0].unwrap_i32(), 0);
        assert_eq!(
            out[1]
                .unwrap_anyref()
                .unwrap()
                .as_i31(&s)
                .unwrap()
                .unwrap()
                .get_i32(),
            0
        );
    }
    for len in [-1, 5] {
        language_error(
            &mut s,
            r,
            "short-literal-property-probe",
            &[owner.clone(), Val::I64(0), Val::I32(len)],
        );
    }
    let bad = call(&mut s, r, "number-box", &[Val::F64(7f64.to_bits())]);
    schema.set(&mut s, 2, bad).unwrap();
    language_error(&mut s, r, "short-literal-property-probe", &args);
}
