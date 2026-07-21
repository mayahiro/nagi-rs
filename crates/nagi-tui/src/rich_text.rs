use nagi_text::{WidthProfile, grapheme_width, graphemes};
use nagi_vt::Style;

use crate::{HorizontalAlignment, Size};

/// Automatic paragraph line-wrapping behavior
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum WrapMode {
    /// Prefer ASCII-space boundaries and hard-wrap oversized words
    #[default]
    Word,
    /// Wrap at the last complete grapheme that fits
    Hard,
    /// Preserve only explicit CR, LF, and CRLF line boundaries
    None,
}

/// One styled run of paragraph text
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextSpan {
    text: String,
    style: Style,
}

impl TextSpan {
    /// Creates one styled text run
    #[must_use]
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    /// Returns this span's UTF-8 text
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the style applied to every grapheme in this span
    #[must_use]
    pub const fn style(&self) -> Style {
        self.style
    }
}

/// Paragraph wrapping and horizontal alignment
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ParagraphOptions {
    /// Automatic line-wrapping behavior
    pub wrap: WrapMode,
    /// Placement of each rendered line within the paragraph rectangle
    pub alignment: HorizontalAlignment,
}

pub(crate) struct ParagraphUnit {
    span: usize,
    start: usize,
    end: usize,
    pub(crate) width: u32,
    space: bool,
    break_line: bool,
}

impl ParagraphUnit {
    pub(crate) fn text<'a>(&self, spans: &'a [TextSpan]) -> &'a str {
        &spans[self.span].text[self.start..self.end]
    }

    pub(crate) fn style(&self, spans: &[TextSpan]) -> Style {
        spans[self.span].style
    }
}

pub(crate) struct ParagraphLine {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) width: u32,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct ParagraphLayoutKey {
    max_width: u32,
    bounded: bool,
    mode: WrapMode,
}

struct ParagraphLayoutEntry {
    key: ParagraphLayoutKey,
    lines: Vec<ParagraphLine>,
    size: Size,
}

pub(crate) struct ParagraphLayout<'a> {
    pub(crate) units: &'a [ParagraphUnit],
    pub(crate) lines: &'a [ParagraphLine],
    pub(crate) size: Size,
}

#[derive(Default)]
pub(crate) struct ParagraphLayoutCache {
    units: Option<Vec<ParagraphUnit>>,
    entries: [Option<ParagraphLayoutEntry>; 2],
    next: usize,
}

impl ParagraphLayoutCache {
    pub(crate) fn resolve(
        &mut self,
        spans: &[TextSpan],
        max_width: u32,
        bounded: bool,
        mode: WrapMode,
    ) -> ParagraphLayout<'_> {
        let key = ParagraphLayoutKey {
            max_width,
            bounded,
            mode,
        };
        let mut entry_index = self
            .entries
            .iter()
            .position(|entry| entry.as_ref().is_some_and(|entry| entry.key == key));
        if entry_index.is_none() {
            let units = self.units.get_or_insert_with(|| span_units(spans));
            let lines = layout_lines(units, max_width, bounded, mode);
            let size = paragraph_layout_size(&lines);
            let index = self
                .entries
                .iter()
                .position(Option::is_none)
                .unwrap_or(self.next);
            self.entries[index] = Some(ParagraphLayoutEntry { key, lines, size });
            self.next = (index + 1) % self.entries.len();
            entry_index = Some(index);
        }
        let entry = self.entries[entry_index.expect("paragraph cache entry")]
            .as_ref()
            .expect("paragraph cache entry");
        ParagraphLayout {
            units: self.units.as_deref().unwrap_or_default(),
            lines: &entry.lines,
            size: entry.size,
        }
    }
}

fn layout_lines(
    units: &[ParagraphUnit],
    max_width: u32,
    bounded: bool,
    mode: WrapMode,
) -> Vec<ParagraphLine> {
    let mut lines = Vec::with_capacity(1);
    let mut start = 0;
    let mut width = 0_u32;
    for (index, unit) in units.iter().enumerate() {
        if unit.break_line {
            lines.push(ParagraphLine {
                start,
                end: index,
                width,
            });
            start = index + 1;
            width = 0;
            continue;
        }
        width = width.saturating_add(unit.width);
        if !bounded || mode == WrapMode::None {
            continue;
        }
        let end = index + 1;
        while width > max_width && end - start > 1 {
            let mut split = end - 1;
            let mut drop_space = false;
            if mode == WrapMode::Word {
                if let Some(space) = last_space(units, start, end).filter(|space| *space > start) {
                    split = space;
                    drop_space = true;
                }
            }
            let before_end = trim_spaces(units, start, split, false);
            lines.push(ParagraphLine {
                start,
                end: before_end,
                width: units_width(units, start, before_end),
            });
            let after_start = split + usize::from(drop_space);
            start = if drop_space {
                trim_spaces(units, after_start, end, true)
            } else {
                after_start
            };
            width = units_width(units, start, end);
        }
    }
    lines.push(ParagraphLine {
        start,
        end: units.len(),
        width,
    });
    lines
}

fn span_units(spans: &[TextSpan]) -> Vec<ParagraphUnit> {
    let capacity = spans
        .iter()
        .try_fold(0_usize, |total, span| {
            total.checked_add(span.text.chars().count())
        })
        .unwrap_or(0);
    let mut units = Vec::with_capacity(capacity);
    for (span_index, span) in spans.iter().enumerate() {
        for grapheme in graphemes(&span.text) {
            if matches!(grapheme.text(), "\r" | "\n" | "\r\n") {
                units.push(ParagraphUnit {
                    span: span_index,
                    start: grapheme.start(),
                    end: grapheme.end(),
                    width: 0,
                    space: false,
                    break_line: true,
                });
                continue;
            }
            let width = grapheme_width(grapheme.text(), WidthProfile::MODERN)
                .max(1)
                .min(u32::MAX as usize) as u32;
            units.push(ParagraphUnit {
                span: span_index,
                start: grapheme.start(),
                end: grapheme.end(),
                width,
                space: grapheme.text() == " ",
                break_line: false,
            });
        }
    }
    units
}

fn last_space(units: &[ParagraphUnit], start: usize, end: usize) -> Option<usize> {
    units[start..end]
        .iter()
        .rposition(|unit| unit.space)
        .map(|index| start + index)
}

fn trim_spaces(units: &[ParagraphUnit], mut start: usize, mut end: usize, leading: bool) -> usize {
    if leading {
        while start < end && units[start].space {
            start += 1;
        }
        return start;
    }
    while end > start && units[end - 1].space {
        end -= 1;
    }
    end
}

fn units_width(units: &[ParagraphUnit], start: usize, end: usize) -> u32 {
    units[start..end]
        .iter()
        .fold(0_u32, |width, unit| width.saturating_add(unit.width))
}

fn paragraph_layout_size(lines: &[ParagraphLine]) -> Size {
    Size::new(
        lines.iter().map(|line| line.width).max().unwrap_or(0),
        lines.len().min(u32::MAX as usize) as u32,
    )
}
