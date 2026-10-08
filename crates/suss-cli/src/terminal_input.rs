//! Single-owner native terminal input. Never share this fd with source I/O.
//! No reader thread and no language execution occur here. Raw-mode editing is
//! deliberately local; completed lines are handed to the frontend parser.
use nix::libc;
use std::{
    collections::VecDeque,
    io::{self, Write},
    os::fd::{AsRawFd, BorrowedFd},
    time::Duration,
};

#[derive(Debug, PartialEq, Eq)]
pub enum InputEvent {
    Line(String),
    Interrupt,
    Eof,
}

pub struct TerminalInput<'fd, W: Write> {
    fd: BorrowedFd<'fd>,
    wake: Option<BorrowedFd<'fd>>,
    output: W,
    original_flags: libc::c_int,
    original_termios: Option<libc::termios>,
    prompt: String,
    line: String,
    cursor: usize,
    bytes: Vec<u8>,
    escape: Vec<u8>,
    history: Vec<String>,
    history_index: Option<usize>,
    saved_line: String,
    events: VecDeque<InputEvent>,
    eof: bool,
}
impl<'fd, W: Write> TerminalInput<'fd, W> {
    /// `wake` is an optional nonblocking self-pipe read end. Its writer belongs
    /// to the signal adapter, which also interrupts active Wasm execution.
    /// The caller retains both fds and must grant this helper exclusive reads.
    pub fn new(fd: BorrowedFd<'fd>, wake: Option<BorrowedFd<'fd>>, output: W) -> io::Result<Self> {
        let raw = fd.as_raw_fd();
        // SAFETY: borrowed fds remain open for this object's lifetime.
        let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut original_termios = None;
        if unsafe { libc::isatty(raw) } == 1 {
            let mut termios = std::mem::MaybeUninit::uninit();
            if unsafe { libc::tcgetattr(raw, termios.as_mut_ptr()) } < 0 {
                return Err(io::Error::last_os_error());
            }
            let original = unsafe { termios.assume_init() };
            let mut edited = original;
            unsafe {
                libc::cfmakeraw(&mut edited);
            }
            // Keep normal output newline conversion; only input is raw.
            edited.c_oflag = original.c_oflag;
            // Keep SIGINT delivery during synchronous Wasm execution. Root's
            // signal adapter must also wake this poll through the self-pipe.
            edited.c_lflag |= original.c_lflag & libc::ISIG;
            if unsafe { libc::tcsetattr(raw, libc::TCSANOW, &edited) } < 0 {
                return Err(io::Error::last_os_error());
            }
            original_termios = Some(original);
        }
        if unsafe { libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            let error = io::Error::last_os_error();
            if let Some(original) = &original_termios {
                unsafe {
                    libc::tcsetattr(raw, libc::TCSANOW, original);
                }
            }
            return Err(error);
        }
        Ok(Self {
            fd,
            wake,
            output,
            original_flags: flags,
            original_termios,
            prompt: String::new(),
            line: String::new(),
            cursor: 0,
            bytes: Vec::new(),
            escape: Vec::new(),
            history: Vec::new(),
            history_index: None,
            saved_line: String::new(),
            events: VecDeque::new(),
            eof: false,
        })
    }
    pub fn set_prompt(&mut self, prompt: &str) -> io::Result<()> {
        self.prompt = prompt.into();
        self.render()
    }
    pub fn redraw(&mut self) -> io::Result<()> {
        self.render()
    }
    pub fn clear_display(&mut self) -> io::Result<()> {
        if self.original_termios.is_some() {
            write!(self.output, "\r\x1b[2K")?;
            self.output.flush()?;
        }
        Ok(())
    }
    pub fn poll(&mut self, timeout: Duration) -> io::Result<Option<InputEvent>> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }
        if self.eof {
            return Ok(None);
        }
        let mut descriptors = [
            libc::pollfd {
                fd: self.fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: self.wake.map_or(-1, |fd| fd.as_raw_fd()),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let millis = timeout.as_millis().min(i32::MAX as u128) as i32;
        let result = unsafe {
            libc::poll(
                descriptors.as_mut_ptr(),
                descriptors.len() as libc::nfds_t,
                millis,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted {
                Ok(None)
            } else {
                Err(error)
            };
        }
        if descriptors
            .iter()
            .any(|fd| fd.revents & libc::POLLNVAL != 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "input fd is closed",
            ));
        }
        if descriptors[1].revents & libc::POLLIN != 0 {
            let mut bytes = [0u8; 64];
            // Exactly one readiness-backed read; the self-pipe is nonblocking.
            if unsafe { libc::read(descriptors[1].fd, bytes.as_mut_ptr().cast(), bytes.len()) } < 0
            {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::WouldBlock
                    && error.kind() != io::ErrorKind::Interrupted
                {
                    return Err(error);
                }
            }
            // An externally delivered SIGINT does not perform the tty driver's
            // normal VINTR input flush. Retire unread editing bytes as well as
            // our decoded partial line before accepting subsequent input.
            if self.original_termios.is_some()
                && unsafe { libc::tcflush(self.fd.as_raw_fd(), libc::TCIFLUSH) } < 0
            {
                return Err(io::Error::last_os_error());
            }
            self.interrupt()?;
            return Ok(self.events.pop_front());
        }
        if descriptors[0].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) == 0 {
            return Ok(None);
        }
        let mut bytes = [0u8; 4096];
        let size =
            unsafe { libc::read(self.fd.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len()) };
        if size < 0 {
            let error = io::Error::last_os_error();
            return if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) {
                Ok(None)
            } else {
                Err(error)
            };
        }
        if size == 0 {
            if !self.bytes.is_empty() {
                if self.original_termios.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "EOF within UTF-8 input",
                    ));
                }
                let partial =
                    String::from_utf8(std::mem::take(&mut self.bytes)).map_err(invalid_utf8)?;
                self.events.push_back(InputEvent::Line(partial));
            }
            if !self.line.is_empty() {
                self.events
                    .push_back(InputEvent::Line(std::mem::take(&mut self.line)));
                self.cursor = 0;
            }
            self.events.push_back(InputEvent::Eof);
            self.eof = true;
        } else if self.original_termios.is_some() {
            for byte in &bytes[..size as usize] {
                self.terminal_byte(*byte)?;
            }
            self.render()?;
        } else {
            self.bytes.extend_from_slice(&bytes[..size as usize]);
            while let Some(end) = self.bytes.iter().position(|b| *b == b'\n') {
                let mut line: Vec<_> = self.bytes.drain(..=end).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                self.events.push_back(InputEvent::Line(
                    String::from_utf8(line).map_err(invalid_utf8)?,
                ));
            }
            // Partial pipe input stays separate from complete events, allowing
            // arbitrary UTF-8 splits. On EOF it becomes one final line.
        }
        Ok(self.events.pop_front())
    }
    fn interrupt(&mut self) -> io::Result<()> {
        self.line.clear();
        self.cursor = 0;
        self.bytes.clear();
        self.escape.clear();
        self.history_index = None;
        self.events.push_back(InputEvent::Interrupt);
        Ok(())
    }
    fn render(&mut self) -> io::Result<()> {
        if self.original_termios.is_some() {
            // Save the actual terminal cursor rather than approximating Unicode
            // display widths (combining marks and wide characters included).
            write!(
                self.output,
                "\r\x1b[2K{}{}\x1b[s{}\x1b[u",
                self.prompt,
                &self.line[..self.cursor],
                &self.line[self.cursor..]
            )?;
            self.output.flush()?;
        }
        Ok(())
    }
    fn terminal_byte(&mut self, byte: u8) -> io::Result<()> {
        if !self.escape.is_empty() {
            self.escape.push(byte);
            if self.escape.len() == 2 && byte != b'[' && byte != b'O' {
                self.escape.clear();
                return Ok(());
            }
            if self.escape.len() >= 3 && ((b'@'..=b'~').contains(&byte)) {
                match self.escape.as_slice() {
                    b"\x1b[A" => self.history_move(true),
                    b"\x1b[B" => self.history_move(false),
                    b"\x1b[D" => self.cursor = previous_boundary(&self.line, self.cursor),
                    b"\x1b[C" => self.cursor = next_boundary(&self.line, self.cursor),
                    b"\x1b[H" | b"\x1bOH" | b"\x1b[1~" => self.cursor = 0,
                    b"\x1b[F" | b"\x1bOF" | b"\x1b[4~" => self.cursor = self.line.len(),
                    b"\x1b[3~" => self.delete(),
                    _ => {}
                }
                self.escape.clear();
            } else if self.escape.len() > 16 {
                self.escape.clear();
            }
            return Ok(());
        }
        match byte {
            3 => return self.interrupt(),
            4 if self.line.is_empty() => {
                self.events.push_back(InputEvent::Eof);
                self.eof = true;
            }
            4 => self.delete(),
            1 => self.cursor = 0,
            5 => self.cursor = self.line.len(),
            11 => {
                self.line.truncate(self.cursor);
            }
            21 => {
                self.line.drain(..self.cursor);
                self.cursor = 0;
            }
            8 | 127 => {
                let previous = previous_boundary(&self.line, self.cursor);
                self.line.drain(previous..self.cursor);
                self.cursor = previous;
            }
            27 => self.escape.push(byte),
            b'\r' | b'\n' => {
                if !self.bytes.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "line ends within UTF-8 input",
                    ));
                }
                write!(self.output, "\r\x1b[2K{}{}\n", self.prompt, self.line)?;
                self.output.flush()?;
                if !self.line.is_empty() && self.history.last() != Some(&self.line) {
                    self.history.push(self.line.clone());
                }
                self.events
                    .push_back(InputEvent::Line(std::mem::take(&mut self.line)));
                self.cursor = 0;
                self.history_index = None;
                self.saved_line.clear();
            }
            byte if byte >= 32 || byte == b'\t' => {
                self.bytes.push(byte);
                match std::str::from_utf8(&self.bytes) {
                    Ok(text) => {
                        self.line.insert_str(self.cursor, text);
                        self.cursor += text.len();
                        self.bytes.clear();
                    }
                    Err(error) if error.error_len().is_none() => {}
                    Err(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid UTF-8 terminal input",
                        ));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn delete(&mut self) {
        let end = next_boundary(&self.line, self.cursor);
        self.line.drain(self.cursor..end);
    }
    fn history_move(&mut self, up: bool) {
        if self.history.is_empty() {
            return;
        }
        let next = if up {
            if self.history_index.is_none() {
                self.saved_line = self.line.clone();
            }
            Some(
                self.history_index
                    .unwrap_or(self.history.len())
                    .saturating_sub(1),
            )
        } else {
            self.history_index
                .and_then(|index| (index + 1 < self.history.len()).then_some(index + 1))
        };
        if !up && self.history_index.is_none() {
            return;
        }
        self.line = next.map_or_else(
            || self.saved_line.clone(),
            |index| self.history[index].clone(),
        );
        self.history_index = next;
        self.cursor = self.line.len();
    }
}
impl<W: Write> Drop for TerminalInput<'_, W> {
    fn drop(&mut self) {
        let raw = self.fd.as_raw_fd();
        unsafe {
            libc::fcntl(raw, libc::F_SETFL, self.original_flags);
            if let Some(original) = &self.original_termios {
                libc::tcsetattr(raw, libc::TCSANOW, original);
            }
        }
    }
}
fn previous_boundary(text: &str, cursor: usize) -> usize {
    text[..cursor]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}
