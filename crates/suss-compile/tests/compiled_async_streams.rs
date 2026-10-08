//! Actual compiled public source stream operations and scheduler execution.
//! Private GC stream profile only; these tests make no canonical WIT stream claim.
use suss_compile::{
    portable_macros::CompiledMacros,
    portable_repl::NativeDisplay,
    portable_session::{FutureState, FutureStatus, Session, SessionError, SessionValue},
};
struct Harness {
    session: Session,
    macros: CompiledMacros,
    display: NativeDisplay,
}
impl Harness {
    fn new() -> Self {
        let mut session = Session::new_repl().unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        session.eval_with_macros("(ns stream-test (:require [suss.async :as a]) (:require-macros [suss.async :refer [future await]]))", &mut macros).unwrap();
        Self {
            session,
            macros,
            display: NativeDisplay::default(),
        }
    }
    fn eval(&mut self, source: &str) -> SessionValue {
        self.eval_at("compiled source evaluation", source)
    }
    fn eval_at(&mut self, stage: &str, source: &str) -> SessionValue {
        self.session
            .eval_with_macros(source, &mut self.macros)
            .unwrap_or_else(|error| panic!("{stage} failed; source={source}\n{error:?}"))
    }
    fn text(&mut self, source: &str) -> String {
        let value = self.eval(source);
        self.display.display(&mut self.session, &value).unwrap()
    }
    fn pump_to_idle(&mut self) {
        for _ in 0..512 {
            let ran = self.session.run_async_turn().unwrap();
            // Pending stream endpoints/chunks and suspended operation tokens
            // must remain live independently of the current execution stack.
            self.session.collect().unwrap();
            if !ran {
                return;
            }
        }
        panic!("stream scheduler did not become idle within bounded turns");
    }
    fn pending(&mut self, future: &SessionValue) {
        assert_eq!(
            self.session.future_status(future).unwrap(),
            Some(FutureStatus::Pending)
        );
    }
    fn ready(&mut self, future: &SessionValue, expected: &str) {
        let Some(FutureState::Ready(value)) = self.session.future_state(future).unwrap() else {
            panic!("expected Ready stream operation");
        };
        assert_eq!(
            self.display.display(&mut self.session, &value).unwrap(),
            expected
        );
    }
    fn failed(&mut self, future: &SessionValue, expected: &str) {
        let Some(FutureState::Failed(value)) = self.session.future_state(future).unwrap() else {
            panic!("expected Failed stream operation");
        };
        assert_eq!(
            self.display.display(&mut self.session, &value).unwrap(),
            expected
        );
    }
    fn pair(&mut self, capacity: usize) {
        self.eval(&format!("(def pair (a/stream-pair {capacity})) (def reader (nth pair 0)) (def writer (nth pair 1))"));
    }
}

#[test]
fn bounded_backpressure_preserves_nil_data_and_writer_close_drains_before_eof() {
    let mut h = Harness::new();
    h.pair(2);
    let initial = h.eval("(a/write-chunk writer [nil 7])");
    h.pump_to_idle();
    h.ready(&initial, "nil");
    let blocked = h.eval("(a/write-chunk writer [8 9])");
    h.pump_to_idle();
    h.pending(&blocked);
    let first = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.ready(&first, "[nil]");
    // One freed slot cannot admit a two-element atomic chunk.
    h.pending(&blocked);
    let second = h.eval("(a/read-chunk reader 2)");
    h.pump_to_idle();
    h.ready(&second, "[7]");
    h.ready(&blocked, "nil");
    h.eval("(a/close! writer)");
    let drained = h.eval("(a/read-chunk reader 2)");
    h.pump_to_idle();
    h.ready(&drained, "[8 9]");
    let eof = h.eval("(future (a/stream-eof? (await (a/read-chunk reader 1))))");
    h.pump_to_idle();
    h.ready(&eof, "true");
    let repeated_eof = h.eval("(future (a/stream-eof? (await (a/read-chunk reader 2))))");
    h.pump_to_idle();
    h.ready(&repeated_eof, "true");
    assert_eq!(
        h.text("[(a/stream-eof? nil) (a/stream-eof? false) (a/stream-eof? [nil])]"),
        "[false false false]"
    );
}

