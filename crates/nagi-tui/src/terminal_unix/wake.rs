use std::ffi::{c_int, c_void};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::wake::RuntimeWake;

const F_GETFD: c_int = 1;
const F_SETFD: c_int = 2;
const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;
const FD_CLOEXEC: c_int = 1;

#[cfg(target_os = "linux")]
const O_NONBLOCK: c_int = 0x800;
#[cfg(target_os = "macos")]
const O_NONBLOCK: c_int = 0x4;

unsafe extern "C" {
    fn close(fd: c_int) -> c_int;
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    fn pipe(descriptors: *mut c_int) -> c_int;
    fn read(fd: c_int, buffer: *mut c_void, count: usize) -> isize;
    fn write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
}

pub(crate) struct WakePipe {
    read_fd: c_int,
    write_fd: c_int,
    closed: Mutex<bool>,
    pending: AtomicBool,
}

impl WakePipe {
    pub(crate) fn new() -> io::Result<Arc<Self>> {
        let mut descriptors = [-1, -1];
        // SAFETY: descriptors provides writable storage for the two file
        // descriptors initialized by pipe on success.
        if unsafe { pipe(descriptors.as_mut_ptr()) } == -1 {
            return Err(io::Error::last_os_error());
        }
        for descriptor in descriptors {
            if let Err(error) = configure_descriptor(descriptor) {
                // SAFETY: both values were returned by the successful pipe
                // call and have not been closed yet.
                unsafe {
                    close(descriptors[0]);
                    close(descriptors[1]);
                }
                return Err(error);
            }
        }
        Ok(Arc::new(Self {
            read_fd: descriptors[0],
            write_fd: descriptors[1],
            closed: Mutex::new(false),
            pending: AtomicBool::new(false),
        }))
    }

    pub(crate) const fn read_fd(&self) -> c_int {
        self.read_fd
    }

    pub(crate) const fn write_fd(&self) -> c_int {
        self.write_fd
    }

    pub(crate) fn acknowledge(&self) -> io::Result<()> {
        let closed = self
            .closed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *closed {
            return Ok(());
        }
        let mut buffer = [0_u8; 64];
        loop {
            // SAFETY: buffer is writable for buffer.len() bytes and read_fd
            // remains open while the closed mutex guard is held.
            let result = unsafe { read(self.read_fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if result > 0 {
                continue;
            }
            if result == 0 {
                self.pending.store(false, Ordering::Release);
                return Ok(());
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => {}
                io::ErrorKind::WouldBlock => {
                    // Work notified before this boundary is observed by the
                    // runtime pass immediately after wait returns. Later work
                    // writes a new byte.
                    self.pending.store(false, Ordering::Release);
                    return Ok(());
                }
                _ => {
                    self.pending.store(false, Ordering::Release);
                    return Err(error);
                }
            }
        }
    }

    pub(crate) fn close(&self) {
        let mut closed = self
            .closed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *closed {
            return;
        }
        *closed = true;
        self.pending.store(false, Ordering::Release);
        // SAFETY: the closed mutex serializes this pair of closes with every
        // read and write performed through this WakePipe.
        unsafe {
            close(self.read_fd);
            close(self.write_fd);
        }
    }

    fn notify_inner(&self) {
        if self.pending.load(Ordering::Acquire) {
            return;
        }
        let closed = self
            .closed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *closed || self.pending.swap(true, Ordering::AcqRel) {
            return;
        }
        let byte = [1_u8];
        loop {
            // SAFETY: byte is readable for one byte and write_fd remains open
            // while the closed mutex guard is held.
            let result = unsafe { write(self.write_fd, byte.as_ptr().cast(), byte.len()) };
            if result == 1 {
                return;
            }
            if result == 0 {
                self.pending.store(false, Ordering::Release);
                return;
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => {}
                io::ErrorKind::WouldBlock => return,
                _ => {
                    self.pending.store(false, Ordering::Release);
                    return;
                }
            }
        }
    }
}

impl RuntimeWake for WakePipe {
    fn notify(&self) {
        self.notify_inner();
    }
}

impl Drop for WakePipe {
    fn drop(&mut self) {
        self.close();
    }
}

fn configure_descriptor(fd: c_int) -> io::Result<()> {
    // SAFETY: fd was returned by pipe and each fcntl command receives the
    // argument shape required by that command.
    let status = unsafe { fcntl(fd, F_GETFL) };
    if status == -1 || unsafe { fcntl(fd, F_SETFL, status | O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the same descriptor remains open and F_GETFD/F_SETFD accept an
    // integer descriptor-flag value.
    let flags = unsafe { fcntl(fd, F_GETFD) };
    if flags == -1 || unsafe { fcntl(fd, F_SETFD, flags | FD_CLOEXEC) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_are_coalesced_until_acknowledged() {
        let wake = WakePipe::new().unwrap();
        for _ in 0..1_000 {
            wake.notify_inner();
        }
        assert!(wake.pending.load(Ordering::Acquire));
        wake.acknowledge().unwrap();
        assert!(!wake.pending.load(Ordering::Acquire));

        wake.notify_inner();
        assert!(wake.pending.load(Ordering::Acquire));
    }

    #[test]
    fn late_notification_after_close_is_ignored() {
        let wake = WakePipe::new().unwrap();
        wake.close();
        wake.notify_inner();
        assert!(!wake.pending.load(Ordering::Acquire));
    }
}
