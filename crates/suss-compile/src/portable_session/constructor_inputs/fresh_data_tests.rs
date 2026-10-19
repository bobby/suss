//! Adapted from the reviewed c74eb63 no-authority data classification test.
//! Source only: not current native execution evidence.
use super::*;
use crate::portable::resolve::Phase;
fn schema_test_session(phase: Phase) -> Session {
    match phase {
        Phase::Runtime => Session::new_repl().unwrap(),
        Phase::Macro => Session::new_macro().unwrap(),
    }
}
    fn put(s: &mut Session, value: &SessionValue, index: u32, replacement: &SessionValue) {
        s.inspect(value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let replacement = replacement.value.to_rooted(&mut store);
            fields.set(&mut store, index, Val::AnyRef(Some(replacement)))
        })
        .unwrap();
    }

#[test]
fn fresh_data_classification_never_mints_memo_or_accepts_other_mutations() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = schema_test_session(phase);
        let class = s.eval("Keyword").unwrap();
        let nil = s.eval("nil").unwrap();
        let name = s.eval("\"fresh-source-data\"").unwrap();
        let wrong = s.eval("\"different-name\"").unwrap();
        let numeric = s.eval("91").unwrap();
        let other_numeric = s.eval("92").unwrap();
        let (value, fact) = s
            .data_construct_with_inputs(&class, &[&nil, &name, &name, &nil])
            .unwrap();
        let fact = fact.unwrap();
        assert_eq!(
            fact.fresh_direct_status(&mut s, &value).unwrap(),
            (true, true)
        );
        put(&mut s, &value, 3, &numeric);
        assert!(!fact.matches(&mut s, &value).unwrap());
        let expected = (true, false);
        assert_eq!(fact.runtime_memo_data.is_some(), phase == Phase::Runtime);
        assert_eq!(fact.memo_watch.is_some(), phase == Phase::Macro);
        assert_eq!(fact.fresh_direct_status(&mut s, &value).unwrap(), expected);
        s.collect().unwrap();
        assert_eq!(fact.fresh_direct_status(&mut s, &value).unwrap(), expected);
        if let Some(watch) = &fact.memo_watch {
            assert!(!watch.matches(&mut s).unwrap());
        }
        // A second field mutation cannot hide behind the numeric memo.
        put(&mut s, &value, 1, &wrong);
        assert_eq!(
            fact.fresh_direct_status(&mut s, &value).unwrap(),
            (false, false)
        );
        put(&mut s, &value, 1, &name);
        put(&mut s, &value, 3, &wrong);
        assert_eq!(
            fact.fresh_direct_status(&mut s, &value).unwrap(),
            (false, false)
        );
        put(&mut s, &value, 3, &nil);
        assert_eq!(
            fact.fresh_direct_status(&mut s, &value).unwrap(),
            (true, true)
        );
        // Originally nonnil factory inputs never gain this classification.
        let (warm, warm_fact) = s
            .data_construct_with_inputs(&class, &[&nil, &name, &name, &numeric])
            .unwrap();
        let warm_fact = warm_fact.unwrap();
        put(&mut s, &warm, 3, &other_numeric);
        assert_eq!(
            warm_fact.fresh_direct_status(&mut s, &warm).unwrap(),
            (false, false)
        );
        let handles = s.stats().external_value_handles;
        for (memo, expected_status) in [(&nil, (true, true)), (&numeric, expected)] {
            put(&mut s, &value, 3, memo);
            let work = s
                .with_operation_budget(|s| {
                    let before = s.operation_fuel_remaining()?;
                    assert_eq!(fact.fresh_direct_status(s, &value)?, expected_status);
                    Ok(before - s.operation_fuel_remaining()?)
                })
                .unwrap();
            for cut in 0..work {
                s.set_operation_fuel(cut);
                let error = fact.fresh_direct_status(&mut s, &value).unwrap_err();
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
                fact.fresh_direct_status(&mut s, &value)
                    .unwrap_err()
                    .is_interrupt()
            );
            drop(request);
            assert_eq!(
                fact.fresh_direct_status(&mut s, &value).unwrap(),
                expected_status
            );
            if let Some(watch) = &fact.memo_watch {
                assert!(!watch.matches(&mut s).unwrap());
            }
        }
        s.reset().unwrap();
        assert!(matches!(
            fact.fresh_direct_status(&mut s, &value),
            Err(SessionError::ForeignValue)
        ));
    }
}


