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
        match &self.inner { FreshKind::Certified(c) => Some(c), FreshKind::Uncached(_) => None }
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
        Ok(Self { inner: FreshKind::Certified(certificate) })
    }
    // Exact current native owner/child order is checked independently of health.
    pub(super) fn checked(
        &self, session: &mut Session, bridge: &crate::portable_macro_data::FormBridge,
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
        valid &= expected.witness.matches_logical_source(session, bridge, self.value())?;
        Ok(valid)
    }
    fn health(&self, session: &mut Session, bridge: &crate::portable_macro_data::FormBridge) -> Result<bool> {
        // Explicit postorder stack; repeated references are checked once in this
        // closed read-only pass. No guest callback or mutable Store escapes it.
        let mut pending = Vec::new();
        populate(session, &mut pending, std::iter::once((self, false)))?;
        let mut done: Vec<(&FreshSourceProof, bool)> = Vec::new();
        let mut audit = crate::portable_session::certification_audit::CertificationAudit::new(session)?;
        while let Some((proof, exit)) = pending.pop() {
            session.charge_host_work(2)?;
            let mut seen = false;
            for (old, _) in &done {
                session.charge_host_work(4)?;
                if old.same(proof) { seen = true; break; }
            }
            if seen { continue; }
            let valid = match &proof.inner {
                FreshKind::Certified(c) => Certificate::validate(session, bridge, c)?,
                FreshKind::Uncached(f) => {
                    if !exit {
                        populate(session, &mut pending, std::iter::once((proof, true)))?;
                        populate(session, &mut pending, f.children.iter().rev().map(|c| (c, false)))?;
                        continue;
                    }
                    let mut children_valid = true;
                    for child in &f.children {
                        session.charge_host_work(1)?;
                        let mut found = None;
                        for (old, valid) in &done {
                            session.charge_host_work(4)?;
                            if old.same(child) { found = Some(*valid); break; }
                        }
                        children_valid &= found.ok_or_else(|| invalid("Unchecked fresh source child"))?;
                    }
                    let (healthy, _) = check_fields(session, bridge, &f.source, &f.value,
                        &f.records, &f.children, f.alias_root, &mut audit)?;
                    children_valid && healthy
                }
            };
            populate(session, &mut done, std::iter::once((proof, valid)))?;
        }
        for (proof, valid) in &done {
            session.charge_host_work(4)?;
            if proof.same(self) { return Ok(*valid); }
        }
        Err(invalid("Unchecked fresh source"))
    }
    pub(super) fn finish(
        session: &mut Session, bridge: &crate::portable_macro_data::FormBridge,
        source: Rc<Expectation>, value: crate::portable_session::SessionValue,
        records: crate::portable_session::construction_records::ConstructionRecords,
        children: Vec<FreshSourceProof>, alias_root: bool,
    ) -> Result<Self> {
        let _request = session.operation_budget_scope()?;
        // Children were independently checked and physically bound immediately
        // before this call, with no callback between binding and validation.
        let mut audit = crate::portable_session::certification_audit::CertificationAudit::new(session)?;
        let (healthy, strict) = check_fields(session, bridge, &source, &value,
            &records, &children, alias_root, &mut audit)?;
        if !healthy { return Err(invalid("Fresh coherent source failed non-memo certification")); }
        if strict {
            let mut owned = Vec::new();
            reserve(session, &mut owned, children.len())?;
            for child in children {
                session.charge_host_work(2)?;
                // No unchecked conversion: strict above requires every child to
                // contain the original persistent authority type.
                owned.push(child.certificate().unwrap().clone());
            }
            session.charge_host_work((std::mem::size_of::<Certificate>() + 2 * std::mem::size_of::<usize>()) as u64)?;
            let certificate = Rc::new(Certificate { source, value, records, children: owned, alias_root });
            certificate.records.seal_source_domain(session)?;
            Self::certified(session, certificate)
        } else {
            session.charge_host_work((std::mem::size_of::<UncachedSource>() + 2 * std::mem::size_of::<usize>()) as u64 + children.len() as u64 + 2)?;
            Ok(Self { inner: FreshKind::Uncached(Rc::new(UncachedSource {
                source, value, records, children, alias_root,
            })) })
        }
    }
}

