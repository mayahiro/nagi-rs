use nagi_vt::{Color, Style};

use crate::{Node, ParagraphOptions, TextSpan};

/// Paragraph behavior used after safe ANSI SGR parsing
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct AnsiTextOptions {
    /// Wrapping and alignment applied to the parsed spans
    pub paragraph: ParagraphOptions,
}

impl<Message> Node<Message> {
    /// Parses ANSI SGR styling and discards every non-display control sequence
    ///
    /// CSI commands other than SGR, OSC, DCS, SOS, PM, APC, raw escape
    /// sequences, and non-line-breaking control characters never reach output
    #[must_use]
    pub fn ansi_text(input: &str, options: AnsiTextOptions) -> Self {
        Self::paragraph(parse_ansi_spans(input), options.paragraph)
    }
}

fn parse_ansi_spans(input: &str) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    let mut text = String::new();
    let mut style = Style::default();
    let mut index = 0;
    while index < input.len() {
        let (character, next) = next_character(input, index);
        match character {
            '\u{1b}' => {
                let (after, next_style) = parse_escape(input, next, style);
                if let Some(next_style) = next_style.filter(|next_style| *next_style != style) {
                    push_span(&mut spans, &mut text, style);
                    style = next_style;
                }
                index = after;
            }
            '\u{009b}' => {
                let (after, body) = scan_csi(input, next);
                if let Some(body) = body {
                    let next_style = apply_sgr(style, body);
                    if next_style != style {
                        push_span(&mut spans, &mut text, style);
                        style = next_style;
                    }
                }
                index = after;
            }
            '\u{009d}' => index = skip_control_string(input, next, true),
            '\u{0090}' | '\u{0098}' | '\u{009e}' | '\u{009f}' => {
                index = skip_control_string(input, next, false);
            }
            '\r' | '\n' => {
                text.push(character);
                index = next;
            }
            character if character.is_control() => index = next,
            _ => {
                text.push(character);
                index = next;
            }
        }
    }
    push_span(&mut spans, &mut text, style);
    spans
}

fn parse_escape(input: &str, start: usize, style: Style) -> (usize, Option<Style>) {
    if start >= input.len() {
        return (start, None);
    }
    let (introducer, next) = next_character(input, start);
    match introducer {
        '[' => {
            let (after, body) = scan_csi(input, next);
            (after, body.map(|body| apply_sgr(style, body)))
        }
        ']' => (skip_control_string(input, next, true), None),
        'P' | 'X' | '^' | '_' => (skip_control_string(input, next, false), None),
        _ => (skip_escape_sequence(input, start), None),
    }
}

fn scan_csi(input: &str, start: usize) -> (usize, Option<&str>) {
    let bytes = input.as_bytes();
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if (0x40..=0x7e).contains(&byte) {
            let body = &input[start..index];
            return (index + 1, (byte == b'm').then_some(body));
        }
        if byte == b'\n' || byte == b'\r' || byte == 0x1b {
            return (index, None);
        }
        index += 1;
    }
    (input.len(), None)
}

fn skip_control_string(input: &str, start: usize, bell_terminated: bool) -> usize {
    let mut index = start;
    while index < input.len() {
        let (character, next) = next_character(input, index);
        if character == '\u{009c}' || bell_terminated && character == '\u{0007}' {
            return next;
        }
        if character == '\u{1b}' && next < input.len() {
            let (terminator, after) = next_character(input, next);
            if terminator == '\\' {
                return after;
            }
        }
        index = next;
    }
    input.len()
}

fn skip_escape_sequence(input: &str, start: usize) -> usize {
    let mut index = start;
    while index < input.len() {
        let (character, next) = next_character(input, index);
        if character.is_ascii() && ('\u{20}'..='\u{2f}').contains(&character) {
            index = next;
            continue;
        }
        if character.is_ascii() && ('\u{30}'..='\u{7e}').contains(&character) {
            return next;
        }
        break;
    }
    index
}

