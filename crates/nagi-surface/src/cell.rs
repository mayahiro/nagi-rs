use std::error::Error;
use std::fmt;
use std::sync::Arc;

use nagi_text::{WidthProfile, grapheme_width, graphemes};
use nagi_vt::Style;

const REPLACEMENT: &str = "\u{FFFD}";

/// The number of grid cells occupied by a leading cell
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CellSpan {
    /// One terminal cell
    One,
    /// Two terminal cells
    Two,
}

impl CellSpan {
    /// Returns the numeric cell count
    #[must_use]
    pub const fn cells(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }
}

/// How a cell participates in surface composition
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Opacity {
    /// Content and style replace the destination cell
    #[default]
    Opaque,
    /// Content is absent and style merges over the destination cell
    Transparent,
}

/// An error returned while constructing a cell
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellError {
    /// Cell content must contain exactly one extended grapheme cluster
    ExpectedSingleGrapheme,
}

impl fmt::Display for CellError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedSingleGrapheme => {
                formatter.write_str("cell content must contain exactly one grapheme")
            }
        }
    }
}

impl Error for CellError {}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Content {
    Empty,
    Space,
    Replacement,
    Text(Arc<str>),
}

impl Content {
    fn new(text: &str) -> Self {
        match text {
            "" => Self::Empty,
            " " => Self::Space,
            REPLACEMENT => Self::Replacement,
            _ => Self::Text(Arc::from(text)),
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Empty => "",
            Self::Space => " ",
            Self::Replacement => REPLACEMENT,
            Self::Text(text) => text,
        }
    }
}

/// One normalized surface cell
///
/// Wide graphemes use a leading `CellSpan::Two` cell followed by a continuation
/// cell. Use [`Surface`](crate::Surface) drawing methods to place cells while
/// preserving that invariant
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Cell {
    content: Content,
    span: CellSpan,
    continuation: bool,
    style: Style,
    opacity: Opacity,
}

impl Cell {
    /// Creates an opaque cell from exactly one extended grapheme cluster
    ///
    /// A zero-width cluster becomes a one-cell U+FFFD replacement
    pub fn new(grapheme: &str, style: Style, profile: WidthProfile<'_>) -> Result<Self, CellError> {
        let mut clusters = graphemes(grapheme);
        let Some(cluster) = clusters.next() else {
            return Err(CellError::ExpectedSingleGrapheme);
        };
        if clusters.next().is_some() {
            return Err(CellError::ExpectedSingleGrapheme);
        }
        Ok(Self::from_cluster(cluster.text(), style, profile))
    }

    /// Creates an opaque one-cell blank with `style`
    #[must_use]
    pub fn blank(style: Style) -> Self {
        Self {
            content: Content::Space,
            span: CellSpan::One,
            continuation: false,
            style,
            opacity: Opacity::Opaque,
        }
    }

    /// Creates a style-only transparent cell
    #[must_use]
    pub fn transparent(style: Style) -> Self {
        Self {
            content: Content::Empty,
            span: CellSpan::One,
            continuation: false,
            style,
            opacity: Opacity::Transparent,
        }
    }

    /// Returns the grapheme content, or an empty string for a continuation or
    /// transparent cell
    #[must_use]
    pub fn content(&self) -> &str {
        self.content.as_str()
    }

    /// Returns the display span of this cell's grapheme unit
    #[must_use]
    pub const fn span(&self) -> CellSpan {
        self.span
    }

    /// Reports whether this is the trailing cell of a wide grapheme
    #[must_use]
    pub const fn is_continuation(&self) -> bool {
        self.continuation
    }

    /// Returns the cell style
    #[must_use]
    pub const fn style(&self) -> Style {
        self.style
    }

    /// Returns the composition opacity
    #[must_use]
    pub const fn opacity(&self) -> Opacity {
        self.opacity
    }

    pub(crate) fn from_cluster(grapheme: &str, style: Style, profile: WidthProfile<'_>) -> Self {
        match grapheme_width(grapheme, profile) {
            1 => Self {
                content: Content::new(grapheme),
                span: CellSpan::One,
                continuation: false,
                style,
                opacity: Opacity::Opaque,
            },
            2 => Self {
                content: Content::new(grapheme),
                span: CellSpan::Two,
                continuation: false,
                style,
                opacity: Opacity::Opaque,
            },
            _ => Self {
                content: Content::Replacement,
                span: CellSpan::One,
                continuation: false,
                style,
                opacity: Opacity::Opaque,
            },
        }
    }

    pub(crate) fn continuation(leading: &Self) -> Self {
        Self {
            content: Content::Empty,
            span: CellSpan::Two,
            continuation: true,
            style: leading.style,
            opacity: leading.opacity,
        }
    }

    pub(crate) fn replace_style(&mut self, style: Style) {
        self.style = style;
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::blank(Style::default())
    }
}

#[cfg(test)]
mod tests {
    use nagi_text::WidthProfile;

    use super::{Cell, CellError, CellSpan};
    use nagi_vt::Style;

    #[test]
    fn validates_single_grapheme_content() {
        assert_eq!(
            Cell::new("", Style::default(), WidthProfile::MODERN),
            Err(CellError::ExpectedSingleGrapheme)
        );
        assert_eq!(
            Cell::new("ab", Style::default(), WidthProfile::MODERN),
            Err(CellError::ExpectedSingleGrapheme)
        );
    }

    #[test]
    fn standalone_zero_width_cluster_uses_replacement() {
        let cell =
            Cell::new("\u{0301}", Style::default(), WidthProfile::MODERN).expect("one cluster");

        assert_eq!(cell.content(), "\u{FFFD}");
        assert_eq!(cell.span(), CellSpan::One);
    }
}
