use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A monotonic runtime timestamp measured in nanoseconds from a clock origin
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Creates a timestamp from nanoseconds since the clock origin
    #[must_use]
    pub const fn from_nanos(nanoseconds: u64) -> Self {
        Self(nanoseconds)
    }

    /// Returns nanoseconds since the clock origin
    #[must_use]
    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    /// Returns a timestamp advanced by `duration`, saturating at the maximum
    #[must_use]
    pub fn saturating_add(self, duration: Duration) -> Self {
        Self(self.0.saturating_add(duration_nanos(duration)))
    }
}

/// A monotonic time source used by runtime scheduling
pub trait Clock {
    /// Returns the current monotonic timestamp
    fn now(&self) -> Timestamp;
}

/// A production monotonic clock based on [`Instant`]
#[derive(Clone, Copy, Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// Creates a production clock with a new origin
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp(duration_nanos(self.origin.elapsed()))
    }
}

/// A cloneable manually advanced monotonic clock for deterministic tests
#[derive(Clone, Debug, Default)]
pub struct VirtualClock {
    nanoseconds: Arc<AtomicU64>,
}

impl VirtualClock {
    /// Creates a virtual clock at timestamp zero
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances the clock and returns its new timestamp
    pub fn advance(&self, duration: Duration) -> Timestamp {
        let delta = duration_nanos(duration);
        let mut current = self.nanoseconds.load(Ordering::Acquire);
        loop {
            let next = current.saturating_add(delta);
            match self.nanoseconds.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Timestamp(next),
                Err(observed) => current = observed,
            }
        }
    }
}

impl Clock for VirtualClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.nanoseconds.load(Ordering::Acquire))
    }
}

fn duration_nanos(duration: Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}
