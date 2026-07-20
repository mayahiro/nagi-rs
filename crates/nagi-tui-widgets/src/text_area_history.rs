use crate::TextAreaState;

const DEFAULT_HISTORY_LIMIT: usize = 100;

/// Bounded application-owned undo and redo state for a text area
///
/// Content changes create undo steps. Cursor, selection, and viewport-only
/// changes update the current state without creating a step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextAreaHistory {
    current: TextAreaState,
    undo: Vec<TextAreaState>,
    redo: Vec<TextAreaState>,
    limit: usize,
}

impl TextAreaHistory {
    /// Creates history retaining up to 100 content changes
    #[must_use]
    pub fn new(initial: TextAreaState) -> Self {
        Self::with_limit(initial, DEFAULT_HISTORY_LIMIT)
    }

    /// Creates history retaining at most `limit` content changes
    #[must_use]
    pub const fn with_limit(initial: TextAreaState, limit: usize) -> Self {
        Self {
            current: initial,
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
        }
    }

    /// Returns the current editor state
    #[must_use]
    pub const fn current(&self) -> &TextAreaState {
        &self.current
    }

    /// Makes `next` current and records a content change as one undo step
    pub fn record(&mut self, next: TextAreaState) {
        if next == self.current {
            return;
        }
        if next.value() != self.current.value() {
            if self.limit > 0 {
                self.undo.push(self.current.clone());
                let extra = self.undo.len().saturating_sub(self.limit);
                if extra > 0 {
                    self.undo.drain(..extra);
                }
            }
            self.redo.clear();
        }
        self.current = next;
    }

    /// Restores and returns the previous content state when available
    pub fn undo(&mut self) -> Option<TextAreaState> {
        let previous = self.undo.pop()?;
        self.redo.push(self.current.clone());
        self.current = previous.clone();
        Some(previous)
    }

    /// Reapplies and returns the next content state when available
    pub fn redo(&mut self) -> Option<TextAreaState> {
        let next = self.redo.pop()?;
        if self.limit > 0 {
            self.undo.push(self.current.clone());
            let extra = self.undo.len().saturating_sub(self.limit);
            if extra > 0 {
                self.undo.drain(..extra);
            }
        }
        self.current = next.clone();
        Some(next)
    }

    /// Replaces current state and clears both history stacks
    pub fn reset(&mut self, state: TextAreaState) {
        self.current = state;
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::TextAreaHistory;
    use crate::TextAreaState;

    #[test]
    fn navigation_does_not_create_an_undo_step() {
        let mut history = TextAreaHistory::new(TextAreaState::new("ab", 0));
        history.record(TextAreaState::new("ab", 1));
        assert!(history.undo().is_none());
    }

    #[test]
    fn new_content_after_undo_discards_redo() {
        let mut history = TextAreaHistory::new(TextAreaState::new("a", 1));
        history.record(TextAreaState::new("ab", 2));
        assert_eq!(history.undo().expect("undo").value(), "a");
        history.record(TextAreaState::new("ac", 2));
        assert!(history.redo().is_none());
    }
}
