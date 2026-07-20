use std::cell::{Ref, RefCell};
use std::marker::PhantomData;

use nagi_surface::{Cursor, Surface};
use nagi_text::{
    WidthProfile, byte_at_cell, cell_at_byte, grapheme_width, graphemes, text_width, truncate,
    wrapped_lines,
};
use nagi_vt::Style;

use crate::layout::{Track, add_size, allocate_into, horizontal_rect, inset, vertical_rect};
use crate::panel::{BorderGlyphs, content_insets as panel_content_insets, glyphs as border_glyphs};
use crate::rich_text::{ParagraphLine, layout as layout_rich_text};
use crate::routing::{EventHandler, InteractiveKind, NodeRecord, TreeIndex};
use crate::{
    BorderKind, Event, EventResult, InteractionState, Length, NodeId, PanelOptions,
    ParagraphOptions, Rect, ScrollAxis, ScrollOffset, ScrollState, Size, TextSpan, WrapMode,
};

/// Padding widths around a node
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Insets {
    /// Cells above the child
    pub top: u32,
    /// Cells to the child's right
    pub right: u32,
    /// Cells below the child
    pub bottom: u32,
    /// Cells to the child's left
    pub left: u32,
}

impl Insets {
    /// Creates insets in top, right, bottom, left order
    #[must_use]
    pub const fn new(top: u32, right: u32, bottom: u32, left: u32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Creates equal insets on every side
    #[must_use]
    pub const fn all(value: u32) -> Self {
        Self::new(value, value, value, value)
    }
}

/// Horizontal placement inside an alignment node
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum HorizontalAlignment {
    /// Place content at the left edge
    #[default]
    Start,
    /// Center content horizontally
    Center,
    /// Place content at the right edge
    End,
}

/// Vertical placement inside an alignment node
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum VerticalAlignment {
    /// Place content at the top edge
    #[default]
    Start,
    /// Center content vertically
    Center,
    /// Place content at the bottom edge
    End,
}

/// Behavior of a ScrollViewport
pub struct ScrollViewportOptions<Message> {
    /// Axes controlled by user and programmatic scrolling
    pub axis: ScrollAxis,
    /// Whether a viewport at the end follows content growth
    pub stick_to_end: bool,
    /// Whether focus movement scrolls the focused descendant into view
    pub ensure_focused_visible: bool,
    /// Optional application message created after user scrolling changes state
    pub on_scroll: Option<Box<dyn Fn(ScrollState) -> Message>>,
}

impl<Message> Default for ScrollViewportOptions<Message> {
    fn default() -> Self {
        Self {
            axis: ScrollAxis::Both,
            stick_to_end: false,
            ensure_focused_visible: false,
            on_scroll: None,
        }
    }
}

/// Visible content range requested by a virtual ScrollViewport
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct VirtualViewport {
    /// First visible cell in content coordinates
    pub offset: ScrollOffset,
    /// Visible viewport size in cells
    pub size: Size,
    /// Complete resolved content extent in cells
    pub content_size: Size,
}

/// Lazily constructed fragment returned for one [`VirtualViewport`]
pub struct VirtualFragment<Message> {
    origin: ScrollOffset,
    node: Box<Node<Message>>,
}

impl<Message> VirtualFragment<Message> {
    /// Creates a fragment whose node begins at `origin` in content coordinates
    #[must_use]
    pub fn new(origin: ScrollOffset, node: Node<Message>) -> Self {
        Self {
            origin,
            node: Box::new(node),
        }
    }

