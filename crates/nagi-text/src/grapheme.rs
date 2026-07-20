use crate::unicode::{
    GraphemeBreak, IndicConjunctBreak, grapheme_break, indic_conjunct_break,
    is_extended_pictographic,
};

/// One extended grapheme cluster and its UTF-8 byte range
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Grapheme<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

impl<'a> Grapheme<'a> {
    /// Returns the cluster text without normalization
    #[must_use]
    pub const fn text(self) -> &'a str {
        self.text
    }

    /// Returns the inclusive UTF-8 start byte offset
    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    /// Returns the exclusive UTF-8 end byte offset
    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }
}

/// An iterator over Unicode extended grapheme clusters
#[derive(Clone, Debug)]
pub struct Graphemes<'a> {
    text: &'a str,
    next: usize,
}

impl<'a> Iterator for Graphemes<'a> {
    type Item = Grapheme<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == self.text.len() {
            return None;
        }
        let start = self.next;
        let end = next_boundary_from(self.text, start);
        self.next = end;
        Some(Grapheme {
            text: &self.text[start..end],
            start,
            end,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::from(self.next < self.text.len()), None)
    }
}

/// Returns an iterator over Unicode extended grapheme clusters
#[must_use]
pub const fn graphemes(text: &str) -> Graphemes<'_> {
    Graphemes { text, next: 0 }
}

/// Returns all extended grapheme cluster boundaries, including zero and the
/// UTF-8 length
#[must_use]
pub fn grapheme_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = Vec::with_capacity(text.chars().count() + 1);
    boundaries.push(0);
    boundaries.extend(graphemes(text).map(Grapheme::end));
    boundaries
}

/// Reports whether `byte_offset` is an extended grapheme cluster boundary
#[must_use]
pub fn is_grapheme_boundary(text: &str, byte_offset: usize) -> bool {
    if byte_offset > text.len() {
        return false;
    }
    byte_offset == 0 || graphemes(text).any(|grapheme| grapheme.end == byte_offset)
}

/// Returns the nearest strict extended grapheme boundary after `byte_offset`
///
/// The input may be inside a UTF-8 sequence or grapheme. `None` is returned at
/// or beyond the end of the string
#[must_use]
pub fn next_grapheme_boundary(text: &str, byte_offset: usize) -> Option<usize> {
    if byte_offset >= text.len() {
        return None;
    }
    graphemes(text)
        .map(Grapheme::end)
        .find(|boundary| *boundary > byte_offset)
}

/// Returns the nearest strict extended grapheme boundary before `byte_offset`
///
/// The input may be inside a UTF-8 sequence or grapheme. `None` is returned at
/// byte offset zero or when the offset is outside the string
#[must_use]
pub fn previous_grapheme_boundary(text: &str, byte_offset: usize) -> Option<usize> {
    if byte_offset == 0 || byte_offset > text.len() {
        return None;
    }
    let mut previous = 0;
    for boundary in graphemes(text).map(Grapheme::end) {
        if boundary >= byte_offset {
            break;
        }
        previous = boundary;
    }
    Some(previous)
}

fn next_boundary_from(text: &str, start: usize) -> usize {
    let mut characters = text[start..].char_indices();
    let Some((_, first)) = characters.next() else {
        return text.len();
    };
    let mut state = BoundaryState::new(first);
    for (relative, right) in characters {
        if state.breaks_before(right) {
            return start + relative;
        }
        state.push(right);
    }
    text.len()
}

struct BoundaryState {
    left: GraphemeBreak,
    trailing_regional_indicators: usize,
    indic_consonant: bool,
    indic_linker: bool,
    emoji_base: bool,
    emoji_zwj: bool,
}

impl BoundaryState {
    fn new(first: char) -> Self {
        let mut state = Self {
            left: GraphemeBreak::Other,
            trailing_regional_indicators: 0,
            indic_consonant: false,
            indic_linker: false,
            emoji_base: false,
            emoji_zwj: false,
        };
        state.push(first);
        state
    }

    fn breaks_before(&self, right: char) -> bool {
        let left = self.left;
        let right_property = grapheme_break(right);

        if left == GraphemeBreak::Cr && right_property == GraphemeBreak::Lf {
            return false;
        }
        if left.is_control() || right_property.is_control() {
            return true;
        }
        if left == GraphemeBreak::L
            && matches!(
                right_property,
                GraphemeBreak::L | GraphemeBreak::V | GraphemeBreak::Lv | GraphemeBreak::Lvt
            )
        {
            return false;
        }
        if matches!(left, GraphemeBreak::Lv | GraphemeBreak::V)
            && matches!(right_property, GraphemeBreak::V | GraphemeBreak::T)
        {
            return false;
        }
        if matches!(left, GraphemeBreak::Lvt | GraphemeBreak::T)
            && right_property == GraphemeBreak::T
        {
            return false;
        }
        if matches!(right_property, GraphemeBreak::Extend | GraphemeBreak::Zwj) {
            return false;
        }
        if right_property == GraphemeBreak::SpacingMark || left == GraphemeBreak::Prepend {
            return false;
        }
        if indic_conjunct_break(right) == IndicConjunctBreak::Consonant
            && self.indic_consonant
            && self.indic_linker
        {
            return false;
        }
        if is_extended_pictographic(right) && left == GraphemeBreak::Zwj && self.emoji_zwj {
            return false;
        }
        if right_property == GraphemeBreak::RegionalIndicator
            && self.trailing_regional_indicators % 2 == 1
        {
            return false;
        }
        true
    }

    fn push(&mut self, character: char) {
        let property = grapheme_break(character);
        if property == GraphemeBreak::RegionalIndicator {
            self.trailing_regional_indicators += 1;
        } else {
            self.trailing_regional_indicators = 0;
        }

        match indic_conjunct_break(character) {
            IndicConjunctBreak::Consonant => {
                self.indic_consonant = true;
                self.indic_linker = false;
            }
            IndicConjunctBreak::Linker => {
                if self.indic_consonant {
                    self.indic_linker = true;
                }
            }
            IndicConjunctBreak::Extend => {}
            IndicConjunctBreak::None => {
                self.indic_consonant = false;
                self.indic_linker = false;
            }
        }

        self.emoji_zwj = property == GraphemeBreak::Zwj && self.emoji_base;
        if property != GraphemeBreak::Extend {
            self.emoji_base = is_extended_pictographic(character);
        }
        self.left = property;
    }
}

#[cfg(test)]
mod tests {
    use super::{grapheme_boundaries, next_grapheme_boundary, previous_grapheme_boundary};

    #[test]
    fn cursor_movement_accepts_offsets_inside_a_cluster() {
        let text = "A日B";

        assert_eq!(next_grapheme_boundary(text, 2), Some(4));
        assert_eq!(previous_grapheme_boundary(text, 3), Some(1));
    }

    #[test]
    fn regional_indicators_pair_from_the_start() {
        assert_eq!(grapheme_boundaries("🇯🇵🇺"), vec![0, 8, 12]);
    }
}
