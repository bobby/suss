//! Buffered and backpressured stream graphs: real post-GC accounting, separate from code.
//! This standalone integration-test binary owns its process-global log probe.
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
fn residency(stats: SessionStats) -> (usize, usize, usize, usize, usize) {
    (
        stats.resident_fragments,
        stats.base_runtime_artifact_bytes,
        stats.resident_artifact_bytes,
        stats.binding_cells,
        stats.loaded_modules,
    )
}

fn pump(session: &mut Session) {
    for _ in 0..512 {
        if !session.run_async_turn().unwrap() {
            return;
        }
    }
    panic!("stream scheduler did not quiesce");
}
struct Fixtures {
    build: SessionValue,
    pair: SessionValue,
    write: SessionValue,
    read: SessionValue,
    close: SessionValue,
}
fn lifecycle(session: &mut Session, fixtures: &Fixtures) -> (usize, usize, usize) {
    use suss_compile::portable_session::{FutureState, FutureStatus};
    let pair = session.invoke(&fixtures.pair, &[]).unwrap();
    let data = session.invoke(&fixtures.build, &[]).unwrap();
    let first = session.invoke(&fixtures.write, &[&pair, &data]).unwrap();
    drop(data);
    let copying = live_bytes(session);
    pump(session);
    assert_eq!(
        session.future_status(&first).unwrap(),
        Some(FutureStatus::Ready)
    );
    drop(first);
    let buffered = live_bytes(session);
    let data = session.invoke(&fixtures.build, &[]).unwrap();
    let blocked = session.invoke(&fixtures.write, &[&pair, &data]).unwrap();
    drop(data);
    pump(session);
    assert_eq!(
        session.future_status(&blocked).unwrap(),
        Some(FutureStatus::Pending)
    );
    assert_eq!(session.pending_stream_operation_count().unwrap(), 1);
    let backpressured = live_bytes(session);
    assert!(
        backpressured > buffered,
        "blocked producer graph must remain live"
    );
    assert!(session.cancel_future(&blocked).unwrap());
    pump(session);
    assert_eq!(
        session.future_status(&blocked).unwrap(),
        Some(FutureStatus::Cancelled)
    );
    drop(blocked);
    assert_eq!(session.pending_stream_operation_count().unwrap(), 0);
    assert_eq!(
        live_bytes(session),
        buffered,
        "cancelled producer graph must retire while accepted data remains"
    );
    let read = session.invoke(&fixtures.read, &[&pair]).unwrap();
    pump(session);
    assert!(matches!(
        session.future_state(&read).unwrap(),
        Some(FutureState::Ready(_))
    ));
    drop(read);
    drop(session.invoke(&fixtures.close, &[&pair]).unwrap());
    drop(pair);
    pump(session);
    assert_eq!(session.async_task_counts().unwrap(), (0, 0, 0));
    assert_eq!(session.pending_stream_operation_count().unwrap(), 0);
    assert_eq!(session.stats().pending_host_requests, 0);
    (copying, buffered, backpressured)
}
#[test]
fn stream_buffer_and_backpressured_payloads_return_actual_live_bytes_to_baseline() {
    install_probe();
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    for source in [
        "(ns stream-heap (:require [suss.async :as a]))",
        "(def heap-build (fn [] (loop [i 0 acc []] (if (< i 64) (recur (+ i 1) (conj acc [i (atom i)])) acc))))",
        "(def heap-pair (fn [] (a/stream-pair 64)))",
        "(def heap-write (fn [pair data] (a/write-chunk (nth pair 1) data)))",
        "(def heap-read (fn [pair] (a/read-chunk (nth pair 0) 64)))",
        "(def heap-close (fn [pair] (a/close! (nth pair 0)) (a/close! (nth pair 1)) nil))",
    ] { drop(session.eval_with_macros(source, &mut macros).unwrap()); }
    let fixtures = Fixtures {
        build: session.eval_with_macros("heap-build", &mut macros).unwrap(),
        pair: session.eval_with_macros("heap-pair", &mut macros).unwrap(),
        write: session.eval_with_macros("heap-write", &mut macros).unwrap(),
        read: session.eval_with_macros("heap-read", &mut macros).unwrap(),
        close: session.eval_with_macros("heap-close", &mut macros).unwrap(),
    };
    lifecycle(&mut session, &fixtures);
    let baseline = live_bytes(&mut session);
    let code = residency(session.stats());
    let handles = session.stats().external_value_handles;
    for round in 0..3 {
        let (copying, buffered, blocked) = lifecycle(&mut session, &fixtures);
        assert!(
            copying >= baseline + 64 * 16,
            "round {round}: copying graph"
        );
        assert!(
            buffered >= baseline + 64 * 16,
            "round {round}: buffered graph"
        );
        assert!(
            blocked > buffered,
            "round {round}: bounded waiting producer"
        );
        assert_eq!(
            live_bytes(&mut session),
            baseline,
            "round {round}: retired stream graph"
        );
        assert_eq!(
            residency(session.stats()),
            code,
            "resident code stays separate"
        );
        assert_eq!(session.stats().external_value_handles, handles);
    }
    drop(fixtures);
    assert_eq!(session.stats().external_value_handles, 0);
    assert_eq!(live_bytes(&mut session), baseline);
}
