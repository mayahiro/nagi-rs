//! Bounded live log model rendered as source, table, and details panes

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use nagi_tui::{
    App, DeliveryPolicy, Effect, Event, EventAction, HorizontalAlignment, KeyAction, KeyCode,
    Length, MouseTracking, Node, Style, Subscription, TerminalOptions, TextSpan, run_terminal,
};
use nagi_tui_widgets::{Help, HelpBinding, List, ListItem, Table, TableColumn, TableRow};

const LOG_INTERVAL: Duration = Duration::from_millis(750);
const MAX_LOG_LINES: usize = 200;
const SOURCES: &[&str] = &["all", "api", "worker", "database"];

#[derive(Clone)]
struct LogEntry {
    sequence: u64,
    source: &'static str,
    level: &'static str,
    text: &'static str,
}

enum Message {
    Log(u64),
    SelectSource(usize),
    SelectRow(usize),
    TogglePause,
}

struct LogViewer {
    paused: bool,
    source: usize,
    row: usize,
    sequence: Arc<AtomicU64>,
    logs: VecDeque<LogEntry>,
}

impl Default for LogViewer {
    fn default() -> Self {
        Self {
            paused: false,
            source: 0,
            row: 0,
            sequence: Arc::new(AtomicU64::new(5)),
            logs: (1..=5).map(generated_log).collect(),
        }
    }
}

impl App for LogViewer {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Log(sequence) => {
                self.logs.push_back(generated_log(sequence));
                if self.logs.len() > MAX_LOG_LINES {
                    self.logs.pop_front();
                }
                self.clamp_row();
            }
            Message::SelectSource(index) => {
                self.source = index;
                self.row = 0;
            }
            Message::SelectRow(index) => self.row = index,
            Message::TogglePause => self.paused = !self.paused,
        }
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        if self.paused {
            return Subscription::none();
        }
        let sequence = Arc::clone(&self.sequence);
        Subscription::every(
            "multi-pane-logs",
            LOG_INTERVAL,
            DeliveryPolicy::latest(),
            move || Message::Log(sequence.fetch_add(1, Ordering::Relaxed) + 1),
        )
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let source_list = List::new(
            "sources",
            SOURCES
                .iter()
                .enumerate()
                .map(|(index, source)| ListItem::new(format!("source-{index}"), *source)),
            self.source,
            Message::SelectSource,
        )
        .into_node();
        let visible = self.filtered_logs();
        let rows = visible.iter().map(|entry| {
            TableRow::new(
                format!("log-{}", entry.sequence),
                [
                    format!("{:06}", entry.sequence),
                    entry.source.to_owned(),
                    entry.level.to_owned(),
                    entry.text.to_owned(),
                ],
            )
        });
        let table = Table::new(
            "logs",
            [
                TableColumn::new("Seq", Length::Fixed(8)),
                TableColumn::new("Source", Length::Fixed(10)),
                TableColumn::new("Level", Length::Fixed(8)),
                TableColumn::new("Message", Length::Flex(1)),
            ],
            rows,
            self.row,
            Message::SelectRow,
        )
        .column_alignment(0, HorizontalAlignment::End)
        .viewport("log-rows", Length::Fixed(9))
        .into_node();
        let detail = visible.get(self.row).map_or_else(
            || "No matching log entry".to_owned(),
            |entry| {
                format!(
                    "#{:06} [{}] {}: {}",
                    entry.sequence, entry.source, entry.level, entry.text
                )
            },
        );
        let status = if self.paused { "PAUSED" } else { "LIVE" };

        Node::column([
            Node::styled_text(
                format!(
                    "Multi-pane log viewer  {status}  buffered: {}",
                    self.logs.len()
                ),
                Style {
                    bold: true,
                    ..Style::default()
                },
            )
            .with_length(Length::Fixed(1)),
            Node::row([
                Node::panel(source_list, "Sources").with_length(Length::Fixed(22)),
                Node::panel(table, "Events").with_length(Length::Flex(1)),
            ])
            .with_length(Length::Flex(1)),
            Node::panel(
                Node::paragraph(
                    [TextSpan::new(detail, Style::default())],
                    nagi_tui::ParagraphOptions::default(),
                ),
                "Details",
            )
            .with_length(Length::Fixed(5)),
            Help::new([
                HelpBinding::new("Tab", "pane focus"),
                HelpBinding::new("Up/Down", "select"),
                HelpBinding::new("p", "pause"),
                HelpBinding::new("Esc", "exit"),
            ])
            .into_node()
            .with_length(Length::Fixed(1)),
        ])
    }
}

impl LogViewer {
    fn filtered_logs(&self) -> Vec<&LogEntry> {
        let source = SOURCES.get(self.source).copied().unwrap_or("all");
        self.logs
            .iter()
            .filter(|entry| source == "all" || entry.source == source)
            .collect()
    }

    fn clamp_row(&mut self) {
        self.row = self.row.min(self.filtered_logs().len().saturating_sub(1));
    }
}

fn generated_log(sequence: u64) -> LogEntry {
    let source = ["api", "worker", "database"][(sequence % 3) as usize];
    let (level, text) = if sequence % 11 == 0 {
        ("ERROR", "upstream connection failed")
    } else if sequence % 5 == 0 {
        ("WARN", "latency threshold exceeded")
    } else {
        ("INFO", "request completed")
    };
    LogEntry {
        sequence,
        source,
        level,
        text,
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        LogViewer::default(),
        TerminalOptions {
            minimum_frame_interval: Duration::from_millis(33),
            mouse_tracking: Some(MouseTracking::Press),
            focus_first: true,
            ..TerminalOptions::default()
        },
        map_event,
    )?;
    Ok(())
}

fn map_event(event: Event) -> EventAction<Message> {
    if is_pause_toggle(&event) {
        return EventAction::Message(Message::TogglePause);
    }
    match event {
        Event::Key(key) if key.code == KeyCode::Escape => EventAction::Exit,
        Event::Key(key) if key.modifiers.control && key.code == KeyCode::Character('c') => {
            EventAction::Exit
        }
        _ => EventAction::Ignore,
    }
}

fn is_pause_toggle(event: &Event) -> bool {
    match event {
        Event::Text(text) => matches!(text.as_str(), "p" | "P"),
        Event::Key(key) => {
            key.action != KeyAction::Release
                && !key.modifiers.alt
                && !key.modifiers.control
                && !key.modifiers.meta
                && matches!(key.code, KeyCode::Character('p' | 'P'))
        }
        _ => false,
    }
}