fn check_fields(
    session: &mut Session, bridge: &crate::portable_macro_data::FormBridge,
    source: &Expectation, value: &crate::portable_session::SessionValue,
    records: &crate::portable_session::construction_records::ConstructionRecords,
    children: &[FreshSourceProof], alias_root: bool,
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
    valid &= source.witness.matches_logical_source(session, bridge, value)?;
    let (data, strict) = records.fresh_direct_status(session)?;
    valid &= data;
    session.charge_host_work(children.len() as u64 + 1)?;
    let all_certified = children.iter().all(|child| child.certificate().is_some());
    let inputs = if alias_root {
        session.charge_host_work(3)?;
        if children.len() != 1 || records.len() != 0 { false }
        else { session.values_identical(value, children[0].value())? }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable_macro_data::FormBridge;
    use crate::portable_session::{SessionValue, construction_records::ConstructionRecords,
        memo_leaf_test_support::Method};

    // Obtain the actual selected metadata field from a real analyzed revision.
    // No guest object provides the native logical expectation or field ordinal.
    fn metadata(session: &mut Session, bridge: &FormBridge,
        global: &Global, revision: &Arc<DefinitionInfo>,
    ) -> (Rc<Expectation>, SessionValue, ConstructionRecords) {
        let _request = session.operation_budget_scope().unwrap();
        let mut graph = AnalysisGraph::new(bridge, session);
        graph.capture_roots = true;
        let root = graph.declaration(global, Some(revision), 0).unwrap();
        graph.finish_recipes().unwrap();
        let native = graph.source_roots.native(graph.session, root).unwrap();
        let meta_key = graph.keyword("meta").unwrap();
        let Recipe::Map(entries) = graph.recipes[native].as_ref().unwrap() else { panic!("declaration map") };
        let meta = entries.iter().find(|(key, _)| *key == meta_key).unwrap().1;
        let id = graph.source_roots.index(graph.session, meta).unwrap().unwrap();
        let expected = graph.source_root_expectation(id).unwrap().unwrap();
        assert!(expected.children.is_empty(), "fixture is an independently owned leaf field");
        let recording = bridge.recording(graph.session).unwrap();
        // The unrelated Symbol recreates the confirmed mixed-domain obligation.
        let _unused = recording.identifier(graph.session, None, "unused-fresh-symbol", false, None).unwrap();
        let value = graph.materialize_using(meta, &recording).unwrap();
        (expected, value, recording.into_records().unwrap())
    }

    #[test]
    fn fresh_source_uncached_memo_keeps_value_and_sibling_but_rejects_poison_and_alias() {
        let mut session = Session::new_macro().unwrap();
        session.eval("(def ^{:audit-leaf :good-fresh-leaf} fresh-good 17) (def ^{:audit-leaf :soft-fresh-leaf} fresh-soft 18) (def leaf-original-method nil)").unwrap();
        let source = hir::SourceNamespace::capture(&session.compilation_snapshot().environment, session.phase());
        let select = |name: &str| source.declarations.iter().find(|(g, _)| g.name() == name)
            .map(|(g, info)| (g.clone(), info.clone())).unwrap();
        let good = select("fresh-good");
        let soft = select("fresh-soft");
        let original = Method::capture(&mut session, "Symbol").unwrap();
        original.bind_delegate(&mut session).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let (good_expected, good_value, good_records) = metadata(&mut session, &bridge, &good.0, &good.1);
        let good_proof = FreshSourceProof::finish(&mut session, &bridge, good_expected.clone(),
            good_value.clone(), good_records, vec![], false).unwrap();
        let mut cache = RootCache::default();
        assert!(cache.publish(&mut session, &bridge, &good_expected, good_proof.certificate().unwrap()).unwrap());
        session.eval("(extend-type Symbol IHash (-hash [self] (leaf-original-method self)))").unwrap();
        let (expected, value, records) = metadata(&mut session, &bridge, &soft.0, &soft.1);
        let read = session.eval("(fn [m] (:audit-leaf m))").unwrap();
        let leaf = session.invoke(&read, &[&value]).unwrap();
        let hash = session.eval("(fn [x] (hash x))").unwrap();
        drop(session.invoke(&hash, &[&leaf]).unwrap());
        assert_eq!(records.fresh_direct_status(&mut session).unwrap(), (true, false));
        let proof = FreshSourceProof::finish(&mut session, &bridge, expected.clone(),
            value.clone(), records, vec![], false).unwrap();
        assert!(proof.certificate().is_none(), "no CheckedChild/domain or cache authority");
        assert!(session.values_identical(proof.value(), &value).unwrap());
        assert!(proof.checked(&mut session, &bridge, &expected).unwrap());
        assert!(cache.lookup(&mut session, &bridge, &expected).unwrap().is_none());
        session.collect().unwrap();
        // A second native graph selects the same genuine sibling revision.
        let (next_good, _, _) = metadata(&mut session, &bridge, &good.0, &good.1);
        let sibling = cache.lookup(&mut session, &bridge, &next_good).unwrap().unwrap();
        assert!(session.values_identical(sibling.value(), &good_value).unwrap());
        session.inspect(&leaf, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.field(&mut store, 1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let name = fields.get(&mut store, 1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            name.set(&mut store, 0, wasmtime::Val::I32('x' as i32))
        }).unwrap();
        assert!(!proof.checked(&mut session, &bridge, &expected).unwrap(), "nonmemo poison cannot become a soft result");
        let (expected, value, records) = metadata(&mut session, &bridge, &good.0, &good.1);
        assert!(FreshSourceProof::finish(&mut session, &bridge, expected, value,
            records, vec![], true).is_err(), "alias binding needs its exact sole child");
        original.restore(&mut session).unwrap();
    }
}
