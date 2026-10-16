//! Exact direct storage of dense source arrays from native constructor inputs.
//! Not descendant/source/memo authorization; no retained cache consumes this.
use super::{Session, SessionError, SessionValue, own};
use wasmtime::{AnyRef, AsContextMut, RootScope, Rooted, Val};

type Result<T> = std::result::Result<T, SessionError>;

#[derive(Debug)]
pub(crate) struct SourceArrayInputs {
    receiver: SessionValue,
    descriptor: SessionValue,
    fields: SessionValue,
    backing: SessionValue,
    length: SessionValue,
    dense: SessionValue,
    mask: SessionValue,
    values: Vec<SessionValue>,
}
fn invalid(message: &'static str) -> SessionError {
    SessionError::Host(wasmtime::Error::msg(message))
}
fn object_field(session: &mut Session, value: &SessionValue, index: usize) -> Result<SessionValue> {
    session.charge_host_work(1)?;
    let mut scope = RootScope::new(&mut session.store);
    let object = value
        .value
        .to_rooted(&mut scope)
        .as_struct(&scope)?
        .ok_or_else(|| invalid("Source array needs object storage"))?;
    let value = object.field(&mut scope, index)?;
    own(&mut scope, value, session.identity, &session.handles)
}
fn array_field(session: &mut Session, value: &SessionValue, index: u32) -> Result<SessionValue> {
    session.charge_host_work(1)?;
    let mut scope = RootScope::new(&mut session.store);
    let array = value
        .value
        .to_rooted(&mut scope)
        .as_array(&scope)?
        .ok_or_else(|| invalid("Source array needs array storage"))?;
    let value = array.get(&mut scope, index)?;
    own(&mut scope, value, session.identity, &session.handles)
}
impl Session {
    pub(crate) fn data_array_with_inputs(
        &mut self,
        values: &[&SessionValue],
    ) -> Result<(SessionValue, SourceArrayInputs)> {
        let _request = self.operation_budget_scope()?;
        self.check_data_input_owners(values)?;
        let bytes = values
            .len()
            .checked_mul(std::mem::size_of::<SessionValue>())
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| invalid("Source array input allocation overflow"))?;
        self.charge_host_work(bytes.saturating_add(1))?;
        let mut expected = Vec::new();
        expected
            .try_reserve_exact(values.len())
            .map_err(|_| invalid("Source array input allocation failed"))?;
        for value in values {
            self.charge_host_work(1)?;
            expected.push((*value).clone());
        }
        // Expected element references precede the callback-free native factory.
        let value = self.data_array(values)?;
        let descriptor = object_field(self, &value, 0)?;
        let fields = object_field(self, &value, 1)?;
        let backing = array_field(self, &fields, 0)?;
        let length = array_field(self, &backing, 0)?;
        let dense = array_field(self, &backing, 2)?;
        let mask = array_field(self, &backing, 3)?;
        let facts = SourceArrayInputs {
            receiver: value.clone(),
            descriptor,
            fields,
            backing,
            length,
            dense,
            mask,
            values: expected,
        };
        if !facts.matches(self, &value)? {
            return Err(invalid("Source array factory did not preserve its inputs"));
        }
        Ok((value, facts))
    }
}
impl SourceArrayInputs {
    pub(super) fn inputs(&self) -> impl Iterator<Item = &SessionValue> {
        self.values.iter()
    }
    pub(crate) fn matches(&self, session: &mut Session, value: &SessionValue) -> Result<bool> {
        session.check(value)?;
        session.check(&self.receiver)?;
        let _request = session.operation_budget_scope()?;
        let interrupt = session.host_work_interrupt();
        let mut scope = RootScope::new(&mut session.store);
        let receiver = value.value.to_rooted(&mut scope);
        let expected = self.receiver.value.to_rooted(&mut scope);
        Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
        if !Rooted::<AnyRef>::ref_eq(&scope, &receiver, &expected)? {
            return Ok(false);
        }
        // Factory-created identity/type bindings are captured before any guest
        // callback can observe the result. All mutable spine contents below
        // compare against independent constants or pre-construction references.
        let object = receiver
            .as_struct(&scope)?
            .ok_or_else(|| invalid("Source array object is malformed"))?;
        for (index, expected) in [(0, &self.descriptor), (1, &self.fields)] {
            Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
            let found = object.field(&mut scope, index)?;
            let expected = expected.value.to_rooted(&mut scope);
            if !Rooted::<AnyRef>::ref_eq(
                &scope,
                found
                    .unwrap_anyref()
                    .ok_or_else(|| invalid("Null source array spine"))?,
                &expected,
            )? {
                return Ok(false);
            }
        }
        for index in [2, 3] {
            Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
            let found = object.field(&mut scope, index)?;
            if !is_nil(&scope, &found)? {
                return Ok(false);
            }
        }
        let fields = self
            .fields
            .value
            .to_rooted(&mut scope)
            .as_array(&scope)?
            .unwrap();
        let backing = self.backing.value.to_rooted(&mut scope);
        Session::consume_host_work(scope.as_context_mut(), 3, &interrupt)?;
        if fields.len(&scope)? != 2 {
            return Ok(false);
        }
        let actual = fields.get(&mut scope, 0)?;
        let properties = fields.get(&mut scope, 1)?;
        if !Rooted::<AnyRef>::ref_eq(
            &scope,
            actual
                .unwrap_anyref()
                .ok_or_else(|| invalid("Null source array backing"))?,
            &backing,
        )? || !is_nil(&scope, &properties)?
        {
            return Ok(false);
        }
        let backing = backing.as_array(&scope)?.unwrap();
        if backing.len(&scope)? != 4 {
            return Ok(false);
        }
        for (index, expected) in [(0, &self.length), (2, &self.dense), (3, &self.mask)] {
            Session::consume_host_work(scope.as_context_mut(), 1, &interrupt)?;
            let actual = backing.get(&mut scope, index)?;
            let expected = expected.value.to_rooted(&mut scope);
            if !Rooted::<AnyRef>::ref_eq(
                &scope,
                actual
                    .unwrap_anyref()
                    .ok_or_else(|| invalid("Null source array storage"))?,
                &expected,
            )? {
                return Ok(false);
            }
        }
        Session::consume_host_work(scope.as_context_mut(), 2, &interrupt)?;
        let head = backing.get(&mut scope, 1)?;
        if !is_nil(&scope, &head)? {
            return Ok(false);
        }
        let length = self
            .length
            .value
            .to_rooted(&mut scope)
            .as_struct(&scope)?
            .unwrap();
        if length.field(&mut scope, 0)?.unwrap_f64().to_bits()
            != (self.values.len() as f64).to_bits()
        {
            return Ok(false);
        }
        let dense = self
            .dense
            .value
            .to_rooted(&mut scope)
            .as_array(&scope)?
            .unwrap();
        let mask = self
            .mask
            .value
            .to_rooted(&mut scope)
            .as_array(&scope)?
            .unwrap();
        if dense.len(&scope)? as usize != self.values.len()
            || mask.len(&scope)? as usize != self.values.len()
        {
            return Ok(false);
        }
        for (index, expected) in self.values.iter().enumerate() {
            Session::consume_host_work(scope.as_context_mut(), 3, &interrupt)?;
            if mask.get(&mut scope, index as u32)?.unwrap_i32() != 1 {
                return Ok(false);
            }
            let actual = dense.get(&mut scope, index as u32)?;
            let expected = expected.value.to_rooted(&mut scope);
            if !Rooted::<AnyRef>::ref_eq(
                &scope,
                actual
                    .unwrap_anyref()
                    .ok_or_else(|| invalid("Null source array element"))?,
                &expected,
            )? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
fn is_nil<C: wasmtime::AsContext>(store: &C, value: &Val) -> Result<bool> {
    Ok(match value {
        Val::AnyRef(Some(value)) => value.as_i31(store)?.is_some_and(|v| v.get_i32() == 0),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::resolve::Phase;

    fn session(phase: Phase) -> Session {
        match phase {
            Phase::Runtime => Session::new_repl().unwrap(),
            Phase::Macro => Session::new_macro().unwrap(),
        }
    }
    #[test]
    fn source_array_inputs_bind_dense_elements_and_reject_spine_mutation_after_gc() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let a = session.eval("17").unwrap();
            let b = session.eval("99").unwrap();
            for length in [0, 1, 65] {
                let values = vec![&a; length];
                let before = session.stats().external_value_handles;
                let (value, facts) = session.data_array_with_inputs(&values).unwrap();
                session.collect().unwrap();
                assert!(facts.matches(&mut session, &value).unwrap());
                let clone = session.data_array(&values).unwrap();
                assert!(!facts.matches(&mut session, &clone).unwrap());
                drop((value, facts, clone));
                assert_eq!(session.stats().external_value_handles, before);
            }
            let distinct: Vec<_> = (0..65)
                .map(|index| {
                    session
                        .data_scalar(&crate::portable::hir::Literal::Number(index as f64))
                        .unwrap()
                })
                .collect();
            let distinct_refs: Vec<_> = distinct.iter().collect();
            let (value, facts) = session.data_array_with_inputs(&distinct_refs).unwrap();
            session.collect().unwrap();
            assert!(facts.matches(&mut session, &value).unwrap());
            session
                .inspect(&value, |mut store, _| {
                    let dense = facts
                        .dense
                        .value
                        .to_rooted(&mut store)
                        .as_array(&store)?
                        .unwrap();
                    for (index, expected) in distinct.iter().enumerate() {
                        let actual = dense.get(&mut store, index as u32)?;
                        let expected = expected.value.to_rooted(&mut store);
                        assert!(Rooted::<AnyRef>::ref_eq(
                            &store,
                            actual.unwrap_anyref().unwrap(),
                            &expected
                        )?);
                    }
                    Ok(())
                })
                .unwrap();
            drop((value, facts, distinct_refs));
            drop(distinct);
            // Each corruption starts with independent fresh native construction.
            for mutation in 0..10 {
                let (value, facts) = session.data_array_with_inputs(&[&a, &a, &a]).unwrap();
                let (other, other_facts) = session.data_array_with_inputs(&[&a, &a, &a]).unwrap();
                let length = session.eval("3").unwrap();
                session
                    .inspect(&value, |mut store, value| {
                        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        let fields = facts
                            .fields
                            .value
                            .to_rooted(&mut store)
                            .as_array(&store)?
                            .unwrap();
                        let backing = facts
                            .backing
                            .value
                            .to_rooted(&mut store)
                            .as_array(&store)?
                            .unwrap();
                        let dense = facts
                            .dense
                            .value
                            .to_rooted(&mut store)
                            .as_array(&store)?
                            .unwrap();
                        let mask = facts
                            .mask
                            .value
                            .to_rooted(&mut store)
                            .as_array(&store)?
                            .unwrap();
                        match mutation {
                            0 => {
                                let b = b.value.to_rooted(&mut store);
                                dense.set(&mut store, 0, Val::AnyRef(Some(b)))?;
                            }
                            1 => mask.set(&mut store, 0, Val::I32(0))?,
                            2 => mask.set(&mut store, 0, Val::I32(2))?,
                            3 => {
                                let cycle = facts.backing.value.to_rooted(&mut store);
                                backing.set(&mut store, 1, Val::AnyRef(Some(cycle)))?;
                            }
                            4 => {
                                let b = b.value.to_rooted(&mut store);
                                fields.set(&mut store, 1, Val::AnyRef(Some(b)))?;
                            }
                            5 => {
                                let b = b.value.to_rooted(&mut store);
                                object.set_field(&mut store, 3, Val::AnyRef(Some(b)))?;
                            }
                            6 => {
                                let length = length.value.to_rooted(&mut store);
                                backing.set(&mut store, 0, Val::AnyRef(Some(length)))?;
                            }
                            7 => {
                                let replacement = other_facts.dense.value.to_rooted(&mut store);
                                backing.set(&mut store, 2, Val::AnyRef(Some(replacement)))?;
                            }
                            8 => {
                                let replacement = other_facts.backing.value.to_rooted(&mut store);
                                fields.set(&mut store, 0, Val::AnyRef(Some(replacement)))?;
                            }
                            9 => {
                                let replacement = other_facts.mask.value.to_rooted(&mut store);
                                backing.set(&mut store, 3, Val::AnyRef(Some(replacement)))?;
                            }
                            _ => unreachable!(),
                        }
                        Ok(())
                    })
                    .unwrap();
                session.collect().unwrap();
                assert!(
                    !facts.matches(&mut session, &value).unwrap(),
                    "mutation {mutation}"
                );
                drop((value, facts, other, other_facts, length));
            }
        }
    }
    #[test]
    fn source_array_inputs_do_not_certify_descendants_and_check_owner_reset() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let child = session.eval("(list 17)").unwrap();
            let b = session.eval("99").unwrap();
            let (value, facts) = session.data_array_with_inputs(&[&child]).unwrap();
            session
                .inspect(&child, |mut store, value| {
                    let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let fields = object
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let b = b.value.to_rooted(&mut store);
                    fields.set(&mut store, 1, Val::AnyRef(Some(b)))?;
                    Ok(())
                })
                .unwrap();
            // Complete retained-source validation must also check the child's
            // independent facts. A direct reference match intentionally cannot.
            session.collect().unwrap();
            assert!(facts.matches(&mut session, &value).unwrap());
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
    fn source_array_inputs_share_budget_and_release_roots_on_error() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let a = session.eval("17").unwrap();
            let values = vec![&a; 65];
            let before = session.stats().external_value_handles;
            // Sample cleanup across budgets; no distinct-stage coverage claim.
            for allowance in [0, 20, 100, 500, 2000, 2500, 5000, 10000] {
                let request = session.operation_budget_scope().unwrap();
                session.store.set_fuel(allowance).unwrap();
                match session.data_array_with_inputs(&values) {
                    Ok((value, facts)) => drop((value, facts)),
                    Err(error) => assert!(
                        matches!(error,SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>()==Some(&wasmtime::Trap::OutOfFuel))
                    ),
                }
                drop(request);
                assert_eq!(session.stats().external_value_handles, before);
            }
            let (value, facts) = session.data_array_with_inputs(&values).unwrap();
            session.collect().unwrap();
            assert!(facts.matches(&mut session, &value).unwrap());
            drop((value, facts));
            assert_eq!(session.stats().external_value_handles, before);
            let request = session.operation_budget_scope().unwrap();
            session.interrupt_handle().interrupt();
            let error = session.data_array_with_inputs(&values).unwrap_err();
            assert!(error.is_interrupt(), "{error:?}");
            drop(request);
            assert_eq!(session.stats().external_value_handles, before);
            let (value, facts) = session.data_array_with_inputs(&values).unwrap();
            assert!(facts.matches(&mut session, &value).unwrap());
        }
    }
    #[test]
    fn source_input_owner_checks_poll_before_later_foreign_values() {
        fn build(
            session: &mut Session,
            route: usize,
            class: &SessionValue,
            values: &[&SessionValue],
        ) -> Result<()> {
            match route {
                0 => session.data_array(values).map(drop),
                1 => session.data_array_with_inputs(values).map(drop),
                2 => session.data_construct(class, values).map(drop),
                3 => session.data_construct_with_inputs(class, values).map(drop),
                _ => unreachable!(),
            }
        }
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            session.eval("(deftype OwnerCheckProbe [a b])").unwrap();
            let class = session.eval("OwnerCheckProbe").unwrap();
            let local = session.eval("17").unwrap();
            let mut foreign_session = Session::new().unwrap();
            let foreign = foreign_session.eval("99").unwrap();
            let handles = session.stats().external_value_handles;
            for route in 0..4 {
                for interrupt in [false, true] {
                    let request = session.operation_budget_scope().unwrap();
                    session.store.set_fuel(0).unwrap();
                    if interrupt {
                        session.interrupt_handle().interrupt();
                    }
                    // At the first item, foreign ownership precedes its charge.
                    assert!(matches!(
                        build(&mut session, route, &class, &[&foreign]),
                        Err(SessionError::ForeignValue)
                    ));
                    let error =
                        build(&mut session, route, &class, &[&local, &foreign]).unwrap_err();
                    if interrupt {
                        assert!(error.is_interrupt(), "route {route}: {error:?}");
                    } else {
                        assert!(
                            matches!(error,SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>()==Some(&wasmtime::Trap::OutOfFuel)),
                            "route {route}: {error:?}"
                        );
                    }
                    assert_eq!(session.stats().external_value_handles, handles);
                    drop(request);
                }
                build(&mut session, route, &class, &[&local, &local]).unwrap();
                assert_eq!(session.stats().external_value_handles, handles);
            }
        }
    }
}

impl SourceArrayInputs {
    pub(super) fn write_cuts(&self)->Vec<&SessionValue> {
        std::iter::once(&self.descriptor).chain(self.values.iter()).collect()
    }
}