    /// Returns the fragment origin in content coordinates
    #[must_use]
    pub const fn origin(&self) -> ScrollOffset {
        self.origin
    }
}

/// A semantic view node rebuilt by an application for each frame
pub struct Node<Message> {
    kind: NodeKind<Message>,
    length: Length,
    id: Option<NodeId>,
    focusable: bool,
    focused_style: Option<Style>,
    handler: Option<Box<EventHandler<Message>>>,
    message: PhantomData<fn() -> Message>,
}

enum NodeKind<Message> {
    Text {
        content: String,
        style: Style,
    },
    RichText {
        spans: Vec<TextSpan>,
        options: ParagraphOptions,
    },
    Surface(Surface),
    Spacer(Size),
    Gap(u32),
    TextInput {
        value: String,
        placeholder: String,
        style: Style,
        placeholder_style: Style,
        on_change: Box<dyn Fn(String) -> Message>,
    },
    Row(LinearNode<Message>),
    Column(LinearNode<Message>),
    Stack(Vec<Node<Message>>),
    Padding {
        insets: Insets,
        child: Box<Node<Message>>,
    },
    Border {
        style: Style,
        child: Box<Node<Message>>,
    },
    Align {
        horizontal: HorizontalAlignment,
        vertical: VerticalAlignment,
        child: Box<Node<Message>>,
    },
    Clip(Box<Node<Message>>),
    ScrollViewport {
        child: Box<Node<Message>>,
        options: ScrollViewportOptions<Message>,
    },
    VirtualScrollViewport(Box<VirtualScrollViewportNode<Message>>),
    Modal(Box<Node<Message>>),
    Panel {
        title: String,
        options: PanelOptions,
        child: Box<Node<Message>>,
    },
}

struct VirtualScrollViewportNode<Message> {
    content_size: Size,
    builder: Box<dyn Fn(VirtualViewport) -> VirtualFragment<Message>>,
    cache: RefCell<Option<VirtualCache<Message>>>,
    options: ScrollViewportOptions<Message>,
}

struct VirtualCache<Message> {
    request: VirtualViewport,
    fragment: VirtualFragment<Message>,
}

struct LinearNode<Message> {
    children: Vec<Node<Message>>,
    cache: RefCell<Option<Box<CachedLinearLayout>>>,
}

impl<Message> LinearNode<Message> {
    fn new(children: impl IntoIterator<Item = Node<Message>>) -> Self {
        Self {
            children: children.into_iter().collect(),
            cache: RefCell::new(None),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ScrollBehavior {
    pub(crate) axis: ScrollAxis,
    pub(crate) ensure_focused_visible: bool,
}

#[derive(Clone, Copy)]
enum Limit {
    Bounded(u32),
    Unbounded,
}

#[derive(Clone, Copy)]
struct Constraints {
    width: Limit,
    height: Limit,
}

impl Constraints {
    const fn bounded(size: Size) -> Self {
        Self {
            width: Limit::Bounded(size.width),
            height: Limit::Bounded(size.height),
        }
    }
}

impl<Message> Node<Message> {
    /// Creates a default-style text node
    #[must_use]
    pub fn text(content: impl Into<String>) -> Self {
        Self::styled_text(content, Style::default())
    }

    /// Creates a styled text node
    #[must_use]
    pub fn styled_text(content: impl Into<String>, style: Style) -> Self {
        Self::new(NodeKind::Text {
            content: content.into(),
            style,
        })
    }

    /// Creates inline styled text with grapheme-safe hard wrapping
    #[must_use]
    pub fn rich_text(spans: impl IntoIterator<Item = TextSpan>) -> Self {
        Self::new(NodeKind::RichText {
            spans: spans.into_iter().collect(),
            options: ParagraphOptions {
                wrap: WrapMode::Hard,
                ..ParagraphOptions::default()
            },
        })
    }

    /// Creates inline styled text using supplied wrapping and alignment
    #[must_use]
    pub fn paragraph(spans: impl IntoIterator<Item = TextSpan>, options: ParagraphOptions) -> Self {
        Self::new(NodeKind::RichText {
            spans: spans.into_iter().collect(),
            options,
        })
    }

    /// Captures an owned public Surface as a semantic node
    ///
    /// Surface cells remain typed and cannot introduce raw terminal escape
    /// sequences
    #[must_use]
    pub fn surface(surface: Surface) -> Self {
        Self::new(NodeKind::Surface(surface))
    }

    /// Creates an invisible node with a fixed measured size
    #[must_use]
    pub fn spacer(width: u32, height: u32) -> Self {
        Self::new(NodeKind::Spacer(Size::new(width, height)))
    }

    /// Creates spacing along the main axis of its immediate Row or Column
    ///
    /// Outside a Row or Column, a gap has zero measured size
    #[must_use]
    pub fn gap(cells: u32) -> Self {
        Self::new(NodeKind::Gap(cells))
    }

    /// Creates a horizontal container
    #[must_use]
    pub fn row(children: impl IntoIterator<Item = Self>) -> Self {
        Self::new(NodeKind::Row(LinearNode::new(children)))
    }

    /// Creates a vertical container
    #[must_use]
    pub fn column(children: impl IntoIterator<Item = Self>) -> Self {
        Self::new(NodeKind::Column(LinearNode::new(children)))
    }

    /// Creates a front-to-back overlay container
    #[must_use]
    pub fn stack(children: impl IntoIterator<Item = Self>) -> Self {
        Self::new(NodeKind::Stack(children.into_iter().collect()))
    }

    /// Wraps a child in fixed padding
    #[must_use]
    pub fn padding(child: Self, insets: Insets) -> Self {
        Self::new(NodeKind::Padding {
            insets,
            child: Box::new(child),
        })
    }

    /// Wraps a child in a single-cell Unicode border
    #[must_use]
    pub fn border(child: Self, style: Style) -> Self {
        Self::new(NodeKind::Border {
            style,
            child: Box::new(child),
        })
    }

    /// Creates a titled single-border container with one-cell inner padding
    #[must_use]
    pub fn panel(child: Self, title: impl Into<String>) -> Self {
        Self::panel_with_options(child, title, PanelOptions::default())
    }

    /// Creates a titled container with configured border, padding, and styles
    #[must_use]
    pub fn panel_with_options(
        child: Self,
        title: impl Into<String>,
        options: PanelOptions,
    ) -> Self {
        Self::new(NodeKind::Panel {
            title: title.into(),
            options,
            child: Box::new(child),
        })
    }

    /// Aligns a child within the rectangle assigned to this node
    #[must_use]
    pub fn align(
        child: Self,
        horizontal: HorizontalAlignment,
        vertical: VerticalAlignment,
    ) -> Self {
        Self::new(NodeKind::Align {
            horizontal,
            vertical,
            child: Box::new(child),
        })
    }

    /// Clips a child's drawing to the assigned rectangle
    #[must_use]
    pub fn clip(child: Self) -> Self {
        Self::new(NodeKind::Clip(Box::new(child)))
    }

    /// Creates a one-line grapheme-aware text input with retained cursor state
    #[must_use]
    pub fn text_input(
        id: impl Into<NodeId>,
        value: impl Into<String>,
        on_change: impl Fn(String) -> Message + 'static,
    ) -> Self {
        Self::text_input_styled(
            id,
            value,
            "",
            Style::default(),
            Style {
                dim: true,
                ..Style::default()
            },
            on_change,
        )
    }

    /// Creates a styled one-line text input with placeholder text
    #[must_use]
    pub fn text_input_styled(
        id: impl Into<NodeId>,
        value: impl Into<String>,
        placeholder: impl Into<String>,
        style: Style,
        placeholder_style: Style,
        on_change: impl Fn(String) -> Message + 'static,
    ) -> Self {
        let mut node = Self::new(NodeKind::TextInput {
            value: value.into(),
            placeholder: placeholder.into(),
            style,
            placeholder_style,
            on_change: Box::new(on_change),
        });
        node.id = Some(id.into());
        node.focusable = true;
        node
    }

    /// Creates a clipped viewport with runtime-owned two-dimensional offset
    ///
    /// The supplied child tree is eager. Use [`Node::virtual_scroll_viewport`]
    /// when content construction must be bounded by the visible region
    #[must_use]
    pub fn scroll_viewport(id: impl Into<NodeId>, child: Self) -> Self {
        Self::scroll_viewport_with_options(id, child, ScrollViewportOptions::default())
    }

    /// Creates a clipped viewport with configured scrolling behavior
    ///
    /// The supplied child tree is eager. Use
    /// [`Node::virtual_scroll_viewport_with_options`] when content construction
    /// must be bounded by the visible region
    #[must_use]
    pub fn scroll_viewport_with_options(
        id: impl Into<NodeId>,
        child: Self,
        options: ScrollViewportOptions<Message>,
    ) -> Self {
        let mut node = Self::new(NodeKind::ScrollViewport {
            child: Box::new(child),
            options,
        });
        node.id = Some(id.into());
        node.focusable = true;
        node
    }

    /// Creates a viewport that constructs only a visible content fragment
    ///
    /// `content_size` declares the complete scrollable cell extent without
    /// constructing it. The builder receives the resolved visible range and is
    /// cached for that range during the semantic frame. It may include bounded
    /// overscan by returning an origin before the requested offset
    #[must_use]
    pub fn virtual_scroll_viewport(
        id: impl Into<NodeId>,
        content_size: Size,
        builder: impl Fn(VirtualViewport) -> VirtualFragment<Message> + 'static,
    ) -> Self {
        Self::virtual_scroll_viewport_with_options(
            id,
            content_size,
            ScrollViewportOptions::default(),
            builder,
        )
    }

    /// Creates a virtual viewport with configured scrolling behavior
    ///
    /// Only the cached fragment participates in measurement, rendering, focus,
    /// hit testing, and event routing. Fragment Node IDs therefore represent
    /// visible or overscanned content and must remain stable across requests
    #[must_use]
    pub fn virtual_scroll_viewport_with_options(
        id: impl Into<NodeId>,
        content_size: Size,
        options: ScrollViewportOptions<Message>,
        builder: impl Fn(VirtualViewport) -> VirtualFragment<Message> + 'static,
    ) -> Self {
        let mut node = Self::new(NodeKind::VirtualScrollViewport(Box::new(
            VirtualScrollViewportNode {
                content_size,
                builder: Box::new(builder),
                cache: RefCell::new(None),
                options,
            },
        )));
        node.id = Some(id.into());
        node.focusable = true;
        node
    }

    /// Marks a subtree as the active modal routing and focus scope
    #[must_use]
    pub fn modal(id: impl Into<NodeId>, child: Self) -> Self {
        let mut node = Self::new(NodeKind::Modal(Box::new(child)));
        node.id = Some(id.into());
        node
    }

    /// Attaches a stable semantic identity without changing focus behavior
    #[must_use]
    pub fn with_id(mut self, id: impl Into<NodeId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Makes this node focusable under a stable identity
    #[must_use]
    pub fn focusable(mut self, id: impl Into<NodeId>) -> Self {
        self.id = Some(id.into());
        self.focusable = true;
        self
    }

    /// Controls Tab traversal participation without changing the stable ID
    #[must_use]
    pub const fn tab_stop(mut self, enabled: bool) -> Self {
        self.focusable = enabled;
        self
    }

    /// Merges a style over this node's clipped rectangle while it owns focus
    ///
    /// The overlay does not change measurement, layout, hit testing, or event
    /// routing. The node must also have a stable identity, normally through
    /// [`Node::focusable`] or [`Node::on_event`]
    #[must_use]
    pub fn with_focused_style(mut self, style: Style) -> Self {
        self.focused_style = Some(style);
        self
    }

    /// Attaches an event handler under a stable identity
    #[must_use]
    pub fn on_event(
        mut self,
        id: impl Into<NodeId>,
        handler: impl Fn(&Event) -> EventResult<Message> + 'static,
    ) -> Self {
        self.id = Some(id.into());
        self.handler = Some(Box::new(handler));
        self
    }

    /// Sets this node's main-axis sizing rule in a row or column
    #[must_use]
    pub fn with_length(mut self, length: Length) -> Self {
        self.length = length;
        self
    }

    fn new(kind: NodeKind<Message>) -> Self {
        Self {
            kind,
            length: Length::Auto,
            id: None,
            focusable: false,
            focused_style: None,
            handler: None,
            message: PhantomData,
        }
    }

    pub(crate) fn render_to(&self, surface: &mut Surface, interaction: &InteractionState) {
        let bounds = Rect::new(0, 0, surface.width(), surface.height());
        self.render(surface, bounds, bounds, interaction);
    }

    fn measure(&self, constraints: Constraints) -> Size {
        let measured = match &self.kind {
            NodeKind::Text { content, .. } => measure_text(content, constraints),
            NodeKind::RichText { spans, options } => {
                measure_rich_text(spans, *options, constraints)
            }
            NodeKind::Surface(surface) => Size::new(surface.width(), surface.height()),
            NodeKind::Spacer(size) => *size,
            NodeKind::Gap(_) => Size::default(),
            NodeKind::TextInput {
                value, placeholder, ..
            } => {
                let width = text_width(value, WidthProfile::MODERN)
                    .max(text_width(placeholder, WidthProfile::MODERN))
                    .min(u32::MAX as usize) as u32;
                Size::new(width, 1)
            }
            NodeKind::Row(linear) => measure_linear(&linear.children, constraints, true),
            NodeKind::Column(linear) => measure_linear(&linear.children, constraints, false),
            NodeKind::Stack(children) => children.iter().fold(Size::default(), |size, child| {
                let child = child.measure(constraints);
                Size::new(size.width.max(child.width), size.height.max(child.height))
            }),
            NodeKind::Padding { insets, child } => add_size(
                child.measure(shrink_constraints(constraints, *insets)),
                insets.left.saturating_add(insets.right),
                insets.top.saturating_add(insets.bottom),
            ),
            NodeKind::Border { child, .. } => add_size(
                child.measure(shrink_constraints(constraints, Insets::all(1))),
                2,
                2,
            ),
            NodeKind::Panel { options, child, .. } => {
                let insets = panel_content_insets(*options);
                add_size(
                    child.measure(shrink_constraints(constraints, insets)),
                    insets.left.saturating_add(insets.right),
                    insets.top.saturating_add(insets.bottom),
                )
            }
            NodeKind::Align { child, .. } | NodeKind::Clip(child) | NodeKind::Modal(child) => {
                child.measure(constraints)
            }
            NodeKind::ScrollViewport { child, .. } => child.measure(constraints),
            NodeKind::VirtualScrollViewport(virtual_node) => virtual_node.content_size,
        };
        clamp_size(measured, constraints)
    }

    fn render(
        &self,
        surface: &mut Surface,
        rect: Rect,
        clip: Rect,
        interaction: &InteractionState,
    ) {
        match &self.kind {
            NodeKind::Text { content, style } => {
                render_text(surface, rect, clip, content, *style);
            }
            NodeKind::RichText { spans, options } => {
                render_rich_text(surface, rect, clip, spans, *options);
            }
            NodeKind::Surface(source) => render_surface_node(surface, rect, clip, source),
            NodeKind::Spacer(_) | NodeKind::Gap(_) => {}
            NodeKind::TextInput {
                value,
                placeholder,
                style,
                placeholder_style,
                ..
            } => render_text_input(
                surface,
                rect,
                clip,
                self.id.as_ref().expect("TextInput always has a NodeId"),
                value,
                placeholder,
                *style,
                *placeholder_style,
                interaction,
            ),
            NodeKind::Row(linear) => render_linear(surface, rect, clip, linear, true, interaction),
            NodeKind::Column(linear) => {
                render_linear(surface, rect, clip, linear, false, interaction)
            }
            NodeKind::Stack(children) => {
                for child in children {
                    child.render(surface, rect, clip, interaction);
                }
            }
            NodeKind::Padding { insets, child } => {
                let child_rect = inset(rect, insets.left, insets.top, insets.right, insets.bottom);
                child.render(surface, child_rect, clip, interaction);
            }
            NodeKind::Border { style, child } => {
                render_border(surface, rect, clip, *style);
                child.render(surface, inset(rect, 1, 1, 1, 1), clip, interaction);
            }
            NodeKind::Align {
                horizontal,
                vertical,
                child,
            } => {
                child.render(
                    surface,
                    aligned_child_rect(rect, child, *horizontal, *vertical),
                    clip,
                    interaction,
                );
            }
            NodeKind::Clip(child) => {
                child.render(surface, rect, clip.intersection(rect), interaction)
            }
            NodeKind::ScrollViewport { child, options } => {
                let id = self
                    .id
                    .as_ref()
                    .expect("ScrollViewport always has a NodeId");
                let child_rect =
                    scroll_child_rect(rect, child, interaction.scroll_offset(id), options.axis);
                child.render(surface, child_rect, clip.intersection(rect), interaction);
            }
            NodeKind::VirtualScrollViewport(virtual_node) => {
                let id = self
                    .id
                    .as_ref()
                    .expect("VirtualScrollViewport always has a NodeId");
                if let Some(fragment) = virtual_fragment(
                    virtual_node.content_size,
                    virtual_node.options.axis,
                    virtual_node.builder.as_ref(),
                    &virtual_node.cache,
                    rect,
                    interaction.scroll_offset(id),
                ) {
                    let fragment_rect = virtual_fragment_rect(rect, &fragment);
                    fragment.fragment.node.render(
                        surface,
                        fragment_rect,
                        clip.intersection(rect),
                        interaction,
                    );
                }
            }
            NodeKind::Modal(child) => child.render(surface, rect, clip, interaction),
            NodeKind::Panel {
                title,
                options,
                child,
            } => render_panel(surface, rect, clip, child, title, *options, interaction),
        }
        let focused = self
            .id
            .as_ref()
            .is_some_and(|id| interaction.focused() == Some(id));
        if focused {
            if let Some(style) = self.focused_style {
                let overlay = rect.intersection(clip);
                merge_node_style(surface, overlay, style);
            }
        }
    }
}

impl<Message> Node<Message> {
    pub(crate) fn build_tree_index_into(
        &self,
        size: Size,
        interaction: &InteractionState,
        index: &mut TreeIndex,
    ) -> Result<(), NodeId> {
        let bounds = Rect::new(0, 0, size.width, size.height);
        index.clear();
        self.build_index(bounds, bounds, None, true, interaction, index)
    }

    pub(crate) fn prepare_interaction(&self, size: Size, interaction: &mut InteractionState) {
        let bounds = Rect::new(0, 0, size.width, size.height);
        self.prepare_at(bounds, interaction);
    }

    pub(crate) fn handle_event(&self, id: &NodeId, event: &Event) -> Option<EventResult<Message>> {
        self.visit(id, &mut |node| {
            node.handler.as_ref().map(|handler| handler(event))
        })
    }

    pub(crate) fn text_input_message(&self, id: &NodeId, value: String) -> Option<Message> {
        let mut value = Some(value);
        self.visit(id, &mut |node| {
            let NodeKind::TextInput { on_change, .. } = &node.kind else {
                return None;
            };
            Some(on_change(
                value.take().expect("a NodeId is unique within one view"),
            ))
        })
    }

    pub(crate) fn scroll_options(&self, id: &NodeId) -> Option<ScrollBehavior> {
        self.visit(id, &mut |node| {
            let options = match &node.kind {
                NodeKind::ScrollViewport { options, .. } => options,
                NodeKind::VirtualScrollViewport(virtual_node) => &virtual_node.options,
                _ => return None,
            };
            Some(ScrollBehavior {
                axis: options.axis,
                ensure_focused_visible: options.ensure_focused_visible,
            })
        })
    }

    pub(crate) fn scroll_message(&self, id: &NodeId, state: ScrollState) -> Option<Message> {
        self.visit(id, &mut |node| {
            let options = match &node.kind {
                NodeKind::ScrollViewport { options, .. } => options,
                NodeKind::VirtualScrollViewport(virtual_node) => &virtual_node.options,
                _ => return None,
            };
            options.on_scroll.as_ref().map(|map| map(state))
        })
    }

    fn visit<Result>(
        &self,
        id: &NodeId,
        operation: &mut impl FnMut(&Self) -> Option<Result>,
    ) -> Option<Result> {
        if self.id.as_ref() == Some(id) {
            return operation(self);
        }
        match &self.kind {
            NodeKind::Row(linear) | NodeKind::Column(linear) => linear
                .children
                .iter()
                .find_map(|child| child.visit(id, operation)),
            NodeKind::Stack(children) => {
                children.iter().find_map(|child| child.visit(id, operation))
            }
            NodeKind::Padding { child, .. }
            | NodeKind::Border { child, .. }
            | NodeKind::Align { child, .. }
            | NodeKind::Clip(child)
            | NodeKind::Modal(child)
            | NodeKind::Panel { child, .. }
            | NodeKind::ScrollViewport { child, .. } => child.visit(id, operation),
            NodeKind::VirtualScrollViewport(virtual_node) => virtual_node
                .cache
                .borrow()
                .as_ref()
                .and_then(|cached| cached.fragment.node.visit(id, operation)),
            NodeKind::Text { .. }
            | NodeKind::RichText { .. }
            | NodeKind::Surface(_)
            | NodeKind::Spacer(_)
            | NodeKind::Gap(_)
            | NodeKind::TextInput { .. } => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_index(
        &self,
        rect: Rect,
        clip: Rect,
        parent: Option<&NodeId>,
        is_root: bool,
        interaction: &InteractionState,
        index: &mut TreeIndex,
    ) -> Result<(), NodeId> {
        let mut child_parent = parent.cloned();
        if let Some(id) = &self.id {
            let kind = match self.kind {
                NodeKind::TextInput { .. } => InteractiveKind::TextInput,
                NodeKind::ScrollViewport { .. } | NodeKind::VirtualScrollViewport(_) => {
                    InteractiveKind::ScrollViewport
                }
                NodeKind::Modal(_) => InteractiveKind::Modal,
                _ => InteractiveKind::Generic,
            };
            index.register(
                NodeRecord {
                    id: id.clone(),
                    parent: parent.cloned(),
                    rect,
                    clip,
                    focusable: self.focusable,
                    has_handler: self.handler.is_some(),
                    kind,
                },
                is_root,
            )?;
            child_parent = Some(id.clone());
        }
        let parent = child_parent.as_ref();
        match &self.kind {
            NodeKind::Text { .. }
            | NodeKind::RichText { .. }
            | NodeKind::Surface(_)
            | NodeKind::Spacer(_)
            | NodeKind::Gap(_)
            | NodeKind::TextInput { .. } => {}
            NodeKind::Row(linear) => {
                let layout = linear.layout(rect, true);
                for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
                    child.build_index(child_rect, clip, parent, false, interaction, index)?;
                }
            }
            NodeKind::Column(linear) => {
                let layout = linear.layout(rect, false);
                for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
                    child.build_index(child_rect, clip, parent, false, interaction, index)?;
                }
            }
            NodeKind::Stack(children) => {
                for child in children {
                    child.build_index(rect, clip, parent, false, interaction, index)?;
                }
            }
            NodeKind::Padding { insets, child } => child.build_index(
                inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                clip,
                parent,
                false,
                interaction,
                index,
            )?,
            NodeKind::Border { child, .. } => child.build_index(
                inset(rect, 1, 1, 1, 1),
                clip,
                parent,
                false,
                interaction,
                index,
            )?,
            NodeKind::Align {
                horizontal,
                vertical,
                child,
            } => child.build_index(
                aligned_child_rect(rect, child, *horizontal, *vertical),
                clip,
                parent,
                false,
                interaction,
                index,
            )?,
            NodeKind::Clip(child) => child.build_index(
                rect,
                clip.intersection(rect),
                parent,
                false,
                interaction,
                index,
            )?,
            NodeKind::ScrollViewport { child, options } => {
                let id = self
                    .id
                    .as_ref()
                    .expect("ScrollViewport always has a NodeId");
                child.build_index(
                    scroll_child_rect(rect, child, interaction.scroll_offset(id), options.axis),
                    clip.intersection(rect),
                    parent,
                    false,
                    interaction,
                    index,
                )?;
            }
            NodeKind::VirtualScrollViewport(virtual_node) => {
                let id = self
                    .id
                    .as_ref()
                    .expect("VirtualScrollViewport always has a NodeId");
                if let Some(fragment) = virtual_fragment(
                    virtual_node.content_size,
                    virtual_node.options.axis,
                    virtual_node.builder.as_ref(),
                    &virtual_node.cache,
                    rect,
                    interaction.scroll_offset(id),
                ) {
                    fragment.fragment.node.build_index(
                        virtual_fragment_rect(rect, &fragment),
                        clip.intersection(rect),
                        parent,
                        false,
                        interaction,
                        index,
                    )?;
                }
            }
            NodeKind::Modal(child) => {
                child.build_index(rect, clip, parent, false, interaction, index)?
            }
            NodeKind::Panel { options, child, .. } => {
                let insets = panel_content_insets(*options);
                child.build_index(
                    inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                    clip,
                    parent,
                    false,
                    interaction,
                    index,
                )?;
            }
        }
        Ok(())
    }

    fn prepare_at(&self, rect: Rect, interaction: &mut InteractionState) {
        match &self.kind {
            NodeKind::TextInput { value, .. } => {
                interaction.ensure_text_input(
                    self.id.as_ref().expect("TextInput always has a NodeId"),
                    value,
                );
            }
            NodeKind::ScrollViewport { child, options } => {
                let id = self
                    .id
                    .as_ref()
                    .expect("ScrollViewport always has a NodeId");
                let content = child.measure(scroll_constraints(rect, options.axis));
                let width = content.width.max(rect.width);
                let height = content.height.max(rect.height);
                let state = interaction.prepare_scroll(
                    id,
                    ScrollOffset::new(
                        width.saturating_sub(rect.width),
                        height.saturating_sub(rect.height),
                    ),
                    options.axis,
                    options.stick_to_end,
                );
                child.prepare_at(
                    scroll_child_rect(rect, child, state.offset, options.axis),
                    interaction,
                );
                return;
            }
            NodeKind::VirtualScrollViewport(virtual_node) => {
                let id = self
                    .id
                    .as_ref()
                    .expect("VirtualScrollViewport always has a NodeId");
                let content = resolved_virtual_content_size(
                    virtual_node.content_size,
                    rect.size(),
                    virtual_node.options.axis,
                );
                let state = interaction.prepare_scroll(
                    id,
                    ScrollOffset::new(
                        content.width.saturating_sub(rect.width),
                        content.height.saturating_sub(rect.height),
                    ),
                    virtual_node.options.axis,
                    virtual_node.options.stick_to_end,
                );
                if let Some(fragment) = virtual_fragment(
                    virtual_node.content_size,
                    virtual_node.options.axis,
                    virtual_node.builder.as_ref(),
                    &virtual_node.cache,
                    rect,
                    state.offset,
                ) {
                    fragment
                        .fragment
                        .node
                        .prepare_at(virtual_fragment_rect(rect, &fragment), interaction);
                }
                return;
            }
            _ => {}
        }
        match &self.kind {
            NodeKind::Row(linear) => {
                let layout = linear.layout(rect, true);
                for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
                    child.prepare_at(child_rect, interaction);
                }
            }
            NodeKind::Column(linear) => {
                let layout = linear.layout(rect, false);
                for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
                    child.prepare_at(child_rect, interaction);
                }
            }
            NodeKind::Stack(children) => {
                for child in children {
                    child.prepare_at(rect, interaction);
                }
            }
            NodeKind::Padding { insets, child } => child.prepare_at(
                inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                interaction,
            ),
            NodeKind::Border { child, .. } => {
                child.prepare_at(inset(rect, 1, 1, 1, 1), interaction)
            }
            NodeKind::Align {
                horizontal,
                vertical,
                child,
            } => child.prepare_at(
                aligned_child_rect(rect, child, *horizontal, *vertical),
                interaction,
            ),
            NodeKind::Clip(child) => child.prepare_at(rect, interaction),
            NodeKind::Modal(child) => child.prepare_at(rect, interaction),
            NodeKind::Panel { options, child, .. } => {
                let insets = panel_content_insets(*options);
                child.prepare_at(
                    inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                    interaction,
                );
            }
            NodeKind::Text { .. }
            | NodeKind::RichText { .. }
            | NodeKind::Surface(_)
            | NodeKind::Spacer(_)
            | NodeKind::Gap(_)
            | NodeKind::TextInput { .. }
            | NodeKind::ScrollViewport { .. }
            | NodeKind::VirtualScrollViewport(_) => {}
        }
    }
}

fn measure_text(content: &str, constraints: Constraints) -> Size {
    let max_width = match constraints.width {
        Limit::Bounded(width) => width as usize,
        Limit::Unbounded => usize::MAX,
    };
    let mut width = 0_usize;
    let mut height = 0_usize;
    for line in wrapped_lines(content, max_width, WidthProfile::MODERN) {
        width = width.max(line.width());
        height = height.saturating_add(1);
    }
    Size::new(
        width.min(u32::MAX as usize) as u32,
        height.min(u32::MAX as usize) as u32,
    )
}

fn measure_rich_text(
    spans: &[TextSpan],
    options: ParagraphOptions,
    constraints: Constraints,
) -> Size {
    let (max_width, bounded) = match constraints.width {
        Limit::Bounded(width) => (width, true),
        Limit::Unbounded => (0, false),
    };
    let lines = layout_rich_text(spans, max_width, bounded, options.wrap);
    Size::new(
        lines.iter().map(|line| line.width).max().unwrap_or(0),
        lines.len().min(u32::MAX as usize) as u32,
    )
}

fn measure_linear<Message>(
    children: &[Node<Message>],
    constraints: Constraints,
    horizontal: bool,
) -> Size {
    let child_constraints = if horizontal {
        Constraints {
            width: Limit::Unbounded,
            height: constraints.height,
        }
    } else {
        Constraints {
            width: constraints.width,
            height: Limit::Unbounded,
        }
    };
    children.iter().fold(Size::default(), |size, child| {
        let mut measured = child.measure(child_constraints);
        if let NodeKind::Gap(cells) = &child.kind {
            if horizontal {
                measured.width = *cells;
            } else {
                measured.height = *cells;
            }
        }
        if horizontal {
            Size::new(
                size.width.saturating_add(measured.width),
                size.height.max(measured.height),
            )
        } else {
            Size::new(
                size.width.max(measured.width),
                size.height.saturating_add(measured.height),
            )
        }
    })
}

fn shrink_constraints(constraints: Constraints, insets: Insets) -> Constraints {
    Constraints {
        width: subtract_limit(constraints.width, insets.left.saturating_add(insets.right)),
        height: subtract_limit(constraints.height, insets.top.saturating_add(insets.bottom)),
    }
}

fn subtract_limit(limit: Limit, value: u32) -> Limit {
    match limit {
        Limit::Bounded(bound) => Limit::Bounded(bound.saturating_sub(value)),
        Limit::Unbounded => Limit::Unbounded,
    }
}

fn clamp_size(mut size: Size, constraints: Constraints) -> Size {
    if let Limit::Bounded(width) = constraints.width {
        size.width = size.width.min(width);
    }
    if let Limit::Bounded(height) = constraints.height {
        size.height = size.height.min(height);
    }
    size
}

fn merge_node_style(surface: &mut Surface, rect: Rect, overlay: Style) {
    for y_offset in 0..rect.height {
        let y = add_coordinate(rect.y, y_offset);
        for x_offset in 0..rect.width {
            let x = add_coordinate(rect.x, x_offset);
            let Some(style) = surface.cell(x, y).map(|cell| cell.style()) else {
                continue;
            };
            surface.set_style(x, y, style.merged(overlay));
        }
    }
}

fn render_linear<Message>(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    linear: &LinearNode<Message>,
    horizontal: bool,
    interaction: &InteractionState,
) {
    let layout = linear.layout(rect, horizontal);
    for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
        child.render(surface, child_rect, clip, interaction);
    }
}

const INLINE_LINEAR_CHILDREN: usize = 32;

struct CachedLinearLayout {
    rect: Rect,
    layout: LinearLayout,
}

struct LinearLayout {
    horizontal: bool,
    inline: [u32; INLINE_LINEAR_CHILDREN],
    overflow: Vec<u32>,
    len: usize,
}

impl<Message> LinearNode<Message> {
    fn layout(&self, rect: Rect, horizontal: bool) -> Ref<'_, LinearLayout> {
        let rebuild = self
            .cache
            .borrow()
            .as_ref()
            .is_none_or(|cached| cached.rect != rect || cached.layout.horizontal != horizontal);
        if rebuild {
            let layout = resolve_linear_layout(&self.children, rect, horizontal);
            *self.cache.borrow_mut() = Some(Box::new(CachedLinearLayout { rect, layout }));
        }
        Ref::map(self.cache.borrow(), |cached| {
            &cached
                .as_ref()
                .expect("linear layout cache is initialized")
                .layout
        })
    }
}

impl LinearLayout {
    fn rects(&self, rect: Rect) -> LinearRects<'_> {
        LinearRects {
            layout: self,
            rect,
            index: 0,
            offset: 0,
        }
    }

