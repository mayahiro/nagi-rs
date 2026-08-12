//! Shared Content-to-Node projection conformance fixtures

mod support;

use nagi_content::{
    AnnotationId, Content, Element, ElementId, ElementKind, Role, SemanticBoundary,
};
use nagi_tui::{
    App, ContentProjectionLimits, ContentProjectionOptions, DeclarationValue, Effect,
    HorizontalAlignment, Length, Node, PresentationDeclaration, PresentationDisplay,
    PresentationRule, PresentationSelector, PresentationSheet, PresentationState, Runtime, Size,
    Style, TextStyleDeclaration, VirtualClock, WrapMode, project_content_with_states,
};

struct ProjectionFixture {
    content: Content,
    sheet: PresentationSheet,
    options: ContentProjectionOptions,
    states: Vec<PresentationState>,
}

struct ProjectionFixtureApp {
    fixture: ProjectionFixture,
}

impl App for ProjectionFixtureApp {
    type Message = ();

    fn update(&mut self, (): ()) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        project_content_with_states(
            &self.fixture.content,
            &self.fixture.sheet,
            self.fixture.options,
            |_| self.fixture.states.as_slice(),
        )
        .expect("successful projection fixture remains valid")
    }
}

#[test]
fn content_projection_matches_shared_fixtures() {
    let Some(records) = support::load(
        "presentation/projection.txt",
        "content-node-projection",
        &[
            "width",
            "height",
            "expected-kind",
            "expected-depth",
            "expected",
        ],
    ) else {
        return;
    };

    for record in records {
        let fixture = projection_fixture(&record.id);
        let projected = project_content_with_states::<(), _>(
            &fixture.content,
            &fixture.sheet,
            fixture.options,
            |_| fixture.states.as_slice(),
        );
        let expected_kind = record.field("expected-kind");
        if expected_kind != "ok" {
            let error = projected
                .err()
                .unwrap_or_else(|| panic!("case {} unexpectedly succeeded", record.id));
            assert_eq!(error.kind().as_str(), expected_kind, "case {}", record.id);
            assert_eq!(
                error.depth(),
                number(record.field("expected-depth")),
                "case {}",
                record.id
            );
            continue;
        }
        projected.unwrap_or_else(|error| panic!("case {} failed: {error}", record.id));

        let mut runtime = Runtime::with_clock(
            ProjectionFixtureApp { fixture },
            nagi_tui::RuntimeConfig::new(Size::new(
                number(record.field("width")),
                number(record.field("height")),
            )),
            VirtualClock::new(),
        )
        .unwrap();
        let frame = runtime.render_if_dirty().unwrap().unwrap();
        let actual = frame.surface().snapshot();
        assert_eq!(actual, record.text("expected"), "case {}", record.id);
    }
}