#[test]
fn writer_close_rejects_unaccepted_write_but_preserves_buffered_data() {
    let mut h = Harness::new();
    h.pair(1);
    let accepted = h.eval("(a/write-chunk writer [11])");
    h.pump_to_idle();
    h.ready(&accepted, "nil");
    let waiting = h.eval("(a/write-chunk writer [22])");
    h.pump_to_idle();
    h.pending(&waiting);
    h.eval("(a/close! writer)");
    h.pump_to_idle();
    h.failed(&waiting, "nil");
    let drained = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.ready(&drained, "[11]");
    let eof = h.eval("(future (a/stream-eof? (await (a/read-chunk reader 1))))");
    h.pump_to_idle();
    h.ready(&eof, "true");
}

#[test]
fn stream_failure_preserves_false_reason_and_discards_buffered_data() {
    let mut h = Harness::new();
    h.pair(1);
    let waiting_read = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.pending(&waiting_read);
    h.eval("(a/fail! writer false)");
    h.pump_to_idle();
    h.failed(&waiting_read, "false");
    let later_write = h.eval("(a/write-chunk writer [7])");
    h.pump_to_idle();
    h.failed(&later_write, "false");
    let caught = h.eval(
        "(future (try (await (a/read-chunk reader 1)) (catch :default reason (= reason false))))",
    );
    h.pump_to_idle();
    h.ready(&caught, "true");
    // Independently cover a write blocked behind data that failure discards.
    h.pair(1);
    let accepted = h.eval("(a/write-chunk writer [11])");
    h.pump_to_idle();
    h.ready(&accepted, "nil");
    let blocked = h.eval("(a/write-chunk writer [22])");
    h.pump_to_idle();
    h.pending(&blocked);
    h.eval("(a/fail! writer 42)");
    h.pump_to_idle();
    h.failed(&blocked, "42");
    let after_failure = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.failed(&after_failure, "42");
}

#[test]
fn cancelled_read_and_blocked_write_retire_operations_without_closing_endpoints() {
    let mut h = Harness::new();
    h.pair(1);
    let abandoned_read = h.eval("(def abandoned-read (a/read-chunk reader 1))");
    h.pump_to_idle();
    h.pending(&abandoned_read);
    assert_eq!(h.text("(a/cancel! abandoned-read)"), "true");
    h.pump_to_idle();
    assert_eq!(
        h.session.future_status(&abandoned_read).unwrap(),
        Some(FutureStatus::Cancelled)
    );
    // The public finally retires the old read token; the same reader is reusable.
    let replacement_read = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.pending(&replacement_read);
    let delivered = h.eval("(a/write-chunk writer [nil])");
    h.pump_to_idle();
    h.ready(&replacement_read, "[nil]");
    h.ready(&delivered, "nil");
    let fill = h.eval("(a/write-chunk writer [31])");
    h.pump_to_idle();
    h.ready(&fill, "nil");
    let abandoned_write = h.eval("(def abandoned-write (a/write-chunk writer [99]))");
    h.pump_to_idle();
    h.pending(&abandoned_write);
    assert_eq!(h.text("(a/cancel! abandoned-write)"), "true");
    h.pump_to_idle();
    assert_eq!(
        h.session.future_status(&abandoned_write).unwrap(),
        Some(FutureStatus::Cancelled)
    );
    let drain = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.ready(&drain, "[31]");
    let reuse = h.eval("(a/write-chunk writer [32])");
    h.pump_to_idle();
    h.ready(&reuse, "nil");
    let final_read = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.ready(&final_read, "[32]");
    // A cancelled write must never leak 99 later, and cancellation must not
    // silently close either endpoint. An empty read therefore stays Pending.
    let empty = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.pending(&empty);
    h.eval("(a/close! writer)");
    h.pump_to_idle();
    let eof = h.eval("(future (a/stream-eof? (await (a/read-chunk reader 1))))");
    h.pump_to_idle();
    h.ready(&eof, "true");
}