    fn allocation(&self, index: usize) -> u32 {
        if self.overflow.is_empty() {
            self.inline[index]
        } else {
            self.overflow[index]
        }
    }
}

struct LinearRects<'layout> {
    layout: &'layout LinearLayout,
    rect: Rect,
    index: usize,
    offset: u32,
}

impl Iterator for LinearRects<'_> {
    type Item = Rect;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.layout.len {
            return None;
        }
        let allocated = self.layout.allocation(self.index);
        let child_rect = if self.layout.horizontal {
            horizontal_rect(self.rect, self.offset, allocated)
        } else {
            vertical_rect(self.rect, self.offset, allocated)
        };
        self.index += 1;
        self.offset = self.offset.saturating_add(allocated);
        Some(child_rect)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.layout.len - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for LinearRects<'_> {}

fn resolve_linear_layout<Message>(
    children: &[Node<Message>],
    rect: Rect,
    horizontal: bool,
) -> LinearLayout {
    let available = if horizontal { rect.width } else { rect.height };
    let mut layout = LinearLayout {
        horizontal,
        inline: [0; INLINE_LINEAR_CHILDREN],
        overflow: Vec::new(),
        len: children.len(),
    };
    if children.len() <= INLINE_LINEAR_CHILDREN {
        let mut tracks = [Track {
            length: Length::Auto,
            desired: 0,
        }; INLINE_LINEAR_CHILDREN];
        let mut minimums = [0; INLINE_LINEAR_CHILDREN];
        fill_linear_tracks(children, rect, horizontal, &mut tracks[..children.len()]);
        allocate_into(
            available,
            &tracks[..children.len()],
            &mut layout.inline[..children.len()],
            &mut minimums[..children.len()],
        );
        return layout;
    }

    let mut tracks = vec![
        Track {
            length: Length::Auto,
            desired: 0,
        };
        children.len()
    ];
    let mut minimums = vec![0; children.len()];
    layout.overflow = vec![0; children.len()];
    fill_linear_tracks(children, rect, horizontal, &mut tracks);
    allocate_into(available, &tracks, &mut layout.overflow, &mut minimums);
    layout
}

