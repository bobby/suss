//! Execute future GC storage only; no scheduler/source futures/WIT acceptance.
mod support;
use suss_compile::runtime_abi;
use wasmtime::{
    AsContextMut, Instance, Module, RootScope, Rooted, Store, StructRef, StructRefPre, Val,
};

struct Harness {
    store: Store<()>,
    runtime: Instance,
}
impl Harness {
    fn new() -> Self {
        let engine = support::engine();
        Self::with_engine(&engine, None)
    }
    fn with_engine(engine: &wasmtime::Engine, initialization_fuel: Option<u64>) -> Self {
        let bytes = runtime_abi::module();
        runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
        let module = Module::new(engine, bytes).unwrap();
        let mut store = Store::new(engine, ());
        if let Some(fuel) = initialization_fuel {
            // Fuel-enabled engines need a bounded budget even for instantiation
            // GC constant expressions. Each transition trial sets its own budget.
            store.set_fuel(fuel).unwrap();
        }
        let runtime = Instance::new(&mut store, &module, &[]).unwrap();
        Self { store, runtime }
    }
    fn value(&mut self, name: &str, args: &[Val]) -> Val {
        let mut result = [Val::null_any_ref()];
        self.runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut result)
            .unwrap();
        result[0].clone()
    }
    fn integer(&mut self, name: &str, args: &[Val]) -> i32 {
        let mut result = [Val::I32(-1)];
        self.runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut result)
            .unwrap();
        result[0].unwrap_i32()
    }
    fn fields(&mut self, object: &Val) -> Vec<Val> {
        object
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap()
            .fields(&mut self.store)
            .unwrap()
            .collect()
    }
    fn slots(&mut self, object: &Val) -> Vec<Val> {
        let fields = self.fields(object);
        let outer = fields[1]
            .unwrap_anyref()
            .unwrap()
            .as_array(&self.store)
            .unwrap()
            .unwrap();
        assert_eq!(outer.len(&self.store).unwrap(), 1);
        outer
            .get(&mut self.store, 0)
            .unwrap()
            .unwrap_anyref()
            .unwrap()
            .as_array(&self.store)
            .unwrap()
            .unwrap()
            .elems(&mut self.store)
            .unwrap()
            .collect()
    }
    fn forge(&mut self, exemplar: &Val, fields: &[Val]) -> Val {
        let ty = exemplar
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap()
            .ty(&self.store)
            .unwrap();
        let pre = StructRefPre::new(&mut self.store, ty);
        Val::AnyRef(Some(
            StructRef::new(&mut self.store, &pre, fields)
                .unwrap()
                .to_anyref(),
        ))
    }
    fn same(&self, left: &Val, right: &Val) -> bool {
        Rooted::ref_eq(
            &self.store,
            left.unwrap_anyref().unwrap(),
            right.unwrap_anyref().unwrap(),
        )
        .unwrap()
    }
    fn rejected(&mut self, name: &str, args: &[Val]) {
        let error = self
            .runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>(), "{error:#}");
        let exception = self.store.take_pending_exception().unwrap();
        let tag = exception.tag(&mut self.store).unwrap();
        let expected = self
            .runtime
            .get_tag(&mut self.store, "language-exception")
            .unwrap();
        assert!(wasmtime::Tag::eq(&tag, &expected, &self.store));
        let payload = exception.field(&mut self.store, 0).unwrap();
        assert_eq!(self.integer("language-error-is", &[payload.clone()]), 1);
        let fields = self.fields(&payload);
        let units: Vec<_> = fields[1]
            .unwrap_anyref()
            .unwrap()
            .as_array(&self.store)
            .unwrap()
            .unwrap()
            .elems(&mut self.store)
            .unwrap()
            .map(|unit| unit.unwrap_i32() as u16)
            .collect();
        assert_eq!(
            units,
            "Invalid future operation"
                .encode_utf16()
                .collect::<Vec<_>>()
        );
        assert!(!self.store.has_pending_exception());
    }
}

