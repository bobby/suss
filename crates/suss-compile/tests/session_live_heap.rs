//! Live GC heap accounting, separate from resident code (design section 7, #15).
//!
//! Pinned Wasmtime 49.0.1 exposes no public live-heap counter: `SessionStats`
//! reports capacity, which never shrinks. After every completed collection,
//! `runtime/vm/gc.rs` logs the exact `GcHeap::allocated_bytes()` result at
//! trace level. This test-only probe reads that one record for one synchronous
//! collection on the calling thread; missing, duplicate or malformed records fail.
use std::sync::Mutex;
use std::thread::ThreadId;

use suss_compile::portable_macros::CompiledMacros;
use suss_compile::portable_session::{Session, SessionStats, SessionValue};

const GC_MODULE: &str = "wasmtime::runtime::vm::gc";
const PREFIX: &str = "After collection, GC heap's allocated bytes = ";

static RECORDS: Mutex<Vec<(ThreadId, String)>> = Mutex::new(Vec::new());

struct PostCollectionProbe;
impl log::Log for PostCollectionProbe {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.target() == GC_MODULE
    }
    fn log(&self, record: &log::Record) {
        if record.module_path() == Some(GC_MODULE) && self.enabled(record.metadata()) {
            let message = record.args().to_string();
            if message.starts_with(PREFIX) {
                RECORDS
                    .lock()
                    .unwrap()
                    .push((std::thread::current().id(), message));
            }
        }
    }
    fn flush(&self) {}
}

fn install_probe() {
    static PROBE: PostCollectionProbe = PostCollectionProbe;
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        log::set_logger(&PROBE).expect("no other logger in this test binary");
        log::set_max_level(log::LevelFilter::Trace);
    });
}

/// Live bytes after exactly one completed collection of this session's Store.
fn live_bytes(session: &mut Session) -> usize {
    let thread = std::thread::current().id();
    RECORDS
        .lock()
        .unwrap()
        .retain(|(owner, _)| *owner != thread);
    session.collect().expect("collect");
    let mut records = RECORDS.lock().unwrap();
    let mine = records
        .iter()
        .filter(|(owner, _)| *owner == thread)
        .map(|(_, message)| message.clone())
        .collect::<Vec<_>>();
    records.retain(|(owner, _)| *owner != thread);
    assert_eq!(
        mine.len(),
        1,
        "expected one post-collection record: {mine:?}"
    );
    let hex = mine[0]
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(" bytes"))
        .and_then(|rest| rest.strip_prefix("0x"))
        .unwrap_or_else(|| panic!("malformed record {:?}", mine[0]));
    usize::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("malformed size {:?}", mine[0]))
}

/// Resident code and host-held roots; none of these is a live-object counter.
fn residency(stats: SessionStats) -> (usize, usize, usize, usize, usize, usize) {
    (
        stats.resident_fragments,
        stats.base_runtime_artifact_bytes,
        stats.resident_artifact_bytes,
        stats.binding_cells,
        stats.loaded_modules,
        stats.external_value_handles,
    )
}

fn eval(session: &mut Session, macros: &mut CompiledMacros, source: &str) -> SessionValue {
    session
        .eval_with_macros(source, macros)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
}

