//! Bounded live log viewing with pause, resume, and scrolling

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use nagi_tui::{
    AnsiTextOptions, App, DeliveryPolicy, Effect, Event, EventAction, KeyAction, KeyCode, Length,
    MouseTracking, Node, ParagraphOptions, ScrollAxis, ScrollViewportOptions, Style, Subscription,
    TerminalOptions, WrapMode, run_terminal,
};

const LOG_INTERVAL: Duration = Duration::from_millis(5);
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const MAX_LOG_LINES: usize = 1_000;

enum Message {
    Log(u64),
    TogglePause,
    Quit,
}

struct LogViewer {
    paused: bool,
    exiting: bool,
    sequence: Arc<AtomicU64>,
    lines: VecDeque<String>,
}

impl Default for LogViewer {
    fn default() -> Self {
        Self {
            paused: false,
            exiting: false,
            sequence: Arc::new(AtomicU64::new(0)),
            lines: VecDeque::new(),
        }
    }
}

impl App for LogViewer {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::Log(sequence) => {
                let level = if sequence % 10 == 0 {
                    "\x1b[31;1mERROR\x1b[0m"
                } else {
                    "\x1b[32mINFO\x1b[0m"
                };
                self.lines
                    .push_back(format!("{sequence:06} {level} simulated log event"));
                if self.lines.len() > MAX_LOG_LINES {
                    self.lines.pop_front();
                }
            }
            Message::TogglePause => self.paused = !self.paused,
            Message::Quit => {
                self.exiting = true;
                return Effect::exit();
            }
        }
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        if self.paused {
            return Subscription::none();
        }
        let sequence = Arc::clone(&self.sequence);
        Subscription::every(
            "live-logs",
            LOG_INTERVAL,
            DeliveryPolicy::latest(),
            move || Message::Log(sequence.fetch_add(1, Ordering::Relaxed) + 1),
        )
    }

    fn view(&self, context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let status = if self.exiting {
            "STOPPED"
        } else if self.paused {
            "PAUSED"
        } else {
            "LIVE"
        };
        let mut lines: Vec<_> = self
            .lines
            .iter()
            .map(|line| {
                Node::ansi_text(
                    line,
                    AnsiTextOptions {
                        paragraph: ParagraphOptions {
                            wrap: WrapMode::None,
                            ..ParagraphOptions::default()
                        },
                    },
                )
                .with_length(Length::Fixed(1))
            })
            .collect();
        if lines.is_empty() {
            lines.push(Node::text("Waiting for logs...").with_length(Length::Fixed(1)));
        }
        let help = if context.size.width < 60 {
            "P pause  Q quit"
        } else {
            "Space/P pause, PageUp/PageDown/Home/End or wheel scroll, Q/Esc quit"
        };
        Node::border(
            Node::column([
                Node::styled_text(
                    "Log Viewer",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
                Node::text(format!("Status: {status}  Buffered: {}", self.lines.len()))
                    .with_length(Length::Fixed(1)),
                Node::scroll_viewport_with_options(
                    "log-scroll",
                    Node::column(lines),
                    ScrollViewportOptions {
                        axis: ScrollAxis::Vertical,
                        stick_to_end: true,
                        ..ScrollViewportOptions::default()
                    },
                )
                .with_length(Length::Flex(1)),
                Node::text(help).with_length(Length::Fixed(1)),
            ]),
            Style::default(),
        )
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    let options = TerminalOptions {
        minimum_frame_interval: FRAME_INTERVAL,
        mouse_tracking: Some(MouseTracking::Press),
        focus_first: true,
        ..TerminalOptions::default()
    };
    run_terminal(LogViewer::default(), options, map_event)?;
    Ok(())
}

fn map_event(event: Event) -> EventAction<Message> {
    if is_pause_toggle(&event) {
        return EventAction::Message(Message::TogglePause);
    }
    match event {
        Event::Text(text) if matches!(text.as_str(), "q" | "Q") => {
            EventAction::Message(Message::Quit)
        }
        Event::Key(key) if key.code == KeyCode::Escape => EventAction::Message(Message::Quit),
        Event::Key(key) if key.modifiers.control && key.code == KeyCode::Character('c') => {
            EventAction::Message(Message::Quit)
        }
        _ => EventAction::Ignore,
    }
}

fn is_pause_toggle(event: &Event) -> bool {
    match event {
        Event::Text(text) => matches!(text.as_str(), " " | "p" | "P"),
        Event::Key(key) => {
            key.action != KeyAction::Release
                && !key.modifiers.alt
                && !key.modifiers.control
                && !key.modifiers.meta
                && matches!(key.code, KeyCode::Character(' ' | 'p' | 'P'))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use nagi_tui::{TimedInputDecoder, VirtualClock};

    use super::*;

    #[test]
    fn decoded_printable_shortcuts_toggle_pause() {
        for input in [b"p".as_slice(), b"P".as_slice(), b" ".as_slice()] {
            let mut decoder = TimedInputDecoder::new(VirtualClock::new(), Duration::ZERO);
            let events = decoder.feed(input);
            assert_eq!(events.len(), 1);
            assert!(matches!(
                map_event(events.into_iter().next().unwrap()),
                EventAction::Message(Message::TogglePause)
            ));
        }
    }

    #[test]
    fn quit_shortcut_produces_application_message() {
        assert!(matches!(
            map_event(Event::Text("q".to_owned())),
            EventAction::Message(Message::Quit)
        ));
    }
}
