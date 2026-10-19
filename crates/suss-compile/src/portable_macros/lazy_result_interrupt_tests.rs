//! Test-only native rendezvous inside an actually returned lazy result's thunk.
//! No timers select the interrupted stage and no production hook is installed.
use super::*;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use wasmtime::{ArrayRef, AsContextMut, Func, StructRef, StructRefPre, Val};

const COMPILER_FUEL: u64 = 2_000_000;
const RENDEZVOUS_TIMEOUT: Duration = Duration::from_secs(10);

enum Event {
    Entered,
    Cancel,
}

// Even an assertion panic or early setup/request error releases the waiting
// thread. The timeout bounds harness failure; it is never stage evidence.
struct InterruptThread {
    events: Sender<Event>,
    thread: Option<JoinHandle<Result<(), String>>>,
}
impl InterruptThread {
    fn finish(mut self) -> Result<(), String> {
        let _ = self.events.send(Event::Cancel);
        self.thread
            .take()
            .unwrap()
            .join()
            .map_err(|_| "interrupter panicked".to_owned())?
    }
}
impl Drop for InterruptThread {
    fn drop(&mut self) {
        let _ = self.events.send(Event::Cancel);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn decoded_number(macros: &mut CompiledMacros, caller: &mut Session, source: &str) -> f64 {
    let value = caller.eval_with_macros(source, macros).unwrap();
    let bridge = FormBridge::new(caller).unwrap();
    let form = bridge.read(caller, &value, 0..source.len()).unwrap();
    let Kind::Number(number) = &form.kind else {
        panic!("expected number: {form:?}")
    };
    *number
}

#[test]
fn lazy_result_interrupt_is_synchronized_after_thunk_effects_and_recovers() {
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let caller_fuel = caller.options().fuel_per_operation;
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .session
            .eval(concat!(
                "(def realization-irq-effects (atom 0)) ",
                "(def realization-irq-spin (fn [] (loop [] (recur))))",
            ))
            .unwrap();
        let spin = macros.session.eval("realization-irq-spin").unwrap();
        let setter = macros
            .session
            .eval("(fn [helper] (set! realization-irq-spin helper))")
            .unwrap();
        let (events, entered) = mpsc::channel();
        let (acknowledge, ack) = mpsc::channel();
        let ack = Arc::new(Mutex::new(ack));
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_events = events.clone();
        let callback_calls = calls.clone();
        // Existing crate-private interop supplies rooted spin + setter, typed
        // ARGS creation and generic invocation in one inspection RootScope.
        // No raw Rooted GC value escapes this callback.
        macros
            .session
            .data_inspect_calls(
                &spin,
                &setter,
                |mut store, spin, args_new, invoke, setter| {
                    let object = spin
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)?
                        .ok_or_else(|| wasmtime::Error::msg("spin is not a closure"))?;
                    let ty = object.ty(&store)?;
                    let mut fields = object.fields(&mut store)?.collect::<Vec<_>>();
                    if fields.len() != 5 {
                        return Err(wasmtime::Error::msg("unexpected closure layout"));
                    }
                    // Direct construction is necessary because inspect does not
                    // expose closure-new. Do not copy an already assigned UID.
                    let uid = fields[4]
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .ok_or_else(|| wasmtime::Error::msg("closure UID already assigned"))?;
                    if uid.get_u32() != 0 {
                        return Err(wasmtime::Error::msg("expected fresh nil closure UID"));
                    }
                    let Val::FuncRef(Some(original)) = fields[1].clone() else {
                        return Err(wasmtime::Error::msg("missing original invoke Func"));
                    };
                    let original_type = original.ty(&store);
                    let host = Func::new(
                        &mut store,
                        original_type,
                        move |mut callback, args, results| {
                            callback_calls.fetch_add(1, Ordering::SeqCst);
                            callback_events.send(Event::Entered).map_err(|_| {
                                wasmtime::Error::msg("interrupter receiver dropped")
                            })?;
                            ack.lock()
                                .map_err(|_| wasmtime::Error::msg("ack mutex poisoned"))?
                                .recv_timeout(RENDEZVOUS_TIMEOUT)
                                .map_err(|_| {
                                    wasmtime::Error::msg("interrupter acknowledgement missing")
                                })?;
                            // Actual guest reentry observes the requested epoch interrupt.
                            // Never fabricate an Interrupted error in the host callback.
                            original.call(callback.as_context_mut(), args, results)
                        },
                    );
                    fields[1] = Val::FuncRef(Some(host));
                    let allocator = StructRefPre::new(&mut store, ty);
                    let replacement = StructRef::new(&mut store, &allocator, &fields)?;
                    let mut args = [Val::null_any_ref()];
                    args_new.call(&mut store, &[Val::I32(1)], &mut args)?;
                    let array: wasmtime::Rooted<ArrayRef> = args[0]
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .ok_or_else(|| wasmtime::Error::msg("missing ARGS array"))?;
                    array.set(&mut store, 0, Val::AnyRef(Some(replacement.to_anyref())))?;
                    invoke.call(
                        &mut store,
                        &[setter, args[0].clone()],
                        &mut [Val::null_any_ref()],
                    )?;
                    Ok(())
                },
            )
            .unwrap();
        // Ordinary guest assignment owns the fresh closure before the native
        // inspection scope exits; spin stays rooted until the test is finished.
        macros
            .define(concat!(
                "(defmacro realization-irq-lazy [] ",
                "(do (reset! realization-irq-effects 1) ",
                "(new cljs.core/LazySeq nil ",
                "(fn [] (do (reset! realization-irq-effects ",
                "(+ (* @realization-irq-effects 10) 2)) (realization-irq-spin))) nil nil)))",
            ))
            .unwrap();
        macros
            .define("(defmacro realization-irq-observed [] @realization-irq-effects)")
            .unwrap();
        macros
            .define("(defmacro realization-irq-recover [] 42)")
            .unwrap();
        macros.set_operation_fuel(COMPILER_FUEL);
        caller.eval("(def realization-irq-stable 7)").unwrap();
        assert_eq!(
            decoded_number(&mut macros, &mut caller, "(realization-irq-recover)"),
            42.0
        );
        let source = "(def realization-irq-unpublished 99) (realization-irq-lazy)";
        let start = source.find("(realization-irq-lazy)").unwrap();
        let before = caller.stats();
        let interrupt = macros.interrupt_handle();
        let thread = thread::spawn(move || match entered.recv_timeout(RENDEZVOUS_TIMEOUT) {
            Ok(Event::Entered) => {
                interrupt.interrupt();
                acknowledge
                    .send(())
                    .map_err(|_| "callback acknowledgement receiver dropped".to_owned())
            }
            Ok(Event::Cancel) => Err("request ended before lazy thunk callback entry".to_owned()),
            Err(_) => Err("lazy thunk entry not observed".to_owned()),
        });
        let guard = InterruptThread {
            events,
            thread: Some(thread),
        };
        // Save result and always join before asserting or starting another request.
        let result = caller.eval_with_macros(source, &mut macros);
        let synchronization = guard.finish();
        assert!(
            synchronization.is_ok(),
            "lazy realization rendezvous failed: {synchronization:?}; request: {result:?}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let error = result.unwrap_err();
        let SessionError::Compile(diagnostic) = error else {
            panic!("expected source/stage compilation diagnostic: {error}");
        };
        assert_eq!(diagnostic.span, start..source.len());
        assert!(
            diagnostic
                .message
                .contains("during expanded result transport"),
            "{diagnostic:?}"
        );
        assert!(diagnostic.message.contains("Interrupted"), "{diagnostic:?}");
        assert_eq!(caller.stats(), before);
        let missing = caller.eval("realization-irq-unpublished").unwrap_err();
        assert!(missing.to_string().contains("Unresolved"), "{missing}");
        let stable = caller.eval("realization-irq-stable").unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        let stable = bridge.read(&mut caller, &stable, 0..1).unwrap();
        assert!(matches!(stable.kind, Kind::Number(n) if n == 7.0));
        assert_eq!(
            decoded_number(&mut macros, &mut caller, "(realization-irq-observed)"),
            12.0
        );
        caller.collect().unwrap();
        assert_eq!(
            decoded_number(&mut macros, &mut caller, "(realization-irq-recover)"),
            42.0
        );
        assert_eq!(caller.options().fuel_per_operation, caller_fuel);
        assert_eq!(
            crate::portable::ExpansionHost::expansion_work_fuel(&macros),
            COMPILER_FUEL
        );
        drop(spin);
    }
}
