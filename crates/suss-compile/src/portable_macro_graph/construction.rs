//! A captured declaration owner paired with an explicit construction recording.
//! Missing seeds and direct-fact mismatches decline this audit; no reuse authority.
use super::{AnalysisGraph, Result, metadata_expectations::MetadataExpectation};
use crate::{
    portable_macro_data::FormBridge,
    portable_session::{Session, SessionValue, construction_records::ConstructionRecords},
};

use std::rc::Rc;

pub(super) struct RecordedMetadata {
    source: Rc<MetadataExpectation>,
    pub(super) value: SessionValue,
    records: ConstructionRecords,
}
impl AnalysisGraph<'_> {
    pub(super) fn record_metadata(&mut self, index: usize) -> Result<Option<RecordedMetadata>> {
        let _request = self.session.operation_budget_scope()?;
        let tracing = std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some();
        let trace = |session: &Session, stage: &str| -> Result<()> {
            if tracing {
                eprintln!("metadata-fresh {index} {stage}: {}", session.operation_fuel_remaining()?);
            }
            Ok(())
        };
        trace(self.session, "start")?;
        self.finish_recipes()?;
        let Some(source) = self.metadata_expectation(index)? else {
            return Ok(None);
        };
        trace(self.session, "expected")?;
        let root = self.metadata_sources[index].recipe;
        let bridge = self.bridge.recording(self.session)?;
        trace(self.session, "bridge")?;
        let value = self.materialize_using(root, &bridge)?;
        trace(self.session, "constructed")?;
        let records = bridge.into_records()?;
        trace(self.session, "recorded")?;
        Ok(Some(RecordedMetadata {
            source,
            value,
            records,
        }))
    }
}
impl RecordedMetadata {
    pub(super) fn has_source(&self, source: &Rc<MetadataExpectation>) -> bool {
        Rc::ptr_eq(&self.source, source)
    }
    pub(super) fn source(&self) -> &MetadataExpectation {
        &self.source
    }
    pub(super) fn record_count(&self) -> usize {
        self.records.len()
    }
    pub(super) fn external_seeds(&self, session: &mut Session) -> Result<usize> {
        self.records.external_seeds(session)
    }
    pub(super) fn matches_recorded_source(
        &self, session: &mut Session, bridge: &FormBridge,
    ) -> Result<bool> {
        self.check_recorded_source(session, bridge, None)
    }
    pub(super) fn matches_recorded_source_audited(
        &self, session: &mut Session, bridge: &FormBridge,
        audit: &mut crate::portable_session::certification_audit::CertificationAudit,
    ) -> Result<bool> {
        self.check_recorded_source(session, bridge, Some(audit))
    }
    fn check_recorded_source(
        &self, session: &mut Session, bridge: &FormBridge,
        mut audit: Option<&mut crate::portable_session::certification_audit::CertificationAudit>,
    ) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        let tracing = std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some();
        let trace = |session: &Session, stage: &str| -> Result<()> {
            if tracing {
                eprintln!("metadata-fuel {}/{} {stage}: {}", self.source.global().namespace(), self.source.global().name(), session.operation_fuel_remaining()?);
            }
            Ok(())
        };
        trace(session, "start")?;
        let source = self
            .source
            .matches_logical_source(session, bridge, &self.value)?;
        trace(session, "logical")?;
        let records = self.records.matches_direct_facts(session)?;
        trace(session, "direct")?;
        let inputs = self.records.matches_recorded_inputs(session, &self.value)?;
        trace(session, "inputs")?;
        // Class/schema authority is revalidated after logical inspection and
        // construction callbacks; no earlier false suppresses this check.
        let classes = match audit.as_deref_mut() {
            Some(audit) => self.records.matches_class_schemas_audited(session, audit)?,
            None => self.records.matches_class_schemas(session)?,
        };
        trace(session, "classes")?;
        let prototypes = match audit.as_deref_mut() {
            Some(audit) => self.records.matches_prototype_origins_audited(session, audit)?,
            None => self.records.matches_prototype_origins(session)?,
        };
        trace(session, "prototypes")?;
        Ok(source && records && inputs && classes && prototypes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::{hir::SourceNamespace, resolve::Phase};
    use std::sync::Arc;
    fn session(phase: Phase) -> Session {
        match phase {
            Phase::Runtime => Session::new_repl().unwrap(),
            Phase::Macro => Session::new_macro().unwrap(),
        }
    }
    fn record(session: &mut Session, bridge: &FormBridge, name: &str) -> RecordedMetadata {
        let snapshot = session.compilation_snapshot();
        let namespace = SourceNamespace::capture(&snapshot.environment, session.phase());
        let (global, revision) = namespace
            .declarations
            .iter()
            .find(|(global, _)| global.name() == name)
            .unwrap();
        let mut graph = AnalysisGraph::new(bridge, session);
        graph.declaration(global, Some(revision), 0).unwrap();
        let record = graph.record_metadata(0).unwrap().unwrap();
        assert_eq!(record.source().global(), global);
        assert!(Arc::ptr_eq(record.source().revision(), revision));
        record
    }
    #[test]
    fn construction_records_bind_source_revision_and_detect_later_nested_poison() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            session
                .eval("(def ^{:audit {:x 17}} recording-probe 17)")
                .unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let first = record(&mut session, &bridge, "recording-probe");
            assert!(first.record_count() > 0);
            assert_eq!(first.external_seeds(&mut session).unwrap(), 0);
            session.collect().unwrap();
            assert!(
                first
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap()
            );
            let mutate = session
                .eval("(fn [m] (aset (.-arr (get m :audit)) 1 99))")
                .unwrap();
            drop(session.invoke(&mutate, &[&first.value]).unwrap());
            session.collect().unwrap();
            assert!(
                !first
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap()
            );
            let read = session.eval("(fn [m] (get (get m :audit) :x))").unwrap();
            let old = session.invoke(&read, &[&first.value]).unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &old).unwrap(),
                "99"
            );
            session
                .eval("(def ^{:audit {:x 18}} recording-probe 18)")
                .unwrap();
            let second = record(&mut session, &bridge, "recording-probe");
            assert!(!Arc::ptr_eq(
                first.source().revision(),
                second.source().revision()
            ));
            assert!(
                second
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap()
            );
            let value = session.invoke(&read, &[&second.value]).unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &value).unwrap(),
                "18"
            );
        }
    }
    #[test]
    fn construction_records_preserve_reader_hash_counts_and_authenticate_only_macro_warming() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            session.eval("(def recorded-events 0) (def recorded-order []) (def recorded-old-hash suss.core/hash) (def ^{:items #{101 102 103 104 105 106 107 108 109}} recording-effects 17)").unwrap();
            session.enter_namespace("suss.core").unwrap();
            session.eval("(def hash (fn [key] (do (if (number? key) (do (set! user/recorded-events (+ user/recorded-events 1)) (set! user/recorded-order (conj user/recorded-order key))) nil) (user/recorded-old-hash key))))").unwrap();
            session.enter_namespace("user").unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let snapshot = session.compilation_snapshot();
            let namespace = SourceNamespace::capture(&snapshot.environment, phase);
            let (global, revision) = namespace
                .declarations
                .iter()
                .find(|(global, _)| global.name() == "recording-effects")
                .unwrap();
            session
                .eval("(set! recorded-events 0) (set! recorded-order [])")
                .unwrap();
            {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                graph.declaration(global, Some(revision), 0).unwrap();
                let root = graph.metadata_sources[0].recipe;
                drop(graph.materialize(root).unwrap());
            }
            let normal = session.eval("recorded-events").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &normal).unwrap(),
                "9"
            );
            let order = session.eval("recorded-order").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &order).unwrap(),
                "[101 102 103 104 105 106 107 108 109]"
            );
            session
                .eval("(set! recorded-events 0) (set! recorded-order [])")
                .unwrap();
            let recorded = record(&mut session, &bridge, "recording-effects");
            let count = session.eval("recorded-events").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &count).unwrap(),
                "9"
            );
            let order = session.eval("recorded-order").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &order).unwrap(),
                "[101 102 103 104 105 106 107 108 109]"
            );
            assert!(
                recorded
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap()
            );
            let hash = session.eval("(fn [m] (user/recorded-old-hash m))").unwrap();
            drop(session.invoke(&hash, &[&recorded.value]).unwrap());
            session.collect().unwrap();
            assert_eq!(
                recorded
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap(),
                phase == Phase::Macro,
                "only instrumented Macro stores provide provisional memo receipts"
            );
            let forge = session.eval("(fn [m] (set! (.-__hash m) 999))").unwrap();
            drop(session.invoke(&forge, &[&recorded.value]).unwrap());
            session.collect().unwrap();
            assert!(recorded.source.matches_logical_source(&mut session, &bridge, &recorded.value).unwrap(),
                "the forged memo must leave the logical source value unchanged");
            assert!(!recorded.matches_recorded_source(&mut session, &bridge).unwrap(),
                "an ordinary field write cannot replace an authenticated memo");
        }
    }
    #[test]
    fn construction_records_verify_provisioned_seeds_and_share_validation_budget() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            session
                .eval("(def ^{:top-fn {:meta {}}} recording-empty (fn [] 17))")
                .unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let recorded = record(&mut session, &bridge, "recording-empty");
            assert_eq!(recorded.external_seeds(&mut session).unwrap(), 0);
            assert!(
                recorded
                    .source
                    .matches_logical_source(&mut session, &bridge, &recorded.value)
                    .unwrap()
            );
            assert!(recorded.records.matches_direct_facts(&mut session).unwrap());
            assert!(
                recorded
                    .records
                    .matches_recorded_inputs(&mut session, &recorded.value)
                    .unwrap()
            );
            assert!(
                recorded
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap()
            );
            for interrupt in [false, true] {
                let request = session.operation_budget_scope().unwrap();
                if interrupt {
                    session.interrupt_handle().interrupt();
                } else {
                    session.charge_host_work(u64::MAX).unwrap_err();
                }
                let error = recorded
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap_err();
                if interrupt {
                    assert!(error.is_interrupt(), "{error:?}");
                } else {
                    assert!(
                        matches!(error,crate::portable_session::SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>()==Some(&wasmtime::Trap::OutOfFuel)),
                        "{error:?}"
                    );
                }
                drop(request);
            }
            assert!(
                recorded
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap()
            );
        }
    }
    #[test]
    fn construction_records_reject_seed_poison_predating_bridge_capture() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            session
                .eval("(def ^{:audit {}} recording-seed 17)")
                .unwrap();
            let empty = session
                .eval("(.-EMPTY suss.core/PersistentArrayMap)")
                .unwrap();
            let poison = session.eval("{:seedPoison 99}").unwrap();
            session
                .inspect(&empty, |mut store, value| {
                    let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let fields = object
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let poison = poison.rooted(&mut store);
                    fields.set(&mut store, 0, wasmtime::Val::AnyRef(Some(poison)))?;
                    Ok(())
                })
                .unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let recorded = record(&mut session, &bridge, "recording-seed");
            assert_eq!(recorded.external_seeds(&mut session).unwrap(), 0);
            assert!(!recorded.records.matches_direct_facts(&mut session).unwrap());
            assert!(
                !recorded
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap()
            );
        }
    }
    #[test]
    fn construction_records_reject_child_poison_during_parent_hash_callback() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            session.eval("(def recorded-child-events []) (def recorded-child-hash suss.core/hash) (def ^{:items #{:recording-target :other1 :other2 :other3 :other4 :other5 :other6 :other7 :other8}} recording-child 17)").unwrap();
            session.enter_namespace("suss.core").unwrap();
            session.eval("(def hash (fn [key] (if (= key :recording-target) (do (set! user/recorded-child-events (conj user/recorded-child-events (.-name key))) (set! (.-name key) \"changed\") 719) (user/recorded-child-hash key))))").unwrap();
            session.enter_namespace("user").unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let recorded = record(&mut session, &bridge, "recording-child");
            let events = session.eval("recorded-child-events").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &events).unwrap(),
                "[\"recording-target\"]"
            );
            let post_callback = session.capture_storage(&recorded.value).unwrap();
            session.collect().unwrap();
            assert!(
                session
                    .storage_matches(&recorded.value, &post_callback)
                    .unwrap()
            );
            assert!(
                !recorded.records.matches_direct_facts(&mut session).unwrap(),
                "construction facts independently reject the callback poison"
            );
            assert!(
                !recorded
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap()
            );
            let read = session
                .eval("(fn [m] (count (filter (fn [key] (= (.-name key) \"changed\")) (get m :items))))")
                .unwrap();
            let changed = session.invoke(&read, &[&recorded.value]).unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &changed).unwrap(),
                "1"
            );
        }
    }
    #[test]
    fn recorded_class_schema_audit_rejects_late_changes_outside_used_classes() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            session
                .eval("(def ^{:audit {:x 17}} class-schema-recording 17)")
                .unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let recorded = record(&mut session, &bridge, "class-schema-recording");
            assert!(
                recorded
                    .records
                    .matches_class_schemas(&mut session)
                    .unwrap()
            );
            let class = session.eval("suss.core/ArrayChunk").unwrap();
            let descriptor = session.data_descriptor(&class, true).unwrap();
            let poison = session.eval(r#""wrong-schema""#).unwrap();
            session
                .inspect(&descriptor, |mut store, value| {
                    let descriptor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let schema = descriptor
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let poison = poison.rooted(&mut store);
                    schema.set(&mut store, 0, wasmtime::Val::AnyRef(Some(poison)))
                })
                .unwrap();
            session.collect().unwrap();
            assert!(
                !recorded
                    .records
                    .matches_class_schemas(&mut session)
                    .unwrap()
            );
            assert!(
                !recorded
                    .matches_recorded_source(&mut session, &bridge)
                    .unwrap()
            );
        }
    }
}