fn fill_linear_tracks<Message>(
    children: &[Node<Message>],
    rect: Rect,
    horizontal: bool,
    tracks: &mut [Track],
) {
    for (index, child) in children.iter().enumerate() {
        let measured = child.measure(Constraints::bounded(rect.size()));
        let desired = match &child.kind {
            NodeKind::Gap(cells) => *cells,
            _ if horizontal => measured.width,
            _ => measured.height,
        };
        tracks[index] = Track {
            length: child.length,
            desired,
        };
    }
}

fn render_text(surface: &mut Surface, rect: Rect, clip: Rect, content: &str, style: Style) {
    if rect.is_empty() {
        return;
    }
    let lines = wrapped_lines(content, rect.width as usize, WidthProfile::MODERN);
    for (line_index, line) in lines.take(rect.height as usize).enumerate() {
        let y = add_coordinate(rect.y, line_index as u32);
        let mut x = i64::from(rect.x);
        for grapheme in graphemes(line.text()) {
            let span = grapheme_width(grapheme.text(), WidthProfile::MODERN).max(1) as i64;
            let end = x.saturating_add(span);
            if contains_unit(clip, x, i64::from(y), end) {
                surface.write(
                    clamp_i64_to_i32(x),
                    y,
                    grapheme.text(),
                    style,
                    WidthProfile::MODERN,
                );
            }
            x = end;
        }
    }
}

