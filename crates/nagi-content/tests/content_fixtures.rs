//! Shared source-neutral content conformance fixtures

mod support;

use nagi_content::{
    AnnotationId, Class, Content, Element, ElementId, ElementKind, IdentifierError,
    IdentifierErrorKind, Limits, Role, SemanticBoundary, Stats, ValidationErrorKind, semantic_text,
    validate,
};

#[test]
fn identifiers_match_shared_fixtures() {
    let Some(records) = support::load(
        "content/identifiers.txt",
        "content-identifiers",
        &["kind", "value", "expected"],
    ) else {
        return;
    };

    for record in records {
        let result = construct_identifier(record.field("kind"), &record.decoded("value"));
        let actual = match result {
            Ok(()) => "ok",
            Err(error) => error.kind().as_str(),
        };
        assert_eq!(actual, record.field("expected"), "case {}", record.id);
    }
}

#[test]
fn elements_match_shared_fixtures() {
    let Some(records) = support::load(
        "content/elements.txt",
        "content-elements",
        &[
            "kind",
            "boundary",
            "id",
            "revision",
            "roles",
            "classes",
            "annotation",
            "children",
        ],
    ) else {
        return;
    };

    for record in records {
        let element = element_case(&record.id);
        assert_eq!(
            element_kind_name(element.kind()),
            record.field("kind"),
            "case {} kind",
            record.id
        );
        assert_eq!(
            boundary_name(element.boundary()),
            record.field("boundary"),
            "case {} boundary",
            record.id
        );
        assert_eq!(
            element.id().map_or("none", ElementId::as_str),
            record.field("id"),
            "case {} id",
            record.id
        );
        assert_eq!(
            element.revision(),
            number(record.field("revision")),
            "case {} revision",
            record.id
        );
        assert_eq!(
            tokens(element.roles().iter().map(Role::as_str)),
            record.field("roles"),
            "case {} roles",
            record.id
        );
        assert_eq!(
            tokens(element.classes().iter().map(Class::as_str)),
            record.field("classes"),
            "case {} classes",
            record.id
        );
        assert_eq!(
            element.annotation().map_or("none", AnnotationId::as_str),
            record.field("annotation"),
            "case {} annotation",
            record.id
        );
        assert_eq!(
            u64::try_from(element.children().len()).unwrap(),
            number(record.field("children")),
            "case {} children",
            record.id
        );
    }
}

#[test]
fn semantic_projection_and_stats_match_shared_fixtures() {
    let Some(records) = support::load(
        "content/semantic.txt",
        "content-semantic",
        &[
            "expected",
            "annotations",
            "nodes",
            "depth",
            "semantic-bytes",
            "metadata-bytes",
            "annotation-count",
            "identified-count",
            "max-tokens",
        ],
    ) else {
        return;
    };

    for record in records {
        let content = semantic_case(&record.id);
        let projected = semantic_text(&content);
        assert_eq!(
            projected.text(),
            record.text("expected"),
            "case {} text",
            record.id
        );

        let annotations = projected
            .annotations()
            .iter()
            .map(|range| format!("{}:{}-{}", range.annotation(), range.start(), range.end()))
            .collect::<Vec<_>>();
        let annotations = if annotations.is_empty() {
            "none".to_owned()
        } else {
            annotations.join(",")
        };
        assert_eq!(
            annotations,
            record.field("annotations"),
            "case {} annotations",
            record.id
        );

        let stats = validate(&content, Limits::UNLIMITED).unwrap();
        assert_eq!(
            stats,
            Stats {
                nodes: number(record.field("nodes")),
                max_depth: number(record.field("depth")),
                semantic_bytes: number(record.field("semantic-bytes")),
                metadata_bytes: number(record.field("metadata-bytes")),
                annotations: number(record.field("annotation-count")),
                identified_elements: number(record.field("identified-count")),
                max_tokens_per_element: number(record.field("max-tokens")),
            },
            "case {} stats",
            record.id
        );
    }
}

