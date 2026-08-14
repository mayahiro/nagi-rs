//! Shared terminal presentation rule conformance fixtures

mod support;

use nagi_content::{Class, Content, Element, ElementKind, Role};
use nagi_text::normalize_utf8;
use nagi_tui::{
    Color, ComputedPresentation, DeclarationValue, HorizontalAlignment, Length,
    PresentationDeclaration, PresentationDisplay, PresentationRule, PresentationSelector,
    PresentationSheet, PresentationState, Style, TextStyleDeclaration, WrapMode,
};

#[test]
fn presentation_states_match_shared_fixtures() {
    let Some(records) = support::load(
        "presentation/states.txt",
        "presentation-states",
        &["value", "expected"],
    ) else {
        return;
    };

    for record in records {
        let actual = match PresentationState::from_bytes(&record.decoded("value")) {
            Ok(_) => "ok",
            Err(error) => error.kind().as_str(),
        };
        assert_eq!(actual, record.field("expected"), "case {}", record.id);
    }
}

#[test]
fn presentation_selectors_match_shared_fixtures() {
    let Some(records) = support::load(
        "presentation/selectors.txt",
        "presentation-selectors",
        &[
            "selector", "roles", "classes", "required", "active", "expected",
        ],
    ) else {
        return;
    };

    for record in records {
        let element = Element::new(ElementKind::Inline, [])
            .with_roles(roles(record.field("roles")))
            .unwrap()
            .with_classes(classes(record.field("classes")))
            .unwrap();
        let rule = PresentationRule::new(
            selector(record.field("selector")),
            PresentationDeclaration::default().with_text_style(
                TextStyleDeclaration::default().with_bold(DeclarationValue::Set(true)),
            ),
        )
        .with_required_states(states(record.field("required")))
        .unwrap();
        let sheet = PresentationSheet::new([rule]);
        let computed = sheet.resolve(&element, Style::default(), &states(record.field("active")));
        let actual = if computed.style().bold {
            "match"
        } else {
            "miss"
        };

        assert_eq!(actual, record.field("expected"), "case {}", record.id);
    }
}

