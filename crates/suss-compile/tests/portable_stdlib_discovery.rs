//! Embedded source discovery is not suspension or scheduler acceptance.
use std::{collections::BTreeSet, path::PathBuf};
use suss_compile::portable::{
    modules::{SourceModule, discover_phase_modules},
    resolve::{Environment, Phase},
};

fn discover(
    namespace: &str,
    roots: &[PathBuf],
    phase: Phase,
) -> Result<Vec<SourceModule>, suss_compile::portable::modules::ModuleDiagnostic> {
    discover_phase_modules(
        namespace,
        roots,
        &Environment::default(),
        phase,
        &BTreeSet::new(),
    )
}

#[test]
fn original_async_macro_source_is_available_without_a_checkout_in_both_phases() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let modules = discover("suss.async", &[], phase).unwrap();
        assert_eq!(modules.len(), 1);
        let module = &modules[0];
        assert_eq!(module.identity.phase(), phase);
        assert_eq!(module.identity.namespace(), "suss.async");
        let (path, source) = match phase {
            Phase::Runtime => (
                "<suss-stdlib>/suss/async.sus",
                include_str!("../src/portable/stdlib/async.sus"),
            ),
            Phase::Macro => (
                "<suss-stdlib>/suss/async-macros.sus",
                include_str!("../src/portable/stdlib/async-macros.sus"),
            ),
        };
        assert_eq!(module.path, PathBuf::from(path));
        assert_eq!(module.source, source);
        assert!(module.dependencies.is_empty());
    }
}

#[test]
fn explicit_source_selection_precedes_embedded_fallback() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("suss")).unwrap();
    let path = root.path().join("suss/async.sus");
    let source = "(ns suss.async) (def selected 7)";
    std::fs::write(&path, source).unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let modules = discover("suss.async", &[root.path().to_owned()], phase).unwrap();
        assert_eq!(modules[0].path, path.canonicalize().unwrap());
        assert_eq!(modules[0].source, source);
    }
}

#[test]
fn embedded_source_does_not_hide_ambiguous_files_or_unknown_namespaces() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("suss")).unwrap();
    for extension in ["sus", "cljc"] {
        std::fs::write(
            root.path().join(format!("suss/async.{extension}")),
            "(ns suss.async)",
        )
        .unwrap();
    }
    let error = discover("suss.async", &[root.path().to_owned()], Phase::Runtime).unwrap_err();
    assert!(error.message.contains("Ambiguous source"), "{error}");
    let error = discover("not.bundled", &[], Phase::Runtime).unwrap_err();
    assert_eq!(error.message, "No source for namespace not.bundled");
}

#[cfg(unix)]
#[test]
fn embedded_source_does_not_hide_filesystem_resolution_errors() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("suss")).unwrap();
    let path = root.path().join("suss/async.sus");
    std::os::unix::fs::symlink("async.sus", &path).unwrap();
    let error = discover("suss.async", &[root.path().to_owned()], Phase::Runtime).unwrap_err();
    assert!(
        error.message.contains("Cannot resolve namespace source"),
        "{error}"
    );
}

#[test]
fn embedded_source_does_not_hide_an_explicit_directory() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("suss/async.sus")).unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let error = discover("suss.async", &[root.path().to_owned()], phase).unwrap_err();
        assert!(error.message.contains("not a file"), "{error}");
    }
}

#[cfg(unix)]
#[test]
fn embedded_source_does_not_hide_a_dangling_selected_symlink() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("suss")).unwrap();
    std::os::unix::fs::symlink("missing.sus", root.path().join("suss/async.sus")).unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let error = discover("suss.async", &[root.path().to_owned()], phase).unwrap_err();
        assert!(
            error.message.contains("Cannot resolve namespace source"),
            "{error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn embedded_source_does_not_hide_a_dangling_source_directory_symlink() {
    let root = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("missing-directory", root.path().join("suss")).unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let error = discover("suss.async", &[root.path().to_owned()], phase).unwrap_err();
        assert!(
            error.message.contains("Cannot resolve namespace source"),
            "{error}"
        );
    }
}
