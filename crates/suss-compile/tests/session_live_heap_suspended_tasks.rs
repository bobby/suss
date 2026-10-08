//! Suspended-task captured graphs: real post-GC accounting, separate from code.
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

fn eval(session: &mut Session, macros: &mut CompiledMacros, source: &str) -> SessionValue {
    session
        .eval_with_macros(source, macros)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
}

fn observed(session: &mut Session, read: &SessionValue) -> f64 {
    let value = session.invoke(read, &[]).unwrap();
    session
        .inspect(&value, |mut store, value| {
            let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(number.field(&mut store, 0)?.unwrap_f64())
        })
        .unwrap()
}

struct Fixtures {
    build: SessionValue,
    pending: SessionValue,
    spawn: SessionValue,
    read: SessionValue,
    clear: SessionValue,
    nil: SessionValue,
}

/// No source evaluation or new binding cells inside a lifecycle. The only
/// references to the graph after `drop(graph)` belong to the task continuation.
fn lifecycle(session: &mut Session, fixtures: &Fixtures) -> (usize, usize, usize) {
    let dependency = session.invoke(&fixtures.pending, &[]).unwrap();
    let cleanup = session.invoke(&fixtures.pending, &[]).unwrap();
    let graph = session.invoke(&fixtures.build, &[]).unwrap();
    let task = session
        .invoke(&fixtures.spawn, &[&graph, &dependency, &cleanup])
        .unwrap();
    drop(graph);
    let queued = live_bytes(session);
    assert_eq!(observed(session, &fixtures.read), 0.0);

    assert!(session.run_async_turn().unwrap());
    assert!(!session.run_async_turn().unwrap());
    let awaiting = live_bytes(session);
    assert_eq!(observed(session, &fixtures.read), 0.0);

    assert!(session.cancel_future(&task).unwrap());
    assert!(session.run_async_turn().unwrap());
    assert!(!session.run_async_turn().unwrap());
    let cleaning = live_bytes(session);
    assert_eq!(observed(session, &fixtures.read), 0.0);
    // Finally really suspended on a second unknown producer. Cancellation must
    // not invent its result or discard the captured graph while cleanup waits.
    assert!(session.resolve_future(&cleanup, &fixtures.nil).unwrap());
    assert!(session.run_async_turn().unwrap());
    assert!(!session.run_async_turn().unwrap());
    assert_eq!(observed(session, &fixtures.read), 300.0);
    assert!(!session.cancel_future(&task).unwrap());

    // Settle the original dependency late: its retired waiter must not replay
    // either the source body or finally. Both completions are source-created,
    // so no native pending-request registry holds them alive after these drops.
    assert!(session.resolve_future(&dependency, &fixtures.nil).unwrap());
    assert!(!session.run_async_turn().unwrap());
    assert_eq!(observed(session, &fixtures.read), 300.0);
    drop(task);
    drop(dependency);
    drop(cleanup);
    drop(session.invoke(&fixtures.clear, &[]).unwrap());
    assert_eq!(session.stats().pending_host_requests, 0);
    (queued, awaiting, cleaning)
}

