//! Shared widget rendering snapshots through the public Runtime

mod support;

use nagi_tui::{
    App, Effect, HorizontalAlignment, Length, Node, Runtime, RuntimeConfig, Size, VirtualClock,
};
use nagi_tui_widgets::{
    BarChart, BarChartBar, Chart, ChartPoint, ChartSeries, FilePicker, FilePickerEntry, Help,
    HelpBinding, Paginator, Sparkline, Table, TableColumn, TableRow, TextArea, TextAreaState, Tree,
    TreeItem,
};

struct WidgetSnapshotApp {
    case_id: String,
}

impl App for WidgetSnapshotApp {
    type Message = usize;

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        widget_snapshot_view(&self.case_id)
    }
}

#[test]
fn widgets_match_shared_surface_snapshots() {
    let Some(records) = support::load(
        "widgets/runtime-snapshots.txt",
        "widget-runtime-snapshot",
        &["width", "height", "expected"],
    ) else {
        return;
    };
    for record in records {
        let mut runtime = Runtime::with_clock(
            WidgetSnapshotApp {
                case_id: record.id.clone(),
            },
            RuntimeConfig::new(Size::new(
                number(record.field("width")),
                number(record.field("height")),
            )),
            VirtualClock::new(),
        )
        .expect("runtime");
        let frame = runtime
            .render_if_dirty()
            .expect("render")
            .expect("dirty frame");
        assert_eq!(
            frame.surface().snapshot(),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

fn widget_snapshot_view(case_id: &str) -> Node<usize> {
    match case_id {
        "sparkline" => Sparkline::new([0, 1, 2, 3], 4).bounds(0, 3).into_node(),
        "bar-chart" => BarChart::new([BarChartBar::new("A", 5)], 4)
            .maximum(10)
            .show_values(false)
            .into_node(),
        "chart" => Chart::new(
            [ChartSeries::new(
                "s",
                [ChartPoint::new(0, 0), ChartPoint::new(2, 2)],
            )],
            5,
            3,
        )
        .bounds(0, 2, 0, 2)
        .into_node(),
        "help" => Help::new([HelpBinding::new("q", "Quit"), HelpBinding::new("?", "Help")])
            .separator(" | ")
            .into_node(),
        "paginator" => Paginator::new("pages", 2, 5, |page| page).into_node(),
        "text-area" => TextArea::new(
            "area",
            TextAreaState::with_selection("A日BC", 4, 1).with_horizontal_offset(1),
            |_| 0,
        )
        .into_node(),
        "table" => Table::new(
            "table",
            [
                TableColumn::new("A", Length::Fixed(3)),
                TableColumn::new("B", Length::Fixed(3)),
            ],
            [TableRow::new("row", ["x", "y"])],
            0,
            |index| index,
        )
        .column_alignment(0, HorizontalAlignment::End)
        .column_alignment(1, HorizontalAlignment::Center)
        .into_node(),
        "tree" => Tree::new(
            "tree",
            (0..5).map(|index| {
                TreeItem::leaf(
                    char::from(b'a' + u8::try_from(index).unwrap()).to_string(),
                    format!("Item{index}"),
                    0,
                )
            }),
            3,
            |index| index,
        )
        .viewport(2)
        .into_node(),
        "file-picker" => FilePicker::new(
            "files",
            [
                FilePickerEntry::file("a", "A", "a"),
                FilePickerEntry::directory("b", "B", "b"),
                FilePickerEntry::file("hidden", "Hidden", ".hidden").hidden(true),
                FilePickerEntry::file("c", "C", "c"),
            ],
            3,
            |index| index,
        )
        .viewport(2)
        .into_node(),
        unknown => panic!("unknown widget snapshot case {unknown}"),
    }
}

fn number(value: &str) -> u32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid u32 {value}: {error}"))
}
