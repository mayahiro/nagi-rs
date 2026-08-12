//! Application-owned typed diff source, projection, and copy policy

use nagi_tui::{
    App, Effect, Event, EventAction, Length, MouseTracking, Node, Style, TerminalOptions,
    run_terminal,
};
use nagi_tui_widgets::{
    CodeLine, DiffCopyRequest, DiffDocument, DiffHunk, DiffLayoutCache, DiffLayoutOptions,
    DiffLine, DiffRange, DiffView, DiffViewState,
};

enum Message {
    State(DiffViewState),
    Copy(DiffCopyRequest),
    ToggleWrap,
}

struct DiffViewExample {
    document: DiffDocument,
    layout_cache: DiffLayoutCache,
    state: DiffViewState,
    wrap: bool,
    copied: Option<String>,
}

impl DiffViewExample {
    fn new() -> Self {
        let hunk = DiffHunk::new(
            DiffRange::new(1, 3).expect("static old range is valid"),
            DiffRange::new(1, 4).expect("static new range is valid"),
        );
        let lines = [
            DiffLine::metadata(line("diff --git a/src/main.rs b/src/main.rs")),
            DiffLine::metadata(line("--- a/src/main.rs")),
            DiffLine::metadata(line("+++ b/src/main.rs")),
            DiffLine::hunk(hunk, line("@@ -1,3 +1,4 @@")),
            DiffLine::context(1, 1, line("fn main() {")).expect("static numbers are valid"),
            DiffLine::deletion(2, line("\tprintln!(\"old\");")).expect("static number is valid"),
            DiffLine::addition(2, line("\tlet message = \"Hello, Nagi\";"))
                .expect("static number is valid"),
            DiffLine::addition(3, line("\tprintln!(\"{message}\");"))
                .expect("static number is valid"),
            DiffLine::context(3, 4, line("}")).expect("static numbers are valid"),
        ];
        Self {
            document: DiffDocument::new(lines, true)
                .expect("static document is within default limits"),
            layout_cache: DiffLayoutCache::default(),
            state: DiffViewState::default(),
            wrap: false,
            copied: None,
        }
    }
}

fn line(text: &str) -> CodeLine {
    CodeLine::plain(text).expect("static line is valid")
}

impl App for DiffViewExample {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::State(state) => self.state = state,
            Message::Copy(request) => {
                self.copied = Some(format!(
                    "Copy lines {}..{}: {} unified bytes",
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
                DiffLayoutOptions::default()
                    .with_viewport_width(viewport_width)
                    .with_wrap(self.wrap)
                    .with_width_profile(context.width_profile),
            )
            .expect("static document projection is within default limits");
        let viewport_height = context.size.height.saturating_sub(5).max(1) as usize;
        let status = self
            .copied
            .as_deref()
            .unwrap_or("Control-C copies selected unified lines");
        Node::border(
            Node::column([
                Node::styled_text(
                    format!("DiffView  wrap={}", self.wrap),
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
                DiffView::new("diff", layout, self.state, Message::State)
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
    run_terminal(DiffViewExample::new(), options, |event| match event {
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
