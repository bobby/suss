//! One Analyzer's native namespace factory. This is not a certificate cache.
//! Retained guest objects still pass the complete existing source/physical proof.
use super::{
    Diagnostic, ExpansionContext,
    hir::SourceNamespace,
    resolve::{Environment, Phase},
};
use std::{cell::RefCell, mem::size_of, sync::Arc};

type Meter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;

struct Revision {
    generation: u64,
    phase: Phase,
    scope: Arc<SourceNamespace>,
}

/// Private to one Analyzer and never installed in Environment or a process map.
/// The Environment is owned by that Analyzer throughout this factory's lifetime.
/// A cloned/branched Environment must get its own factory, even at equal generation.
pub(super) struct NamespaceFactory {
    current: RefCell<Option<Revision>>,
}

/// Borrowed capability minted only by the Analyzer's private factory. Neither
/// callers nor guest data can supply a cached SourceNamespace or a generation.
/// Public struct-literal ExpansionContexts use None for this optional field.
pub struct NamespaceRevision<'a> {
    factory: &'a NamespaceFactory,
    environment: &'a Environment,
    phase: Phase,
}

impl NamespaceFactory {
    pub(super) fn new_metered(meter: &mut Meter<'_>) -> Result<Self, Diagnostic> {
        // Inline initialization, later moves and retirement, including RefCell.
        meter((2 * size_of::<Self>() + 4) as u64)?;
        Ok(Self {
            current: RefCell::new(None),
        })
    }

    pub(super) fn borrow_revision<'a>(
        &'a self,
        environment: &'a Environment,
        phase: Phase,
        meter: &mut Meter<'_>,
    ) -> Result<NamespaceRevision<'a>, Diagnostic> {
        // No source reads/capture here. Non-macro forms retain lazy behavior.
        meter((2 * size_of::<NamespaceRevision<'a>>() + 4) as u64)?;
        Ok(NamespaceRevision {
            factory: self,
            environment,
            phase,
        })
    }

    pub(super) fn capture(
        &self,
        environment: &Environment,
        phase: Phase,
        meter: &mut Meter<'_>,
    ) -> Result<Arc<SourceNamespace>, Diagnostic> {
        // Snapshot the cache with a prepaid retain, then release the RefCell
        // borrow BEFORE any fallible/user-supplied meter callback.
        meter((2 * size_of::<Option<Revision>>() + 12) as u64)?;
        let cached = self.current.borrow().as_ref().map(|entry| Revision {
            generation: entry.generation,
            phase: entry.phase,
            scope: entry.scope.clone(),
        });
        let generation = environment.source_generation();
        let namespace = environment.current_namespace(phase);
        if let Some(entry) = cached {
            // Charge actual bounded text comparison before reading its bytes.
            meter((entry.scope.namespace.len().min(namespace.len()) + 8) as u64)?;
            if entry.generation == generation
                && entry.phase == phase
                && entry.scope.namespace == namespace
            {
                // Transfer the already prepaid snapshot Arc; no second retain.
                return Ok(entry.scope);
            }
        }
        // Fixed factory backing/retain work is distinct from the complete variable
        // metered builder below. No cache replacement happens until it succeeds.
        meter(
            (size_of::<SourceNamespace>() + 2 * size_of::<usize>() + 2 * size_of::<Revision>() + 16)
                as u64,
        )?;
        let snapshot = environment.capture_source_namespace_metered(phase, meter)?;
        meter((2 * size_of::<usize>() + 4) as u64)?; // Arc header retirement before acquisition
        let scope = Arc::new(snapshot);
        *self.current.borrow_mut() = Some(Revision {
            generation,
            phase,
            scope: scope.clone(),
        });
        Ok(scope)
    }
}