#[test]
fn future_storage_has_nominal_identity_and_distinct_terminal_states() {
    let mut h = Harness::new();
    let pending = h.value("future-pending-new", &[]);
    let nil = h.value("nil", &[]);
    let fields = h.fields(&pending);
    assert_eq!(fields.len(), 4, "existing ABI2 object shape");
    let descriptor_fields = h.fields(&fields[0]);
    assert_eq!(descriptor_fields.len(), 5, "existing ABI2 descriptor shape");
    let schema = descriptor_fields[1]
        .unwrap_anyref()
        .unwrap()
        .as_array(&h.store)
        .unwrap()
        .unwrap();
    assert_eq!(schema.len(&h.store).unwrap(), 1);
    let slots = h.slots(&pending);
    assert_eq!(slots.len(), 2);
    assert_eq!(
        slots[0]
            .unwrap_anyref()
            .unwrap()
            .as_i31(&h.store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0
    );
    assert!(h.same(&slots[1], &nil));
    assert_eq!(h.integer("future-is", &[pending.clone()]), 1);
    assert_eq!(h.integer("future-status", &[pending.clone()]), 0);
    h.rejected("future-result", &[pending.clone()]);
    assert_eq!(h.integer("future-status", &[pending]), 0);
    for (event, status) in [
        ("future-resolve", 1),
        ("future-reject", 2),
        ("future-cancel", 3),
    ] {
        let future = h.value("future-pending-new", &[]);
        let args = if status == 3 {
            vec![future.clone()]
        } else {
            vec![future.clone(), nil.clone()]
        };
        assert_eq!(h.integer(event, &args), 1);
        h.store.gc(None).unwrap();
        assert_eq!(h.integer("future-status", &[future.clone()]), status);
        if status == 3 {
            h.rejected("future-result", &[future]);
        } else {
            let result = h.value("future-result", &[future]);
            assert!(h.same(&result, &nil));
        }
    }
}

#[test]
fn every_terminal_event_pair_is_first_terminal_wins_without_overwrite() {
    let mut h = Harness::new();
    let first = h.value("number-box", &[Val::F64(17.0f64.to_bits())]);
    let second = h.value("number-box", &[Val::F64(99.0f64.to_bits())]);
    let nil = h.value("nil", &[]);
    for (winner, status) in [
        ("future-resolve", 1),
        ("future-reject", 2),
        ("future-cancel", 3),
    ] {
        for loser in ["future-resolve", "future-reject", "future-cancel"] {
            let future = h.value("future-pending-new", &[]);
            let args = if winner == "future-cancel" {
                vec![future.clone()]
            } else {
                vec![future.clone(), first.clone()]
            };
            assert_eq!(h.integer(winner, &args), 1);
            let args = if loser == "future-cancel" {
                vec![future.clone()]
            } else {
                vec![future.clone(), second.clone()]
            };
            assert_eq!(h.integer(loser, &args), 0);
            h.store.gc(None).unwrap();
            assert_eq!(h.integer("future-status", &[future.clone()]), status);
            let slots = h.slots(&future);
            assert!(h.same(&slots[1], if status == 3 { &nil } else { &first }));
        }
    }
}

#[test]
fn future_alone_retains_terminal_payload_across_gc_without_flattening_or_error_inference() {
    let mut h = Harness::new();
    for (event, status) in [("future-resolve", 1), ("future-reject", 2)] {
        let future = h.value("future-pending-new", &[]);
        {
            let mut scope = RootScope::new(&mut h.store);
            let mut number = [Val::null_any_ref()];
            h.runtime
                .get_func(&mut scope, "number-box")
                .unwrap()
                .call(&mut scope, &[Val::F64((-0.0f64).to_bits())], &mut number)
                .unwrap();
            let mut text = [Val::null_any_ref()];
            h.runtime
                .get_func(&mut scope, "string-new")
                .unwrap()
                .call(&mut scope, &[Val::I32(3)], &mut text)
                .unwrap();
            let array = text[0]
                .unwrap_anyref()
                .unwrap()
                .as_array(&scope)
                .unwrap()
                .unwrap();
            for (index, unit) in [0xd800, 0, 0xdc00].into_iter().enumerate() {
                array.set(&mut scope, index as u32, Val::I32(unit)).unwrap();
            }
            let mut payload = [Val::null_any_ref()];
            h.runtime
                .get_func(&mut scope, "args-new")
                .unwrap()
                .call(&mut scope, &[Val::I32(2)], &mut payload)
                .unwrap();
            let array = payload[0]
                .unwrap_anyref()
                .unwrap()
                .as_array(&scope)
                .unwrap()
                .unwrap();
            array.set(&mut scope, 0, number[0].clone()).unwrap();
            array.set(&mut scope, 1, text[0].clone()).unwrap();
            let mut result = [Val::I32(0)];
            h.runtime
                .get_func(&mut scope, event)
                .unwrap()
                .call(
                    &mut scope,
                    &[future.clone(), payload[0].clone()],
                    &mut result,
                )
                .unwrap();
            assert_eq!(result[0].unwrap_i32(), 1);
        }
        // The payload and children have no independent caller roots now.
        for _ in 0..3 {
            h.store.gc(None).unwrap();
        }
        assert_eq!(h.integer("future-status", &[future.clone()]), status);
        let payload = h.value("future-result", &[future]);
        let array = payload
            .unwrap_anyref()
            .unwrap()
            .as_array(&h.store)
            .unwrap()
            .unwrap();
        assert_eq!(array.len(&h.store).unwrap(), 2);
        let number = array.get(&mut h.store, 0).unwrap();
        let number = number
            .unwrap_anyref()
            .unwrap()
            .as_struct(&h.store)
            .unwrap()
            .unwrap();
        assert_eq!(
            number
                .field(&mut h.store, 0)
                .unwrap()
                .unwrap_f64()
                .to_bits(),
            (-0.0f64).to_bits()
        );
        let text = array.get(&mut h.store, 1).unwrap();
        let units: Vec<_> = text
            .unwrap_anyref()
            .unwrap()
            .as_array(&h.store)
            .unwrap()
            .unwrap()
            .elems(&mut h.store)
            .unwrap()
            .map(|unit| unit.unwrap_i32() as u16)
            .collect();
        assert_eq!(units, [0xd800, 0, 0xdc00]);
    }
    // Resolving a language-error-shaped value is still Ready. No payload shape
    // determines failure; future-reject is the only Failed transition. WIT is not
    // attached here and no WIT result encoding is invented by this storage test.
    let future = h.value("future-pending-new", &[]);
    let text = h.value("string-new", &[Val::I32(0)]);
    let error = h.value("language-error-new", &[text]);
    assert_eq!(
        h.integer("future-resolve", &[future.clone(), error.clone()]),
        1
    );
    assert_eq!(h.integer("future-status", &[future.clone()]), 1);
    let result = h.value("future-result", &[future]);
    assert!(h.same(&result, &error));
}

#[test]
fn ordinary_copied_and_foreign_descriptors_are_not_futures_after_gc() {
    let mut h = Harness::new();
    let exemplar = h.value("future-pending-new", &[]);
    let mut fields = h.fields(&exemplar);
    let descriptor = fields[0].clone();
    let descriptor_fields = h.fields(&descriptor);
    fields[0] = h.forge(&descriptor, &descriptor_fields);
    let copied = h.forge(&exemplar, &fields);
    let schema = h.value("args-new", &[Val::I32(1)]);
    let ordinary_descriptor = h.value("descriptor-new", &[schema]);
    fields[0] = ordinary_descriptor;
    let ordinary = h.forge(&exemplar, &fields);
    let module = Module::new(h.store.engine(), runtime_abi::module()).unwrap();
    let foreign_runtime = Instance::new(&mut h.store, &module, &[]).unwrap();
    let mut foreign = [Val::null_any_ref()];
    foreign_runtime
        .get_func(&mut h.store, "future-pending-new")
        .unwrap()
        .call(&mut h.store, &[], &mut foreign)
        .unwrap();
    let nil = h.value("nil", &[]);
    let number = h.value("number-box", &[Val::F64(1.0f64.to_bits())]);
    h.store.gc(None).unwrap();
    for invalid in [
        copied,
        ordinary,
        foreign[0].clone(),
        nil.clone(),
        number,
        Val::null_any_ref(),
    ] {
        assert_eq!(h.integer("future-is", &[invalid.clone()]), 0);
        for operation in ["future-status", "future-result", "future-cancel"] {
            h.rejected(operation, &[invalid.clone()]);
        }
        for operation in ["future-resolve", "future-reject"] {
            h.rejected(operation, &[invalid.clone(), nil.clone()]);
        }
    }
    assert_eq!(h.integer("future-status", &[exemplar]), 0);
}

#[test]
fn nested_future_payload_is_retained_without_flattening_or_state_propagation() {
    let mut h = Harness::new();
    let outer = h.value("future-pending-new", &[]);
    let inner = h.value("future-pending-new", &[]);
    let failure = h.value("number-box", &[Val::F64(17.0f64.to_bits())]);
    assert_eq!(
        h.integer("future-reject", &[inner.clone(), failure.clone()]),
        1
    );
    assert_eq!(
        h.integer("future-resolve", &[outer.clone(), inner.clone()]),
        1
    );
    h.store.gc(None).unwrap();
    assert_eq!(h.integer("future-status", &[outer.clone()]), 1);
    let result = h.value("future-result", &[outer.clone()]);
    assert!(h.same(&result, &inner));
    assert_eq!(h.integer("future-status", &[result.clone()]), 2);
    let payload = h.value("future-result", &[result]);
    assert!(h.same(&payload, &failure));
    assert_eq!(h.integer("future-cancel", &[outer.clone()]), 0);
    let result = h.value("future-result", &[outer]);
    assert!(h.same(&result, &inner));
}

#[test]
fn malformed_future_storage_is_rejected_before_any_slot_mutation() {
    let mut h = Harness::new();
    let exemplar = h.value("future-pending-new", &[]);
    let nil = h.value("nil", &[]);
    let number = h.value("number-box", &[Val::F64(99.0f64.to_bits())]);
    let mut candidates = Vec::new();
    for length in [0, 2, 3] {
        candidates.push(h.value("args-new", &[Val::I32(length)]));
    }
    // Invalid outer slot: it must contain a complete two-slot outcome array.
    for value in [nil.clone(), number.clone(), Val::null_any_ref()] {
        let outer = h.value("args-new", &[Val::I32(1)]);
        outer
            .unwrap_anyref()
            .unwrap()
            .as_array(&h.store)
            .unwrap()
            .unwrap()
            .set(&mut h.store, 0, value)
            .unwrap();
        candidates.push(outer);
    }
    let mut outcomes = Vec::new();
    for length in [0, 1, 3] {
        outcomes.push(h.value("args-new", &[Val::I32(length)]));
    }
    for status in [number.clone(), Val::null_any_ref()] {
        let storage = h.value("args-new", &[Val::I32(2)]);
        storage
            .unwrap_anyref()
            .unwrap()
            .as_array(&h.store)
            .unwrap()
            .unwrap()
            .set(&mut h.store, 0, status)
            .unwrap();
        outcomes.push(storage);
    }
    // Forge invalid status values using an existing status scalar's Wasm API.
    for status in [-1, 4, 0, 3] {
        let storage = h.value("args-new", &[Val::I32(2)]);
        let array = storage
            .unwrap_anyref()
            .unwrap()
            .as_array(&h.store)
            .unwrap()
            .unwrap();
        let scalar = wasmtime::AnyRef::from_i31(
            &mut h.store,
            wasmtime::I31::new_u32((status as u32) & 0x7fff_ffff).unwrap(),
        );
        array
            .set(&mut h.store, 0, Val::AnyRef(Some(scalar)))
            .unwrap();
        if status == 0 || status == 3 {
            array.set(&mut h.store, 1, number.clone()).unwrap();
        }
        outcomes.push(storage);
    }
    for outcome in outcomes {
        let outer = h.value("args-new", &[Val::I32(1)]);
        outer
            .unwrap_anyref()
            .unwrap()
            .as_array(&h.store)
            .unwrap()
            .unwrap()
            .set(&mut h.store, 0, outcome)
            .unwrap();
        candidates.push(outer);
    }
    for storage in candidates {
        let mut fields = h.fields(&exemplar);
        fields[1] = storage.clone();
        let malformed = h.forge(&exemplar, &fields);
        let array = storage
            .unwrap_anyref()
            .unwrap()
            .as_array(&h.store)
            .unwrap()
            .unwrap();
        let before: Vec<_> = array.elems(&mut h.store).unwrap().collect();
        assert_eq!(h.integer("future-is", &[malformed.clone()]), 0);
        for operation in ["future-status", "future-result", "future-cancel"] {
            h.rejected(operation, &[malformed.clone()]);
        }
        for operation in ["future-resolve", "future-reject"] {
            h.rejected(operation, &[malformed.clone(), number.clone()]);
        }
        let after: Vec<_> = array.elems(&mut h.store).unwrap().collect();
        assert_eq!(before.len(), after.len());
        for (before, after) in before.iter().zip(&after) {
            match (before.unwrap_anyref(), after.unwrap_anyref()) {
                (None, None) => {}
                (Some(_), Some(_)) => assert!(h.same(before, after)),
                _ => panic!("invalid operation mutated future storage"),
            }
        }
    }
    assert_eq!(h.integer("future-resolve", &[exemplar.clone(), nil]), 1);
    assert_eq!(h.integer("future-status", &[exemplar]), 1);
}

#[test]
fn fuel_exhaustion_cannot_publish_a_partial_terminal_outcome() {
    let mut config = wasmtime::Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let mut h = Harness::with_engine(&engine, Some(100_000));
    let runtime = h.runtime;
    let mut traps = 0;
    let mut successes = 0;
    let mut pending_after_trap = 0;
    for (event, terminal) in [
        ("future-resolve", 1),
        ("future-reject", 2),
        ("future-cancel", 3),
    ] {
        for fuel in 0..=512 {
            h.store.set_fuel(100_000).unwrap();
            let mut scope = RootScope::new(&mut h.store);
            let mut future = [Val::null_any_ref()];
            runtime
                .get_func(&mut scope, "future-pending-new")
                .unwrap()
                .call(&mut scope, &[], &mut future)
                .unwrap();
            let mut payload = [Val::null_any_ref()];
            runtime
                .get_func(&mut scope, "number-box")
                .unwrap()
                .call(&mut scope, &[Val::F64(17.0f64.to_bits())], &mut payload)
                .unwrap();
            scope.as_context_mut().set_fuel(fuel).unwrap();
            let args = if terminal == 3 {
                vec![future[0].clone()]
            } else {
                vec![future[0].clone(), payload[0].clone()]
            };
            let mut output = [Val::I32(-1)];
            let attempted =
                runtime
                    .get_func(&mut scope, event)
                    .unwrap()
                    .call(&mut scope, &args, &mut output);
            match &attempted {
                Ok(()) => {
                    successes += 1;
                    assert_eq!(output[0].unwrap_i32(), 1);
                }
                Err(error) => {
                    traps += 1;
                    assert_eq!(
                        error.downcast_ref::<wasmtime::Trap>(),
                        Some(&wasmtime::Trap::OutOfFuel),
                        "{error:#}"
                    );
                }
            }
            assert!(!scope.as_context_mut().has_pending_exception());
            scope.as_context_mut().set_fuel(100_000).unwrap();
            scope.as_context_mut().gc(None).unwrap();
            runtime
                .get_func(&mut scope, "future-is")
                .unwrap()
                .call(&mut scope, &future, &mut output)
                .unwrap();
            assert_eq!(
                output[0].unwrap_i32(),
                1,
                "{event}, fuel {fuel}: partial outcome"
            );
            runtime
                .get_func(&mut scope, "future-status")
                .unwrap()
                .call(&mut scope, &future, &mut output)
                .unwrap();
            let observed = output[0].unwrap_i32();
            assert!(
                observed == 0 || observed == terminal,
                "{event}, fuel {fuel}: {observed}"
            );
            if attempted.is_ok() {
                assert_eq!(observed, terminal);
            } else if observed == 0 {
                pending_after_trap += 1;
            }
            // Independently inspect the complete record, including Pending=nil.
            let object = future[0]
                .unwrap_anyref()
                .unwrap()
                .as_struct(&scope)
                .unwrap()
                .unwrap();
            let outer = object
                .field(&mut scope, 1)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_array(&scope)
                .unwrap()
                .unwrap();
            assert_eq!(outer.len(&scope).unwrap(), 1);
            let outcome = outer
                .get(&mut scope, 0)
                .unwrap()
                .unwrap_anyref()
                .unwrap()
                .as_array(&scope)
                .unwrap()
                .unwrap();
            assert_eq!(outcome.len(&scope).unwrap(), 2);
            let state = outcome.get(&mut scope, 0).unwrap();
            assert_eq!(
                state
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&scope)
                    .unwrap()
                    .unwrap()
                    .get_u32(),
                observed as u32
            );
            let stored = outcome.get(&mut scope, 1).unwrap();
            if observed == 0 || observed == 3 {
                assert_eq!(
                    stored
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&scope)
                        .unwrap()
                        .unwrap()
                        .get_u32(),
                    0
                );
            } else {
                assert!(
                    Rooted::ref_eq(
                        &scope,
                        stored.unwrap_anyref().unwrap(),
                        payload[0].unwrap_anyref().unwrap()
                    )
                    .unwrap()
                );
            }
            // Retry recovers Pending; a published terminal rejects overwrite.
            runtime
                .get_func(&mut scope, event)
                .unwrap()
                .call(&mut scope, &args, &mut output)
                .unwrap();
            assert_eq!(output[0].unwrap_i32(), if observed == 0 { 1 } else { 0 });
        }
    }
    assert!(traps > 0 && successes > 0 && pending_after_trap > 0);
}
