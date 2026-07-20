use std::marker::PhantomData;

use nagi_text::{WidthProfile, grapheme_width, graphemes};
use nagi_tui::{Node, Style, Surface};

/// One signed integer coordinate
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChartPoint {
    /// Horizontal data coordinate
    pub x: i32,
    /// Vertical data coordinate
    pub y: i32,
}

impl ChartPoint {
    /// Creates one chart coordinate
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// One connected data series
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChartSeries {
    name: String,
    points: Vec<ChartPoint>,
    style: Style,
    marker: String,
}

impl ChartSeries {
    /// Creates a connected series with a bullet marker
    #[must_use]
    pub fn new(name: impl Into<String>, points: impl IntoIterator<Item = ChartPoint>) -> Self {
        Self {
            name: name.into(),
            points: points.into_iter().collect(),
            style: Style::default(),
            marker: "•".to_owned(),
        }
    }

    /// Replaces the line and point style
    #[must_use]
    pub const fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Replaces the one-cell point marker
    #[must_use]
    pub fn marker(mut self, marker: impl Into<String>) -> Self {
        self.marker = marker.into();
        self
    }

    /// Returns the series name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the points in connection order
    #[must_use]
    pub fn points(&self) -> &[ChartPoint] {
        &self.points
    }
}

/// Visual styles used by a [`Chart`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChartStyle {
    /// Style used by the left and bottom axes
    pub axis: Style,
}

