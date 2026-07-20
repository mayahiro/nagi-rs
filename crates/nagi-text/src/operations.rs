use crate::grapheme::{Graphemes, graphemes};
use crate::width::{WidthProfile, cluster_width};

/// One hard-wrapped line and its terminal cell width
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WrappedLine<'text> {
    text: &'text str,
    width: usize,
}

impl<'text> WrappedLine<'text> {
    /// Returns the line content without a mandatory line-break grapheme
    #[must_use]
    pub const fn text(self) -> &'text str {
        self.text
    }

    /// Returns the line width in terminal cells
    #[must_use]
    pub const fn width(self) -> usize {
        self.width
    }
}

/// An iterator over hard-wrapped lines
#[derive(Clone, Debug)]
pub struct WrappedLines<'text, 'profile> {
    text: &'text str,
    graphemes: Graphemes<'text>,
    profile: WidthProfile<'profile>,
    max_cells: usize,
    start: usize,
    cells: usize,
    finished: bool,
}

impl<'text> Iterator for WrappedLines<'text, '_> {
    type Item = WrappedLine<'text>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        for grapheme in self.graphemes.by_ref() {
            if is_mandatory_break(grapheme.text()) {
                let line = WrappedLine {
                    text: &self.text[self.start..grapheme.start()],
                    width: self.cells,
                };
                self.start = grapheme.end();
                self.cells = 0;
                return Some(line);
            }

            let width = cluster_width(grapheme.text(), self.profile);
            let next = self.cells.saturating_add(width);
            if width != 0 && grapheme.start() != self.start && next > self.max_cells {
                let line = WrappedLine {
                    text: &self.text[self.start..grapheme.start()],
                    width: self.cells,
                };
                self.start = grapheme.start();
                self.cells = width;
                return Some(line);
            }
            self.cells = next;
        }
        self.finished = true;
        Some(WrappedLine {
            text: &self.text[self.start..],
            width: self.cells,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.finished {
            (0, Some(0))
        } else {
            (1, None)
        }
    }
}

/// Returns an iterator using the same hard-wrapping rules as [`wrap`]
#[must_use]
pub fn wrapped_lines<'text, 'profile>(
    text: &'text str,
    max_cells: usize,
    profile: WidthProfile<'profile>,
) -> WrappedLines<'text, 'profile> {
    WrappedLines {
        text,
        graphemes: graphemes(text),
        profile,
        max_cells,
        start: 0,
        cells: 0,
        finished: false,
    }
}

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
    wrapped_lines(text, max_cells, profile)
        .map(WrappedLine::text)
        .collect()
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
    use super::{byte_at_cell, cell_at_byte, truncate, wrap, wrapped_lines};
    use crate::width::WidthProfile;

    #[test]
    fn cell_operations_do_not_split_wide_graphemes() {
        assert_eq!(truncate("A日B", 2, WidthProfile::MODERN), "A");
        assert_eq!(wrap("A日B", 2, WidthProfile::MODERN), vec!["A", "日", "B"]);
        assert_eq!(cell_at_byte("A日B", 2, WidthProfile::MODERN), None);
        assert_eq!(byte_at_cell("A日B", 2, WidthProfile::MODERN), None);
    }

    #[test]
    fn wrapped_line_iterator_reports_text_and_width() {
        let lines: Vec<_> = wrapped_lines("A日\nB", 2, WidthProfile::MODERN).collect();

        assert_eq!(
            lines.iter().map(|line| line.text()).collect::<Vec<_>>(),
            vec!["A", "日", "B"]
        );
        assert_eq!(
            lines.iter().map(|line| line.width()).collect::<Vec<_>>(),
            vec![1, 2, 1]
        );
    }
}