fn next_boundary(text: &str, cursor: usize) -> usize {
    text[cursor..]
        .chars()
        .next()
        .map_or(cursor, |ch| cursor + ch.len_utf8())
}
fn invalid_utf8(error: std::string::FromUtf8Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::{AsFd, FromRawFd, OwnedFd};
    fn pipe() -> (OwnedFd, OwnedFd) {
        let mut fds = [-1; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        // SAFETY: pipe returned two uniquely owned descriptors.
        unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
    }
    fn write_fd(fd: &OwnedFd, bytes: &[u8]) {
        assert_eq!(
            unsafe { libc::write(fd.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) },
            bytes.len() as isize
        );
    }
    #[test]
    fn partial_utf8_pipe_lines_eof_and_fd_flags_are_preserved() {
        let (reader, writer) = pipe();
        let flags = unsafe { libc::fcntl(reader.as_raw_fd(), libc::F_GETFL) };
        {
            let mut input = TerminalInput::new(reader.as_fd(), None, Vec::new()).unwrap();
            write_fd(&writer, &[0xe2]);
            assert_eq!(input.poll(Duration::ZERO).unwrap(), None);
            write_fd(&writer, &[0x82, 0xac, b'\n', b'x']);
            assert_eq!(
                input.poll(Duration::ZERO).unwrap(),
                Some(InputEvent::Line("€".into()))
            );
            drop(writer);
            assert_eq!(
                input.poll(Duration::ZERO).unwrap(),
                Some(InputEvent::Line("x".into()))
            );
            assert_eq!(input.poll(Duration::ZERO).unwrap(), Some(InputEvent::Eof));
            assert_eq!(input.poll(Duration::ZERO).unwrap(), None);
        }
        assert_eq!(
            unsafe { libc::fcntl(reader.as_raw_fd(), libc::F_GETFL) },
            flags
        );
    }
    #[test]
    fn unicode_cursor_history_and_interrupt_preserve_edit_boundaries() {
        let (reader, _writer) = pipe();
        let mut input = TerminalInput::new(reader.as_fd(), None, Vec::new()).unwrap();
        for byte in "a€b".bytes().chain(b"\x1b[D\x7fZ\r".iter().copied()) {
            input.terminal_byte(byte).unwrap();
        }
        assert_eq!(
            input.events.pop_front(),
            Some(InputEvent::Line("aZb".into()))
        );
        for byte in b"\x1b[A" {
            input.terminal_byte(*byte).unwrap();
        }
        assert_eq!(input.line, "aZb");
        for byte in b"\x1b[B" {
            input.terminal_byte(*byte).unwrap();
        }
        assert!(input.line.is_empty());
        input.terminal_byte(b'x').unwrap();
        input.terminal_byte(3).unwrap();
        assert_eq!(input.events.pop_front(), Some(InputEvent::Interrupt));
        assert!(input.line.is_empty());
    }
}
