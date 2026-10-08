use suss_compile::{portable_repl::NativeDisplay, portable_session::Session};

#[test]
fn native_future_display_observes_nominal_status_without_running_or_flattening() {
    let mut session = Session::new().unwrap();
    let future = session
        .eval("(def effects 0) (suss.async/future* (set! effects (+ effects 1)) 42)")
        .unwrap();
    let mut display = NativeDisplay::default();
    let handles = session.stats().external_value_handles;
    for _ in 0..3 {
        assert_eq!(
            display.display(&mut session, &future).unwrap(),
            "#<future pending>"
        );
        assert_eq!(session.stats().external_value_handles, handles);
    }
    let effects = session.eval("effects").unwrap();
    assert_eq!(display.display(&mut session, &effects).unwrap(), "0");
    assert!(session.run_async_turn().unwrap());
    assert_eq!(
        display.display(&mut session, &future).unwrap(),
        "#<future ready>"
    );
    assert!(!session.run_async_turn().unwrap());
    let effects = session.eval("effects").unwrap();
    assert_eq!(display.display(&mut session, &effects).unwrap(), "1");
    let cancelled = session.eval("(suss.async/future* 99)").unwrap();
    session.cancel_future(&cancelled).unwrap();
    assert_eq!(
        display.display(&mut session, &cancelled).unwrap(),
        "#<future pending>"
    );
    assert!(session.run_async_turn().unwrap());
    assert_eq!(
        display.display(&mut session, &cancelled).unwrap(),
        "#<future cancelled>"
    );
    let failed = session.pending_future().unwrap();
    session.reject_future(&failed, &effects).unwrap();
    assert_eq!(
        display.display(&mut session, &failed).unwrap(),
        "#<future failed>"
    );
}

#[test]
fn native_display_keeps_nested_streams_and_futures_opaque_without_macro_placeholders() {
    use suss_compile::{portable_macro_data::FormBridge, portable_macros::CompiledMacros};
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    drop(
        session
            .eval_with_macros(
                "(ns stream-display (:require [suss.async :as a]))",
                &mut macros,
            )
            .unwrap(),
    );
    let nested = session.eval_with_macros("(def pair (a/stream-pair 2)) (def effects 0) (def pending (suss.async/future* (set! effects (+ effects 1)) 42)) {:pair pair :future [pending]}", &mut macros).unwrap();
    let pair = session.eval("pair").unwrap();
    let reader = session.eval("(nth pair 0)").unwrap();
    let writer = session.eval("(nth pair 1)").unwrap();
    let mut display = NativeDisplay::default();
    assert_eq!(
        display.display(&mut session, &reader).unwrap(),
        "#<stream reader>"
    );
    assert_eq!(
        display.display(&mut session, &writer).unwrap(),
        "#<stream writer>"
    );
    let expected = "{:pair [#<stream reader> #<stream writer>] :future [#<future pending>]}";
    assert_eq!(display.display(&mut session, &nested).unwrap(), expected);
    let handles = session.stats().external_value_handles;
    let counts = session.async_task_counts().unwrap();
    for _ in 0..3 {
        session.collect().unwrap();
        assert_eq!(
            display.display(&mut session, &pair).unwrap(),
            "[#<stream reader> #<stream writer>]"
        );
        assert_eq!(display.display(&mut session, &nested).unwrap(), expected);
        assert_eq!(
            session.stats().external_value_handles,
            handles,
            "no terminal payload handles from display"
        );
        assert_eq!(
            session.async_task_counts().unwrap(),
            counts,
            "display does not pump pending tasks"
        );
    }
    let effects = session.eval("effects").unwrap();
    assert_eq!(display.display(&mut session, &effects).unwrap(), "0");
    // Macro syntax transport must reject these objects, rather than encoding
    // display placeholders as ordinary source strings/keywords/symbols.
    let bridge = FormBridge::new(&mut session).unwrap();
    assert!(bridge.read(&mut session, &pair, 0..0).is_err());
    assert!(bridge.read(&mut session, &nested, 0..0).is_err());
}

#[test]
fn native_display_distinguishes_nested_stream_eof_from_nil_and_ready_future_payload() {
    use suss_compile::{portable_macros::CompiledMacros, portable_session::FutureState};
    let mut session = Session::new_repl().unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    drop(
        session
            .eval_with_macros(
                "(ns eof-display (:require [suss.async :as a]))",
                &mut macros,
            )
            .unwrap(),
    );
    let read = session.eval_with_macros("(def pair (a/stream-pair 2)) (a/close! (nth pair 1)) (def read (a/read-chunk (nth pair 0) 1)) read", &mut macros).unwrap();
    let mut idle = false;
    for _ in 0..32 {
        if !session.run_async_turn().unwrap() {
            idle = true;
            break;
        }
    }
    assert!(idle, "bounded EOF completion drain");
    let Some(FutureState::Ready(eof)) = session.future_state(&read).unwrap() else {
        panic!("closed empty writer must complete the read with EOF");
    };
    let mut display = NativeDisplay::default();
    assert_eq!(
        display.display(&mut session, &eof).unwrap(),
        "#<stream eof>"
    );
    assert_eq!(
        display.display(&mut session, &read).unwrap(),
        "#<future ready>"
    );
    let nested = session
        .eval("[(suss.internal.async/result read) [read] pair nil]")
        .unwrap();
    let expected = "[#<stream eof> [#<future ready>] [#<stream reader> #<stream writer>] nil]";
    assert_eq!(display.display(&mut session, &nested).unwrap(), expected);
    let handles = session.stats().external_value_handles;
    for _ in 0..3 {
        session.collect().unwrap();
        assert_eq!(display.display(&mut session, &nested).unwrap(), expected);
        assert_eq!(session.stats().external_value_handles, handles);
        assert_eq!(session.async_task_counts().unwrap(), (0, 0, 0));
    }
}