#[test]
fn validation_matches_shared_fixtures() {
    let Some(records) = support::load(
        "content/validation.txt",
        "content-validation",
        &["limits", "expected"],
    ) else {
        return;
    };

    for record in records {
        let result = validate(&validation_case(&record.id), limits(record.field("limits")));
        let actual = match result {
            Ok(_) => "ok",
            Err(error) => error.kind().as_str(),
        };
        assert_eq!(actual, record.field("expected"), "case {}", record.id);
    }
}

fn construct_identifier(kind: &str, value: &[u8]) -> Result<(), IdentifierError> {
    match kind {
        "element" => ElementId::from_bytes(value).map(|_| ()),
        "role" => Role::from_bytes(value).map(|_| ()),
        "class" => Class::from_bytes(value).map(|_| ()),
        "annotation" => AnnotationId::from_bytes(value).map(|_| ()),
        _ => panic!("unknown identifier kind {kind}"),
    }
}

fn element_case(id: &str) -> Element {
    match id {
        "inline-default" => Element::new(ElementKind::Inline, []),
        "flow-default" => Element::new(ElementKind::Flow, []),
        "paragraph-default" => Element::new(ElementKind::Paragraph, []),
        "sequence-default" => Element::new(ElementKind::Sequence, []),
        "configured" => Element::new(
            ElementKind::Sequence,
            [Content::text("left"), Content::text("right")],
        )
        .with_id(element_id("document 1"))
        .with_revision(42)
        .with_roles([role("document"), role("diagnostic.code")])
        .unwrap()
        .with_classes([class("source.bold"), class("theme.high_contrast")])
        .unwrap()
        .with_annotation(annotation("copy.document"))
        .with_boundary(SemanticBoundary::Space),
        _ => panic!("unknown element case {id}"),
    }
}

fn element_kind_name(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::Inline => "inline",
        ElementKind::Flow => "flow",
        ElementKind::Paragraph => "paragraph",
        ElementKind::Sequence => "sequence",
    }
}

fn boundary_name(boundary: SemanticBoundary) -> &'static str {
    match boundary {
        SemanticBoundary::None => "none",
        SemanticBoundary::Space => "space",
        SemanticBoundary::Line => "line",
        SemanticBoundary::Tab => "tab",
    }
}

fn tokens<'a>(values: impl Iterator<Item = &'a str>) -> String {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.join(",")
    }
}

fn semantic_case(id: &str) -> Content {
    match id {
        "empty-text" => Content::text(""),
        "normalized-text" => Content::text_bytes(b"A\xFF\xFEB"),
        "hard-break" => Content::hard_break(),
        "default-boundaries" => Element::new(
            ElementKind::Flow,
            [
                Element::new(ElementKind::Paragraph, [Content::text("alpha")]).into_content(),
                Element::new(
                    ElementKind::Sequence,
                    [Content::text("left"), Content::text("right")],
                )
                .into_content(),
                Element::new(
                    ElementKind::Inline,
                    [Content::text("tail"), Content::text("!")],
                )
                .into_content(),
            ],
        )
        .into_content(),
        "boundary-overrides" => Element::new(
            ElementKind::Flow,
            [
                Element::new(
                    ElementKind::Paragraph,
                    [Content::text("A"), Content::text("B")],
                )
                .with_boundary(SemanticBoundary::Space)
                .into_content(),
                Element::new(ElementKind::Flow, [Content::text("C"), Content::text("D")])
                    .with_boundary(SemanticBoundary::None)
                    .into_content(),
            ],
        )
        .into_content(),
        "nested-annotations" => nested_annotations(),
        "empty-annotation" => Element::new(ElementKind::Inline, [])
            .with_annotation(annotation("marker.empty"))
            .into_content(),
        "parent-boundary-range" => Element::new(
            ElementKind::Sequence,
            [
                Content::text("a"),
                Element::new(ElementKind::Inline, [Content::text("b")])
                    .with_annotation(annotation("cell.middle"))
                    .into_content(),
                Content::text("c"),
            ],
        )
        .into_content(),
        _ => panic!("unknown semantic case {id}"),
    }
}

