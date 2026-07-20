use std::marker::PhantomData;

use nagi_tui::{Node, Style};

const LEVELS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// A compact view of unsigned samples
pub struct Sparkline<Message> {
    values: Vec<u64>,
    width: u16,
    bounds: Option<(u64, u64)>,
    style: Style,
    message: PhantomData<fn() -> Message>,
}

impl<Message> Sparkline<Message> {
    /// Creates a sparkline showing the newest samples within `width`
    #[must_use]
    pub fn new(values: impl IntoIterator<Item = u64>, width: u16) -> Self {
        Self {
            values: values.into_iter().collect(),
            width,
            bounds: None,
            style: Style::default(),
            message: PhantomData,
        }
    }

    /// Sets an explicit inclusive value range used for scaling
    #[must_use]
    pub const fn bounds(mut self, minimum: u64, maximum: u64) -> Self {
        self.bounds = Some((minimum, maximum));
        self
    }

    /// Replaces the sample style
    #[must_use]
    pub const fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this sparkline
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        Node::styled_text(
            render_sparkline(&self.values, self.width, self.bounds),
            self.style,
        )
    }
}

fn render_sparkline(values: &[u64], width: u16, bounds: Option<(u64, u64)>) -> String {
    let cells = usize::from(width);
    if cells == 0 {
        return String::new();
    }
    let (minimum, maximum) = bounds.unwrap_or_else(|| sparkline_bounds(values));
    let start = values.len().saturating_sub(cells);
    let visible = &values[start..];
    let mut output = String::with_capacity(cells.saturating_mul(3));
    output.push_str(&" ".repeat(cells.saturating_sub(visible.len())));
    for value in visible {
        output.push_str(LEVELS[scaled_level(*value, minimum, maximum, 7)]);
    }
    output
}

fn sparkline_bounds(values: &[u64]) -> (u64, u64) {
    let Some(first) = values.first().copied() else {
        return (0, 0);
    };
    values
        .iter()
        .copied()
        .fold((first, first), |(minimum, maximum), value| {
            (minimum.min(value), maximum.max(value))
        })
}

pub(crate) fn scaled_level(value: u64, minimum: u64, maximum: u64, levels: u64) -> usize {
    if maximum <= minimum || levels == 0 {
        return 0;
    }
    let value = value.clamp(minimum, maximum);
    usize::try_from(
        (u128::from(value - minimum) * u128::from(levels)) / u128::from(maximum - minimum),
    )
    .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::render_sparkline;

    #[test]
    fn rendering_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/sparkline.txt",
            "widget-sparkline",
            &["values", "width", "bounds", "expected"],
        ) else {
            return;
        };
        for record in records {
            let values = if record.field("values") == "-" {
                Vec::new()
            } else {
                record.field("values").split(',').map(number).collect()
            };
            let bounds = if record.field("bounds") == "-" {
                None
            } else {
                let values: Vec<_> = record.field("bounds").split(',').map(number).collect();
                Some((values[0], values[1]))
            };
            assert_eq!(
                render_sparkline(&values, u16_number(record.field("width")), bounds),
                record.text("expected"),
                "case {}",
                record.id
            );
        }
    }

    fn number(value: &str) -> u64 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid u64 {value}: {error}"))
    }

    fn u16_number(value: &str) -> u16 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid u16 {value}: {error}"))
    }
}
