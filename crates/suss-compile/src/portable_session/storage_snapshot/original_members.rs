//! Private downward membership in an ORIGINAL snapshot. No current fields,
//! source certificate, memo receipt or validity bit are produced by this walk.
use super::*;

pub(in crate::portable_session) struct OriginalMember {
    scope: MemberScope,
}
enum MemberScope {
    Node(usize),
    Terminal(SessionValue),
}
impl OriginalMember {
    pub(in crate::portable_session) fn retained(&self, session: &mut Session) -> Result<Self> {
        session.charge_host_work((2 * size_of::<Self>() + 6) as u64)?;
        let scope = match &self.scope {
            MemberScope::Node(index) => MemberScope::Node(*index),
            MemberScope::Terminal(value) => {
                session.check(value)?;
                MemberScope::Terminal(value.clone())
            }
        };
        Ok(Self { scope })
    }
}

// New proof scratch pays creation, live relocation AND backing retirement
// before reserve. Existing snapshot capture/query rates are not changed.
fn member_room<T>(items: &mut Vec<T>, extra: usize, meter: &mut Meter<'_>) -> Result<()> {
    meter.work(8)?;
    let needed = items
        .len()
        .checked_add(extra)
        .ok_or_else(|| invalid("Original member scratch extent overflow"))?;
    if needed <= items.capacity() {
        return Ok(());
    }
    let cap = items
        .capacity()
        .checked_mul(2)
        .unwrap_or(needed)
        .max(needed);
    let bytes = cap
        .checked_mul(2)
        .and_then(|n| n.checked_add(items.len()))
        .and_then(|n| n.checked_mul(size_of::<T>()))
        .and_then(|n| u64::try_from(n).ok())
        .and_then(|n| n.checked_add(4))
        .ok_or_else(|| invalid("Original member scratch storage overflow"))?;
    meter.work(bytes)?;
    items
        .try_reserve_exact(cap - items.len())
        .map_err(|_| invalid("Original member scratch allocation failed"))
}
impl StorageSnapshot {
    // CanonicalSeed binds within to its actual immutable Arc before calling.
    // This type's fields are private; arbitrary indices cannot be supplied by
    // the source controller or a public restore/health path.
    pub(in crate::portable_session) fn original_member(
        &self,
        session: &mut Session,
        value: &SessionValue,
        within: Option<&OriginalMember>,
    ) -> Result<Option<OriginalMember>> {
        session.check(value)?;
        if self.owner != session.identity {
            return Err(SessionError::ForeignValue);
        }
        let _request = session.operation_budget_scope()?;
        session.charge_host_work(5)?; // owner/scope/root controls
        let interrupt = session.host_work_interrupt();
        let mut root_scope = RootScope::new(&mut session.store);
        let target = Val::AnyRef(Some(value.value.to_rooted(&mut root_scope)));
        let mut meter = Meter::new(root_scope.as_context_mut(), &interrupt);
        meter.work((4 * size_of::<Vec<usize>>() + 12) as u64)?;
        // No terminal may expand via another node/seed which happens to know
        // the same reference. Its exact identity is the whole permission.
        if let Some(OriginalMember {
            scope: MemberScope::Terminal(original),
        }) = within
        {
            meter.work(4)?;
            let expected = Val::AnyRef(Some(original.value.to_rooted(&mut meter.store)));
            if !Self::same_member_value(&mut meter, &expected, &target)? {
                return Ok(None);
            }
            meter.work((2 * size_of::<OriginalMember>() + 4) as u64)?;
            return Ok(Some(OriginalMember {
                scope: MemberScope::Terminal(original.clone()),
            }));
        }
        let subroot;
        let start = match within {
            None => &self.root,
            Some(OriginalMember {
                scope: MemberScope::Node(index),
            }) => {
                meter.work(3)?;
                if self.nodes.get(*index).is_none() {
                    return Err(invalid("Original member node outside snapshot"));
                }
                subroot = Atom::Node(*index);
                &subroot
            }
            Some(_) => unreachable!("terminal handled above"),
        };
        let mut pending = Vec::new();
        let mut seen = Vec::new();
        member_room(&mut pending, 1, &mut meter)?;
        meter.work(2)?;
        pending.push(start);
        while let Some(atom) = pending.pop() {
            meter.work(4)?; // pop, tag, references/control
            if let Atom::Node(index) = atom {
                if self.nodes.get(*index).is_none() {
                    return Err(invalid("Original captured edge outside snapshot"));
                }
            }
            if self.atom_matches(atom, target.clone(), &mut meter)? {
                meter.work((2 * size_of::<OriginalMember>() + 4) as u64)?;
                let scope = match atom {
                    Atom::Node(index) => MemberScope::Node(*index),
                    // An AnyRef SessionValue can match only an actual Node,
                    // opaque original cut or i31 atom. Numeric/null/function
                    // storage fields cannot mint a boxed guest member.
                    Atom::I31(_) | Atom::Opaque(_) => MemberScope::Terminal(value.clone()),
                    _ => continue,
                };
                return Ok(Some(OriginalMember { scope }));
            }
            let Atom::Node(index) = atom else {
                continue;
            };
            let mut duplicate = false;
            for old in &seen {
                meter.work(2)?;
                if old == index {
                    duplicate = true;
                    break;
                }
            }
            if duplicate {
                continue;
            }
            member_room(&mut seen, 1, &mut meter)?;
            meter.work(2)?;
            seen.push(*index);
            let contents = self.nodes[*index]
                .contents
                .as_ref()
                .ok_or_else(|| invalid("Incomplete original snapshot node"))?;
            let children = match contents {
                Contents::Struct(_, atoms) | Contents::Array(_, atoms) => atoms,
            };
            member_room(&mut pending, children.len(), &mut meter)?;
            for child in children {
                meter.work(3)?;
                pending.push(child);
            }
        }
        Ok(None)
    }
    fn same_member_value(meter: &mut Meter<'_>, left: &Val, right: &Val) -> Result<bool> {
        meter.work(2)?;
        let (Val::AnyRef(Some(left)), Val::AnyRef(Some(right))) = (left, right) else {
            return Ok(false);
        };
        Ok(Rooted::<AnyRef>::ref_eq(&meter.store, left, right)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::hir::Literal;

    #[test]
    fn original_snapshot_subroot_is_downward_and_opaque_cuts_stay_terminal() {
        let mut session = Session::new_macro().unwrap();
        let leaf = session.data_scalar(&Literal::Number(17.0)).unwrap();
        let other = session.data_scalar(&Literal::Number(23.0)).unwrap();
        let branch = session.data_array(&[&leaf]).unwrap();
        let sibling = session.data_array(&[&other]).unwrap();
        let root = session.data_array(&[&branch, &sibling]).unwrap();
        let snapshot = session.capture_storage(&root).unwrap();
        let member = snapshot
            .original_member(&mut session, &branch, None)
            .unwrap()
            .unwrap();
        session.collect().unwrap();
        assert!(
            snapshot
                .original_member(&mut session, &leaf, Some(&member))
                .unwrap()
                .is_some()
        );
        assert!(
            snapshot
                .original_member(&mut session, &root, Some(&member))
                .unwrap()
                .is_none()
        );
        assert!(
            snapshot
                .original_member(&mut session, &sibling, Some(&member))
                .unwrap()
                .is_none()
        );
        assert!(
            snapshot
                .original_member(&mut session, &other, Some(&member))
                .unwrap()
                .is_none()
        );
        let cut = session.capture_storage_cut(&root, &[&branch]).unwrap();
        let opaque = cut
            .original_member(&mut session, &branch, None)
            .unwrap()
            .unwrap();
        assert!(
            cut.original_member(&mut session, &branch, Some(&opaque))
                .unwrap()
                .is_some()
        );
        assert!(
            cut.original_member(&mut session, &leaf, Some(&opaque))
                .unwrap()
                .is_none()
        );
        assert!(
            cut.original_member(&mut session, &root, Some(&opaque))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn original_snapshot_subroot_fuel_irq_owner_and_retry_preserve_the_original_graph() {
        let mut session = Session::new_macro().unwrap();
        let leaf = session.data_scalar(&Literal::Number(17.0)).unwrap();
        let branch = session.data_array(&[&leaf]).unwrap();
        let root = session.data_array(&[&branch]).unwrap();
        let snapshot = session.capture_storage(&root).unwrap();
        let member = snapshot
            .original_member(&mut session, &branch, None)
            .unwrap()
            .unwrap();
        let handles = session.stats().external_value_handles;
        let used = {
            let _operation = session.operation_budget_scope().unwrap();
            let before = session.store.get_fuel().unwrap();
            assert!(
                snapshot
                    .original_member(&mut session, &leaf, Some(&member))
                    .unwrap()
                    .is_some()
            );
            before - session.store.get_fuel().unwrap()
        };
        assert!(used > 2);
        for cut in [0, 1, used / 2, used - 1] {
            let operation = session.operation_budget_scope().unwrap();
            session.store.set_fuel(cut).unwrap();
            let result = snapshot.original_member(&mut session, &leaf, Some(&member));
            assert!(
                matches!(result, Err(SessionError::Trap(ref error))
                if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                "the original traversal must not publish a member at fuel cut {cut}"
            );
            drop(operation);
            assert_eq!(session.stats().external_value_handles, handles);
        }
        let operation = session.operation_budget_scope().unwrap();
        session.interrupt_handle().interrupt();
        let result = snapshot.original_member(&mut session, &leaf, Some(&member));
        assert!(matches!(result, Err(ref error) if error.is_interrupt()));
        drop(operation);
        assert_eq!(session.stats().external_value_handles, handles);
        // A normal new operation resets the interruption, without recapturing
        // or widening the original snapshot/member graph.
        assert!(
            snapshot
                .original_member(&mut session, &leaf, Some(&member))
                .unwrap()
                .is_some()
        );
        assert!(
            snapshot
                .original_member(&mut session, &root, Some(&member))
                .unwrap()
                .is_none()
        );
        let mut foreign_session = Session::new_macro().unwrap();
        let foreign = foreign_session.data_scalar(&Literal::Number(17.0)).unwrap();
        let before = session.stats().external_value_handles;
        assert!(matches!(
            snapshot.original_member(&mut session, &foreign, Some(&member)),
            Err(SessionError::ForeignValue)
        ));
        assert_eq!(session.stats().external_value_handles, before);
    }
}