#[test]
fn stream_public_bounds_validate_before_endpoint_work_and_endpoints_remain_usable() {
    let mut h = Harness::new();
    for capacity in ["0", "-1", "4097", "1.5", "##NaN", "nil"] {
        assert_eq!(
            h.text(&format!(
                "(try (a/stream-pair {capacity}) false (catch :default reason true))"
            )),
            "true",
            "capacity {capacity}"
        );
    }
    h.pair(1);
    for limit in ["0", "-1", "2", "1.5", "##NaN"] {
        assert_eq!(
            h.text(&format!(
                "(try (a/read-chunk reader {limit}) false (catch :default reason true))"
            )),
            "true",
            "limit {limit}"
        );
    }
    for chunk in ["[]", "[1 2]", "nil", "(list 1)"] {
        assert_eq!(
            h.text(&format!(
                "(try (a/write-chunk writer {chunk}) false (catch :default reason true))"
            )),
            "true",
            "chunk {chunk}"
        );
    }
    let write = h.eval("(a/write-chunk writer [17])");
    h.pump_to_idle();
    h.ready(&write, "nil");
    let read = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.ready(&read, "[17]");
    h.pair(4096);
    assert_eq!(
        h.text("(try (a/read-chunk reader 257) false (catch :default reason true))"),
        "true"
    );
    let oversized = format!("[{}]", (0..257).map(|_| "1").collect::<Vec<_>>().join(" "));
    let rejected = h.eval_at(
        "reject 257-element chunk at capacity 4096",
        &format!("(try (a/write-chunk writer {oversized}) false (catch :default reason true))"),
    );
    assert_eq!(
        h.display.display(&mut h.session, &rejected).unwrap(),
        "true"
    );
    let boundary = format!("[{}]", (0..256).map(|_| "1").collect::<Vec<_>>().join(" "));
    let write = h.eval_at(
        "accept 256-element chunk at capacity 4096",
        &format!("(a/write-chunk writer {boundary})"),
    );
    h.pump_to_idle();
    h.ready(&write, "nil");
    let read = h.eval_at(
        "create read demand 256 at capacity 4096",
        "(a/read-chunk reader 256)",
    );
    h.pump_to_idle();
    // Observe actual public immutable vector conversion at the maximum demand.
    let Some(FutureState::Ready(chunk)) = h.session.future_state(&read).unwrap() else {
        panic!("256-element read did not complete");
    };
    assert_eq!(h.display.display(&mut h.session, &chunk).unwrap(), boundary);
}

#[test]
fn duplicate_read_and_write_fail_without_replacing_original_endpoint_operation() {
    let mut h = Harness::new();
    h.pair(1);
    let original_read = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.pending(&original_read);
    // Busy checks run inside the task body, so inspect actual Failed futures
    // rather than accepting a synchronous exception from source validation.
    let duplicate_read = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.failed(&duplicate_read, "nil");
    h.pending(&original_read);
    let first_write = h.eval("(a/write-chunk writer [17])");
    h.pump_to_idle();
    h.ready(&first_write, "nil");
    h.ready(&original_read, "[17]");
    let fill = h.eval("(a/write-chunk writer [31])");
    h.pump_to_idle();
    h.ready(&fill, "nil");
    let original_write = h.eval("(a/write-chunk writer [32])");
    h.pump_to_idle();
    h.pending(&original_write);
    let duplicate_write = h.eval("(a/write-chunk writer [99])");
    h.pump_to_idle();
    h.failed(&duplicate_write, "nil");
    h.pending(&original_write);
    let drain = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.ready(&drain, "[31]");
    h.ready(&original_write, "nil");
    let retained = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.ready(&retained, "[32]");
    let empty = h.eval("(def empty-read (a/read-chunk reader 1))");
    h.pump_to_idle();
    h.pending(&empty);
    h.eval("(a/close! writer)");
    let eof = h.eval("(future (a/stream-eof? (await empty-read)))");
    h.pump_to_idle();
    h.ready(&eof, "true");
}

#[test]
fn reader_close_fails_pending_operations_and_discards_previously_accepted_buffer() {
    let mut h = Harness::new();
    h.pair(1);
    let waiting_read = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.pending(&waiting_read);
    h.eval("(a/close! reader)");
    h.pump_to_idle();
    h.failed(&waiting_read, "nil");
    let later_write = h.eval("(a/write-chunk writer [17])");
    h.pump_to_idle();
    h.failed(&later_write, "nil");
    let later_read = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.failed(&later_read, "nil");
    h.pair(1);
    let fill = h.eval("(a/write-chunk writer [41])");
    h.pump_to_idle();
    h.ready(&fill, "nil");
    let waiting_write = h.eval("(a/write-chunk writer [42])");
    h.pump_to_idle();
    h.pending(&waiting_write);
    h.eval("(a/close! reader)");
    h.pump_to_idle();
    h.failed(&waiting_write, "nil");
    // Reader-close discards, unlike writer-close drain. Neither buffered41 nor
    // the rejected42 may be observed as data or a successful EOF afterward.
    let discarded = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.failed(&discarded, "nil");
    let caught = h.eval("(future (try (await (a/read-chunk reader 1)) true (catch :default reason (a/stream-eof? reason))))");
    h.pump_to_idle();
    h.ready(&caught, "false");
}