impl Default for ChartStyle {
    fn default() -> Self {
        Self {
            axis: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A fixed-cell connected plot over signed integer coordinates
pub struct Chart<Message> {
    series: Vec<ChartSeries>,
    width: u16,
    height: u16,
    bounds: Option<(i32, i32, i32, i32)>,
    show_axes: bool,
    style: ChartStyle,
    message: PhantomData<fn() -> Message>,
}

impl<Message> Chart<Message> {
    /// Creates a connected plot using automatic data bounds
    #[must_use]
    pub fn new(series: impl IntoIterator<Item = ChartSeries>, width: u16, height: u16) -> Self {
        Self {
            series: series.into_iter().collect(),
            width,
            height,
            bounds: None,
            show_axes: true,
            style: ChartStyle::default(),
            message: PhantomData,
        }
    }

    /// Sets explicit inclusive data bounds
    #[must_use]
    pub const fn bounds(
        mut self,
        minimum_x: i32,
        maximum_x: i32,
        minimum_y: i32,
        maximum_y: i32,
    ) -> Self {
        self.bounds = Some((minimum_x, maximum_x, minimum_y, maximum_y));
        self
    }

    /// Sets whether the left and bottom axes reserve plot cells
    #[must_use]
    pub const fn show_axes(mut self, show: bool) -> Self {
        self.show_axes = show;
        self
    }

    /// Replaces the chart styles
    #[must_use]
    pub const fn style(mut self, style: ChartStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this chart
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let width = u32::from(self.width);
        let height = u32::from(self.height);
        let Ok(mut drawing) = Surface::new(width, height) else {
            return Node::spacer(width, height);
        };
        if width == 0 || height == 0 {
            return Node::surface(drawing);
        }
        let (left, bottom) = if self.show_axes {
            (1_i32, 1_i32)
        } else {
            (0, 0)
        };
        if self.show_axes {
            for y in 0..i32::from(self.height).saturating_sub(bottom) {
                drawing.write(0, y, "│", self.style.axis, WidthProfile::MODERN);
            }
            let axis_y = i32::from(self.height).saturating_sub(1);
            for x in 0..i32::from(self.width) {
                drawing.write(x, axis_y, "─", self.style.axis, WidthProfile::MODERN);
            }
            drawing.write(0, axis_y, "└", self.style.axis, WidthProfile::MODERN);
        }
        let plot_width = i32::from(self.width).saturating_sub(left);
        let plot_height = i32::from(self.height).saturating_sub(bottom);
        if plot_width == 0 || plot_height == 0 {
            return Node::surface(drawing);
        }
        let (minimum_x, maximum_x, minimum_y, maximum_y) = self.chart_bounds();
        for series in self.series {
            let marker = chart_marker(&series.marker);
            let mut mapped = Vec::with_capacity(series.points.len());
            for point in series.points {
                let current = CellPoint {
                    x: left + chart_scale(point.x, minimum_x, maximum_x, plot_width),
                    y: plot_height - 1 - chart_scale(point.y, minimum_y, maximum_y, plot_height),
                };
                if let Some(previous) = mapped.last().copied() {
                    draw_line(&mut drawing, previous, current, series.style);
                }
                mapped.push(current);
            }
            for point in mapped {
                drawing.write(point.x, point.y, marker, series.style, WidthProfile::MODERN);
            }
        }
        Node::surface(drawing)
    }

    fn chart_bounds(&self) -> (i32, i32, i32, i32) {
        if let Some(bounds) = self.bounds {
            return normalized_bounds(bounds.0, bounds.1, bounds.2, bounds.3);
        }
        let mut points = self.series.iter().flat_map(|series| series.points.iter());
        let Some(first) = points.next().copied() else {
            return (0, 1, 0, 1);
        };
        let (minimum_x, maximum_x, minimum_y, maximum_y) = points.fold(
            (first.x, first.x, first.y, first.y),
            |(minimum_x, maximum_x, minimum_y, maximum_y), point| {
                (
                    minimum_x.min(point.x),
                    maximum_x.max(point.x),
                    minimum_y.min(point.y),
                    maximum_y.max(point.y),
                )
            },
        );
        normalized_bounds(minimum_x, maximum_x, minimum_y, maximum_y)
    }
}

fn normalized_bounds(
    mut minimum_x: i32,
    mut maximum_x: i32,
    mut minimum_y: i32,
    mut maximum_y: i32,
) -> (i32, i32, i32, i32) {
    if maximum_x <= minimum_x {
        if minimum_x < i32::MAX {
            maximum_x = minimum_x + 1;
        } else {
            minimum_x -= 1;
        }
    }
    if maximum_y <= minimum_y {
        if minimum_y < i32::MAX {
            maximum_y = minimum_y + 1;
        } else {
            minimum_y -= 1;
        }
    }
    (minimum_x, maximum_x, minimum_y, maximum_y)
}

fn chart_scale(value: i32, minimum: i32, maximum: i32, cells: i32) -> i32 {
    if cells <= 1 || maximum <= minimum {
        return 0;
    }
    let value = value.clamp(minimum, maximum);
    let numerator = (i64::from(value) - i64::from(minimum)) * i64::from(cells - 1);
    let denominator = i64::from(maximum) - i64::from(minimum);
    i32::try_from(numerator / denominator).unwrap_or(i32::MAX)
}

fn chart_marker(marker: &str) -> &str {
    let Some(grapheme) = graphemes(marker).next() else {
        return "•";
    };
    if grapheme_width(grapheme.text(), WidthProfile::MODERN) == 1 {
        grapheme.text()
    } else {
        "•"
    }
}

#[derive(Clone, Copy)]
struct CellPoint {
    x: i32,
    y: i32,
}

fn draw_line(drawing: &mut Surface, start: CellPoint, end: CellPoint, style: Style) {
    let (mut x, mut y) = (start.x, start.y);
    let delta_x = (end.x - start.x).abs();
    let delta_y = -(end.y - start.y).abs();
    let step_x = if start.x < end.x { 1 } else { -1 };
    let step_y = if start.y < end.y { 1 } else { -1 };
    let mut error_value = delta_x + delta_y;
    loop {
        drawing.write(x, y, "·", style, WidthProfile::MODERN);
        if x == end.x && y == end.y {
            return;
        }
        let doubled = 2 * error_value;
        if doubled >= delta_y {
            error_value += delta_y;
            x += step_x;
        }
        if doubled <= delta_x {
            error_value += delta_x;
            y += step_y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::chart_scale;

    #[test]
    fn scaling_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/chart-scale.txt",
            "widget-chart-scale",
            &["value", "minimum", "maximum", "cells", "expected"],
        ) else {
            return;
        };
        for record in records {
            assert_eq!(
                chart_scale(
                    number(record.field("value")),
                    number(record.field("minimum")),
                    number(record.field("maximum")),
                    number(record.field("cells")),
                ),
                number(record.field("expected")),
                "case {}",
                record.id
            );
        }
    }

    fn number(value: &str) -> i32 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid i32 {value}: {error}"))
    }
}
