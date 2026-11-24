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
pub(super) struct InterruptRelay {
    finished: Arc<(Mutex<bool>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}
impl InterruptRelay {
    pub(super) fn start(
        parent: Arc<AtomicBool>,
        target: crate::portable_session::InterruptHandle,
    ) -> Result<Self, SessionError> {
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
            finished,
            thread: Some(thread),
        })
    }
}
impl Drop for InterruptRelay {
    fn drop(&mut self) {
        let (lock, changed) = &*self.finished;
        *lock.lock().unwrap_or_else(|error| error.into_inner()) = true;
        changed.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn transferable_error(host: &mut MacroHost<'static>, error: SessionError) -> SessionError {
    // A language exception owns a root in this Macro Store. Preserve its actual
    // display as a new caller-side Error instead of sending that root across
    // Stores. VM traps remain traps and never carry a portable payload.
    if matches!(error, SessionError::Language(_)) {
        SessionError::Host(wasmtime::Error::msg(crate::portable_repl::error_display(
            &mut host.session,
            &error,
        )))
    } else {
        error
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
        interrupt: Arc<AtomicBool>,
        prepare_caller: impl FnOnce(Form, u64) -> Result<(T, u64), SessionError>,
    ) -> Result<(Result<(T, SessionValue), SessionError>, u64), SessionError> {
        let mut lease = self.lease();
        let allowance = lease.limit_fuel(fuel);
        let host = lease.host();
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
        let (caller, remaining) = match prepare_caller(template, spent) {
            Ok(caller) => caller,
            Err(error) => {
                host.enter_namespace(&original)?;
                return Ok((Err(error), 0));
            }
        };
        let before = host.session.cap_operation_fuel(remaining)?;
        let committed = host.commit_macro_definition(pending);
        let spent = before.saturating_sub(host.session.operation_fuel_remaining()?);
        let committed = committed
            .map(|(root, _variable)| (caller, root))
            .map_err(|error| transferable_error(host, error));
        host.enter_namespace(&original)?;
        Ok((committed, spent))
    }

    pub(crate) fn invoke_root(
        &self,
        root: &SessionValue,
        namespace: &str,
        arguments: Vec<Form>,
        fuel: u64,
        interrupt: Arc<AtomicBool>,
    ) -> Result<(Result<Form, SessionError>, u64), SessionError> {
        let mut lease = self.lease();
        let allowance = lease.limit_fuel(fuel);
        let host = lease.host();
        let _budget = host.session.operation_budget_scope()?;
        let _relay = InterruptRelay::start(interrupt, host.session.interrupt_handle())?;
        let result = (|| {
            host.select_invocation_namespace(namespace)?;
            let mut values = Vec::with_capacity(arguments.len());
            for form in arguments {
                values.push(host.bridge.quote(&mut host.session, form)?);
            }
            let refs = values.iter().collect::<Vec<_>>();
            let result = host.session.invoke(root, &refs)?;
            host.bridge.read(&mut host.session, &result, 0..0)
        })()
        .map_err(|error| transferable_error(host, error));
        let spent = allowance.saturating_sub(host.session.operation_fuel_remaining()?);
        Ok((result, spent))
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
            let object = self.session.invoke(&self.namespace_interner, &[&name])?;
            catalog(&self.catalog)
                .namespace_objects
                .insert(namespace.to_owned(), object.clone());
            object
        };
        self.session.invoke(&self.namespace_setter, &[&object])?;
        catalog(&self.catalog).macro_namespace = Some(namespace.to_owned());
        Ok(())
    }
}