fn apply_sgr(mut style: Style, body: &str) -> Style {
    if body.is_empty() {
        return Style::default();
    }
    if !body
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b';' | b':'))
    {
        return style;
    }
    let parameters: Vec<_> = body.split(';').collect();
    let mut index = 0;
    while index < parameters.len() {
        let parameter = parameters[index];
        if parameter.contains(':') {
            apply_colon_sgr(&mut style, parameter);
            index += 1;
            continue;
        }
        let Some(code) = parse_parameter(parameter) else {
            index += 1;
            continue;
        };
        if matches!(code, 38 | 48 | 58) {
            if let Some((color, consumed)) = semicolon_color(&parameters[index + 1..]) {
                set_color(&mut style, code, color);
                index = index.saturating_add(consumed + 1);
                continue;
            }
            index += 1;
            continue;
        }
        apply_basic_sgr(&mut style, code);
        index += 1;
    }
    style
}

fn parse_parameter(parameter: &str) -> Option<u16> {
    if parameter.is_empty() {
        Some(0)
    } else {
        parameter.parse().ok()
    }
}

fn semicolon_color(parameters: &[&str]) -> Option<(Color, usize)> {
    match parse_parameter(parameters.first()?)? {
        5 => {
            let index = parse_parameter(parameters.get(1)?)?;
            Some((Color::Indexed(u8::try_from(index).ok()?), 2))
        }
        2 => {
            let red = u8::try_from(parse_parameter(parameters.get(1)?)?).ok()?;
            let green = u8::try_from(parse_parameter(parameters.get(2)?)?).ok()?;
            let blue = u8::try_from(parse_parameter(parameters.get(3)?)?).ok()?;
            Some((Color::Rgb { red, green, blue }, 4))
        }
        _ => None,
    }
}

fn apply_colon_sgr(style: &mut Style, parameter: &str) {
    let parts: Vec<_> = parameter.split(':').collect();
    let Some(code) = parts.first().and_then(|part| parse_parameter(part)) else {
        return;
    };
    if code == 4 {
        style.underline = parts.get(1).is_none_or(|variant| *variant != "0");
        return;
    }
    if !matches!(code, 38 | 48 | 58) {
        apply_basic_sgr(style, code);
        return;
    }
    let Some(mode) = parts.get(1).and_then(|part| parse_parameter(part)) else {
        return;
    };
    let color = match mode {
        5 => parts
            .get(2)
            .and_then(|part| parse_parameter(part))
            .and_then(|index| u8::try_from(index).ok())
            .map(Color::Indexed),
        2 => {
            let components: Vec<_> = parts[2..]
                .iter()
                .filter_map(|part| (!part.is_empty()).then(|| parse_parameter(part)).flatten())
                .collect();
            let components = components.get(components.len().saturating_sub(3)..);
            components.and_then(|components| match components {
                [red, green, blue] => Some(Color::Rgb {
                    red: u8::try_from(*red).ok()?,
                    green: u8::try_from(*green).ok()?,
                    blue: u8::try_from(*blue).ok()?,
                }),
                _ => None,
            })
        }
        _ => None,
    };
    if let Some(color) = color {
        set_color(style, code, color);
    }
}

fn apply_basic_sgr(style: &mut Style, code: u16) {
    match code {
        0 => *style = Style::default(),
        1 => style.bold = true,
        2 => style.dim = true,
        3 => style.italic = true,
        4 | 21 => style.underline = true,
        5 | 6 => style.blink = true,
        7 => style.reverse = true,
        8 => style.hidden = true,
        9 => style.strikethrough = true,
        22 => {
            style.bold = false;
            style.dim = false;
        }
        23 => style.italic = false,
        24 => style.underline = false,
        25 => style.blink = false,
        27 => style.reverse = false,
        28 => style.hidden = false,
        29 => style.strikethrough = false,
        30..=37 => style.foreground = Color::Indexed((code - 30) as u8),
        39 => style.foreground = Color::Default,
        40..=47 => style.background = Color::Indexed((code - 40) as u8),
        49 => style.background = Color::Default,
        59 => style.underline_color = None,
        90..=97 => style.foreground = Color::Indexed((code - 90 + 8) as u8),
        100..=107 => style.background = Color::Indexed((code - 100 + 8) as u8),
        _ => {}
    }
}

fn set_color(style: &mut Style, code: u16, color: Color) {
    match code {
        38 => style.foreground = color,
        48 => style.background = color,
        58 => style.underline_color = Some(color),
        _ => {}
    }
}

