use std::error::Error;
use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
use nagi_vt::encode;
use nagi_vt::{Capabilities, MouseTracking, TerminalOp, append_encoded};

use super::system::UnixBackend;
use super::wake::WakePipe;
use crate::wake::WakeHandle;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct WaitReady {
    pub(crate) input: bool,
    pub(crate) wake: bool,
}

pub(crate) trait Backend {
    type State: Clone;
    type SignalGuard;

    fn get_state(&mut self, fd: i32) -> io::Result<Self::State>;
    fn make_raw(&mut self, state: &mut Self::State);
    fn set_state(&mut self, fd: i32, state: &Self::State) -> io::Result<()>;
    fn install_resize_handler(&mut self, wake_fd: i32) -> io::Result<Self::SignalGuard>;
    fn restore_resize_handler(&mut self, guard: Self::SignalGuard) -> io::Result<()>;
    fn resize_pending(&mut self) -> bool;
    fn read(&mut self, fd: i32, buffer: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, fd: i32, buffer: &[u8]) -> io::Result<usize>;
    fn wait(
        &mut self,
        input_fd: i32,
        wake_fd: i32,
        timeout: Option<Duration>,
    ) -> io::Result<WaitReady>;
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
    wake: Arc<WakePipe>,
    mouse_tracking: Option<MouseTracking>,
    raw_mode_active: bool,
    lifecycle_started: bool,
    output_buffer: Vec<u8>,
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

        let wake = WakePipe::new()
            .map_err(|error| TerminalError::new("create runtime wake pipe", error))?;
        let signal_guard = backend
            .install_resize_handler(wake.write_fd())
            .map_err(|error| TerminalError::new("install SIGWINCH handler", error))?;

        let mut session = Self {
            backend,
            input_fd,
            output_fd,
            original_state: Some(original_state),
            signal_guard: Some(signal_guard),
            wake,
            mouse_tracking,
            raw_mode_active: false,
            lifecycle_started: false,
            output_buffer: Vec::new(),
        };
        if let Err(error) = session.activate("enable terminal raw mode") {
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
        self.write_operations_with_extra(operations, None, capabilities)
    }

    pub(crate) fn write_operations_with_extra(
        &mut self,
        operations: &[TerminalOp],
        extra: Option<&TerminalOp>,
        capabilities: Capabilities,
    ) -> Result<()> {
        let mut output = std::mem::take(&mut self.output_buffer);
        output.clear();
        append_encoded(&mut output, operations, capabilities);
        if let Some(operation) = extra {
            append_encoded(&mut output, std::slice::from_ref(operation), capabilities);
        }
        let result = self.write_all(&output);
        self.output_buffer = output;
        result
    }

    pub(crate) fn wait(&mut self, timeout: Option<Duration>) -> Result<bool> {
        let ready = self
            .backend
            .wait(self.input_fd, self.wake.read_fd(), timeout)
            .map_err(|error| TerminalError::new("poll terminal input", error))?;
        if ready.wake {
            self.wake
                .acknowledge()
                .map_err(|error| TerminalError::new("acknowledge runtime wake-up", error))?;
        }
        Ok(ready.input)
    }

    pub(crate) fn wake_handle(&self) -> WakeHandle {
        WakeHandle::new(Arc::clone(&self.wake))
    }

    pub(crate) fn size(&mut self) -> Result<(u16, u16)> {
        self.backend
            .size(self.output_fd)
            .map_err(|error| TerminalError::new("read terminal size", error))
    }

    pub(crate) fn take_resize(&mut self) -> bool {
        self.backend.resize_pending()
    }

    pub(crate) fn suspend(&mut self) -> Result<()> {
        self.deactivate("suspend terminal mode")
    }

    pub(crate) fn resume(&mut self) -> Result<()> {
        self.activate("resume terminal raw mode")
    }

    pub(crate) fn finish(mut self) -> Result<()> {
        self.restore()
    }

    fn activate(&mut self, mode_operation: &'static str) -> Result<()> {
        if self.lifecycle_started && self.raw_mode_active {
            return Ok(());
        }
        if !self.raw_mode_active {
            let Some(original_state) = self.original_state.as_ref() else {
                return Err(TerminalError::new(
                    "resume terminal session",
                    io::Error::new(io::ErrorKind::NotConnected, "terminal session is closed"),
                ));
            };
            let mut raw_state = original_state.clone();
            self.backend.make_raw(&mut raw_state);
            self.backend
                .set_state(self.input_fd, &raw_state)
                .map_err(|error| TerminalError::new(mode_operation, error))?;
            self.raw_mode_active = true;
        }

        self.lifecycle_started = true;
        let mut operations = vec![
            TerminalOp::EnterAlternateScreen,
            TerminalOp::HideCursor,
            TerminalOp::EnableBracketedPaste,
        ];
        if let Some(tracking) = self.mouse_tracking {
            operations.push(TerminalOp::EnableMouse(tracking));
        }
        operations.push(TerminalOp::EnableFocus);
        if let Err(error) = self.write_operations(&operations, Capabilities::BASELINE) {
            let _ = self.deactivate("restore terminal mode");
            return Err(error);
        }
        Ok(())
    }

