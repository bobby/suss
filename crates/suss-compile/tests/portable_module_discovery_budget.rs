//! Dependency discovery acceptance, separate from source initializer execution.
use std::{collections::BTreeSet, path::Path};
use suss_compile::portable::{
    self, Diagnostic, ExpansionContext, ExpansionHost,
    modules::{
        discover_phase_modules, discover_phase_modules_metered, prepare_modules_with_expander,
    },
    resolve::{Environment, Phase},
};
use suss_reader::forms::Form;

fn chain(root: &Path, count: usize) {
    for index in 0..count {
        let source = if index + 1 < count {
            format!("(ns dep{index} (:require [dep{}]))", index + 1)
        } else {
            format!("(ns dep{index})")
        };
        std::fs::write(root.join(format!("dep{index}.sus")), source).unwrap();
    }
}

#[test]
fn dependency_chain_beyond_sixty_four_preserves_dependency_first_order_and_source() {
    let root = tempfile::tempdir().unwrap();
    chain(root.path(), 96);
    for phase in [Phase::Runtime, Phase::Macro] {
        let graph = discover_phase_modules(
            "dep0",
            &[root.path()],
            &Environment::default(),
            phase,
            &BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(graph.len(), 96);
        for (unit, index) in graph.iter().zip((0..96).rev()) {
            assert_eq!(unit.identity.namespace(), format!("dep{index}"));
            assert_eq!(unit.identity.phase(), phase);
            assert_eq!(unit.source, std::fs::read_to_string(&unit.path).unwrap());
            let expected = if index < 95 {
                vec![format!("dep{}", index + 1)]
            } else {
                vec![]
            };
            assert_eq!(
                unit.dependencies
                    .iter()
                    .map(|id| id.namespace().to_owned())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
}

#[test]
fn deep_cycle_is_reported_before_a_later_require_is_visited() {
    let root = tempfile::tempdir().unwrap();
    chain(root.path(), 80);
    std::fs::write(
        root.path().join("dep79.sus"),
        "(ns dep79 (:require [dep20]))",
    )
    .unwrap();
    std::fs::write(
        root.path().join("entry.sus"),
        "(ns entry (:require [dep0] [later]))",
    )
    .unwrap();
    std::fs::write(root.path().join("later.sus"), "(ns wrong)").unwrap();
    let error = discover_phase_modules(
        "entry",
        &[root.path()],
        &Environment::default(),
        Phase::Runtime,
        &BTreeSet::new(),
    )
    .unwrap_err();
    assert!(
        error
            .message
            .starts_with("Cyclic namespace dependency: Runtime:dep20 -> Runtime:dep21"),
        "{error}"
    );
    assert!(
        error.message.ends_with("Runtime:dep79 -> Runtime:dep20"),
        "{error}"
    );
    assert_eq!(error.namespace, "dep20");
    assert!(error.source_path.unwrap().ends_with("dep79.sus"));
    assert!(error.span.start < error.span.end);
}

#[test]
fn discovery_uses_one_cumulative_allowance_across_all_dependency_files() {
    let root = tempfile::tempdir().unwrap();
    chain(root.path(), 12);
    let environment = Environment::default();
    let provided = BTreeSet::new();
    let mut used = 0_u64;
    let original = discover_phase_modules_metered(
        "dep0",
        &[root.path()],
        &environment,
        Phase::Runtime,
        &provided,
        &mut |work| {
            used += work;
            Ok(())
        },
    )
    .unwrap();
    assert!(used > 12);
    let run = |mut remaining: u64| {
        discover_phase_modules_metered(
            "dep0",
            &[root.path()],
            &environment,
            Phase::Runtime,
            &provided,
            &mut |work| {
                remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                    span: 0..0,
                    message: "caller discovery fuel exhausted".into(),
                })?;
                Ok(())
            },
        )
    };
    assert_eq!(
        run(used - 1).unwrap_err().message,
        "caller discovery fuel exhausted"
    );
    let recovered = run(used).unwrap();
    assert_eq!(
        recovered
            .iter()
            .map(|unit| &unit.identity)
            .collect::<Vec<_>>(),
        original
            .iter()
            .map(|unit| &unit.identity)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        recovered
            .iter()
            .map(|unit| &unit.source)
            .collect::<Vec<_>>(),
        original.iter().map(|unit| &unit.source).collect::<Vec<_>>()
    );
}

#[test]
fn cancellation_keeps_its_diagnostic_and_a_fresh_discovery_recovers() {
    let root = tempfile::tempdir().unwrap();
    chain(root.path(), 3);
    let mut calls = 0;
    let error = discover_phase_modules_metered(
        "dep0",
        &[root.path()],
        &Environment::default(),
        Phase::Runtime,
        &BTreeSet::new(),
        &mut |_| {
            calls += 1;
            if calls == 20 {
                Err(Diagnostic {
                    span: 2..3,
                    message: "caller cancelled discovery".into(),
                })
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(error.message, "caller cancelled discovery");
    assert_eq!(error.span, 2..3);
    assert_eq!(calls, 20);
    assert!(error.source_path.is_some());
    assert_eq!(
        discover_phase_modules(
            "dep0",
            &[root.path()],
            &Environment::default(),
            Phase::Runtime,
            &BTreeSet::new()
        )
        .unwrap()
        .len(),
        3
    );
}

struct EmissionHost {
    limit: u64,
    begins: usize,
    ends: usize,
    emissions: usize,
}
impl ExpansionHost for EmissionHost {
    fn expansion_work_fuel(&self) -> u64 {
        self.limit
    }
    fn begin_expansion_request(&mut self, _: std::ops::Range<usize>) -> Result<(), Diagnostic> {
        self.begins += 1;
        Ok(())
    }
    fn end_expansion_request(&mut self) {
        self.ends += 1;
    }
    fn emit_fragment(
        &mut self,
        function: &portable::ir::Function,
        _: Phase,
        _: &[Form],
        _: Option<&portable::SourceOrigin>,
    ) -> Result<Vec<u8>, Diagnostic> {
        self.emissions += 1;
        portable::compile_ir(function)
    }
    fn expand(&mut self, _: &Form, _: ExpansionContext<'_>) -> Result<Option<Form>, Diagnostic> {
        Ok(None)
    }
}

#[test]
fn preparation_bounds_default_host_work_and_preserves_custom_emission() {
    let root = tempfile::tempdir().unwrap();
    chain(root.path(), 2);
    let mut host = EmissionHost {
        limit: 1,
        begins: 0,
        ends: 0,
        emissions: 0,
    };
    let error = prepare_modules_with_expander(
        "dep0",
        &[root.path()],
        &Environment::default(),
        Phase::Runtime,
        &BTreeSet::new(),
        &mut host,
    )
    .unwrap_err();
    assert!(error.message.contains("work fuel exhausted"), "{error}");
    assert_eq!(host.begins, host.ends);
    assert_eq!(host.emissions, 0);
    host.limit = portable::DEFAULT_OPERATION_FUEL;
    let plan = prepare_modules_with_expander(
        "dep0",
        &[root.path()],
        &Environment::default(),
        Phase::Runtime,
        &BTreeSet::new(),
        &mut host,
    )
    .unwrap();
    assert_eq!(plan.modules.len(), 2);
    assert_eq!(
        host.emissions, 2,
        "request wrappers must retain the actual host's emission override"
    );
    assert_eq!(host.begins, host.ends);
}
