//! Standard widgets composed from the public Nagi TUI API

#![deny(unsafe_code)]

mod bar_chart;
mod button;
mod calendar;
mod chart;
mod checkbox;
mod command_palette;
mod event;
mod file_picker;
mod help;
mod list;
mod modal;
mod navigation;
mod paginator;
mod progress;
mod radio;
mod scrollbar;
mod select;
mod sparkline;
mod spinner;
mod table;
mod tabs;
mod text_area;
mod text_area_history;
mod tree;
mod tree_state;

#[cfg(test)]
mod fixture_support;

pub use bar_chart::{BarChart, BarChartBar, BarChartStyle};
pub use button::{Button, ButtonStyle};
pub use calendar::{Calendar, CalendarDate, CalendarStyle, CalendarWeekStart};
pub use chart::{Chart, ChartPoint, ChartSeries, ChartStyle};
pub use checkbox::{Checkbox, CheckboxStyle};
pub use command_palette::{Command, CommandPalette, CommandPaletteStyle};
pub use file_picker::{FilePicker, FilePickerEntry, FilePickerStyle};
pub use help::{Help, HelpBinding, HelpMode, HelpStyle};
pub use list::{List, ListItem, ListStyle};
pub use modal::{Modal, ModalStyle};
pub use paginator::{Paginator, PaginatorMode, PaginatorStyle};
pub use progress::{Progress, ProgressStyle};
pub use radio::{Radio, RadioStyle};
pub use scrollbar::{Scrollbar, ScrollbarOrientation, ScrollbarStyle};
pub use select::{Select, SelectStyle};
pub use sparkline::Sparkline;
pub use spinner::{SPINNER_FRAMES, Spinner, SpinnerStyle};
pub use table::{Table, TableColumn, TableRow, TableStyle};
pub use tabs::{TabItem, Tabs, TabsStyle};
pub use text_area::{TextArea, TextAreaState, TextAreaStyle};
pub use text_area_history::TextAreaHistory;
pub use tree::{Tree, TreeItem, TreeStyle};
pub use tree_state::TreeState;
