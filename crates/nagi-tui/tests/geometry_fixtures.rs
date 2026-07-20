//! Shared geometry conformance fixtures

mod support;

use nagi_tui::{Point, Rect};

#[test]
fn contains_matches_shared_fixtures() {
    let Some(records) = support::load(
        "geometry/contains.txt",
        "geometry-contains",
        &["rect", "point", "contains"],
    ) else {
        return;
    };

    for record in records {
        let rect = rect(record.field("rect"));
        let point = point(record.field("point"));
        let expected = boolean(record.field("contains"));
        assert_eq!(rect.contains(point), expected, "case {}", record.id);
    }
}

#[test]
fn intersection_matches_shared_fixtures() {
    let Some(records) = support::load(
        "geometry/intersection.txt",
        "geometry-intersection",
        &["a", "b", "intersection"],
    ) else {
        return;
    };

    for record in records {
        let actual = rect(record.field("a")).intersection(rect(record.field("b")));
        assert_eq!(
            actual,
            rect(record.field("intersection")),
            "case {}",
            record.id
        );
    }
}

#[test]
fn point_translation_matches_shared_fixtures() {
    let Some(records) = support::load(
        "geometry/point-translate.txt",
        "geometry-point-translate",
        &["point", "delta", "translated"],
    ) else {
        return;
    };

    for record in records {
        let input = point(record.field("point"));
        let delta = point(record.field("delta"));
        let expected = optional(record.field("translated"), point);
        assert_eq!(
            input.translated(delta.x, delta.y),
            expected,
            "case {}",
            record.id
        );
    }
}

#[test]
fn rect_translation_matches_shared_fixtures() {
    let Some(records) = support::load(
        "geometry/rect-translate.txt",
        "geometry-rect-translate",
        &["rect", "delta", "translated"],
    ) else {
        return;
    };

    for record in records {
        let input = rect(record.field("rect"));
        let delta = point(record.field("delta"));
        let expected = optional(record.field("translated"), rect);
        assert_eq!(
            input.translated(delta.x, delta.y),
            expected,
            "case {}",
            record.id
        );
    }
}

fn point(value: &str) -> Point {
    let values = numbers(value, 2);
    Point::new(i32_value(values[0]), i32_value(values[1]))
}

fn rect(value: &str) -> Rect {
    let values = numbers(value, 4);
    Rect::new(
        i32_value(values[0]),
        i32_value(values[1]),
        u32_value(values[2]),
        u32_value(values[3]),
    )
}

fn numbers(value: &str, expected: usize) -> Vec<&str> {
    let values: Vec<_> = value.split(',').collect();
    assert_eq!(values.len(), expected, "invalid tuple {value}");
    values
}

fn i32_value(value: &str) -> i32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid i32 {value}: {error}"))
}

fn u32_value(value: &str) -> u32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid u32 {value}: {error}"))
}

fn boolean(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => panic!("invalid Boolean {value}"),
    }
}

fn optional<T>(value: &str, parse: impl FnOnce(&str) -> T) -> Option<T> {
    (value != "none").then(|| parse(value))
}
