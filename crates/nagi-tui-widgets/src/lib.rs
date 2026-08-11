//! Standard widgets composed from the public Nagi TUI API

#![deny(unsafe_code)]

mod action;
mod bar_chart;
mod button;
mod calendar;
mod chart;
mod checkbox;
mod command_palette;
mod composer;
mod dialog;
mod disclosure;
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
mod selectable_text;
mod sparkline;
mod spinner;
mod table;
mod tabs;
mod text_area;
mod text_area_history;
mod tree;
mod tree_state;
mod virtual_feed;

#[cfg(test)]
mod fixture_support;

pub use action::{
    ACTIVATE_ACTION_ID, COLLAPSE_ACTION_ID, COMPOSER_SUBMIT_ACTION_ID, CONFIRM_ACTION_ID,
    DISMISS_ACTION_ID, EXPAND_ACTION_ID, HISTORY_NEXT_ACTION_ID, HISTORY_PREVIOUS_ACTION_ID,
    NAVIGATION_BACK_ACTION_ID, SELECTION_FIRST_ACTION_ID, SELECTION_FIRST_DAY_OF_MONTH_ACTION_ID,
    SELECTION_LAST_ACTION_ID, SELECTION_LAST_DAY_OF_MONTH_ACTION_ID, SELECTION_NEXT_ACTION_ID,
    SELECTION_NEXT_DAY_ACTION_ID, SELECTION_NEXT_MONTH_ACTION_ID, SELECTION_NEXT_PAGE_ACTION_ID,
    SELECTION_NEXT_WEEK_ACTION_ID, SELECTION_PREVIOUS_ACTION_ID, SELECTION_PREVIOUS_DAY_ACTION_ID,
    SELECTION_PREVIOUS_MONTH_ACTION_ID, SELECTION_PREVIOUS_PAGE_ACTION_ID,
    SELECTION_PREVIOUS_WEEK_ACTION_ID, activate_action_descriptor, confirm_action_descriptor,
    dismiss_action_descriptor,
};
pub use bar_chart::{BarChart, BarChartBar, BarChartStyle};
pub use button::{Button, ButtonStyle};
pub use calendar::{Calendar, CalendarDate, CalendarStyle, CalendarWeekStart};
pub use chart::{Chart, ChartPoint, ChartSeries, ChartStyle};
pub use checkbox::{Checkbox, CheckboxStyle};
pub use command_palette::{Command, CommandPalette, CommandPaletteStyle};
pub use composer::{Composer, ComposerOverflowPolicy, ComposerState};
pub use dialog::{ConfirmDialog, ConfirmDialogDefault, Dialog, DialogAction, DialogStyle};
pub use disclosure::{Disclosure, DisclosureStyle};
pub use file_picker::{FilePicker, FilePickerEntry, FilePickerStyle};
pub use help::{Help, HelpBinding, HelpMode, HelpStyle};
pub use list::{List, ListItem, ListStyle};
pub use modal::{Modal, ModalStyle};
pub use paginator::{Paginator, PaginatorMode, PaginatorStyle};
pub use progress::{Progress, ProgressStyle};
pub use radio::{Radio, RadioStyle};
pub use scrollbar::{Scrollbar, ScrollbarOrientation, ScrollbarStyle};
pub use select::{Select, SelectStyle};
pub use selectable_text::{
    SelectableText, SelectableTextContent, SelectableTextState, SelectableTextStyle, TextCopyKind,
    TextCopyRequest,
};
pub use sparkline::Sparkline;
pub use spinner::{SPINNER_FRAMES, Spinner, SpinnerStyle};
pub use table::{Table, TableColumn, TableRow, TableStyle};
pub use tabs::{TabItem, Tabs, TabsStyle};
pub use text_area::{TextArea, TextAreaBoundaryNavigation, TextAreaState, TextAreaStyle};
pub use text_area_history::TextAreaHistory;
pub use tree::{Tree, TreeItem, TreeStyle};
pub use tree_state::TreeState;
pub use virtual_feed::VirtualFeed;
