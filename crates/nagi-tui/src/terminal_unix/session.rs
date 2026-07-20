use std::error::Error;
use std::fmt;
use std::io;
use std::time::Duration;

use nagi_vt::{Capabilities, MouseTracking, TerminalOp, encode};

use super::system::UnixBackend;

pub(crate) trait Backend {
    type State: Clone;
    type SignalGuard;

    fn get_state(&mut self, fd: i32) -> io::Result<Self::State>;
    fn make_raw(&mut self, state: &mut Self::State);
    fn set_state(&mut self, fd: i32, state: &Self::State) -> io::Result<()>;
    fn install_resize_handler(&mut self) -> io::Result<Self::SignalGuard>;
    fn restore_resize_handler(&mut self, guard: Self::SignalGuard) -> io::Result<()>;
    fn resize_pending(&mut self) -> bool;
    fn read(&mut self, fd: i32, buffer: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, fd: i32, buffer: &[u8]) -> io::Result<usize>;
    fn wait_readable(&mut self, fd: i32, timeout: Duration) -> io::Result<bool>;
    fn size(&mut self, fd: i32) -> io::Result<(u16, u16)>;
}

/// An error from the private terminal-session boundary
#[derive(Debug)]
pub(crate) struct TerminalError {
    operation: &'static str,
    source: io::Error,
}

impl TerminalError {
    fn new(operation: &'static str, source: io::Error) -> Self {
        Self { operation, source }
    }

    pub(crate) fn operation(&self) -> &'static str {
        self.operation
    }

    pub(crate) fn io_error(&self) -> &io::Error {
        &self.source
    }

    pub(crate) fn into_parts(self) -> (&'static str, io::Error) {
        (self.operation, self.source)
    }
}

impl fmt::Display for TerminalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.source)
    }
}

impl Error for TerminalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

type Result<T> = std::result::Result<T, TerminalError>;

pub(crate) type TerminalSession = Session<UnixBackend>;

pub(crate) struct Session<B: Backend> {
    backend: B,
    input_fd: i32,
    output_fd: i32,
    original_state: Option<B::State>,
    signal_guard: Option<B::SignalGuard>,
    lifecycle_started: bool,
}

impl Session<UnixBackend> {
    pub(crate) fn open(mouse_tracking: Option<MouseTracking>) -> Result<Self> {
        Self::start(UnixBackend, 0, 1, mouse_tracking)
    }
}

impl<B: Backend> Session<B> {
    fn start(
        mut backend: B,
        input_fd: i32,
        output_fd: i32,
        mouse_tracking: Option<MouseTracking>,
    ) -> Result<Self> {
        let original_state = backend
            .get_state(input_fd)
            .map_err(|error| TerminalError::new("validate terminal input", error))?;
        backend
            .get_state(output_fd)
            .map_err(|error| TerminalError::new("validate terminal output", error))?;

        let signal_guard = backend
            .install_resize_handler()
            .map_err(|error| TerminalError::new("install SIGWINCH handler", error))?;

        let mut raw_state = original_state.clone();
        backend.make_raw(&mut raw_state);
        if let Err(error) = backend.set_state(input_fd, &raw_state) {
            let _ = backend.restore_resize_handler(signal_guard);
            return Err(TerminalError::new("enable terminal raw mode", error));
        }

        let mut session = Self {
            backend,
            input_fd,
            output_fd,
            original_state: Some(original_state),
            signal_guard: Some(signal_guard),
            lifecycle_started: true,
        };
        let mut operations = vec![
            TerminalOp::EnterAlternateScreen,
            TerminalOp::HideCursor,
            TerminalOp::EnableBracketedPaste,
        ];
        if let Some(tracking) = mouse_tracking {
            operations.push(TerminalOp::EnableMouse(tracking));
        }
        operations.push(TerminalOp::EnableFocus);
        if let Err(error) = session.write_all(&encode(&operations, Capabilities::BASELINE)) {
            let _ = session.restore();
            return Err(error);
        }
        Ok(session)
    }

