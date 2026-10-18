//! Actual-source admission and mixed-plan domain regression tests.
//! Exactly two tests. Uses actual initialize/cache; no staging or fault framework.
use super::*;
use crate::portable_macro_graph::{
    form_plan_input::{Index, Legacy, Native, Plans},
    raw_keys::{Plan, compiled_prefix::GraphInterner},
};

fn registry(phase: Phase, session: &mut Session) -> Arc<Registry> {
    // Normal regenerated bootstrap is prerequisite. This setup deliberately uses
    // actual private shipped owners; no synthetic positive authority fixture.
    bootstrap::shipped(phase).unwrap();
    session
        .with_operation_budget(|session| initialize(phase, session))
        .unwrap()
        .unwrap()
}
fn fixture_global(phase: Phase, namespace: &str, name: &str) -> Global {
    crate::portable::resolve::Environment::default()
        .declare_cell(phase, namespace, name)
        .unwrap()
}

#[test]
fn genuine_field_admission_and_consumption_reject_equal_copies_and_wrong_domains() {
    let mut session = Session::new().unwrap();
    let runtime = registry(Phase::Runtime, &mut session);
    let macro_registry = registry(Phase::Macro, &mut session);
    for (registry, other) in [(&runtime, &macro_registry), (&macro_registry, &runtime)] {
        let binding = registry
            .bindings
            .iter()
            .find(|b| b.fields[0].is_some())
            .unwrap();
        let (global, owner, form) = (
            &binding.global,
            &binding.owner,
            &binding.owner.definition_form,
        );
        let equal_form = form.clone();
        assert_eq!(&equal_form, form);
        assert!(!std::ptr::eq(&equal_form, form));
        let equal_owner = Arc::new((**owner).clone());
        let wrong_phase = fixture_global(other.phase, global.namespace(), global.name());
        let wrong_name = fixture_global(
            registry.phase,
            global.namespace(),
            "native-runtime-wrong-global-control",
        );
        let wrong_namespace = fixture_global(registry.phase, "native.runtime.wrong", global.name());
        session
            .with_operation_budget(|session| {
                assert!(
                    registry
                        .lookup(session, global, owner, Selector::Definition, form)?
                        .is_some()
                );
                assert!(
                    registry
                        .lookup(session, global, owner, Selector::Definition, &equal_form)?
                        .is_none()
                );
                assert!(
                    registry
                        .lookup(
                            session,
                            global,
                            &equal_owner,
                            Selector::Definition,
                            &equal_owner.definition_form
                        )?
                        .is_none(),
                    "equal contents cannot confer owner authority"
                );
                for wrong in [&wrong_phase, &wrong_name, &wrong_namespace] {
                    assert!(
                        registry
                            .lookup(session, wrong, owner, Selector::Definition, form)?
                            .is_none()
                    );
                }
                assert!(
                    registry
                        .lookup(session, global, owner, Selector::Declaration, form)?
                        .is_none()
                );
                assert!(
                    registry
                        .lookup(session, global, owner, Selector::Initializer, form)?
                        .is_none()
                );
                assert!(
                    registry
                        .lookup(
                            session,
                            global,
                            owner,
                            Selector::Declaration,
                            &owner.declaration
                        )?
                        .is_some(),
                    "correct alternate selector remains usable"
                );
                let mut scratch = Plan::default();
                let mut adapter = Native {
                    plan: registry
                        .lookup(session, global, owner, Selector::Definition, form)?
                        .unwrap(),
                    generated: Legacy { plan: &mut scratch },
                };
                let mut same = GraphInterner::with_prefix(&registry.prefix, &mut |u| {
                    session.charge_host_work(u)
                })?;
                let mut legacy = GraphInterner::default();
                let mut other_prefix = GraphInterner::with_prefix(&other.prefix, &mut |u| {
                    session.charge_host_work(u)
                })?;
                assert!(same.has_prefix(&registry.prefix));
                assert!(!legacy.has_prefix(&registry.prefix));
                assert!(!other_prefix.has_prefix(&registry.prefix));
                for wrong in [&mut legacy, &mut other_prefix] {
                    assert!(adapter.root(form, wrong, session).is_err());
                    assert!(adapter.generated.plan.nodes.is_empty());
                }
                assert!(adapter.root(&equal_form, &mut same, session).is_err());
                assert!(
                    adapter.generated.plan.nodes.is_empty(),
                    "reject before generated scratch writes"
                );
                assert!(
                    matches!(adapter.root(form, &mut same, session)?, Index::Native(0)),
                    "rejections must leave genuine binding usable"
                );
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn tagged_native_generated_overlap_matches_original_rows_keys_and_metadata() {
    let mut session = Session::new().unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let registry = registry(phase, &mut session);
        // One genuine DEFINITION field with actual raw metadata, not a million
        // full-table fault rebuilds. Setup selection is not a savings metric.
        let binding = registry
            .bindings
            .iter()
            .find(|b| {
                b.fields[0].is_some_and(|f| {
                    let f = &registry.data.fields[f];
                    registry.data.nodes[f.nodes_start..f.nodes_start + f.nodes_len]
                        .iter()
                        .any(|node| node.metadata_count != 0)
                })
            })
            .expect("genuine definition with metadata-bearing row");
        let form = &binding.owner.definition_form;
        let literal = suss_reader::forms::read_forms("\"native-runtime-generated-index-control\"")
            .unwrap()
            .remove(0);
        session.with_operation_budget(|session| {
            let plan = registry.lookup(session, &binding.global, &binding.owner,
                Selector::Definition, form)?.unwrap();
            let mut raw = GraphInterner::with_prefix(&registry.prefix, &mut |u| session.charge_host_work(u))?;
            // Runs the unchanged BYTE walker, sharing only canonical storage,
            // NOT compiled native rows or the native adapter's occurrence route.
            let mut original = Plan::default();
            let old_root = raw.plan(form, &mut original, &mut |u| session.charge_host_work(u))?;
            assert_eq!(old_root, 0);
            assert_eq!(plan.len(), original.nodes.len());
            let mut saw_metadata = false;
            for (index, expected) in original.nodes.iter().enumerate() {
                let (key, metadata, children) = plan.node(index, session)?;
                assert_eq!(key, expected.key, "original global BYTE key at row {index}");
                assert_eq!(metadata, expected.metadata_count);
                assert_eq!(children, expected.children.as_slice(), "original LOCAL metadata/body child order");
                saw_metadata |= metadata != 0;
            }
            assert!(saw_metadata);
            let mut scratch = Plan::default();
            let mut adapter = Native { plan, generated: Legacy { plan: &mut scratch } };
            let native = adapter.root(form, &mut raw, session)?;
            let generated = adapter.generated(&literal, &mut raw, session)?;
            assert!(matches!(native, Index::Native(0)));
            assert!(matches!(generated, Index::Generated(0)), "force equal numeric index in distinct domains");
            let native_key = adapter.key(native, session)?;
            let generated_key = adapter.key(generated, session)?;
            assert_eq!(native_key, original.nodes[0].key);
            assert_ne!(native_key, generated_key);
            assert_eq!(generated_key, adapter.generated.plan.nodes[0].key);
            assert_eq!(adapter.metadata(native, session)?, original.nodes[0].metadata_count);
            assert_eq!(adapter.metadata(generated, session)?, 0);
            assert!(adapter.child(generated, 0, session).is_err(), "literal cannot read native children");
            for ordinal in 0..original.nodes[0].children.len() {
                let child = adapter.child(native, ordinal, session)?;
                assert!(matches!(child, Index::Native(i) if i == original.nodes[0].children[ordinal]));
                assert_eq!(adapter.key(child, session)?, original.nodes[original.nodes[0].children[ordinal]].key);
            }
            assert_eq!(adapter.key(native, session)?, native_key, "generated zero cannot replace native zero");
            original.clear_metered(&mut |u| session.charge_host_work(u))?;
            adapter.generated.plan.clear_metered(&mut |u| session.charge_host_work(u))?;
            Ok(())
        }).unwrap();
    }
}
