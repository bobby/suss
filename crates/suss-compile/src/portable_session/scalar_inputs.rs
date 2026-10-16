//! Immutable native literal expectations prepared before scalar allocation.
//! This binds scalar contents/identity, not source ownership or an enclosing graph.
use super::{Session, SessionError, SessionValue};
use crate::portable::hir::Literal;
use wasmtime::{AnyRef, AsContextMut, RootScope, Rooted};

type Result<T> = std::result::Result<T, SessionError>;
#[derive(Debug)]
enum Expected {
    Sentinel(u32),
    Number(u64),
    String(Vec<u16>),
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
        &mut self,
        literal: &Literal,
    ) -> Result<(SessionValue, ScalarInputs)> {
        let _request = self.operation_budget_scope()?;
        self.charge_host_work(1)?;
        let expected = match literal {
            Literal::Nil => Expected::Sentinel(0),
            Literal::Bool(false) => Expected::Sentinel(2),
            Literal::Bool(true) => Expected::Sentinel(4),
            Literal::Number(value) => Expected::Number(value.to_bits()),
            Literal::String(units) => {
                let bytes = units
                    .len()
                    .checked_mul(std::mem::size_of::<u16>())
                    .and_then(|v| u64::try_from(v).ok())
                    .ok_or_else(|| invalid("Scalar input allocation overflow"))?;
                self.charge_host_work(bytes.saturating_add(1))?;
                let mut expected = Vec::new();
                expected
                    .try_reserve_exact(units.len())
                    .map_err(|_| invalid("Scalar input allocation failed"))?;
                for unit in units {
                    self.charge_host_work(1)?;
                    expected.push(*unit);
                }
                Expected::String(expected)
            }
            Literal::Undefined => {
                return self.data_scalar(literal).and_then(|_| {
                    Err(invalid("Undefined compiler literal unexpectedly accepted"))
                });
            }
        };
        let value = self.data_scalar(literal)?;
        let facts = ScalarInputs {
            receiver: value.clone(),
            expected,
        };
        if !facts.matches(self, &value)? {
            return Err(invalid("Scalar factory did not preserve its literal"));
        }
        Ok((value, facts))
    }
}
impl ScalarInputs {
    pub(crate) fn matches(&self, session: &mut Session, value: &SessionValue) -> Result<bool> {
        session.check(value)?;
        session.check(&self.receiver)?;
        let _request = session.operation_budget_scope()?;
        session.charge_host_work(1)?;
        let interrupt = session.host_work_interrupt();
        let mut scope = RootScope::new(&mut session.store);
        let found = value.value.to_rooted(&mut scope);
        let receiver = self.receiver.value.to_rooted(&mut scope);
        if !Rooted::<AnyRef>::ref_eq(&scope, &found, &receiver)? {
            return Ok(false);
        }
        match &self.expected {
            Expected::Sentinel(expected) => Ok(found
                .as_i31(&scope)?
                .is_some_and(|value| value.get_u32() == *expected)),
            Expected::Number(expected) => {
                let value = found
                    .as_struct(&scope)?
                    .ok_or_else(|| invalid("Scalar number needs boxed storage"))?;
                Ok(value.field(&mut scope, 0)?.unwrap_f64().to_bits() == *expected)
            }
            Expected::String(expected) => {
                let value = found
                    .as_array(&scope)?
                    .ok_or_else(|| invalid("Scalar string needs UTF-16 storage"))?;
                if value.len(&scope)? as usize != expected.len() {
                    return Ok(false);
                }
                for (index, expected) in expected.iter().enumerate() {
                    Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
                    if value.get(&mut scope, index as u32)?.unwrap_i32() as u16 != *expected {
                        return Ok(false);
                    }
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
