//! Concrete, single-owner Unix REPL host. Kept unlinked while Session APIs stage.
//! Native read-byte is an explicit profile, not canonical WIT transport. Source
//! calls only enqueue Pending storage; this owner copies paths, polls and settles.
#![cfg(unix)]

use super::session_event_loop::{
    AsyncErrorDisposition, FrontendHost, HostReport, Submission, TaskCounts,
};
use nix::libc;
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, BorrowedFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    sync::{Arc, Mutex},
};
use suss_compile::{
    portable::hir::Literal,
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_repl::{self, CompiledValue, NativeDisplay},
    portable_session::{
        FutureState, FutureStatus, NativeRequest, NativeRequestQueue, Session, SessionError,
        SessionValue,
    },
};
use suss_reader::{Symbol, forms::Kind};

#[derive(Clone, Copy)]
struct InputIdentity {
    dev: u64,
    ino: u64,
}
impl InputIdentity {
    fn from_fd(fd: BorrowedFd<'_>) -> io::Result<Self> {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: the borrowed input fd stays open for this call, and fstat
        // initializes the complete output only on success.
        if unsafe { libc::fstat(fd.as_raw_fd(), stat.as_mut_ptr()) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let stat = unsafe { stat.assume_init() };
        Ok(Self {
            dev: stat.st_dev as u64,
            ino: stat.st_ino as u64,
        })
    }
    fn aliases(&self, file: &File) -> io::Result<bool> {
        let stat = file.metadata()?;
        Ok(self.dev == stat.dev() && self.ino == stat.ino())
    }
}

/// The cancellation hook and controller share one owned fd slot. No lock spans
/// a Session call: hooks run on this owner and close exactly once by taking it.
type FileSlot = Arc<Mutex<Option<File>>>;
fn close(slot: &FileSlot) {
    drop(slot.lock().unwrap_or_else(|e| e.into_inner()).take());
}

enum Outcome {
    Byte(u8),
    Eof,
    Error(String),
}
struct Endpoint {
    request: NativeRequest,
    file: Option<FileSlot>,
    outcome: Option<Outcome>,
    // Retain the exact prepared publication across fuel errors; no read replay.
    payload: Option<SessionValue>,
}
impl Endpoint {
    fn new(request: NativeRequest) -> Self {
        Self {
            request,
            file: None,
            outcome: None,
            payload: None,
        }
    }
    fn close(&mut self) {
        if let Some(file) = self.file.take() {
            close(&file);
        }
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.close();
    }
}

/// A single owned, O_NONBLOCK endpoint; it never reads the REPL input identity.
fn open_endpoint(path: &str, input: InputIdentity) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOCTTY)
        .open(path)?;
    if input.aliases(&file)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "suss.io/read-byte cannot open an alias of REPL input",
        ));
    }
    Ok(file)
}

