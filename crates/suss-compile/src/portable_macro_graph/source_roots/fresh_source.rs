//! One build's fully checked ordinary source value, not persistent authority.
//! Only a strict result contains a Certificate. Uncached results never expose
//! a domain, CheckedChild conversion, or a RootCache publication path.
use super::*;

enum FreshKind {
    Certified(Rc<Certificate>),
    Uncached(Rc<UncachedSource>),
}
struct UncachedSource {
    source: Rc<Expectation>,
    value: crate::portable_session::SessionValue,
    records: crate::portable_session::construction_records::ConstructionRecords,
    children: Vec<FreshSourceProof>,
    alias_root: bool,
}
pub(crate) struct FreshSourceProof {
    inner: FreshKind,
}
impl FreshSourceProof {
    pub(crate) fn value(&self) -> &crate::portable_session::SessionValue {
        match &self.inner {
            FreshKind::Certified(c) => &c.value,
            FreshKind::Uncached(f) => &f.value,
        }
    }
    pub(super) fn source(&self) -> &Rc<Expectation> {
        match &self.inner {
            FreshKind::Certified(c) => &c.source,
            FreshKind::Uncached(f) => &f.source,
        }
    }
    pub(super) fn certificate(&self) -> Option<&Rc<Certificate>> {
        match &self.inner {
            FreshKind::Certified(c) => Some(c),
            FreshKind::Uncached(_) => None,
        }
    }
    fn same(&self, other: &Self) -> bool {
        match (&self.inner, &other.inner) {
            (FreshKind::Certified(a), FreshKind::Certified(b)) => Rc::ptr_eq(a, b),
            (FreshKind::Uncached(a), FreshKind::Uncached(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    pub(super) fn retain(&self, session: &mut Session) -> Result<Self> {
        session.charge_host_work(2)?; // Rc retain and eventual release
        let inner = match &self.inner {
            FreshKind::Certified(c) => FreshKind::Certified(c.clone()),
            FreshKind::Uncached(f) => FreshKind::Uncached(f.clone()),
        };
        Ok(Self { inner })
    }
    pub(super) fn certified(session: &mut Session, certificate: Rc<Certificate>) -> Result<Self> {
        session.charge_host_work(2)?; // inline discriminator/owner move, no new allocation
        Ok(Self {
            inner: FreshKind::Certified(certificate),
        })
    }
    // Exact current native owner/child order is checked independently of health.
    pub(super) fn checked(
        &self,
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        expected: &Rc<Expectation>,
    ) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        session.charge_host_work(1)?;
        if let FreshKind::Certified(certificate) = &self.inner {
            // Canonical/cacheable children retain the ORIGINAL strict validator
            // and its operation/error order; no additional health walk.
            return Ok(Certificate::checked(session, bridge, certificate, expected)?.is_some());
        }
        session.inspect(self.value(), |_, _| Ok(()))?;
        let mut valid = expected.owner.live(session)?;
        session.charge_host_work(key_comparison_work(&expected.key, &self.source().key))?;
        valid &= expected.key == self.source().key;
        valid &= same_child_keys(session, &expected.children, &self.source().children)?;
        // Keep the original independent current logical comparison after health.
        valid &= self.health(session, bridge)?;
        valid &= expected
            .witness
            .matches_logical_source(session, bridge, self.value())?;
        Ok(valid)
    }
    fn health(
        &self,
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
    ) -> Result<bool> {
        // Explicit postorder stack; repeated references are checked once in this
        // closed read-only pass. No guest callback or mutable Store escapes it.
        let mut pending = Vec::new();
        populate(session, &mut pending, std::iter::once((self, false)))?;
        let mut done: Vec<(&FreshSourceProof, bool)> = Vec::new();
        let mut audit =
            crate::portable_session::certification_audit::CertificationAudit::new(session)?;
        while let Some((proof, exit)) = pending.pop() {
            session.charge_host_work(2)?;
            let mut seen = false;
            for (old, _) in &done {
                session.charge_host_work(4)?;
                if old.same(proof) {
                    seen = true;
                    break;
                }
            }
            if seen {
                continue;
            }
            let valid = match &proof.inner {
                FreshKind::Certified(c) => Certificate::validate(session, bridge, c)?,
                FreshKind::Uncached(f) => {
                    if !exit {
                        populate(session, &mut pending, std::iter::once((proof, true)))?;
                        populate(
                            session,
                            &mut pending,
                            f.children.iter().rev().map(|c| (c, false)),
                        )?;
                        continue;
                    }
                    let mut children_valid = true;
                    for child in &f.children {
                        session.charge_host_work(1)?;
                        let mut found = None;
                        for (old, valid) in &done {
                            session.charge_host_work(4)?;
                            if old.same(child) {
                                found = Some(*valid);
                                break;
                            }
                        }
                        children_valid &=
                            found.ok_or_else(|| invalid("Unchecked fresh source child"))?;
                    }
                    let (healthy, _) = check_fields(
                        session,
                        bridge,
                        &f.source,
                        &f.value,
                        &f.records,
                        &f.children,
                        f.alias_root,
                        &mut audit,
                    )?;
                    children_valid && healthy
                }
            };
            populate(session, &mut done, std::iter::once((proof, valid)))?;
        }
        for (proof, valid) in &done {
            session.charge_host_work(4)?;
            if proof.same(self) {
                return Ok(*valid);
            }
        }
        Err(invalid("Unchecked fresh source"))
    }
    pub(super) fn finish(
        session: &mut Session,
        bridge: &crate::portable_macro_data::FormBridge,
        source: Rc<Expectation>,
        value: crate::portable_session::SessionValue,
        records: crate::portable_session::construction_records::ConstructionRecords,
        children: Vec<FreshSourceProof>,
        alias_root: bool,
    ) -> Result<Self> {
        let _request = session.operation_budget_scope()?;
        // Children were independently checked and physically bound immediately
        // before this call, with no callback between binding and validation.
        let mut audit =
            crate::portable_session::certification_audit::CertificationAudit::new(session)?;
        let (healthy, strict) = check_fields(
            session, bridge, &source, &value, &records, &children, alias_root, &mut audit,
        )?;
        if !healthy {
            return Err(invalid(
                "Fresh coherent source failed non-memo certification",
            ));
        }
        if strict {
            let mut owned = Vec::new();
            reserve(session, &mut owned, children.len())?;
            for child in children {
                session.charge_host_work(2)?;
                // No unchecked conversion: strict above requires every child to
                // contain the original persistent authority type.
                owned.push(child.certificate().unwrap().clone());
            }
            session.charge_host_work(
                (std::mem::size_of::<Certificate>() + 2 * std::mem::size_of::<usize>()) as u64,
            )?;
            let certificate = Rc::new(Certificate {
                source,
                value,
                records,
                children: owned,
                alias_root,
            });
            certificate.records.seal_source_domain(session)?;
            Self::certified(session, certificate)
        } else {
            session.charge_host_work(
                (std::mem::size_of::<UncachedSource>() + 2 * std::mem::size_of::<usize>()) as u64
                    + children.len() as u64
                    + 2,
            )?;
            Ok(Self {
                inner: FreshKind::Uncached(Rc::new(UncachedSource {
                    source,
                    value,
                    records,
                    children,
                    alias_root,
                })),
            })
        }
    }
}

fn check_fields(
    session: &mut Session,
    bridge: &crate::portable_macro_data::FormBridge,
    source: &Expectation,
    value: &crate::portable_session::SessionValue,
    records: &crate::portable_session::construction_records::ConstructionRecords,
    children: &[FreshSourceProof],
    alias_root: bool,
    audit: &mut crate::portable_session::certification_audit::CertificationAudit,
) -> Result<(bool, bool)> {
    session.inspect(value, |_, _| Ok(()))?;
    session.charge_host_work(3)?;
    let mut valid = source.owner.live(session)?;
    valid &= source.children.len() == children.len();
    for (key, child) in source.children.iter().zip(children) {
        session.charge_host_work(key_comparison_work(key, &child.source().key))?;
        valid &= *key == child.source().key;
    }
    valid &= source
        .witness
        .matches_logical_source(session, bridge, value)?;
    let (data, strict) = records.fresh_direct_status(session)?;
    valid &= data;
    session.charge_host_work(children.len() as u64 + 1)?;
    let all_certified = children.iter().all(|child| child.certificate().is_some());
    let inputs = if alias_root {
        session.charge_host_work(3)?;
        if children.len() != 1 || records.len() != 0 {
            false
        } else {
            session.values_identical(value, children[0].value())?
        }
    } else if all_certified {
        records.matches_recorded_inputs(session, value)?
    } else {
        records.matches_fresh_source_inputs(session, value, children)?
    };
    valid &= inputs;
    valid &= records.matches_class_schemas_audited(session, audit)?;
    valid &= records.matches_prototype_origins_audited(session, audit)?;
    Ok((valid, valid && strict && all_certified))
}