fn projection_fixture(case_id: &str) -> ProjectionFixture {
    match case_id {
        "paragraph-inline-style" => {
            let paragraph = role("paragraph");
            let emphasis = role("emphasis");
            let inline = Element::new(ElementKind::Inline, [Content::text("日")])
                .with_roles([emphasis.clone()])
                .unwrap();
            let root = Element::new(
                ElementKind::Paragraph,
                [
                    Content::text("A"),
                    inline.into_content(),
                    Content::hard_break(),
                    Content::text("B"),
                ],
            )
            .with_roles([paragraph.clone()])
            .unwrap()
            .into_content();
            let rules = [
                PresentationRule::new(
                    PresentationSelector::Role(paragraph),
                    PresentationDeclaration::default()
                        .with_visual_separator(DeclarationValue::Set("|".to_owned()))
                        .with_wrap(DeclarationValue::Set(WrapMode::Hard))
                        .with_alignment(DeclarationValue::Set(HorizontalAlignment::Center)),
                ),
                PresentationRule::new(
                    PresentationSelector::Role(emphasis),
                    PresentationDeclaration::default().with_text_style(
                        TextStyleDeclaration::default()
                            .with_bold(DeclarationValue::Set(false))
                            .with_underline(DeclarationValue::Set(true)),
                    ),
                ),
            ];
            ProjectionFixture {
                content: root,
                sheet: PresentationSheet::new(rules),
                options: ContentProjectionOptions::default().with_base_style(Style {
                    bold: true,
                    ..Style::default()
                }),
                states: Vec::new(),
            }
        }
        "flow-boundaries" => {
            let layout = role("layout");
            let root = Element::new(
                ElementKind::Flow,
                [
                    Element::new(ElementKind::Paragraph, [Content::text("A")]).into_content(),
                    Element::new(ElementKind::Inline, [Content::text("B")]).into_content(),
                    Element::new(ElementKind::Paragraph, [Content::text("C")]).into_content(),
                ],
            )
            .with_roles([layout.clone()])
            .unwrap()
            .into_content();
            let rule = PresentationRule::new(
                PresentationSelector::Role(layout),
                PresentationDeclaration::default()
                    .with_gap(DeclarationValue::Set(1))
                    .with_visual_separator(DeclarationValue::Set("-".to_owned()))
                    .with_text_style(
                        TextStyleDeclaration::default().with_dim(DeclarationValue::Set(true)),
                    ),
            );
            ProjectionFixture {
                content: root,
                sheet: PresentationSheet::new([rule]),
                options: ContentProjectionOptions::default(),
                states: Vec::new(),
            }
        }
        "sequence-boundaries" => {
            let layout = role("layout");
            let root = Element::new(
                ElementKind::Sequence,
                [
                    Content::text("A"),
                    Element::new(ElementKind::Paragraph, [Content::text("B")]).into_content(),
                    Element::new(ElementKind::Inline, [Content::text("C")]).into_content(),
                ],
            )
            .with_roles([layout.clone()])
            .unwrap()
            .into_content();
            let rule = PresentationRule::new(
                PresentationSelector::Role(layout),
                PresentationDeclaration::default()
                    .with_gap(DeclarationValue::Set(1))
                    .with_visual_separator(DeclarationValue::Set("|".to_owned())),
            );
            ProjectionFixture {
                content: root,
                sheet: PresentationSheet::new([rule]),
                options: ContentProjectionOptions::default(),
                states: Vec::new(),
            }
        }
        "state-display-override" => {
            let override_role = role("override");
            let selected = PresentationState::new("selected").unwrap();
            let root = Element::new(
                ElementKind::Flow,
                [
                    Element::new(ElementKind::Paragraph, [Content::text("A")]).into_content(),
                    Element::new(ElementKind::Paragraph, [Content::text("B")]).into_content(),
                ],
            )
            .with_roles([override_role.clone()])
            .unwrap()
            .into_content();
            let rule = PresentationRule::new(
                PresentationSelector::Role(override_role),
                PresentationDeclaration::default()
                    .with_display(DeclarationValue::Set(PresentationDisplay::Sequence))
                    .with_visual_separator(DeclarationValue::Set("/".to_owned())),
            )
            .with_required_states([selected.clone()])
            .unwrap();
            ProjectionFixture {
                content: root,
                sheet: PresentationSheet::new([rule]),
                options: ContentProjectionOptions::default().with_base_style(Style {
                    italic: true,
                    ..Style::default()
                }),
                states: vec![selected],
            }
        }
        "identity-annotation-boundary" => {
            let id = ElementId::new("same").unwrap();
            let annotation = AnnotationId::new("action").unwrap();
            let child = |value| {
                Element::new(ElementKind::Paragraph, [Content::text(value)])
                    .with_id(id.clone())
                    .with_annotation(annotation.clone())
                    .into_content()
            };
            ProjectionFixture {
                content: Element::new(ElementKind::Flow, [child("A"), child("B")]).into_content(),
                sheet: PresentationSheet::default(),
                options: ContentProjectionOptions::default(),
                states: Vec::new(),
            }
        }
        "paragraph-length" => {
            let tall = role("tall");
            let first = Element::new(ElementKind::Paragraph, [Content::text("A")])
                .with_roles([tall.clone()])
                .unwrap();
            let rule = PresentationRule::new(
                PresentationSelector::Role(tall),
                PresentationDeclaration::default()
                    .with_length(DeclarationValue::Set(Length::Fixed(2))),
            );
            ProjectionFixture {
                content: Element::new(
                    ElementKind::Flow,
                    [
                        first.into_content(),
                        Element::new(ElementKind::Paragraph, [Content::text("B")]).into_content(),
                    ],
                )
                .into_content(),
                sheet: PresentationSheet::new([rule]),
                options: ContentProjectionOptions::default(),
                states: Vec::new(),
            }
        }
        "inline-layout-ignored" => {
            let inline_role = role("inline-layout");
            let inline = Element::new(ElementKind::Inline, [Content::text("A")])
                .with_roles([inline_role.clone()])
                .unwrap();
            let rule = PresentationRule::new(
                PresentationSelector::Role(inline_role),
                PresentationDeclaration::default()
                    .with_length(DeclarationValue::Set(Length::Fixed(3)))
                    .with_gap(DeclarationValue::Set(3))
                    .with_wrap(DeclarationValue::Set(WrapMode::None))
                    .with_alignment(DeclarationValue::Set(HorizontalAlignment::End)),
            );
            ProjectionFixture {
                content: Element::new(
                    ElementKind::Flow,
                    [
                        inline.into_content(),
                        Element::new(ElementKind::Paragraph, [Content::text("B")]).into_content(),
                    ],
                )
                .into_content(),
                sheet: PresentationSheet::new([rule]),
                options: ContentProjectionOptions::default(),
                states: Vec::new(),
            }
        }
        "semantic-boundary-ignored" => ProjectionFixture {
            content: Element::new(
                ElementKind::Paragraph,
                [Content::text("A"), Content::text("B")],
            )
            .with_boundary(SemanticBoundary::Space)
            .into_content(),
            sheet: PresentationSheet::default(),
            options: ContentProjectionOptions::default(),
            states: Vec::new(),
        },
        "root-text" => ProjectionFixture {
            content: Content::text("A"),
            sheet: PresentationSheet::default(),
            options: ContentProjectionOptions::default(),
            states: Vec::new(),
        },
        "invalid-inline-block" => ProjectionFixture {
            content: Element::new(
                ElementKind::Paragraph,
                [Element::new(ElementKind::Flow, []).into_content()],
            )
            .into_content(),
            sheet: PresentationSheet::default(),
            options: ContentProjectionOptions::default(),
            states: Vec::new(),
        },
        "content-node-limit" => {
            limited_paragraph(ContentProjectionLimits::default().with_max_content_nodes(2))
        }
        "output-node-limit" => ProjectionFixture {
            content: Element::new(ElementKind::Flow, [Content::text("A"), Content::text("B")])
                .into_content(),
            sheet: PresentationSheet::default(),
            options: ContentProjectionOptions::default()
                .with_limits(ContentProjectionLimits::default().with_max_output_nodes(1)),
            states: Vec::new(),
        },
        "span-limit" => limited_paragraph(ContentProjectionLimits::default().with_max_spans(1)),
        "depth-limit" => limited_paragraph(ContentProjectionLimits::default().with_max_depth(1)),
        "visual-byte-limit" => {
            limited_paragraph(ContentProjectionLimits::default().with_max_visual_bytes(1))
        }
        _ => panic!("unknown projection fixture {case_id}"),
    }
}

fn limited_paragraph(limits: ContentProjectionLimits) -> ProjectionFixture {
    ProjectionFixture {
        content: Element::new(
            ElementKind::Paragraph,
            [Content::text("A"), Content::text("B")],
        )
        .into_content(),
        sheet: PresentationSheet::default(),
        options: ContentProjectionOptions::default().with_limits(limits),
        states: Vec::new(),
    }
}

fn role(value: &str) -> Role {
    Role::new(value).unwrap()
}

fn number(value: &str) -> u32 {
    value.parse().unwrap()
}