fn next_character(input: &str, index: usize) -> (char, usize) {
    let character = input[index..]
        .chars()
        .next()
        .expect("index is before the UTF-8 string end");
    (character, index + character.len_utf8())
}

fn push_span(spans: &mut Vec<TextSpan>, text: &mut String, style: Style) {
    if text.is_empty() {
        return;
    }
    spans.push(TextSpan::new(std::mem::take(text), style));
}

#[cfg(test)]
mod tests {
    use crate::fixture_support;

    use super::*;

    #[test]
    fn sgr_styles_are_parsed_and_non_display_controls_are_removed() {
        let spans = parse_ansi_spans(
            "plain\x1b[31;1mred\x1b[0m!\x1b]8;;https://invalid.example\x07link\x1b]8;;\x07\x1b[2Jdone\u{0008}",
        );

        assert_eq!(
            spans.iter().map(TextSpan::text).collect::<String>(),
            "plainred!linkdone"
        );
        assert_eq!(spans[1].style().foreground, Color::Indexed(1));
        assert!(spans[1].style().bold);
        assert!(!spans.iter().any(|span| span.text().contains('\u{1b}')));
    }

    #[test]
    fn indexed_rgb_colon_and_attribute_resets_are_supported() {
        let spans =
            parse_ansi_spans("\x1b[38;5;200mA\x1b[48;2;1;2;3mB\x1b[38:2::4:5:6;3mC\x1b[23;39;49mD");

        assert_eq!(spans[0].style().foreground, Color::Indexed(200));
        assert_eq!(
            spans[1].style().background,
            Color::Rgb {
                red: 1,
                green: 2,
                blue: 3,
            }
        );
        assert_eq!(
            spans[2].style().foreground,
            Color::Rgb {
                red: 4,
                green: 5,
                blue: 6,
            }
        );
        assert!(spans[2].style().italic);
        assert_eq!(spans[3].style(), Style::default());
    }

    #[test]
    fn parsing_matches_shared_fixtures() {
        let Some(records) = fixture_support::load(
            "text/ansi.txt",
            "ansi-text",
            &["input", "text", "foregrounds", "backgrounds", "attributes"],
        ) else {
            return;
        };
        for record in records {
            let spans = parse_ansi_spans(&record.text("input"));
            assert_eq!(
                spans.iter().map(TextSpan::text).collect::<String>(),
                record.text("text"),
                "case {} text",
                record.id
            );
            assert_eq!(
                spans
                    .iter()
                    .map(|span| color_signature(span.style().foreground))
                    .collect::<Vec<_>>(),
                record.field("foregrounds").split('|').collect::<Vec<_>>(),
                "case {} foregrounds",
                record.id
            );
            assert_eq!(
                spans
                    .iter()
                    .map(|span| color_signature(span.style().background))
                    .collect::<Vec<_>>(),
                record.field("backgrounds").split('|').collect::<Vec<_>>(),
                "case {} backgrounds",
                record.id
            );
            assert_eq!(
                spans
                    .iter()
                    .map(|span| attribute_signature(span.style()))
                    .collect::<Vec<_>>(),
                record.field("attributes").split('|').collect::<Vec<_>>(),
                "case {} attributes",
                record.id
            );
        }
    }

    fn color_signature(color: Color) -> String {
        match color {
            Color::Default => "default".to_owned(),
            Color::Indexed(index) => format!("index:{index}"),
            Color::Rgb { red, green, blue } => format!("rgb:{red},{green},{blue}"),
        }
    }

    fn attribute_signature(style: Style) -> String {
        let mut attributes = Vec::new();
        if style.bold {
            attributes.push("bold");
        }
        if style.dim {
            attributes.push("dim");
        }
        if style.italic {
            attributes.push("italic");
        }
        if style.underline {
            attributes.push("underline");
        }
        if style.blink {
            attributes.push("blink");
        }
        if style.reverse {
            attributes.push("reverse");
        }
        if style.hidden {
            attributes.push("hidden");
        }
        if style.strikethrough {
            attributes.push("strikethrough");
        }
        if attributes.is_empty() {
            "-".to_owned()
        } else {
            attributes.join("+")
        }
    }
}
