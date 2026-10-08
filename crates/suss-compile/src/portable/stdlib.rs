//! Original Suss library sources selected independently for each execution phase.
use super::resolve::Phase;

pub(super) fn source(namespace: &str, phase: Phase) -> Option<(&'static str, &'static str)> {
    match (namespace, phase) {
        ("suss.async", Phase::Runtime) => Some((
            "<suss-stdlib>/suss/async.sus",
            include_str!("stdlib/async.sus"),
        )),
        ("suss.async", Phase::Macro) => Some((
            "<suss-stdlib>/suss/async-macros.sus",
            include_str!("stdlib/async-macros.sus"),
        )),
        _ => None,
    }
}
