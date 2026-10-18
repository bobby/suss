//! Build-local locations in fresh callback-free factory storage. Not source authority.
use super::super::own;
use super::*;
use std::{cell::RefCell, rc::Rc};
pub(crate) type SharedSlotLog = Rc<RefCell<PrivateSlotLog>>;
struct Token {
    id: usize,
    value: SessionValue,
}
struct Slot {
    array: SessionValue,
    index: u32,
    token: usize,
}
pub(crate) struct PrivateSlotLog {
    tokens: Vec<Token>,
    slots: Vec<Slot>,
    complete: bool,
    identity: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn number(s: &mut Session, n: f64) -> SessionValue {
        s.data_scalar(&crate::portable::hir::Literal::Number(n))
            .unwrap()
    }
    #[test]
    fn private_slot_log_dense_copy_nominal_clone_alias_and_walker_agree_after_gc() {
        let mut s = Session::new_macro().unwrap();
        let operation = s.operation_budget_scope().unwrap();
        let replacement = number(&mut s, 17.0);
        let log = PrivateSlotLog::new(&mut s).unwrap();
        let token = log.borrow_mut().token(&mut s, 7).unwrap();
        let selected = log
            .borrow()
            .inputs(&mut s, &[&token, &replacement, &token])
            .unwrap();
        let array = s.data_array(&[&token, &replacement, &token]).unwrap();
        log.borrow_mut()
            .capture_dense(&mut s, &array, 3, &selected)
            .unwrap();
        let class = s.eval("PersistentArrayMap").unwrap();
        let nil = s.data_scalar(&crate::portable::hir::Literal::Nil).unwrap();
        let count = number(&mut s, 1.0);
        let args = [&token, &count, &array, &nil];
        let selected = log.borrow().inputs(&mut s, &args).unwrap();
        let parent = s
            .data_construct_without_guest_callbacks(&class, &args)
            .unwrap();
        log.borrow_mut()
            .capture_nominal(&mut s, &parent, &selected)
            .unwrap();
        let fields = s.data_fields(&parent).unwrap();
        let args = fields.iter().collect::<Vec<_>>();
        let clone = s
            .data_construct_without_guest_callbacks(&class, &args)
            .unwrap();
        let selected = log.borrow().inputs(&mut s, &args).unwrap();
        log.borrow_mut()
            .capture_nominal(&mut s, &clone, &selected)
            .unwrap();
        let root = s.data_array(&[&parent, &clone, &array]).unwrap();
        // Build a separate matching graph; the old walker never sees log-patched storage.
        let old_token = s.data_array(&[]).unwrap();
        let old_array = s
            .data_array(&[&old_token, &replacement, &old_token])
            .unwrap();
        let old_parent = s
            .data_construct_without_guest_callbacks(&class, &[&old_token, &count, &old_array, &nil])
            .unwrap();
        let old_fields = s.data_fields(&old_parent).unwrap();
        let old_clone = s
            .data_construct_without_guest_callbacks(&class, &old_fields.iter().collect::<Vec<_>>())
            .unwrap();
        let old_root = s
            .data_array(&[&old_parent, &old_clone, &old_array])
            .unwrap();
        s.collect().unwrap();
        let writes = log
            .borrow()
            .apply(&mut s, &root, &[(&token, &replacement)])
            .unwrap();
        assert_eq!(writes, 4);
        assert_eq!(
            s.fixup_private_placeholders(&old_root, &[(&old_token, &replacement)])
                .unwrap(),
            4
        );
        for (a, b) in [(&parent, &old_parent), (&clone, &old_clone)] {
            let fields = s.data_fields(a).unwrap();
            let old_fields = s.data_fields(b).unwrap();
            assert!(same_value(&mut s, &fields[0], &replacement));
            assert!(same_value(&mut s, &old_fields[0], &replacement));
        }
        for data in [&array, &old_array] {
            let get = s.eval("(fn [a i] (aget a i))").unwrap();
            for i in 0..3 {
                let index = number(&mut s, i as f64);
                let v = s.invoke(&get, &[data, &index]).unwrap();
                assert!(same_value(&mut s, &v, &replacement));
            }
        }
        assert_eq!(
            s.fixup_private_placeholders(&root, &[(&token, &replacement)])
                .unwrap(),
            0
        );
        drop(log);
        drop(operation);
    }
    fn set_slot(s: &mut Session, array: &SessionValue, index: u32, value: &SessionValue) {
        s.inspect(array, |mut store, array| {
            let array = array.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let value = value.value.to_rooted(&mut store);
            array.set(&mut store, index, Val::AnyRef(Some(value)))?;
            Ok(())
        })
        .unwrap();
    }
    fn same_value(s: &mut Session, a: &SessionValue, b: &SessionValue) -> bool {
        s.inspect(a, |mut store, a| {
            let b = b.value.to_rooted(&mut store);
            Ok(Rooted::ref_eq(&store, a.unwrap_anyref().unwrap(), &b)?)
        })
        .unwrap()
    }
    #[test]
    fn private_slot_log_rejects_wrong_slot_foreign_and_recovers_after_fuel_irq() {
        let mut s = Session::new_macro().unwrap();
        let op = s.operation_budget_scope().unwrap();
        let target = number(&mut s, 19.0);
        let log = PrivateSlotLog::new(&mut s).unwrap();
        let token = log.borrow_mut().token(&mut s, 0).unwrap();
        let selected = log.borrow().inputs(&mut s, &[&token]).unwrap();
        let array = s.data_array(&[&token]).unwrap();
        log.borrow_mut()
            .capture_dense(&mut s, &array, 1, &selected)
            .unwrap();
        let mut foreign = Session::new_macro().unwrap();
        let other = number(&mut foreign, 3.0);
        assert!(
            log.borrow()
                .apply(&mut s, &array, &[(&token, &other)])
                .is_err()
        );
        // Deliberately alter the physical final slot before apply, not merely reapply.
        let physical = log.borrow().slots[0].array.clone();
        set_slot(&mut s, &physical, 0, &target);
        assert!(
            log.borrow()
                .apply(&mut s, &array, &[(&token, &target)])
                .is_err()
        );
        set_slot(&mut s, &physical, 0, &token);
        drop(op);
        s.set_operation_fuel(1);
        let op = s.operation_budget_scope().unwrap();
        assert!(
            log.borrow()
                .apply(&mut s, &array, &[(&token, &target)])
                .is_err()
        );
        drop(op);
        s.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let op = s.operation_budget_scope().unwrap();
        s.interrupt_handle().interrupt();
        assert!(
            log.borrow()
                .apply(&mut s, &array, &[(&token, &target)])
                .unwrap_err()
                .is_interrupt()
        );
        drop(op);
        let op = s.operation_budget_scope().unwrap();
        assert_eq!(
            log.borrow()
                .apply(&mut s, &array, &[(&token, &target)])
                .unwrap(),
            1
        );
        assert!(
            log.borrow()
                .apply(&mut s, &array, &[(&token, &target)])
                .is_err()
        );
        drop(op);
    }
    fn physical_slot_is(s: &mut Session, array: &SessionValue, index: u32, expected: &SessionValue) -> bool {
        s.inspect(array, |mut store, array| {
            let array = array.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let actual = array.get(&mut store, index)?.unwrap_anyref().copied().unwrap();
            let expected = expected.value.to_rooted(&mut store);
            Ok(Rooted::ref_eq(&store, &actual, &expected)?)
        }).unwrap()
    }
    fn two_slot_fixture(s: &mut Session, capture: bool)
        -> (SharedSlotLog, SessionValue, SessionValue, Vec<(u32, usize)>)
    {
        let log = PrivateSlotLog::new(s).unwrap();
        let token = log.borrow_mut().token(s, 0).unwrap();
        let selected = log.borrow().inputs(s, &[&token, &token]).unwrap();
        let parent = s.data_array(&[&token, &token]).unwrap();
        if capture { log.borrow_mut().capture_dense(s, &parent, 2, &selected).unwrap(); }
        (log, token, parent, selected)
    }
    #[test]
    fn private_slot_log_late_poison_rejects_before_any_earlier_slot_write() {
        let mut s = Session::new_macro().unwrap();
        let op = s.operation_budget_scope().unwrap();
        let replacement = number(&mut s, 17.0);
        let (log, token, parent, _) = two_slot_fixture(&mut s, true);
        let physical = log.borrow().slots[0].array.clone();
        assert_eq!(log.borrow().slots.len(), 2);
        assert!(physical_slot_is(&mut s, &physical, 0, &token));
        // Poison the LATER physical slot after successful complete capture.
        set_slot(&mut s, &physical, 1, &replacement);
        s.collect().unwrap();
        let error = log.borrow().apply(&mut s, &parent, &[(&token, &replacement)]).unwrap_err();
        assert!(matches!(error, SessionError::Host(_)), "{error:?}");
        // Staging must reject before writing even the earlier valid location.
        assert!(physical_slot_is(&mut s, &physical, 0, &token));
        assert!(physical_slot_is(&mut s, &physical, 1, &replacement));
        set_slot(&mut s, &physical, 1, &token);
        assert_eq!(log.borrow().apply(&mut s, &parent, &[(&token, &replacement)]).unwrap(), 2);
        assert!(physical_slot_is(&mut s, &physical, 0, &replacement));
        assert!(physical_slot_is(&mut s, &physical, 1, &replacement));
        drop(op);
    }
    fn measured_operation(s: &mut Session, capture: bool, replacement: &SessionValue) -> u64 {
        let op = s.operation_budget_scope().unwrap();
        let (log, token, parent, selected) = two_slot_fixture(s, !capture);
        let before = s.operation_fuel_remaining().unwrap();
        if capture {
            log.borrow_mut().capture_dense(s, &parent, 2, &selected).unwrap();
        } else {
            assert_eq!(log.borrow().apply(s, &parent, &[(&token, replacement)]).unwrap(), 2);
        }
        let cost = before - s.operation_fuel_remaining().unwrap();
        drop(op);
        cost
    }
    #[test]
    fn private_slot_log_every_small_capture_apply_fuel_cut_discards_private_state_and_recovers() {
        let mut s = Session::new_macro().unwrap();
        let leaf = number(&mut s, 91.0);
        let public_source = s.data_array(&[&leaf]).unwrap();
        let get = s.eval("(fn [a] (aget a 0))").unwrap();
        for capture in [true, false] {
            let cost = measured_operation(&mut s, capture, &public_source);
            let handles = s.stats().external_value_handles;
            let mut failures = 0;
            let mut partial_private_writes = 0;
            for allowance in 0..=cost {
                let op = s.operation_budget_scope().unwrap();
                let (log, token, parent, selected) = two_slot_fixture(&mut s, !capture);
                s.store.set_fuel(allowance).unwrap();
                let result = if capture {
                    log.borrow_mut().capture_dense(&mut s, &parent, 2, &selected)
                } else {
                    log.borrow().apply(&mut s, &parent, &[(&token, &public_source)]).map(|_| ())
                };
                // Apply target is the same owned public array used in measurement;
                // its payload must never be traversed or written.
                let failed = result.is_err();
                match result {
                    Ok(()) => assert_eq!(allowance, cost, "unexpected earlier success capture={capture} allowance={allowance} cost={cost}"),
                    Err(error) => {
                        failures += 1;
                        assert!(matches!(error, SessionError::Trap(ref error)
                            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                            "capture={capture} allowance={allowance}: {error:?}");
                    }
                }
                drop(op);
                if !capture && failed {
                    let read = s.operation_budget_scope().unwrap();
                    let physical = log.borrow().slots[0].array.clone();
                    if physical_slot_is(&mut s, &physical, 0, &public_source)
                        && physical_slot_is(&mut s, &physical, 1, &token)
                    { partial_private_writes += 1; }
                    drop(physical); drop(read);
                }
                // Any private write-prefix is intentionally discarded: there
                // is NO all-or-nothing guarantee for WRITE-phase interruption.
                drop(selected); drop(parent); drop(token); drop(log);
                assert_eq!(s.stats().external_value_handles, handles);
                let observed = s.invoke(&get, &[&public_source]).unwrap();
                assert!(same_value(&mut s, &observed, &leaf));
                drop(observed);
                let retry = s.operation_budget_scope().unwrap();
                let (log, token, parent, selected) = two_slot_fixture(&mut s, false);
                log.borrow_mut().capture_dense(&mut s, &parent, 2, &selected).unwrap();
                assert_eq!(log.borrow().apply(&mut s, &parent, &[(&token, &public_source)]).unwrap(), 2);
                drop(selected); drop(parent); drop(token); drop(log); drop(retry);
                assert_eq!(s.stats().external_value_handles, handles);
            }
            assert_eq!(failures, cost, "all insufficient integer allowances must fail capture={capture}");
            if !capture { assert!(partial_private_writes > 0, "write-phase cut must exercise unpublished partial state"); }
            // Deterministic IRQ at operation entry, after budget installation.
            // This is NOT an exhaustive mid-operation IRQ-boundary sweep.
            let op = s.operation_budget_scope().unwrap();
            let (log, token, parent, selected) = two_slot_fixture(&mut s, !capture);
            s.interrupt_handle().interrupt();
            let error = if capture {
                log.borrow_mut().capture_dense(&mut s, &parent, 2, &selected).unwrap_err()
            } else {
                log.borrow().apply(&mut s, &parent, &[(&token, &public_source)]).unwrap_err()
            };
            assert!(error.is_interrupt(), "{error:?}");
            drop(op); drop(selected); drop(parent); drop(token); drop(log);
            assert_eq!(s.stats().external_value_handles, handles);
            let retry = s.operation_budget_scope().unwrap();
            let (log, token, parent, selected) = two_slot_fixture(&mut s, true);
            assert_eq!(log.borrow().apply(&mut s, &parent, &[(&token, &public_source)]).unwrap(), 2);
            drop(selected); drop(parent); drop(token); drop(log); drop(retry);
            s.collect().unwrap();
            let observed = s.invoke(&get, &[&public_source]).unwrap();
            assert!(same_value(&mut s, &observed, &leaf));
            drop(observed);
            assert_eq!(s.stats().external_value_handles, handles);
            eprintln!("private-slot-cuts capture={capture} measured={cost} insufficient-allowances={failures} private-write-prefixes={partial_private_writes} irq=entry-only");
        }
    }


    #[test]
    fn private_slot_owner_invariant_many_tokens_gc_and_foreign_boundaries() {
        let mut s = Session::new_macro().unwrap();
        let op = s.operation_budget_scope().unwrap();
        let log = PrivateSlotLog::new(&mut s).unwrap();
        let mut tokens = Vec::new();
        for id in 0..32 { tokens.push(log.borrow_mut().token(&mut s, id).unwrap()); }
        let leaf = number(&mut s, 17.0);
        let unrelated_empty = s.data_array(&[]).unwrap();
        let unrelated_nonempty = s.data_array(&[&leaf]).unwrap();
        // Adversarial exposure is test-only: even a genuine token whose logical
        // length/backing is mutated must remain selected by exact identity.
        let setter = s.eval("(fn [a x] (aset a 0 x))").unwrap();
        s.invoke(&setter, &[&tokens[31], &leaf]).unwrap();
        let getter = s.eval("(fn [a] (aget a 0))").unwrap();
        let observed = s.invoke(&getter, &[&tokens[31]]).unwrap();
        assert!(same_value(&mut s, &observed, &leaf));
        drop(observed);
        s.collect().unwrap();
        let identity = s.identity;
        let handles = s.handles.clone();
        let raw_fields = {
            let mut roots = RootScope::new(&mut s.store);
            let raw = tokens[0].value.to_rooted(&mut roots);
            let object = raw.as_struct(&roots).unwrap().unwrap();
            let fields = object.field(&mut roots, 1).unwrap();
            own(&mut roots, fields, identity, &handles).unwrap()
        };
        let nil = s.data_scalar(&crate::portable::hir::Literal::Nil).unwrap();
        let args = [&leaf, &unrelated_empty, &tokens[31], &unrelated_nonempty, &tokens[0], &tokens[31], &raw_fields, &nil];
        assert_eq!(log.borrow().inputs(&mut s, &args).unwrap(), vec![(2,31),(4,0),(5,31)]);
        assert_eq!(log.borrow().inputs(&mut s, &args).unwrap(),
            log.borrow().inputs_linear_owner_control(&mut s, &args).unwrap());
        let count = log.borrow().tokens.len();
        assert!(log.borrow_mut().register_created(&mut s, 32, &leaf).is_err());
        assert!(log.borrow_mut().register_created(&mut s, 32, &raw_fields).is_err());
        assert!(log.borrow_mut().register_created(&mut s, 32, &tokens[0]).is_err());
        assert!(log.borrow_mut().register_created(&mut s, 0, &unrelated_empty).is_err());
        assert_eq!(log.borrow().tokens.len(), count);
        let mut foreign = Session::new_macro().unwrap();
        let foreign_value = foreign.data_array(&[]).unwrap();
        assert!(matches!(log.borrow_mut().register_created(&mut s, 32, &foreign_value), Err(SessionError::ForeignValue)));
        assert!(log.borrow_mut().register_created(&mut foreign, 32, &foreign_value).is_err());
        // A preceding negative must not conceal a later foreign input.
        assert!(matches!(log.borrow().inputs(&mut s, &[&leaf, &foreign_value]), Err(SessionError::ForeignValue)));
        assert_eq!(log.borrow().tokens.len(), count);
        assert_eq!(log.borrow().inputs(&mut s, &args).unwrap(), vec![(2,31),(4,0),(5,31)]);
        drop(op);
        s.reset().unwrap();
        assert!(log.borrow().inputs(&mut s, &[]).is_err());
        let fresh = s.data_array(&[]).unwrap();
        assert!(log.borrow_mut().register_created(&mut s, 32, &fresh).is_err());
        assert_eq!(log.borrow().tokens.len(), count);
    }
    #[test]
    fn private_slot_owner_scan_inputs_fuel_cuts_irq_and_retry() {
        let mut s = Session::new_macro().unwrap();
        let op = s.operation_budget_scope().unwrap();
        let log = PrivateSlotLog::new(&mut s).unwrap();
        let mut tokens = Vec::new();
        for id in 0..8 { tokens.push(log.borrow_mut().token(&mut s, id).unwrap()); }
        let leaf = number(&mut s, 23.0);
        let empty = s.data_array(&[]).unwrap();
        let args = [&leaf, &tokens[7], &empty, &tokens[0]];
        let before = s.operation_fuel_remaining().unwrap();
        assert_eq!(log.borrow().inputs(&mut s, &args).unwrap(), vec![(1,7),(3,0)]);
        let cost = before - s.operation_fuel_remaining().unwrap();
        drop(op);
        let handles = s.stats().external_value_handles;
        for allowance in 0..=cost {
            let op = s.operation_budget_scope().unwrap();
            s.store.set_fuel(allowance).unwrap();
            match log.borrow().inputs(&mut s, &args) {
                Ok(value) => { assert_eq!(allowance, cost); assert_eq!(value, vec![(1,7),(3,0)]); }
                Err(error) => assert!(matches!(error, SessionError::Trap(ref e)
                    if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}"),
            }
            drop(op);
            assert_eq!(s.stats().external_value_handles, handles);
            let retry = s.operation_budget_scope().unwrap();
            assert_eq!(log.borrow().inputs(&mut s, &args).unwrap(), vec![(1,7),(3,0)]);
            assert_eq!(log.borrow().tokens.len(), 8);
            drop(retry);
        }
        let op = s.operation_budget_scope().unwrap();
        s.interrupt_handle().interrupt();
        assert!(log.borrow().inputs(&mut s, &args).unwrap_err().is_interrupt());
        drop(op);
        let retry = s.operation_budget_scope().unwrap();
        s.collect().unwrap();
        assert_eq!(log.borrow().inputs(&mut s, &args).unwrap(), vec![(1,7),(3,0)]);
        drop(retry);
        assert_eq!(s.stats().external_value_handles, handles);
    }


    #[test]
    fn private_slot_immutable_shape_filter_wide_work_control() {
        let mut s = Session::new_macro().unwrap();
        let op = s.operation_budget_scope().unwrap();
        let log = PrivateSlotLog::new(&mut s).unwrap();
        let mut tokens = Vec::new();
        for id in 0..64 { tokens.push(log.borrow_mut().token(&mut s, id).unwrap()); }
        let leaf = number(&mut s, 23.0);
        let unrelated_empty = s.data_array(&[]).unwrap();
        let unrelated_nonempty = s.data_array(&[&leaf]).unwrap();
        s.collect().unwrap();
        let mut args = vec![&leaf; 96];
        args.extend([&tokens[63], &unrelated_empty, &unrelated_nonempty, &tokens[0]]);
        let before = s.operation_fuel_remaining().unwrap();
        let old = log.borrow().inputs_linear_owner_control(&mut s, &args).unwrap();
        let old_cost = before - s.operation_fuel_remaining().unwrap();
        let before = s.operation_fuel_remaining().unwrap();
        let new = log.borrow().inputs(&mut s, &args).unwrap();
        let new_cost = before - s.operation_fuel_remaining().unwrap();
        assert_eq!(old, vec![(96,63),(99,0)]);
        assert_eq!(new, old);
        eprintln!("private-slot-immutable-shape case=wide tokens=64 inputs=100 legacy={old_cost} candidate={new_cost}");
        assert!(new_cost < old_cost, "wide native work must improve: legacy={old_cost} candidate={new_cost}");
        // T1/T2 retain linear ref_eq with only fixed dispatch overhead;
        // per-input shape classification runs only on the large-token route.
        // Report the small fixed tradeoff; no universal saving is promised.
        for token_count in [0usize, 1, 2] {
            let small = PrivateSlotLog::new(&mut s).unwrap();
            let mut small_tokens = Vec::new();
            for id in 0..token_count {
                small_tokens.push(small.borrow_mut().token(&mut s, 901 + id).unwrap());
            }
            let mut previous_delta = None;
            for width in [8usize, 96] {
                let mut compatible = vec![&unrelated_empty; width / 2];
                compatible.extend(vec![&unrelated_nonempty; width / 2]);
                for token in &small_tokens { compatible.push(token); }
                s.collect().unwrap();
                let before = s.operation_fuel_remaining().unwrap();
                let old = small.borrow().inputs_linear_owner_control(&mut s, &compatible).unwrap();
                let old_cost = before - s.operation_fuel_remaining().unwrap();
                let before = s.operation_fuel_remaining().unwrap();
                let new = small.borrow().inputs(&mut s, &compatible).unwrap();
                let new_cost = before - s.operation_fuel_remaining().unwrap();
                let expected = (0..token_count).map(|i| ((width + i) as u32, 901 + i)).collect::<Vec<_>>();
                assert_eq!(old, expected);
                assert_eq!(new, old);
                let delta = new_cost as i128 - old_cost as i128;
                if token_count > 0 {
                    // Only fixed dispatch (3) minus the removed owner rescan
                    // (T) differs: no per-input classification on this route.
                    assert_eq!(delta, 3 - token_count as i128);
                    if let Some(previous) = previous_delta { assert_eq!(delta, previous); }
                    previous_delta = Some(delta);
                } else {
                    // Owner checks still traverse every input; only the old
                    // pointless root/scan loop is absent for zero tokens.
                    assert_eq!(delta, 3 - width as i128);
                    assert!(new_cost < old_cost);
                }
                eprintln!("private-slot-immutable-shape case=compatible tokens={token_count} width={width} inputs={} selected={} legacy={old_cost} candidate={new_cost} delta={delta}", compatible.len(), new.len());
            }
        }
        drop(op);
    }

}

impl PrivateSlotLog {
    pub(crate) fn new(s: &mut Session) -> Result<SharedSlotLog> {
        s.charge_host_work((mem::size_of::<RefCell<Self>>() + 2 * mem::size_of::<usize>()) as u64)?;
        Ok(Rc::new(RefCell::new(Self {
            tokens: Vec::new(),
            slots: Vec::new(),
            complete: true,
            identity: s.identity,
        })))
    }
    pub(crate) fn complete(&self) -> bool {
        self.complete
    }
    // Explicit unsupported-path marker, never an error handler.
    #[allow(dead_code)]
    pub(crate) fn unsupported(&mut self) {
        self.complete = false;
    }
    pub(crate) fn token(&mut self, s: &mut Session, id: usize) -> Result<SessionValue> {
        self.check_session(s)?;
        let value = s.data_array(&[])?;
        self.register_created(s, id, &value)?;
        Ok(value)
    }
    fn check_session(&self, s: &mut Session) -> Result<()> {
        s.charge_host_work(1)?;
        if s.identity != self.identity {
            return Err(invalid("Foreign private slot log"));
        }
        Ok(())
    }
    // Negative identity filter only. Intrinsic GC kind and struct field count
    // cannot change for an owned object. Never inspect mutable logical length,
    // descriptor, class, backing or payload to decide token identity.
    fn token_struct_shape(m: &mut Meter<'_>, value: &Rooted<AnyRef>) -> Result<bool> {
        m.charge(2)?; // as_struct and Option branch, before inspection.
        let Some(object) = value.as_struct(&m.store)? else { return Ok(false); };
        m.charge(3)?; // intrinsic type query, field-count read and comparison.
        Ok(object.ty(&m.store)?.fields().len() == 4)
    }
    fn register_created(&mut self, s: &mut Session, id: usize, value: &SessionValue) -> Result<()> {
        self.check_session(s)?;
        s.check(value)?;
        let irq = s.host_work_interrupt();
        let mut scope = RootScope::new(&mut s.store);
        let mut m = Meter {
            store: scope.as_context_mut(),
            interrupt: &irq,
        };
        let v = value.value.to_rooted(&mut m.store);
        if !Self::token_struct_shape(&mut m, &v)? {
            return Err(invalid("Private slot token needs four-field OBJECT storage"));
        }
        for t in &self.tokens {
            m.charge(2)?;
            let old = t.value.value.to_rooted(&mut m.store);
            if t.id == id || m.same(&v, &old)? {
                return Err(invalid("Duplicate private slot token"));
            }
        }
        m.reserve(&mut self.tokens, 1)?;
        m.charge(2)?;
        self.tokens.push(Token {
            id,
            value: value.clone(),
        });
        Ok(())
    }
    pub(crate) fn inputs(
        &self,
        s: &mut Session,
        values: &[&SessionValue],
    ) -> Result<Vec<(u32, usize)>> {
        self.check_session(s)?;
        for v in values {
            s.charge_host_work(1)?;
            s.check(v)?;
        }
        // register_created is the sole publisher: log Session and value owner
        // were both checked before retain. SessionValue.identity is immutable;
        // clone preserves it, GC never changes it, and reset replaces Session
        // identity so check_session above rejects this old log before rooting.
        // No guest callbacks occur between this check and the native scan.
        // This removes repeated ownership work only, NOT token comparisons.
        // Prepay length read, small/large decision and route dispatch once.
        // No error retry; all input owners were checked before this decision.
        s.charge_host_work(3)?;
        match self.tokens.len() {
            0 => Ok(Vec::new()),
            1 | 2 => self.select_inputs::<false>(s, values),
            _ => self.select_inputs::<true>(s, values),
        }
    }
    fn select_inputs<const FILTER: bool>(
        &self,
        s: &mut Session,
        values: &[&SessionValue],
    ) -> Result<Vec<(u32, usize)>> {
        let irq = s.host_work_interrupt();
        let mut scope = RootScope::new(&mut s.store);
        let mut m = Meter {
            store: scope.as_context_mut(),
            interrupt: &irq,
        };
        let mut selected = Vec::new();
        for (i, v) in values.iter().enumerate() {
            m.charge(1)?;
            let actual = v.value.to_rooted(&mut m.store);
            if FILTER && !Self::token_struct_shape(&mut m, &actual)? { continue; }
            for t in &self.tokens {
                m.charge(1)?;
                let expected = t.value.value.to_rooted(&mut m.store);
                if m.same(&actual, &expected)? {
                    m.reserve(&mut selected, 1)?;
                    m.charge(1)?;
                    selected.push((
                        u32::try_from(i).map_err(|_| invalid("Private slot index overflow"))?,
                        t.id,
                    ));
                    break;
                }
            }
        }
        Ok(selected)
    }
    #[cfg(test)]
    fn inputs_linear_owner_control(
        &self,
        s: &mut Session,
        values: &[&SessionValue],
    ) -> Result<Vec<(u32, usize)>> {
        self.check_session(s)?;
        for v in values {
            s.charge_host_work(1)?;
            s.check(v)?;
        }
        for t in &self.tokens {
            s.charge_host_work(1)?;
            s.check(&t.value)?;
        }
        let irq = s.host_work_interrupt();
        let mut scope = RootScope::new(&mut s.store);
        let mut m = Meter {
            store: scope.as_context_mut(),
            interrupt: &irq,
        };
        let mut selected = Vec::new();
        for (i, v) in values.iter().enumerate() {
            m.charge(1)?;
            let actual = v.value.to_rooted(&mut m.store);
            for t in &self.tokens {
                m.charge(1)?;
                let expected = t.value.value.to_rooted(&mut m.store);
                if m.same(&actual, &expected)? {
                    m.reserve(&mut selected, 1)?;
                    m.charge(1)?;
                    selected.push((
                        u32::try_from(i).map_err(|_| invalid("Private slot index overflow"))?,
                        t.id,
                    ));
                    break;
                }
            }
        }
        Ok(selected)
    }
    fn capture(
        &mut self,
        s: &mut Session,
        value: &SessionValue,
        dense: Option<usize>,
        selected: &[(u32, usize)],
    ) -> Result<()> {
        self.check_session(s)?;
        if selected.is_empty() {
            return Ok(());
        }
        s.charge_host_work(1)?;
        s.check(value)?;
        for t in &self.tokens {
            s.charge_host_work(1)?;
            s.check(&t.value)?;
        }
        let identity = s.identity;
        let handles = s.handles.clone();
        let irq = s.host_work_interrupt();
        let mut scope = RootScope::new(&mut s.store);
        let mut m = Meter {
            store: scope.as_context_mut(),
            interrupt: &irq,
        };
        // Prepay token lookup/root, OBJECT cast/type, field read, ARGS cast/type.
        m.charge(12)?;
        let expected = self
            .tokens
            .first()
            .ok_or_else(|| invalid("Missing private tokens"))?
            .value
            .value
            .to_rooted(&mut m.store);
        let sample = expected
            .as_struct(&m.store)?
            .ok_or_else(|| invalid("Private token is not OBJECT"))?;
        let object_type = sample.ty(&m.store)?;
        let args = sample
            .field(&mut m.store, 1)?
            .unwrap_anyref()
            .copied()
            .ok_or_else(|| invalid("Missing token fields"))?
            .as_array(&m.store)?
            .ok_or_else(|| invalid("Token fields not ARGS"))?;
        let args_type = args.ty(&m.store)?;
        // Output root/cast/type, field read/cast/type and both type comparisons.
        m.charge(12)?;
        let result = value.value.to_rooted(&mut m.store);
        let object = result
            .as_struct(&m.store)?
            .ok_or_else(|| invalid("Private factory did not return OBJECT"))?;
        if !StructType::eq(&object.ty(&m.store)?, &object_type) {
            return Err(invalid("Private factory OBJECT type mismatch"));
        }
        let field = object.field(&mut m.store, 1)?;
        let fields = field
            .unwrap_anyref()
            .copied()
            .ok_or_else(|| invalid("Missing private factory fields"))?
            .as_array(&m.store)?
            .ok_or_else(|| invalid("Private factory fields not ARGS"))?;
        if !ArrayType::eq(&fields.ty(&m.store)?, &args_type) {
            return Err(invalid("Private factory ARGS type mismatch"));
        }
        let array = if let Some(length) = dense {
            m.charge(32)?; // Fixed dense-owner/spine reads, casts, identity and width checks.
            if fields.len(&m.store)? != 2 {
                return Err(invalid("Private dense owner width"));
            }
            let descriptor = object
                .field(&mut m.store, 0)?
                .unwrap_anyref()
                .copied()
                .ok_or_else(|| invalid("Missing private array descriptor"))?;
            let expected = sample
                .field(&mut m.store, 0)?
                .unwrap_anyref()
                .copied()
                .ok_or_else(|| invalid("Missing token descriptor"))?;
            if !m.same(&descriptor, &expected)? {
                return Err(invalid("Private dense descriptor mismatch"));
            }
            let backing = fields
                .get(&mut m.store, 0)?
                .unwrap_anyref()
                .copied()
                .ok_or_else(|| invalid("Missing private backing"))?
                .as_array(&m.store)?
                .ok_or_else(|| invalid("Private backing not ARGS"))?;
            if !ArrayType::eq(&backing.ty(&m.store)?, &args_type) || backing.len(&m.store)? != 4 {
                return Err(invalid("Private backing layout"));
            }
            let head = backing.get(&mut m.store, 1)?;
            if !matches!(head,Val::AnyRef(Some(v)) if v.as_i31(&m.store)?.is_some_and(|i|i.get_u32()==0))
            {
                return Err(invalid("Private dense backing is not fresh"));
            }
            let dense = backing
                .get(&mut m.store, 2)?
                .unwrap_anyref()
                .copied()
                .ok_or_else(|| invalid("Missing private dense storage"))?
                .as_array(&m.store)?
                .ok_or_else(|| invalid("Private dense storage not ARGS"))?;
            if !ArrayType::eq(&dense.ty(&m.store)?, &args_type)
                || dense.len(&m.store)? as usize != length
            {
                return Err(invalid("Private dense length/type mismatch"));
            }
            dense
        } else {
            fields
        };
        // Own only FINAL factory storage. Nominal normalization can move/copy tokens.
        let raw = Val::AnyRef(Some(array.to_anyref()));
        m.charge(mem::size_of::<SessionValue>() as u64 + 1)?;
        let owner = {
            let mut roots = RootScope::new(&mut m.store);
            own(&mut roots, raw, identity, &handles)?
        };
        let mut staged = Vec::new();
        if dense.is_some() {
            for &(index, id) in selected {
                m.charge(2)?;
                if index >= array.len(&m.store)? {
                    return Err(invalid("Private dense index out of bounds"));
                }
                let actual = array
                    .get(&mut m.store, index)?
                    .unwrap_anyref()
                    .copied()
                    .ok_or_else(|| invalid("Private dense token missing"))?;
                let mut token = None;
                for candidate in &self.tokens {
                    m.charge(1)?;
                    if candidate.id == id {
                        token = Some(candidate);
                        break;
                    }
                }
                let token = token.ok_or_else(|| invalid("Unknown private token"))?;
                m.charge(1)?;
                let expected = token.value.value.to_rooted(&mut m.store);
                if !m.same(&actual, &expected)? {
                    return Err(invalid("Private dense copy changed token"));
                }
                m.reserve(&mut staged, 1)?;
                m.charge(3)?;
                staged.push(Slot {
                    array: owner.clone(),
                    index,
                    token: id,
                });
            }
        } else {
            for index in 0..array.len(&m.store)? {
                m.charge(2)?;
                let v = array.get(&mut m.store, index)?;
                if let Val::AnyRef(Some(actual)) = v {
                    for t in &self.tokens {
                        m.charge(1)?;
                        let expected = t.value.value.to_rooted(&mut m.store);
                        if m.same(&actual, &expected)? {
                            m.reserve(&mut staged, 1)?;
                            m.charge(3)?;
                            staged.push(Slot {
                                array: owner.clone(),
                                index,
                                token: t.id,
                            });
                            break;
                        }
                    }
                }
            }
        }
        m.reserve(&mut self.slots, staged.len())?;
        m.charge(staged.len() as u64)?;
        self.slots.extend(staged);
        Ok(())
    }
    pub(crate) fn capture_dense(
        &mut self,
        s: &mut Session,
        v: &SessionValue,
        n: usize,
        selected: &[(u32, usize)],
    ) -> Result<()> {
        self.capture(s, v, Some(n), selected)
    }
    pub(crate) fn capture_nominal(
        &mut self,
        s: &mut Session,
        v: &SessionValue,
        selected: &[(u32, usize)],
    ) -> Result<()> {
        self.capture(s, v, None, selected)
    }
    pub(crate) fn apply(
        &self,
        s: &mut Session,
        root: &SessionValue,
        pairs: &[(&SessionValue, &SessionValue)],
    ) -> Result<usize> {
        self.check_session(s)?;
        s.charge_host_work(1)?;
        s.check(root)?;
        for t in &self.tokens {
            s.charge_host_work(1)?;
            s.check(&t.value)?;
        }
        for (a, b) in pairs {
            s.charge_host_work(2)?;
            s.check(a)?;
            s.check(b)?;
        }
        for slot in &self.slots {
            s.charge_host_work(1)?;
            s.check(&slot.array)?;
        }
        if !self.complete {
            return Err(invalid("Incomplete private slot log"));
        }
        if pairs.len() != self.tokens.len() {
            return Err(invalid("Private slot bindings incomplete"));
        }
        if pairs.is_empty() {
            return Ok(0);
        }
        let irq = s.host_work_interrupt();
        let mut scope = RootScope::new(&mut s.store);
        let mut m = Meter {
            store: scope.as_context_mut(),
            interrupt: &irq,
        };
        let mut bindings = Vec::new();
        m.reserve(&mut bindings, self.tokens.len())?;
        for token in &self.tokens {
            m.charge(2)?;
            let expected = token.value.value.to_rooted(&mut m.store);
            let mut found = None;
            for (a, b) in pairs {
                m.charge(3)?;
                let actual = a.value.to_rooted(&mut m.store);
                if m.same(&expected, &actual)? {
                    if found.is_some() {
                        return Err(invalid("Duplicate private binding"));
                    }
                    found = Some(b.value.to_rooted(&mut m.store));
                }
            }
            let target = found.ok_or_else(|| invalid("Private token missing binding"))?;
            for other in &self.tokens {
                m.charge(1)?;
                let other = other.value.value.to_rooted(&mut m.store);
                if m.same(&target, &other)? {
                    return Err(invalid("Private target overlaps tokens"));
                }
            }
            m.charge(2)?;
            let root = root.value.to_rooted(&mut m.store);
            if m.same(&root, &expected)? {
                return Err(invalid("Private fixup root is token"));
            }
            m.charge(1)?;
            bindings.push((token.id, expected, target));
        }
        m.charge(8)?; // Sample lookup/cast, storage field read/cast/type.
        let sample = bindings[0]
            .1
            .as_struct(&m.store)?
            .ok_or_else(|| invalid("Token not OBJECT"))?;
        let array = sample
            .field(&mut m.store, 1)?
            .unwrap_anyref()
            .copied()
            .ok_or_else(|| invalid("Missing token storage"))?
            .as_array(&m.store)?
            .ok_or_else(|| invalid("Token storage not ARGS"))?;
        let ty = array.ty(&m.store)?;
        let mut writes: Vec<(Rooted<ArrayRef>, u32, Rooted<AnyRef>)> = Vec::new();
        m.reserve(&mut writes, self.slots.len())?;
        for slot in &self.slots {
            m.charge(3)?;
            let raw = slot.array.value.to_rooted(&mut m.store);
            let array = raw
                .as_array(&m.store)?
                .ok_or_else(|| invalid("Private slot owner not array"))?;
            m.charge(5)?;
            if !ArrayType::eq(&array.ty(&m.store)?, &ty) || slot.index >= array.len(&m.store)? {
                return Err(invalid("Private slot type/bounds changed"));
            }
            let mut binding = None;
            for candidate in &bindings {
                m.charge(1)?;
                if candidate.0 == slot.token {
                    binding = Some(candidate);
                    break;
                }
            }
            let (_, token, target) =
                binding.ok_or_else(|| invalid("Unknown private slot token"))?;
            let actual = array
                .get(&mut m.store, slot.index)?
                .unwrap_anyref()
                .copied()
                .ok_or_else(|| invalid("Private slot token replaced"))?;
            if !m.same(&actual, token)? {
                return Err(invalid("Private slot token changed"));
            }
            for (prior, index, _) in &writes {
                m.charge(3)?;
                if *index == slot.index && m.same(&prior.to_anyref(), &array.to_anyref())? {
                    return Err(invalid("Duplicate private slot location"));
                }
            }
            m.charge(1)?;
            writes.push((array, slot.index, *target));
        }
        for (array, index, target) in &writes {
            m.charge(1)?;
            array.set(&mut m.store, *index, Val::AnyRef(Some(*target)))?;
        }
        Ok(writes.len())
    }
}
