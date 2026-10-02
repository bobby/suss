use std::collections::BTreeSet;
use suss_compile::portable::{
    modules::{ModuleIdentity, discover_phase_modules},
    resolve::{Environment, Phase},
};

#[test]
fn phase_graph_preserves_order_and_distinguishes_same_source_in_both_stores() {
    let root = tempfile::tempdir().unwrap();
    let helper = "(ns helper) (def bias 2)";
    std::fs::write(root.path().join("helper.sus"), helper).unwrap();
    std::fs::write(
        root.path().join("tools.sus"),
        "(ns tools (:require [helper :as h])) (defmacro add-two [x] (list '+ x h/bias))",
    )
    .unwrap();
    std::fs::write(root.path().join("app.sus"),
        "(ns app (:require [helper :as h]) (:require-macros [tools :as t :refer [add-two] :rename {add-two plus-two}])) (t/add-two h/bias)").unwrap();
    let graph = discover_phase_modules(
        "app",
        &[root.path()],
        &Environment::default(),
        Phase::Runtime,
        &BTreeSet::new(),
    )
    .unwrap();
    let identities = graph
        .iter()
        .map(|unit| (unit.identity.phase(), unit.identity.namespace()))
        .collect::<Vec<_>>();
    assert_eq!(
        identities,
        [
            (Phase::Runtime, "helper"),
            (Phase::Macro, "helper"),
            (Phase::Macro, "tools"),
            (Phase::Runtime, "app")
        ]
    );
    assert_eq!(graph[0].path, graph[1].path);
    assert_eq!(graph[0].source, helper);
    assert_eq!(graph[0].forms, graph[1].forms);
    assert_eq!(
        graph[3].dependencies,
        [
            ModuleIdentity::new(Phase::Runtime, "helper").unwrap(),
            ModuleIdentity::new(Phase::Macro, "tools").unwrap()
        ]
    );
}

#[test]
fn phase_graph_cycle_and_missing_diagnostics_locate_the_source_edge() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.sus"), "(ns a (:require-macros [b]))").unwrap();
    std::fs::write(root.path().join("b.sus"), "(ns b (:require [a]))").unwrap();
    let error = discover_phase_modules(
        "a",
        &[root.path()],
        &Environment::default(),
        Phase::Runtime,
        &BTreeSet::new(),
    )
    .unwrap_err();
    assert!(
        error.message.contains("Macro:b -> Macro:a -> Macro:b"),
        "{error}"
    );
    assert!(error.source_path.unwrap().ends_with("a.sus"));
    assert!(error.span.end > error.span.start);
    std::fs::write(root.path().join("b.sus"), "(ns b (:require [missing]))").unwrap();
    let error = discover_phase_modules(
        "a",
        &[root.path()],
        &Environment::default(),
        Phase::Runtime,
        &BTreeSet::new(),
    )
    .unwrap_err();
    assert_eq!(error.namespace, "missing");
    assert!(error.source_path.unwrap().ends_with("b.sus"));
    assert!(error.span.end > error.span.start);
}

#[test]
fn phase_graph_requires_actual_provided_phase_catalog_and_shared_header_validation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        "(ns app (:require-macros [tools :as t]))",
    )
    .unwrap();
    let mut environment = Environment::default();
    environment
        .declare_namespace(Phase::Runtime, "tools")
        .unwrap();
    let provided = BTreeSet::from([ModuleIdentity::new(Phase::Macro, "tools").unwrap()]);
    let error = discover_phase_modules(
        "app",
        &[root.path()],
        &environment,
        Phase::Runtime,
        &provided,
    )
    .unwrap_err();
    assert!(
        error.message.contains("no matching phase declaration"),
        "{error}"
    );
    environment
        .declare_namespace(Phase::Macro, "tools")
        .unwrap();
    let graph = discover_phase_modules(
        "app",
        &[root.path()],
        &environment,
        Phase::Runtime,
        &provided,
    )
    .unwrap();
    assert_eq!(graph.len(), 1);
    std::fs::write(
        root.path().join("app.sus"),
        "(ns app (:require-macros [tools :rename {x y}]))",
    )
    .unwrap();
    let error = discover_phase_modules(
        "app",
        &[root.path()],
        &environment,
        Phase::Runtime,
        &provided,
    )
    .unwrap_err();
    assert!(error.message.contains("must be referred"), "{error}");
}