pub(super) fn catalog(
    context: &ExpansionContext<'_>,
    meter: &mut Meter<'_>,
) -> Result<Arc<SourceNamespace>, Diagnostic> {
    meter(6)?;
    if let Some(revision) = &context.namespace_revision {
        // Exact live native borrow binding, NOT guest pointer trust or a source
        // verdict. This capability cannot outlive/rebind its Environment borrow.
        meter(6)?;
        if std::ptr::eq(revision.environment, context.environment)
            && revision.phase == context.phase
        {
            return revision
                .factory
                .capture(context.environment, context.phase, meter);
        }
    }
    // Manually assembled/foreign contexts retain old fresh native capture.
    // No foreign query can overwrite the genuine Analyzer's cache.
    meter((size_of::<SourceNamespace>() + 2 * size_of::<usize>() + 4) as u64)?;
    let snapshot = context
        .environment
        .capture_source_namespace_metered(context.phase, meter)?;
    meter((2 * size_of::<usize>() + 4) as u64)?; // Arc header retirement before acquisition
    Ok(Arc::new(snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factory() -> NamespaceFactory {
        NamespaceFactory::new_metered(&mut |_| Ok(())).unwrap()
    }
    fn context<'a>(
        environment: &'a Environment,
        snapshot: &'a Arc<SourceNamespace>,
        revision: Option<NamespaceRevision<'a>>,
        locals: &'a super::super::hir::SourceLocals,
        fields: &'a super::super::hir::SourceFields,
    ) -> ExpansionContext<'a> {
        ExpansionContext {
            environment,
            namespace_snapshot: snapshot,
            namespace_revision: revision,
            phase: Phase::Runtime,
            origin: None,
            context: super::super::AnalysisContext::Expression,
            locals,
            fields,
            function_scopes: &[],
        }
    }

    #[test]
    fn analyzer_namespace_factory_keeps_frozen_snapshot_and_live_revision_distinct() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut environment = Environment::new("revision.user").unwrap();
            let factory = factory();
            let snapshot = factory
                .capture(&environment, phase, &mut |_| Ok(()))
                .unwrap();
            let live = factory
                .capture(&environment, phase, &mut |_| Ok(()))
                .unwrap();
            assert!(Arc::ptr_eq(&snapshot, &live));
            let unrelated = environment
                .declare_cell(phase, "revision.user", "unrelated")
                .unwrap();
            let next = factory
                .capture(&environment, phase, &mut |_| Ok(()))
                .unwrap();
            assert!(!Arc::ptr_eq(&snapshot, &next));
            assert!(!snapshot.identities.contains(&unrelated));
            assert!(next.identities.contains(&unrelated));
            let new_top = factory
                .capture(&environment, phase, &mut |_| Ok(()))
                .unwrap();
            assert!(Arc::ptr_eq(&next, &new_top));
        }
    }

    #[test]
    fn analyzer_namespace_capability_is_lazy_and_foreign_borrow_cannot_rebind_cache() {
        let environment = Environment::new("revision.user").unwrap();
        let foreign = Environment::new("foreign.user").unwrap();
        let factory = factory();
        let revision = factory
            .borrow_revision(&environment, Phase::Runtime, &mut |_| Ok(()))
            .unwrap();
        assert!(
            factory.current.borrow().is_none(),
            "minting the capability must not capture"
        );
        let snapshot = Arc::new(SourceNamespace::capture(&environment, Phase::Runtime));
        let locals = Default::default();
        let fields = Default::default();
        let foreign_context = context(&foreign, &snapshot, Some(revision), &locals, &fields);
        let result = foreign_context
            .resolution_catalog_metered(&mut |_| Ok(()))
            .unwrap();
        assert_eq!(result.namespace, "foreign.user");
        assert!(
            factory.current.borrow().is_none(),
            "foreign query must not populate factory"
        );
        let genuine = context(
            &environment,
            &snapshot,
            Some(
                factory
                    .borrow_revision(&environment, Phase::Runtime, &mut |_| Ok(()))
                    .unwrap(),
            ),
            &locals,
            &fields,
        );
        let first = genuine.resolution_catalog_metered(&mut |_| Ok(())).unwrap();
        let second = genuine.resolution_catalog_metered(&mut |_| Ok(())).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(
            !Arc::ptr_eq(&first, &snapshot),
            "unrelated equal snapshot is not interned"
        );
        let legacy = context(&environment, &snapshot, None, &locals, &fields);
        let a = legacy.resolution_catalog_metered(&mut |_| Ok(())).unwrap();
        let b = legacy.resolution_catalog_metered(&mut |_| Ok(())).unwrap();
        assert!(!Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn analyzer_namespace_nonmacro_host_does_not_request_a_live_capture() {
        struct Decline(usize);
        impl super::super::ExpansionHost for Decline {
            fn expand(
                &mut self,
                _form: &suss_reader::forms::Form,
                context: ExpansionContext<'_>,
            ) -> Result<Option<suss_reader::forms::Form>, Diagnostic> {
                let revision = context
                    .namespace_revision
                    .as_ref()
                    .expect("genuine Analyzer capability");
                assert!(std::ptr::eq(revision.environment, context.environment));
                // The top-level capture is already present. Merely constructing
                // a nonmacro context must not replace it with a fresh live Arc.
                let entry = revision.factory.current.borrow();
                assert!(Arc::ptr_eq(
                    &entry.as_ref().unwrap().scope,
                    context.namespace_snapshot
                ));
                self.0 += 1;
                Ok(None)
            }
        }
        let source = "(if true 17 18)";
        let forms = suss_reader::forms::read_forms(source).unwrap();
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut host = Decline(0);
            super::super::hir::prepare_with_expander(
                &forms,
                0..source.len(),
                &Environment::new("revision.user").unwrap(),
                phase,
                &mut host,
            )
            .unwrap();
            assert!(host.0 > 0);
        }
    }

    #[test]
    fn analyzer_namespace_equal_generation_branches_keep_independent_factories() {
        let mut a = Environment::new("revision.user").unwrap();
        let mut b = a.clone();
        let ga = a
            .declare_cell(Phase::Runtime, "revision.user", "left")
            .unwrap();
        let gb = b
            .declare_cell(Phase::Runtime, "revision.user", "right")
            .unwrap();
        assert_eq!(a.source_generation(), b.source_generation());
        let fa = factory();
        let fb = factory();
        let ra = fa.capture(&a, Phase::Runtime, &mut |_| Ok(())).unwrap();
        let rb = fb.capture(&b, Phase::Runtime, &mut |_| Ok(())).unwrap();
        assert!(!Arc::ptr_eq(&ra, &rb));
        assert!(ra.identities.contains(&ga) && !ra.identities.contains(&gb));
        assert!(rb.identities.contains(&gb) && !rb.identities.contains(&ga));
    }

    // All fields are supplied through genuine native analysis/configuration;
    // no guest object, forged source owner or registry flag is manufactured.
    fn representative_environment(phase: Phase) -> Environment {
        let source = "(def ^number local 17) (def other 23)";
        let forms = suss_reader::forms::read_forms(source).unwrap();
        let (_, mut env) = super::super::hir::prepare(
            &forms,
            0..source.len(),
            &Environment::new("revision.user").unwrap(),
            phase,
        )
        .unwrap();
        env.declare_cell(phase, "outside.ns", "outside").unwrap();
        env.alias(phase, "external", "outside.ns").unwrap();
        env.refer(phase, "outside", "outside.ns", "outside")
            .unwrap();
        env.refer(phase, "renamed", "outside.ns", "outside")
            .unwrap();
        env.exclude_core(phase, "inc").unwrap();
        env.declare_macro_exports(phase, "external.macros", &["macro-entry".into()])
            .unwrap();
        env.macro_alias(phase, "mac", "external.macros").unwrap();
        env.macro_refer(phase, "macro-entry", "external.macros", "macro-entry")
            .unwrap();
        env.macro_refer(phase, "renamed-macro", "external.macros", "macro-entry")
            .unwrap();
        let (_, hidden, _) = env.reader_sequence_cells(phase);
        let local = env
            .resolve(
                phase,
                &suss_reader::Symbol {
                    namespace: None,
                    name: "local".into(),
                },
                0..0,
            )
            .unwrap()
            .global()
            .clone();
        // Genuine recorded native identity; exercise duplicate binding/bootstrap
        // membership while the real reader/internal cells stay hidden.
        env.materialize_bootstrap(local);
        assert!(env.is_hidden_cell(phase, hidden.namespace(), hidden.name()));
        env
    }
    fn same_snapshot(actual: &SourceNamespace, expected: &SourceNamespace) {
        assert_eq!(actual.namespace, expected.namespace);
        assert_eq!(actual.aliases, expected.aliases);
        assert_eq!(actual.refers, expected.refers);
        assert_eq!(actual.used_refers, expected.used_refers);
        assert_eq!(actual.renamed_refers, expected.renamed_refers);
        assert_eq!(actual.excluded_core, expected.excluded_core);
        assert_eq!(actual.macro_aliases, expected.macro_aliases);
        assert_eq!(actual.macro_refers, expected.macro_refers);
        assert_eq!(actual.used_macro_refers, expected.used_macro_refers);
        assert_eq!(actual.renamed_macro_refers, expected.renamed_macro_refers);
        assert_eq!(actual.identities, expected.identities);
        assert_eq!(actual.declarations.len(), expected.declarations.len());
        for (global, info) in &expected.declarations {
            assert!(Arc::ptr_eq(&actual.declarations[global], info));
        }
    }
    #[test]
    fn analyzer_namespace_variable_capture_matches_original_fields_and_exact_info_owners() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let environment = representative_environment(phase);
            let expected = SourceNamespace::capture(&environment, phase);
            let before = environment.source_generation();
            let actual = environment
                .capture_source_namespace_metered(phase, &mut |_| Ok(()))
                .unwrap();
            same_snapshot(&actual, &expected);
            same_snapshot(&SourceNamespace::capture(&environment, phase), &expected);
            assert_eq!(environment.source_generation(), before);
            assert!(!actual.aliases.is_empty() && !actual.refers.is_empty());
            assert!(!actual.used_refers.is_empty() && !actual.renamed_refers.is_empty());
            assert!(!actual.macro_aliases.is_empty() && !actual.macro_refers.is_empty());
            assert!(
                !actual.used_macro_refers.is_empty() && !actual.renamed_macro_refers.is_empty()
            );
            assert_eq!(actual.declarations.len(), 2);
            let mut hidden_scope = environment.clone();
            hidden_scope
                .enter_namespace(phase, "suss.internal.reader")
                .unwrap();
            assert!(
                hidden_scope
                    .cells()
                    .iter()
                    .any(|g| g.phase() == phase && g.namespace() == "suss.internal.reader")
            );
            let expected_hidden = SourceNamespace::capture(&hidden_scope, phase);
            let actual_hidden = hidden_scope
                .capture_source_namespace_metered(phase, &mut |_| Ok(()))
                .unwrap();
            same_snapshot(&actual_hidden, &expected_hidden);
            assert!(
                actual_hidden.identities.is_empty(),
                "real InternalCell stays hidden"
            );
        }
    }
    #[test]
    fn analyzer_namespace_every_variable_charge_boundary_preserves_cache_environment_and_retry() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let environment = representative_environment(phase);
            for mode in 0..3 {
                // cold / hit / replacement, same native owner per trial
                let setup = || {
                    let mut env = environment.clone();
                    let factory = factory();
                    let held = if mode == 0 {
                        None
                    } else {
                        Some(factory.capture(&env, phase, &mut |_| Ok(())).unwrap())
                    };
                    if mode == 2 {
                        env.declare_cell(phase, "revision.user", "next").unwrap();
                    }
                    (env, factory, held)
                };
                let (env, measured, _) = setup();
                let mut costs = Vec::new();
                measured
                    .capture(&env, phase, &mut |work| {
                        costs.push(work);
                        Ok(())
                    })
                    .unwrap();
                // Capture shares live revisions: its charges cover the scope
                // query, namespace text and retained roots, and do not grow
                // with the namespace. Every one of them is still a cut point.
                if mode != 1 {
                    assert!(costs.len() > 3);
                    let mut larger = env.clone();
                    for index in 0..200 {
                        larger.declare_cell(phase, "revision.user", &format!("extra-{index}")).unwrap();
                    }
                    let fresh_costs = |env: &Environment| {
                        let mut costs = Vec::new();
                        factory().capture(env, phase, &mut |work| { costs.push(work); Ok(()) }).unwrap();
                        costs
                    };
                    assert_eq!(fresh_costs(&larger), fresh_costs(&env), "capture work is independent of namespace size");
                }
                for cut in 0..costs.len() {
                    let (env, trial, held) = setup();
                    let expected = SourceNamespace::capture(&env, phase);
                    let generation = env.source_generation();
                    let owner_counts: Vec<_> = expected
                        .declarations
                        .values()
                        .map(Arc::strong_count)
                        .collect();
                    let mut index = 0;
                    let failure = trial
                        .capture(&env, phase, &mut |_| {
                            let fail = index == cut;
                            index += 1;
                            if fail {
                                Err(Diagnostic {
                                    span: 7..9,
                                    message: "namespace injected cut".into(),
                                })
                            } else {
                                Ok(())
                            }
                        })
                        .unwrap_err();
                    assert_eq!(failure.span, 7..9);
                    assert_eq!(failure.message, "namespace injected cut");
                    match &*trial.current.borrow() {
                        None => assert!(held.is_none()),
                        Some(entry) => assert!(Arc::ptr_eq(&entry.scope, held.as_ref().unwrap())),
                    }
                    assert_eq!(env.source_generation(), generation);
                    assert_eq!(
                        expected
                            .declarations
                            .values()
                            .map(Arc::strong_count)
                            .collect::<Vec<_>>(),
                        owner_counts,
                        "failed partial capture must release retained Info owners"
                    );
                    same_snapshot(&SourceNamespace::capture(&env, phase), &expected);
                    let retry = trial.capture(&env, phase, &mut |_| Ok(())).unwrap();
                    same_snapshot(&retry, &expected);
                    if mode == 1 {
                        assert!(Arc::ptr_eq(&retry, held.as_ref().unwrap()));
                    }
                    if mode == 2 {
                        assert!(!Arc::ptr_eq(&retry, held.as_ref().unwrap()));
                    }
                }
                let total = costs
                    .iter()
                    .try_fold(0u64, |a, b| a.checked_add(*b))
                    .unwrap();
                for allowance in [0, 1, total - 1] {
                    let (env, trial, held) = setup();
                    let mut remaining = allowance;
                    let failure = trial
                        .capture(&env, phase, &mut |work| {
                            remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                                span: 11..13,
                                message: "namespace native fuel exhausted".into(),
                            })?;
                            Ok(())
                        })
                        .unwrap_err();
                    assert_eq!(failure.message, "namespace native fuel exhausted");
                    match &*trial.current.borrow() {
                        None => assert!(held.is_none()),
                        Some(entry) => assert!(Arc::ptr_eq(&entry.scope, held.as_ref().unwrap())),
                    }
                    let retry = trial.capture(&env, phase, &mut |_| Ok(())).unwrap();
                    same_snapshot(&retry, &SourceNamespace::capture(&env, phase));
                }
            }
        }
    }
    #[test]
    fn analyzer_namespace_manual_fallback_variable_cuts_retry_without_factory_or_source_change() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let environment = representative_environment(phase);
            let snapshot = Arc::new(SourceNamespace::capture(&environment, phase));
            let locals = Default::default();
            let fields = Default::default();
            let mut context = context(&environment, &snapshot, None, &locals, &fields);
            context.phase = phase;
            let mut count = 0;
            context
                .resolution_catalog_metered(&mut |_| {
                    count += 1;
                    Ok(())
                })
                .unwrap();
            assert!(count > 3, "manual fallback still has several capture cut points");
            for cut in 0..count {
                let mut index = 0;
                let failure = context
                    .resolution_catalog_metered(&mut |_| {
                        let fail = index == cut;
                        index += 1;
                        if fail {
                            Err(Diagnostic {
                                span: 17..19,
                                message: "manual interrupted capture".into(),
                            })
                        } else {
                            Ok(())
                        }
                    })
                    .unwrap_err();
                assert_eq!(failure.span, 17..19);
                assert_eq!(failure.message, "manual interrupted capture");
                same_snapshot(&SourceNamespace::capture(&environment, phase), &snapshot);
                let retry = context.resolution_catalog_metered(&mut |_| Ok(())).unwrap();
                same_snapshot(&retry, &snapshot);
                assert!(!Arc::ptr_eq(&retry, &snapshot));
            }
        }
    }
}
