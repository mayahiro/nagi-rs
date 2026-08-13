use std::error::Error;
use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(test)]
use nagi_vt::encode;
use nagi_vt::{Capabilities, Event, MouseTracking, TerminalOp, append_encoded, append_encoded_at};

use super::system::UnixBackend;
use super::wake::WakePipe;
use crate::terminal::TerminalViewport;
use crate::wake::WakeHandle;

const MAX_CURSOR_QUERY_INPUT_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InlineRegion {
    columns: u16,
    terminal_rows: u16,
    origin_y: u16,
    height: u16,
    cursor_offset_y: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InlinePlacement {
    origin_y: u16,
    height: u16,
    lines_after_cursor: u16,
}

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
    viewport: TerminalViewport,
    cursor_query_timeout: Duration,
    inline_region: Option<InlineRegion>,
    raw_mode_active: bool,
    lifecycle_started: bool,
    pending_input: Vec<u8>,
    pending_input_offset: usize,
    output_buffer: Vec<u8>,
}

impl Session<UnixBackend> {
    pub(crate) fn open(
        mouse_tracking: Option<MouseTracking>,
        viewport: TerminalViewport,
        cursor_query_timeout: Duration,
    ) -> Result<Self> {
        Self::start_with_viewport(
            UnixBackend,
            0,
            1,
            mouse_tracking,
            viewport,
            cursor_query_timeout,
        )
    }
}

impl<B: Backend> Session<B> {
    #[cfg(test)]
    fn start(
        backend: B,
        input_fd: i32,
        output_fd: i32,
        mouse_tracking: Option<MouseTracking>,
    ) -> Result<Self> {
        Self::start_with_viewport(
            backend,
            input_fd,
            output_fd,
            mouse_tracking,
            TerminalViewport::FULLSCREEN,
            Duration::from_millis(100),
        )
    }