#[test]
fn public_nominal_guards_preserve_writer_only_failure_and_nil_failure_is_not_eof() {
    let mut h = Harness::new();
    h.pair(1);
    let original = h.eval("(def guarded-read (a/read-chunk reader 1))");
    h.pump_to_idle();
    h.pending(&original);
    // The staged native profile's fail! accepts a WRITER. Rejecting a reader
    // must not mutate the paired state or replace the original pending read.
    for source in [
        "(a/fail! reader 99)",
        "(a/fail! nil 99)",
        "(a/close! [])",
        "(a/read-chunk writer 1)",
        "(a/read-chunk nil 1)",
        "(a/write-chunk reader [1])",
        "(a/write-chunk nil [1])",
    ] {
        assert_eq!(
            h.text(&format!(
                "(try {source} false (catch :default reason true))"
            )),
            "true",
            "{source}"
        );
        h.pump_to_idle();
        h.pending(&original);
    }
    h.eval("(a/fail! writer nil)");
    h.pump_to_idle();
    h.failed(&original, "nil");
    let caught = h.eval("(future (try (await guarded-read) [false false] (catch :default reason [(nil? reason) (a/stream-eof? reason)])))");
    h.pump_to_idle();
    h.ready(&caught, "[true false]");
    assert_eq!(h.text("[(a/stream-eof? nil) (a/stream-eof? false) (a/stream-eof? []) (a/stream-eof? reader) (a/stream-eof? writer)]"), "[false false false false false]");
}

#[test]
fn cancelled_parent_finally_awaits_pending_stream_read_until_independent_writer_supplies_data() {
    let mut h = Harness::new();
    h.pair(1);
    h.eval("(def body-dependency (a/completion)) (def cleanup-steps 0) (def cleanup-value 0)");
    let parent = h.eval("(def parent (future (try (await body-dependency) (finally (set! cleanup-steps (+ cleanup-steps 1)) (set! cleanup-value (nth (await (a/read-chunk reader 1)) 0)) (set! cleanup-steps (+ cleanup-steps 1))))))");
    h.pump_to_idle();
    h.pending(&parent);
    assert_eq!(h.text("(a/cancel! parent)"), "true");
    h.pump_to_idle();
    // Cancellation is already dispatched, but its retained phase-2 latch must
    // not immediately cancel a new operation awaited by finally. This empty
    // stream read genuinely suspends; no data or EOF has yet been published.
    h.pending(&parent);
    assert_eq!(h.text("[cleanup-steps cleanup-value]"), "[1 0]");
    h.session.collect().unwrap();
    h.pump_to_idle();
    h.pending(&parent);
    assert_eq!(h.text("[cleanup-steps cleanup-value]"), "[1 0]");
    // Created by the caller, this producer is independent of the cancelled
    // parent's cleanup task. Actual source scheduling must transport42 once.
    let independent = h.eval("(a/write-chunk writer [42])");
    h.pump_to_idle();
    h.ready(&independent, "nil");
    assert_eq!(
        h.session.future_status(&parent).unwrap(),
        Some(FutureStatus::Cancelled)
    );
    assert_eq!(h.text("[cleanup-steps cleanup-value]"), "[2 42]");
    h.pump_to_idle();
    assert_eq!(h.text("[cleanup-steps cleanup-value]"), "[2 42]");
    // Finally retired its read token without closing or leaking either endpoint.
    let write = h.eval("(a/write-chunk writer [7])");
    h.pump_to_idle();
    h.ready(&write, "nil");
    let read = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.ready(&read, "[7]");
    h.eval("(a/close! writer)");
}

#[test]
fn stream_maximum_chunk_calls_use_prepared_vector_under_default_operation_budget() {
    let mut h = Harness::new();
    h.pair(4096);
    let boundary = format!("[{}]", (0..256).map(|_| "1").collect::<Vec<_>>().join(" "));
    // Isolate source literal construction/macro preparation from the public
    // copy/validation operation. Every call retains Session's default 10M fuel;
    // this is not a larger shared budget or a replenishment inside either call.
    h.eval_at(
        "prepare and root 256-element immutable vector",
        &format!("(def boundary {boundary})"),
    );
    assert_eq!(h.text("(count boundary)"), "256");
    h.session.collect().unwrap();
    let write = h.eval_at(
        "write prepared 256-element vector into capacity4096",
        "(a/write-chunk writer boundary)",
    );
    h.pump_to_idle();
    h.ready(&write, "nil");
    let read = h.eval_at(
        "read 256 elements after prepared-vector write",
        "(a/read-chunk reader 256)",
    );
    h.pump_to_idle();
    let Some(FutureState::Ready(chunk)) = h.session.future_state(&read).unwrap() else {
        panic!("prepared-vector read did not complete");
    };
    assert_eq!(h.display.display(&mut h.session, &chunk).unwrap(), boundary);
    h.eval("(a/close! writer)");
}

