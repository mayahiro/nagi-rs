//! Million-row virtual ScrollViewport without a million-node tree

use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Length, Node, ScrollAxis, ScrollOffset,
    ScrollViewportOptions, Size, TerminalOptions, ViewContext, VirtualFragment, run_terminal,
};

const ROWS: u32 = 1_000_000;

enum Message {
    Quit,
}

#[derive(Default)]
struct VirtualScroll {
    exiting: bool,
}

impl App for VirtualScroll {
    type Message = Message;

    fn update(&mut self, message: Message) -> Effect<Message> {
        match message {
            Message::Quit => {
                self.exiting = true;
                Effect::exit()
            }
        }
    }

    fn view(&self, _context: ViewContext) -> Node<Message> {
        let status = if self.exiting {
            "Stopping"
        } else {
            "1000000 rows"
        };
        Node::panel(
            Node::column([
                Node::text(status).with_length(Length::Fixed(1)),
                Node::virtual_scroll_viewport_with_options(
                    "rows",
                    Size::new(0, ROWS),
                    ScrollViewportOptions {
                        axis: ScrollAxis::Vertical,
                        ..ScrollViewportOptions::default()
                    },
                    |viewport| {
                        let start = viewport.offset.y;
                        let end = start.saturating_add(viewport.size.height);
                        let rows = (start..end).map(|index| {
                            Node::text(format!("Row {index:06}"))
                                .with_id(format!("row-{index}"))
                                .with_length(Length::Fixed(1))
                        });
                        VirtualFragment::new(ScrollOffset::new(0, start), Node::column(rows))
                    },
                )
                .with_length(Length::Flex(1)),
                Node::text("PageUp/PageDown/Home/End or wheel scroll, Escape quits")
                    .with_length(Length::Fixed(1)),
            ]),
            "Virtual Scroll",
        )
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        VirtualScroll::default(),
        TerminalOptions {
            focus_first: true,
            ..TerminalOptions::default()
        },
        |event| match event {
            Event::Key(key) if key.code == KeyCode::Escape => EventAction::Message(Message::Quit),
            Event::Text(text) if matches!(text.as_str(), "q" | "Q") => {
                EventAction::Message(Message::Quit)
            }
            _ => EventAction::Ignore,
        },
    )?;
    Ok(())
}