#[test]
fn suspended_task_graph_survives_cancel_cleanup_then_returns_actual_live_bytes_to_baseline() {
    install_probe();
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    for source in [
        "(def heap-build (fn [] (loop [i 0 acc []] (if (< i 300) (recur (+ i 1) (conj acc [i (atom i)])) acc))))",
        "(def heap-observation (atom 0))",
        "(def heap-pending (fn [] (suss.internal.async/pending)))",
        "(def heap-spawn (fn [graph dependency cleanup] (suss.async/future* (try (suss.async/await* dependency) (finally (suss.async/await* cleanup) (reset! heap-observation (+ @heap-observation (count graph))))))))",
        "(def heap-read (fn [] @heap-observation))",
        "(def heap-clear (fn [] (reset! heap-observation 0) nil))",
    ] {
        drop(eval(&mut session, &mut macros, source));
    }
    let fixtures = Fixtures {
        build: eval(&mut session, &mut macros, "heap-build"),
        pending: eval(&mut session, &mut macros, "heap-pending"),
        spawn: eval(&mut session, &mut macros, "heap-spawn"),
        read: eval(&mut session, &mut macros, "heap-read"),
        clear: eval(&mut session, &mut macros, "heap-clear"),
        nil: eval(&mut session, &mut macros, "nil"),
    };
    // Warm every path, including cancellation and late completion, before the
    // baseline. Fixed helper cells retain code only, never a task or its graph.
    lifecycle(&mut session, &fixtures);
    let baseline = live_bytes(&mut session);
    let code = residency(session.stats());
    let handles = session.stats().external_value_handles;
    let mut first = None;
    for round in 0..3 {
        let readings = lifecycle(&mut session, &fixtures);
        for (state, bytes) in [
            ("queued", readings.0),
            ("awaiting", readings.1),
            ("finally awaiting", readings.2),
        ] {
            assert!(
                bytes >= baseline + 300 * 16,
                "round {round}: {state} captured graph {bytes} vs baseline {baseline}"
            );
        }
        assert_eq!(*first.get_or_insert(readings), readings, "round {round}");
        assert_eq!(
            live_bytes(&mut session),
            baseline,
            "round {round}: retired graph"
        );
        assert_eq!(
            residency(session.stats()),
            code,
            "round {round}: resident code"
        );
        assert_eq!(session.stats().external_value_handles, handles);
    }
    // All host-owned values disappear too. Helper closures remain in their
    // original code cells, exactly as at baseline; no graph/task source cell
    // exists, and the sole observation cell has been reset to its initial 0.
    drop(fixtures);
    assert_eq!(session.stats().external_value_handles, 0);
    assert_eq!(
        live_bytes(&mut session),
        baseline,
        "no host-owned roots remain"
    );
    assert_eq!(residency(session.stats()), code, "resident code retained");
}

/// Reuse compiled functions for 10,000 calls; no source replay or accumulating
/// fragments can obscure task/waiter retirement. Vary terminal state, payload
/// shape and late completion while every invocation suspends in finally.
#[test]
fn ten_thousand_varied_tasks_retire_waiters_and_return_live_heap_to_baseline() {
    use suss_compile::portable_session::{FutureState, FutureStatus};
    install_probe();
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let pending = eval(
        &mut session,
        &mut macros,
        "(fn [] (suss.internal.async/pending))",
    );
    let spawn = eval(
        &mut session,
        &mut macros,
        "(fn [dependency cleanup] (suss.async/future* (try (suss.async/await* dependency) (finally (suss.async/await* cleanup)))))",
    );
    let bridge = suss_compile::portable_macro_data::FormBridge::new(&mut session).unwrap();
    const SOURCES: [&str; 8] = [
        "nil",
        "false",
        "true",
        "0",
        "-17",
        "42",
        "[nil false 42]",
        "{:value 17}",
    ];
    fn without_locations(mut form: suss_reader::forms::Form) -> suss_reader::forms::Form {
        use suss_reader::forms::Kind;
        form.span = 0..0;
        match &mut form.kind {
            Kind::Vector(items) | Kind::Map(items) => {
                for item in items {
                    *item = without_locations(item.clone());
                }
            }
            _ => {}
        }
        form
    }
    let expected_payloads = SOURCES
        .map(|source| without_locations(suss_reader::forms::read_forms(source).unwrap().remove(0)));
    let payloads = SOURCES.map(|source| eval(&mut session, &mut macros, source));
    let exercise = |session: &mut Session, call: usize| {
        let dependency = session.invoke(&pending, &[]).unwrap();
        let cleanup = session.invoke(&pending, &[]).unwrap();
        let task = session.invoke(&spawn, &[&dependency, &cleanup]).unwrap();
        assert!(session.run_async_turn().unwrap());
        assert!(!session.run_async_turn().unwrap());
        let payload = &payloads[call % payloads.len()];
        let expected = match call % 3 {
            0 => {
                assert!(session.resolve_future(&dependency, payload).unwrap());
                FutureStatus::Ready
            }
            1 => {
                assert!(session.reject_future(&dependency, payload).unwrap());
                FutureStatus::Failed
            }
            _ => {
                assert!(session.cancel_future(&task).unwrap());
                FutureStatus::Cancelled
            }
        };
        assert!(session.run_async_turn().unwrap());
        assert!(!session.run_async_turn().unwrap());
        assert_eq!(
            session.future_status(&task).unwrap(),
            Some(FutureStatus::Pending)
        );
        assert!(session.resolve_future(&cleanup, &payloads[0]).unwrap());
        assert!(session.run_async_turn().unwrap());
        assert!(!session.run_async_turn().unwrap());
        assert_eq!(session.future_status(&task).unwrap(), Some(expected));
        match session.future_state(&task).unwrap().unwrap() {
            FutureState::Ready(result) | FutureState::Failed(result) => {
                let actual = bridge.read(session, &result, 0..0).unwrap();
                let expected = expected_payloads[call % payloads.len()].clone();
                assert_eq!(
                    without_locations(actual),
                    expected,
                    "call {call}: independently decoded payload"
                );
            }
            FutureState::Cancelled => {
                assert!(session.resolve_future(&dependency, payload).unwrap());
            }
            FutureState::Pending => panic!("call {call} remained pending"),
        }
        assert!(!session.cancel_future(&task).unwrap());
        assert!(!session.run_async_turn().unwrap());
        drop(task);
        drop(cleanup);
        drop(dependency);
        assert_eq!(session.async_task_counts().unwrap(), (0, 0, 0));
        assert_eq!(session.pending_stream_operation_count().unwrap(), 0);
        assert_eq!(session.stats().pending_host_requests, 0);
    };
    // Warm each outcome and shape before measuring exact post-GC bytes.
    for call in 0..24 {
        exercise(&mut session, call);
    }
    let baseline = live_bytes(&mut session);
    let code = residency(session.stats());
    let handles = session.stats().external_value_handles;
    for call in 0..10_000 {
        exercise(&mut session, call);
        if (call + 1) % 1000 == 0 {
            assert_eq!(
                live_bytes(&mut session),
                baseline,
                "after {} calls",
                call + 1
            );
            assert_eq!(residency(session.stats()), code);
            assert_eq!(session.stats().external_value_handles, handles);
        }
    }
}

