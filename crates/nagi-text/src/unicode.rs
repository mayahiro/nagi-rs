use crate::generated::{
    EAST_ASIAN_WIDTH_RANGES, EMOJI_PRESENTATION_RANGES, EMOJI_VARIATION_BASES,
    EXTENDED_PICTOGRAPHIC_RANGES, GRAPHEME_BREAK_RANGES, INDIC_CONJUNCT_BREAK_RANGES,
    RGI_EMOJI_SEQUENCES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum GraphemeBreak {
    Other = 0,
    Cr = 1,
    Lf = 2,
    Control = 3,
    Extend = 4,
    Zwj = 5,
    RegionalIndicator = 6,
    Prepend = 7,
    SpacingMark = 8,
    L = 9,
    V = 10,
    T = 11,
    Lv = 12,
    Lvt = 13,
}

impl GraphemeBreak {
    fn from_value(value: u8) -> Self {
        match value {
            1 => Self::Cr,
            2 => Self::Lf,
            3 => Self::Control,
            4 => Self::Extend,
            5 => Self::Zwj,
            6 => Self::RegionalIndicator,
            7 => Self::Prepend,
            8 => Self::SpacingMark,
            9 => Self::L,
            10 => Self::V,
            11 => Self::T,
            12 => Self::Lv,
            13 => Self::Lvt,
            _ => Self::Other,
        }
    }

    pub(crate) const fn is_control(self) -> bool {
        matches!(self, Self::Cr | Self::Lf | Self::Control)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IndicConjunctBreak {
    None,
    Consonant,
    Extend,
    Linker,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EastAsianWidth {
    Narrow,
    Ambiguous,
    Wide,
}

pub(crate) fn grapheme_break(character: char) -> GraphemeBreak {
    GraphemeBreak::from_value(value_at(GRAPHEME_BREAK_RANGES, u32::from(character)))
}

pub(crate) fn indic_conjunct_break(character: char) -> IndicConjunctBreak {
    match value_at(INDIC_CONJUNCT_BREAK_RANGES, u32::from(character)) {
        1 => IndicConjunctBreak::Consonant,
        2 => IndicConjunctBreak::Extend,
        3 => IndicConjunctBreak::Linker,
        _ => IndicConjunctBreak::None,
    }
}

pub(crate) fn east_asian_width(character: char) -> EastAsianWidth {
    match value_at(EAST_ASIAN_WIDTH_RANGES, u32::from(character)) {
        1 => EastAsianWidth::Ambiguous,
        2 => EastAsianWidth::Wide,
        _ => EastAsianWidth::Narrow,
    }
}

pub(crate) fn is_extended_pictographic(character: char) -> bool {
    contains(EXTENDED_PICTOGRAPHIC_RANGES, u32::from(character))
}

pub(crate) fn is_emoji_presentation(character: char) -> bool {
    contains(EMOJI_PRESENTATION_RANGES, u32::from(character))
}

pub(crate) fn is_emoji_variation_base(character: char) -> bool {
    EMOJI_VARIATION_BASES
        .binary_search(&u32::from(character))
        .is_ok()
}

pub(crate) fn is_rgi_emoji(sequence: &[u32]) -> bool {
    RGI_EMOJI_SEQUENCES
        .binary_search_by(|candidate| (*candidate).cmp(sequence))
        .is_ok()
}

fn value_at(ranges: &[(u32, u32, u8)], code_point: u32) -> u8 {
    let index = ranges.partition_point(|(_, end, _)| *end < code_point);
    ranges
        .get(index)
        .filter(|(start, _, _)| *start <= code_point)
        .map_or(0, |(_, _, value)| *value)
}

fn contains(ranges: &[(u32, u32)], code_point: u32) -> bool {
    let index = ranges.partition_point(|(_, end)| *end < code_point);
    ranges
        .get(index)
        .is_some_and(|(start, _)| *start <= code_point)
}

#[cfg(test)]
mod tests {
    use super::{
        EastAsianWidth, GraphemeBreak, east_asian_width, grapheme_break, is_extended_pictographic,
    };

    #[test]
    fn generated_properties_are_searchable() {
        assert_eq!(grapheme_break('\u{0301}'), GraphemeBreak::Extend);
        assert_eq!(east_asian_width('日'), EastAsianWidth::Wide);
        assert!(is_extended_pictographic('\u{1F469}'));
    }
}
