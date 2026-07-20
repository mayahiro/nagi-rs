//! Shared Core Node rendering conformance fixtures

mod support;

use nagi_surface::Cursor;
use nagi_text::WidthProfile;
use nagi_tui::{
    App, BorderKind, Color, Effect, HorizontalAlignment, Node, PanelOptions, PanelStyle,
    ParagraphOptions, Runtime, Size, Style, Surface, TextSpan, VirtualClock, WrapMode,
};

struct CoreNodeFixtureApp {
    case_id: String,
}

impl App for CoreNodeFixtureApp {
    type Message = String;

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        core_node_fixture_view(&self.case_id)
    }
}

#[test]
fn core_nodes_match_shared_surface_snapshots() {
    let Some(records) = support::load(
        "runtime/core-nodes.txt",
        "core-node-render",
        &["width", "height", "expected"],
    ) else {
        return;
    };
    for record in records {
        let mut runtime = Runtime::with_clock(
            CoreNodeFixtureApp {
                case_id: record.id.clone(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(
                number(record.field("width")),
                number(record.field("height")),
            )),
            VirtualClock::new(),
        )
        .unwrap();

        let frame = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(
            frame.surface().snapshot(),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

fn core_node_fixture_view(case_id: &str) -> Node<String> {
    match case_id {
        "rich-text" => Node::paragraph(
            [
                TextSpan::new(
                    "Hel",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                ),
                TextSpan::new(
                    "lo world",
                    Style {
                        italic: true,
                        ..Style::default()
                    },
                ),
            ],
            ParagraphOptions::default(),
        ),
        "paragraph-center" => Node::paragraph(
            [TextSpan::new(
                "A日",
                Style {
                    underline: true,
                    ..Style::default()
                },
            )],
            ParagraphOptions {
                wrap: WrapMode::Hard,
                alignment: HorizontalAlignment::Center,
            },
        ),
        "surface-node" => {
            let mut source = Surface::transparent(3, 1).unwrap();
            source.write(
                0,
                0,
                "日A",
                Style {
                    bold: true,
                    ..Style::default()
                },
                WidthProfile::MODERN,
            );
            source.fill_transparent(
                2,
                0,
                1,
                1,
                Style {
                    underline: true,
                    ..Style::default()
                },
            );
            assert!(source.set_cursor(Some(Cursor::new(2, 0))));
            Node::stack([Node::text("xyz"), Node::surface(source)])
        }
        "panel-layout" => Node::panel_with_options(
            Node::column([
                Node::row([Node::text("A"), Node::gap(2), Node::text("B")]),
                Node::spacer(1, 1),
                Node::text("C"),
            ]),
            "Panel",
            PanelOptions {
                border: BorderKind::Double,
                style: PanelStyle {
                    background: Style {
                        background: Color::Indexed(4),
                        ..Style::default()
                    },
                    ..PanelStyle::default()
                },
                ..PanelOptions::default()
            },
        ),
        unknown => panic!("unknown core node fixture {unknown}"),
    }
}

fn number(value: &str) -> u32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
}
