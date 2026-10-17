//! Immutable factory expectations and exact physical scalar storage witnesses.
//! The owner contains no domain, record, or Watch backpointer.
use super::{Session, SessionError, SessionValue};
use crate::portable::hir::Literal;
use std::sync::{Arc, atomic::AtomicBool};
use wasmtime::{AnyRef, ArrayType, AsContextMut, RootScope, Rooted, StoreContextMut, StructType, Val};

type Result<T> = std::result::Result<T, SessionError>;
// Expectations are completely prepared before the guest factory allocation.
enum Prepared { Sentinel(u32), Number(u64), String(Vec<u16>) }
#[derive(Debug)]
enum Expected {
    Sentinel(u32),
    Number(StructType, u64),
    String(ArrayType, Vec<u16>),
}
#[derive(Debug)]
pub(crate) struct ScalarInputs {
    receiver: SessionValue,
    expected: Expected,
}
fn invalid(message: &'static str) -> SessionError {
    SessionError::Host(wasmtime::Error::msg(message))
}
impl Session {
    pub(crate) fn data_scalar_with_inputs(
        &mut self, literal: &Literal,
    ) -> Result<(SessionValue, Arc<ScalarInputs>)> {
        let _request = self.operation_budget_scope()?;
        self.charge_host_work(1)?;
        let prepared = match literal {
            Literal::Nil => Prepared::Sentinel(0),
            Literal::Bool(false) => Prepared::Sentinel(2),
            Literal::Bool(true) => Prepared::Sentinel(4),
            Literal::Number(value) => Prepared::Number(value.to_bits()),
            Literal::String(units) => {
                let bytes = units.len().checked_mul(std::mem::size_of::<u16>())
                    .and_then(|v| u64::try_from(v).ok())
                    .ok_or_else(|| invalid("Scalar input allocation overflow"))?;
                // Copy, backing, and eventual buffer release are prepaid.
                self.charge_host_work(bytes.checked_add(2).ok_or_else(|| invalid("Scalar input allocation overflow"))?)?;
                let mut expected = Vec::new();
                expected.try_reserve_exact(units.len())
                    .map_err(|_| invalid("Scalar input allocation failed"))?;
                for unit in units { self.charge_host_work(1)?; expected.push(*unit); }
                Prepared::String(expected)
            }
            Literal::Undefined => return self.data_scalar(literal).and_then(|_| {
                Err(invalid("Undefined compiler literal unexpectedly accepted"))
            }),
        };
        let value = self.data_scalar(literal)?;
        // Exact factory type capture/root inspection, retain and release work.
        // No user code runs between the sealed allocation and this capture.
        self.charge_host_work(6)?;
        let expected = {
            let mut scope = RootScope::new(&mut self.store);
            let root = value.value.to_rooted(&mut scope);
            match prepared {
                Prepared::Sentinel(tag) => Expected::Sentinel(tag),
                Prepared::Number(bits) => {
                    let object = root.as_struct(&scope)?.ok_or_else(|| invalid("Scalar number needs boxed storage"))?;
                    let ty = object.ty(&scope)?;
                    if ty.fields().len() != 1 { return Err(invalid("Scalar number factory has extra fields")); }
                    Expected::Number(ty, bits)
                }
                Prepared::String(units) => {
                    let array = root.as_array(&scope)?.ok_or_else(|| invalid("Scalar string needs UTF-16 storage"))?;
                    Expected::String(array.ty(&scope)?, units)
                }
            }
        };
        // All factories pay, including facts never enrolled in a domain. The
        // Arc/header, receiver retain and bounded final owner cleanup are real.
        let bytes = std::mem::size_of::<ScalarInputs>().checked_add(2 * std::mem::size_of::<usize>())
            .and_then(|n| n.checked_add(5)).ok_or_else(|| invalid("Scalar owner allocation overflow"))?;
        self.charge_host_work(bytes as u64)?;
        let facts = Arc::new(ScalarInputs { receiver: value.clone(), expected });
        if !facts.matches(self, &value)? { return Err(invalid("Scalar factory did not preserve its literal")); }
        Ok((value, facts))
    }
}
impl ScalarInputs {
    pub(super) fn receiver(&self) -> &SessionValue { &self.receiver }
    pub(super) fn check_owner(&self, identity: u64) -> Result<()> {
        if self.receiver.identity != identity { Err(SessionError::ForeignValue) } else { Ok(()) }
    }
    pub(crate) fn matches(&self, session: &mut Session, value: &SessionValue) -> Result<bool> {
        // Preserve direct-fact owner checks BEFORE the operation poll.
        session.check(value)?;
        session.check(&self.receiver)?;
        let _request = session.operation_budget_scope()?;
        // Preserve the original direct-fact cancellation boundary: poll BEFORE
        // rooting either value. The context matcher also polls its own entry.
        session.charge_host_work(1)?;
        let irq = session.host_work_interrupt();
        let identity = session.identity;
        let mut scope = RootScope::new(&mut session.store);
        let root = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
        self.matches_in_context(scope.as_context_mut(), root, identity, &irq, true)
    }
    pub(super) fn matches_in_context(
        &self, mut store: StoreContextMut<'_, ()>, value: Val,
        identity: u64, irq: &AtomicBool, direct: bool,
    ) -> Result<bool> {
        // Poll before every false/sentinel path. No guest lookup or callback.
        Session::consume_host_work(store.as_context_mut(), 1, irq)?;
        self.check_owner(identity)?;
        let mut scope = RootScope::new(store);
        let Some(found) = value.unwrap_anyref() else { return Ok(false); };
        let receiver = self.receiver.value.to_rooted(&mut scope);
        if !Rooted::<AnyRef>::ref_eq(&scope, found, &receiver)? { return Ok(false); }
        match &self.expected {
            Expected::Sentinel(expected) => Ok(found.as_i31(&scope)?.is_some_and(|v| v.get_u32() == *expected)),
            Expected::Number(ty, expected) => {
                Session::consume_host_work(scope.as_context_mut(), 2, irq)?;
                let Some(object) = found.as_struct(&scope)? else {
                    return if direct { Err(invalid("Scalar number needs boxed storage")) } else { Ok(false) };
                };
                if !StructType::eq(&object.ty(&scope)?, ty) { return Ok(false); }
                Ok(object.field(&mut scope, 0)?.unwrap_f64().to_bits() == *expected)
            }
            Expected::String(ty, expected) => {
                Session::consume_host_work(scope.as_context_mut(), 2, irq)?;
                let Some(array) = found.as_array(&scope)? else {
                    return if direct { Err(invalid("Scalar string needs UTF-16 storage")) } else { Ok(false) };
                };
                if !ArrayType::eq(&array.ty(&scope)?, ty) || array.len(&scope)? as usize != expected.len() { return Ok(false); }
                for (index, expected) in expected.iter().enumerate() {
                    Session::consume_host_work(scope.as_context_mut(), 1, irq)?;
                    if array.get(&mut scope, index as u32)?.unwrap_i32() as u16 != *expected { return Ok(false); }
                }
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::resolve::Phase;
    use wasmtime::Val;
    fn session(phase: Phase) -> Session {
        match phase {
            Phase::Runtime => Session::new_repl().unwrap(),
            Phase::Macro => Session::new_macro().unwrap(),
        }
    }
    #[test]
    fn scalar_factory_owner_preparation_cancellation_releases_unenrolled_facts() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            for literal in [Literal::Nil, Literal::Number(-0.0), Literal::String(vec![0, 0xd800, 17])] {
                let cost = {
                    let request = session.operation_budget_scope().unwrap();
                    let before = session.store.get_fuel().unwrap();
                    let result = session.data_scalar_with_inputs(&literal).unwrap();
                    let cost = before - session.store.get_fuel().unwrap();
                    drop(result); drop(request); cost
                };
                let handles = session.stats().external_value_handles;
                for allowance in 0..cost {
                    let request = session.operation_budget_scope().unwrap();
                    session.store.set_fuel(allowance).unwrap();
                    let error = session.data_scalar_with_inputs(&literal).unwrap_err();
                    assert!(matches!(error,SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                        "allowance={allowance}: {error:?}");
                    assert_eq!(session.stats().external_value_handles, handles);
                    drop(request);
                }
                let result = session.data_scalar_with_inputs(&literal).unwrap();
                drop(result);
                assert_eq!(session.stats().external_value_handles, handles);
            }
        }
    }
    #[test]
    fn scalar_physical_types_reject_extra_field_subtype_and_preserve_direct_errors() {
        use wasmtime::{FieldType, Finality, Mutability, StorageType, StructRef, StructRefPre, ValType};
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let base = StructType::with_finality_and_supertype(&session.engine, Finality::NonFinal, None,
                [FieldType::new(Mutability::Var, StorageType::ValType(ValType::F64))]).unwrap();
            let subtype = StructType::with_finality_and_supertype(&session.engine, Finality::Final, Some(&base),
                [FieldType::new(Mutability::Var, StorageType::ValType(ValType::F64)),
                 FieldType::new(Mutability::Var, StorageType::ValType(ValType::I32))]).unwrap();
            let allocator = StructRefPre::new(&mut session.store, subtype);
            let receiver = {
                let mut scope = RootScope::new(&mut session.store);
                let object = StructRef::new(&mut scope, &allocator, &[Val::F64(17.0f64.to_bits()), Val::I32(99)]).unwrap();
                super::super::own(&mut scope, Val::AnyRef(Some(object.to_anyref())), session.identity, &session.handles).unwrap()
            };
            // Exact receiver and first field match; nominal subtype with EXTRA
            // physical storage must still fail the retained exact base type.
            let facts = ScalarInputs { receiver: receiver.clone(), expected: Expected::Number(base, 17.0f64.to_bits()) };
            session.collect().unwrap();
            assert!(!facts.matches(&mut session, &receiver).unwrap());
            let (string, _) = session.data_scalar_with_inputs(&Literal::String(vec![17])).unwrap();
            let other_array_type = ArrayType::new(&session.engine,
                FieldType::new(Mutability::Var, StorageType::ValType(ValType::I32)));
            let wrong_array = ScalarInputs { receiver: string.clone(), expected: Expected::String(other_array_type, vec![17]) };
            assert!(!wrong_array.matches(&mut session, &string).unwrap(), "equal units cannot replace exact packed array type");
            let wrong_kind = ScalarInputs { receiver: string.clone(), expected: facts.expected };
            assert!(matches!(wrong_kind.matches(&mut session, &string), Err(SessionError::Host(_))));
            let irq = session.host_work_interrupt(); let identity = session.identity;
            let request = session.operation_budget_scope().unwrap();
            let mut scope = RootScope::new(&mut session.store);
            let root = Val::AnyRef(Some(string.value.to_rooted(&mut scope)));
            assert!(!wrong_kind.matches_in_context(scope.as_context_mut(), root, identity, &irq, false).unwrap());
            drop(scope); drop(request);
        }
    }
    #[test]
    fn scalar_physical_false_and_sentinel_poll_irq_and_number_bits_restore_after_gc() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            for literal in [Literal::Nil, Literal::Number(-0.0), Literal::Number(f64::from_bits(0x7ff8000000000071)),
                Literal::String(vec![0, 0xd800, 0xffff])] {
                let (value, facts) = session.data_scalar_with_inputs(&literal).unwrap();
                let wrong = session.data_scalar(&Literal::Number(99.0)).unwrap();
                for receiver in [&value, &wrong] {
                    let request = session.operation_budget_scope().unwrap();
                    session.interrupt_handle().interrupt();
                    let error = facts.matches(&mut session, receiver).unwrap_err();
                    assert!(error.is_interrupt(), "{error:?}");
                    drop(request);
                }
                if let Literal::Number(number) = literal {
                    let error = session.inspect(&value, |mut store, root| {
                        root.unwrap_anyref().unwrap().as_struct(&store)?.unwrap().set_field(&mut store, 0, Val::F64(99.0f64.to_bits()))?;
                        Ok(())
                    }).unwrap_err();
                    let SessionError::Host(inner) = error else { panic!("immutable factory write returned {error:?}") };
                    assert_eq!(inner.to_string(), "cannot set field 0: field is not mutable");
                    session.collect().unwrap();
                    session.inspect(&value, |mut store, root| {
                        assert_eq!(root.unwrap_anyref().unwrap().as_struct(&store)?.unwrap()
                            .field(&mut store, 0)?.unwrap_f64().to_bits(), number.to_bits());
                        Ok(())
                    }).unwrap();
                    assert!(facts.matches(&mut session, &value).unwrap());

                    // Test-only mutable physical box, NOT a sealed factory or
                    // source-authorized construction: exercise exact witness
                    // comparison across mutation, GC and bit-for-bit restore.
                    use wasmtime::{FieldType, Mutability, StorageType, StructRef, StructRefPre, ValType};
                    let ty = StructType::new(&session.engine,
                        [FieldType::new(Mutability::Var, StorageType::ValType(ValType::F64))]).unwrap();
                    let allocator = StructRefPre::new(&mut session.store, ty.clone());
                    let mutable = {
                        let mut scope = RootScope::new(&mut session.store);
                        let object = StructRef::new(&mut scope, &allocator, &[Val::F64(number.to_bits())]).unwrap();
                        super::super::own(&mut scope, Val::AnyRef(Some(object.to_anyref())), session.identity, &session.handles).unwrap()
                    };
                    let mutable_facts = ScalarInputs { receiver: mutable.clone(), expected: Expected::Number(ty, number.to_bits()) };
                    assert!(mutable_facts.matches(&mut session, &mutable).unwrap());
                    session.inspect(&mutable, |mut store, root| {
                        root.unwrap_anyref().unwrap().as_struct(&store)?.unwrap().set_field(&mut store, 0, Val::F64(99.0f64.to_bits()))?;
                        Ok(())
                    }).unwrap();
                    session.collect().unwrap();
                    assert!(!mutable_facts.matches(&mut session, &mutable).unwrap());
                    session.inspect(&mutable, |mut store, root| {
                        root.unwrap_anyref().unwrap().as_struct(&store)?.unwrap().set_field(&mut store, 0, Val::F64(number.to_bits()))?;
                        Ok(())
                    }).unwrap();
                    session.collect().unwrap();
                    assert!(mutable_facts.matches(&mut session, &mutable).unwrap());
                }
                if let Literal::String(ref units) = literal {
                    for (index, original) in units.iter().copied().enumerate() {
                        session.inspect(&value, |mut store, root| {
                            root.unwrap_anyref().unwrap().as_array(&store)?.unwrap()
                                .set(&mut store, index as u32, Val::I32(i32::from(original ^ 1)))?;
                            Ok(())
                        }).unwrap();
                        session.collect().unwrap();
                        assert!(!facts.matches(&mut session, &value).unwrap(), "mutated UTF16 unit {index}");
                        session.inspect(&value, |mut store, root| {
                            root.unwrap_anyref().unwrap().as_array(&store)?.unwrap()
                                .set(&mut store, index as u32, Val::I32(i32::from(original)))?;
                            Ok(())
                        }).unwrap();
                        session.collect().unwrap();
                        assert!(facts.matches(&mut session, &value).unwrap(), "restored UTF16 unit {index}");
                    }
                }
                session.collect().unwrap();
                assert!(facts.matches(&mut session, &value).unwrap());
            }
        }
    }
    #[test]
    fn scalar_host_work_string_copy_exceeds_guest_allocation_cost() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let units = vec![0xd800; 4096];
            let mut costs = Vec::new();
            for copy in [false, true] {
                let request = session.operation_budget_scope().unwrap();
                let before = session.store.get_fuel().unwrap();
                let value = if copy {
                    session
                        .data_scalar(&Literal::String(units.clone()))
                        .unwrap()
                } else {
                    let mut scope = RootScope::new(&mut session.store);
                    let function = session.runtime.get_func(&mut scope, "string-new").unwrap();
                    super::super::call(
                        &mut scope,
                        session.runtime,
                        function,
                        &[Val::I32(units.len() as i32)],
                        session.identity,
                        &session.handles,
                    )
                    .unwrap()
                };
                costs.push(before - session.store.get_fuel().unwrap());
                drop(value);
                drop(request);
            }
            assert!(
                costs[1] >= costs[0] + units.len() as u64,
                "guest allocation {}, native copy {}",
                costs[0],
                costs[1]
            );
        }
    }
    #[test]
    fn scalar_host_work_invoke_polls_before_later_foreign_argument() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let function = session.eval("(fn [a b] a)").unwrap();
            let local = session.eval("17").unwrap();
            let mut other = Session::new().unwrap();
            let foreign = other.eval("99").unwrap();
            let handles = session.stats().external_value_handles;
            for interrupt in [false, true] {
                let request = session.operation_budget_scope().unwrap();
                session.store.set_fuel(0).unwrap();
                if interrupt {
                    session.interrupt_handle().interrupt();
                }
                assert!(matches!(
                    session.invoke(&function, &[&foreign]),
                    Err(SessionError::ForeignValue)
                ));
                let error = session.invoke(&function, &[&local, &foreign]).unwrap_err();
                if interrupt {
                    assert!(error.is_interrupt(), "{error:?}");
                } else {
                    assert!(
                        matches!(error,SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>()==Some(&wasmtime::Trap::OutOfFuel)),
                        "{error:?}"
                    );
                }
                assert_eq!(session.stats().external_value_handles, handles);
                drop(request);
                let value = session.invoke(&function, &[&local, &local]).unwrap();
                session
                    .inspect(&value, |mut store, value| {
                        let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        assert_eq!(
                            number.field(&mut store, 0)?.unwrap_f64().to_bits(),
                            17.0f64.to_bits()
                        );
                        Ok(())
                    })
                    .unwrap();
                drop(value);
                assert_eq!(session.stats().external_value_handles, handles);
            }
        }
    }
    #[test]
    fn scalar_inputs_preserve_exact_bits_and_reject_string_mutation_after_gc() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            for literal in [
                Literal::Nil,
                Literal::Bool(false),
                Literal::Bool(true),
                Literal::Number(-0.0),
                Literal::Number(f64::from_bits(0x7ff8000000000071)),
                Literal::Number(f64::INFINITY),
                Literal::String(vec![]),
                Literal::String(vec![0, 0xd800, 0xdc00, 65535]),
            ] {
                let before = session.stats().external_value_handles;
                let (value, facts) = session.data_scalar_with_inputs(&literal).unwrap();
                session.collect().unwrap();
                assert!(facts.matches(&mut session, &value).unwrap());
                if matches!(literal, Literal::Number(_) | Literal::String(_)) {
                    let clone = session.data_scalar(&literal).unwrap();
                    assert!(!facts.matches(&mut session, &clone).unwrap());
                }
                drop((value, facts));
                assert_eq!(session.stats().external_value_handles, before);
            }
            let (value, facts) = session
                .data_scalar_with_inputs(&Literal::String(vec![17, 0, 0xd800]))
                .unwrap();
            session
                .inspect(&value, |mut store, value| {
                    value
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap()
                        .set(&mut store, 0, Val::I32(99))?;
                    Ok(())
                })
                .unwrap();
            session.collect().unwrap();
            assert!(!facts.matches(&mut session, &value).unwrap());
            let mut foreign = Session::new().unwrap();
            let foreign = foreign.eval("17").unwrap();
            assert!(matches!(
                facts.matches(&mut session, &foreign),
                Err(SessionError::ForeignValue)
            ));
            session.reset().unwrap();
            assert!(matches!(
                facts.matches(&mut session, &value),
                Err(SessionError::ForeignValue)
            ));
        }
    }
    #[test]
    fn scalar_inputs_share_budget_and_release_failed_preparations() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let literal = Literal::String(vec![0xd800; 1024]);
            let before = session.stats().external_value_handles;
            for allowance in [0, 100, 2200, 3500, 4500] {
                let request = session.operation_budget_scope().unwrap();
                session.store.set_fuel(allowance).unwrap();
                match session.data_scalar_with_inputs(&literal) {
                    Ok(value) => drop(value),
                    Err(error) => assert!(
                        matches!(error,SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>()==Some(&wasmtime::Trap::OutOfFuel))
                    ),
                }
                drop(request);
                assert_eq!(session.stats().external_value_handles, before);
            }
            assert!(
                session
                    .data_scalar_with_inputs(&Literal::Undefined)
                    .is_err()
            );
            let (value, facts) = session.data_scalar_with_inputs(&literal).unwrap();
            assert!(facts.matches(&mut session, &value).unwrap());
            drop((value, facts));
            assert_eq!(session.stats().external_value_handles, before);
            let request = session.operation_budget_scope().unwrap();
            session.interrupt_handle().interrupt();
            let error = session.data_scalar_with_inputs(&literal).unwrap_err();
            assert!(error.is_interrupt(), "{error:?}");
            drop(request);
            assert_eq!(session.stats().external_value_handles, before);
            let (value, facts) = session.data_scalar_with_inputs(&literal).unwrap();
            assert!(facts.matches(&mut session, &value).unwrap());
        }
    }
}
