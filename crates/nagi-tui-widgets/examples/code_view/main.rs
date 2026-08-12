//! Application-owned styled code, layout memoization, and copy policy

use nagi_tui::{
    App, Color, Effect, Event, EventAction, Length, MouseTracking, Node, Style, TerminalOptions,
    TextSpan, run_terminal,
};
use nagi_tui_widgets::{
    CodeCopyRequest, CodeDocument, CodeLayoutCache, CodeLayoutOptions, CodeLine, CodeView,
    CodeViewState,
};

enum Message {
    State(CodeViewState),
    Copy(CodeCopyRequest),
    ToggleWrap,
}

struct CodeViewExample {
    document: CodeDocument,
    layout_cache: CodeLayoutCache,
    state: CodeViewState,
    wrap: bool,
    copied: Option<String>,
}

impl CodeViewExample {
    fn new() -> Self {
        let keyword = Style {
            foreground: Color::Indexed(5),
            bold: true,
            ..Style::default()
        };
        let string = Style {
            foreground: Color::Indexed(2),
            ..Style::default()
        };
        let comment = Style {
            dim: true,
            ..Style::default()
        };
        let lines = [
            CodeLine::styled([
                TextSpan::new("fn", keyword),
                TextSpan::new(" main() {", Style::default()),
            ]),
            CodeLine::styled([
                TextSpan::new("\tlet", keyword),
                TextSpan::new(" greeting = ", Style::default()),
                TextSpan::new("\"Hello, Nagi\"", string),
                TextSpan::new(";", Style::default()),
            ]),
            CodeLine::plain("\tprintln!(\"{greeting}\");"),
            CodeLine::styled([TextSpan::new(
                "\t// Styled spans are supplied by the application",
                comment,
            )]),
            CodeLine::plain("}"),
        ]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("static logical lines are valid");
        Self {
            document: CodeDocument::new(lines, true)
                .expect("static document is within default limits"),
            layout_cache: CodeLayoutCache::default(),
            state: CodeViewState::default(),
            wrap: false,
            copied: None,
        }
    }
}

impl App for CodeViewExample {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::State(state) => self.state = state,
            Message::Copy(request) => {
                self.copied = Some(format!(
                    "Copy lines {}..{}: {} bytes",
                    request.lines().start + 1,
                    request.lines().end,
                    request.text().len()
                ));
            }
            Message::ToggleWrap => self.wrap = !self.wrap,
        }
        Effect::none()
    }

    fn view(&self, context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let viewport_width = context.size.width.saturating_sub(2).max(1);
        let key = (u64::from(viewport_width) << 1) | u64::from(self.wrap);
        let layout = self
            .layout_cache
            .resolve(
                key,
                &self.document,
                CodeLayoutOptions::default()
                    .with_viewport_width(viewport_width)
                    .with_wrap(self.wrap)
                    .with_width_profile(context.width_profile),
            )
            .expect("static document projection is within default limits");
        let viewport_height = context.size.height.saturating_sub(5).max(1) as usize;
        let status = self
            .copied
            .as_deref()
            .unwrap_or("Control-C copies selected complete lines");
        Node::border(
            Node::column([
                Node::styled_text(
                    format!("CodeView  wrap={}", self.wrap),
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
                CodeView::new("code", layout, self.state, Message::State)
                    .viewport(viewport_height)
                    .on_copy(Message::Copy)
                    .into_node()
                    .with_length(Length::Flex(1)),
                Node::text(status).with_length(Length::Fixed(1)),
                Node::text("Arrows select and scroll, Shift-Up/Down extends, W toggles wrap")
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
    run_terminal(CodeViewExample::new(), options, |event| match event {
        Event::Key(key)
            if key.code == nagi_tui::KeyCode::Escape
                || key.code == nagi_tui::KeyCode::Character('q') =>
        {
            EventAction::Exit
        }
        Event::Key(key)
            if key.code == nagi_tui::KeyCode::Character('w')
                && key.modifiers == nagi_tui::Modifiers::NONE =>
        {
            EventAction::Message(Message::ToggleWrap)
        }
        Event::Text(text) if text == "w" => EventAction::Message(Message::ToggleWrap),
        _ => EventAction::Ignore,
    })?;
    Ok(())
}
