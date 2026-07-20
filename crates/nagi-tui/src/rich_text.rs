use nagi_text::{WidthProfile, grapheme_width, graphemes};
use nagi_vt::Style;

use crate::HorizontalAlignment;

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

#[derive(Clone)]
pub(crate) struct ParagraphUnit {
    pub(crate) text: String,
    pub(crate) style: Style,
    pub(crate) width: u32,
    space: bool,
    break_line: bool,
}

#[derive(Default)]
pub(crate) struct ParagraphLine {
    pub(crate) units: Vec<ParagraphUnit>,
    pub(crate) width: u32,
}

pub(crate) fn layout(
    spans: &[TextSpan],
    max_width: u32,
    bounded: bool,
    mode: WrapMode,
) -> Vec<ParagraphLine> {
    let units = span_units(spans);
    let mut lines = Vec::with_capacity(1);
    let mut current = ParagraphLine::default();
    for unit in units {
        if unit.break_line {
            lines.push(current);
            current = ParagraphLine::default();
            continue;
        }
        current.width = current.width.saturating_add(unit.width);
        current.units.push(unit);
        if !bounded || mode == WrapMode::None {
            continue;
        }
        while current.width > max_width && current.units.len() > 1 {
            let mut split = current.units.len() - 1;
            let mut drop_space = false;
            if mode == WrapMode::Word {
                if let Some(space) = last_space(&current.units).filter(|space| *space > 0) {
                    split = space;
                    drop_space = true;
                }
            }
            let before = line_from_units(trim_spaces(&current.units[..split], false));
            let after_start = split + usize::from(drop_space);
            let after = if drop_space {
                trim_spaces(&current.units[after_start..], true)
            } else {
                &current.units[after_start..]
            };
            lines.push(before);
            current = line_from_units(after);
        }
    }
    lines.push(current);
    lines
}

fn span_units(spans: &[TextSpan]) -> Vec<ParagraphUnit> {
    let mut units = Vec::new();
    for span in spans {
        for grapheme in graphemes(&span.text) {
            if matches!(grapheme.text(), "\r" | "\n" | "\r\n") {
                units.push(ParagraphUnit {
                    text: String::new(),
                    style: span.style,
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
                text: grapheme.text().to_owned(),
                style: span.style,
                width,
                space: grapheme.text() == " ",
                break_line: false,
            });
        }
    }
    units
}

fn last_space(units: &[ParagraphUnit]) -> Option<usize> {
    units.iter().rposition(|unit| unit.space)
}

fn trim_spaces(units: &[ParagraphUnit], leading: bool) -> &[ParagraphUnit] {
    let mut start = 0;
    let mut end = units.len();
    if leading {
        while start < end && units[start].space {
            start += 1;
        }
    } else {
        while end > start && units[end - 1].space {
            end -= 1;
        }
    }
    &units[start..end]
}

fn line_from_units(units: &[ParagraphUnit]) -> ParagraphLine {
    ParagraphLine {
        units: units.to_vec(),
        width: units
            .iter()
            .fold(0_u32, |width, unit| width.saturating_add(unit.width)),
    }
}
