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
    fn pump_one(&mut self) -> Result<bool, Self::Error>;
    /// Classify only after pump_one has attempted its bounded runtime recovery.
    /// Recovered guarantees the next turn/input is safe: caller scope restored,
    /// failed producer not replayed, and remaining cleanup roots retained.
    /// Error type alone (including Interrupt) does not prove recovery succeeded.
    fn async_error_disposition(&self, _error: &Self::Error) -> AsyncErrorDisposition {
        AsyncErrorDisposition::Quarantine
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
    loop {
        host.poll_completions().map_err(LoopError::Host)?;
        for _ in 0..8 {
            match host.pump_one() {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
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
            if host.retired().map_err(LoopError::Host)? {
                match action {
                    Retirement::Exit => return Ok(()),
                    Retirement::Reset => {
                        input.before_output().map_err(LoopError::Input)?;
                        match host.reset() {
                            Ok(()) => host.report(HostReport::Reset),
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
        let counts = host.task_counts().map_err(LoopError::Host)?;
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
    }
    impl FrontendHost for Host {
        type RootResult = String;
        type Error = &'static str;
        fn submit(&mut self, source: &str) -> Result<Submission<String>, Self::Error> {
            if !source.trim_end().ends_with(')') {
                return Ok(Submission::NeedMoreInput);
            }
            self.pending = 1;
            Ok(Submission::Value(source.into()))
        }
        fn pump_one(&mut self) -> Result<bool, Self::Error> {
            if let Some(error) = self.pump_error.take() {
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
        fn task_counts(&mut self) -> Result<TaskCounts, Self::Error> {
            Ok(TaskCounts {
                pending: self.pending,
                queued: self.cleanup,
                active: 0,
            })
        }
        fn request_cancel_pending(&mut self) -> Result<(), Self::Error> {
            self.cancels += 1;
            if self.pending > 0 {
                self.cleanup = 2;
            }
            Ok(())
        }
        fn poll_completions(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
        fn retired(&mut self) -> Result<bool, Self::Error> {
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
}