#[test]
fn fresh_runtime_data_memo_rejects_custom_schema_wrong_slot_and_noncanonical_number() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = schema_test_session(phase);
        let class = s.eval("Keyword").unwrap();
        let nil = s.eval("nil").unwrap();
        let name = s.eval("\"data-only-negative\"").unwrap();
        let number = s.eval("19").unwrap();
        let (value, fact) = s.data_construct_with_inputs(&class, &[&nil, &name, &name, &nil]).unwrap();
        let fact = fact.unwrap();
        // Wrong slot cannot borrow the genuine memo-slot eligibility.
        put(&mut s, &value, 2, &number);
        assert_eq!(fact.fresh_direct_status(&mut s, &value).unwrap(), (false, false));
        put(&mut s, &value, 2, &name);
        // Same f64 shape with a different nominal GC type is not a Number.
        let fake = {
            use wasmtime::{FieldType, Mutability, StorageType, StructRef, StructRefPre, ValType};
            let ty = StructType::new(&s.engine,
                [FieldType::new(Mutability::Var, StorageType::ValType(ValType::F64))]).unwrap();
            let allocator = StructRefPre::new(&mut s.store, ty);
            let mut scope = RootScope::new(&mut s.store);
            let object = StructRef::new(&mut scope, &allocator, &[Val::F64(19.0f64.to_bits())]).unwrap();
            own(&mut scope, Val::AnyRef(Some(object.to_anyref())), s.identity, &s.handles).unwrap()
        };
        put(&mut s, &value, 3, &fake);
        assert_eq!(fact.fresh_direct_status(&mut s, &value).unwrap(), (false, false));
        put(&mut s, &value, 3, &number);
        assert_eq!(fact.fresh_direct_status(&mut s, &value).unwrap(), (true, false));
        assert!(!fact.matches(&mut s, &value).unwrap());
        // Mutable contents of the original captured schema remain checked.
        let old = s.inspect(&fact.schema, |mut store, value| {
            let schema = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let key = schema.get(&mut store, 0)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let old = key.get(&mut store, 0)?.unwrap_i32();
            key.set(&mut store, 0, Val::I32(old ^ 1))?;
            Ok(old)
        }).unwrap();
        assert_eq!(fact.fresh_direct_status(&mut s, &value).unwrap(), (false, false));
        s.inspect(&fact.schema, |mut store, value| {
            let schema = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let key = schema.get(&mut store, 0)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            key.set(&mut store, 0, Val::I32(old))
        }).unwrap();
        s.collect().unwrap();
        assert_eq!(fact.fresh_direct_status(&mut s, &value).unwrap(), (true, false));
        s.eval("(deftype DataOnlyLookalike [ns name fqn memo])").unwrap();
        let custom = s.eval("DataOnlyLookalike").unwrap();
        let (custom_value, custom_fact) = s.data_construct_with_inputs(&custom, &[&nil, &name, &name, &nil]).unwrap();
        let custom_fact = custom_fact.unwrap();
        assert!(custom_fact.runtime_memo_data.is_none());
        assert!(custom_fact.memo_watch.is_none());
        put(&mut s, &custom_value, 3, &number);
        assert_eq!(custom_fact.fresh_direct_status(&mut s, &custom_value).unwrap(), (false, false));
    }
}


#[test]
fn runtime_data_memo_capture_sampled_fuel_irq_failure_and_retry_remain_typed() {
    let mut s = Session::new_repl().unwrap();
    let class = s.eval("Keyword").unwrap();
    let nil = s.eval("nil").unwrap();
    let name = s.eval("\"capture-data-only\"").unwrap();
    // Warm the existing factory/prototype before comparing capture boundaries.
    drop(s.data_construct_with_inputs(&class, &[&nil, &name, &name, &nil]).unwrap());
    let work = s.with_operation_budget(|s| {
        let before = s.operation_fuel_remaining()?;
        let (_, fact) = s.data_construct_with_inputs(&class, &[&nil, &name, &name, &nil])?;
        assert!(fact.unwrap().runtime_memo_data.is_some());
        Ok(before - s.operation_fuel_remaining()?)
    }).unwrap();
    assert!(work > 1);
    for cut in [0, 1, work / 2, work.saturating_sub(1)] {
        s.set_operation_fuel(cut);
        let trial = s.data_construct_with_inputs(&class, &[&nil, &name, &name, &nil]);
        match trial {
            Err(SessionError::Trap(ref error)) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel) => {},
            Ok((value, Some(fact))) if cut > 1 => {
                assert!(fact.runtime_memo_data.is_some());
                assert!(fact.memo_watch.is_none());
                s.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
                assert!(fact.matches(&mut s, &value).unwrap());
            },
            other => panic!("capture cut={cut}/{work}: {other:?}"),
        }
        s.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let (value, fact) = s.data_construct_with_inputs(&class, &[&nil, &name, &name, &nil]).unwrap();
        let fact = fact.unwrap();
        assert!(fact.runtime_memo_data.is_some());
        assert!(fact.matches(&mut s, &value).unwrap());
    }
    let request = s.operation_budget_scope().unwrap();
    s.interrupt_handle().interrupt();
    assert!(s.data_construct_with_inputs(&class, &[&nil, &name, &name, &nil]).unwrap_err().is_interrupt());
    drop(request);
    let (value, fact) = s.data_construct_with_inputs(&class, &[&nil, &name, &name, &nil]).unwrap();
    let fact = fact.unwrap();
    assert!(fact.runtime_memo_data.is_some());
    assert!(fact.memo_watch.is_none());
    assert!(fact.matches(&mut s, &value).unwrap());
}