#[test]
fn presentation_cascade_matches_shared_fixtures() {
    let Some(records) = support::load(
        "presentation/cascade.txt",
        "presentation-cascade",
        &[
            "element",
            "roles",
            "classes",
            "states",
            "inherited",
            "rules",
            "expected",
        ],
    ) else {
        return;
    };

    for record in records {
        let element = Element::new(
            element_kind(record.field("element")),
            [Content::text("value")],
        )
        .with_roles(roles(record.field("roles")))
        .unwrap()
        .with_classes(classes(record.field("classes")))
        .unwrap();
        let sheet = sheet(record.field("rules"));
        let active_states = states(record.field("states"));
        let computed = sheet.resolve(
            &element,
            inherited_style(record.field("inherited")),
            &active_states,
        );

        assert_eq!(
            canonical_presentation(computed),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

fn selector(value: &str) -> PresentationSelector {
    if value == "any" {
        return PresentationSelector::Any;
    }
    if let Some(value) = value.strip_prefix("role:") {
        return PresentationSelector::Role(role(value));
    }
    if let Some(value) = value.strip_prefix("class:") {
        return PresentationSelector::Class(class(value));
    }
    panic!("unknown presentation selector {value}");
}

fn roles(value: &str) -> Vec<Role> {
    list(value).map(role).collect()
}

fn classes(value: &str) -> Vec<Class> {
    list(value).map(class).collect()
}

fn states(value: &str) -> Vec<PresentationState> {
    list(value)
        .map(|value| PresentationState::new(value).unwrap())
        .collect()
}

fn list(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').filter(|value| *value != "none")
}

fn role(value: &str) -> Role {
    Role::new(value).unwrap()
}

fn class(value: &str) -> Class {
    Class::new(value).unwrap()
}

fn element_kind(value: &str) -> ElementKind {
    match value {
        "inline" => ElementKind::Inline,
        "flow" => ElementKind::Flow,
        "paragraph" => ElementKind::Paragraph,
        "sequence" => ElementKind::Sequence,
        _ => panic!("unknown presentation element kind {value}"),
    }
}

fn sheet(value: &str) -> PresentationSheet {
    let rules = match value {
        "none" => Vec::new(),
        "universal" => vec![rule(
            PresentationSelector::Any,
            text(TextStyleDeclaration::default().with_bold(DeclarationValue::Set(true))),
        )],
        "heading" => vec![heading_rule()],
        "source-class" => vec![rule(
            PresentationSelector::Class(class("source.bold")),
            text(
                TextStyleDeclaration::default()
                    .with_foreground(DeclarationValue::Set(Color::Indexed(3)))
                    .with_bold(DeclarationValue::Set(true)),
            ),
        )],
        "stateful" => vec![
            rule(
                PresentationSelector::Role(role("item")),
                text(TextStyleDeclaration::default().with_reverse(DeclarationValue::Set(true))),
            )
            .with_required_states(states("selected,focused"))
            .unwrap(),
        ],
        "ordered" => ordered_rules(),
        "text-initial" => vec![rule(
            PresentationSelector::Role(role("reset")),
            text(initial_text_declaration()),
        )],
        "underline-default" => vec![rule(
            PresentationSelector::Role(role("underline")),
            text(
                TextStyleDeclaration::default()
                    .with_underline_color(DeclarationValue::Set(Some(Color::Default)))
                    .with_underline(DeclarationValue::Set(true)),
            ),
        )],
        "layout-set" => vec![layout_set_rule()],
        "layout-initial" => vec![
            layout_set_rule(),
            rule(
                PresentationSelector::Class(class("reset")),
                initial_layout_declaration(),
            ),
        ],
        "separator-empty" => vec![separator_rule(String::new())],
        "separator-invalid" => vec![separator_rule(normalize_utf8(b"\xFF").into_owned())],
        "unspecified" => vec![
            heading_rule(),
            rule(
                PresentationSelector::Any,
                PresentationDeclaration::default(),
            ),
        ],
        _ => panic!("unknown presentation rule arrangement {value}"),
    };
    PresentationSheet::new(rules)
}

fn heading_rule() -> PresentationRule {
    rule(
        PresentationSelector::Role(role("heading")),
        text(
            TextStyleDeclaration::default()
                .with_foreground(DeclarationValue::Set(Color::Indexed(6)))
                .with_bold(DeclarationValue::Set(true)),
        ),
    )
}

fn ordered_rules() -> Vec<PresentationRule> {
    vec![
        rule(
            PresentationSelector::Any,
            text(
                TextStyleDeclaration::default()
                    .with_foreground(DeclarationValue::Set(Color::Indexed(1)))
                    .with_bold(DeclarationValue::Set(true)),
            ),
        ),
        rule(
            PresentationSelector::Role(role("code")),
            text(
                TextStyleDeclaration::default()
                    .with_foreground(DeclarationValue::Set(Color::Indexed(2)))
                    .with_italic(DeclarationValue::Set(true)),
            ),
        ),
        rule(
            PresentationSelector::Class(class("source.bold")),
            text(
                TextStyleDeclaration::default()
                    .with_bold(DeclarationValue::Set(false))
                    .with_dim(DeclarationValue::Set(true)),
            ),
        ),
        rule(
            PresentationSelector::Role(role("code")),
            text(
                TextStyleDeclaration::default()
                    .with_foreground(DeclarationValue::Set(Color::Default))
                    .with_italic(DeclarationValue::Set(false)),
            ),
        )
        .with_required_states(states("disabled"))
        .unwrap(),
    ]
}

fn initial_text_declaration() -> TextStyleDeclaration {
    TextStyleDeclaration::default()
        .with_foreground(DeclarationValue::Initial)
        .with_background(DeclarationValue::Initial)
        .with_underline_color(DeclarationValue::Initial)
        .with_bold(DeclarationValue::Initial)
        .with_dim(DeclarationValue::Initial)
        .with_italic(DeclarationValue::Initial)
        .with_underline(DeclarationValue::Initial)
        .with_blink(DeclarationValue::Initial)
        .with_reverse(DeclarationValue::Initial)
        .with_hidden(DeclarationValue::Initial)
        .with_strikethrough(DeclarationValue::Initial)
}

fn layout_set_rule() -> PresentationRule {
    rule(
        PresentationSelector::Role(role("layout")),
        PresentationDeclaration::default()
            .with_display(DeclarationValue::Set(PresentationDisplay::Sequence))
            .with_length(DeclarationValue::Set(Length::Fixed(3)))
            .with_gap(DeclarationValue::Set(2))
            .with_visual_separator(DeclarationValue::Set(" | ".to_owned()))
            .with_wrap(DeclarationValue::Set(WrapMode::None))
            .with_alignment(DeclarationValue::Set(HorizontalAlignment::End)),
    )
}

fn separator_rule(separator: String) -> PresentationRule {
    rule(
        PresentationSelector::Role(role("separator")),
        PresentationDeclaration::default().with_visual_separator(DeclarationValue::Set(separator)),
    )
}

fn initial_layout_declaration() -> PresentationDeclaration {
    PresentationDeclaration::default()
        .with_display(DeclarationValue::Initial)
        .with_length(DeclarationValue::Initial)
        .with_gap(DeclarationValue::Initial)
        .with_visual_separator(DeclarationValue::Initial)
        .with_wrap(DeclarationValue::Initial)
        .with_alignment(DeclarationValue::Initial)
}

fn rule(selector: PresentationSelector, declaration: PresentationDeclaration) -> PresentationRule {
    PresentationRule::new(selector, declaration)
}

fn text(text_style: TextStyleDeclaration) -> PresentationDeclaration {
    PresentationDeclaration::default().with_text_style(text_style)
}

fn inherited_style(value: &str) -> Style {
    match value {
        "default" => Style::default(),
        "accent" => Style {
            foreground: Color::Indexed(2),
            background: Color::Indexed(4),
            underline_color: Some(Color::Indexed(3)),
            bold: true,
            italic: true,
            ..Style::default()
        },
        _ => panic!("unknown inherited presentation style {value}"),
    }
}

fn canonical_presentation(computed: ComputedPresentation<'_>) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        display_name(computed.display()),
        length_name(computed.length()),
        computed.gap(),
        visual_separator_name(computed.visual_separator()),
        wrap_name(computed.wrap()),
        alignment_name(computed.alignment()),
        style_name(computed.style()),
    )
}

fn visual_separator_name(separator: Option<&str>) -> String {
    let Some(separator) = separator else {
        return "absent".to_owned();
    };
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity("present:".len() + separator.len() * 2);
    encoded.push_str("present:");
    for byte in separator.bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0F)]));
    }
    encoded
}

