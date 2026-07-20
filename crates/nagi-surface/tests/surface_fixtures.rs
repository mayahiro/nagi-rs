//! Shared surface conformance fixtures

mod support;

use nagi_surface::{ChangedRun, Cursor, Surface};
use nagi_text::WidthProfile;
use nagi_vt::{Color, Style};

#[test]
fn snapshots_match_shared_fixtures() {
    let Some(records) = support::load(
        "surface/snapshots.txt",
        "surface-snapshots",
        &[
            "width",
            "height",
            "background",
            "operations",
            "cursor",
            "expected",
        ],
    ) else {
        return;
    };

    for record in records {
        let surface = fixture_surface(
            record.field("width"),
            record.field("height"),
            record.field("background"),
            record.field("operations"),
            record.field("cursor"),
        );
        assert_eq!(
            surface.snapshot(),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

#[test]
fn composition_matches_shared_fixtures() {
    let Some(records) = support::load(
        "surface/composition.txt",
        "surface-composition",
        &[
            "width",
            "height",
            "base-background",
            "base",
            "base-cursor",
            "layer-width",
            "layer-height",
            "layer-background",
            "layer",
            "layer-cursor",
            "offset",
            "expected",
        ],
    ) else {
        return;
    };

    for record in records {
        let mut base = fixture_surface(
            record.field("width"),
            record.field("height"),
            record.field("base-background"),
            record.field("base"),
            record.field("base-cursor"),
        );
        let layer = fixture_surface(
            record.field("layer-width"),
            record.field("layer-height"),
            record.field("layer-background"),
            record.field("layer"),
            record.field("layer-cursor"),
        );
        let (offset_x, offset_y) = signed_pair(record.field("offset"));
        base.composite(&layer, offset_x, offset_y);
        assert_eq!(
            base.snapshot(),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

#[test]
fn changed_runs_match_shared_fixtures() {
    let Some(records) = support::load(
        "surface/diff.txt",
        "surface-diff",
        &["width", "height", "previous", "current", "expected"],
    ) else {
        return;
    };

    for record in records {
        let previous = fixture_surface(
            record.field("width"),
            record.field("height"),
            "opaque",
            record.field("previous"),
            "none",
        );
        let current = fixture_surface(
            record.field("width"),
            record.field("height"),
            "opaque",
            record.field("current"),
            "none",
        );
        assert_eq!(
            current.changed_runs(&previous),
            fixture_runs(record.field("expected")),
            "case {}",
            record.id
        );
    }
}

fn fixture_surface(
    width: &str,
    height: &str,
    background: &str,
    operations: &str,
    cursor: &str,
) -> Surface {
    let width = unsigned(width);
    let height = unsigned(height);
    let mut surface = match background {
        "opaque" => Surface::new(width, height),
        "transparent" => Surface::transparent(width, height),
        _ => panic!("unknown surface background {background}"),
    }
    .unwrap_or_else(|error| panic!("fixture surface allocation failed: {error}"));
    apply_operations(&mut surface, operations);
    if cursor != "none" {
        let (x, y) = unsigned_pair(cursor);
        assert!(surface.set_cursor(Some(Cursor::new(x, y))));
    }
    surface
}

fn apply_operations(surface: &mut Surface, operations: &str) {
    if operations == "-" {
        return;
    }
    for operation in operations.split(';') {
        let fields: Vec<_> = operation.split(',').collect();
        match fields.as_slice() {
            ["write", x, y, text, style] => surface.write(
                signed(x),
                signed(y),
                &scalar_text(text),
                fixture_style(style),
                WidthProfile::MODERN,
            ),
            ["fill", x, y, width, height, style] => surface.fill(
                signed(x),
                signed(y),
                unsigned(width),
                unsigned(height),
                fixture_style(style),
            ),
            ["fill-transparent", x, y, width, height, style] => surface.fill_transparent(
                signed(x),
                signed(y),
                unsigned(width),
                unsigned(height),
                fixture_style(style),
            ),
            _ => panic!("invalid surface operation {operation}"),
        }
    }
}

fn fixture_style(value: &str) -> Style {
    let mut style = Style::default();
    if value == "-" {
        return style;
    }
    for token in value.split('+') {
        if let Some(color) = token.strip_prefix("fg-") {
            style.foreground = fixture_color(color);
        } else if let Some(color) = token.strip_prefix("bg-") {
            style.background = fixture_color(color);
        } else if let Some(color) = token.strip_prefix("underline-color-") {
            style.underline_color = Some(fixture_color(color));
        } else {
            match token {
                "bold" => style.bold = true,
                "dim" => style.dim = true,
                "italic" => style.italic = true,
                "underline" => style.underline = true,
                "blink" => style.blink = true,
                "reverse" => style.reverse = true,
                "hidden" => style.hidden = true,
                "strikethrough" => style.strikethrough = true,
                _ => panic!("unknown fixture style {token}"),
            }
        }
    }
    style
}

fn fixture_color(value: &str) -> Color {
    if let Some(index) = value.strip_prefix("indexed-") {
        return Color::Indexed(
            index
                .parse()
                .unwrap_or_else(|error| panic!("invalid color index {index}: {error}")),
        );
    }
    if let Some(rgb) = value.strip_prefix("rgb-") {
        assert_eq!(rgb.len(), 6, "invalid RGB color {rgb}");
        return Color::Rgb {
            red: hexadecimal(&rgb[0..2]) as u8,
            green: hexadecimal(&rgb[2..4]) as u8,
            blue: hexadecimal(&rgb[4..6]) as u8,
        };
    }
    panic!("unknown fixture color {value}");
}

fn scalar_text(value: &str) -> String {
    value
        .split('+')
        .map(|scalar| {
            char::from_u32(hexadecimal(scalar))
                .unwrap_or_else(|| panic!("invalid Unicode scalar {scalar}"))
        })
        .collect()
}

fn fixture_runs(value: &str) -> Vec<ChangedRun> {
    if value == "none" {
        return Vec::new();
    }
    value
        .split(',')
        .map(|run| {
            let (row, columns) = run
                .split_once(':')
                .unwrap_or_else(|| panic!("invalid changed run {run}"));
            let (start, end) = columns
                .split_once('-')
                .unwrap_or_else(|| panic!("invalid changed run {run}"));
            ChangedRun {
                row: unsigned(row),
                start: unsigned(start),
                end: unsigned(end),
            }
        })
        .collect()
}

fn unsigned_pair(value: &str) -> (u32, u32) {
    let (left, right) = value
        .split_once(',')
        .unwrap_or_else(|| panic!("invalid unsigned pair {value}"));
    (unsigned(left), unsigned(right))
}

fn signed_pair(value: &str) -> (i32, i32) {
    let (left, right) = value
        .split_once(',')
        .unwrap_or_else(|| panic!("invalid signed pair {value}"));
    (signed(left), signed(right))
}

fn unsigned(value: &str) -> u32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid unsigned integer {value}: {error}"))
}

fn signed(value: &str) -> i32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid signed integer {value}: {error}"))
}

fn hexadecimal(value: &str) -> u32 {
    u32::from_str_radix(value, 16)
        .unwrap_or_else(|error| panic!("invalid hexadecimal integer {value}: {error}"))
}
