//! Ratatui-style operations dashboard composed from public Nagi TUI APIs

use nagi_tui::{
    App, Effect, Event, EventAction, HorizontalAlignment, Length, Node, Style, TerminalOptions,
    run_terminal,
};
use nagi_tui_widgets::{
    BarChart, BarChartBar, Chart, ChartPoint, ChartSeries, Help, HelpBinding, Progress, Sparkline,
    Table, TableColumn, TableRow,
};

enum Message {
    SelectService(usize),
}

#[derive(Default)]
struct Dashboard {
    service: usize,
}

impl App for Dashboard {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        let Message::SelectService(index) = message;
        self.service = index;
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let requests = Node::panel(
            Node::column([
                Node::styled_text(
                    "12.8k req/min",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                ),
                Sparkline::new([4, 6, 5, 9, 8, 12, 11, 15, 13, 18, 16, 20], 24).into_node(),
            ]),
            "Requests",
        )
        .with_length(Length::Flex(1));
        let latency = Node::panel(
            Node::column([
                Node::styled_text(
                    "p95 84 ms",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                ),
                Sparkline::new([70, 74, 69, 77, 81, 75, 79, 84, 82, 80, 84], 24)
                    .bounds(60, 100)
                    .into_node(),
            ]),
            "Latency",
        )
        .with_length(Length::Flex(1));
        let errors = Node::panel(
            Node::column([
                Node::styled_text(
                    "18% budget used",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                ),
                Progress::new(18, 100, 24).into_node(),
            ]),
            "Errors",
        )
        .with_length(Length::Flex(1));

        let traffic = Chart::new(
            [ChartSeries::new(
                "requests",
                [
                    ChartPoint::new(0, 3),
                    ChartPoint::new(1, 4),
                    ChartPoint::new(2, 4),
                    ChartPoint::new(3, 7),
                    ChartPoint::new(4, 6),
                    ChartPoint::new(5, 9),
                    ChartPoint::new(6, 8),
                    ChartPoint::new(7, 11),
                ],
            )],
            34,
            8,
        )
        .bounds(0, 7, 0, 12)
        .into_node();
        let resources = BarChart::new(
            [
                BarChartBar::new("api", 68),
                BarChartBar::new("worker", 47),
                BarChartBar::new("database", 81),
            ],
            16,
        )
        .maximum(100)
        .into_node();
        let services = Table::new(
            "services",
            [
                TableColumn::new("Service", Length::Flex(1)),
                TableColumn::new("Status", Length::Fixed(10)),
                TableColumn::new("RPS", Length::Fixed(8)),
                TableColumn::new("p95", Length::Fixed(8)),
            ],
            [
                TableRow::new("service-api", ["api", "healthy", "8,420", "72 ms"]),
                TableRow::new("service-worker", ["worker", "healthy", "3,910", "84 ms"]),
                TableRow::new("service-database", ["database", "warning", "470", "41 ms"]),
            ],
            self.service,
            Message::SelectService,
        )
        .column_alignment(2, HorizontalAlignment::End)
        .column_alignment(3, HorizontalAlignment::End)
        .viewport("service-rows", Length::Fixed(4))
        .into_node();

        Node::column([
            Node::styled_text(
                "Operations dashboard",
                Style {
                    bold: true,
                    ..Style::default()
                },
            )
            .with_length(Length::Fixed(1)),
            Node::row([requests, latency, errors]).with_length(Length::Fixed(5)),
            Node::row([
                Node::panel(traffic, "Traffic").with_length(Length::Flex(1)),
                Node::panel(resources, "CPU by service").with_length(Length::Flex(1)),
            ])
            .with_length(Length::Fixed(10)),
            Node::panel(services, "Services").with_length(Length::Flex(1)),
            Help::new([
                HelpBinding::new("Tab", "focus"),
                HelpBinding::new("Up/Down", "select service"),
                HelpBinding::new("Esc", "exit"),
            ])
            .into_node()
            .with_length(Length::Fixed(1)),
        ])
    }
}

fn main() -> Result<(), nagi_tui::RunError> {
    run_terminal(
        Dashboard::default(),
        TerminalOptions {
            focus_first: true,
            ..TerminalOptions::default()
        },
        |event| match event {
            Event::Key(key) if key.code == nagi_tui::KeyCode::Escape => EventAction::Exit,
            Event::Key(key)
                if key.modifiers.control && key.code == nagi_tui::KeyCode::Character('c') =>
            {
                EventAction::Exit
            }
            _ => EventAction::Ignore,
        },
    )?;
    Ok(())
}
