//! Shared Unicode text conformance fixtures

mod support;

use nagi_text::{
    WidthProfile, byte_at_cell, cell_at_byte, grapheme_boundaries, next_grapheme_boundary,
    normalize_utf8, previous_grapheme_boundary, text_width, truncate, wrap,
};

#[test]
fn graphemes_match_shared_fixtures() {
    check_grapheme_file("text/graphemes.txt", "text-graphemes");
}

#[test]
fn graphemes_match_unicode_conformance_data() {
    check_grapheme_file("text/grapheme-break-17.0.0.txt", "text-grapheme-break");
}

#[test]
fn widths_match_shared_fixtures() {
    let Some(records) = support::load("text/width.txt", "text-width", &["text", "modern", "cjk"])
    else {
        return;
    };

    for record in records {
        let text = record.text("text");
        assert_eq!(
            text_width(&text, WidthProfile::MODERN),
            number(record.field("modern")),
            "case {} modern",
            record.id
        );
        assert_eq!(
            text_width(&text, WidthProfile::CJK),
            number(record.field("cjk")),
            "case {} cjk",
            record.id
        );
    }
}

#[test]
fn truncation_matches_shared_fixtures() {
    let Some(records) = support::load(
        "text/truncate.txt",
        "text-truncate",
        &["text", "cells", "profile", "expected"],
    ) else {
        return;
    };

    for record in records {
        let text = record.text("text");
        let expected = record.text("expected");
        assert_eq!(
            truncate(
                &text,
                number(record.field("cells")),
                profile(record.field("profile")),
            ),
            expected,
            "case {}",
            record.id
        );
    }
}

#[test]
fn wrapping_matches_shared_fixtures() {
    let Some(records) = support::load(
        "text/wrap.txt",
        "text-wrap",
        &["text", "cells", "profile", "expected"],
    ) else {
        return;
    };

    for record in records {
        let text = record.text("text");
        let expected: Vec<_> = record
            .field("expected")
            .split('|')
            .map(|line| {
                support::text_value(line)
                    .unwrap_or_else(|error| panic!("case {}: {error}", record.id))
            })
            .collect();
        assert_eq!(
            wrap(
                &text,
                number(record.field("cells")),
                profile(record.field("profile")),
            ),
            expected,
            "case {}",
            record.id
        );
    }
}

#[test]
fn positions_match_shared_fixtures() {
    let Some(records) = support::load(
        "text/position.txt",
        "text-position",
        &["text", "byte", "cell"],
    ) else {
        return;
    };

    for record in records {
        let text = record.text("text");
        let byte = optional_number(record.field("byte"));
        let cell = optional_number(record.field("cell"));
        if let Some(byte) = byte {
            assert_eq!(
                cell_at_byte(&text, byte, WidthProfile::MODERN),
                cell,
                "case {} byte to cell",
                record.id
            );
        }
        if let Some(cell) = cell {
            assert_eq!(
                byte_at_cell(&text, cell, WidthProfile::MODERN),
                byte,
                "case {} cell to byte",
                record.id
            );
        }
    }
}

#[test]
fn cursor_movement_matches_shared_fixtures() {
    let Some(records) = support::load(
        "text/cursor.txt",
        "text-cursor",
        &["text", "offset", "previous", "next"],
    ) else {
        return;
    };

    for record in records {
        let text = record.text("text");
        let offset = number(record.field("offset"));
        assert_eq!(
            previous_grapheme_boundary(&text, offset),
            optional_number(record.field("previous")),
            "case {} previous",
            record.id
        );
        assert_eq!(
            next_grapheme_boundary(&text, offset),
            optional_number(record.field("next")),
            "case {} next",
            record.id
        );
    }
}

#[test]
fn invalid_utf8_matches_shared_fixtures() {
    let Some(records) = support::load(
        "text/invalid-utf8.txt",
        "text-invalid-utf8",
        &["input", "expected"],
    ) else {
        return;
    };

    for record in records {
        assert_eq!(
            normalize_utf8(&record.decoded("input")),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

fn check_grapheme_file(path: &str, suite: &str) {
    let Some(records) = support::load(path, suite, &["text", "boundaries"]) else {
        return;
    };
    for record in records {
        let text = record.text("text");
        assert_eq!(
            grapheme_boundaries(&text),
            numbers(record.field("boundaries")),
            "case {}",
            record.id
        );
    }
}

fn profile(value: &str) -> WidthProfile<'static> {
    match value {
        "modern" => WidthProfile::MODERN,
        "cjk" => WidthProfile::CJK,
        _ => panic!("unknown width profile {value}"),
    }
}

fn numbers(value: &str) -> Vec<usize> {
    value.split(',').map(number).collect()
}

fn number(value: &str) -> usize {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
}

fn optional_number(value: &str) -> Option<usize> {
    (value != "none").then(|| number(value))
}
