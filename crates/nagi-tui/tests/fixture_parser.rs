//! Shared fixture parser diagnostics

mod support;

use std::path::Path;

#[test]
fn structural_errors_include_the_fixture_location() {
    let cases = [
        (
            "nagi-fixture-v1\tother\n",
            "fixture.txt:1: unsupported or mismatched header",
        ),
        (
            "nagi-fixture-v1\tsuite\ncase\tvalue=a\tvalue=b\n",
            "fixture.txt:2: duplicate field",
        ),
        (
            "nagi-fixture-v1\tsuite\ncase\tother=a\n",
            "fixture.txt:2: unknown field",
        ),
        (
            "nagi-fixture-v1\tsuite\ncase\n",
            "fixture.txt:2: missing field",
        ),
    ];

    for (input, expected) in cases {
        let error = match support::parse(Path::new("fixture.txt"), input, "suite", &["value"]) {
            Ok(_) => panic!("invalid fixture was accepted"),
            Err(error) => error,
        };
        assert_eq!(error, expected);
    }
}

#[test]
fn invalid_unicode_scalar_escape_is_rejected() {
    assert_eq!(
        support::decoded_value("\\u{D800}"),
        Err("invalid Unicode scalar value".to_owned())
    );
}
