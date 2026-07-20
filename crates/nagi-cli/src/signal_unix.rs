use std::ffi::c_int;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

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
compile_error!("Nagi CLI process integration supports Linux and macOS on x86_64 and aarch64");

const SIGINT: c_int = 2;
const SIG_ERR: usize = usize::MAX;

unsafe extern "C" {
    fn signal(signal: c_int, handler: usize) -> usize;
}

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static HANDLER_OWNED: AtomicBool = AtomicBool::new(false);

extern "C" fn mark_interrupted(_signal: c_int) {
    INTERRUPTED.store(true, Ordering::Release);
}

pub(crate) fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::Acquire)
}

pub(crate) struct SignalGuard {
    previous_handler: usize,
}

impl SignalGuard {
    pub(crate) fn install() -> io::Result<Self> {
        if HANDLER_OWNED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a Nagi CLI process runner is already active",
            ));
        }
        INTERRUPTED.store(false, Ordering::Release);
        // SAFETY: mark_interrupted has the C signal-handler ABI and performs
        // one lock-free atomic store. Drop restores the paired prior handler.
        let previous_handler = unsafe { signal(SIGINT, mark_interrupted as *const () as usize) };
        if previous_handler == SIG_ERR {
            HANDLER_OWNED.store(false, Ordering::Release);
            return Err(io::Error::last_os_error());
        }
        Ok(Self { previous_handler })
    }
}

impl Drop for SignalGuard {
    fn drop(&mut self) {
        // SAFETY: previous_handler is the value returned by the successful
        // signal call owned by this guard.
        let _ = unsafe { signal(SIGINT, self.previous_handler) };
        HANDLER_OWNED.store(false, Ordering::Release);
    }
}
