//! Records analyzed top-level forms for tooling (#299 step 2), while a caller
//! asks for them. Compilation drops each fragment's HIR once it is lowered;
//! `suss analyze` runs the ordinary compilation pipeline with recording on.
use super::{SourceOrigin, hir::Hir, resolve};
use std::cell::RefCell;

/// One analyzed fragment: its top-level forms' HIR, in order, and the
/// environment after them.
pub struct RecordedFragment {
    pub phase: resolve::Phase,
    pub origin: Option<SourceOrigin>,
    pub namespace_directive: Option<suss_reader::forms::Form>,
    pub forms: Vec<Hir>,
    pub environment: resolve::Environment,
}

/// Which fragments to record, by the source they were analyzed from.
pub type Filter = Box<dyn Fn(Option<&SourceOrigin>) -> bool>;

thread_local! {
    static RECORDING: RefCell<Option<Vec<RecordedFragment>>> = const { RefCell::new(None) };
    static FILTER: RefCell<Option<Filter>> = const { RefCell::new(None) };
    static PENDING: RefCell<Option<Vec<Hir>>> = const { RefCell::new(None) };
}

/// Run `f`, returning what it returns and the fragments analyzed meanwhile on
/// this thread from a source that `filter` accepts. Others, such as the core
/// a session restores, are not copied. Recordings do not nest.
pub fn record<T>(filter: Filter, f: impl FnOnce() -> T) -> (T, Vec<RecordedFragment>) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            RECORDING.with(|r| r.borrow_mut().take());
            FILTER.with(|r| r.borrow_mut().take());
            PENDING.with(|p| p.borrow_mut().take());
        }
    }
    assert!(!recording(), "analysis recordings do not nest");
    RECORDING.with(|r| *r.borrow_mut() = Some(Vec::new()));
    FILTER.with(|r| *r.borrow_mut() = Some(filter));
    let reset = Reset;
    let value = f();
    let fragments = RECORDING.with(|r| r.borrow_mut().take()).unwrap_or_default();
    drop(reset);
    (value, fragments)
}

fn recording() -> bool {
    RECORDING.with(|r| r.borrow().is_some())
}

fn wanted(origin: Option<&SourceOrigin>) -> bool {
    recording() && FILTER.with(|f| f.borrow().as_ref().is_none_or(|filter| filter(origin)))
}

/// Called by the analyzer with a fragment's analyzed top-level forms.
pub(super) fn analyzed_forms(origin: Option<&SourceOrigin>, forms: &[Hir]) {
    if wanted(origin) {
        PENDING.with(|p| *p.borrow_mut() = Some(forms.to_vec()));
    }
}

/// Called once the fragment whose forms were just analyzed is complete.
pub(super) fn fragment(
    phase: resolve::Phase,
    origin: Option<&SourceOrigin>,
    namespace_directive: Option<&suss_reader::forms::Form>,
    environment: &resolve::Environment,
) {
    if !wanted(origin) {
        return;
    }
    let forms = PENDING.with(|p| p.borrow_mut().take()).unwrap_or_default();
    let fragment = RecordedFragment {
        phase,
        origin: origin.cloned(),
        namespace_directive: namespace_directive.cloned(),
        forms,
        environment: environment.clone(),
    };
    RECORDING.with(|r| {
        if let Some(fragments) = r.borrow_mut().as_mut() {
            fragments.push(fragment);
        }
    });
}

/// Forget forms analyzed outside a fragment.
pub(super) fn clear_pending() {
    if recording() {
        PENDING.with(|p| p.borrow_mut().take());
    }
}