    pub(crate) fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        loop {
            match self.backend.read(self.input_fd, buffer) {
                Ok(read) => return Ok(read),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(TerminalError::new("read terminal input", error)),
            }
        }
    }

    pub(crate) fn write_all(&mut self, mut buffer: &[u8]) -> Result<()> {
        while !buffer.is_empty() {
            match self.backend.write(self.output_fd, buffer) {
                Ok(0) => {
                    return Err(TerminalError::new(
                        "write terminal output",
                        io::Error::from(io::ErrorKind::WriteZero),
                    ));
                }
                Ok(written) => buffer = &buffer[written..],
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(TerminalError::new("write terminal output", error)),
            }
        }
        Ok(())
    }

    pub(crate) fn write_operations(
        &mut self,
        operations: &[TerminalOp],
        capabilities: Capabilities,
    ) -> Result<()> {
        self.write_all(&encode(operations, capabilities))
    }

    pub(crate) fn wait_readable(&mut self, timeout: Duration) -> Result<bool> {
        self.backend
            .wait_readable(self.input_fd, timeout)
            .map_err(|error| TerminalError::new("poll terminal input", error))
    }

    pub(crate) fn size(&mut self) -> Result<(u16, u16)> {
        self.backend
            .size(self.output_fd)
            .map_err(|error| TerminalError::new("read terminal size", error))
    }

    pub(crate) fn take_resize(&mut self) -> bool {
        self.backend.resize_pending()
    }

    pub(crate) fn finish(mut self) -> Result<()> {
        self.restore()
    }

    fn restore(&mut self) -> Result<()> {
        let mut first_error = None;

        if self.lifecycle_started {
            self.lifecycle_started = false;
            let operations = [
                TerminalOp::DisableMouse,
                TerminalOp::DisableFocus,
                TerminalOp::DisableBracketedPaste,
                TerminalOp::ResetStyle,
                TerminalOp::ShowCursor,
                TerminalOp::LeaveAlternateScreen,
            ];
            if let Err(error) = self.write_all(&encode(&operations, Capabilities::BASELINE)) {
                first_error = Some(error);
            }
        }

        if let Some(original_state) = self.original_state.take() {
            if let Err(error) = self.backend.set_state(self.input_fd, &original_state) {
                first_error
                    .get_or_insert_with(|| TerminalError::new("restore terminal mode", error));
            }
        }

        if let Some(signal_guard) = self.signal_guard.take() {
            if let Err(error) = self.backend.restore_resize_handler(signal_guard) {
                first_error
                    .get_or_insert_with(|| TerminalError::new("restore SIGWINCH handler", error));
            }
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl<B: Backend> Drop for Session<B> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct FakeState {
        calls: Vec<String>,
        writes: Vec<Vec<u8>>,
        fail_write: Option<usize>,
        write_count: usize,
        resize: bool,
    }

    #[derive(Clone)]
    struct FakeBackend(Arc<Mutex<FakeState>>);

    impl Backend for FakeBackend {
        type SignalGuard = ();
        type State = u8;

        fn get_state(&mut self, fd: i32) -> io::Result<Self::State> {
            self.0.lock().unwrap().calls.push(format!("get:{fd}"));
            Ok(7)
        }

        fn make_raw(&mut self, state: &mut Self::State) {
            self.0.lock().unwrap().calls.push("raw".to_owned());
            *state = 1;
        }

        fn set_state(&mut self, fd: i32, state: &Self::State) -> io::Result<()> {
            self.0
                .lock()
                .unwrap()
                .calls
                .push(format!("set:{fd}:{state}"));
            Ok(())
        }

        fn install_resize_handler(&mut self) -> io::Result<Self::SignalGuard> {
            self.0.lock().unwrap().calls.push("signal:on".to_owned());
            Ok(())
        }

        fn restore_resize_handler(&mut self, (): Self::SignalGuard) -> io::Result<()> {
            self.0.lock().unwrap().calls.push("signal:off".to_owned());
            Ok(())
        }

        fn resize_pending(&mut self) -> bool {
            let mut state = self.0.lock().unwrap();
            std::mem::take(&mut state.resize)
        }

        fn read(&mut self, _fd: i32, buffer: &mut [u8]) -> io::Result<usize> {
            let input = b"input";
            let length = input.len().min(buffer.len());
            buffer[..length].copy_from_slice(&input[..length]);
            Ok(length)
        }

        fn write(&mut self, _fd: i32, buffer: &[u8]) -> io::Result<usize> {
            let mut state = self.0.lock().unwrap();
            let call = state.write_count;
            state.write_count += 1;
            if state.fail_write == Some(call) {
                return Err(io::Error::other("injected write failure"));
            }
            state.writes.push(buffer.to_vec());
            Ok(buffer.len())
        }

        fn wait_readable(&mut self, _fd: i32, _timeout: Duration) -> io::Result<bool> {
            Ok(true)
        }

        fn size(&mut self, _fd: i32) -> io::Result<(u16, u16)> {
            Ok((80, 24))
        }
    }

    fn fake() -> (FakeBackend, Arc<Mutex<FakeState>>) {
        let state = Arc::new(Mutex::new(FakeState::default()));
        (FakeBackend(Arc::clone(&state)), state)
    }

    #[test]
    fn normal_finish_restores_terminal_state() {
        let (backend, state) = fake();
        let session = Session::start(backend, 0, 1, None).unwrap();
        session.finish().unwrap();

        let state = state.lock().unwrap();
        assert_eq!(state.calls.last().unwrap(), "signal:off");
        assert!(state.calls.iter().any(|call| call == "set:0:7"));
        assert_eq!(state.writes.len(), 2);
        assert_eq!(
            state.writes[0],
            encode(
                &[
                    TerminalOp::EnterAlternateScreen,
                    TerminalOp::HideCursor,
                    TerminalOp::EnableBracketedPaste,
                    TerminalOp::EnableFocus,
                ],
                Capabilities::BASELINE,
            )
        );
    }

    #[test]
    fn error_exit_restores_terminal_state() {
        let (backend, state) = fake();
        let result: Result<()> = (|| {
            let _session = Session::start(backend, 0, 1, None)?;
            Err(TerminalError::new(
                "application",
                io::Error::other("failure"),
            ))
        })();

        assert_eq!(result.unwrap_err().operation(), "application");
        let state = state.lock().unwrap();
        assert_eq!(state.calls.last().unwrap(), "signal:off");
        assert!(state.calls.iter().any(|call| call == "set:0:7"));
    }

    #[test]
    fn panic_unwind_restores_terminal_state() {
        let (backend, state) = fake();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _session = Session::start(backend, 0, 1, None).unwrap();
            panic!("injected panic");
        }));

        assert!(result.is_err());
        let state = state.lock().unwrap();
        assert_eq!(state.calls.last().unwrap(), "signal:off");
        assert!(state.calls.iter().any(|call| call == "set:0:7"));
    }

    #[test]
    fn lifecycle_write_failure_still_restores_terminal_state() {
        let (backend, state) = fake();
        state.lock().unwrap().fail_write = Some(0);

        let error = match Session::start(backend, 0, 1, None) {
            Ok(_) => panic!("session unexpectedly started"),
            Err(error) => error,
        };

        assert_eq!(error.operation(), "write terminal output");
        let state = state.lock().unwrap();
        assert_eq!(state.calls.last().unwrap(), "signal:off");
        assert!(state.calls.iter().any(|call| call == "set:0:7"));
    }

    #[test]
    fn io_resize_and_size_are_forwarded() {
        let (backend, state) = fake();
        let mut session = Session::start(backend, 0, 1, None).unwrap();
        state.lock().unwrap().resize = true;
        let mut buffer = [0; 8];

        assert_eq!(session.read(&mut buffer).unwrap(), 5);
        assert_eq!(&buffer[..5], b"input");
        assert!(session.wait_readable(Duration::ZERO).unwrap());
        assert_eq!(session.size().unwrap(), (80, 24));
        assert!(session.take_resize());
        assert!(!session.take_resize());
    }

    #[test]
    fn configured_mouse_tracking_is_enabled_and_restored() {
        let (backend, state) = fake();
        let session = Session::start(backend, 0, 1, Some(MouseTracking::Press)).unwrap();

        let expected_start = encode(
            &[
                TerminalOp::EnterAlternateScreen,
                TerminalOp::HideCursor,
                TerminalOp::EnableBracketedPaste,
                TerminalOp::EnableMouse(MouseTracking::Press),
                TerminalOp::EnableFocus,
            ],
            Capabilities::BASELINE,
        );
        assert_eq!(state.lock().unwrap().writes, [expected_start]);

        session.finish().unwrap();
        let expected_restore = encode(
            &[
                TerminalOp::DisableMouse,
                TerminalOp::DisableFocus,
                TerminalOp::DisableBracketedPaste,
                TerminalOp::ResetStyle,
                TerminalOp::ShowCursor,
                TerminalOp::LeaveAlternateScreen,
            ],
            Capabilities::BASELINE,
        );
        assert_eq!(state.lock().unwrap().writes[1], expected_restore);
    }
}
