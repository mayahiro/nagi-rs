use std::fs::File;
use std::io::{self, IsTerminal, Write};
use std::os::fd::{AsRawFd, RawFd};

use crate::StatusIo;

#[cfg(not(any(
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ),
    all(
        target_os = "macos",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ),
)))]
compile_error!("Nagi CLI Status process I/O supports Linux and macOS on x86_64 and aarch64");

#[cfg(target_os = "linux")]
const TIOCGWINSZ: std::ffi::c_ulong = 0x5413;
#[cfg(target_os = "macos")]
const TIOCGWINSZ: std::ffi::c_ulong = 0x4008_7468;

#[repr(C)]
struct WindowSize {
    rows: u16,
    columns: u16,
    x_pixels: u16,
    y_pixels: u16,
}

unsafe extern "C" {
    fn ioctl(descriptor: std::ffi::c_int, request: std::ffi::c_ulong, ...) -> std::ffi::c_int;
}

trait OutputFile: Write + AsRawFd + IsTerminal {}
impl<T: Write + AsRawFd + IsTerminal> OutputFile for T {}

/// Unix process status I/O backed by standard error or an owned file
pub struct ProcessIo {
    output: Box<dyn OutputFile>,
    terminal: bool,
}

impl ProcessIo {
    /// Constructs process status I/O from an owned output file
    ///
    /// This is useful when an application opens `/dev/tty` explicitly. The
    /// ordinary [`Default`] implementation uses standard error
    #[must_use]
    pub fn new(output: File) -> Self {
        Self::from_stream(output)
    }

    fn from_stream(output: impl OutputFile + 'static) -> Self {
        let terminal = output.is_terminal();
        Self {
            output: Box::new(output),
            terminal,
        }
    }
}

impl Default for ProcessIo {
    fn default() -> Self {
        Self::from_stream(io::stderr())
    }
}

impl Write for ProcessIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.output.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

impl StatusIo for ProcessIo {
    fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn terminal_width(&self) -> Option<usize> {
        if !self.is_terminal() {
            return None;
        }
        window_width(self.output.as_raw_fd()).ok().map(usize::from)
    }
}

fn window_width(descriptor: RawFd) -> io::Result<u16> {
    loop {
        let mut size = WindowSize {
            rows: 0,
            columns: 0,
            x_pixels: 0,
            y_pixels: 0,
        };
        // SAFETY: size is writable storage with the winsize layout shared by
        // Linux and macOS, and TIOCGWINSZ writes that structure
        let result = unsafe { ioctl(descriptor, TIOCGWINSZ, &mut size as *mut WindowSize) };
        if result == 0 {
            if size.columns == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "terminal reported zero columns",
                ));
            }
            return Ok(size.columns);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use std::ffi::{c_char, c_int, c_void};
    use std::os::fd::FromRawFd as _;

    use super::*;

    #[cfg_attr(target_os = "linux", link(name = "util"))]
    unsafe extern "C" {
        fn openpty(
            master: *mut c_int,
            slave: *mut c_int,
            name: *mut c_char,
            termios: *const c_void,
            window_size: *const c_void,
        ) -> c_int;
    }

    #[test]
    fn pseudo_terminal_is_detected_with_current_width() {
        let (_master, slave) = pty(93, 24);
        let io = ProcessIo::new(slave);
        assert!(io.is_terminal());
        assert_eq!(io.terminal_width(), Some(93));
    }

    #[test]
    fn ordinary_file_is_not_a_terminal() {
        let file = File::open("/dev/null").expect("/dev/null must be available");
        let io = ProcessIo::new(file);
        assert!(!io.is_terminal());
        assert_eq!(io.terminal_width(), None);
    }

    fn pty(columns: u16, rows: u16) -> (File, File) {
        let mut master = -1;
        let mut slave = -1;
        let size = WindowSize {
            rows,
            columns,
            x_pixels: 0,
            y_pixels: 0,
        };
        // SAFETY: descriptor pointers are writable, optional name and termios
        // pointers are null, size has the platform winsize layout, and
        // successful descriptors are immediately owned by File
        let result = unsafe {
            openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                (&raw const size).cast(),
            )
        };
        assert_eq!(result, 0, "openpty failed: {}", io::Error::last_os_error());
        // SAFETY: openpty returned two unique owned descriptors
        let master = unsafe { File::from_raw_fd(master) };
        // SAFETY: openpty returned two unique owned descriptors
        let slave = unsafe { File::from_raw_fd(slave) };
        (master, slave)
    }
}
