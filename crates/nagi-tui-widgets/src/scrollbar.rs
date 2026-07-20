use nagi_tui::{Node, Style};

/// Direction in which a [`Scrollbar`] is rendered
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScrollbarOrientation {
    /// Render one cell per row
    #[default]
    Vertical,
    /// Render all cells in one row
    Horizontal,
}

/// Visual styles used by a [`Scrollbar`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScrollbarStyle {
    /// Style used by the unoccupied track
    pub track: Style,
    /// Style used by the viewport thumb
    pub thumb: Style,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self {
            track: Style {
                dim: true,
                ..Style::default()
            },
            thumb: Style::default(),
        }
    }
}

/// A pure view of an application-owned scroll position
pub struct Scrollbar<Message> {
    content_length: u64,
    viewport_length: u64,
    offset: u64,
    track_length: u16,
    orientation: ScrollbarOrientation,
    style: ScrollbarStyle,
    message: std::marker::PhantomData<fn() -> Message>,
}

impl<Message> Scrollbar<Message> {
    /// Creates a vertical scrollbar with the supplied logical lengths
    #[must_use]
    pub fn new(content_length: u64, viewport_length: u64, offset: u64, track_length: u16) -> Self {
        Self {
            content_length,
            viewport_length,
            offset,
            track_length,
            orientation: ScrollbarOrientation::Vertical,
            style: ScrollbarStyle::default(),
            message: std::marker::PhantomData,
        }
    }

    /// Sets the rendering direction
    #[must_use]
    pub const fn orientation(mut self, orientation: ScrollbarOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Replaces the track and thumb styles
    #[must_use]
    pub const fn style(mut self, style: ScrollbarStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic node for this scrollbar
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let (start, length) = thumb_geometry(
            self.content_length,
            self.viewport_length,
            self.offset,
            self.track_length,
        );
        let before = start;
        let after = self
            .track_length
            .saturating_sub(start.saturating_add(length));
        match self.orientation {
            ScrollbarOrientation::Vertical => {
                let mut segments = Vec::with_capacity(3);
                push_vertical_segment(&mut segments, '│', before, self.style.track);
                push_vertical_segment(&mut segments, '█', length, self.style.thumb);
                push_vertical_segment(&mut segments, '│', after, self.style.track);
                Node::column(segments)
            }
            ScrollbarOrientation::Horizontal => Node::row([
                horizontal_segment('─', before, self.style.track),
                horizontal_segment('█', length, self.style.thumb),
                horizontal_segment('─', after, self.style.track),
            ]),
        }
    }
}

fn push_vertical_segment<Message>(
    segments: &mut Vec<Node<Message>>,
    character: char,
    length: u16,
    style: Style,
) {
    if length == 0 {
        return;
    }
    let content = std::iter::repeat_n(character.to_string(), usize::from(length))
        .collect::<Vec<_>>()
        .join("\n");
    segments.push(Node::styled_text(content, style));
}

fn horizontal_segment<Message>(character: char, length: u16, style: Style) -> Node<Message> {
    Node::styled_text(character.to_string().repeat(usize::from(length)), style)
}

fn thumb_geometry(
    content_length: u64,
    viewport_length: u64,
    offset: u64,
    track_length: u16,
) -> (u16, u16) {
    if track_length == 0 {
        return (0, 0);
    }
    if content_length == 0 || viewport_length >= content_length {
        return (0, track_length);
    }

    let length = ((u128::from(viewport_length) * u128::from(track_length)
        / u128::from(content_length))
    .max(1)
    .min(u128::from(track_length))) as u16;
    let maximum_offset = content_length - viewport_length;
    let maximum_start = track_length - length;
    let offset = offset.min(maximum_offset);
    let start =
        (u128::from(offset) * u128::from(maximum_start) / u128::from(maximum_offset)) as u16;
    (start, length)
}

#[cfg(test)]
mod tests {
    use super::thumb_geometry;

    #[test]
    fn thumb_geometry_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/scrollbar.txt",
            "widget-scrollbar",
            &["content", "viewport", "offset", "track", "start", "length"],
        ) else {
            return;
        };
        for record in records {
            assert_eq!(
                thumb_geometry(
                    number(record.field("content")),
                    number(record.field("viewport")),
                    number(record.field("offset")),
                    short(record.field("track")),
                ),
                (short(record.field("start")), short(record.field("length"))),
                "case {}",
                record.id
            );
        }
    }

    fn number(value: &str) -> u64 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
    }

    fn short(value: &str) -> u16 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid short number {value}: {error}"))
    }
}
