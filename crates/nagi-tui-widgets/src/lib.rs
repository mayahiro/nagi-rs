//! Standard widgets composed from the public Nagi TUI API

#![deny(unsafe_code)]

mod action;
mod bar_chart;
mod button;
mod calendar;
mod chart;
mod checkbox;
mod code;
mod code_view;
mod command_palette;
mod composer;
mod dialog;
mod diff;
mod diff_view;
mod disclosure;
mod drawer;
mod event;
mod file_picker;
mod help;
mod json;
mod json_inspector;
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
mod split_pane;
mod status_bar;
mod suggestion_popup;
mod table;
mod tabs;
mod text_area;
mod text_area_history;
mod toast;
mod tree;
mod tree_state;
mod virtual_feed;

#[cfg(test)]
mod fixture_support;

pub use action::{
    ACTIVATE_ACTION_ID, COLLAPSE_ACTION_ID, COMPOSER_SUBMIT_ACTION_ID, CONFIRM_ACTION_ID,
    DISMISS_ACTION_ID, EXPAND_ACTION_ID, HISTORY_NEXT_ACTION_ID, HISTORY_PREVIOUS_ACTION_ID,
    HORIZONTAL_SCROLL_NEXT_ACTION_ID, HORIZONTAL_SCROLL_PREVIOUS_ACTION_ID,
    INSPECTOR_COPY_ACTION_ID, NAVIGATION_BACK_ACTION_ID, PANE_FOCUS_NEXT_ACTION_ID,
    PANE_FOCUS_PREVIOUS_ACTION_ID, PANE_RESIZE_NEXT_ACTION_ID, PANE_RESIZE_PREVIOUS_ACTION_ID,
    SELECTION_EXTEND_FIRST_ACTION_ID, SELECTION_EXTEND_LAST_ACTION_ID,
    SELECTION_EXTEND_NEXT_ACTION_ID, SELECTION_EXTEND_PREVIOUS_ACTION_ID,
    SELECTION_FIRST_ACTION_ID, SELECTION_FIRST_DAY_OF_MONTH_ACTION_ID, SELECTION_LAST_ACTION_ID,
    SELECTION_LAST_DAY_OF_MONTH_ACTION_ID, SELECTION_NEXT_ACTION_ID, SELECTION_NEXT_DAY_ACTION_ID,
    SELECTION_NEXT_MONTH_ACTION_ID, SELECTION_NEXT_PAGE_ACTION_ID, SELECTION_NEXT_WEEK_ACTION_ID,
    SELECTION_PREVIOUS_ACTION_ID, SELECTION_PREVIOUS_DAY_ACTION_ID,
    SELECTION_PREVIOUS_MONTH_ACTION_ID, SELECTION_PREVIOUS_PAGE_ACTION_ID,
    SELECTION_PREVIOUS_WEEK_ACTION_ID, SUGGESTION_ACCEPT_ACTION_ID, SUGGESTION_DISMISS_ACTION_ID,
    activate_action_descriptor, confirm_action_descriptor, dismiss_action_descriptor,
};
pub use bar_chart::{BarChart, BarChartBar, BarChartStyle};
pub use button::{Button, ButtonStyle};
pub use calendar::{Calendar, CalendarDate, CalendarStyle, CalendarWeekStart};
pub use chart::{Chart, ChartPoint, ChartSeries, ChartStyle};
pub use checkbox::{Checkbox, CheckboxStyle};
pub use code::{
    CodeDocument, CodeDocumentError, CodeDocumentErrorKind, CodeDocumentLimits, CodeLayout,
    CodeLayoutCache, CodeLayoutError, CodeLayoutErrorKind, CodeLayoutLimits, CodeLayoutOptions,
    CodeLine, DEFAULT_CODE_DOCUMENT_MAX_LINES, DEFAULT_CODE_DOCUMENT_MAX_SPANS,
    DEFAULT_CODE_DOCUMENT_MAX_TEXT_BYTES, DEFAULT_CODE_LAYOUT_MAX_DISPLAY_BYTES,
    DEFAULT_CODE_LAYOUT_MAX_VISUAL_ROWS, DEFAULT_CODE_LAYOUT_TAB_WIDTH,
    DEFAULT_CODE_LAYOUT_VIEWPORT_WIDTH, InvalidCodeLine, InvalidCodeLineKind,
};
pub use code_view::{CodeCopyKind, CodeCopyRequest, CodeView, CodeViewState, CodeViewStyle};
pub use command_palette::{Command, CommandPalette, CommandPaletteStyle};
pub use composer::{Composer, ComposerOverflowPolicy, ComposerState};
pub use dialog::{ConfirmDialog, ConfirmDialogDefault, Dialog, DialogAction, DialogStyle};
pub use diff::{
    DEFAULT_DIFF_DOCUMENT_MAX_LINES, DEFAULT_DIFF_DOCUMENT_MAX_SPANS,
    DEFAULT_DIFF_DOCUMENT_MAX_TEXT_BYTES, DiffDocument, DiffDocumentError, DiffDocumentErrorKind,
    DiffDocumentLimits, DiffHunk, DiffLayout, DiffLayoutCache, DiffLayoutError,
    DiffLayoutErrorKind, DiffLayoutLimits, DiffLayoutOptions, DiffLine, DiffLineKind, DiffRange,
    DiffSide, InvalidDiffLineNumber, InvalidDiffRange, InvalidDiffRangeKind,
};
pub use diff_view::{DiffCopyKind, DiffCopyRequest, DiffView, DiffViewState, DiffViewStyle};
pub use disclosure::{Disclosure, DisclosureStyle};
pub use drawer::{Drawer, DrawerSide, DrawerStyle};
pub use file_picker::{FilePicker, FilePickerEntry, FilePickerStyle};
pub use help::{Help, HelpBinding, HelpMode, HelpStyle};
pub use json::{
    DEFAULT_JSON_DOCUMENT_MAX_DEPTH, DEFAULT_JSON_DOCUMENT_MAX_NODES,
    DEFAULT_JSON_DOCUMENT_MAX_SERIALIZED_BYTES, DEFAULT_JSON_DOCUMENT_MAX_STRING_BYTES,
    DuplicateJsonKey, InvalidJsonNumber, InvalidJsonPointer, JsonDocument, JsonDocumentError,
    JsonDocumentErrorKind, JsonDocumentLimits, JsonKind, JsonMember, JsonNode, JsonNodes,
    JsonNumber, JsonPointer, JsonValue, MAX_JSON_DOCUMENT_DEPTH,
};
pub use json_inspector::{
    DEFAULT_JSON_INSPECTOR_MAX_SCALAR_GRAPHEMES, JsonInspector, JsonInspectorCopyRequest,
    JsonInspectorState, JsonInspectorStyle,
};
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
pub use split_pane::{SPLIT_PANE_RATIO_SCALE, SplitPane, SplitPaneState, SplitPaneStyle};
pub use status_bar::{StatusBar, StatusBarPriority, StatusBarSlot};
pub use suggestion_popup::{
    DuplicateSuggestionId, SuggestionId, SuggestionItems, SuggestionPopup, SuggestionPopupStatus,
    SuggestionPopupStyle, SuggestionRowContext,
};
pub use table::{Table, TableColumn, TableRow, TableStyle};
pub use tabs::{TabItem, Tabs, TabsStyle};
pub use text_area::{TextArea, TextAreaBoundaryNavigation, TextAreaState, TextAreaStyle};
pub use text_area_history::TextAreaHistory;
pub use toast::{Toast, ToastPlacement, ToastRegion, ToastStyle, ToastTone};
pub use tree::{Tree, TreeItem, TreeStyle};
pub use tree_state::TreeState;
pub use virtual_feed::VirtualFeed;