/// Released values, values retained only through a global cell and a
/// self-referential atom: live bytes return to the baseline exactly, while
/// resident code, cells and handles stay fixed and are reported separately.
fn released_values_return_live_bytes_to_baseline(mut session: Session) {
    install_probe();
    let mut macros = CompiledMacros::new().unwrap();
    for source in [
        "(def build (fn [n] (loop [i 0 acc []] (if (< i n) (recur (+ i 1) (conj acc [i (atom i)])) acc))))",
        "(def keep (atom nil))",
        "(def retain! (fn [value] (reset! keep value) nil))",
        "(def cycle (fn [n] (let [a (atom nil)] (reset! a [a (build n)]) a)))",
    ] {
        eval(&mut session, &mut macros, source);
    }
    let build = eval(&mut session, &mut macros, "build");
    let retain = eval(&mut session, &mut macros, "retain!");
    let cycle = eval(&mut session, &mut macros, "cycle");
    let count = eval(&mut session, &mut macros, "300");
    let nil = eval(&mut session, &mut macros, "nil");
    // One complete warm-up: lazily created runtime state belongs to the baseline.
    drop(session.invoke(&build, &[&count]).unwrap());
    drop(session.invoke(&cycle, &[&count]).unwrap());
    drop(session.invoke(&retain, &[&nil]).unwrap());
    let baseline = live_bytes(&mut session);
    let code = residency(session.stats());
    // 300 vectors and atoms are far larger than this bound.
    let graph = 300 * 16;
    // Identical rounds allocate identical graphs: live bytes are deterministic.
    let mut first: Option<(usize, usize, usize)> = None;
    for round in 0..3 {
        let value = session.invoke(&build, &[&count]).unwrap();
        let held = live_bytes(&mut session);
        assert!(
            held >= baseline + graph,
            "round {round}: handle-held graph {held} vs {baseline}"
        );
        drop(value);
        assert_eq!(
            live_bytes(&mut session),
            baseline,
            "round {round}: released handle"
        );

        let value = session.invoke(&cycle, &[&count]).unwrap();
        let cyclic = live_bytes(&mut session);
        // The same graph plus the self-referential atom and its pair vector.
        assert!(
            cyclic > held,
            "round {round}: cycle {cyclic} vs graph {held}"
        );
        drop(value);
        assert_eq!(
            live_bytes(&mut session),
            baseline,
            "round {round}: released cycle"
        );

        let value = session.invoke(&build, &[&count]).unwrap();
        drop(session.invoke(&retain, &[&value]).unwrap());
        drop(value);
        let retained = live_bytes(&mut session);
        assert_eq!(
            retained, held,
            "round {round}: the cell retains the same graph"
        );
        drop(session.invoke(&retain, &[&nil]).unwrap());
        assert_eq!(
            live_bytes(&mut session),
            baseline,
            "round {round}: cell cleared"
        );

        let readings = (held, cyclic, retained);
        assert_eq!(*first.get_or_insert(readings), readings, "round {round}");
        assert_eq!(
            residency(session.stats()),
            code,
            "round {round}: resident code"
        );
    }
}

#[test]
fn runtime_store_live_bytes_return_to_baseline_separately_from_code() {
    released_values_return_live_bytes_to_baseline(Session::new_repl().unwrap());
}

#[test]
fn macro_store_live_bytes_return_to_baseline_separately_from_code() {
    released_values_return_live_bytes_to_baseline(Session::new_macro().unwrap());
}

/// After repeated resets that each follow retained state, the replacement
/// Store's live bytes and resident code equal a fresh session's: nothing
/// accumulates across generations. The old Store is dropped by value
/// (`*self = replacement`); this test does not observe that drop itself.
#[test]
fn repeated_reset_returns_live_bytes_and_code_to_a_fresh_session() {
    install_probe();
    let mut fresh = Session::new_repl().unwrap();
    let fresh_live = live_bytes(&mut fresh);
    let fresh_code = residency(fresh.stats());
    let mut session = Session::new_repl().unwrap();
    for round in 0..3 {
        let mut macros = CompiledMacros::new().unwrap();
        eval(
            &mut session,
            &mut macros,
            "(def keep (atom (loop [i 0 acc []] (if (< i 300) (recur (+ i 1) (conj acc (atom i))) acc))))",
        );
        let held = eval(&mut session, &mut macros, "keep");
        assert!(
            live_bytes(&mut session) > fresh_live,
            "round {round}: retained state"
        );
        session.reset().unwrap();
        drop(held);
        assert_eq!(
            live_bytes(&mut session),
            fresh_live,
            "round {round}: reset live bytes"
        );
        assert_eq!(
            residency(session.stats()),
            fresh_code,
            "round {round}: reset code"
        );
    }
}
