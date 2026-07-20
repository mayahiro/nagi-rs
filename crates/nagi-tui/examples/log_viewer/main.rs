//! Event-driven process monitoring without an application UI loop

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use nagi_tui::{
    AnsiTextOptions, App, CancelToken, DeliveryPolicy, Effect, Event, EventAction, KeyAction,
    KeyCode, Length, MouseTracking, Node, ParagraphOptions, ScrollAxis, ScrollOffset,
    ScrollViewportOptions, Size, Style, Subscription, SubscriptionSink, TerminalOptions,
    VirtualFragment, WrapMode, run_terminal,
};

const LOG_INTERVAL: Duration = Duration::from_millis(5);
const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);
const UPTIME_INTERVAL: Duration = Duration::from_secs(1);
const MAXIMUM_BATCH_LINES: usize = 64;
const MAX_LOG_LINES: usize = 1_000;

enum Message {
    ProcessOutput(u64),
    Uptime(u64),
    TogglePause,
    Quit,
}

struct LogViewer {
    paused: bool,
    exiting: bool,
    started_at: Instant,
    uptime: u64,
    sequence: Arc<AtomicU64>,
    lines: Rc<RefCell<VecDeque<String>>>,
}

impl Default for LogViewer {
    fn default() -> Self {
        Self {
            paused: false,
            exiting: false,
            started_at: Instant::now(),
            uptime: 0,
            sequence: Arc::new(AtomicU64::new(0)),
            lines: Rc::new(RefCell::new(VecDeque::with_capacity(MAX_LOG_LINES))),
        }
    }
}

impl App for LogViewer {
    type Message = Message;

    fn init(&mut self) -> Effect<Self::Message> {
        self.started_at = Instant::now();
        Effect::none()
    }

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        match message {
            Message::ProcessOutput(sequence) => {
                let mut lines = self.lines.borrow_mut();
                lines.push_back(format_log_line(sequence));
                if lines.len() > MAX_LOG_LINES {
                    lines.pop_front();
                }
            }
            Message::Uptime(seconds) => self.uptime = seconds,
            Message::TogglePause => self.paused = !self.paused,
            Message::Quit => {
                self.exiting = true;
                return Effect::exit();
            }
        }
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        if self.exiting {
            return Subscription::none();
        }
        let started_at = self.started_at;
        let mut subscriptions = vec![Subscription::every(
            "uptime",
            UPTIME_INTERVAL,
            DeliveryPolicy::latest(),
            move || Message::Uptime(started_at.elapsed().as_secs()),
        )];
        if !self.paused {
            subscriptions.push(process_output_subscription(Arc::clone(&self.sequence)));
        }
        Subscription::batch(subscriptions)
    }

    fn view(&self, context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let status = if self.exiting {
            "STOPPED"
        } else if self.paused {
            "PAUSED"
        } else {
            "LIVE"
        };
        let lines = Rc::clone(&self.lines);
        let line_count = lines.borrow().len();
        let content_height = u32::try_from(line_count.max(1)).expect("log buffer is bounded");
        let help = if context.size.width < 60 {
            "P pause  Q quit"
        } else {
            "Space/P pause, PageUp/PageDown/Home/End or wheel scroll, Q/Esc quit"
        };
        Node::border(
            Node::column([
                Node::styled_text(
                    "Event-driven Process Monitor",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                )
                .with_length(Length::Fixed(1)),
                Node::text(format!(
                    "Status: {status}  Uptime: {}  Buffered: {line_count}",
                    format_uptime(self.uptime),
                ))
                .with_length(Length::Fixed(1)),
                Node::virtual_scroll_viewport_with_options(
                    "log-scroll",
                    Size::new(0, content_height),
                    ScrollViewportOptions {
                        axis: ScrollAxis::Vertical,
                        stick_to_end: true,
                        ..ScrollViewportOptions::default()
                    },
                    move |viewport| {
                        let lines = lines.borrow();
                        if lines.is_empty() {
                            return VirtualFragment::new(
                                ScrollOffset::default(),
                                Node::column([Node::text("Waiting for process output...")
                                    .with_length(Length::Fixed(1))]),
                            );
                        }
                        let start = usize::try_from(viewport.offset.y).expect("visible offset");
                        let visible_height =
                            usize::try_from(viewport.size.height).expect("visible height");
                        let end = start.saturating_add(visible_height).min(lines.len());
                        let visible = (start..end)
                            .map(|index| log_node(&lines[index]))
                            .collect::<Vec<_>>();
                        VirtualFragment::new(
                            ScrollOffset::new(0, viewport.offset.y),
                            Node::column(visible),
                        )
                    },
                )
                .with_length(Length::Flex(1)),
                Node::text(help).with_length(Length::Fixed(1)),
            ]),
            Style::default(),
        )
    }
}

fn process_output_subscription(sequence: Arc<AtomicU64>) -> Subscription<Message> {
    Subscription::stream(
        "process-output",
        DeliveryPolicy::batch(MAXIMUM_BATCH_LINES, FRAME_INTERVAL),
        move |cancel, sink| simulate_process_output(&cancel, &sink, &sequence),
    )
}

fn simulate_process_output(
    cancel: &CancelToken,
    sink: &SubscriptionSink<Message>,
    sequence: &AtomicU64,
) {
    // A real adapter blocks on process stdout. Sleeping only keeps this example
    // self-contained while Nagi owns cancellation and wake-up.
    while !cancel.is_cancelled() {
        thread::sleep(LOG_INTERVAL);
        if cancel.is_cancelled() {
            break;
        }
        let sequence = sequence.fetch_add(1, Ordering::Relaxed) + 1;
        if sink.send(Message::ProcessOutput(sequence)).is_err() {
            break;
        }
    }
}

fn log_node(line: &str) -> Node<Message> {
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
}

fn format_log_line(sequence: u64) -> String {
    let level = if sequence % 10 == 0 {
        "\x1b[31;1mERROR\x1b[0m"
    } else {
        "\x1b[32mINFO\x1b[0m"
    };
    format!("{sequence:06} {level} simulated process output")
}

fn format_uptime(seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        seconds / 60 % 60,
        seconds % 60,
    )
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

    #[test]
    fn log_storage_retains_only_newest_lines() {
        let mut viewer = LogViewer::default();
        for sequence in 1..=MAX_LOG_LINES as u64 + 2 {
            drop(viewer.update(Message::ProcessOutput(sequence)));
        }
        let lines = viewer.lines.borrow();
        assert_eq!(lines.len(), MAX_LOG_LINES);
        assert!(lines.front().unwrap().starts_with("000003"));
        assert!(lines.back().unwrap().starts_with("001002"));
    }

    #[test]
    fn subscription_topology_follows_application_state() {
        let mut viewer = LogViewer::default();
        drop(viewer.init());
        assert_eq!(
            format!("{:?}", viewer.subscriptions()),
            "Subscription::Batch(2)"
        );
        viewer.paused = true;
        assert_eq!(
            format!("{:?}", viewer.subscriptions()),
            "Subscription::Batch(1)"
        );
        viewer.exiting = true;
        assert!(viewer.subscriptions().is_none());
    }
}
