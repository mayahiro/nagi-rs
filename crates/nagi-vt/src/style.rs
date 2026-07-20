/// A terminal color
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Color {
    /// The terminal's default color
    #[default]
    Default,
    /// An indexed terminal palette entry
    Indexed(u8),
    /// A 24-bit RGB color
    Rgb {
        /// Red component
        red: u8,
        /// Green component
        green: u8,
        /// Blue component
        blue: u8,
    },
}

/// Boolean terminal text attributes
///
/// This type groups the non-color parts of [`Style`] while the individual
/// fields on `Style` remain available for direct construction
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Attributes {
    /// Bold intensity
    pub bold: bool,
    /// Dim intensity
    pub dim: bool,
    /// Italic text
    pub italic: bool,
    /// Underlined text
    pub underline: bool,
    /// Blinking text
    pub blink: bool,
    /// Reversed foreground and background
    pub reverse: bool,
    /// Hidden text
    pub hidden: bool,
    /// Struck-through text
    pub strikethrough: bool,
}

impl Attributes {
    /// Returns `overlay` merged over these attributes
    #[must_use]
    pub const fn merged(self, overlay: Self) -> Self {
        Self {
            bold: self.bold || overlay.bold,
            dim: self.dim || overlay.dim,
            italic: self.italic || overlay.italic,
            underline: self.underline || overlay.underline,
            blink: self.blink || overlay.blink,
            reverse: self.reverse || overlay.reverse,
            hidden: self.hidden || overlay.hidden,
            strikethrough: self.strikethrough || overlay.strikethrough,
        }
    }

    /// Reports whether every attribute is disabled
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !self.bold
            && !self.dim
            && !self.italic
            && !self.underline
            && !self.blink
            && !self.reverse
            && !self.hidden
            && !self.strikethrough
    }
}

/// Visual attributes attached to a cell
///
/// The zero value is the terminal default style. During transparent
/// composition, default colors do not replace an existing color, an absent
/// underline color is preserved, and Boolean attributes are combined
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Style {
    /// Foreground color
    pub foreground: Color,
    /// Background color
    pub background: Color,
    /// Optional underline color
    pub underline_color: Option<Color>,
    /// Bold intensity
    pub bold: bool,
    /// Dim intensity
    pub dim: bool,
    /// Italic text
    pub italic: bool,
    /// Underlined text
    pub underline: bool,
    /// Blinking text
    pub blink: bool,
    /// Reversed foreground and background
    pub reverse: bool,
    /// Hidden text
    pub hidden: bool,
    /// Struck-through text
    pub strikethrough: bool,
}

impl Style {
    /// Returns the Boolean attributes in this style
    #[must_use]
    pub const fn attributes(self) -> Attributes {
        Attributes {
            bold: self.bold,
            dim: self.dim,
            italic: self.italic,
            underline: self.underline,
            blink: self.blink,
            reverse: self.reverse,
            hidden: self.hidden,
            strikethrough: self.strikethrough,
        }
    }

    /// Returns this style with its Boolean attributes replaced
    #[must_use]
    pub const fn with_attributes(mut self, attributes: Attributes) -> Self {
        self.bold = attributes.bold;
        self.dim = attributes.dim;
        self.italic = attributes.italic;
        self.underline = attributes.underline;
        self.blink = attributes.blink;
        self.reverse = attributes.reverse;
        self.hidden = attributes.hidden;
        self.strikethrough = attributes.strikethrough;
        self
    }

    /// Returns `overlay` merged over this style for transparent composition
    #[must_use]
    pub fn merged(self, overlay: Self) -> Self {
        Self {
            foreground: choose_color(self.foreground, overlay.foreground),
            background: choose_color(self.background, overlay.background),
            underline_color: overlay.underline_color.or(self.underline_color),
            bold: self.bold || overlay.bold,
            dim: self.dim || overlay.dim,
            italic: self.italic || overlay.italic,
            underline: self.underline || overlay.underline,
            blink: self.blink || overlay.blink,
            reverse: self.reverse || overlay.reverse,
            hidden: self.hidden || overlay.hidden,
            strikethrough: self.strikethrough || overlay.strikethrough,
        }
    }
}

fn choose_color(base: Color, overlay: Color) -> Color {
    if overlay == Color::Default {
        base
    } else {
        overlay
    }
}

#[cfg(test)]
mod tests {
    use super::{Attributes, Color, Style};

    #[test]
    fn attributes_round_trip_through_style() {
        let attributes = Attributes {
            bold: true,
            underline: true,
            ..Attributes::default()
        };

        let style = Style::default().with_attributes(attributes);

        assert_eq!(style.attributes(), attributes);
        assert!(!attributes.is_empty());
    }

    #[test]
    fn transparent_merge_preserves_unspecified_values() {
        let base = Style {
            foreground: Color::Indexed(2),
            italic: true,
            ..Style::default()
        };
        let overlay = Style {
            background: Color::Rgb {
                red: 1,
                green: 2,
                blue: 3,
            },
            bold: true,
            ..Style::default()
        };

        let merged = base.merged(overlay);

        assert_eq!(merged.foreground, Color::Indexed(2));
        assert_eq!(merged.background, overlay.background);
        assert!(merged.bold);
        assert!(merged.italic);
    }
}
