use std::marker::PhantomData;

use nagi_text::{WidthProfile, text_width};
use nagi_tui::{Node, Style};

use crate::sparkline::scaled_level;

/// One labeled unsigned value in a [`BarChart`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BarChartBar {
    label: String,
    value: u64,
    style: Style,
}

impl BarChartBar {
    /// Creates one labeled bar
    #[must_use]
    pub fn new(label: impl Into<String>, value: u64) -> Self {
        Self {
            label: label.into(),
            value,
            style: Style::default(),
        }
    }

    /// Replaces the style merged over the chart's bar style
    #[must_use]
    pub const fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Returns the label
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the unsigned magnitude
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.value
    }
}

/// Visual styles used by a [`BarChart`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarChartStyle {
    /// Style used by aligned labels
    pub label: Style,
    /// Style used by completed bar cells
    pub bar: Style,
    /// Style used by remaining bar cells
    pub empty: Style,
    /// Style used by numeric values
    pub value: Style,
}

impl Default for BarChartStyle {
    fn default() -> Self {
        Self {
            label: Style::default(),
            bar: Style {
                bold: true,
                ..Style::default()
            },
            empty: Style {
                dim: true,
                ..Style::default()
            },
            value: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A horizontal chart with aligned labels and bounded bars
pub struct BarChart<Message> {
    bars: Vec<BarChartBar>,
    width: u16,
    maximum: Option<u64>,
    show_values: bool,
    style: BarChartStyle,
    message: PhantomData<fn() -> Message>,
}

impl<Message> BarChart<Message> {
    /// Creates a horizontal chart whose bar portion occupies `width` cells
    #[must_use]
    pub fn new(bars: impl IntoIterator<Item = BarChartBar>, width: u16) -> Self {
        Self {
            bars: bars.into_iter().collect(),
            width,
            maximum: None,
            show_values: true,
            style: BarChartStyle::default(),
            message: PhantomData,
        }
    }

    /// Sets the shared scaling maximum
    ///
    /// Zero is a valid maximum and renders every bar empty.
    #[must_use]
    pub const fn maximum(mut self, maximum: u64) -> Self {
        self.maximum = Some(maximum);
        self
    }

    /// Sets whether numeric values follow each bar
    #[must_use]
    pub const fn show_values(mut self, show: bool) -> Self {
        self.show_values = show;
        self
    }

    /// Replaces the bar chart styles
    #[must_use]
    pub const fn style(mut self, style: BarChartStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this bar chart
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let maximum = self
            .maximum
            .unwrap_or_else(|| self.bars.iter().map(BarChartBar::value).max().unwrap_or(0));
        let label_width = self
            .bars
            .iter()
            .map(|bar| text_width(bar.label(), WidthProfile::MODERN))
            .max()
            .unwrap_or(0);
        let rows = self.bars.into_iter().map(|bar| {
            let filled = scaled_bar_cells(bar.value, maximum, self.width);
            let mut parts = vec![
                Node::styled_text(
                    format!(
                        "{}{}",
                        bar.label,
                        " ".repeat(
                            label_width
                                .saturating_sub(text_width(&bar.label, WidthProfile::MODERN))
                        )
                    ),
                    self.style.label,
                ),
                Node::text(" "),
                Node::styled_text("█".repeat(filled), self.style.bar.merged(bar.style)),
                Node::styled_text(
                    "░".repeat(usize::from(self.width).saturating_sub(filled)),
                    self.style.empty,
                ),
            ];
            if self.show_values {
                parts.push(Node::text(" "));
                parts.push(Node::styled_text(bar.value.to_string(), self.style.value));
            }
            Node::row(parts)
        });
        Node::column(rows)
    }
}

fn scaled_bar_cells(value: u64, maximum: u64, width: u16) -> usize {
    if maximum == 0 {
        0
    } else {
        scaled_level(value, 0, maximum, u64::from(width))
    }
}

#[cfg(test)]
mod tests {
    use super::scaled_bar_cells;

    #[test]
    fn scaling_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/bar-chart.txt",
            "widget-bar-chart",
            &["value", "maximum", "width", "filled"],
        ) else {
            return;
        };
        for record in records {
            assert_eq!(
                scaled_bar_cells(
                    u64_number(record.field("value")),
                    u64_number(record.field("maximum")),
                    u16_number(record.field("width")),
                ),
                usize_number(record.field("filled")),
                "case {}",
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
