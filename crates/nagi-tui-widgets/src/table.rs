use std::sync::Arc;

use nagi_tui::{
    EventResult, HorizontalAlignment, Insets, Length, Node, NodeId, ParagraphOptions, ScrollAxis,
    ScrollOffset, ScrollViewportOptions, Size, Style, TextSpan, VirtualFragment, WrapMode,
};

use crate::event::is_activation_event;
use crate::navigation::{Navigation, navigate, navigate_event};

/// One sized column rendered by a [`Table`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableColumn {
    title: String,
    width: Length,
}

impl TableColumn {
    /// Creates a column using a Core [`Length`] sizing rule
    #[must_use]
    pub fn new(title: impl Into<String>, width: Length) -> Self {
        Self {
            title: title.into(),
            width,
        }
    }

    /// Returns the displayed heading
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the requested column sizing rule
    #[must_use]
    pub const fn width(&self) -> Length {
        self.width
    }
}

/// One stable row rendered by a [`Table`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableRow {
    id: NodeId,
    cells: Vec<String>,
}

impl TableRow {
    /// Creates a row with an application-defined stable identity
    #[must_use]
    pub fn new(id: impl Into<NodeId>, cells: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            id: id.into(),
            cells: cells.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns the row's stable identity
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns the row's cell values
    #[must_use]
    pub fn cells(&self) -> &[String] {
        &self.cells
    }
}

/// Visual styles used by a [`Table`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableStyle {
    /// Style used by the heading row
    pub header: Style,
    /// Style used by unselected data rows
    pub normal: Style,
    /// Style used by the application-selected row
    pub selected: Style,
    /// Style merged over the selected row while the table owns focus
    pub focused: Style,
    /// Style used by every data row while the table is disabled
    pub disabled: Style,
}

impl Default for TableStyle {
    fn default() -> Self {
        Self {
            header: Style {
                bold: true,
                ..Style::default()
            },
            normal: Style::default(),
            selected: Style {
                reverse: true,
                ..Style::default()
            },
            focused: Style {
                underline: true,
                ..Style::default()
            },
            disabled: Style {
                dim: true,
                ..Style::default()
            },
        }
    }
}

/// A sized-column table with one composite focus target and owned row selection
pub struct Table<Message> {
    id: NodeId,
    columns: Vec<TableColumn>,
    column_alignments: Vec<HorizontalAlignment>,
    rows: Vec<TableRow>,
    selected: usize,
    enabled: bool,
    style: TableStyle,
    viewport: Option<(NodeId, Length)>,
    on_select: Arc<dyn Fn(usize) -> Message>,
}

