//! Explicit form-data transfer to an isolated compiled Macro function. The
//! caller's closure retains the actual function root and its owning service.
use super::*;
use std::{
    sync::{
        Condvar,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

// Forward cancellation through the public engine/flag handle, never a Store
// pointer. One operation owns the relay; it is joined before the lease returns.
// The condition variable wakes teardown immediately (no per-operation delay).
pub(crate) struct InterruptRelay {
    finished: Option<Arc<(Mutex<bool>, Condvar)>>,
    thread: Option<JoinHandle<()>>,
}
impl InterruptRelay {
    pub(crate) fn start(
        parent: Arc<AtomicBool>,
        target: crate::portable_session::InterruptHandle,
    ) -> Result<Self, SessionError> {
        if target.has_suspended_parent(&parent) {
            return Ok(Self { finished: None, thread: None });
        }
        let finished = Arc::new((Mutex::new(false), Condvar::new()));
        let worker = finished.clone();
        let thread = std::thread::Builder::new()
            .name("suss-macro-interrupt".into())
            .spawn(move || {
                let (lock, changed) = &*worker;
                let mut done = lock.lock().unwrap_or_else(|error| error.into_inner());
                while !*done {
                    if parent.load(Ordering::SeqCst) {
                        target.interrupt();
                    }
                    done = changed
                        .wait_timeout(done, Duration::from_millis(5))
                        .unwrap_or_else(|error| error.into_inner())
                        .0;
                }
            })
            .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
        Ok(Self {
            finished: Some(finished),
            thread: Some(thread),
        })
    }
}
impl Drop for InterruptRelay {
    fn drop(&mut self) {
        let Some(finished) = &self.finished else { return };
        let (lock, changed) = &**finished;
        *lock.lock().unwrap_or_else(|error| error.into_inner()) = true;
        changed.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) fn transferable_error(host: &mut MacroHost<'_>, error: SessionError) -> SessionError {
    let SessionError::Language(value) = error else { return error };
    match host.bridge.transfer_exception(&mut host.session, &value) {
        Ok(payload) => SessionError::Host(wasmtime::Error::new(payload)),
        // A failure while transporting the owned payload must also remain in
        // its source Store. Preserve an honest diagnostic, never a foreign root.
        Err(error) if matches!(error, SessionError::Language(_)) => SessionError::Host(
            wasmtime::Error::msg(format!("Cannot transfer macro exception: {}",
                crate::portable_repl::error_display(&mut host.session, &error)))),
        Err(error) => error,
    }
}

impl MacroService {
    /// Prepare once, then let the caller compile its publication before any
    /// macro binding is written. `prepare_caller` first debits preparation work
    /// and returns its remaining allowance. No lease is held during caller
    /// source execution. Its initializer is only a private native constant.
    pub(crate) fn declare_evaluation<T>(
        &self,
        form: Form,
        namespace: &str,
        fuel: u64,
        context: crate::portable_session::EvaluationMathContext,
        interrupt: Arc<AtomicBool>,
        prepare_caller: impl FnOnce(Form, Form, Vec<String>, u64) -> Result<(T, u64), SessionError>,
    ) -> Result<(Result<(T, SessionValue), SessionError>, u64), SessionError> {
        let mut lease = self.lease();
        let allowance = lease.limit_fuel(fuel);
        lease.with_math_context(context, |host| {
        let _budget = host.session.operation_budget_scope()?;
        let _relay = InterruptRelay::start(interrupt, host.session.interrupt_handle())?;
        let original = host.session.current_namespace().to_owned();
        host.enter_namespace(namespace)?;
        let origin = crate::portable::SourceOrigin::native_evaluation(host.session.native_owner());
        let pending = host.prepare_macro_definition(&form, form.span.clone(), Some(&origin));
        let spent = allowance.saturating_sub(host.session.operation_fuel_remaining()?);
        let mut pending = match pending {
            Ok(pending) => pending,
            Err(error) => {
                let error = transferable_error(host, error);
                host.enter_namespace(&original)?;
                return Ok((Err(error), spent));
            }
        };
        let template = std::mem::replace(
            &mut pending.template,
            Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::Nil,
            },
        );
        let Kind::List(parts) = &template.kind else {
            unreachable!("validated macro prefix")
        };
        let Kind::Symbol(name) = &parts[1].kind else {
            unreachable!("validated macro name")
        };
        let mut names: std::collections::BTreeSet<String> = catalog(&host.catalog)
            .definitions
            .keys()
            .filter(|(ns, _)| ns == namespace)
            .map(|(_, name)| name.clone())
            .collect();
        names.insert(name.name.clone());
        let (caller, remaining) = match prepare_caller(template, form, names.into_iter().collect(), spent)
        {
            Ok(caller) => caller,
            Err(error) => {
                host.enter_namespace(&original)?;
                return Ok((Err(error), 0));
            }
        };
        let before = match host.session.cap_operation_fuel(remaining) {
            Ok(before) => before,
            Err(error) => {
                host.enter_namespace(&original)?;
                return Err(error);
            }
        };
        let committed = host.commit_macro_definition(pending);
        let spent = before.saturating_sub(host.session.operation_fuel_remaining()?);
        let committed = committed
            .map(|(root, _variable)| (caller, root))
            .map_err(|error| transferable_error(host, error));
        host.enter_namespace(&original)?;
        Ok((committed, spent))
        })
    }

    pub(crate) fn invoke_root(
        &self,
        root: &SessionValue,
        namespace: &str,
        arguments: Vec<Form>,
        fuel: u64,
        context: crate::portable_session::EvaluationMathContext,
        interrupt: Arc<AtomicBool>,
    ) -> Result<(Result<Form, SessionError>, u64), SessionError> {
        self.with_request(fuel, Some(context), interrupt, |host| {
            let result = (|| {
                host.select_invocation_namespace(namespace)?;
                let mut values = Vec::with_capacity(arguments.len());
                for form in arguments {
                    values.push(host.bridge.quote(&mut host.session, form)?);
                }
                let refs = values.iter().collect::<Vec<_>>();
                let result = host.session.invoke(root, &refs)?;
                host.bridge.read(&mut host.session, &result, 0..0)
            })();
            result.map_err(|error| transferable_error(host, error))
        })
    }
}

impl MacroHost<'_> {
    fn select_invocation_namespace(&mut self, namespace: &str) -> Result<(), SessionError> {
        // Always select the actual caller. A preceding nested eval can have
        // rebound *ns*, independently of the expansion catalog's memo.
        let existing = catalog(&self.catalog)
            .namespace_objects
            .get(namespace)
            .cloned();
        let object = if let Some(object) = existing {
            object
        } else {
            let name = self.bridge.quote(
                &mut self.session,
                Form {
                    span: 0..0,
                    metadata: vec![],
                    kind: Kind::Symbol(suss_reader::Symbol::new(namespace)),
                },
            )?;
            let object = self.session.invoke_compiler_root(&self.namespace_interner, &[&name])?;
            catalog(&self.catalog)
                .namespace_objects
                .insert(namespace.to_owned(), object.clone());
            object
        };
        self.session.invoke_compiler_root(&self.namespace_setter, &[&object])?;
        catalog(&self.catalog).macro_namespace = Some(namespace.to_owned());
        Ok(())
    }
}

#[cfg(all(test, not(suss_generator)))]
mod tests {
    use super::*;

    #[test]
    fn suspended_cancellation_uses_ancestor_flags_without_creating_relay_threads() {
        let mut parent = Session::new().unwrap();
        let child = Session::with_options_in(crate::portable_session::SessionOptions::default(), crate::portable::resolve::Phase::Macro).unwrap();
        let flag = parent.host_work_interrupt();
        crate::portable_session::suspended::scope(&mut parent, || {
            let direct = InterruptRelay::start(flag, child.interrupt_handle()).unwrap();
            assert!(direct.thread.is_none());
            assert!(direct.finished.is_none());
            let independent = InterruptRelay::start(Arc::new(AtomicBool::new(false)), child.interrupt_handle()).unwrap();
            assert!(independent.thread.is_some(), "unregistered parents still need the original relay");
        });
    }

    #[test]
    fn declaration_cancellation_restores_namespace_before_returning_the_lease() {
        let mut macros = CompiledMacros::new().unwrap();
        let service = macros.service();
        let original = macros.current_namespace();
        let interrupt = macros.interrupt_handle();
        let form = suss_reader::forms::read_forms("(defmacro cancelled-declaration [] 42)").unwrap().pop().unwrap();
        let result = service.declare_evaluation(
            form, "cancelled-namespace", 400_000_000, crate::portable_session::EvaluationMathContext::Nil,
            Arc::new(AtomicBool::new(false)),
            move |_, _, _, _| {
                // Cancel after preparation, precisely at the cap before commit.
                interrupt.interrupt();
                Ok(((), u64::MAX))
            },
        );
        assert!(matches!(result, Err(ref error) if error.is_interrupt()), "{result:?}");
        assert_eq!(macros.current_namespace(), original);
        macros.define("(defmacro recovered-declaration [] 42)").unwrap();
        assert_eq!(macros.current_namespace(), original);
    }
    /// Find an observable compiled effect boundary; successful return is not
    /// required because a later trap cannot undo the observation.
    fn first_effect_fuel(mut run: impl FnMut(u64) -> bool) -> u64 {
        let mut high = 1024u64;
        while !run(high) {
            high = high.checked_mul(2).unwrap();
            assert!(high < 400_000_000, "finite marker must fit the normal operation allowance");
        }
        let mut low = 0;
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if run(middle) { high = middle; } else { low = middle; }
        }
        assert!(run(high));
        high
    }

    #[test]
    fn isolated_source_macro_context_snapshot_work_is_debited_before_child_allowance() {
        use crate::portable_session::EvaluationMathContext;
        use std::sync::atomic::AtomicUsize;
        let mut failures = Vec::new();
        for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let phase = caller.phase();
            let mut macros = CompiledMacros::new().unwrap();
            let observations = Arc::new(AtomicUsize::new(0));
            let seen = observations.clone();
            macros.install_observation_hook(move |_, _| {
                seen.fetch_add(1, Ordering::SeqCst);
                Ok(0)
            }).unwrap();
            // A genuine source declaration retains the isolated Store. Public
            // eval declarations instead belong to the caller's live catalog.
            macros.define("(defmacro context-ledger-marker [] (suss.harness.observation/capture 1) 42)").unwrap();
            // Measure the same resident compiler/bridge used throughout the
            // boundary search, rather than charging one cold initialization
            // only to the separately measured snapshot.
            caller.eval_with_macros("(eval '(+ 0 0))", &mut macros).unwrap();
            // Raw function calls have the core namespace root, independently
            // of the source input's user namespace frame.
            let wrapper = caller.eval_with_macros("(fn [context] (binding [*math-context* context] (eval '(user/context-ledger-marker))))", &mut macros).unwrap();
            let namespace = caller.current_namespace();
            let service = macros.service();
            let root = {
                let mut lease = service.lease();
                let values = catalog(&lease.host().catalog);
                values.definitions.get(&(namespace.clone(), "context-ledger-marker".to_owned())).unwrap().clone()
            };
            let bridge = FormBridge::new(&mut caller).unwrap();
            for count in [256usize, 1024] {
                caller.set_operation_fuel(400_000_000);
                let form = Form { span: 0..0, metadata: vec![], kind: Kind::Vector((0..count).map(|n| Form { span: 0..0, metadata: vec![], kind: Kind::Long(n as i64) }).collect()) };
                let context = bridge.quote(&mut caller, form.clone()).unwrap();
                // Measure actual caller-side transport work independently of
                // child execution. No fixed instruction count is assumed.
                let snapshot_cost = {
                    let _budget = caller.operation_budget_scope().unwrap();
                    let checkpoint = caller.enter_evaluation_math_context(EvaluationMathContext::Data(form.clone()), &bridge).unwrap();
                    let before = caller.operation_fuel_remaining().unwrap();
                    let transported = caller.evaluation_math_context().unwrap();
                    let after = caller.operation_fuel_remaining().unwrap();
                    assert!(matches!(transported, EvaluationMathContext::Data(_)));
                    caller.leave_evaluation_math_context(&checkpoint).unwrap();
                    before.checked_sub(after).unwrap()
                };
                assert!(snapshot_cost > 0);
                let nil = Form { span: 0..0, metadata: vec![], kind: Kind::Nil };
                // Prove both real entrypoints reach the marker before using
                // its presence as a monotonic fuel-search observation.
                observations.store(0, Ordering::SeqCst);
                let (control, _) = service.invoke_root(&root, &namespace, vec![nil.clone(), nil.clone()], 400_000_000,
                    EvaluationMathContext::Data(form.clone()), Arc::new(AtomicBool::new(false))).unwrap();
                assert!(control.is_ok(), "{phase:?} elements={count}: direct child control: {control:?}");
                assert!(observations.load(Ordering::SeqCst) > 0);
                caller.set_operation_fuel(400_000_000);
                observations.store(0, Ordering::SeqCst);
                let control = caller.invoke(&wrapper, &[&context]);
                if let Err(error) = control {
                    panic!("{phase:?} elements={count}: public control: {}", crate::portable_repl::error_display(&mut caller, &error));
                }
                assert!(observations.load(Ordering::SeqCst) > 0);
                let child_minimum = first_effect_fuel(|fuel| {
                    observations.store(0, Ordering::SeqCst);
                    let _ = service.invoke_root(&root, &namespace, vec![nil.clone(), nil.clone()], fuel,
                        EvaluationMathContext::Data(form.clone()), Arc::new(AtomicBool::new(false)));
                    observations.load(Ordering::SeqCst) != 0
                });
                let public_minimum = first_effect_fuel(|fuel| {
                    caller.set_operation_fuel(fuel);
                    observations.store(0, Ordering::SeqCst);
                    let _ = caller.invoke(&wrapper, &[&context]);
                    observations.load(Ordering::SeqCst) != 0
                });
                // The public entry must pay at least the independently
                // measured caller snapshot plus the direct child's real work.
                // All remaining parent work is nonnegative. This remains true
                // when legitimate optimizations change either measured cost.
                let minimum = child_minimum.checked_add(snapshot_cost).unwrap();
                eprintln!("context-budget-ledger {phase:?} elements={count} snapshot={snapshot_cost} child={child_minimum} public={public_minimum} lower-bound={minimum}");
                if public_minimum < minimum { failures.push(format!("{phase:?} elements={count}: child effect at {public_minimum}, below caller+child lower bound {minimum}")); }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

}