    fn deactivate(&mut self, mode_operation: &'static str) -> Result<()> {
        let mut first_error = None;

        if self.lifecycle_started {
            let operations = [
                TerminalOp::DisableMouse,
                TerminalOp::DisableFocus,
                TerminalOp::DisableBracketedPaste,
                TerminalOp::ResetStyle,
                TerminalOp::ShowCursor,
                TerminalOp::LeaveAlternateScreen,
            ];
            match self.write_operations(&operations, Capabilities::BASELINE) {
                Ok(()) => self.lifecycle_started = false,
                Err(error) => first_error = Some(error),
            }
        }

        if self.raw_mode_active {
            if let Some(original_state) = self.original_state.as_ref() {
                match self.backend.set_state(self.input_fd, original_state) {
                    Ok(()) => self.raw_mode_active = false,
                    Err(error) => {
                        first_error
                            .get_or_insert_with(|| TerminalError::new(mode_operation, error));
                    }
                }
            }
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn restore(&mut self) -> Result<()> {
        let mut first_error = self.deactivate("restore terminal mode").err();
        self.lifecycle_started = false;
        self.raw_mode_active = false;
        self.original_state.take();

        if let Some(signal_guard) = self.signal_guard.take() {
            if let Err(error) = self.backend.restore_resize_handler(signal_guard) {
                first_error
                    .get_or_insert_with(|| TerminalError::new("restore SIGWINCH handler", error));
            }
        }

        self.wake.close();

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
        fail_set: Option<usize>,
        set_count: usize,
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
            let mut fake = self.0.lock().unwrap();
            let call = fake.set_count;
            fake.set_count += 1;
            fake.calls.push(format!("set:{fd}:{state}"));
            if fake.fail_set == Some(call) {
                return Err(io::Error::other("injected set-state failure"));
            }
            Ok(())
        }

        fn install_resize_handler(&mut self, _wake_fd: i32) -> io::Result<Self::SignalGuard> {
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

        fn wait(
            &mut self,
            _input_fd: i32,
            _wake_fd: i32,
            _timeout: Option<Duration>,
        ) -> io::Result<WaitReady> {
            Ok(WaitReady {
                input: true,
                wake: false,
            })
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
        assert!(session.wait(Some(Duration::ZERO)).unwrap());
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

    #[test]
    fn suspend_and_resume_restore_configured_terminal_lifecycle() {
        let (backend, state) = fake();
        let mut session = Session::start(backend, 0, 1, Some(MouseTracking::Press)).unwrap();

        session.suspend().unwrap();
        let writes_after_suspend = state.lock().unwrap().writes.len();
        session.suspend().unwrap();
        assert_eq!(state.lock().unwrap().writes.len(), writes_after_suspend);

        session.resume().unwrap();
        let writes_after_resume = state.lock().unwrap().writes.len();
        session.resume().unwrap();
        assert_eq!(state.lock().unwrap().writes.len(), writes_after_resume);

        let expected_enter = encode(
            &[
                TerminalOp::EnterAlternateScreen,
                TerminalOp::HideCursor,
                TerminalOp::EnableBracketedPaste,
                TerminalOp::EnableMouse(MouseTracking::Press),
                TerminalOp::EnableFocus,
            ],
            Capabilities::BASELINE,
        );
        let expected_leave = encode(
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
        let state_guard = state.lock().unwrap();
        assert_eq!(
            state_guard.writes,
            [expected_enter.clone(), expected_leave, expected_enter,]
        );
        assert_eq!(state_guard.calls.last().unwrap(), "set:0:1");
        drop(state_guard);

        session.finish().unwrap();
        assert_eq!(state.lock().unwrap().calls.last().unwrap(), "signal:off");
    }

    #[test]
    fn failed_resume_output_rolls_back_to_original_mode() {
        let (backend, state) = fake();
        let mut session = Session::start(backend, 0, 1, None).unwrap();
        session.suspend().unwrap();
        state.lock().unwrap().fail_write = Some(2);

        let error = session.resume().unwrap_err();

        assert_eq!(error.operation(), "write terminal output");
        assert_eq!(state.lock().unwrap().calls.last().unwrap(), "set:0:7");
        session.finish().unwrap();
    }

    #[test]
    fn close_retries_mode_restoration_after_failed_suspend() {
        let (backend, state) = fake();
        let mut session = Session::start(backend, 0, 1, None).unwrap();
        state.lock().unwrap().fail_set = Some(1);

        let error = session.suspend().unwrap_err();
        assert_eq!(error.operation(), "suspend terminal mode");

        session.finish().unwrap();
        let state = state.lock().unwrap();
        assert_eq!(state.calls.last().unwrap(), "signal:off");
        assert!(
            state
                .calls
                .iter()
                .rev()
                .take(2)
                .any(|call| call == "set:0:7")
        );
    }

    #[test]
    fn close_retries_screen_cleanup_after_failed_suspend_write() {
        let (backend, state) = fake();
        let mut session = Session::start(backend, 0, 1, None).unwrap();
        state.lock().unwrap().fail_write = Some(1);

        assert_eq!(
            session.suspend().unwrap_err().operation(),
            "write terminal output"
        );
        session.finish().unwrap();

        let state = state.lock().unwrap();
        assert_eq!(state.writes.len(), 2);
        assert_eq!(state.calls.last().unwrap(), "signal:off");
    }

    #[test]
    fn frame_and_clipboard_extra_share_one_serialized_write() {
        let (backend, state) = fake();
        let mut session = Session::start(backend, 0, 1, None).unwrap();
        let clipboard = TerminalOp::SetClipboard("copy".to_owned());

        session
            .write_operations_with_extra(
                &[TerminalOp::WriteText("view".to_owned())],
                Some(&clipboard),
                Capabilities::BASELINE,
            )
            .unwrap();

        assert_eq!(
            state.lock().unwrap().writes[1],
            encode(
                &[TerminalOp::WriteText("view".to_owned()), clipboard],
                Capabilities::BASELINE,
            )
        );
        session.finish().unwrap();
    }
}
