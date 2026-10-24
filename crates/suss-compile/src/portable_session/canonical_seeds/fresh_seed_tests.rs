//! Full snapshot negatives for the private initial-IHash-slot DATA exception.
//! No change to strict matches/membership/receipts. Source only, unexecuted.
use super::*;
use wasmtime::{RootScope, Val};
use crate::portable::resolve::Phase;
fn session(phase: Phase) -> Session {
    match phase {
        Phase::Runtime => Session::new_repl().unwrap(),
        Phase::Macro => Session::new_macro().unwrap(),
    }
}
fn put_seed_field(s: &mut Session, value: &SessionValue, field: u32, replacement: &SessionValue) {
    s.inspect(value, |mut store, value| {
        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        let fields = object
            .field(&mut store, 1)?
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)?
            .unwrap();
        let replacement = replacement.value.to_rooted(&mut store);
        fields.set(&mut store, field, Val::AnyRef(Some(replacement)))
    })
    .unwrap();
}
#[test]
fn fresh_seed_hash_exception_does_not_excuse_prototype_or_other_protocol_poison() {
    use crate::portable_session::memo_leaf_test_support::Method;
    // Retain every extracted GC reference as a SessionValue before the
    // scope ends; subsequent collection cannot invalidate restore values.
    fn retain_path(s: &mut Session, root: &SessionValue, path: &[(bool, u32)]) -> SessionValue {
        s.charge_host_work(path.len() as u64 * 3 + 1).unwrap();
        let mut scope = RootScope::new(&mut s.store);
        let mut value = Val::AnyRef(Some(root.value.to_rooted(&mut scope)));
        for &(array, index) in path {
            let reference = value.unwrap_anyref().unwrap();
            value = if array {
                reference
                    .as_array(&scope)
                    .unwrap()
                    .unwrap()
                    .get(&mut scope, index)
                    .unwrap()
            } else {
                reference
                    .as_struct(&scope)
                    .unwrap()
                    .unwrap()
                    .field(&mut scope, index as usize)
                    .unwrap()
            };
        }
        super::super::own(&mut scope, value, s.identity, &s.handles).unwrap()
    }
    fn put(s: &mut Session, array: &SessionValue, index: u32, value: &SessionValue) {
        s.inspect(array, |mut store, root| {
            let array = root.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let value = value.value.to_rooted(&mut store);
            array.set(&mut store, index, Val::AnyRef(Some(value)))
        })
        .unwrap();
    }
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = session(phase);
        s.eval("(def leaf-original-method nil)").unwrap();
        let original_hash = Method::capture(&mut s, "PersistentArrayMap").unwrap();
        original_hash.bind_delegate(&mut s).unwrap();
        let map = s.eval("(.-EMPTY PersistentArrayMap)").unwrap();
        let seed = s.canonical_seed("empty-map").unwrap().unwrap();
        assert!(seed.matches(&mut s, &map).unwrap());
        // Exact seed-owned route: nominal field2 properties[0] is its
        // prototype; native prototype field1 is [descriptors,parent].
        // Mutate the prototype's own parent edge, not the seed root edge.
        let prototype = retain_path(&mut s, &map, &[(false, 2), (true, 0)]);
        let prototype_storage = retain_path(&mut s, &prototype, &[(false, 1)]);
        let original_parent = retain_path(&mut s, &prototype_storage, &[(true, 1)]);
        s.inspect(&prototype_storage, |store, value| {
            assert_eq!(
                value
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap()
                    .len(&store)?,
                2
            );
            Ok(())
        })
        .unwrap();
        let descriptor = s.data_descriptor(&map, false).unwrap();
        let table = retain_path(&mut s, &descriptor, &[(false, 2)]);
        let handles = s.handles.clone();
        let key_name = format!(
            "key-{}",
            "suss.core/ICounted/-count/1"
                .bytes()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let count_key =
            canonical_methods::binding(&mut s, "suss.internal.protocol-keys", &key_name, &handles)
                .unwrap();
        let original_count = canonical_methods::runtime(
            &mut s,
            "protocol-method-get",
            &[&descriptor, &count_key],
            &handles,
        )
        .unwrap();
        // Locate the already-existing slot by exact key root, never by
        // assumed slot ordinal or textual equality of a current key.
        let count_index = s
            .inspect(&table, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let key = count_key.value.to_rooted(&mut store);
                let len = array.len(&store)?;
                assert_eq!(len % 2, 0);
                for index in (0..len).step_by(2) {
                    let found = array.get(&mut store, index)?;
                    if wasmtime::Rooted::<wasmtime::AnyRef>::ref_eq(
                        &store,
                        found.unwrap_anyref().unwrap(),
                        &key,
                    )? {
                        return Ok(index + 1);
                    }
                }
                panic!("shipped PersistentArrayMap must already implement ICounted");
            })
            .unwrap();
        s.eval("(extend-type PersistentArrayMap IHash (-hash [self] (leaf-original-method self)))")
            .unwrap();
        assert_eq!(
            seed.fresh_direct_status(&mut s, &map).unwrap(),
            (true, false)
        );
        let poison = s.eval("99").unwrap();
        put(&mut s, &prototype_storage, 1, &poison);
        s.collect().unwrap();
        assert_eq!(
            seed.fresh_direct_status(&mut s, &map).unwrap(),
            (false, false)
        );
        assert!(!seed.matches(&mut s, &map).unwrap());
        put(&mut s, &prototype_storage, 1, &original_parent);
        s.collect().unwrap();
        assert_eq!(
            seed.fresh_direct_status(&mut s, &map).unwrap(),
            (true, false)
        );
        // A valid closure in another protocol's existing value slot must
        // remain an ordinary snapshot mismatch, even while IHash is allowed.
        let replacement_count = s.eval("(fn [_] 73)").unwrap();
        put(&mut s, &table, count_index, &replacement_count);
        s.collect().unwrap();
        assert_eq!(
            seed.fresh_direct_status(&mut s, &map).unwrap(),
            (false, false)
        );
        assert!(!seed.matches(&mut s, &map).unwrap());
        put(&mut s, &table, count_index, &original_count);
        s.collect().unwrap();
        assert_eq!(
            seed.fresh_direct_status(&mut s, &map).unwrap(),
            (true, false)
        );
        assert!(!seed.matches(&mut s, &map).unwrap());
        original_hash.restore(&mut s).unwrap();
        assert_eq!(
            seed.fresh_direct_status(&mut s, &map).unwrap(),
            (true, true)
        );
        assert!(seed.matches(&mut s, &map).unwrap());
    }
}
#[test]
fn fresh_seed_method_data_preserves_pristine_membership_and_rejects_payload_poison() {
    use crate::portable_session::memo_leaf_test_support::Method;
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = session(phase);
        s.eval("(def leaf-original-method nil)").unwrap();
        let method = Method::capture(&mut s, "PersistentArrayMap").unwrap();
        method.bind_delegate(&mut s).unwrap();
        let map = s.eval("(.-EMPTY PersistentArrayMap)").unwrap();
        let set = s.eval("(.-EMPTY PersistentHashSet)").unwrap();
        let map_seed = s.canonical_seed("empty-map").unwrap().unwrap();
        let set_seed = s.canonical_seed("empty-set").unwrap().unwrap();
        assert_eq!(
            map_seed.fresh_direct_status(&mut s, &map).unwrap(),
            (true, true)
        );
        assert_eq!(
            set_seed.fresh_direct_status(&mut s, &set).unwrap(),
            (true, true)
        );
        s.eval("(extend-type PersistentArrayMap IHash (-hash [self] (leaf-original-method self)))")
            .unwrap();
        s.collect().unwrap();
        for (value, seed) in [(&map, &map_seed), (&set, &set_seed)] {
            assert!(!seed.matches(&mut s, value).unwrap());
            assert_eq!(
                seed.fresh_direct_status(&mut s, value).unwrap(),
                (true, false)
            );
            assert!(
                !seed.matches(&mut s, value).unwrap(),
                "fresh check cannot refresh pristine authority"
            );
            assert!(seed.contains_initial_reference(&mut s, value).unwrap());
        }
        let class = s.eval("PersistentArrayMap").unwrap();
        let fields = s.data_fields(&map).unwrap();
        let clone = s
            .data_construct(&class, &fields.iter().collect::<Vec<_>>())
            .unwrap();
        assert_eq!(
            map_seed.fresh_direct_status(&mut s, &clone).unwrap(),
            (false, false)
        );
        assert!(!map_seed.contains_initial_reference(&mut s, &clone).unwrap());
        let bad = s.eval("99").unwrap();
        for index in [0u32, 1, 3] {
            put_seed_field(&mut s, &map, index, &bad);
            assert_eq!(
                map_seed.fresh_direct_status(&mut s, &map).unwrap(),
                (false, false)
            );
            put_seed_field(&mut s, &map, index, &fields[index as usize]);
            assert_eq!(
                map_seed.fresh_direct_status(&mut s, &map).unwrap(),
                (true, false)
            );
        }
        // UID is a struct slot: an array exception must never excuse it.
        let old_uid = s
            .inspect(&map, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let old = object
                    .field(&mut store, 3)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32();
                let bad =
                    wasmtime::AnyRef::from_i31(&mut store, wasmtime::I31::new_u32(42).unwrap());
                object.set_field(&mut store, 3, Val::AnyRef(Some(bad)))?;
                Ok(old)
            })
            .unwrap();
        assert_eq!(
            map_seed.fresh_direct_status(&mut s, &map).unwrap(),
            (false, false)
        );
        s.inspect(&map, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let old =
                wasmtime::AnyRef::from_i31(&mut store, wasmtime::I31::new_u32(old_uid).unwrap());
            object.set_field(&mut store, 3, Val::AnyRef(Some(old)))
        })
        .unwrap();
        assert_eq!(
            map_seed.fresh_direct_status(&mut s, &map).unwrap(),
            (true, false)
        );
        method.restore(&mut s).unwrap();
        assert!(map_seed.matches(&mut s, &map).unwrap());
        assert!(set_seed.matches(&mut s, &set).unwrap());
        // Poison nested SourceArray backing through the actual source setter.
        s.eval("(extend-type PersistentArrayMap IHash (-hash [self] (leaf-original-method self)))")
            .unwrap();
        let mutate = s.eval("(fn [a] (aset a 0 99))").unwrap();
        s.invoke(&mutate, &[&fields[2]]).unwrap();
        s.collect().unwrap();
        assert_eq!(
            map_seed.fresh_direct_status(&mut s, &map).unwrap(),
            (false, false)
        );
        assert_eq!(
            set_seed.fresh_direct_status(&mut s, &set).unwrap(),
            (false, false)
        );
        assert!(!map_seed.matches(&mut s, &map).unwrap());
    }
}
#[test]
fn fresh_seed_method_data_fuel_irq_missing_authority_and_reset_do_not_certify() {
    use crate::portable_session::memo_leaf_test_support::Method;
    let mut s = session(Phase::Macro);
    s.eval("(def leaf-original-method nil)").unwrap();
    let method = Method::capture(&mut s, "PersistentArrayMap").unwrap();
    method.bind_delegate(&mut s).unwrap();
    let map = s.eval("(.-EMPTY PersistentArrayMap)").unwrap();
    let seed = s.canonical_seed("empty-map").unwrap().unwrap();
    s.eval("(extend-type PersistentArrayMap IHash (-hash [self] (leaf-original-method self)))")
        .unwrap();
    let work = s
        .with_operation_budget(|s| {
            let before = s.operation_fuel_remaining()?;
            assert_eq!(seed.fresh_direct_status(s, &map)?, (true, false));
            Ok(before - s.operation_fuel_remaining()?)
        })
        .unwrap();
    let handles = s.stats().external_value_handles;
    for cut in [
        0,
        1,
        2,
        8,
        32,
        128,
        512,
        2048,
        8192,
        work / 2,
        work.saturating_sub(1),
    ] {
        if cut >= work {
            continue;
        }
        s.set_operation_fuel(cut);
        let error = seed.fresh_direct_status(&mut s, &map).unwrap_err();
        assert!(
            matches!(error, SessionError::Trap(ref e)
                if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
            "cut={cut}/{work}: {error:?}"
        );
        assert_eq!(s.stats().external_value_handles, handles);
    }
    s.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
    let request = s.operation_budget_scope().unwrap();
    s.interrupt_handle().interrupt();
    assert!(
        seed.fresh_direct_status(&mut s, &map)
            .unwrap_err()
            .is_interrupt()
    );
    drop(request);
    assert_eq!(
        seed.fresh_direct_status(&mut s, &map).unwrap(),
        (true, false)
    );
    let authority = s.canonical_methods.take();
    assert_eq!(
        seed.fresh_direct_status(&mut s, &map).unwrap(),
        (false, false)
    );
    s.canonical_methods = authority;
    assert_eq!(
        seed.fresh_direct_status(&mut s, &map).unwrap(),
        (true, false)
    );
    assert!(!seed.matches(&mut s, &map).unwrap());
    method.restore(&mut s).unwrap();
    assert!(seed.matches(&mut s, &map).unwrap());
    s.reset().unwrap();
    assert!(matches!(
        seed.fresh_direct_status(&mut s, &map),
        Err(SessionError::ForeignValue)
    ));
    let current = s.eval("(.-EMPTY PersistentArrayMap)").unwrap();
    assert!(matches!(
        seed.fresh_direct_status(&mut s, &current),
        Err(SessionError::ForeignValue)
    ));
}
