use std::marker::PhantomData;

use nagi_tui::{Node, Style};

/// Visual styles used by a [`Progress`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgressStyle {
    /// Style used by completed cells
    pub complete: Style,
    /// Style used by remaining cells
    pub remaining: Style,
}

impl Default for ProgressStyle {
    fn default() -> Self {
        Self {
            complete: Style {
                bold: true,
                ..Style::default()
            },
            remaining: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A bounded-width determinate progress indicator
pub struct Progress<Message> {
    current: u64,
    total: u64,
    width: u16,
    style: ProgressStyle,
    message: PhantomData<fn() -> Message>,
}

impl<Message> Progress<Message> {
    /// Creates a determinate progress bar
    #[must_use]
    pub const fn new(current: u64, total: u64, width: u16) -> Self {
        Self {
            current,
            total,
            width,
            style: ProgressStyle {
                complete: Style {
                    foreground: nagi_tui::Color::Default,
                    background: nagi_tui::Color::Default,
                    underline_color: None,
                    bold: true,
                    dim: false,
                    italic: false,
                    underline: false,
                    blink: false,
                    reverse: false,
                    hidden: false,
                    strikethrough: false,
                },
                remaining: Style {
                    foreground: nagi_tui::Color::Default,
                    background: nagi_tui::Color::Default,
                    underline_color: None,
                    bold: false,
                    dim: true,
                    italic: false,
                    underline: false,
                    blink: false,
                    reverse: false,
                    hidden: false,
                    strikethrough: false,
                },
            },
            message: PhantomData,
        }
    }

    /// Replaces the progress styles
    #[must_use]
    pub const fn style(mut self, style: ProgressStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this progress indicator
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let filled = completed_cells(self.current, self.total, self.width);
        Node::row([
            Node::styled_text("█".repeat(filled), self.style.complete),
            Node::styled_text(
                "░".repeat(usize::from(self.width).saturating_sub(filled)),
                self.style.remaining,
            ),
        ])
    }
}

fn completed_cells(current: u64, total: u64, width: u16) -> usize {
    if total == 0 {
        return 0;
    }
    let current = current.min(total);
    ((u128::from(current) * u128::from(width)) / u128::from(total)) as usize
}

#[cfg(test)]
fn rendered(current: u64, total: u64, width: u16) -> String {
    let filled = completed_cells(current, total, width);
    format!(
        "{}{}",
        "█".repeat(filled),
        "░".repeat(usize::from(width).saturating_sub(filled))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/progress.txt",
            "widget-progress",
            &["current", "total", "width", "filled", "expected"],
        ) else {
            return;
        };
        for record in records {
            let current = u64_number(record.field("current"));
            let total = u64_number(record.field("total"));
            let width = u16_number(record.field("width"));
            assert_eq!(
                completed_cells(current, total, width),
                usize_number(record.field("filled")),
                "case {} count",
                record.id
            );
            assert_eq!(
                rendered(current, total, width),
                record.text("expected"),
                "case {} text",
                record.id
            );
        }
    }

    fn u64_number(value: &str) -> u64 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid u64 {value}: {error}"))
    }

    fn u16_number(value: &str) -> u16 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid u16 {value}: {error}"))
    }

    fn usize_number(value: &str) -> usize {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid usize {value}: {error}"))
    }
}