#[test]
fn session_cannot_settle_or_directly_cancel_private_stream_operation_future() {
    let mut h = Harness::new();
    h.pair(1);
    h.eval("(def operation-future nil)");
    // Capture the actual private operation future from a running compiled task,
    // rather than manufacturing storage with a host ABI call. Its owner finally
    // remains responsible for retirement, exactly as the public wrapper does.
    let owner = h.eval("(def owner (future (let [operation (suss.internal.async/stream-begin-read reader 1)] (set! operation-future (suss.internal.async/stream-operation-future operation)) (try (await operation-future) (finally (suss.internal.async/stream-operation-retire operation))))))");
    h.pump_to_idle();
    h.pending(&owner);
    let operation = h.eval("operation-future");
    h.pending(&operation);
    let forged = h.eval("42");
    for result in [
        h.session.resolve_future(&operation, &forged),
        h.session.reject_future(&operation, &forged),
    ] {
        let Err(SessionError::Language(reason)) = result else {
            panic!("private stream future must reject external settlement/cancellation");
        };
        assert_eq!(h.display.display(&mut h.session, &reason).unwrap(), "nil");
    }
    // Native cancel_future is deliberately task-only: its wrong-owner error
    // is a language error, not the nil settlement-guard payload above.
    assert!(matches!(h.session.cancel_future(&operation), Err(SessionError::Language(_))));
    h.pump_to_idle();
    h.pending(&operation);
    h.pending(&owner);
    // No endpoint data or terminal result was forged. Only cancelling the real
    // owner may unwind finally and retire the read token for endpoint reuse.
    assert!(h.session.cancel_future(&owner).unwrap());
    h.pump_to_idle();
    assert_eq!(
        h.session.future_status(&owner).unwrap(),
        Some(FutureStatus::Cancelled)
    );
    let replacement_read = h.eval("(a/read-chunk reader 1)");
    h.pump_to_idle();
    h.pending(&replacement_read);
    let write = h.eval("(a/write-chunk writer [17])");
    h.pump_to_idle();
    h.ready(&write, "nil");
    h.ready(&replacement_read, "[17]");
    h.session.collect().unwrap();
    h.session.reset().unwrap();
    let fresh = h.session.eval("nil").unwrap();
    assert!(matches!(
        h.session.resolve_future(&operation, &fresh),
        Err(SessionError::ForeignValue)
    ));
    assert!(matches!(
        h.session.reject_future(&operation, &fresh),
        Err(SessionError::ForeignValue)
    ));
    assert!(matches!(
        h.session.cancel_future(&owner),
        Err(SessionError::ForeignValue)
    ));
    assert!(
        !h.session.run_async_turn().unwrap(),
        "old endpoint work must not cross reset"
    );
}

#[test]
fn cancelling_cooperative_chunk_copy_leaves_endpoint_unclaimed_and_reusable() {
    let mut h = Harness::new();
    h.pair(256);
    let chunk = format!("[{}]", (0..256).map(|_| "99").collect::<Vec<_>>().join(" "));
    h.eval(&format!("(def copying-input {chunk})"));
    let copying = h.eval("(a/write-chunk writer copying-input)");
    assert!(h.session.run_async_turn().unwrap());
    h.pending(&copying);
    assert_eq!(h.session.pending_stream_operation_count().unwrap(), 0,
        "bounded copy must yield before registering endpoint work");
    assert!(h.session.cancel_future(&copying).unwrap());
    h.pump_to_idle();
    assert_eq!(h.session.future_status(&copying).unwrap(), Some(FutureStatus::Cancelled));
    assert_eq!(h.session.pending_stream_operation_count().unwrap(), 0);
    let write = h.eval("(a/write-chunk writer [17])");
    h.pump_to_idle();
    h.ready(&write, "nil");
    let read = h.eval("(a/read-chunk reader 256)");
    h.pump_to_idle();
    h.ready(&read, "[17]");
    h.eval("(a/close! writer)");
    let eof = h.eval("(future (a/stream-eof? (await (a/read-chunk reader 256))))");
    h.pump_to_idle();
    h.ready(&eof, "true");
}
