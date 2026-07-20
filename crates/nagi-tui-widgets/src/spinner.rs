use std::marker::PhantomData;

use nagi_tui::{Node, Style};

/// Stable spinner frames in display order
pub const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Visual style used by a [`Spinner`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpinnerStyle {
    /// Style used by the frame and optional label
    pub content: Style,
}

impl Default for SpinnerStyle {
    fn default() -> Self {
        Self {
            content: Style {
                bold: true,
                ..Style::default()
            },
        }
    }
}

/// A pure spinner view driven by an application-owned tick
pub struct Spinner<Message> {
    tick: u64,
    label: String,
    style: SpinnerStyle,
    message: PhantomData<fn() -> Message>,
}

impl<Message> Spinner<Message> {
    /// Creates an unlabeled spinner
    #[must_use]
    pub fn new(tick: u64) -> Self {
        Self {
            tick,
            label: String::new(),
            style: SpinnerStyle::default(),
            message: PhantomData,
        }
    }

    /// Sets text displayed after the spinner frame
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Replaces the spinner style
    #[must_use]
    pub const fn style(mut self, style: SpinnerStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this spinner
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        Node::styled_text(rendered(self.tick, &self.label), self.style.content)
    }
}

fn rendered(tick: u64, label: &str) -> String {
    let frame = SPINNER_FRAMES[(tick % SPINNER_FRAMES.len() as u64) as usize];
    if label.is_empty() {
        frame.to_owned()
    } else {
        format!("{frame} {label}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/spinner.txt",
            "widget-spinner",
            &["tick", "label", "expected"],
        ) else {
            return;
        };
        for record in records {
            let tick = record
                .field("tick")
                .parse()
                .unwrap_or_else(|error| panic!("invalid tick: {error}"));
            assert_eq!(
                rendered(tick, &record.text("label")),
                record.text("expected"),
                "case {}",
                record.id
            );
        }
    }
}