fn nested_annotations() -> Content {
    let emphasized = Element::new(ElementKind::Inline, [Content::text(" now")])
        .with_classes([class("source.bold")])
        .unwrap()
        .with_annotation(annotation("emphasis.one"))
        .into_content();
    let first = Element::new(ElementKind::Paragraph, [Content::text("Go"), emphasized])
        .with_id(element_id("p-1"))
        .with_roles([role("paragraph")])
        .unwrap()
        .with_annotation(annotation("link.one"))
        .into_content();
    let second = Element::new(ElementKind::Paragraph, [Content::text("Done")]).into_content();

    Element::new(ElementKind::Flow, [first, second])
        .with_id(element_id("doc 1"))
        .with_revision(7)
        .with_roles([role("document")])
        .unwrap()
        .with_classes([class("theme.base")])
        .unwrap()
        .with_annotation(annotation("copy.all"))
        .into_content()
}

fn validation_case(id: &str) -> Content {
    match id {
        "valid" => Element::new(ElementKind::Flow, [Content::text("ok")])
            .with_id(element_id("root"))
            .with_roles([role("document")])
            .unwrap()
            .with_classes([class("theme.base")])
            .unwrap()
            .with_annotation(annotation("copy.all"))
            .into_content(),
        "node-limit" => {
            Element::new(ElementKind::Flow, [Content::text("a"), Content::text("b")]).into_content()
        }
        "depth-limit" => Element::new(
            ElementKind::Inline,
            [Element::new(ElementKind::Inline, [Content::text("deep")]).into_content()],
        )
        .into_content(),
        "semantic-byte-limit" => Element::new(
            ElementKind::Sequence,
            [Content::text("a"), Content::text("b")],
        )
        .into_content(),
        "metadata-byte-limit" => Element::new(ElementKind::Inline, [])
            .with_id(element_id("abc"))
            .with_roles([role("heading")])
            .unwrap()
            .into_content(),
        "token-limit" => Element::new(ElementKind::Inline, [])
            .with_roles([role("a"), role("b")])
            .unwrap()
            .with_classes([class("c")])
            .unwrap()
            .into_content(),
        "duplicate-element-id" => Element::new(
            ElementKind::Flow,
            [
                Element::new(ElementKind::Paragraph, [Content::text("a")])
                    .with_id(element_id("same"))
                    .into_content(),
                Element::new(ElementKind::Paragraph, [Content::text("b")])
                    .with_id(element_id("same"))
                    .into_content(),
            ],
        )
        .into_content(),
        "limit-precedence" => Content::text("x"),
        _ => panic!("unknown validation case {id}"),
    }
}

fn limits(value: &str) -> Limits {
    let values = value.split(',').map(number).collect::<Vec<_>>();
    assert_eq!(values.len(), 5, "invalid limits {value}");
    Limits {
        max_depth: values[0],
        max_nodes: values[1],
        max_semantic_bytes: values[2],
        max_metadata_bytes: values[3],
        max_tokens_per_element: values[4],
    }
}

fn number(value: &str) -> u64 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
}

fn element_id(value: &str) -> ElementId {
    ElementId::new(value).unwrap()
}

fn role(value: &str) -> Role {
    Role::new(value).unwrap()
}

fn class(value: &str) -> Class {
    Class::new(value).unwrap()
}

fn annotation(value: &str) -> AnnotationId {
    AnnotationId::new(value).unwrap()
}

#[test]
fn stable_error_details_are_available() {
    let error = validate(
        &Content::text("x"),
        Limits {
            max_nodes: 0,
            ..Limits::UNLIMITED
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), ValidationErrorKind::NodeLimit);
    assert_eq!(error.observed(), Some(1));
    assert_eq!(error.limit_value(), Some(0));
    assert_eq!(error.element_id(), None);

    assert_eq!(
        Role::from_bytes(b"\xFF").unwrap_err().kind(),
        IdentifierErrorKind::InvalidUtf8
    );
}

#[test]
fn deep_traversal_does_not_use_the_call_stack() {
    let mut content = Content::text("deep");
    const DEPTH: u64 = 2_048;
    for _ in 1..DEPTH {
        content = Element::new(ElementKind::Inline, [content]).into_content();
    }
    assert_eq!(semantic_text(&content).text(), "deep");
    assert_eq!(
        validate(&content, Limits::UNLIMITED).unwrap().max_depth,
        DEPTH
    );
}
