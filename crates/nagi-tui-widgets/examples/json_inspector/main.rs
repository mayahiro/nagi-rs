//! Application-owned JSON inspection state and copy policy

use nagi_tui::{
    App, Effect, Event, EventAction, Length, MouseTracking, Node, Style, TerminalOptions,
    run_terminal,
};
use nagi_tui_widgets::{
    JsonDocument, JsonInspector, JsonInspectorCopyRequest, JsonInspectorState, JsonMember,
    JsonNumber, JsonValue,
};

enum Message {
    State(JsonInspectorState),
    Copy(JsonInspectorCopyRequest),
}

struct JsonInspectorExample {
    document: JsonDocument,
    state: JsonInspectorState,
    copied: Option<String>,
}

impl JsonInspectorExample {
    fn new() -> Self {
        let limits = JsonValue::object([
            JsonMember::new(
                "max_nodes",
                JsonValue::number(JsonNumber::new("100000").expect("static JSON number")),
            ),
            JsonMember::new(
                "max_depth",
                JsonValue::number(JsonNumber::new("128").expect("static JSON number")),
            ),
        ])
        .expect("static object keys are unique");
        let root = JsonValue::object([
            JsonMember::new("component", JsonValue::string("JsonInspector")),
            JsonMember::new("enabled", JsonValue::boolean(true)),
            JsonMember::new(
                "features",
                JsonValue::array([
                    JsonValue::string("controlled selection"),
                    JsonValue::string("bounded preview"),
                    JsonValue::string("complete copy payload"),
                ]),
            ),
            JsonMember::new("limits", limits),
            JsonMember::new("optional", JsonValue::null()),
        ])
        .expect("static object keys are unique");
        Self {
            document: JsonDocument::new(root).expect("static document is within default limits"),
            state: JsonInspectorState::default(),
            copied: None,
        }
    }
}

impl App for JsonInspectorExample {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::State(state) => self.state = state,
            Message::Copy(request) => {
                let path = if request.path().as_str().is_empty() {
                    "$"
                } else {
                    request.path().as_str()
                };
                self.copied = Some(format!("Copy {path}: {}", request.text()));
            }
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let copied = self
            .copied
            .as_deref()
            .unwrap_or("Control-C requests the complete selected value");
        Node::border(
            Node::column([
                Node::styled_text(
                    "JsonInspector",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
                JsonInspector::new(
                    "inspector",
                    self.document.clone(),
                    self.state.clone(),
                    Message::State,
                )
                .maximum_scalar_graphemes(24)
                .on_copy(Message::Copy)
                .into_node()
                .with_length(Length::Flex(1)),
                Node::text(copied).with_length(Length::Fixed(1)),
                Node::text("Arrows navigate, Enter toggles, Esc or Q exits")
                    .with_length(Length::Fixed(1)),
            ]),
            Style::default(),
        )
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    let options = TerminalOptions {
        mouse_tracking: Some(MouseTracking::Press),
        focus_first: true,
        ..TerminalOptions::default()
    };
    run_terminal(JsonInspectorExample::new(), options, |event| match event {
        Event::Key(key)
            if key.code == nagi_tui::KeyCode::Escape
                || key.code == nagi_tui::KeyCode::Character('q') =>
        {
            EventAction::Exit
        }
        _ => EventAction::Ignore,
    })?;
    Ok(())
}