fn render_rich_text(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    spans: &[TextSpan],
    options: ParagraphOptions,
) {
    if rect.is_empty() {
        return;
    }
    let lines = layout_rich_text(spans, rect.width, true, options.wrap);
    for (line_index, line) in lines.into_iter().take(rect.height as usize).enumerate() {
        render_rich_text_line(surface, rect, clip, line_index as u32, &line, options);
    }
}

fn render_rich_text_line(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    line_index: u32,
    line: &ParagraphLine,
    options: ParagraphOptions,
) {
    let desired = line.width.min(rect.width);
    let mut x = i64::from(add_coordinate(
        rect.x,
        alignment_offset(rect.width, desired, options.alignment),
    ));
    let right = i64::from(rect.x) + i64::from(rect.width);
    let y = add_coordinate(rect.y, line_index);
    for unit in &line.units {
        let end = x.saturating_add(i64::from(unit.width));
        if end > right {
            break;
        }
        if contains_unit(clip, x, i64::from(y), end) {
            surface.write(
                clamp_i64_to_i32(x),
                y,
                &unit.text,
                unit.style,
                WidthProfile::MODERN,
            );
        }
        x = end;
    }
}

fn render_surface_node(surface: &mut Surface, rect: Rect, clip: Rect, source: &Surface) {
    if rect.is_empty() {
        return;
    }
    let visible_width = rect.width.min(source.width());
    let visible_height = rect.height.min(source.height());
    for source_y in 0..visible_height {
        let target_y = add_coordinate(rect.y, source_y);
        for source_x in 0..visible_width {
            let Some(cell) = source.cell(source_x as i32, source_y as i32) else {
                continue;
            };
            if cell.is_continuation() {
                continue;
            }
            let span = cell.span().cells().min(u32::MAX as usize) as u32;
            if source_x.saturating_add(span) > visible_width {
                continue;
            }
            let target_x = add_coordinate(rect.x, source_x);
            let end = i64::from(target_x) + i64::from(span);
            if !contains_unit(clip, i64::from(target_x), i64::from(target_y), end) {
                continue;
            }
            if cell.opacity() == nagi_surface::Opacity::Transparent {
                if let Some(destination) = surface.cell(target_x, target_y) {
                    let style = destination.style().merged(cell.style());
                    surface.set_style(target_x, target_y, style);
                }
            } else {
                surface.write(
                    target_x,
                    target_y,
                    cell.content(),
                    cell.style(),
                    WidthProfile::MODERN,
                );
            }
        }
    }
    if let Some(cursor) = source.cursor() {
        let point = crate::Point::new(
            add_coordinate(rect.x, cursor.x),
            add_coordinate(rect.y, cursor.y),
        );
        if cursor.x < visible_width
            && cursor.y < visible_height
            && point.x >= 0
            && point.y >= 0
            && clip.contains(point)
        {
            let _ = surface.set_cursor(Some(Cursor::new(point.x as u32, point.y as u32)));
        } else {
            let _ = surface.set_cursor(None);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_text_input(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    id: &NodeId,
    value: &str,
    placeholder: &str,
    style: Style,
    placeholder_style: Style,
    interaction: &InteractionState,
) {
    if rect.is_empty() {
        return;
    }
    let state = interaction.text_input(id);
    let cursor = state.map_or(value.len(), |state| state.cursor());
    let focused = interaction.focused() == Some(id);
    let content = if value.is_empty() { placeholder } else { value };
    let content_style = if value.is_empty() {
        placeholder_style
    } else {
        style
    };
    let cursor_cell = if value.is_empty() {
        0
    } else {
        cell_at_byte(value, cursor, WidthProfile::MODERN).unwrap_or(0)
    };
    let visible_width = rect.width as usize;
    let requested_start = cursor_cell.saturating_sub(visible_width.saturating_sub(1));
    let mut start_cell = requested_start;
    let start_byte = loop {
        if let Some(byte) = byte_at_cell(content, start_cell, WidthProfile::MODERN) {
            break byte;
        }
        if start_cell == 0 {
            break 0;
        }
        start_cell -= 1;
    };
    render_single_line(surface, rect, clip, &content[start_byte..], content_style);

    if focused {
        let relative = cursor_cell
            .saturating_sub(start_cell)
            .min(visible_width - 1) as u32;
        let point = crate::Point::new(add_coordinate(rect.x, relative), rect.y);
        if clip.contains(point) && point.x >= 0 && point.y >= 0 {
            let _ = surface.set_cursor(Some(Cursor::new(point.x as u32, point.y as u32)));
        }
    }
}

fn render_single_line(surface: &mut Surface, rect: Rect, clip: Rect, content: &str, style: Style) {
    let mut x = i64::from(rect.x);
    let right = i64::from(rect.x) + i64::from(rect.width);
    for grapheme in graphemes(content) {
        let span = grapheme_width(grapheme.text(), WidthProfile::MODERN).max(1) as i64;
        let end = x.saturating_add(span);
        if end > right {
            break;
        }
        if contains_unit(clip, x, i64::from(rect.y), end) {
            surface.write(
                clamp_i64_to_i32(x),
                rect.y,
                grapheme.text(),
                style,
                WidthProfile::MODERN,
            );
        }
        x = end;
    }
}

fn scroll_child_rect<Message>(
    viewport: Rect,
    child: &Node<Message>,
    requested: ScrollOffset,
    axis: ScrollAxis,
) -> Rect {
    let content = child.measure(scroll_constraints(viewport, axis));
    let width = content.width.max(viewport.width);
    let height = content.height.max(viewport.height);
    let offset =
        crate::interaction::clamp_scroll(width, height, viewport.width, viewport.height, requested);
    Rect::new(
        clamp_i64_to_i32(i64::from(viewport.x) - i64::from(offset.x)),
        clamp_i64_to_i32(i64::from(viewport.y) - i64::from(offset.y)),
        width,
        height,
    )
}

fn virtual_fragment<'a, Message>(
    declared_content_size: Size,
    axis: ScrollAxis,
    builder: &dyn Fn(VirtualViewport) -> VirtualFragment<Message>,
    cache: &'a RefCell<Option<VirtualCache<Message>>>,
    viewport: Rect,
    requested: ScrollOffset,
) -> Option<Ref<'a, VirtualCache<Message>>> {
    if viewport.is_empty() || virtual_content_is_empty(declared_content_size, axis) {
        cache.borrow_mut().take();
        return None;
    }
    let content_size = resolved_virtual_content_size(declared_content_size, viewport.size(), axis);
    let requested = crate::interaction::normalize_scroll_offset(axis, requested);
    let offset = crate::interaction::clamp_scroll(
        content_size.width,
        content_size.height,
        viewport.width,
        viewport.height,
        requested,
    );
    let request = VirtualViewport {
        offset,
        size: Size::new(
            viewport
                .width
                .min(content_size.width.saturating_sub(offset.x)),
            viewport
                .height
                .min(content_size.height.saturating_sub(offset.y)),
        ),
        content_size,
    };
    let rebuild = cache
        .borrow()
        .as_ref()
        .is_none_or(|cached| cached.request != request);
    if rebuild {
        *cache.borrow_mut() = Some(VirtualCache {
            request,
            fragment: builder(request),
        });
    }
    Some(Ref::map(cache.borrow(), |cached| {
        cached
            .as_ref()
            .expect("virtual fragment was cached for a non-empty viewport")
    }))
}

fn resolved_virtual_content_size(declared: Size, viewport: Size, _axis: ScrollAxis) -> Size {
    Size::new(
        declared.width.max(viewport.width),
        declared.height.max(viewport.height),
    )
}

fn virtual_content_is_empty(content: Size, axis: ScrollAxis) -> bool {
    match axis {
        ScrollAxis::Both => content.is_empty(),
        ScrollAxis::Vertical => content.height == 0,
        ScrollAxis::Horizontal => content.width == 0,
    }
}

fn virtual_fragment_rect<Message>(viewport: Rect, cached: &VirtualCache<Message>) -> Rect {
    let request = cached.request;
    let origin = ScrollOffset::new(
        cached.fragment.origin.x.min(request.content_size.width),
        cached.fragment.origin.y.min(request.content_size.height),
    );
    let remaining = Size::new(
        request.content_size.width.saturating_sub(origin.x),
        request.content_size.height.saturating_sub(origin.y),
    );
    let measured = cached
        .fragment
        .node
        .measure(Constraints::bounded(remaining));
    let visible_end = ScrollOffset::new(
        request.offset.x.saturating_add(request.size.width),
        request.offset.y.saturating_add(request.size.height),
    );
    let coverage = Size::new(
        visible_end.x.saturating_sub(origin.x),
        visible_end.y.saturating_sub(origin.y),
    );
    Rect::new(
        clamp_i64_to_i32(i64::from(viewport.x) + i64::from(origin.x) - i64::from(request.offset.x)),
        clamp_i64_to_i32(i64::from(viewport.y) + i64::from(origin.y) - i64::from(request.offset.y)),
        measured.width.max(coverage.width).min(remaining.width),
        measured.height.max(coverage.height).min(remaining.height),
    )
}

fn scroll_constraints(viewport: Rect, axis: ScrollAxis) -> Constraints {
    Constraints {
        width: if axis.allows_horizontal() {
            Limit::Unbounded
        } else {
            Limit::Bounded(viewport.width)
        },
        height: if axis.allows_vertical() {
            Limit::Unbounded
        } else {
            Limit::Bounded(viewport.height)
        },
    }
}

fn render_border(surface: &mut Surface, rect: Rect, clip: Rect, style: Style) {
    render_border_with_glyphs(
        surface,
        rect,
        clip,
        style,
        &border_glyphs(BorderKind::Single),
    );
}

fn render_border_with_glyphs(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    style: Style,
    glyphs: &BorderGlyphs,
) {
    if rect.is_empty() {
        return;
    }
    let right = i64::from(rect.x) + i64::from(rect.width) - 1;
    let bottom = i64::from(rect.y) + i64::from(rect.height) - 1;
    for x in i64::from(rect.x)..=right {
        write_border_cell(
            surface,
            clip,
            x,
            i64::from(rect.y),
            glyphs.horizontal,
            style,
        );
        if bottom != i64::from(rect.y) {
            write_border_cell(surface, clip, x, bottom, glyphs.horizontal, style);
        }
    }
    for y in i64::from(rect.y)..=bottom {
        write_border_cell(surface, clip, i64::from(rect.x), y, glyphs.vertical, style);
        if right != i64::from(rect.x) {
            write_border_cell(surface, clip, right, y, glyphs.vertical, style);
        }
    }
    write_border_cell(
        surface,
        clip,
        i64::from(rect.x),
        i64::from(rect.y),
        glyphs.top_left,
        style,
    );
    if right != i64::from(rect.x) {
        write_border_cell(
            surface,
            clip,
            right,
            i64::from(rect.y),
            glyphs.top_right,
            style,
        );
    }
    if bottom != i64::from(rect.y) {
        write_border_cell(
            surface,
            clip,
            i64::from(rect.x),
            bottom,
            glyphs.bottom_left,
            style,
        );
        if right != i64::from(rect.x) {
            write_border_cell(surface, clip, right, bottom, glyphs.bottom_right, style);
        }
    }
}

fn render_panel<Message>(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    child: &Node<Message>,
    title: &str,
    options: PanelOptions,
    interaction: &InteractionState,
) {
    if rect.is_empty() {
        return;
    }
    let background = rect.intersection(clip);
    surface.fill(
        background.x,
        background.y,
        background.width,
        background.height,
        options.style.background,
    );
    render_border_with_glyphs(
        surface,
        rect,
        clip,
        options.style.border,
        &border_glyphs(options.border),
    );
    render_panel_title(surface, rect, clip, title, options.style.title);
    let insets = panel_content_insets(options);
    child.render(
        surface,
        inset(rect, insets.left, insets.top, insets.right, insets.bottom),
        clip,
        interaction,
    );
}

fn render_panel_title(surface: &mut Surface, rect: Rect, clip: Rect, title: &str, style: Style) {
    if title.is_empty() || rect.width < 4 {
        return;
    }
    let title = truncate(title, (rect.width - 4) as usize, WidthProfile::MODERN);
    let content = format!(" {title} ");
    render_single_line(
        surface,
        Rect::new(add_coordinate(rect.x, 1), rect.y, rect.width - 2, 1),
        clip,
        &content,
        style,
    );
}

fn write_border_cell(surface: &mut Surface, clip: Rect, x: i64, y: i64, text: &str, style: Style) {
    if contains_unit(clip, x, y, x + 1) {
        surface.write(
            clamp_i64_to_i32(x),
            clamp_i64_to_i32(y),
            text,
            style,
            WidthProfile::MODERN,
        );
    }
}

fn contains_unit(clip: Rect, start_x: i64, y: i64, end_x: i64) -> bool {
    start_x >= i64::from(clip.x)
        && end_x <= i64::from(clip.x) + i64::from(clip.width)
        && y >= i64::from(clip.y)
        && y < i64::from(clip.y) + i64::from(clip.height)
}

fn alignment_offset(available: u32, desired: u32, alignment: HorizontalAlignment) -> u32 {
    let remaining = available.saturating_sub(desired);
    match alignment {
        HorizontalAlignment::Start => 0,
        HorizontalAlignment::Center => remaining / 2,
        HorizontalAlignment::End => remaining,
    }
}

fn vertical_alignment_offset(available: u32, desired: u32, alignment: VerticalAlignment) -> u32 {
    let remaining = available.saturating_sub(desired);
    match alignment {
        VerticalAlignment::Start => 0,
        VerticalAlignment::Center => remaining / 2,
        VerticalAlignment::End => remaining,
    }
}

fn aligned_child_rect<Message>(
    rect: Rect,
    child: &Node<Message>,
    horizontal: HorizontalAlignment,
    vertical: VerticalAlignment,
) -> Rect {
    let desired = child.measure(Constraints::bounded(rect.size()));
    let width = desired.width.min(rect.width);
    let height = desired.height.min(rect.height);
    Rect::new(
        add_coordinate(rect.x, alignment_offset(rect.width, width, horizontal)),
        add_coordinate(
            rect.y,
            vertical_alignment_offset(rect.height, height, vertical),
        ),
        width,
        height,
    )
}

fn add_coordinate(origin: i32, offset: u32) -> i32 {
    clamp_i64_to_i32(i64::from(origin) + i64::from(offset))
}

fn clamp_i64_to_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use nagi_surface::Surface;
    use nagi_vt::Color;

    use super::*;

    enum Message {}

    #[test]
    fn virtual_scroll_requests_match_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "interaction/virtual-scroll.txt",
            "virtual-scroll-request",
            &[
                "axis",
                "content-width",
                "content-height",
                "viewport-width",
                "viewport-height",
                "request-x",
                "request-y",
                "expected-build",
                "expected-x",
                "expected-y",
                "expected-width",
                "expected-height",
                "expected-content-width",
                "expected-content-height",
            ],
        ) else {
            return;
        };

        for record in records {
            let axis = match record.field("axis") {
                "both" => ScrollAxis::Both,
                "vertical" => ScrollAxis::Vertical,
                "horizontal" => ScrollAxis::Horizontal,
                value => panic!("case {} has invalid axis {value}", record.id),
            };
            let declared = Size::new(
                fixture_number(record.field("content-width")),
                fixture_number(record.field("content-height")),
            );
            let viewport = Rect::new(
                0,
                0,
                fixture_number(record.field("viewport-width")),
                fixture_number(record.field("viewport-height")),
            );
            let requested = ScrollOffset::new(
                fixture_number(record.field("request-x")),
                fixture_number(record.field("request-y")),
            );
            let expected_build = fixture_number(record.field("expected-build")) == 1;
            let expected = VirtualViewport {
                offset: ScrollOffset::new(
                    fixture_number(record.field("expected-x")),
                    fixture_number(record.field("expected-y")),
                ),
                size: Size::new(
                    fixture_number(record.field("expected-width")),
                    fixture_number(record.field("expected-height")),
                ),
                content_size: Size::new(
                    fixture_number(record.field("expected-content-width")),
                    fixture_number(record.field("expected-content-height")),
                ),
            };
            let builds = Cell::new(0);
            let cache = RefCell::new(None);
            let builder = |request: VirtualViewport| {
                builds.set(builds.get() + 1);
                VirtualFragment::new(request.offset, Node::<Message>::column(std::iter::empty()))
            };

            let first = virtual_fragment(declared, axis, &builder, &cache, viewport, requested);
            assert_eq!(first.is_some(), expected_build, "case {}", record.id);
            if let Some(first) = first {
                assert_eq!(first.request, expected, "case {}", record.id);
            }
            let second = virtual_fragment(declared, axis, &builder, &cache, viewport, requested);
            assert_eq!(second.is_some(), expected_build, "case {}", record.id);
            if let Some(second) = second {
                assert_eq!(second.request, expected, "case {}", record.id);
            }
            assert_eq!(
                builds.get(),
                usize::from(expected_build),
                "case {} cache",
                record.id
            );
        }
    }

    #[test]
    fn primitive_tree_renders_graphemes_and_layout() {
        let node = Node::<Message>::border(
            Node::column([
                Node::text("A日").with_length(Length::Fixed(1)),
                Node::align(
                    Node::text("B"),
                    HorizontalAlignment::End,
                    VerticalAlignment::End,
                )
                .with_length(Length::Flex(1)),
            ]),
            Style::default(),
        );
        let mut surface = Surface::new(5, 4).unwrap();

        node.render_to(&mut surface, &InteractionState::new());

        assert_eq!(surface.cell(1, 1).unwrap().content(), "A");
        assert_eq!(surface.cell(2, 1).unwrap().content(), "日");
        assert_eq!(surface.cell(3, 2).unwrap().content(), "B");
        assert_eq!(surface.cell(0, 0).unwrap().content(), "┌");
        assert_eq!(surface.cell(4, 3).unwrap().content(), "┘");
    }

    #[test]
    fn clip_prevents_child_drawing_outside_its_rect() {
        let node = Node::<Message>::clip(Node::text("ABCDE"));
        let mut surface = Surface::new(3, 1).unwrap();

        node.render_to(&mut surface, &InteractionState::new());

        assert_eq!(surface.cell(2, 0).unwrap().content(), "C");
    }

    #[test]
    fn focused_style_overlay_preserves_text_and_base_style() {
        let node = Node::<Message>::styled_text(
            "A日",
            Style {
                reverse: true,
                ..Style::default()
            },
        )
        .focusable("item")
        .with_focused_style(Style {
            underline: true,
            ..Style::default()
        });
        let mut surface = Surface::new(3, 1).unwrap();
        let mut interaction = InteractionState::new();
        interaction.focused = Some(NodeId::from("item"));

        node.render_to(&mut surface, &interaction);

        assert_eq!(surface.cell(0, 0).unwrap().content(), "A");
        assert_eq!(surface.cell(1, 0).unwrap().content(), "日");
        for x in 0..3 {
            let style = surface.cell(x, 0).unwrap().style();
            assert!(style.reverse && style.underline);
        }
    }

    #[test]
    fn rich_text_preserves_span_styles_across_word_wrapping() {
        let first = Style {
            bold: true,
            ..Style::default()
        };
        let second = Style {
            italic: true,
            ..Style::default()
        };
        let node = Node::<Message>::paragraph(
            [
                TextSpan::new("Hel", first),
                TextSpan::new("lo world", second),
            ],
            ParagraphOptions::default(),
        );
        let mut surface = Surface::new(7, 2).unwrap();

        node.render_to(&mut surface, &InteractionState::new());

        assert_eq!(surface.cell(0, 0).unwrap().content(), "H");
        assert_eq!(surface.cell(4, 0).unwrap().content(), "o");
        assert_eq!(surface.cell(0, 1).unwrap().content(), "w");
        assert_eq!(surface.cell(0, 0).unwrap().style(), first);
        assert_eq!(surface.cell(3, 0).unwrap().style(), second);
        assert_eq!(surface.cell(0, 1).unwrap().style(), second);
    }

    #[test]
    fn paragraph_alignment_and_no_wrap_respond_to_bounds() {
        let centered = Node::<Message>::paragraph(
            [TextSpan::new("A日", Style::default())],
            ParagraphOptions {
                wrap: WrapMode::Hard,
                alignment: HorizontalAlignment::Center,
            },
        );
        let mut surface = Surface::new(5, 1).unwrap();
        centered.render_to(&mut surface, &InteractionState::new());

        assert_eq!(surface.cell(1, 0).unwrap().content(), "A");
        assert_eq!(surface.cell(2, 0).unwrap().content(), "日");

        let unwrapped = Node::<Message>::paragraph(
            [TextSpan::new("ABCDE", Style::default())],
            ParagraphOptions {
                wrap: WrapMode::None,
                ..ParagraphOptions::default()
            },
        );
        let mut clipped = Surface::new(3, 2).unwrap();
        unwrapped.render_to(&mut clipped, &InteractionState::new());
        assert_eq!(clipped.cell(2, 0).unwrap().content(), "C");
        assert_eq!(clipped.cell(0, 1).unwrap().content(), " ");
    }

    #[test]
    fn surface_node_safely_composites_typed_cells() {
        let mut source = Surface::transparent(3, 1).unwrap();
        source.write(
            0,
            0,
            "日A",
            Style {
                bold: true,
                ..Style::default()
            },
            WidthProfile::MODERN,
        );
        source.fill_transparent(
            2,
            0,
            1,
            1,
            Style {
                underline: true,
                ..Style::default()
            },
        );
        assert!(source.set_cursor(Some(Cursor::new(2, 0))));
        let node = Node::<Message>::stack([Node::text("xyz"), Node::surface(source.clone())]);
        source.clear();
        let mut target = Surface::new(3, 1).unwrap();

        node.render_to(&mut target, &InteractionState::new());

        assert_eq!(target.cell(0, 0).unwrap().content(), "日");
        assert_eq!(target.cell(2, 0).unwrap().content(), "z");
        assert!(target.cell(0, 0).unwrap().style().bold);
        assert!(target.cell(2, 0).unwrap().style().underline);
        assert_eq!(target.cursor(), Some(Cursor::new(2, 0)));
    }

    #[test]
    fn panel_renders_title_border_padding_and_background() {
        let options = PanelOptions {
            border: BorderKind::Rounded,
            style: crate::PanelStyle {
                background: Style {
                    background: Color::Indexed(4),
                    ..Style::default()
                },
                ..crate::PanelStyle::default()
            },
            ..PanelOptions::default()
        };
        let node = Node::<Message>::panel_with_options(Node::text("X"), "Title", options);
        let mut surface = Surface::new(10, 5).unwrap();

        node.render_to(&mut surface, &InteractionState::new());

        assert_eq!(surface.cell(0, 0).unwrap().content(), "╭");
        assert_eq!(surface.cell(1, 0).unwrap().content(), " ");
        assert_eq!(surface.cell(2, 0).unwrap().content(), "T");
        assert_eq!(surface.cell(9, 4).unwrap().content(), "╯");
        assert_eq!(surface.cell(2, 2).unwrap().content(), "X");
        assert_eq!(
            surface.cell(5, 2).unwrap().style().background,
            Color::Indexed(4)
        );
    }

    #[test]
    fn gap_and_spacer_reserve_deterministic_layout_space() {
        let node = Node::<Message>::column([
            Node::row([Node::text("A"), Node::gap(2), Node::text("B")]),
            Node::spacer(1, 2),
            Node::text("C"),
        ]);
        let mut surface = Surface::new(4, 4).unwrap();

        node.render_to(&mut surface, &InteractionState::new());

        assert_eq!(surface.cell(0, 0).unwrap().content(), "A");
        assert_eq!(surface.cell(3, 0).unwrap().content(), "B");
        assert_eq!(surface.cell(0, 3).unwrap().content(), "C");
    }

    fn fixture_number(value: &str) -> u32 {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid fixture number {value}: {error}"))
    }
}
