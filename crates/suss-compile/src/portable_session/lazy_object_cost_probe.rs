//! Test-only diagnostic. Wire ONLY in isolated ce88795 checkout with
//! #[cfg(test)] mod lazy_object_cost_probe; in portable_session.rs.
use super::*;
use wasmtime::{Rooted, AsContextMut};
const FUEL: u64 = 10_000_000;
const SETUP: &str = r#"
(def probe-thunks 0)
(def probe-lazy (new suss.core/LazySeq nil
  (fn* [] (set! probe-thunks (+ probe-thunks 1)) (list 42)) nil nil))
(def probe-expected (.sval probe-lazy))
(def probe-dynamic (fn* [x] (.sval x)))
(def probe-method (.-sval probe-lazy))
(def probe-lookup-call (fn* [m x] (.call (.-sval x) x)))
(def probe-captured-call (fn* [m x] (.call m x)))
(if (identical? probe-method (.-sval probe-lazy)) true (throw "Method identity unstable"))
(if (= probe-thunks 1) true (throw "Warm thunk count not one"))
"#;
#[derive(Clone, Copy, Debug)]
enum Route { SourceWrapper, MatchedLookupCall, MatchedCapturedCall, StoredS }
#[test]
fn realized_lazy_object_lookup_vs_captured_original_cost() -> wasmtime::Result<()> {
    let mut session = Session::new_macro().unwrap();
    assert_eq!(session.options.fuel_per_operation, FUEL);
    session.eval(SETUP).unwrap();
    let receiver = session.eval("probe-lazy").unwrap();
    let expected = session.eval("probe-expected").unwrap();
    let wrapper = session.eval("probe-dynamic").unwrap();
    let lookup_adapter = session.eval("probe-lookup-call").unwrap();
    let captured_adapter = session.eval("probe-captured-call").unwrap();
    // Exact runtime exports selected by existing source Nominal lowering.
    // Prepare handles outside measured loops; do not capture a replacement body.
    let invoke = session.runtime.get_func(&mut session.store, "invoke").unwrap();
    let args_new = session.runtime.get_func(&mut session.store, "args-new").unwrap();
    let field_get = session.runtime.get_func(&mut session.store, "object-field-get").unwrap();
    let current_this = session.runtime.get_func(&mut session.store, "current-this").unwrap();
    let captured = session.eval("probe-method").unwrap();
    session.collect().unwrap();
    for n in [100usize, 200] {
        for (order, routes) in [
            ("forward", [Route::SourceWrapper, Route::MatchedLookupCall, Route::MatchedCapturedCall, Route::StoredS]),
            ("reverse", [Route::StoredS, Route::MatchedCapturedCall, Route::MatchedLookupCall, Route::SourceWrapper]),
        ] {
            for route in routes {
                let _op = session.operation_budget_scope().unwrap();
                // All100/200 nested VM calls share THIS sole10M request.
                let mut scope = RootScope::new(&mut session.store);
                let recv = Val::AnyRef(Some(receiver.rooted(&mut scope.as_context_mut())));
                let target = Val::AnyRef(Some(captured.rooted(&mut scope.as_context_mut())));
                let wrapper_value = Val::AnyRef(Some(wrapper.rooted(&mut scope.as_context_mut())));
                let lookup_value = Val::AnyRef(Some(lookup_adapter.rooted(&mut scope.as_context_mut())));
                let captured_value = Val::AnyRef(Some(captured_adapter.rooted(&mut scope.as_context_mut())));
                let mut this_before = [Val::null_any_ref()];
                current_this.call(&mut scope, &[], &mut this_before).unwrap();
                let expected_value = expected.rooted(&mut scope.as_context_mut());
                let before = scope.as_context_mut().get_fuel().unwrap();
                let mut completed = 0usize;
                let outcome: wasmtime::Result<()> = (|| {
                    for _ in 0..n {
                        let mut result = [Val::null_any_ref()];
                        match route {
                            Route::StoredS => field_get.call(&mut scope, &[recv.clone(), Val::I32(2)], &mut result)?,
                            Route::SourceWrapper => {
                                let mut array = [Val::null_any_ref()];
                                args_new.call(&mut scope, &[Val::I32(1)], &mut array)?;
                                array[0].unwrap_anyref().unwrap().as_array(&scope)?.unwrap()
                                    .set(&mut scope, 0, recv.clone())?;
                                invoke.call(&mut scope, &[wrapper_value.clone(), array[0].clone()], &mut result)?;
                            }
                            Route::MatchedLookupCall | Route::MatchedCapturedCall => {
                                // SAME two input values and ABI staging in A/B.
                                let mut array = [Val::null_any_ref()];
                                args_new.call(&mut scope, &[Val::I32(2)], &mut array)?;
                                let args = array[0].unwrap_anyref().unwrap().as_array(&scope)?.unwrap();
                                args.set(&mut scope, 0, target.clone())?;
                                args.set(&mut scope, 1, recv.clone())?;
                                let adapter = if matches!(route, Route::MatchedLookupCall) {
                                    lookup_value.clone()
                                } else { captured_value.clone() };
                                invoke.call(&mut scope, &[adapter, array[0].clone()], &mut result)?;
                            }
                        }
                        let actual = result[0].unwrap_anyref().unwrap();
                        assert!(Rooted::ref_eq(&scope, actual, &expected_value)?);
                        completed += 1;
                    }
                    Ok(())
                })();
                let after = scope.as_context_mut().get_fuel().ok();
                println!("lazy-object-probe route={route:?} order={order} n={n} completed={completed} before={before} after={after:?} delta={:?} success={} error={:?}",
                    after.and_then(|a|before.checked_sub(a)),outcome.is_ok(),outcome.as_ref().err().map(|e|e.to_string()));
                // Check physical-this restoration outside measured interval,
                // including an error outcome if encountered; retain original result.
                let mut this_after = [Val::null_any_ref()];
                let restoration = current_this.call(&mut scope, &[], &mut this_after);
                println!("lazy-object-restoration route={route:?} order={order} n={n} success={} error={:?}",
                    restoration.is_ok(), restoration.as_ref().err().map(|e| e.to_string()));
                // Do not mask the invocation error with an exhausted observer.
                // No refill/retry; only successful calls require equality.
                outcome?;
                restoration?;
                assert!(Rooted::ref_eq(&scope,
                    this_before[0].unwrap_anyref().unwrap(),
                    this_after[0].unwrap_anyref().unwrap())?);
                assert!(after.is_some());
                drop(scope);
                drop(_op);
                session.eval("(if (and (= probe-thunks 1) (identical? probe-method (.-sval probe-lazy))) true (throw \"Thunk count or method identity changed\"))").unwrap();
            }
        }
    }
    session.collect().unwrap();
    session.eval("(if (and (= probe-thunks 1) (identical? (.sval probe-lazy) probe-expected)) true (throw \"Final warm/GC check failed\"))").unwrap();
    Ok(())
}