    fn start_with_viewport(
        mut backend: B,
        input_fd: i32,
        output_fd: i32,
        mouse_tracking: Option<MouseTracking>,
        viewport: TerminalViewport,
        cursor_query_timeout: Duration,
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
            viewport,
            cursor_query_timeout,
            inline_region: None,
            raw_mode_active: false,
            lifecycle_started: false,
            pending_input: Vec::new(),
            pending_input_offset: 0,
            output_buffer: Vec::new(),
        };
        if let Err(error) = session.activate("enable terminal raw mode") {
            let _ = session.restore();
            return Err(error);
        }
        Ok(session)
    }

    pub(crate) fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        if self.pending_input_offset < self.pending_input.len() {
            let available = &self.pending_input[self.pending_input_offset..];
            let length = available.len().min(buffer.len());
            buffer[..length].copy_from_slice(&available[..length]);
            self.pending_input_offset += length;
            if self.pending_input_offset == self.pending_input.len() {
                self.pending_input.clear();
                self.pending_input_offset = 0;
            }
            return Ok(length);
        }
        self.read_backend(buffer)
    }

    fn read_backend(&mut self, buffer: &mut [u8]) -> Result<usize> {
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

    pub(crate) fn write_viewport_operations_with_extra(
        &mut self,
        operations: &[TerminalOp],
        cursor: Option<(u32, u32)>,
        extra: Option<&TerminalOp>,
        capabilities: Capabilities,
    ) -> Result<()> {
        let mut output = std::mem::take(&mut self.output_buffer);
        output.clear();
        let origin_y = self
            .inline_region
            .map_or(0, |region| u32::from(region.origin_y));
        append_encoded_at(&mut output, operations, capabilities, 0, origin_y);
        if self.inline_region.is_some() && cursor.is_none() {
            append_encoded_at(
                &mut output,
                &[TerminalOp::MoveTo { x: 0, y: 0 }],
                capabilities,
                0,
                origin_y,
            );
        }
        if let Some(operation) = extra {
            append_encoded(&mut output, std::slice::from_ref(operation), capabilities);
        }
        let result = self.write_all(&output);
        self.output_buffer = output;
        if result.is_ok() {
            if let Some(region) = self.inline_region.as_mut() {
                region.cursor_offset_y = cursor
                    .map_or(0, |(_, y)| u16::try_from(y).unwrap_or(u16::MAX))
                    .min(region.height.saturating_sub(1));
            }
        }
        result
    }

    pub(crate) fn wait(&mut self, timeout: Option<Duration>) -> Result<bool> {
        if self.pending_input_offset < self.pending_input.len() {
            return Ok(true);
        }
        self.wait_backend(timeout)
    }

    fn wait_backend(&mut self, timeout: Option<Duration>) -> Result<bool> {
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

    pub(crate) fn viewport_size(&mut self) -> Result<(u16, u16)> {
        match self.inline_region {
            Some(region) => Ok((region.columns, region.height)),
            None => self.size(),
        }
    }

    pub(crate) fn refresh_viewport(&mut self) -> Result<(u16, u16)> {
        let Some(height) = self.viewport.inline_height() else {
            return self.size();
        };
        let cursor_offset = self
            .inline_region
            .map_or(0, |region| region.cursor_offset_y);
        self.establish_inline_region(height, cursor_offset)?;
        let region = self.inline_region.expect("inline region was established");
        self.write_operations(
            &[
                TerminalOp::MoveTo {
                    x: 0,
                    y: u32::from(region.origin_y),
                },
                TerminalOp::EraseDisplay(nagi_vt::EraseMode::After),
            ],
            Capabilities::BASELINE,
        )?;
        Ok((region.columns, region.height))
    }

    pub(crate) fn localize_event(&self, event: Event) -> Option<Event> {
        let Some(region) = self.inline_region else {
            return Some(event);
        };
        match event {
            Event::Mouse(mut mouse) => {
                let origin_y = u32::from(region.origin_y);
                let end_y = origin_y + u32::from(region.height);
                if mouse.x >= u32::from(region.columns) || mouse.y < origin_y || mouse.y >= end_y {
                    return None;
                }
                mouse.y -= origin_y;
                Some(Event::Mouse(mouse))
            }
            other => Some(other),
        }
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
        let mut operations = Vec::with_capacity(6);
        if self.viewport.inline_height().is_none() {
            operations.push(TerminalOp::EnterAlternateScreen);
        }
        operations.extend([TerminalOp::HideCursor, TerminalOp::EnableBracketedPaste]);
        if let Some(tracking) = self.mouse_tracking {
            operations.push(TerminalOp::EnableMouse(tracking));
        }
        operations.push(TerminalOp::EnableFocus);
        if let Err(error) = self.write_operations(&operations, Capabilities::BASELINE) {
            let _ = self.deactivate("restore terminal mode");
            return Err(error);
        }
        if let Some(height) = self.viewport.inline_height() {
            if let Err(error) = self.establish_inline_region(height, 0) {
                let _ = self.deactivate("restore terminal mode");
                return Err(error);
            }
        }
        Ok(())
    }

    fn establish_inline_region(&mut self, requested_height: u16, cursor_offset: u16) -> Result<()> {
        let (columns, terminal_rows) = self.size()?;
        let (_, cursor_y) = self.query_cursor_position()?;
        let cursor_y = cursor_y.min(terminal_rows.saturating_sub(1));
        let placement = inline_placement(terminal_rows, requested_height, cursor_y, cursor_offset);

        let mut output = std::mem::take(&mut self.output_buffer);
        output.clear();
        append_encoded(
            &mut output,
            &[TerminalOp::MoveTo {
                x: 0,
                y: u32::from(cursor_y),
            }],
            Capabilities::BASELINE,
        );
        for _ in 0..placement.lines_after_cursor {
            append_encoded(&mut output, &[TerminalOp::NextLine], Capabilities::BASELINE);
        }
        let result = self.write_all(&output);
        self.output_buffer = output;
        result?;
        self.inline_region = Some(InlineRegion {
            columns,
            terminal_rows,
            origin_y: placement.origin_y,
            height: placement.height,
            cursor_offset_y: placement.height.saturating_sub(1),
        });
        Ok(())
    }

    fn query_cursor_position(&mut self) -> Result<(u16, u16)> {
        self.compact_pending_input();
        self.write_operations(&[TerminalOp::RequestCursorPosition], Capabilities::BASELINE)?;
        let started = Instant::now();
        let mut input = [0_u8; 8_192];
        loop {
            if let Some(position) = take_cursor_position_report(&mut self.pending_input) {
                return Ok(position);
            }
            let elapsed = started.elapsed();
            let Some(remaining) = self.cursor_query_timeout.checked_sub(elapsed) else {
                return Err(cursor_query_timeout_error());
            };
            if !self.wait_backend(Some(remaining))? {
                if started.elapsed() >= self.cursor_query_timeout {
                    return Err(cursor_query_timeout_error());
                }
                continue;
            }
            let read = self.read_backend(&mut input)?;
            if read == 0 {
                return Err(TerminalError::new(
                    "query terminal cursor position",
                    io::Error::from(io::ErrorKind::UnexpectedEof),
                ));
            }
            if self.pending_input.len().saturating_add(read) > MAX_CURSOR_QUERY_INPUT_BYTES {
                return Err(TerminalError::new(
                    "query terminal cursor position",
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "terminal input exceeded the cursor-query limit",
                    ),
                ));
            }
            self.pending_input.extend_from_slice(&input[..read]);
        }
    }

    fn compact_pending_input(&mut self) {
        if self.pending_input_offset == 0 {
            return;
        }
        self.pending_input.drain(..self.pending_input_offset);
        self.pending_input_offset = 0;
    }

    fn deactivate(&mut self, mode_operation: &'static str) -> Result<()> {
        let mut first_error = None;

        if self.lifecycle_started {
            let mut operations = vec![
                TerminalOp::DisableMouse,
                TerminalOp::DisableFocus,
                TerminalOp::DisableBracketedPaste,
                TerminalOp::ResetStyle,
            ];
            if self.viewport.inline_height().is_some() {
                self.append_inline_finish_operations(&mut operations);
                operations.push(TerminalOp::ShowCursor);
            } else {
                operations.extend([TerminalOp::ShowCursor, TerminalOp::LeaveAlternateScreen]);
            }
            match self.write_operations(&operations, Capabilities::BASELINE) {
                Ok(()) => {
                    self.lifecycle_started = false;
                    self.inline_region = None;
                }
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

    fn append_inline_finish_operations(&self, operations: &mut Vec<TerminalOp>) {
        let Some(region) = self.inline_region else {
            return;
        };
        let after = region.origin_y.saturating_add(region.height);
        if after < region.terminal_rows {
            operations.push(TerminalOp::MoveTo {
                x: 0,
                y: u32::from(after),
            });
        } else {
            operations.push(TerminalOp::MoveTo {
                x: 0,
                y: u32::from(region.terminal_rows.saturating_sub(1)),
            });
            operations.push(TerminalOp::NextLine);
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

fn inline_placement(
    terminal_rows: u16,
    requested_height: u16,
    cursor_y: u16,
    cursor_offset: u16,
) -> InlinePlacement {
    let height = requested_height.min(terminal_rows);
    let cursor_offset = cursor_offset.min(height.saturating_sub(1));
    let lines_after_cursor = height.saturating_sub(cursor_offset).saturating_sub(1);
    let available_lines = terminal_rows.saturating_sub(cursor_y).saturating_sub(1);
    let missing_lines = lines_after_cursor.saturating_sub(available_lines);
    InlinePlacement {
        origin_y: cursor_y
            .saturating_sub(missing_lines)
            .saturating_sub(cursor_offset),
        height,
        lines_after_cursor,
    }
}

fn cursor_query_timeout_error() -> TerminalError {
    TerminalError::new(
        "query terminal cursor position",
        io::Error::from(io::ErrorKind::TimedOut),
    )
}

fn take_cursor_position_report(input: &mut Vec<u8>) -> Option<(u16, u16)> {
    for start in 0..input.len().saturating_sub(1) {
        if input.get(start..start + 2) != Some(b"\x1B[") {
            continue;
        }
        let mut index = start + 2;
        let row_start = index;
        while input.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == row_start || input.get(index) != Some(&b';') {
            continue;
        }
        let Some(row) = parse_cursor_coordinate(&input[row_start..index]) else {
            continue;
        };
        index += 1;
        let column_start = index;
        while input.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == column_start || input.get(index) != Some(&b'R') {
            continue;
        }
        let Some(column) = parse_cursor_coordinate(&input[column_start..index]) else {
            continue;
        };
        input.drain(start..=index);
        return Some((column - 1, row - 1));
    }
    None
}

fn parse_cursor_coordinate(input: &[u8]) -> Option<u16> {
    let mut value = 0_u32;
    for byte in input {
        value = value
            .checked_mul(10)?
            .checked_add(u32::from(byte.saturating_sub(b'0')))?;
        if value > u32::from(u16::MAX) {
            return None;
        }
    }
    u16::try_from(value).ok().filter(|value| *value != 0)
}

impl<B: Backend> Drop for Session<B> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
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
        reads: VecDeque<Vec<u8>>,
        fail_wait: bool,
        size: (u16, u16),
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
            let mut state = self.0.lock().unwrap();
            let input = state.reads.pop_front().unwrap_or_else(|| b"input".to_vec());
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
            if self.0.lock().unwrap().fail_wait {
                return Err(io::Error::other("injected wait failure"));
            }
            Ok(WaitReady {
                input: true,
                wake: false,
            })
        }

        fn size(&mut self, _fd: i32) -> io::Result<(u16, u16)> {
            Ok(self.0.lock().unwrap().size)
        }
    }

    fn fake() -> (FakeBackend, Arc<Mutex<FakeState>>) {
        let state = Arc::new(Mutex::new(FakeState {
            size: (80, 24),
            ..FakeState::default()
        }));
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

    #[test]
    fn inline_placements_match_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "tui/terminal-viewport.txt",
            "terminal-viewport",
            &["terminal", "requested", "cursor", "offset", "expected"],
        ) else {
            return;
        };

        for record in records {
            let [_, terminal_rows] = fixture_pair(record.field("terminal"));
            let [_, cursor_y] = fixture_pair(record.field("cursor"));
            let requested = record.field("requested").parse().unwrap();
            let offset = record.field("offset").parse().unwrap();
            let placement = inline_placement(terminal_rows, requested, cursor_y, offset);
            assert_eq!(
                format!(
                    "origin:{};height:{};lines:{}",
                    placement.origin_y, placement.height, placement.lines_after_cursor
                ),
                record.field("expected"),
                "case {}",
                record.id
            );
        }
    }

    #[test]
    fn inline_session_preserves_input_translates_frames_and_leaves_output() {
        let (backend, state) = fake();
        state
            .lock()
            .unwrap()
            .reads
            .push_back(b"typed\x1B[23;5Rtail".to_vec());
        let viewport = TerminalViewport::inline(4).unwrap();
        let mut session = Session::start_with_viewport(
            backend,
            0,
            1,
            Some(MouseTracking::Press),
            viewport,
            Duration::from_secs(1),
        )
        .unwrap();

        assert_eq!(session.viewport_size().unwrap(), (80, 4));
        state.lock().unwrap().fail_wait = true;
        assert!(session.wait(Some(Duration::from_secs(1))).unwrap());
        let mut input = [0; 16];
        let read = session.read(&mut input).unwrap();
        assert_eq!(&input[..read], b"typedtail");

        session
            .write_viewport_operations_with_extra(
                &[
                    TerminalOp::MoveTo { x: 1, y: 2 },
                    TerminalOp::WriteText("view".to_owned()),
                ],
                Some((1, 2)),
                None,
                Capabilities::BASELINE,
            )
            .unwrap();
        assert_eq!(
            state.lock().unwrap().writes[3],
            nagi_vt::encode_at(
                &[
                    TerminalOp::MoveTo { x: 1, y: 2 },
                    TerminalOp::WriteText("view".to_owned()),
                ],
                Capabilities::BASELINE,
                0,
                20,
            )
        );

        let inside = Event::Mouse(nagi_vt::MouseEvent {
            kind: nagi_vt::MouseKind::Move,
            button: nagi_vt::MouseButton::None,
            x: 3,
            y: 22,
            modifiers: nagi_vt::Modifiers::NONE,
        });
        let localized = session.localize_event(inside).unwrap();
        assert!(matches!(localized, Event::Mouse(mouse) if mouse.y == 2));
        let outside = Event::Mouse(nagi_vt::MouseEvent {
            kind: nagi_vt::MouseKind::Move,
            button: nagi_vt::MouseButton::None,
            x: 3,
            y: 19,
            modifiers: nagi_vt::Modifiers::NONE,
        });
        assert!(session.localize_event(outside).is_none());

        session.finish().unwrap();
        let writes = &state.lock().unwrap().writes;
        assert!(!writes[0].windows(8).any(|bytes| bytes == b"\x1B[?1049"));
        assert_eq!(writes[1], b"\x1B[6n");
        assert!(
            writes
                .last()
                .unwrap()
                .ends_with(b"\x1B[24;1H\x1BE\x1B[?25h")
        );
    }

    #[test]
    fn inline_hidden_cursor_is_parked_at_viewport_origin() {
        let (backend, state) = fake();
        state.lock().unwrap().reads.push_back(b"\x1B[6;5R".to_vec());
        let mut session = Session::start_with_viewport(
            backend,
            0,
            1,
            None,
            TerminalViewport::inline(3).unwrap(),
            Duration::from_secs(1),
        )
        .unwrap();
        let operations = [
            TerminalOp::MoveTo { x: 2, y: 1 },
            TerminalOp::WriteText("view".to_owned()),
        ];

        session
            .write_viewport_operations_with_extra(&operations, None, None, Capabilities::BASELINE)
            .unwrap();

        let mut expected = nagi_vt::encode_at(&operations, Capabilities::BASELINE, 0, 5);
        nagi_vt::append_encoded_at(
            &mut expected,
            &[TerminalOp::MoveTo { x: 0, y: 0 }],
            Capabilities::BASELINE,
            0,
            5,
        );
        assert_eq!(state.lock().unwrap().writes[3], expected);
        assert_eq!(session.inline_region.unwrap().cursor_offset_y, 0);
        session.finish().unwrap();
    }

    #[test]
    fn inline_suspend_finalizes_then_resume_reserves_a_fresh_region() {
        let (backend, state) = fake();
        {
            let mut fake = state.lock().unwrap();
            fake.reads.push_back(b"\x1B[6;1R".to_vec());
            fake.reads.push_back(b"\x1B[12;1R".to_vec());
        }
        let mut session = Session::start_with_viewport(
            backend,
            0,
            1,
            None,
            TerminalViewport::inline(3).unwrap(),
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(session.inline_region.unwrap().origin_y, 5);

        session.suspend().unwrap();
        assert!(session.inline_region.is_none());
        session.resume().unwrap();
        assert_eq!(session.inline_region.unwrap().origin_y, 11);

        {
            let writes = &state.lock().unwrap().writes;
            assert!(writes[3].ends_with(b"\x1B[9;1H\x1B[?25h"));
            assert_eq!(writes[5], b"\x1B[6n");
            assert!(
                writes
                    .iter()
                    .all(|write| !write.windows(8).any(|bytes| bytes == b"\x1B[?1049"))
            );
        }
        session.finish().unwrap();
    }

    #[test]
    fn inline_cursor_query_timeout_restores_the_terminal() {
        let (backend, state) = fake();
        let error = match Session::start_with_viewport(
            backend,
            0,
            1,
            None,
            TerminalViewport::inline(3).unwrap(),
            Duration::ZERO,
        ) {
            Ok(_) => panic!("session unexpectedly started"),
            Err(error) => error,
        };

        assert_eq!(error.operation(), "query terminal cursor position");
        assert_eq!(error.io_error().kind(), io::ErrorKind::TimedOut);
        let state = state.lock().unwrap();
        assert_eq!(state.calls.last().unwrap(), "signal:off");
        assert!(state.calls.iter().any(|call| call == "set:0:7"));
    }

    #[test]
    fn inline_cursor_query_rejects_more_than_the_input_limit() {
        let (backend, state) = fake();
        state
            .lock()
            .unwrap()
            .reads
            .extend(std::iter::repeat_with(|| vec![b'x'; 8_192]).take(9));
        let error = match Session::start_with_viewport(
            backend,
            0,
            1,
            None,
            TerminalViewport::inline(3).unwrap(),
            Duration::from_secs(1),
        ) {
            Ok(_) => panic!("session unexpectedly started"),
            Err(error) => error,
        };

        assert_eq!(error.operation(), "query terminal cursor position");
        assert_eq!(error.io_error().kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("cursor-query limit"));
        assert_eq!(state.lock().unwrap().calls.last().unwrap(), "signal:off");
    }

    #[test]
    fn cursor_report_parser_skips_invalid_and_preserves_other_input() {
        let mut input = b"a\x1B[999999;1Rb\x1B[3;4Rc".to_vec();

        assert_eq!(take_cursor_position_report(&mut input), Some((3, 2)));
        assert_eq!(input, b"a\x1B[999999;1Rbc");
    }

    #[test]
    fn inline_resize_preserves_logical_cursor_offset() {
        let (backend, state) = fake();
        state.lock().unwrap().reads.push_back(b"\x1B[6;5R".to_vec());
        let mut session = Session::start_with_viewport(
            backend,
            0,
            1,
            None,
            TerminalViewport::inline(5).unwrap(),
            Duration::from_secs(1),
        )
        .unwrap();
        session
            .write_viewport_operations_with_extra(
                &[TerminalOp::MoveTo { x: 0, y: 2 }],
                Some((0, 2)),
                None,
                Capabilities::BASELINE,
            )
            .unwrap();
        {
            let mut fake = state.lock().unwrap();
            fake.size = (100, 30);
            fake.reads.push_back(b"\x1B[19;13R".to_vec());
        }

        assert_eq!(session.refresh_viewport().unwrap(), (100, 5));
        assert_eq!(session.inline_region.unwrap().origin_y, 16);
        session.finish().unwrap();
    }

    fn fixture_pair(value: &str) -> [u16; 2] {
        let mut fields = value.split(',');
        let result = [
            fields.next().unwrap().parse().unwrap(),
            fields.next().unwrap().parse().unwrap(),
        ];
        assert!(fields.next().is_none());
        result
    }
}
