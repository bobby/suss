//! Authored, unexecuted tests of original DATA edge relevance and root isolation.
use super::*;
use crate::portable::hir::Literal;
use crate::portable_macro_data::MetadataShape;

fn symbol(session: &mut Session, bridge: &FormBridge, name: &str) -> Evidence {
    let recording = bridge.recording(session).unwrap();
    let value = recording.identifier(session, None, name, false, None).unwrap();
    let records = recording.into_records().unwrap();
    Evidence::Native(Native::allocate(session, &value, records, Vec::new()).unwrap())
}
fn native_records(evidence: &Evidence) -> &ConstructionRecords {
    match evidence { Evidence::Native(node) => &node.records,
        Evidence::Source(_) => unreachable!("fixture allocates actual native factories") }
}
fn nil(session: &mut Session, bridge: &FormBridge) -> Evidence {
    let recording = bridge.recording(session).unwrap();
    let value = recording.scalar(session, &Literal::Nil).unwrap();
    let records = recording.into_records().unwrap();
    Evidence::Native(Native::allocate(session, &value, records, Vec::new()).unwrap())
}
#[test]
fn original_data_descendants_close_copied_fields_but_never_supply_an_unrecorded_root_or_sibling() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        session.with_operation_budget(|session| {
            let base = symbol(session, &bridge, "original-data-base");
            let metadata = nil(session, &bridge);
            let recording = bridge.recording(session)?;
            let copy = recording.with_form_metadata(session, MetadataShape::Symbol,
                base.value(), metadata.value(), &[])?;
            let records = recording.into_records()?;
            let mut graph = AnalysisGraph::new(&bridge, session);
            let mut audit = Audit::new(graph.session)?;
            assert!(audit.check(&mut graph, &base)?);
            assert!(audit.check(&mut graph, &metadata)?);
            let direct = [base.value(), metadata.value()];
            assert!(!records.matches_current_native_inputs(graph.session, &copy, &direct)?,
                "direct roots do not prove inherited physical fields");
            let actual = [base.retained(graph.session)?, metadata.retained(graph.session)?];
            {
                let mut closure = OriginalDataInputs::new(graph.session, &actual, &mut audit.checked, &records)?;
                assert!(records.matches_current_native_inputs_with_descendants(
                    graph.session, &copy, &direct, &mut closure)?);
                assert!(!records.matches_current_native_inputs_with_descendants(
                    graph.session, base.value(), &direct, &mut closure)?,
                    "even the correctly bound DATA closure cannot supply an absent local output root");
                let empty = ConstructionRecords::default();
                assert!(!empty.matches_current_native_inputs_with_descendants(
                    graph.session, base.value(), &direct, &mut closure)?,
                    "a child root cannot authenticate an absent local factory");
            }
            // base is already healthy in audit.checked, but is NOT reachable
            // from this deliberately unrelated current input frontier.
            let unrelated = [metadata.retained(graph.session)?];
            let mut closure = OriginalDataInputs::new(graph.session, &unrelated, &mut audit.checked, &records)?;
            assert!(!records.matches_current_native_inputs_with_descendants(
                graph.session, &copy, &[metadata.value()], &mut closure)?,
                "unrelated earlier healthy sibling cannot close copied edges");
            Ok(())
        }).unwrap();
        session.collect().unwrap();
    }
}
#[test]
fn original_data_descendants_require_current_health_and_cannot_use_an_unchecked_ledger() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    session.with_operation_budget(|session| {
        let base = symbol(session, &bridge, "unchecked-original");
        let values = [base.retained(session)?];
        let mut audit = Audit::new(session)?;
        let mut closure = OriginalDataInputs::new(session, &values, &mut audit.checked, native_records(&values[0]))?;
        assert!(!closure.contains_original(session, base.value())?,
            "an original record is not a current health proof");
        Ok(())
    }).unwrap();
}

