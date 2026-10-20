//! Deferred direct facts for private factory outputs. These facts provide no
//! source authority and do not enroll a write domain or admit external seeds.
use super::*;

use construction_records::Fact;
use wasmtime::{AnyRef, RootScope, Rooted};
type Result<T> = std::result::Result<T, SessionError>;
fn invalid(message: &'static str) -> SessionError {
    wasmtime::Error::msg(message).into()
}
fn same(s: &mut Session, a: &SessionValue, b: &SessionValue) -> Result<bool> {
    s.check(a)?;
    s.check(b)?;
    s.charge_host_work(1)?;
    let mut scope = RootScope::new(&mut s.store);
    let a = a.value.to_rooted(&mut scope);
    let b = b.value.to_rooted(&mut scope);
    Ok(Rooted::<AnyRef>::ref_eq(&scope, &a, &b)?)
}
// This is an input-reference substitution, NOT a certified-child capability.
// Returning a Fact cannot admit an external input into ConstructionRecords.
// The graph controller must independently certify source/closure before use.
pub(super) struct NativeBindings<'a> {
    pairs: &'a [(&'a SessionValue, &'a SessionValue)],
}
impl<'a> NativeBindings<'a> {
    fn checked(s: &mut Session, pairs: &'a [(&'a SessionValue, &'a SessionValue)]) -> Result<Self> {
        for (i, (token, target)) in pairs.iter().enumerate() {
            s.charge_host_work(2)?;
            s.check(token)?;
            s.check(target)?;
            s.charge_host_work(3)?;
            {
                let mut scope = RootScope::new(&mut s.store);
                let token = token.value.to_rooted(&mut scope);
                if token.as_i31(&scope)?.is_some_and(|v| v.get_u32() == 0) {
                    return Err(invalid("Nil cannot be a deferred factory token"));
                }
            }
            for (j, (other, _)) in pairs.iter().enumerate() {
                s.charge_host_work(1)?;
                if (i != j && same(s, token, other)?) || same(s, target, other)? {
                    return Err(invalid("Ambiguous deferred factory bindings"));
                }
            }
        }
        Ok(Self { pairs })
    }
    pub(super) fn resolve<'v>(
        &'v self,
        s: &mut Session,
        value: &'v SessionValue,
    ) -> Result<&'v SessionValue> {
        s.check(value)?;
        for (token, target) in self.pairs {
            s.charge_host_work(1)?;
            if same(s, value, token)? {
                return Ok(target);
            }
        }
        Ok(value)
    }
}
// No domain enrollment occurs here. The consuming recording context may enroll
// only after graph-owned certified-child bindings and class checks are complete.
pub(crate) struct DeferredFactory {
    value: SessionValue,
    fact: Fact,
}
impl DeferredFactory {
    pub(crate) fn array(s: &mut Session, values: &[&SessionValue]) -> Result<(SessionValue, Self)> {
        let (value, fact) = s.data_array_with_inputs(values)?;
        s.charge_host_work(1)?;
        Ok((
            value.clone(),
            Self {
                value,
                fact: Fact::array(s, fact)?,
            },
        ))
    }
    pub(crate) fn constructor(
        s: &mut Session,
        class: &SessionValue,
        values: &[&SessionValue],
    ) -> Result<(SessionValue, Self)> {
        let (value, fact) = s.data_construct_private_with_inputs(class, values)?;
        s.charge_host_work(1)?;
        Ok((
            value.clone(),
            Self {
                value,
                fact: Fact::constructor(s, fact)?,
            },
        ))
    }
    pub(crate) fn finalize(
        mut self,
        s: &mut Session,
        pairs: &[(&SessionValue, &SessionValue)],
    ) -> Result<(SessionValue, Fact)> {
        let _request = s.operation_budget_scope()?;
        let bindings = NativeBindings::checked(s, pairs)?;
        let valid = match &mut self.fact {
            Fact::Array(fact) => {
                fact.bind_native_inputs(s, &bindings)?;
                fact.matches(s, &self.value)?
            }
            Fact::Constructor(fact) => {
                fact.bind_native_inputs(s, &bindings)?;
                let direct = fact.matches(s, &self.value)?;
                let origin = fact.matches_prototype_origin(s)?;
                direct && origin
            }
            _ => return Err(invalid("Unsupported deferred factory fact")),
        };
        if !valid {
            return Err(invalid(
                "Deferred factory does not match native bound inputs",
            ));
        }
        Ok((self.value, self.fact))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::{RootScope, Val};

    fn set_dense(s: &mut Session, object: &SessionValue, index: u32, value: &SessionValue) {
        let mut scope = RootScope::new(&mut s.store);
        let object = object
            .value
            .to_rooted(&mut scope)
            .as_struct(&scope)
            .unwrap()
            .unwrap();
        let fields = object
            .field(&mut scope, 1)
            .unwrap()
            .unwrap_anyref()
            .copied()
            .unwrap()
            .as_array(&scope)
            .unwrap()
            .unwrap();
        let backing = fields
            .get(&mut scope, 0)
            .unwrap()
            .unwrap_anyref()
            .copied()
            .unwrap()
            .as_array(&scope)
            .unwrap()
            .unwrap();
        let dense = backing
            .get(&mut scope, 2)
            .unwrap()
            .unwrap_anyref()
            .copied()
            .unwrap()
            .as_array(&scope)
            .unwrap()
            .unwrap();
        let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
        dense.set(&mut scope, index, value).unwrap();
    }
    #[test]
    fn deferred_factory_expected_bindings_precede_storage_and_reject_poison() {
        let mut s = Session::new_macro().unwrap();
        let _op = s.operation_budget_scope().unwrap();
        let token = s.data_array(&[]).unwrap();
        let child = s
            .data_scalar(&crate::portable::hir::Literal::Number(17.0))
            .unwrap();
        let poison = s
            .data_scalar(&crate::portable::hir::Literal::Number(99.0))
            .unwrap();
        let (value, deferred) = DeferredFactory::array(&mut s, &[&token, &child]).unwrap();
        set_dense(&mut s, &value, 0, &child);
        set_dense(&mut s, &value, 1, &poison);
        assert!(deferred.finalize(&mut s, &[(&token, &child)]).is_err());
        let (value, deferred) = DeferredFactory::array(&mut s, &[&token, &child]).unwrap();
        set_dense(&mut s, &value, 0, &child);
        s.collect().unwrap();
        let (actual, fact) = deferred.finalize(&mut s, &[(&token, &child)]).unwrap();
        match fact {
            construction_records::Fact::Array(f) => assert!(f.matches(&mut s, &actual).unwrap()),
            _ => panic!("array fact required"),
        }
    }
    #[test]
    fn deferred_factory_does_not_treat_unpatched_or_wrong_children_as_expected() {
        let mut s = Session::new_macro().unwrap();
        let _op = s.operation_budget_scope().unwrap();
        let token = s.data_array(&[]).unwrap();
        let child = s
            .data_scalar(&crate::portable::hir::Literal::Number(17.0))
            .unwrap();
        let (_, deferred) = DeferredFactory::array(&mut s, &[&token]).unwrap();
        assert!(deferred.finalize(&mut s, &[(&token, &child)]).is_err());
        let (value, deferred) = DeferredFactory::array(&mut s, &[&token]).unwrap();
        set_dense(&mut s, &value, 0, &child);
        assert!(deferred.finalize(&mut s, &[]).is_err());
    }
    fn set_nominal(s: &mut Session, object: &SessionValue, index: u32, value: &SessionValue) {
        let mut scope = RootScope::new(&mut s.store);
        let object = object
            .value
            .to_rooted(&mut scope)
            .as_struct(&scope)
            .unwrap()
            .unwrap();
        let fields = object
            .field(&mut scope, 1)
            .unwrap()
            .unwrap_anyref()
            .copied()
            .unwrap()
            .as_array(&scope)
            .unwrap()
            .unwrap();
        let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
        fields.set(&mut scope, index, value).unwrap();
    }
    #[test]
    fn deferred_factory_constructor_rejects_memo_poison_and_preserves_captured_inputs() {
        let mut s = Session::new_macro().unwrap();
        let _op = s.operation_budget_scope().unwrap();
        let class = s.eval("PersistentArrayMap").unwrap();
        let token = s.data_array(&[]).unwrap();
        let child = s.data_array(&[]).unwrap();
        let count = s
            .data_scalar(&crate::portable::hir::Literal::Number(0.0))
            .unwrap();
        let nil = s.data_scalar(&crate::portable::hir::Literal::Nil).unwrap();
        let poison = s
            .data_scalar(&crate::portable::hir::Literal::Number(991.0))
            .unwrap();
        let (value, deferred) =
            DeferredFactory::constructor(&mut s, &class, &[&token, &count, &child, &nil]).unwrap();
        set_nominal(&mut s, &value, 0, &child);
        set_nominal(&mut s, &value, 3, &poison);
        assert!(deferred.finalize(&mut s, &[(&token, &child)]).is_err());
        let (value, deferred) =
            DeferredFactory::constructor(&mut s, &class, &[&token, &count, &child, &nil]).unwrap();
        set_nominal(&mut s, &value, 0, &child);
        s.collect().unwrap();
        let (value, fact) = deferred.finalize(&mut s, &[(&token, &child)]).unwrap();
        match fact {
            Fact::Constructor(f) => {
                assert!(f.matches(&mut s, &value).unwrap());
                assert!(f.matches_prototype_origin(&mut s).unwrap());
            }
            _ => panic!("constructor fact required"),
        }
    }
    #[test]
    fn deferred_factory_fuel_irq_foreign_and_reset_never_finalize_a_fact() {
        let mut s = Session::new_macro().unwrap();
        let op = s.operation_budget_scope().unwrap();
        let token = s.data_array(&[]).unwrap();
        let child = s
            .data_scalar(&crate::portable::hir::Literal::Number(17.0))
            .unwrap();
        let (value, deferred) = DeferredFactory::array(&mut s, &[&token]).unwrap();
        set_dense(&mut s, &value, 0, &child);
        drop(op);
        s.set_operation_fuel(1);
        let op = s.operation_budget_scope().unwrap();
        assert!(deferred.finalize(&mut s, &[(&token, &child)]).is_err());
        drop(op);
        s.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let op = s.operation_budget_scope().unwrap();
        let (value, deferred) = DeferredFactory::array(&mut s, &[&token]).unwrap();
        set_dense(&mut s, &value, 0, &child);
        s.interrupt_handle().interrupt();
        assert!(
            deferred
                .finalize(&mut s, &[(&token, &child)])
                .err()
                .unwrap()
                .is_interrupt()
        );
        drop(op);
        let op = s.operation_budget_scope().unwrap();
        let mut foreign = Session::new_macro().unwrap();
        let other = foreign
            .data_scalar(&crate::portable::hir::Literal::Number(17.0))
            .unwrap();
        let (_, deferred) = DeferredFactory::array(&mut s, &[&token]).unwrap();
        assert!(deferred.finalize(&mut s, &[(&token, &other)]).is_err());
        let (value, deferred) = DeferredFactory::array(&mut s, &[&token]).unwrap();
        set_dense(&mut s, &value, 0, &child);
        assert!(deferred.finalize(&mut s, &[(&token, &child)]).is_ok());
        drop(op);
        let (_, deferred) = {
            let _op = s.operation_budget_scope().unwrap();
            DeferredFactory::array(&mut s, &[&token]).unwrap()
        };
        s.reset().unwrap();
        let _op = s.operation_budget_scope().unwrap();
        assert!(deferred.finalize(&mut s, &[(&token, &child)]).is_err());
    }
    #[test]
    fn deferred_factory_rejects_hash_field_substitution_in_both_phases() {
        for mut s in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let _op = s.operation_budget_scope().unwrap();
            let class = s.eval("PersistentArrayMap").unwrap();
            let nil = s.data_scalar(&crate::portable::hir::Literal::Nil).unwrap();
            let number = s
                .data_scalar(&crate::portable::hir::Literal::Number(17.0))
                .unwrap();
            let zero = s
                .data_scalar(&crate::portable::hir::Literal::Number(0.0))
                .unwrap();
            let token = s.data_array(&[]).unwrap();
            let array = s.data_array(&[]).unwrap();
            // nil -> number must not mint eligibility from patched storage.
            let (value, deferred) =
                DeferredFactory::constructor(&mut s, &class, &[&nil, &zero, &array, &nil]).unwrap();
            set_nominal(&mut s, &value, 3, &number);
            assert!(deferred.finalize(&mut s, &[(&nil, &number)]).is_err());
            // Numeric and actual private-array inputs at the captured hash slot
            // cannot be replaced with nil either, even without a Runtime Watch.
            for initial in [&number, &token] {
                let (value, deferred) =
                    DeferredFactory::constructor(&mut s, &class, &[&nil, &zero, &array, initial])
                        .unwrap();
                set_nominal(&mut s, &value, 3, &nil);
                assert!(deferred.finalize(&mut s, &[(initial, &nil)]).is_err());
            }
        }
    }
}
