use nagi_vt::Style;

use crate::node::Insets;

/// A built-in one-cell panel border
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BorderKind {
    /// Square single-line box drawing characters
    #[default]
    Single,
    /// Rounded single-line corners
    Rounded,
    /// Double-line box drawing characters
    Double,
    /// Heavy box drawing characters
    Thick,
}

/// Visual styles used by a panel node
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PanelStyle {
    /// Border-cell style
    pub border: Style,
    /// Optional-title style
    pub title: Style,
    /// Style used to fill the complete panel rectangle before child rendering
    pub background: Style,
}

/// Panel border, padding, and styles
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PanelOptions {
    /// Border character set
    pub border: BorderKind,
    /// Padding inside the one-cell border
    pub padding: Insets,
    /// Panel colors and attributes
    pub style: PanelStyle,
}

impl Default for PanelOptions {
    fn default() -> Self {
        Self {
            border: BorderKind::Single,
            padding: Insets::all(1),
            style: PanelStyle::default(),
        }
    }
}

pub(crate) struct BorderGlyphs {
    pub(crate) top_left: &'static str,
    pub(crate) horizontal: &'static str,
    pub(crate) top_right: &'static str,
    pub(crate) vertical: &'static str,
    pub(crate) bottom_left: &'static str,
    pub(crate) bottom_right: &'static str,
}

pub(crate) const fn glyphs(kind: BorderKind) -> BorderGlyphs {
    match kind {
        BorderKind::Single => BorderGlyphs {
            top_left: "┌",
            horizontal: "─",
            top_right: "┐",
            vertical: "│",
            bottom_left: "└",
            bottom_right: "┘",
        },
        BorderKind::Rounded => BorderGlyphs {
            top_left: "╭",
            horizontal: "─",
            top_right: "╮",
            vertical: "│",
            bottom_left: "╰",
            bottom_right: "╯",
        },
        BorderKind::Double => BorderGlyphs {
            top_left: "╔",
            horizontal: "═",
            top_right: "╗",
            vertical: "║",
            bottom_left: "╚",
            bottom_right: "╝",
        },
        BorderKind::Thick => BorderGlyphs {
            top_left: "┏",
            horizontal: "━",
            top_right: "┓",
            vertical: "┃",
            bottom_left: "┗",
            bottom_right: "┛",
        },
    }
}

pub(crate) const fn content_insets(options: PanelOptions) -> Insets {
    Insets {
        top: 1_u32.saturating_add(options.padding.top),
        right: 1_u32.saturating_add(options.padding.right),
        bottom: 1_u32.saturating_add(options.padding.bottom),
        left: 1_u32.saturating_add(options.padding.left),
    }
}
