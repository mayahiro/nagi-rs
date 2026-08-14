//! Variable-height feed with stable item identity and application-owned unread state

use std::sync::Arc;

use nagi_tui::{
    App, Effect, Event, EventAction, KeyCode, Length, Node, NodeId, ScrollState, Style,
    TerminalOptions, ViewContext, VirtualFlowItem, VirtualFlowItems, VirtualFlowSource,
    run_terminal,
};
use nagi_tui_widgets::VirtualFeed;

#[derive(Clone)]
struct FeedEntry {
    id: NodeId,
    label: String,
    height: u32,
}

enum Message {
    Scrolled(ScrollState),
    Quit,
}

struct FeedExample {
    entries: Arc<[FeedEntry]>,
    items: VirtualFlowItems,
    unread: bool,
}

impl Default for FeedExample {
    fn default() -> Self {
        let entries: Arc<[FeedEntry]> = (0..60)
            .map(|index| FeedEntry {
                id: NodeId::from(format!("entry-{index}")),
                label: format!("Entry {index:02}"),
                height: 1 + index % 3,
            })
            .collect::<Vec<_>>()
            .into();
        let items = VirtualFlowItems::new(
            entries
                .iter()
                .map(|entry| VirtualFlowItem::new(entry.id.clone())),
        )
        .expect("example item keys are unique");
        Self {
            entries,
            items,
            unread: true,
        }
    }
}

impl App for FeedExample {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Scrolled(state) => self.unread = !state.at_end,
            Message::Quit => return Effect::exit(),
        }
        Effect::none()
    }

    fn view(&self, _context: ViewContext) -> Node<Self::Message> {
        let built_entries = Arc::clone(&self.entries);
        let estimated_entries = Arc::clone(&self.entries);
        let source = VirtualFlowSource::new(self.items.clone(), move |context| {
            let entry = &built_entries[context.index()];
            Node::column((0..entry.height).map(|line| {
                if line == 0 {
                    Node::text(entry.label.clone())
                } else {
                    Node::text(format!("  continuation {line}"))
                }
            }))
            .with_id(format!("content-{}", entry.id.as_str()))
        })
        .estimated_height(move |context| estimated_entries[context.index()].height);
        let mut feed = VirtualFeed::new("feed", source)
            .follow_end(false)
            .on_scroll(Message::Scrolled)
            .empty(Node::text("No entries"));
        if self.unread {
            feed = feed.unread_indicator(Node::styled_text(
                " More entries below ",
                Style {
                    reverse: true,
                    ..Style::default()
                },
            ));
        }
        Node::panel(
            Node::column([
                feed.into_node().with_length(Length::Flex(1)),
                Node::text("PageUp/PageDown/Home/End or wheel scroll, Escape quits")
                    .with_length(Length::Fixed(1)),
            ]),
            "Variable Feed",
        )
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        FeedExample::default(),
        TerminalOptions {
            focus_first: true,
            ..TerminalOptions::default()
        },
        |event| match event {
            Event::Key(key) if key.code == KeyCode::Escape => EventAction::Message(Message::Quit),
            _ => EventAction::Ignore,
        },
    )?;
    Ok(())
}
