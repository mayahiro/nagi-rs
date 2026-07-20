use std::time::Duration;

use nagi_vt::{Decoder, Event};

use crate::{Clock, Timestamp};

/// The application-level decision for one normalized terminal event
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventAction<Message> {
    /// Add a message to the application queue
    Message(Message),
    /// Finish the event loop normally
    Exit,
    /// Consume the event without changing application state
    Ignore,
}

/// A VT input decoder whose ambiguous lone-ESC timeout uses an injected clock
pub struct TimedInputDecoder<C: Clock> {
    decoder: Decoder,
    clock: C,
    escape_timeout: Duration,
    escape_deadline: Option<Timestamp>,
}

impl<C: Clock> TimedInputDecoder<C> {
    /// Creates a timed decoder
    #[must_use]
    pub fn new(clock: C, escape_timeout: Duration) -> Self {
        Self {
            decoder: Decoder::new(),
            clock,
            escape_timeout,
            escape_deadline: None,
        }
    }

    /// Consumes one arbitrary input byte chunk
    pub fn feed(&mut self, input: &[u8]) -> Vec<Event> {
        let events = self.decoder.feed(input);
        self.update_escape_deadline();
        events
    }

    /// Resolves a lone ESC when its deadline has elapsed
    pub fn poll(&mut self) -> Vec<Event> {
        let Some(deadline) = self.escape_deadline else {
            return Vec::new();
        };
        if self.clock.now() < deadline {
            return Vec::new();
        }
        self.escape_deadline = None;
        self.decoder.flush_pending()
    }

    /// Resolves all currently incomplete input immediately
    pub fn flush(&mut self) -> Vec<Event> {
        self.escape_deadline = None;
        self.decoder.flush_pending()
    }

    /// Reports whether incomplete input is buffered
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.decoder.has_pending()
    }

    /// Returns time until a lone-ESC deadline, if one is active
    #[must_use]
    pub fn time_until_deadline(&self) -> Option<Duration> {
        self.escape_deadline.map(|deadline| {
            Duration::from_nanos(
                deadline
                    .as_nanos()
                    .saturating_sub(self.clock.now().as_nanos()),
            )
        })
    }

    fn update_escape_deadline(&mut self) {
        self.escape_deadline = self
            .decoder
            .has_pending_escape()
            .then(|| self.clock.now().saturating_add(self.escape_timeout));
    }
}

#[cfg(test)]
mod tests {
    use nagi_vt::{Event, KeyCode};

    use crate::VirtualClock;

    use super::*;

    #[test]
    fn lone_escape_uses_virtual_deadline() {
        let clock = VirtualClock::new();
        let mut decoder = TimedInputDecoder::new(clock.clone(), Duration::from_millis(25));

        assert!(decoder.feed(b"\x1B").is_empty());
        clock.advance(Duration::from_millis(24));
        assert!(decoder.poll().is_empty());
        clock.advance(Duration::from_millis(1));

        let events = decoder.poll();
        assert!(matches!(
            events.as_slice(),
            [Event::Key(key)] if key.code == KeyCode::Escape
        ));
    }
}
