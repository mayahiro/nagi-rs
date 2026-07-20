use std::fmt;

use crate::grapheme::graphemes;
use crate::unicode::{
    EastAsianWidth, GraphemeBreak, east_asian_width, grapheme_break, is_emoji_presentation,
    is_emoji_variation_base, is_rgi_emoji,
};

/// A valid custom terminal width for one grapheme
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum CellCount {
    /// The grapheme advances no cells
    Zero = 0,
    /// The grapheme advances one cell
    One = 1,
    /// The grapheme advances two cells
    Two = 2,
}

impl CellCount {
    /// Returns the width as a platform-sized cell count
    #[must_use]
    pub const fn get(self) -> usize {
        self as usize
    }
}

/// A callback that can override the width of a complete grapheme
pub type WidthOverride<'a> = dyn Fn(&str) -> Option<CellCount> + 'a;

/// Terminal cell-width policy
///
/// `MODERN` renders East Asian Ambiguous characters as one cell and `CJK`
/// renders them as two. A custom profile layers a grapheme callback over one
/// of those base policies
#[derive(Clone, Copy)]
pub struct WidthProfile<'a> {
    ambiguous_is_wide: bool,
    override_width: Option<&'a WidthOverride<'a>>,
}

impl WidthProfile<'static> {
    /// Modern terminal width with ambiguous characters occupying one cell
    pub const MODERN: Self = Self {
        ambiguous_is_wide: false,
        override_width: None,
    };

    /// CJK terminal width with ambiguous characters occupying two cells
    pub const CJK: Self = Self {
        ambiguous_is_wide: true,
        override_width: None,
    };
}

impl<'a> WidthProfile<'a> {
    /// Creates a profile that invokes `override_width` before the base policy
    #[must_use]
    pub const fn custom(
        base: WidthProfile<'static>,
        override_width: &'a WidthOverride<'a>,
    ) -> Self {
        Self {
            ambiguous_is_wide: base.ambiguous_is_wide,
            override_width: Some(override_width),
        }
    }

    pub(crate) fn override_for(self, grapheme: &str) -> Option<usize> {
        self.override_width
            .and_then(|override_width| override_width(grapheme))
            .map(CellCount::get)
    }

    pub(crate) const fn ambiguous_is_wide(self) -> bool {
        self.ambiguous_is_wide
    }
}

impl fmt::Debug for WidthProfile<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WidthProfile")
            .field("ambiguous_is_wide", &self.ambiguous_is_wide)
            .field("has_override", &self.override_width.is_some())
            .finish()
    }
}

/// Returns the terminal cell width of one complete extended grapheme cluster
///
/// Passing more than one grapheme is supported but returns the total width;
/// callers that already iterate clusters avoid that extra segmentation
#[must_use]
pub fn grapheme_width(text: &str, profile: WidthProfile<'_>) -> usize {
    let mut clusters = graphemes(text);
    let Some(first) = clusters.next() else {
        return 0;
    };
    if clusters.next().is_some() {
        return graphemes(text)
            .map(|grapheme| cluster_width(grapheme.text(), profile))
            .sum();
    }
    cluster_width(first.text(), profile)
}

pub(crate) fn cluster_width(grapheme: &str, profile: WidthProfile<'_>) -> usize {
    if let Some(width) = profile.override_for(grapheme) {
        return width;
    }
    if grapheme.is_empty()
        || grapheme
            .chars()
            .any(|character| grapheme_break(character).is_control())
    {
        return 0;
    }
    if is_rgi_emoji(grapheme) {
        return 2;
    }

    let mut text_presentation = false;
    let mut previous = None;
    for character in grapheme.chars() {
        if previous.is_some_and(is_emoji_variation_base) {
            if character == '\u{FE0F}' {
                return 2;
            }
            if character == '\u{FE0E}' {
                text_presentation = true;
            }
        }
        previous = Some(character);
    }
    if text_presentation {
        return 1;
    }
    if grapheme.chars().any(is_emoji_presentation) {
        return 2;
    }

    let mut width = 0;
    for character in grapheme.chars() {
        if matches!(
            grapheme_break(character),
            GraphemeBreak::Extend | GraphemeBreak::Zwj | GraphemeBreak::Prepend
        ) {
            continue;
        }
        width = width.max(match east_asian_width(character) {
            EastAsianWidth::Wide => 2,
            EastAsianWidth::Ambiguous if profile.ambiguous_is_wide() => 2,
            EastAsianWidth::Ambiguous | EastAsianWidth::Narrow => 1,
        });
    }
    width
}

#[cfg(test)]
mod tests {
    use super::{CellCount, WidthProfile, grapheme_width};

    #[test]
    fn custom_profile_overrides_a_complete_grapheme() {
        let override_width = |grapheme: &str| (grapheme == "日").then_some(CellCount::One);
        let profile = WidthProfile::custom(WidthProfile::MODERN, &override_width);

        assert_eq!(grapheme_width("日", profile), 1);
        assert_eq!(grapheme_width("本", profile), 2);
    }

    #[test]
    fn empty_text_has_no_grapheme_to_override() {
        let override_width = |_: &str| Some(CellCount::One);
        let profile = WidthProfile::custom(WidthProfile::MODERN, &override_width);

        assert_eq!(grapheme_width("", profile), 0);
    }
}