fn display_name(value: PresentationDisplay) -> &'static str {
    match value {
        PresentationDisplay::Inline => "inline",
        PresentationDisplay::Flow => "flow",
        PresentationDisplay::Paragraph => "paragraph",
        PresentationDisplay::Sequence => "sequence",
    }
}

fn length_name(value: Length) -> String {
    match value {
        Length::Auto => "auto".to_owned(),
        Length::Fixed(value) => format!("fixed:{value}"),
        Length::Flex(value) => format!("flex:{value}"),
        Length::Percent(value) => format!("percent:{value}"),
        Length::MinMax {
            min,
            preferred,
            max,
        } => format!("minmax:{min}:{preferred}:{max}"),
    }
}

fn wrap_name(value: WrapMode) -> &'static str {
    match value {
        WrapMode::Word => "word",
        WrapMode::Hard => "hard",
        WrapMode::None => "none",
    }
}

fn alignment_name(value: HorizontalAlignment) -> &'static str {
    match value {
        HorizontalAlignment::Start => "start",
        HorizontalAlignment::Center => "center",
        HorizontalAlignment::End => "end",
    }
}

fn style_name(value: Style) -> &'static str {
    if value == Style::default() {
        "default"
    } else if value == inherited_style("accent") {
        "accent"
    } else if value
        == (Style {
            bold: true,
            ..Style::default()
        })
    {
        "bold"
    } else if value
        == (Style {
            foreground: Color::Indexed(6),
            bold: true,
            ..Style::default()
        })
    {
        "heading"
    } else if value
        == (Style {
            foreground: Color::Indexed(3),
            bold: true,
            ..Style::default()
        })
    {
        "source"
    } else if value
        == (Style {
            reverse: true,
            ..Style::default()
        })
    {
        "selected"
    } else if value
        == (Style {
            dim: true,
            ..Style::default()
        })
    {
        "ordered"
    } else if value
        == (Style {
            underline_color: Some(Color::Default),
            underline: true,
            ..Style::default()
        })
    {
        "underline-default"
    } else {
        panic!("unknown computed presentation style {value:?}");
    }
}