#[test]
fn original_data_descendant_seed_membership_stays_initial_private_and_non_authoritative() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        session.with_operation_budget(|session| {
            let recording = bridge.recording(session)?;
            let value = recording.map_values(session, &[])?;
            let fields = session.data_fields(&value)?; // test fixture only
            let records = recording.into_records()?;
            assert_eq!(records.canonical_seed_fact_count_for_test(), 1,
                "empty-map fixture must use the genuine private CanonicalSeed route");
            assert_eq!(records.len(), 1, "seed fixture must not silently become a new factory allocation");
            let seed = Evidence::Native(Native::allocate(session, &value, records, Vec::new())?);
            let foreign_reference = symbol(session, &bridge, "not-in-seed-initial-storage");
            let mut graph = AnalysisGraph::new(&bridge, session);
            let mut audit = Audit::new(graph.session)?;
            assert!(audit.check(&mut graph, &seed)?);
            let inputs = [seed.retained(graph.session)?];
            let mut closure = OriginalDataInputs::new(graph.session, &inputs, &mut audit.checked, native_records(&inputs[0]))?;
            for field in &fields {
                assert!(closure.contains_original(graph.session, field)?,
                    "original seed fields require original private membership plus current DATA health");
            }
            assert!(!closure.contains_original(graph.session, foreign_reference.value())?,
                "a fresh reference cannot enter the initial membership set");
            let empty = ConstructionRecords::default();
            assert!(!empty.matches_current_native_inputs_with_descendants(
                graph.session, &value, &[&value], &mut closure)?,
                "seed membership never authorizes an unrecorded output root");
            Ok(())
        }).unwrap();
        session.collect().unwrap();
    }
}
#[test]
fn original_data_descendant_new_work_fuel_irq_and_fresh_audit_retry_preserve_original_inputs() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let (base, candidate) = session.with_operation_budget(|session| {
        let base = symbol(session, &bridge, "metered-original-descendant");
        let fields = session.data_fields(base.value())?; // test fixture only
        Ok((base, fields[1].clone()))
    }).unwrap();
    let inputs = [base];
    let measure = session.with_operation_budget(|session| {
        let mut graph = AnalysisGraph::new(&bridge, session);
        let mut audit = Audit::new(graph.session)?;
        assert!(audit.check(&mut graph, &inputs[0])?);
        let before = graph.session.operation_fuel_remaining()?;
        let mut closure = OriginalDataInputs::new(graph.session, &inputs, &mut audit.checked, native_records(&inputs[0]))?;
        assert!(closure.contains_original(graph.session, &candidate)?);
        Ok(before - graph.session.operation_fuel_remaining()?)
    }).unwrap();
    assert!(measure > 1);
    for allowance in [0, 1, measure - 1] {
        let error = session.with_operation_budget(|session| {
            let mut graph = AnalysisGraph::new(&bridge, session);
            let mut audit = Audit::new(graph.session)?;
            assert!(audit.check(&mut graph, &inputs[0])?);
            // Burn only this test's remaining budget AFTER genuine child
            // auditing, without refill or guest callback. No production cap changes.
            let remaining = graph.session.operation_fuel_remaining()?;
            graph.session.charge_host_work(remaining.checked_sub(allowance).unwrap())?;
            let mut closure = OriginalDataInputs::new(graph.session, &inputs, &mut audit.checked, native_records(&inputs[0]))?;
            closure.contains_original(graph.session, &candidate)
        }).unwrap_err();
        assert!(matches!(&error, SessionError::Trap(e)
            if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
        session.with_operation_budget(|session| {
            let mut graph = AnalysisGraph::new(&bridge, session);
            let mut audit = Audit::new(graph.session)?;
            assert!(audit.check(&mut graph, &inputs[0])?);
            let mut closure = OriginalDataInputs::new(graph.session, &inputs, &mut audit.checked, native_records(&inputs[0]))?;
            assert!(closure.contains_original(graph.session, &candidate)?);
            Ok(())
        }).unwrap();
    }
    let error = session.with_operation_budget(|session| {
        let mut graph = AnalysisGraph::new(&bridge, session);
        let mut audit = Audit::new(graph.session)?;
        assert!(audit.check(&mut graph, &inputs[0])?);
        graph.session.interrupt_handle().interrupt(); // active operation
        let mut closure = OriginalDataInputs::new(graph.session, &inputs, &mut audit.checked, native_records(&inputs[0]))?;
        closure.contains_original(graph.session, &candidate)
    }).unwrap_err();
    assert!(error.is_interrupt(), "{error:?}");
    session.with_operation_budget(|session| {
        let mut graph = AnalysisGraph::new(&bridge, session);
        let mut audit = Audit::new(graph.session)?;
        assert!(audit.check(&mut graph, &inputs[0])?);
        let mut closure = OriginalDataInputs::new(graph.session, &inputs, &mut audit.checked, native_records(&inputs[0]))?;
        assert!(closure.contains_original(graph.session, &candidate)?);
        Ok(())
    }).unwrap();
    session.collect().unwrap();
}

#[test]
fn original_data_descendants_do_not_lend_original_references_after_nonmemo_poison() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let base = session.with_operation_budget(|session| Ok(symbol(session, &bridge, "before-poison"))).unwrap();
    let candidate = session.data_fields(base.value()).unwrap()[1].clone();
    let poison = session.eval("(fn [value] (set! (.-name value) \"after-poison\"))").unwrap();
    session.invoke(&poison, &[base.value()]).unwrap();
    session.with_operation_budget(|session| {
        let mut graph = AnalysisGraph::new(&bridge, session);
        let mut audit = Audit::new(graph.session)?;
        assert!(!audit.check(&mut graph, &base)?, "actual nonmemo mutation must invalidate original factory health");
        let inputs = [base.retained(graph.session)?];
        let mut closure = OriginalDataInputs::new(graph.session, &inputs, &mut audit.checked, native_records(&inputs[0]))?;
        assert!(!closure.contains_original(graph.session, &candidate)?,
            "a still-rooted original reference is not proof of healthy current input");
        Ok(())
    }).unwrap();
}