#[test]
fn dead_capture_graph_is_released_at_await_while_owner_remains_pending() {
    use suss_compile::portable_session::FutureStatus;
    install_probe();
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    let build = eval(
        &mut session,
        &mut macros,
        "(fn [] (loop [i 0 acc []] (if (< i 300) (recur (+ i 1) (conj acc [i (atom i)])) acc)))",
    );
    let pending = eval(
        &mut session,
        &mut macros,
        "(fn [] (suss.internal.async/pending))",
    );
    let spawn = eval(
        &mut session,
        &mut macros,
        "(fn [graph dependency] (suss.async/future* (count graph) (suss.async/await* dependency)))",
    );
    let nil = eval(&mut session, &mut macros, "nil");
    let lifecycle = |session: &mut Session, large: bool| {
        let dependency = session.invoke(&pending, &[]).unwrap();
        let graph = if large {
            session.invoke(&build, &[]).unwrap()
        } else {
            nil.clone()
        };
        let task = session.invoke(&spawn, &[&graph, &dependency]).unwrap();
        drop(graph);
        let queued = live_bytes(session);
        assert!(session.run_async_turn().unwrap());
        assert!(!session.run_async_turn().unwrap());
        assert_eq!(
            session.future_status(&task).unwrap(),
            Some(FutureStatus::Pending)
        );
        let awaiting = live_bytes(session);
        assert!(session.resolve_future(&dependency, &nil).unwrap());
        assert!(session.run_async_turn().unwrap());
        assert!(!session.run_async_turn().unwrap());
        assert_eq!(
            session.future_status(&task).unwrap(),
            Some(FutureStatus::Ready)
        );
        drop(task);
        drop(dependency);
        assert_eq!(session.async_task_counts().unwrap(), (0, 0, 0));
        (queued, awaiting, live_bytes(session))
    };
    lifecycle(&mut session, false);
    lifecycle(&mut session, true);
    let code = residency(session.stats());
    let small = lifecycle(&mut session, false);
    let large = lifecycle(&mut session, true);
    assert!(
        large.0 >= small.0 + 300 * 16,
        "queued capture really retains graph"
    );
    assert_eq!(
        large.1, small.1,
        "await snapshot releases dead capture graph before settlement"
    );
    assert_eq!(large.2, small.2, "terminal state retires both owners");
    assert_eq!(residency(session.stats()), code);
}
