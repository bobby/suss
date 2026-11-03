//! The shipped core's analysis round-trips through `emit-form` (#299 step 2).
use std::path::Path;
use suss_compile::portable::{bootstrap, resolve::Phase};
use suss_compile::portable_analyze::{comparable_asts, core_document, emitted_source, hir_document};
use suss_reader::edn::print;

#[test]
fn core_round_trips_in_both_phases() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let document = core_document(phase).unwrap();
        let emitted = emitted_source(&document).unwrap();
        let again = bootstrap::analyze_core_source(&emitted, phase)
            .unwrap_or_else(|error| panic!("{phase:?}: {error}"));
        let second = hir_document(&again, Path::new("suss/core.sus")).unwrap();
        let (a, b) = (print(&comparable_asts(&document)).unwrap(), print(&comparable_asts(&second)).unwrap());
        if a != b {
            let common = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
            let start = common.saturating_sub(600);
            panic!(
                "{phase:?}: round trip differs at byte {common} of {}\nfirst:  {}\nsecond: {}",
                a.len(),
                &a[start..(common + 300).min(a.len())],
                &b[start..(common + 300).min(b.len())],
            );
        }
    }
}

/// Every core form is projected with source facts: none is left as a
/// `:suss/lowered` node, which emit-form cannot emit.
#[test]
fn core_document_has_no_lowered_nodes() {
    let document = core_document(Phase::Runtime).unwrap();
    let text = print(&document).unwrap();
    let asts = match &document {
        suss_reader::edn::Value::Map(map) => match map.get(&suss_reader::edn::Value::keyword("asts")) {
            Some(suss_reader::edn::Value::Vector(asts)) => asts.len(),
            _ => 0,
        },
        _ => 0,
    };
    assert!(asts > 600, "{asts} top-level nodes");
    assert!(!text.contains(":op :suss/lowered"), "a core node has no source facts");
    for op in [":op :fn-method", ":op :loop", ":op :try", ":op :suss/this-as", ":op :suss/intrinsic"] {
        assert!(text.contains(op), "{op}");
    }
}