fn poll_byte(file: &FileSlot) -> io::Result<Option<Outcome>> {
    let mut guard = file.lock().unwrap_or_else(|e| e.into_inner());
    let Some(file) = guard.as_mut() else {
        return Ok(None);
    };
    let mut descriptor = libc::pollfd {
        fd: file.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one valid pollfd is borrowed from the owned file; zero timeout
    // performs no readiness wait, and the subsequent one-byte read is nonblocking.
    let ready = unsafe { libc::poll(&mut descriptor, 1, 0) };
    if ready < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            return Ok(None);
        }
        return Err(error);
    }
    if ready == 0 {
        return Ok(None);
    }
    if descriptor.revents & libc::POLLNVAL != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid read-byte endpoint",
        ));
    }
    let mut byte = [0];
    match file.read(&mut byte) {
        Ok(0) => Ok(Some(Outcome::Eof)),
        Ok(1) => Ok(Some(Outcome::Byte(byte[0]))),
        Ok(_) => unreachable!("one-byte read"),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub struct NativeReplHost<Out: Write, Err: Write> {
    session: Session,
    macros: CompiledMacros,
    display: NativeDisplay,
    bridge: Option<FormBridge>,
    requests: NativeRequestQueue,
    endpoints: Vec<Endpoint>,
    tasks: Vec<SessionValue>,
    input: InputIdentity,
    last_generation: u64,
    cancelling: bool,
    output: Out,
    errors: Err,
    output_error: Option<io::Error>,
    async_reports: Vec<HostReport<CompiledValue, SessionError>>,
    reported_trap_owners: Vec<SessionValue>,
    last_pump_recovered: bool,
    deferred_observation_interrupt: bool,
    completed_submission: Option<CompiledValue>,
    submission_quarantined: bool,
    #[cfg(test)]
    injected_submission_tracking_failure: bool,
    #[cfg(test)]
    reset_preflight: bool,
    #[cfg(test)]
    injected_observation_interrupt: Option<(&'static str, usize)>,
}
impl<Out: Write, Err: Write> NativeReplHost<Out, Err> {
    /// The caller installs signal/terminal handling and owns main linkage. This
    /// explicit native profile is reinstalled by Session's replacement contract.
    pub fn new(
        mut session: Session,
        macros: CompiledMacros,
        input: BorrowedFd<'_>,
        output: Out,
        errors: Err,
    ) -> Result<Self, SessionError> {
        let input = InputIdentity::from_fd(input).map_err(host_io)?;
        let bridge = FormBridge::new(&mut session)?;
        let requests =
            session.install_native_future_factory(Symbol::namespaced("suss.io", "read-byte"), 1)?;
        Ok(Self {
            session,
            macros,
            display: NativeDisplay::default(),
            bridge: Some(bridge),
            requests,
            endpoints: Vec::new(),
            tasks: Vec::new(),
            input,
            last_generation: 0,
            cancelling: false,
            output,
            errors,
            output_error: None,
            async_reports: Vec::new(),
            reported_trap_owners: Vec::new(),
            last_pump_recovered: false,
            deferred_observation_interrupt: false,
            completed_submission: None,
            submission_quarantined: false,
            #[cfg(test)]
            injected_submission_tracking_failure: false,
            #[cfg(test)]
            reset_preflight: false,
            #[cfg(test)]
            injected_observation_interrupt: None,
        })
    }
    pub fn interrupt_handles(
        &self,
    ) -> (
        suss_compile::portable_session::InterruptHandle,
        suss_compile::portable_session::InterruptHandle,
    ) {
        (
            self.session.interrupt_handle(),
            self.macros.interrupt_handle(),
        )
    }
    fn check_output(&mut self) -> Result<(), SessionError> {
        if let Some(error) = self.output_error.take() {
            return Err(host_io(error));
        }
        Ok(())
    }
    fn ensure_bridge(&mut self) -> Result<(), SessionError> {
        if self.bridge.is_none() {
            self.bridge = Some(FormBridge::new(&mut self.session)?);
        }
        Ok(())
    }
    fn scalar(&mut self, literal: &Literal) -> Result<SessionValue, SessionError> {
        self.ensure_bridge()?;
        self.bridge
            .as_ref()
            .unwrap()
            .scalar(&mut self.session, literal)
    }
    fn nil(&mut self) -> Result<CompiledValue, SessionError> {
        self.scalar(&Literal::Nil).map(CompiledValue::Runtime)
    }
    // Only feed results of explicit pure Session getters into this classifier.
    // Test injection happens after executing the real getter, before its result
    // reaches the host: nil/zero results must never masquerade as known counts.
    fn getter_result<R>(
        &mut self,
        _site: &'static str,
        result: Result<R, SessionError>,
    ) -> Result<R, SessionError> {
        #[cfg(test)]
        let result = if result.is_ok()
            && self
                .injected_observation_interrupt
                .as_ref()
                .is_some_and(|(site, _)| *site == _site)
        {
            let (_, remaining) = self.injected_observation_interrupt.as_mut().unwrap();
            if *remaining == 0 {
                self.injected_observation_interrupt = None;
                Err(SessionError::Trap(wasmtime::Trap::Interrupt.into()))
            } else {
                *remaining -= 1;
                result
            }
        } else {
            result
        };
        self.deferred_observation_interrupt =
            result.as_ref().is_err_and(|error| error.is_interrupt());
        result
    }
    fn complete_submission(
        &mut self,
        value: CompiledValue,
    ) -> Result<Submission<CompiledValue>, SessionError> {
        let tracking = self.track_tasks();
        let tracking = self.getter_result("submission-tracking", tracking);
        #[cfg(test)]
        let tracking = if self.injected_submission_tracking_failure && tracking.is_ok() {
            self.injected_submission_tracking_failure = false;
            Err(SessionError::Trap(wasmtime::Trap::OutOfFuel.into()))
        } else {
            tracking
        };
        match tracking {
            Ok(()) => Ok(Submission::Value(value)),
            Err(error) => {
                // Source already completed: keep its rooted result, including
                // nil. Only observations may be retried; never evaluate again.
                self.completed_submission = Some(value);
                // This fresh submission receipt is separate from scheduler
                // recovery. Non-signal tracking failure must stop the frontend,
                // retaining the result rather than leaving a poisoned prompt.
                self.submission_quarantined = !error.is_interrupt();
                Err(error)
            }
        }
    }
    fn track_tasks(&mut self) -> Result<(), SessionError> {
        let snapshot = self.session.pending_task_snapshot();
        for owner in self.getter_result("snapshot", snapshot)? {
            let mut present = false;
            for index in 0..self.tasks.len() {
                let identical = self.session.values_identical(&self.tasks[index], &owner);
                if self.getter_result("identity", identical)? {
                    present = true;
                    break;
                }
            }
            if !present {
                self.tasks.push(owner);
            }
        }
        Ok(())
    }
    fn observe_tasks(&mut self) -> Result<(), SessionError> {
        let mut index = 0;
        while index < self.tasks.len() {
            let status = self.session.future_status(&self.tasks[index]);
            match self.getter_result("task-status", status)? {
                Some(FutureStatus::Pending) => index += 1,
                Some(FutureStatus::Failed) => {
                    let state = self.session.future_state(&self.tasks[index]);
                    let Some(FutureState::Failed(payload)) =
                        self.getter_result("task-state", state)?
                    else {
                        return Err(
                            wasmtime::Error::msg("Failed task changed terminal state").into()
                        );
                    };
                    let mut consumed = None;
                    for receipt in 0..self.reported_trap_owners.len() {
                        let identical = self.session.values_identical(
                            &self.reported_trap_owners[receipt],
                            &self.tasks[index],
                        );
                        if self.getter_result("receipt-identity", identical)? {
                            consumed = Some(receipt);
                            break;
                        }
                    }
                    let marker = self.session.is_async_runtime_trap(&payload);
                    let marker = self.getter_result("trap-marker", marker)?;
                    self.tasks.remove(index);
                    if let Some(receipt) = consumed {
                        self.reported_trap_owners.remove(receipt);
                    }
                    if !(marker && consumed.is_some()) {
                        // Suppress only this exact owner's already-reported turn
                        // error. A transported marker is not a language throw.
                        let error = if marker {
                            wasmtime::Error::msg(
                                "Async task failed with a transported runtime trap marker",
                            )
                            .into()
                        } else {
                            SessionError::Language(payload)
                        };
                        self.async_reports.push(HostReport::Error(error));
                    }
                }
                Some(FutureStatus::Ready | FutureStatus::Cancelled) => {
                    self.tasks.remove(index);
                }
                None => {
                    return Err(wasmtime::Error::msg("Task owner is not a nominal future").into());
                }
            }
        }
        self.check_output()
    }
    fn drain_requests(&mut self) -> Result<(), SessionError> {
        let first = self.endpoints.len();
        // Publish ALL drained native ownership before any fallible validation.
        // An error leaves every unprocessed request available for retry/reset.
        self.endpoints
            .extend(self.requests.drain().into_iter().map(Endpoint::new));
        for endpoint in &self.endpoints[first..] {
            if endpoint.request.id.generation <= self.last_generation {
                return Err(
                    wasmtime::Error::msg("Duplicate/stale native operation generation").into(),
                );
            }
            self.last_generation = endpoint.request.id.generation;
        }
        Ok(())
    }
    fn prepare_endpoint(&mut self, index: usize) -> Result<(), SessionError> {
        if self.endpoints[index].file.is_some() || self.endpoints[index].outcome.is_some() {
            return Ok(());
        }
        self.ensure_bridge()?;
        let string_argument = if self.endpoints[index].request.args.len() == 1 {
            self.session
                .inspect(&self.endpoints[index].request.args[0], |store, value| {
                    let Some(reference) = value.unwrap_anyref() else {
                        return Ok(false);
                    };
                    let Some(array) = reference.as_array(&store)? else {
                        return Ok(false);
                    };
                    Ok(matches!(
                        array.ty(&store)?.element_type(),
                        wasmtime::StorageType::I16
                    ))
                })?
        } else {
            false
        };
        // Reject non-strings before generic FormBridge decoding: a lazy/seq
        // argument must never run user callbacks merely to diagnose its type.
        let path = if !string_argument {
            Err("suss.io/read-byte expects one string path".to_owned())
        } else {
            let form = self.bridge.as_ref().unwrap().read(
                &mut self.session,
                &self.endpoints[index].request.args[0],
                0..0,
            )?;
            match form.kind {
                Kind::String(units) => String::from_utf16(&units)
                    .map_err(|_| "suss.io/read-byte path contains invalid UTF-16".to_owned()),
                _ => {
                    return Err(
                        wasmtime::Error::msg("String transport returned non-string form").into(),
                    );
                }
            }
        };
        let endpoint = &mut self.endpoints[index];
        match path {
            Err(message) => endpoint.outcome = Some(Outcome::Error(message)),
            Ok(path) => match open_endpoint(&path, self.input) {
                Err(error) => {
                    endpoint.outcome = Some(Outcome::Error(format!("suss.io/read-byte: {error}")))
                }
                Ok(file) => {
                    let slot = Arc::new(Mutex::new(Some(file)));
                    let cancellation = slot.clone();
                    endpoint
                        .request
                        .set_cancel_hook(move || close(&cancellation))?;
                    endpoint.file = Some(slot);
                }
            },
        }
        endpoint.request.args.clear();
        Ok(())
    }
    fn publish_endpoint(&mut self, index: usize) -> Result<bool, SessionError> {
        // Validation is per endpoint, after the full queue is owned. An old
        // Store cannot open a path or publish into its replacement generation.
        let nominal = self
            .session
            .is_future(&self.endpoints[index].request.future);
        match self.getter_result("endpoint-is-future", nominal) {
            Err(SessionError::ForeignValue) => {
                self.endpoints[index].close();
                return Ok(true);
            }
            Err(error) => return Err(error),
            Ok(false) => {
                return Err(
                    wasmtime::Error::msg("Native request has non-future completion").into(),
                );
            }
            Ok(true) => {}
        }
        if self.endpoints[index].request.is_cancelled() {
            self.endpoints[index].close();
            return Ok(true);
        }
        // Source first-terminal-wins must not be relabelled native completion.
        // The next ordinary pump sweeps its hook; this controller closes its fd.
        let status = self
            .session
            .future_status(&self.endpoints[index].request.future);
        match self.getter_result("endpoint-status", status)? {
            Some(FutureStatus::Pending) => {}
            Some(_) => {
                self.endpoints[index].close();
                return Ok(true);
            }
            None => {
                return Err(wasmtime::Error::msg("Native completion lost nominal storage").into());
            }
        }
        self.prepare_endpoint(index)?;
        if self.endpoints[index].outcome.is_none() {
            let outcome = poll_byte(self.endpoints[index].file.as_ref().unwrap());
            self.endpoints[index].outcome = match outcome {
                Ok(outcome) => outcome,
                Err(error) => Some(Outcome::Error(format!("suss.io/read-byte: {error}"))),
            };
        }
        if self.endpoints[index].outcome.is_none() {
            return Ok(false);
        }
        let failed = matches!(self.endpoints[index].outcome, Some(Outcome::Error(_)));
        if self.endpoints[index].payload.is_none() {
            // Copy only small owned outcome data before mutably borrowing the
            // bridge/Session. No read or source work is repeated after this point.
            let literal = match self.endpoints[index].outcome.as_ref().unwrap() {
                Outcome::Byte(byte) => Literal::Number(*byte as f64),
                Outcome::Eof => Literal::Nil,
                Outcome::Error(message) => Literal::String(message.encode_utf16().collect()),
            };
            let value = self.scalar(&literal)?;
            let payload = if failed {
                let data = self
                    .bridge
                    .as_ref()
                    .unwrap()
                    .map_values(&mut self.session, &[])?;
                self.session.native_exception_info(&value, &data)?
            } else {
                value
            };
            self.endpoints[index].payload = Some(payload);
        }
        // Read outcome is owned now; close before publishing, and preserve it
        // on a refresh trap so retry never consumes a second byte or reopens.
        self.endpoints[index].close();
        let endpoint = &self.endpoints[index];
        let payload = endpoint.payload.as_ref().unwrap();
        if failed {
            self.session
                .reject_future(&endpoint.request.future, payload)?;
        } else {
            self.session
                .resolve_future(&endpoint.request.future, payload)?;
        }
        Ok(true)
    }
}
fn host_io(error: io::Error) -> SessionError {
    wasmtime::Error::msg(error.to_string()).into()
}

impl<Out: Write, Err: Write> FrontendHost for NativeReplHost<Out, Err> {
    type RootResult = CompiledValue;
    type Error = SessionError;
    fn submit(&mut self, source: &str) -> Result<Submission<CompiledValue>, SessionError> {
        self.deferred_observation_interrupt = false;
        self.submission_quarantined = false;
        self.check_output()?;
        if self.completed_submission.is_some() {
            self.submission_quarantined = true;
            return Err(wasmtime::Error::msg("Completed submission still awaits tracking").into());
        }
        let mut words = source.split_whitespace();
        let command = words.next().unwrap_or("");
        if matches!(command, ":load" | ":reload" | ":reload-all" | ":in-ns") {
            let namespace = words
                .next()
                .filter(|_| words.next().is_none())
                .ok_or_else(|| {
                    wasmtime::Error::msg(format!("{command} expects one namespace name"))
                })?;
            match command {
                ":load" => {
                    self.session
                        .load_namespace_with_macros(namespace, &mut self.macros)?;
                }
                ":reload" => {
                    self.session.reload_namespace_with_macros(
                        namespace,
                        false,
                        &mut self.macros,
                    )?;
                }
                ":reload-all" => {
                    self.session
                        .reload_namespace_with_macros(namespace, true, &mut self.macros)?;
                }
                ":in-ns" => self.session.enter_namespace(namespace)?,
                _ => unreachable!(),
            }
            let value = self.nil()?;
            return self.complete_submission(value);
        }
        // Complete reader validation precedes macro expansion, native factories
        // and all source effects. The evaluator resolves reader conditionals.
        match suss_reader::forms::read_forms(source) {
            Err(error) if error.expected.iter().any(|hint| hint == "more input") => {
                return Ok(Submission::NeedMoreInput);
            }
            Err(error) => {
                return Err(SessionError::Compile(suss_compile::portable::Diagnostic {
                    span: error.span,
                    message: error.message,
                }));
            }
            Ok(forms) if forms.is_empty() => return Ok(Submission::Empty),
            Ok(_) => {}
        }
        let result =
            portable_repl::evaluate_compiled_value(&mut self.session, &mut self.macros, source);
        match result {
            Err(original) => {
                // Keep discovered owners, but an evaluation failure is not a
                // pure observation receipt even if subsequent tracking fails.
                let _ = self.track_tasks();
                self.deferred_observation_interrupt = false;
                Err(original)
            }
            Ok(value) => self.complete_submission(value),
        }
    }
    fn submission_error_requires_quarantine(&self, _: &SessionError) -> bool {
        self.submission_quarantined
    }
    fn resume_completed_submission(&mut self) -> Result<Option<CompiledValue>, SessionError> {
        self.deferred_observation_interrupt = false;
        if self.completed_submission.is_none() {
            return Ok(None);
        }
        self.track_tasks()?;
        Ok(self.completed_submission.take())
    }
    fn pump_one(&mut self) -> Result<bool, SessionError> {
        self.last_pump_recovered = false;
        self.deferred_observation_interrupt = false;
        self.check_output()?;
        // Tracking performs only Session getters. Partial additions stay rooted;
        // no scheduler callback has started if this observation is interrupted.
        let tracking = self.track_tasks();
        self.deferred_observation_interrupt =
            tracking.as_ref().is_err_and(|error| error.is_interrupt());
        tracking?;
        let result = self.session.run_async_turn();
        match result {
            Err(original) => {
                // Cache this turn's receipt before another turn replaces it.
                // Pre-turn observation errors cannot reuse an earlier receipt.
                if self.session.last_async_turn_recovered() {
                    if let Some(owner) = self.session.last_async_turn_recovered_owner() {
                        self.reported_trap_owners.push(owner);
                    }
                    self.last_pump_recovered = true;
                }
                Err(original)
            }
            Ok(ran) => {
                // The turn completed. These getters cannot replay source or
                // mutate scheduler ownership; completed reports remain queued.
                let observation = self.track_tasks().and_then(|()| self.observe_tasks());
                self.deferred_observation_interrupt = observation
                    .as_ref()
                    .is_err_and(|error| error.is_interrupt());
                observation?;
                Ok(ran)
            }
        }
    }
    fn observation_interrupted(&self, error: &SessionError) -> bool {
        self.deferred_observation_interrupt && error.is_interrupt()
    }
    fn async_error_disposition(&self, _: &SessionError) -> AsyncErrorDisposition {
        if self.last_pump_recovered {
            AsyncErrorDisposition::Recovered
        } else {
            AsyncErrorDisposition::Quarantine
        }
    }
    fn take_async_reports(&mut self) -> Vec<HostReport<CompiledValue, SessionError>> {
        std::mem::take(&mut self.async_reports)
    }
    fn task_counts(&mut self) -> Result<TaskCounts, SessionError> {
        self.deferred_observation_interrupt = false;
        let observation = self.session.async_task_counts();
        self.deferred_observation_interrupt = observation
            .as_ref()
            .is_err_and(|error| error.is_interrupt());
        #[cfg(test)]
        let site = if self.reset_preflight {
            "reset-counts"
        } else {
            "counts"
        };
        #[cfg(not(test))]
        let site = "counts";
        let (pending, queued, active) = self.getter_result(site, observation)?;
        Ok(TaskCounts {
            pending,
            queued,
            active,
        })
    }
    fn request_cancel_pending(&mut self) -> Result<(), SessionError> {
        self.cancelling = true;
        self.track_tasks()?;
        if self.session.async_retiring_count()? == 0 {
            self.session.request_cancel_pending_tasks()?;
        }
        Ok(())
    }
    fn poll_completions(&mut self) -> Result<(), SessionError> {
        self.deferred_observation_interrupt = false;
        self.check_output()?;
        self.drain_requests()?;
        let mut index = 0;
        while index < self.endpoints.len() {
            if self.publish_endpoint(index)? {
                self.endpoints.remove(index);
            } else {
                index += 1;
            }
        }
        Ok(())
    }
    fn retired(&mut self) -> Result<bool, SessionError> {
        self.deferred_observation_interrupt = false;
        let observation = (|| {
            self.track_tasks()?;
            self.observe_tasks()?;
            self.task_counts()
        })();
        self.deferred_observation_interrupt = observation
            .as_ref()
            .is_err_and(|error| error.is_interrupt());
        let counts = observation?;
        if !counts.retired() {
            // Do not cancel cleanup-created children while a previous owner's
            // cancellation latch remains live, even if queued work is empty.
            if self.cancelling {
                let observation = self.session.async_retiring_count();
                self.deferred_observation_interrupt = observation
                    .as_ref()
                    .is_err_and(|error| error.is_interrupt());
                if self.getter_result("retiring-count", observation)? == 0 {
                    // This is mutation, not an observation-only interruption.
                    self.deferred_observation_interrupt = false;
                    self.session.request_cancel_pending_tasks()?;
                }
            }
            return Ok(false);
        }
        if self.cancelling {
            self.deferred_observation_interrupt = false;
            self.session.request_cancel_pending_host_requests()?;
            self.poll_completions()?;
        }
        let retired = self.endpoints.is_empty()
            && self.requests.pending() == 0
            && self.session.stats().pending_host_requests == 0;
        if retired {
            self.cancelling = false;
        }
        Ok(retired)
    }
    fn reset(&mut self) -> Result<(), SessionError> {
        #[cfg(test)]
        {
            self.reset_preflight = true;
        }
        let preflight = self.retired();
        #[cfg(test)]
        {
            self.reset_preflight = false;
        }
        if !preflight? {
            return Err(SessionError::ResetPending);
        }
        self.deferred_observation_interrupt = false;
        portable_repl::reset_compiled(&mut self.session, &mut self.macros)?;
        self.display.clear();
        self.tasks.clear();
        self.reported_trap_owners.clear();
        self.last_pump_recovered = false;
        // No fallible bridge re-provisioning after Store replacement. Future
        // owner operations initialize fresh roots lazily; old roots are gone.
        self.bridge = None;
        self.cancelling = false;
        Ok(())
    }
    fn report(&mut self, report: HostReport<CompiledValue, SessionError>) {
        let result = match report {
            HostReport::Value(value) => match portable_repl::display_compiled_value(
                &mut self.session,
                &mut self.macros,
                &value,
                &mut self.display,
            ) {
                Ok(text) => writeln!(self.output, "{text}"),
                Err(error) => writeln!(
                    self.errors,
                    "Error: {}",
                    portable_repl::error_display(&mut self.session, &error)
                ),
            },
            HostReport::Error(error) => writeln!(
                self.errors,
                "Error: {}",
                portable_repl::error_display(&mut self.session, &error)
            ),
            HostReport::Interrupted => writeln!(self.output, "^C"),
            HostReport::Reset => writeln!(self.output, "nil"),
            HostReport::IncompleteEof(source) => {
                // Reader-only diagnostic; never execute an incomplete EOF buffer.
                let message = suss_reader::forms::read_forms(&source)
                    .err()
                    .map(|error| error.message)
                    .unwrap_or_else(|| "Incomplete input at EOF".into());
                writeln!(self.errors, "Error: {message}")
            }
        };
        if let Err(error) = result {
            self.output_error = Some(error);
        }
    }
    fn prompt(&self, continuation: bool) -> String {
        if continuation {
            "...=> ".into()
        } else {
            format!("{}=> ", self.session.current_namespace())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> NativeReplHost<Vec<u8>, Vec<u8>> {
        let input = File::open("/dev/null").unwrap();
        NativeReplHost::new(
            Session::new_repl().unwrap(),
            CompiledMacros::new().unwrap(),
            std::os::fd::AsFd::as_fd(&input),
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
    }

    #[test]
    fn failed_first_native_inspection_retains_the_entire_drained_batch_for_retry() {
        let mut host = host();
        drop(host.submit("(def first-read (suss.io/read-byte \"/dev/null\")) (def second-read (suss.io/read-byte \"/dev/null\"))").unwrap());
        assert_eq!(host.requests.pending(), 2);
        host.session.set_operation_fuel(1);
        let error = host.poll_completions().unwrap_err();
        assert!(
            !host.observation_interrupted(&error),
            "fuel failure is not a deferred signal"
        );
        assert_eq!(host.requests.pending(), 0);
        assert_eq!(
            host.endpoints.len(),
            2,
            "both producer requests remain owned"
        );
        assert_eq!(host.session.stats().pending_host_requests, 2);
        assert!(
            host.endpoints
                .iter()
                .all(|endpoint| endpoint.file.is_none())
        );
        host.session.set_operation_fuel(1_000_000);
        host.poll_completions().unwrap();
        assert!(host.endpoints.is_empty());
        assert_eq!(host.session.stats().pending_host_requests, 0);
        // /dev/null aliases this host's input. Both reads must receive genuine
        // language failures, not disappear or publish fabricated successful EOF.
        for source in ["first-read", "second-read"] {
            let future = host.session.eval(source).unwrap();
            assert!(matches!(
                host.session.future_state(&future).unwrap(),
                Some(FutureState::Failed(_))
            ));
        }
    }

    #[test]
    fn invalid_callable_path_is_rejected_without_executing_source_effects() {
        let mut host = host();
        drop(host.submit("(def path-effects 0) (def invalid-read (suss.io/read-byte (fn [] (set! path-effects (+ path-effects 1)) \"unused\")))").unwrap());
        host.poll_completions().unwrap();
        let effects = host.session.eval("path-effects").unwrap();
        assert_eq!(
            host.display.display(&mut host.session, &effects).unwrap(),
            "0"
        );
        let future = host.session.eval("invalid-read").unwrap();
        let Some(FutureState::Failed(payload)) = host.session.future_state(&future).unwrap() else {
            panic!("invalid source argument must reject its completion");
        };
        let ex_message = host.session.eval("suss.core/ex-message").unwrap();
        let message = host.session.invoke(&ex_message, &[&payload]).unwrap();
        assert_eq!(
            host.display.display(&mut host.session, &message).unwrap(),
            "\"suss.io/read-byte expects one string path\""
        );
    }
    #[test]
    fn failed_task_report_is_queued_once_without_writing_over_input() {
        let mut host = host();
        drop(
            host.submit("(suss.async/future* (throw (ex-info \"background\" {})))")
                .unwrap(),
        );
        assert!(host.pump_one().unwrap());
        assert!(host.output.is_empty());
        assert!(host.errors.is_empty());
        let reports = host.take_async_reports();
        assert_eq!(reports.len(), 1);
        assert!(matches!(
            &reports[0],
            HostReport::Error(SessionError::Language(_))
        ));
        assert!(host.take_async_reports().is_empty());
        assert!(!host.pump_one().unwrap());
        assert!(host.take_async_reports().is_empty());
    }
    #[test]
    fn recovered_fuel_trap_reports_original_once_and_keeps_source_prompt_usable() {
        let mut host = host();
        drop(host.submit("(def trap-effects 0) (def trap-gate (suss.internal.async/pending)) (suss.async/future* (set! trap-effects (+ trap-effects 1)) (suss.async/await* trap-gate) ((fn [] (loop [] (recur)))))").unwrap());
        // Prove the source effect committed before the trapped turn. The gate
        // separates that proof from fuel consumed by activation/publication.
        host.session.set_operation_fuel(1_000_000);
        assert!(host.pump_one().unwrap());
        assert!(!host.pump_one().unwrap());
        let effect = host.session.eval("trap-effects").unwrap();
        assert_eq!(
            host.display.display(&mut host.session, &effect).unwrap(),
            "1",
            "effect must really commit before we induce the trap"
        );
        let gate = host.session.eval("trap-gate").unwrap();
        let nil = host.scalar(&Literal::Nil).unwrap();
        assert!(host.session.resolve_future(&gate, &nil).unwrap());
        // An invoked ordinary function does not acquire cooperative future-loop
        // yielding: this is an actual fuel trap after resuming the source task.
        host.session.set_operation_fuel(1_000_000);
        let error = host.pump_one().unwrap_err();
        assert!(matches!(&error, SessionError::Trap(error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
        assert_eq!(
            host.async_error_disposition(&error),
            AsyncErrorDisposition::Recovered
        );
        host.report(HostReport::Error(error));
        host.session.set_operation_fuel(1_000_000);
        assert!(!host.pump_one().unwrap());
        assert!(
            host.take_async_reports().is_empty(),
            "consumed marker cannot duplicate original trap"
        );
        assert!(host.reported_trap_owners.is_empty());
        let Submission::Value(value) = host.submit("trap-effects").unwrap() else {
            panic!("complete input");
        };
        let text = portable_repl::display_compiled_value(
            &mut host.session,
            &mut host.macros,
            &value,
            &mut host.display,
        )
        .unwrap();
        assert_eq!(text, "1", "failed source effect is never replayed");
        assert!(!host.errors.is_empty(), "original failure was reported");
        // An unrelated ordinary source failure remains visible after recovery.
        drop(
            host.submit("(suss.async/future* (throw (ex-info \"unrelated\" {})))")
                .unwrap(),
        );
        assert!(host.pump_one().unwrap());
        let reports = host.take_async_reports();
        assert_eq!(reports.len(), 1);
        assert!(matches!(
            &reports[0],
            HostReport::Error(SessionError::Language(_))
        ));
    }
    #[test]
    fn interrupted_endpoint_getter_retains_entire_batch_before_cancellation() {
        let mut host = host();
        drop(host.submit("(def first-read (suss.io/read-byte \"/dev/null\")) (def second-read (suss.io/read-byte \"/dev/null\"))").unwrap());
        host.injected_observation_interrupt = Some(("endpoint-status", 0));
        let error = host.poll_completions().unwrap_err();
        assert!(error.is_interrupt());
        assert!(host.observation_interrupted(&error));
        assert_eq!(host.endpoints.len(), 2);
        assert_eq!(host.requests.pending(), 0);
        assert_eq!(host.session.stats().pending_host_requests, 2);
        assert!(
            host.endpoints
                .iter()
                .all(|endpoint| endpoint.file.is_none() && endpoint.payload.is_none())
        );
        for index in 0..host.endpoints.len() {
            assert_eq!(
                host.session
                    .future_status(&host.endpoints[index].request.future)
                    .unwrap(),
                Some(FutureStatus::Pending)
            );
        }
        host.report(HostReport::Interrupted);
        host.request_cancel_pending().unwrap();
        for _ in 0..64 {
            host.poll_completions().unwrap();
            host.pump_one().unwrap();
            if host.retired().unwrap() {
                break;
            }
        }
        assert!(host.retired().unwrap());
        assert!(host.endpoints.is_empty());
        assert_eq!(host.session.stats().pending_host_requests, 0);
        assert_eq!(
            String::from_utf8_lossy(&host.output).matches("^C").count(),
            1
        );
    }

    #[test]
    fn partial_interrupted_tracking_preserves_existing_and_new_roots() {
        let mut host = host();
        drop(host.submit("(def retained (suss.async/future* (suss.async/await* (suss.internal.async/pending))))").unwrap());
        assert_eq!(host.tasks.len(), 1);
        let retained = host.session.eval("retained").unwrap();
        drop(host.session.eval("(def added (suss.async/future* (suss.async/await* (suss.internal.async/pending)))) (def later (suss.async/future* (suss.async/await* (suss.internal.async/pending))))").unwrap());
        // The first new owner is appended before the next comparison fails.
        host.injected_observation_interrupt = Some(("identity", 2));
        let error = host.pump_one().unwrap_err();
        assert!(host.observation_interrupted(&error));
        assert_eq!(host.tasks.len(), 2);
        assert!(
            host.session
                .values_identical(&host.tasks[0], &retained)
                .unwrap()
        );
        assert_eq!(host.session.async_task_counts().unwrap(), (3, 3, 0));
        host.track_tasks().unwrap();
        assert_eq!(host.tasks.len(), 3);
        host.session.collect().unwrap();
        assert_eq!(
            host.session.future_status(&retained).unwrap(),
            Some(FutureStatus::Pending)
        );
    }

    struct InterruptInput {
        events: std::collections::VecDeque<super::super::terminal_input::InputEvent>,
    }
    impl super::super::session_event_loop::FrontendInput for InterruptInput {
        fn poll(
            &mut self,
            _: std::time::Duration,
        ) -> io::Result<Option<super::super::terminal_input::InputEvent>> {
            Ok(self.events.pop_front())
        }
        fn prompt(&mut self, _: &str) -> io::Result<()> {
            Ok(())
        }
        fn redraw(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn interrupted_zero_counts_keep_real_nil_root_and_report_input_once() {
        use super::super::{session_event_loop, terminal_input::InputEvent};
        let mut host = host();
        let nil = host.session.eval("nil").unwrap();
        assert_eq!(host.session.async_task_counts().unwrap(), (0, 0, 0));
        host.injected_observation_interrupt = Some(("counts", 0));
        let mut input = InterruptInput {
            events: std::collections::VecDeque::from([InputEvent::Interrupt, InputEvent::Eof]),
        };
        session_event_loop::run(&mut host, &mut input).unwrap();
        assert!(host.injected_observation_interrupt.is_none());
        assert_eq!(
            host.display.display(&mut host.session, &nil).unwrap(),
            "nil"
        );
        assert_eq!(
            String::from_utf8_lossy(&host.output).matches("^C").count(),
            1
        );
        assert!(host.errors.is_empty());
    }

    #[test]
    fn interrupted_retirement_getter_holds_cleanup_until_real_input() {
        use super::super::{session_event_loop, terminal_input::InputEvent};
        let mut host = host();
        drop(host.submit("(def cleanup-effects 0) (suss.async/future* (try (suss.async/await* (suss.internal.async/pending)) (finally (set! cleanup-effects (+ cleanup-effects 1)))))").unwrap());
        assert!(host.pump_one().unwrap());
        host.report(HostReport::Interrupted);
        host.request_cancel_pending().unwrap();
        host.injected_observation_interrupt = Some(("counts", 0));
        let error = host.retired().unwrap_err();
        assert!(host.observation_interrupted(&error));
        assert!(
            !host.tasks.is_empty(),
            "unknown retirement must retain pending cleanup roots"
        );
        assert!(host.session.async_task_counts().unwrap().0 > 0);
        // Continue through the real loop with one additional input signal; the
        // explicit first cancellation has not been mislabeled completion/reset.
        let mut input = InterruptInput {
            events: std::collections::VecDeque::from([InputEvent::Interrupt, InputEvent::Eof]),
        };
        session_event_loop::run(&mut host, &mut input).unwrap();
        let effects = host.session.eval("cleanup-effects").unwrap();
        assert_eq!(
            host.display.display(&mut host.session, &effects).unwrap(),
            "1"
        );
        assert_eq!(host.session.async_task_counts().unwrap(), (0, 0, 0));
        assert_eq!(
            String::from_utf8_lossy(&host.output).matches("^C").count(),
            2
        );
        assert!(host.errors.is_empty());
    }
    #[test]
    fn interrupted_submission_tracking_retains_result_without_replaying_effect() {
        use super::super::{session_event_loop, terminal_input::InputEvent};
        let mut host = host();
        drop(host.submit("(def once-effects 0)").unwrap());
        host.injected_observation_interrupt = Some(("submission-tracking", 0));
        let mut input = InterruptInput {
            events: std::collections::VecDeque::from([
                InputEvent::Line("(do (set! once-effects (+ once-effects 1)) 47191)".into()),
                InputEvent::Interrupt,
                InputEvent::Eof,
            ]),
        };
        session_event_loop::run(&mut host, &mut input).unwrap();
        assert!(host.completed_submission.is_none());
        assert!(host.injected_observation_interrupt.is_none());
        let effects = host.session.eval("once-effects").unwrap();
        assert_eq!(
            host.display.display(&mut host.session, &effects).unwrap(),
            "1"
        );
        let output = String::from_utf8_lossy(&host.output);
        assert_eq!(
            output.matches("47191").count(),
            1,
            "completed value reported exactly once"
        );
        assert_eq!(output.matches("^C").count(), 1);
        assert!(output.find("^C").unwrap() < output.find("47191").unwrap());
        assert!(host.errors.is_empty());
    }

    #[test]
    fn interrupted_reset_second_preflight_preserves_reset_intent() {
        use super::super::{session_event_loop, terminal_input::InputEvent};
        let mut host = host();
        let old_nil = host.session.eval("nil").unwrap();
        host.injected_observation_interrupt = Some(("reset-counts", 0));
        let mut input = InterruptInput {
            events: std::collections::VecDeque::from([
                InputEvent::Line(":reset".into()),
                InputEvent::Interrupt,
                InputEvent::Line("(+ 47200 2)".into()),
                InputEvent::Eof,
            ]),
        };
        session_event_loop::run(&mut host, &mut input).unwrap();
        assert!(host.injected_observation_interrupt.is_none());
        assert!(
            matches!(
                host.session.future_status(&old_nil),
                Err(SessionError::ForeignValue)
            ),
            "reset actually replaced the old Session"
        );
        let output = String::from_utf8_lossy(&host.output);
        assert_eq!(output.matches("^C").count(), 1);
        assert_eq!(output.matches("47202").count(), 1);
        assert!(
            host.errors.is_empty(),
            "preflight interrupt is acknowledged through input, not reported as reset failure"
        );
    }
    #[test]
    fn noninterrupt_postsubmission_tracking_failure_terminates_before_next_source() {
        use super::super::{session_event_loop, terminal_input::InputEvent};
        let mut host = host();
        drop(host.submit("(def once-effects 0)").unwrap());
        host.injected_submission_tracking_failure = true;
        let mut input = InterruptInput {
            events: std::collections::VecDeque::from([
                InputEvent::Line("(do (set! once-effects (+ once-effects 1)) 47192)".into()),
                InputEvent::Line("(set! once-effects 999)".into()),
            ]),
        };
        let error = match session_event_loop::run(&mut host, &mut input) {
            Err(session_event_loop::LoopError::Host(error)) => error,
            other => panic!("expected conservative termination, got {other:?}"),
        };
        assert!(!error.is_interrupt());
        assert!(host.submission_error_requires_quarantine(&error));
        // A stale async recovery receipt cannot authorize this submission.
        host.last_pump_recovered = true;
        assert!(host.submission_error_requires_quarantine(&error));
        assert!(
            host.completed_submission.is_some(),
            "committed result stays rooted for teardown"
        );
        assert_eq!(input.events.len(), 1, "next source is never consumed");
        let effects = host.session.eval("once-effects").unwrap();
        assert_eq!(
            host.display.display(&mut host.session, &effects).unwrap(),
            "1"
        );
        assert!(
            host.output.is_empty(),
            "no apparently successful report or continuing prompt"
        );
        assert!(
            host.errors.is_empty(),
            "return the original error to the frontend owner"
        );
    }

    #[test]
    fn ordinary_submission_compile_and_language_errors_keep_prompt_usable() {
        use super::super::{session_event_loop, terminal_input::InputEvent};
        let mut host = host();
        let mut input = InterruptInput {
            events: std::collections::VecDeque::from([
                InputEvent::Line("(unresolved-test-function)".into()),
                InputEvent::Line("(throw 88)".into()),
                InputEvent::Line("(+ 47300 3)".into()),
                InputEvent::Eof,
            ]),
        };
        session_event_loop::run(&mut host, &mut input).unwrap();
        assert!(host.completed_submission.is_none());
        assert!(!host.submission_quarantined);
        assert_eq!(
            String::from_utf8_lossy(&host.output)
                .matches("47303")
                .count(),
            1
        );
        assert!(!host.errors.is_empty());
    }
}
