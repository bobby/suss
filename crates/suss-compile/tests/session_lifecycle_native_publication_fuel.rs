//! Real publication/refresh fuel traps with an already suspended source waiter.
//! A bounded fuel sweep must observe terminal storage AND OutOfFuel; merely
//! exhausting the ownership guard before publication cannot satisfy this test.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use suss_compile::portable_session::{Session, SessionError, SessionValue};
use suss_compile::runtime_abi;

const NORMAL_FUEL: u64 = 1_000_000;

fn state(session: &mut Session, future: &SessionValue) -> u32 {
    session
        .inspect(future, |mut store, value| {
            assert!(!store.has_pending_exception());
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let storage = object.field(&mut store, 1)?;
            let storage = storage.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let outcome = storage.get(&mut store, 0)?;
            let outcome = outcome.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let status = outcome.get(&mut store, 0)?;
            let status = status
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32();
            if status == 1 || status == 2 {
                let payload = outcome.get(&mut store, 1)?;
                let number = payload.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                assert_eq!(number.field(&mut store, 0)?.unwrap_f64(), 42.0);
            }
            Ok(status)
        })
        .unwrap()
}

fn number(session: &mut Session, reader: &SessionValue) -> f64 {
    let value = session.invoke(reader, &[]).unwrap();
    session
        .inspect(&value, |mut store, value| {
            let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(number.field(&mut store, 0)?.unwrap_f64())
        })
        .unwrap()
}

#[test]
fn native_terminal_publication_fuel_trap_keeps_producer_disarmed_and_wakes_waiter_once() {
    // Resolve the real export index, rather than assuming generated function
    // numbering or relying on optional function names in the runtime artifact.
    let mut refresh_index = None;
    for payload in wasmparser::Parser::new(0).parse_all(&runtime_abi::module()) {
        if let wasmparser::Payload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                let export = export.unwrap();
                if export.name == "async-scheduler-refresh" {
                    assert_eq!(export.kind, wasmparser::ExternalKind::Func);
                    refresh_index = Some(export.index);
                }
            }
        }
    }
    let refresh_index = refresh_index.expect("real runtime refresh export");
    for reject in [false, true] {
        let mut session = Session::new().unwrap();
        drop(session.eval("(def starts 0) (def finishes 0)").unwrap());
        let spawn = session.eval("(fn [dependency] (suss.async/future* (set! starts (+ starts 1)) (try (suss.async/await* dependency) (catch :default error nil)) (set! finishes (+ finishes 1))))").unwrap();
        let read_starts = session.eval("(fn [] starts)").unwrap();
        let read_finishes = session.eval("(fn [] finishes)").unwrap();
        let payload = session.eval("42").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut published_errors = Vec::new();
        let mut refresh_errors = Vec::new();
        // Stream-operation ownership guards run before publication. Include
        // their integrated cost while still requiring real terminal storage
        // followed by an actual OutOfFuel inside runtime refresh.
        for fuel in 1..=32768 {
            session.set_operation_fuel(NORMAL_FUEL);
            let producer = calls.clone();
            let dependency = session
                .pending_future_with_cancel(move || {
                    producer.fetch_add(1, Ordering::SeqCst);
                })
                .unwrap();
            let waiter = session.invoke(&spawn, &[&dependency]).unwrap();
            assert!(session.run_async_turn().unwrap());
            assert!(!session.run_async_turn().unwrap());
            assert_eq!(number(&mut session, &read_starts), fuel as f64);
            assert_eq!(number(&mut session, &read_finishes), (fuel - 1) as f64);
            session.set_operation_fuel(fuel);
            let result = if reject {
                session.reject_future(&dependency, &payload)
            } else {
                session.resolve_future(&dependency, &payload)
            };
            session.set_operation_fuel(NORMAL_FUEL);
            let status = state(&mut session, &dependency);
            if let Err(error) = &result {
                assert!(
                    matches!(error, SessionError::Trap(error)
                    if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                    "fuel {fuel}: unexpected publication error {error}"
                );
                if status != 0 {
                    assert_eq!(status, if reject { 2 } else { 1 });
                    assert_eq!(
                        state(&mut session, &waiter),
                        0,
                        "source waiter cannot execute inside native publication"
                    );
                    published_errors.push(fuel);
                    if let SessionError::Trap(error) = error {
                        let trace = error
                            .downcast_ref::<wasmtime::WasmBacktrace>()
                            .expect("real Wasm fuel trap backtrace");
                        if trace
                            .frames()
                            .iter()
                            .any(|frame| frame.func_index() == refresh_index)
                        {
                            refresh_errors.push(fuel);
                        }
                    }
                }
            }
            // Before-publication failures need one publication retry. For actual
            // terminal+error samples, retry must be a no-op (first-terminal-wins).
            let retried = if reject {
                session.reject_future(&dependency, &payload)
            } else {
                session.resolve_future(&dependency, &payload)
            }
            .unwrap();
            assert_eq!(retried, status == 0, "fuel {fuel}: publication retry");
            // A fresh turn refreshes any interrupted wake-up. Exactly one turn
            // executes this waiter, including when refresh had partly enqueued it.
            assert!(
                session.run_async_turn().unwrap(),
                "fuel {fuel}: lost wake-up"
            );
            assert!(
                !session.run_async_turn().unwrap(),
                "fuel {fuel}: duplicate wake-up"
            );
            assert_eq!(
                number(&mut session, &read_starts),
                fuel as f64,
                "fuel {fuel}: pre-await source replay"
            );
            assert_eq!(
                number(&mut session, &read_finishes),
                fuel as f64,
                "fuel {fuel}: continuation executes once"
            );
            assert_eq!(session.stats().pending_host_requests, 0);
            assert_eq!(
                calls.load(Ordering::SeqCst),
                0,
                "fuel {fuel}: native producer reclassified"
            );
            drop(waiter);
            drop(dependency);
            if refresh_errors.len() >= 4 {
                break;
            }
        }
        assert!(
            !published_errors.is_empty(),
            "reject={reject}: sweep never observed real terminal storage plus OutOfFuel"
        );
        assert!(
            !refresh_errors.is_empty(),
            "reject={reject}: terminal+error samples {published_errors:?} never trapped inside runtime refresh"
        );
        eprintln!(
            "reject={reject}: terminal-publication OutOfFuel budgets {published_errors:?}; refresh frames {refresh_errors:?}"
        );
        session.collect().unwrap();
        session.reset().unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "later sweep/reset cannot cancel finished producers"
        );
    }
}
