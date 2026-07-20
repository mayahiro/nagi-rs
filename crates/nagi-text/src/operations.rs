use crate::grapheme::graphemes;
use crate::width::{WidthProfile, cluster_width};

/// Returns the total terminal cell width of `text`
#[must_use]
pub fn text_width(text: &str, profile: WidthProfile<'_>) -> usize {
    graphemes(text).fold(0, |total, grapheme| {
        total.saturating_add(cluster_width(grapheme.text(), profile))
    })
}

/// Returns the longest grapheme-aligned prefix within `max_cells`
#[must_use]
pub fn truncate<'text>(
    text: &'text str,
    max_cells: usize,
    profile: WidthProfile<'_>,
) -> &'text str {
    let mut cells = 0_usize;
    let mut end = 0_usize;
    for grapheme in graphemes(text) {
        let next = cells.saturating_add(cluster_width(grapheme.text(), profile));
        if next > max_cells {
            break;
        }
        cells = next;
        end = grapheme.end();
    }
    &text[..end]
}

/// Hard-wraps text without splitting extended grapheme clusters
///
/// CR, LF, and CRLF force a line boundary and are omitted. A grapheme wider
/// than `max_cells` occupies a line by itself, which guarantees progress when
/// `max_cells` is zero
#[must_use]
pub fn wrap<'text>(
    text: &'text str,
    max_cells: usize,
    profile: WidthProfile<'_>,
) -> Vec<&'text str> {
    if text.is_empty() {
        return vec![""];
    }

    let mut lines = Vec::new();
    let mut start = 0_usize;
    let mut cells = 0_usize;
    for grapheme in graphemes(text) {
        if is_mandatory_break(grapheme.text()) {
            lines.push(&text[start..grapheme.start()]);
            start = grapheme.end();
            cells = 0;
            continue;
        }

        let width = cluster_width(grapheme.text(), profile);
        let next = cells.saturating_add(width);
        if width != 0 && grapheme.start() != start && next > max_cells {
            lines.push(&text[start..grapheme.start()]);
            start = grapheme.start();
            cells = width;
        } else {
            cells = next;
        }
    }
    lines.push(&text[start..]);
    lines
}

/// Converts an exact grapheme byte boundary to its terminal cell position
///
/// `None` is returned for offsets inside a grapheme or outside the string
#[must_use]
pub fn cell_at_byte(text: &str, byte_offset: usize, profile: WidthProfile<'_>) -> Option<usize> {
    if byte_offset > text.len() {
        return None;
    }
    if byte_offset == 0 {
        return Some(0);
    }
    let mut cells = 0_usize;
    for grapheme in graphemes(text) {
        cells = cells.saturating_add(cluster_width(grapheme.text(), profile));
        if grapheme.end() == byte_offset {
            return Some(cells);
        }
        if grapheme.end() > byte_offset {
            return None;
        }
    }
    None
}

/// Converts an exact terminal cell boundary to the earliest matching byte
/// boundary
///
/// `None` is returned for positions inside a wide grapheme or beyond the text
#[must_use]
pub fn byte_at_cell(text: &str, cell_offset: usize, profile: WidthProfile<'_>) -> Option<usize> {
    if cell_offset == 0 {
        return Some(0);
    }
    let mut cells = 0_usize;
    for grapheme in graphemes(text) {
        cells = cells.saturating_add(cluster_width(grapheme.text(), profile));
        if cells == cell_offset {
            return Some(grapheme.end());
        }
        if cells > cell_offset {
            return None;
        }
    }
    None
}

fn is_mandatory_break(text: &str) -> bool {
    matches!(text, "\r" | "\n" | "\r\n")
}

#[cfg(test)]
mod tests {
    use super::{byte_at_cell, cell_at_byte, truncate, wrap};
    use crate::width::WidthProfile;

    #[test]
    fn cell_operations_do_not_split_wide_graphemes() {
        assert_eq!(truncate("A日B", 2, WidthProfile::MODERN), "A");
        assert_eq!(wrap("A日B", 2, WidthProfile::MODERN), vec!["A", "日", "B"]);
        assert_eq!(cell_at_byte("A日B", 2, WidthProfile::MODERN), None);
        assert_eq!(byte_at_cell("A日B", 2, WidthProfile::MODERN), None);
    }
}