impl<Message: 'static> Table<Message> {
    /// Creates an enabled table using application-owned selection state
    #[must_use]
    pub fn new(
        id: impl Into<NodeId>,
        columns: impl IntoIterator<Item = TableColumn>,
        rows: impl IntoIterator<Item = TableRow>,
        selected: usize,
        on_select: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        let columns: Vec<_> = columns.into_iter().collect();
        let column_alignments = vec![HorizontalAlignment::Start; columns.len()];
        Self {
            id: id.into(),
            columns,
            column_alignments,
            rows: rows.into_iter().collect(),
            selected,
            enabled: true,
            style: TableStyle::default(),
            viewport: None,
            on_select: Arc::new(on_select),
        }
    }

    /// Sets whether the table can receive focus and emit selection messages
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Replaces the table styles
    #[must_use]
    pub const fn style(mut self, style: TableStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets horizontal alignment for one zero-based column
    ///
    /// Out-of-range columns are ignored. The default is start alignment
    #[must_use]
    pub fn column_alignment(mut self, column: usize, alignment: HorizontalAlignment) -> Self {
        if let Some(value) = self.column_alignments.get_mut(column) {
            *value = alignment;
        }
        self
    }

    /// Keeps the header fixed and wraps data rows in a virtual Core ScrollViewport
    ///
    /// `viewport_id` must differ from the table root and row IDs. Applications
    /// may control its retained offset through `Runtime::set_scroll_offset`.
    /// Semantic row construction is bounded by the visible body height. Row
    /// metadata collection remains eager. Virtualized body rows are one Cell
    /// high and clip multiline content
    #[must_use]
    pub fn viewport(mut self, viewport_id: impl Into<NodeId>, body_height: Length) -> Self {
        self.viewport = Some((viewport_id.into(), body_height));
        self
    }

    /// Builds the public semantic node for this table
    #[must_use]
    pub fn into_node(mut self) -> Node<Message> {
        let row_count = self.rows.len();
        let selected = navigate(row_count, self.selected, Navigation::Normalize);
        let headings: Vec<_> = self
            .columns
            .iter()
            .map(|column| column.title.as_str())
            .collect();
        let header = table_row(
            "  ",
            &headings,
            &self.columns,
            &self.column_alignments,
            self.style.header,
        );
        let root_id = self.id;
        if let Some((viewport_id, height)) = self.viewport.take() {
            let body = virtual_table_body(
                root_id.clone(),
                self.columns,
                self.column_alignments,
                self.rows,
                selected,
                self.enabled,
                self.style,
                self.on_select,
                viewport_id,
                height,
            );
            let root = Node::column([header, body]);
            return if self.enabled && selected.is_some() {
                root
            } else {
                root.with_id(root_id)
            };
        }
        let mut row_nodes = Vec::with_capacity(self.rows.len());

        for (index, row) in self.rows.into_iter().enumerate() {
            let is_selected = selected == Some(index);
            let style = if !self.enabled {
                self.style.disabled
            } else if is_selected {
                self.style.selected
            } else {
                self.style.normal
            };
            let values: Vec<_> = row.cells.iter().map(String::as_str).collect();
            let marker = if is_selected { "> " } else { "  " };
            let node = table_row(
                marker,
                &values,
                &self.columns,
                &self.column_alignments,
                style,
            );
            if !self.enabled {
                row_nodes.push(node.with_id(row.id));
                continue;
            }
            let row_id = row.id;
            let click_focus = root_id.clone();
            let click_select = Arc::clone(&self.on_select);
            let row = node.with_id(row_id.clone()).on_event(row_id, move |event| {
                if !is_activation_event(event) {
                    return EventResult::ignored();
                }
                EventResult::message(click_select(index)).focus(click_focus.clone())
            });
            if !is_selected {
                row_nodes.push(row);
                continue;
            }
            let navigation_focus = root_id.clone();
            let navigation_select = Arc::clone(&self.on_select);
            row_nodes.push(
                Node::column([row])
                    .focusable(root_id.clone())
                    .with_focused_style(self.style.focused)
                    .on_event(root_id.clone(), move |event| {
                        let Some(next) = navigate_event(event, row_count, index) else {
                            return EventResult::ignored();
                        };
                        let mut result = EventResult::consumed().focus(navigation_focus.clone());
                        if next != index {
                            result = result.emit(navigation_select(next));
                        }
                        result
                    }),
            );
        }

        let mut children = Vec::with_capacity(row_nodes.len().saturating_add(1));
        children.push(header);
        children.extend(row_nodes);
        let root = Node::column(children);
        if self.enabled && selected.is_some() {
            root
        } else {
            root.with_id(root_id)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn virtual_table_body<Message: 'static>(
    root_id: NodeId,
    columns: Vec<TableColumn>,
    alignments: Vec<HorizontalAlignment>,
    rows: Vec<TableRow>,
    selected: Option<usize>,
    enabled: bool,
    style: TableStyle,
    on_select: Arc<dyn Fn(usize) -> Message>,
    viewport_id: NodeId,
    height: Length,
) -> Node<Message> {
    let content_height = u32::try_from(rows.len()).unwrap_or(u32::MAX);
    let columns = Arc::new(columns);
    let alignments = Arc::new(alignments);
    let rows = Arc::new(rows);
    Node::virtual_scroll_viewport_with_options(
        viewport_id,
        Size::new(0, content_height),
        ScrollViewportOptions {
            axis: ScrollAxis::Vertical,
            ensure_focused_visible: true,
            ..ScrollViewportOptions::default()
        },
        move |viewport| {
            let start = usize::try_from(viewport.offset.y)
                .unwrap_or(usize::MAX)
                .min(rows.len());
            let count = usize::try_from(viewport.size.height).unwrap_or(usize::MAX);
            let end = start.saturating_add(count).min(rows.len());
            let visible_rows = Node::padding(
                Node::column((start..end).map(|index| {
                    virtual_table_row(
                        &rows,
                        &columns,
                        &alignments,
                        selected,
                        index,
                        enabled,
                        style,
                        &root_id,
                        &on_select,
                    )
                })),
                Insets::new(u32::try_from(start).unwrap_or(u32::MAX), 0, 0, 0),
            );
            let mut layers = vec![visible_rows];
            if enabled {
                if let Some(index) = selected.filter(|index| *index < start || *index >= end) {
                    let proxy = table_navigation_target(
                        Node::spacer(0, 1),
                        root_id.clone(),
                        rows.len(),
                        index,
                        None,
                        Arc::clone(&on_select),
                    );
                    layers.push(Node::padding(
                        proxy,
                        Insets::new(u32::try_from(index).unwrap_or(u32::MAX), 0, 0, 0),
                    ));
                }
            }
            VirtualFragment::new(ScrollOffset::default(), Node::stack(layers))
        },
    )
    .tab_stop(false)
    .with_length(height)
}

#[allow(clippy::too_many_arguments)]
fn virtual_table_row<Message: 'static>(
    rows: &[TableRow],
    columns: &[TableColumn],
    alignments: &[HorizontalAlignment],
    selected: Option<usize>,
    index: usize,
    enabled: bool,
    style: TableStyle,
    root_id: &NodeId,
    on_select: &Arc<dyn Fn(usize) -> Message>,
) -> Node<Message> {
    let row = &rows[index];
    let is_selected = selected == Some(index);
    let row_style = if !enabled {
        style.disabled
    } else if is_selected {
        style.selected
    } else {
        style.normal
    };
    let values: Vec<_> = row.cells.iter().map(String::as_str).collect();
    let marker = if is_selected { "> " } else { "  " };
    let node = table_row(marker, &values, columns, alignments, row_style);
    if !enabled {
        return node.with_id(row.id.clone()).with_length(Length::Fixed(1));
    }
    let row_id = row.id.clone();
    let click_focus = root_id.clone();
    let click_select = Arc::clone(on_select);
    let row = node.with_id(row_id.clone()).on_event(row_id, move |event| {
        if !is_activation_event(event) {
            return EventResult::ignored();
        }
        EventResult::message(click_select(index)).focus(click_focus.clone())
    });
    if !is_selected {
        return row.with_length(Length::Fixed(1));
    }
    table_navigation_target(
        Node::column([row]),
        root_id.clone(),
        rows.len(),
        index,
        Some(style.focused),
        Arc::clone(on_select),
    )
    .with_length(Length::Fixed(1))
}

fn table_navigation_target<Message: 'static>(
    node: Node<Message>,
    root_id: NodeId,
    row_count: usize,
    index: usize,
    focused_style: Option<Style>,
    on_select: Arc<dyn Fn(usize) -> Message>,
) -> Node<Message> {
    let navigation_focus = root_id.clone();
    let mut node = node
        .focusable(root_id.clone())
        .on_event(root_id, move |event| {
            let Some(next) = navigate_event(event, row_count, index) else {
                return EventResult::ignored();
            };
            let mut result = EventResult::consumed().focus(navigation_focus.clone());
            if next != index {
                result = result.emit(on_select(next));
            }
            result
        });
    if let Some(style) = focused_style {
        node = node.with_focused_style(style);
    }
    node
}

fn table_row<Message>(
    marker: &str,
    values: &[&str],
    columns: &[TableColumn],
    alignments: &[HorizontalAlignment],
    style: Style,
) -> Node<Message> {
    let mut children = Vec::with_capacity(columns.len().saturating_mul(2).saturating_add(1));
    children.push(Node::styled_text(marker, style));
    for (index, column) in columns.iter().enumerate() {
        if index != 0 {
            children.push(Node::styled_text(" │ ", style));
        }
        let alignment = alignments
            .get(index)
            .copied()
            .unwrap_or(HorizontalAlignment::Start);
        children.push(
            Node::paragraph(
                [TextSpan::new(
                    values.get(index).copied().unwrap_or_default(),
                    style,
                )],
                ParagraphOptions {
                    wrap: WrapMode::None,
                    alignment,
                },
            )
            .with_length(column.width),
        );
    }
    Node::row(children)
}
