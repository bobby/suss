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
#[test]
fn fresh_data_classification_never_mints_memo_or_accepts_other_mutations() {
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
        let expected = if phase == Phase::Macro {
            (true, false)
        } else {
            (false, false)
        };
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
