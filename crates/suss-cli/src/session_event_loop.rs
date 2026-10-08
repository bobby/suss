//! Single-owner frontend scheduling. No threads execute language or settle
//! Session handles. Source I/O readiness belongs to the host adapter, never the
//! input fd. This facade does not replace canonical WIT transport.
use super::terminal_input::{InputEvent, TerminalInput};
use std::io::Write;
use std::{collections::VecDeque, io, time::Duration};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TaskCounts {
    pub pending: usize,
    pub queued: usize,
    pub active: usize,
}
impl TaskCounts {
    pub fn retired(self) -> bool {
        self.pending == 0 && self.queued == 0 && self.active == 0
    }
}
/// Values stay rooted, typed and unflattened until the concrete host displays
/// them. `NeedMoreInput` must be decided before any submitted source effects.
pub enum Submission<R> {
    Value(R),
    NeedMoreInput,
    Empty,
}
pub enum HostReport<R, E> {
    Value(R),
    Error(E),
    Interrupted,
    Reset,
    IncompleteEof(String),
}

/// Recovery classification is supplied by the concrete Session adapter, which
/// knows whether caller state and interrupted producer ownership were restored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncErrorDisposition {
    Recovered,
    Quarantine,
}

pub trait FrontendHost {
    type RootResult;
    type Error;
    fn submit(&mut self, source: &str) -> Result<Submission<Self::RootResult>, Self::Error>;
    /// Fresh post-evaluation observation failure; never consult an earlier
    /// async-turn recovery receipt to decide whether this submission is usable.
    fn submission_error_requires_quarantine(&self, _error: &Self::Error) -> bool {
        false
    }
    /// Finish tracking an already evaluated, retained result. Never replay source.
    fn resume_completed_submission(&mut self) -> Result<Option<Self::RootResult>, Self::Error> {
        Ok(None)
    }
    fn pump_one(&mut self) -> Result<bool, Self::Error>;
    /// Classify only after pump_one has attempted its bounded runtime recovery.
    /// Recovered guarantees the next turn/input is safe: caller scope restored,
    /// failed producer not replayed, and remaining cleanup roots retained.
    /// Error type alone (including Interrupt) does not prove recovery succeeded.
    fn async_error_disposition(&self, _error: &Self::Error) -> AsyncErrorDisposition {
        AsyncErrorDisposition::Quarantine
    }
    /// True only for an interrupted pure observation at the immediately
    /// preceding pump/counts/retirement boundary. It is not scheduler recovery: input's
    /// self-pipe must acknowledge the signal before any further host operation.
    fn observation_interrupted(&self, _error: &Self::Error) -> bool {
        false
    }
    fn task_counts(&mut self) -> Result<TaskCounts, Self::Error>;
    /// Request cancellation of the captured pending-task set. Acceptance is not
    /// completion. The adapter owns later cancellation waves and at-most-once
    /// host hooks; awaited cleanup children remain protected during retirement.
    fn request_cancel_pending(&mut self) -> Result<(), Self::Error>;
    /// Poll source adapters without blocking; drain queued typed completions on
    /// this owner only, checking Session identity and operation generation.
    /// Callbacks enqueue data, never execute source or borrow this host reentrantly.
    fn poll_completions(&mut self) -> Result<(), Self::Error>;
    /// True only after task cleanup AND native operation acknowledgements retire.
    fn retired(&mut self) -> Result<bool, Self::Error>;
    /// Replace both phases only after retirement; failure retains the old host.
    fn reset(&mut self) -> Result<(), Self::Error>;
    fn report(&mut self, report: HostReport<Self::RootResult, Self::Error>);
    fn take_async_reports(&mut self) -> Vec<HostReport<Self::RootResult, Self::Error>> {
        Vec::new()
    }
    fn prompt(&self, continuation: bool) -> String;
}
pub trait FrontendInput {
    fn poll(&mut self, timeout: Duration) -> io::Result<Option<InputEvent>>;
    fn prompt(&mut self, text: &str) -> io::Result<()>;
    fn redraw(&mut self) -> io::Result<()>;
    fn before_output(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl<W: Write> FrontendInput for TerminalInput<'_, W> {
    fn poll(&mut self, timeout: Duration) -> io::Result<Option<InputEvent>> {
        TerminalInput::poll(self, timeout)
    }
    fn prompt(&mut self, text: &str) -> io::Result<()> {
        self.set_prompt(text)
    }
    fn redraw(&mut self) -> io::Result<()> {
        TerminalInput::redraw(self)
    }
    fn before_output(&mut self) -> io::Result<()> {
        self.clear_display()
    }
}
#[derive(Debug)]
pub enum LoopError<E> {
    Input(io::Error),
    Host(E),
}
#[derive(Clone, Copy)]
enum Retirement {
    Interrupt,
    Reset,
    Exit,
}

// Do not invent task counts or submit buffered source while waiting for the
// corresponding signal wake. Roots and reports remain with the concrete host.
fn await_observation_interrupt<H: FrontendHost, I: FrontendInput>(
    host: &mut H,
    input: &mut I,
    source: &mut String,
    buffered: &mut VecDeque<InputEvent>,
) -> Result<(), LoopError<H::Error>> {
    loop {
        let interrupt = if let Some(index) = buffered
            .iter()
            .position(|event| matches!(event, InputEvent::Interrupt))
        {
            buffered.remove(index);
            true
        } else {
            match input
                .poll(Duration::from_millis(10))
                .map_err(LoopError::Input)?
            {
                Some(InputEvent::Interrupt) => true,
                Some(event) => {
                    buffered.push_back(event);
                    false
                }
                None => false,
            }
        };
        if interrupt {
            source.clear();
            input.before_output().map_err(LoopError::Input)?;
            host.report(HostReport::Interrupted);
            // Mutation/teardown errors here still propagate; only the preceding
            // pure observation was deferred, never a cancellation operation.
            host.request_cancel_pending().map_err(LoopError::Host)?;
            input.redraw().map_err(LoopError::Input)?;
            return Ok(());
        }
    }
}

/// Own one frontend host for this call. A bounded ready batch gives input and
/// native completion polling a checkpoint even when source never goes idle.
/// Idle polls are capped at 10ms until the native adapter supplies a wake fd;
/// no source spin or source-I/O reads are performed by the input helper.
pub fn run<H: FrontendHost, I: FrontendInput>(
    host: &mut H,
    input: &mut I,
) -> Result<(), LoopError<H::Error>> {
    let mut source = String::new();
    let mut buffered = VecDeque::new();
    let mut retirement = None;
    input
        .prompt(&host.prompt(false))
        .map_err(LoopError::Input)?;
    'turn: loop {
        if let Err(error) = host.poll_completions() {
            if host.observation_interrupted(&error) {
                await_observation_interrupt(host, input, &mut source, &mut buffered)?;
                retirement.get_or_insert(Retirement::Interrupt);
                continue 'turn;
            }
            return Err(LoopError::Host(error));
        }
        for _ in 0..8 {
            match host.pump_one() {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    if host.observation_interrupted(&error) {
                        await_observation_interrupt(host, input, &mut source, &mut buffered)?;
                        retirement.get_or_insert(Retirement::Interrupt);
                        continue 'turn;
                    }
                    if host.async_error_disposition(&error) == AsyncErrorDisposition::Quarantine {
                        // Preserve the original error and retain the host; do
                        // not submit source or replay a possibly dirty Store.
                        return Err(LoopError::Host(error));
                    }
                    input.before_output().map_err(LoopError::Input)?;
                    host.report(HostReport::Error(error));
                    input
                        .prompt(&host.prompt(!source.is_empty()))
                        .map_err(LoopError::Input)?;
                    input.redraw().map_err(LoopError::Input)?;
                    // Leave this batch. The next turn may retire cleanup but
                    // must never retry the failed source callback.
                    break;
                }
            }
        }
        let reports = host.take_async_reports();
        if !reports.is_empty() {
            input.before_output().map_err(LoopError::Input)?;
            for report in reports {
                host.report(report);
            }
            input.redraw().map_err(LoopError::Input)?;
        }
        if let Some(action) = retirement {
            let retired = match host.retired() {
                Ok(retired) => retired,
                Err(error) if host.observation_interrupted(&error) => {
                    await_observation_interrupt(host, input, &mut source, &mut buffered)?;
                    // Keep the existing reset/exit intent while acknowledging
                    // another signal during its pure retirement observation.
                    continue 'turn;
                }
                Err(error) => return Err(LoopError::Host(error)),
            };
            if retired {
                match host.resume_completed_submission() {
                    Ok(Some(value)) => {
                        input.before_output().map_err(LoopError::Input)?;
                        host.report(HostReport::Value(value));
                    }
                    Ok(None) => {}
                    Err(error) if host.observation_interrupted(&error) => {
                        await_observation_interrupt(host, input, &mut source, &mut buffered)?;
                        continue 'turn;
                    }
                    Err(error) => return Err(LoopError::Host(error)),
                }
                match action {
                    Retirement::Exit => return Ok(()),
                    Retirement::Reset => {
                        input.before_output().map_err(LoopError::Input)?;
                        match host.reset() {
                            Ok(()) => host.report(HostReport::Reset),
                            Err(error) if host.observation_interrupted(&error) => {
                                await_observation_interrupt(
                                    host,
                                    input,
                                    &mut source,
                                    &mut buffered,
                                )?;
                                continue 'turn;
                            }
                            Err(error) => host.report(HostReport::Error(error)),
                        }
                    }
                    Retirement::Interrupt => {}
                }
                retirement = None;
                input
                    .prompt(&host.prompt(false))
                    .map_err(LoopError::Input)?;
            }
        }
        let counts = match host.task_counts() {
            Ok(counts) => counts,
            Err(error) if host.observation_interrupted(&error) => {
                await_observation_interrupt(host, input, &mut source, &mut buffered)?;
                retirement.get_or_insert(Retirement::Interrupt);
                continue 'turn;
            }
            Err(error) => return Err(LoopError::Host(error)),
        };
        let timeout = if counts.queued > 0
            || counts.active > 0
            || (!buffered.is_empty() && retirement.is_none())
        {
            Duration::ZERO
        } else {
            Duration::from_millis(10)
        };
        let incoming = input.poll(timeout).map_err(LoopError::Input)?;
        // Never evaluate new source while a reset/exit/cancellation retirement
        // owns cleanup. Keep input events for afterwards, rather than dropping
        // lines or replacing the old Store while callbacks can still arrive.
        if let Some(event) = incoming {
            buffered.push_back(event);
        }
        if retirement.is_some() {
            continue;
        }
        let Some(event) = buffered.pop_front() else {
            continue;
        };
        match event {
            InputEvent::Interrupt => {
                source.clear();
                input.before_output().map_err(LoopError::Input)?;
                host.report(HostReport::Interrupted);
                host.request_cancel_pending().map_err(LoopError::Host)?;
                retirement = Some(Retirement::Interrupt);
            }
            InputEvent::Eof => {
                if !source.is_empty() {
                    input.before_output().map_err(LoopError::Input)?;
                    host.report(HostReport::IncompleteEof(std::mem::take(&mut source)));
                }
                host.request_cancel_pending().map_err(LoopError::Host)?;
                retirement = Some(Retirement::Exit);
            }
            InputEvent::Line(line) => {
                if source.is_empty() && matches!(line.trim(), ":quit" | ":reset") {
                    host.request_cancel_pending().map_err(LoopError::Host)?;
                    retirement = Some(if line.trim() == ":quit" {
                        Retirement::Exit
                    } else {
                        Retirement::Reset
                    });
                } else {
                    source.push_str(&line);
                    source.push('\n');
                    match host.submit(&source) {
                        Ok(Submission::NeedMoreInput) => {}
                        Ok(Submission::Empty) => source.clear(),
                        Ok(Submission::Value(value)) => {
                            source.clear();
                            input.before_output().map_err(LoopError::Input)?;
                            host.report(HostReport::Value(value));
                        }
                        Err(error) if host.observation_interrupted(&error) => {
                            await_observation_interrupt(host, input, &mut source, &mut buffered)?;
                            retirement = Some(Retirement::Interrupt);
                            continue 'turn;
                        }
                        Err(error) if host.submission_error_requires_quarantine(&error) => {
                            return Err(LoopError::Host(error));
                        }
                        Err(error) => {
                            source.clear();
                            input.before_output().map_err(LoopError::Input)?;
                            host.report(HostReport::Error(error));
                        }
                    }
                    input
                        .prompt(&host.prompt(!source.is_empty()))
                        .map_err(LoopError::Input)?;
                }
            }
        }
        input.redraw().map_err(LoopError::Input)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Host {
        pending: usize,
        cleanup: usize,
        cancels: usize,
        resets: usize,
        values: Vec<String>,
        interrupts: usize,
        pump_error: Option<&'static str>,
        recoverable: bool,
        errors: Vec<&'static str>,
        counts_error: Option<&'static str>,
        retired_error: Option<&'static str>,
        deferred_observation: bool,
        observation_waiting: bool,
        cancel_error: Option<&'static str>,
        require_retirement: bool,
    }
    impl FrontendHost for Host {
        type RootResult = String;
        type Error = &'static str;
        fn submit(&mut self, source: &str) -> Result<Submission<String>, Self::Error> {
            assert!(
                !self.observation_waiting,
                "no source before input acknowledges interrupt"
            );
            if self.require_retirement {
                assert_eq!(self.pending, 0, "cleanup before new source");
            }
            if !source.trim_end().ends_with(')') {
                return Ok(Submission::NeedMoreInput);
            }
            self.pending = 1;
            Ok(Submission::Value(source.into()))
        }
        fn pump_one(&mut self) -> Result<bool, Self::Error> {
            assert!(
                !self.observation_waiting,
                "no host pump while awaiting self-pipe"
            );
            if let Some(error) = self.pump_error.take() {
                self.observation_waiting = self.observation_interrupted(&error);
                return Err(error);
            }
            if self.cleanup > 0 {
                self.cleanup -= 1;
                if self.cleanup == 0 {
                    self.pending = 0;
                }
                return Ok(true);
            }
            Ok(false)
        }
        fn async_error_disposition(&self, _: &Self::Error) -> AsyncErrorDisposition {
            if self.recoverable {
                AsyncErrorDisposition::Recovered
            } else {
                AsyncErrorDisposition::Quarantine
            }
        }
        fn observation_interrupted(&self, error: &Self::Error) -> bool {
            self.deferred_observation && *error == "observation interrupted"
        }
        fn task_counts(&mut self) -> Result<TaskCounts, Self::Error> {
            assert!(
                !self.observation_waiting,
                "unknown counts cannot be fabricated or retried"
            );
            if let Some(error) = self.counts_error.take() {
                self.observation_waiting = self.observation_interrupted(&error);
                return Err(error);
            }
            Ok(TaskCounts {
                pending: self.pending,
                queued: self.cleanup,
                active: 0,
            })
        }
        fn request_cancel_pending(&mut self) -> Result<(), Self::Error> {
            if self.observation_waiting {
                assert_eq!(
                    self.interrupts, 1,
                    "actual input interrupt starts cancellation"
                );
                self.observation_waiting = false;
            }
            if let Some(error) = self.cancel_error.take() {
                return Err(error);
            }
            self.cancels += 1;
            if self.pending > 0 {
                self.cleanup = 2;
            }
            Ok(())
        }
        fn poll_completions(&mut self) -> Result<(), Self::Error> {
            assert!(
                !self.observation_waiting,
                "no producer publication while awaiting input"
            );
            Ok(())
        }
        fn retired(&mut self) -> Result<bool, Self::Error> {
            if let Some(error) = self.retired_error.take() {
                self.observation_waiting = self.observation_interrupted(&error);
                return Err(error);
            }
            Ok(self.pending == 0 && self.cleanup == 0)
        }
        fn reset(&mut self) -> Result<(), Self::Error> {
            assert_eq!(self.pending, 0);
            self.resets += 1;
            Ok(())
        }
        fn report(&mut self, report: HostReport<String, Self::Error>) {
            match report {
                HostReport::Value(value) => self.values.push(value),
                HostReport::Interrupted => self.interrupts += 1,
                HostReport::Error(error) => self.errors.push(error),
                _ => {}
            }
        }
        fn prompt(&self, continuation: bool) -> String {
            if continuation { "..." } else { ">" }.into()
        }
    }
    struct Input {
        events: VecDeque<InputEvent>,
        prompts: Vec<String>,
    }
    impl FrontendInput for Input {
        fn poll(&mut self, _: Duration) -> io::Result<Option<InputEvent>> {
            Ok(self.events.pop_front())
        }
        fn prompt(&mut self, text: &str) -> io::Result<()> {
            self.prompts.push(text.into());
            Ok(())
        }
        fn redraw(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn multiline_interrupt_cleanup_reset_and_eof_share_one_owner() {
        let mut host = Host::default();
        let mut input = Input {
            events: VecDeque::from([
                InputEvent::Line("(a".into()),
                InputEvent::Line(" b)".into()),
                InputEvent::Interrupt,
                InputEvent::Line(":reset".into()),
                InputEvent::Eof,
            ]),
            prompts: Vec::new(),
        };
        run(&mut host, &mut input).unwrap();
        assert_eq!(host.values, ["(a\n b)\n"]);
        assert!(input.prompts.iter().any(|prompt| prompt == "..."));
        assert_eq!(host.interrupts, 1);
        assert_eq!(host.cancels, 3);
        assert_eq!(host.resets, 1);
        assert_eq!(host.pending, 0);
        assert_eq!(host.cleanup, 0);
    }
    #[test]
    fn recovered_async_error_reports_original_and_keeps_prompt_usable() {
        let mut host = Host {
            pump_error: Some("original fuel trap"),
            recoverable: true,
            ..Host::default()
        };
        let mut input = Input {
            events: VecDeque::from([InputEvent::Line("(ok)".into()), InputEvent::Eof]),
            prompts: Vec::new(),
        };
        run(&mut host, &mut input).unwrap();
        assert_eq!(host.errors, ["original fuel trap"]);
        assert_eq!(host.values, ["(ok)\n"]);
        assert!(input.prompts.len() >= 3);
        assert_eq!(host.pending, 0);
    }
    #[test]
    fn unrecovered_async_error_quarantines_before_next_input() {
        let mut host = Host {
            pump_error: Some("original interrupted trap"),
            ..Host::default()
        };
        let mut input = Input {
            events: VecDeque::from([InputEvent::Line("(must-not-run)".into())]),
            prompts: Vec::new(),
        };
        assert!(matches!(
            run(&mut host, &mut input),
            Err(LoopError::Host("original interrupted trap"))
        ));
        assert!(host.values.is_empty());
        assert!(host.errors.is_empty());
        assert_eq!(input.events.len(), 1);
        assert_eq!(host.cancels, 0);
    }
    #[test]
    fn observation_interrupt_waits_for_input_and_cleanup_before_buffered_source() {
        for during_pump in [true, false] {
            let mut host = Host {
                pending: 1,
                deferred_observation: true,
                require_retirement: true,
                pump_error: during_pump.then_some("observation interrupted"),
                counts_error: (!during_pump).then_some("observation interrupted"),
                ..Host::default()
            };
            let mut input = Input {
                // A complete new form before the signal wake must remain held:
                // observations retain the pending owner, cancellation must drain it.
                events: VecDeque::from([
                    InputEvent::Line("(after-cleanup)".into()),
                    InputEvent::Interrupt,
                    InputEvent::Eof,
                ]),
                prompts: Vec::new(),
            };
            run(&mut host, &mut input).unwrap();
            assert_eq!(host.values, ["(after-cleanup)\n"]);
            assert_eq!(host.interrupts, 1);
            assert_eq!(host.cancels, 2);
            assert_eq!((host.pending, host.cleanup), (0, 0));
            assert!(
                host.errors.is_empty(),
                "signal is reported once through input"
            );
        }
    }

    #[test]
    fn deferred_observation_does_not_swallow_cancellation_mutation_failure() {
        let mut host = Host {
            pending: 1,
            deferred_observation: true,
            counts_error: Some("observation interrupted"),
            cancel_error: Some("observation interrupted"),
            ..Host::default()
        };
        let mut input = Input {
            events: VecDeque::from([
                InputEvent::Interrupt,
                InputEvent::Line("(must-not-run)".into()),
            ]),
            prompts: Vec::new(),
        };
        assert!(matches!(
            run(&mut host, &mut input),
            Err(LoopError::Host("observation interrupted"))
        ));
        assert_eq!(host.pending, 1, "uncertain cancellation retains the owner");
        assert!(host.values.is_empty());
        assert_eq!(input.events.len(), 1);
    }

    #[test]
    fn count_observation_noninterrupt_failure_remains_fatal() {
        let mut host = Host {
            pending: 1,
            deferred_observation: true,
            counts_error: Some("fuel trap"),
            ..Host::default()
        };
        let mut input = Input {
            events: VecDeque::from([InputEvent::Interrupt]),
            prompts: Vec::new(),
        };
        assert!(matches!(
            run(&mut host, &mut input),
            Err(LoopError::Host("fuel trap"))
        ));
        assert_eq!(input.events.len(), 1);
        assert_eq!(host.pending, 1);
    }
    #[test]
    fn retirement_observation_interrupt_preserves_reset_intent() {
        let mut host = Host {
            pending: 1,
            deferred_observation: true,
            retired_error: Some("observation interrupted"),
            require_retirement: true,
            ..Host::default()
        };
        let mut input = Input {
            events: VecDeque::from([
                InputEvent::Line(":reset".into()),
                InputEvent::Line("(after-reset)".into()),
                InputEvent::Interrupt,
                InputEvent::Eof,
            ]),
            prompts: Vec::new(),
        };
        run(&mut host, &mut input).unwrap();
        assert_eq!(host.resets, 1);
        assert_eq!(host.interrupts, 1);
        assert_eq!(host.values, ["(after-reset)\n"]);
        assert_eq!((host.pending, host.cleanup), (0, 0));
        assert!(host.errors.is_empty());
    }
}
