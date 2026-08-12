use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::marker::PhantomData;

use nagi_surface::{Cursor, Surface};
use nagi_text::{
    WidthProfile, byte_at_cell, cell_at_byte, grapheme_width, graphemes, text_width, truncate,
    wrapped_lines,
};
use nagi_vt::Style;

use crate::action_routing::{ActionIndex, NodeKeyInteraction};
use crate::layout::{Track, add_size, allocate_into, horizontal_rect, inset, vertical_rect};
use crate::panel::{BorderGlyphs, content_insets as panel_content_insets, glyphs as border_glyphs};
use crate::responsive_row::{
    INLINE_RESPONSIVE_ROW_ITEMS, ResolvedResponsiveRowLayout, ResponsiveRowItem,
    ResponsiveRowMetric, ResponsiveRowOptions, resolve_responsive_row_layout,
};
use crate::rich_text::ParagraphLayoutCache;
use crate::routing::{EventHandler, InteractiveKind, NodeRecord, PointerEventHandler, TreeIndex};
use crate::split_pane::resolve_split_pane_layout;
use crate::{
    Action, BorderKind, Event, EventResult, InteractionState, KeyScope, Length, NodeId,
    PanelOptions, ParagraphOptions, PointerEventContext, Rect, ScrollAxis, ScrollOffset,
    ScrollState, Size, SplitPaneCollapse, SplitPaneOptions, TextSpan, VirtualFlowOptions,
    VirtualFlowSource, WrapMode,
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

/// Preferred vertical side of an anchored overlay
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum AnchoredOverlaySide {
    /// Place the overlay after the anchor row
    #[default]
    Below,
    /// Place the overlay before the anchor row
    Above,
}

/// Fallback used when an anchored overlay does not fit on its preferred side
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum AnchoredOverlayFallback {
    /// Use the opposite side when it has more available rows
    #[default]
    Flip,
    /// Keep the preferred side and clip to its available rows
    Clip,
}

/// Placement and size limits for an anchored overlay
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct AnchoredOverlayOptions {
    /// Preferred vertical side of the anchor
    pub side: AnchoredOverlaySide,
    /// Horizontal alignment relative to the anchor
    pub alignment: HorizontalAlignment,
    /// Empty rows inserted between the anchor and overlay
    pub gap: u32,
    /// Fallback when the preferred side cannot contain the natural height
    pub fallback: AnchoredOverlayFallback,
    /// Greatest overlay width, or zero for the available boundary width
    pub maximum_width: u32,
    /// Greatest overlay height, or zero for the available boundary height
    pub maximum_height: u32,
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

/// Focus selection applied when a modal becomes active
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ModalInitialFocus {
    /// Focus the first focusable node in the modal scope
    #[default]
    First,
    /// Focus a specific stable node, falling back to the first focusable node
    Target(NodeId),
    /// Leave the modal scope unfocused
    None,
}

/// Focus selection applied when a modal stops being active
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ModalReturnFocus {
    /// Return to the node focused immediately before modal entry
    #[default]
    Previous,
    /// Focus a specific stable node after the modal closes
    Target(NodeId),
    /// Leave the resumed scope unfocused
    None,
}

/// Focus lifecycle policies attached to one modal scope
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModalFocusOptions {
    /// Policy used when the modal becomes active
    pub initial: ModalInitialFocus,
    /// Policy used when the modal stops being active
    pub return_focus: ModalReturnFocus,
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
    blocks_unhandled_events: bool,
    focusable: bool,
    focused_style: Option<Style>,
    handler: Option<Box<EventHandler<Message>>>,
    pointer_handler: Option<Box<PointerEventHandler<Message>>>,
    key_interaction: Option<Box<NodeKeyInteraction<Message>>>,
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
        cache: RefCell<ParagraphLayoutCache>,
    },
    Surface(Surface),
    Spacer(Size),
    Gap(u32),
    CursorAnchor {
        focus_owner: NodeId,
    },
    TextInput {
        value: String,
        placeholder: String,
        style: Style,
        placeholder_style: Style,
        on_change: Box<dyn Fn(String) -> Message>,
    },
    Row(LinearNode<Message>),
    Column(LinearNode<Message>),
    ResponsiveRow(ResponsiveRowNode<Message>),
    SplitPane {
        primary: Box<Node<Message>>,
        secondary: Box<Node<Message>>,
        options: SplitPaneOptions,
    },
    Stack(Vec<Node<Message>>),
    Overlay {
        base: Box<Node<Message>>,
        layer: Box<Node<Message>>,
    },
    AnchoredOverlay {
        base: Box<Node<Message>>,
        anchor: NodeId,
        overlay: Box<Node<Message>>,
        options: AnchoredOverlayOptions,
        cache: RefCell<Option<AnchoredOverlayFrame>>,
    },
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
    VirtualFlow(Box<VirtualFlowNode<Message>>),
    Modal {
        child: Box<Node<Message>>,
        focus: ModalFocusOptions,
    },
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

struct VirtualFlowNode<Message> {
    source: VirtualFlowSource<Message>,
    options: VirtualFlowOptions<Message>,
    cache: RefCell<Option<VirtualFlowFrame<Message>>>,
}

struct VirtualFlowFrame<Message> {
    rect: Rect,
    offset: u32,
    content_height: u32,
    generation: u64,
    items: Vec<VirtualFlowBuiltItem<Message>>,
}

struct VirtualFlowBuiltItem<Message> {
    index: usize,
    origin: u32,
    height: u32,
    node: Node<Message>,
}

#[derive(Clone, Copy)]
struct AnchoredOverlayFrame {
    rect: Rect,
    clip: Rect,
    overlay: Option<Rect>,
}

struct LinearNode<Message> {
    children: Vec<Node<Message>>,
    cache: RefCell<Option<Box<CachedLinearLayout>>>,
}

struct ResponsiveRowNode<Message> {
    items: Vec<ResponsiveRowItem<Message>>,
    options: ResponsiveRowOptions,
    cache: RefCell<Option<Box<CachedResponsiveRowLayout>>>,
}

struct CachedResponsiveRowLayout {
    rect: Rect,
    layout: ResolvedResponsiveRowLayout,
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
            cache: RefCell::new(ParagraphLayoutCache::default()),
        })
    }

    /// Creates inline styled text using supplied wrapping and alignment
    #[must_use]
    pub fn paragraph(spans: impl IntoIterator<Item = TextSpan>, options: ParagraphOptions) -> Self {
        Self::new(NodeKind::RichText {
            spans: spans.into_iter().collect(),
            options,
            cache: RefCell::new(ParagraphLayoutCache::default()),
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

    /// Creates a zero-width cursor position shown while `focus_owner` is
    /// focused
    ///
    /// The anchor measures one row high, consumes no horizontal layout space,
    /// and sets the output Surface cursor only while its position is visible
    #[must_use]
    pub fn cursor_anchor(focus_owner: impl Into<NodeId>) -> Self {
        Self::new(NodeKind::CursorAnchor {
            focus_owner: focus_owner.into(),
        })
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

    /// Creates a priority-aware three-region horizontal container
    ///
    /// Supplied item Nodes are eager. Items that do not fit the assigned width
    /// are omitted from preparation, semantic indexing, hit testing, routing,
    /// and rendering. Higher priorities are retained first and source order
    /// breaks equal-priority ties
    #[must_use]
    pub fn responsive_row(
        items: impl IntoIterator<Item = ResponsiveRowItem<Message>>,
        options: ResponsiveRowOptions,
    ) -> Self {
        Self::new(NodeKind::ResponsiveRow(ResponsiveRowNode {
            items: items.into_iter().collect(),
            options,
            cache: RefCell::new(None),
        }))
    }

    /// Creates a responsive two-pane container with a one-Cell divider
    ///
    /// Both supplied Nodes are eager, but only panes present in the resolved
    /// layout participate in preparation, semantic indexing, hit testing, and
    /// rendering. The configured collapse pane is omitted when the assigned
    /// main-axis extent cannot satisfy both normalized minima plus the divider
    #[must_use]
    pub fn split_pane(primary: Self, secondary: Self, options: SplitPaneOptions) -> Self {
        Self::new(NodeKind::SplitPane {
            primary: Box::new(primary),
            secondary: Box::new(secondary),
            options,
        })
    }

    /// Creates a front-to-back overlay container
    #[must_use]
    pub fn stack(children: impl IntoIterator<Item = Self>) -> Self {
        Self::new(NodeKind::Stack(children.into_iter().collect()))
    }

    /// Places a front layer over a base without adding the layer to measurement
    ///
    /// Both children receive the complete assigned rectangle. The base is
    /// prepared, indexed, and rendered first, so the layer is topmost for
    /// overlapping pointer hits
    #[must_use]
    pub fn overlay(base: Self, layer: Self) -> Self {
        Self::new(NodeKind::Overlay {
            base: Box::new(base),
            layer: Box::new(layer),
        })
    }

    /// Places a front layer relative to an identified descendant of `base`
    ///
    /// The default prefers below-start placement, flips above when that side
    /// has more room, and constrains the layer to this node's visible boundary
    #[must_use]
    pub fn anchored_overlay(base: Self, anchor: impl Into<NodeId>, overlay: Self) -> Self {
        Self::anchored_overlay_with_options(
            base,
            anchor,
            overlay,
            AnchoredOverlayOptions::default(),
        )
    }

    /// Places a configured front layer relative to an identified descendant
    ///
    /// The overlay does not affect measurement. It is omitted from rendering,
    /// hit testing, and routing while the anchor is absent or not visible
    #[must_use]
    pub fn anchored_overlay_with_options(
        base: Self,
        anchor: impl Into<NodeId>,
        overlay: Self,
        options: AnchoredOverlayOptions,
    ) -> Self {
        Self::new(NodeKind::AnchoredOverlay {
            base: Box::new(base),
            anchor: anchor.into(),
            overlay: Box::new(overlay),
            options,
            cache: RefCell::new(None),
        })
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

    /// Creates a vertical viewport for stable variable-height items
    ///
    /// Only items intersecting the visible range and bounded Cell overscan are
    /// built. Item heights are measured from their Nodes and retained by the
    /// runtime across semantic frames. The viewport has zero intrinsic height;
    /// assign it a layout length or place it where the parent supplies a
    /// rectangle
    #[must_use]
    pub fn virtual_flow(id: impl Into<NodeId>, source: VirtualFlowSource<Message>) -> Self {
        Self::virtual_flow_with_options(id, source, VirtualFlowOptions::default())
    }

    /// Creates a variable-height flow with configured vertical scrolling
    ///
    /// The viewport has zero intrinsic height; assign it a layout length or
    /// place it where the parent supplies a rectangle
    #[must_use]
    pub fn virtual_flow_with_options(
        id: impl Into<NodeId>,
        source: VirtualFlowSource<Message>,
        options: VirtualFlowOptions<Message>,
    ) -> Self {
        let mut node = Self::new(NodeKind::VirtualFlow(Box::new(VirtualFlowNode {
            source,
            options,
            cache: RefCell::new(None),
        })));
        node.id = Some(id.into());
        node.focusable = true;
        node
    }

    /// Marks a subtree as the active modal routing and focus scope
    ///
    /// The default focus lifecycle selects the first focusable descendant on
    /// entry and returns to the previously focused node on close
    #[must_use]
    pub fn modal(id: impl Into<NodeId>, child: Self) -> Self {
        Self::modal_with_focus(id, child, ModalFocusOptions::default())
    }

    /// Creates a modal scope with explicit entry and return focus policies
    #[must_use]
    pub fn modal_with_focus(id: impl Into<NodeId>, child: Self, focus: ModalFocusOptions) -> Self {
        let mut node = Self::new(NodeKind::Modal {
            child: Box::new(child),
            focus,
        });
        node.id = Some(id.into());
        node
    }

    /// Attaches a stable semantic identity without changing focus behavior
    #[must_use]
    pub fn with_id(mut self, id: impl Into<NodeId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Consumes routed events that remain unhandled after this identified node
    /// has processed its actions, built-in behavior, pointer handler, and raw
    /// handler
    ///
    /// The boundary prevents the event from reaching ancestors and
    /// terminal-level fallback mapping. It has no effect until the node has a
    /// stable identity
    #[must_use]
    pub const fn block_unhandled_events(mut self) -> Self {
        self.blocks_unhandled_events = true;
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

    /// Attaches a geometry-aware mouse event handler under a stable identity
    ///
    /// The handler receives Node-local coordinates, clipping, Runtime width
    /// policy, paragraph text hit information, and the nearest ScrollViewport.
    /// Raw [`Node::on_event`] handling remains independent and runs afterward
    /// when this handler does not consume the event
    #[must_use]
    pub fn on_pointer_event(
        mut self,
        id: impl Into<NodeId>,
        handler: impl Fn(&PointerEventContext) -> EventResult<Message> + 'static,
    ) -> Self {
        self.id = Some(id.into());
        self.pointer_handler = Some(Box::new(handler));
        self
    }

    /// Attaches a complete semantic action group under a stable owner identity
    ///
    /// A later call replaces the complete group
    #[must_use]
    pub fn on_actions(
        mut self,
        id: impl Into<NodeId>,
        actions: impl IntoIterator<Item = Action<Message>>,
    ) -> Self {
        self.id = Some(id.into());
        self.key_interaction_mut().set_actions(actions);
        self
    }

    /// Attaches one immutable KeyMap scope to this semantic node
    ///
    /// The scope ID becomes the node identity. Later identity modifiers may
    /// replace it without retaining a stale scope identity. A later call
    /// replaces the complete scope
    #[must_use]
    pub fn with_key_scope(mut self, scope: KeyScope) -> Self {
        self.id = Some(scope.id().clone());
        self.key_interaction_mut()
            .set_scope(scope.key_map().clone(), scope.propagation());
        self
    }

    /// Keeps an identified descendant visible inside this ScrollViewport
    ///
    /// The target must be present below an eager ScrollViewport or in the
    /// current fragment of a virtual ScrollViewport. A later call replaces the
    /// target. On other node kinds this metadata has no effect
    #[must_use]
    pub fn reveal_descendant(mut self, target: impl Into<NodeId>) -> Self {
        if matches!(
            &self.kind,
            NodeKind::ScrollViewport { .. }
                | NodeKind::VirtualScrollViewport(_)
                | NodeKind::VirtualFlow(_)
        ) {
            self.key_interaction_mut().set_reveal_target(target.into());
        }
        self
    }

    /// Prefers a stable focus target when a focused node in this subtree disappears
    ///
    /// The target must remain focusable in the next frame and belong to the
    /// active modal scope. Nested declarations override outer declarations.
    /// When the target is unavailable, normal deterministic reconciliation is
    /// used. This metadata has no effect while the current focus remains valid
    #[must_use]
    pub fn focus_fallback(mut self, target: impl Into<NodeId>) -> Self {
        self.key_interaction_mut().set_focus_fallback(target.into());
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
            blocks_unhandled_events: false,
            focusable: false,
            focused_style: None,
            handler: None,
            pointer_handler: None,
            key_interaction: None,
            message: PhantomData,
        }
    }

    fn key_interaction_mut(&mut self) -> &mut NodeKeyInteraction<Message> {
        self.key_interaction
            .get_or_insert_with(|| Box::new(NodeKeyInteraction::new()))
    }

    #[cfg(test)]
    pub(crate) fn render_to(&self, surface: &mut Surface, interaction: &InteractionState) {
        self.render_to_profile(surface, interaction, WidthProfile::MODERN);
    }

    pub(crate) fn render_to_profile(
        &self,
        surface: &mut Surface,
        interaction: &InteractionState,
        profile: WidthProfile<'static>,
    ) {
        let bounds = Rect::new(0, 0, surface.width(), surface.height());
        self.render(surface, bounds, bounds, interaction, profile);
    }

    fn measure(&self, constraints: Constraints, profile: WidthProfile<'static>) -> Size {
        let measured = match &self.kind {
            NodeKind::Text { content, .. } => measure_text(content, constraints, profile),
            NodeKind::RichText {
                spans,
                options,
                cache,
            } => measure_rich_text(spans, *options, cache, constraints, profile),
            NodeKind::Surface(surface) => Size::new(surface.width(), surface.height()),
            NodeKind::Spacer(size) => *size,
            NodeKind::Gap(_) => Size::default(),
            NodeKind::CursorAnchor { .. } => Size::new(0, 1),
            NodeKind::TextInput {
                value, placeholder, ..
            } => {
                let width = text_width(value, profile)
                    .max(text_width(placeholder, profile))
                    .min(u32::MAX as usize) as u32;
                Size::new(width, 1)
            }
            NodeKind::Row(linear) => measure_linear(&linear.children, constraints, true, profile),
            NodeKind::Column(linear) => {
                measure_linear(&linear.children, constraints, false, profile)
            }
            NodeKind::ResponsiveRow(responsive) => {
                measure_responsive_row(responsive, constraints, profile)
            }
            NodeKind::SplitPane {
                primary,
                secondary,
                options,
            } => measure_split_pane(primary, secondary, constraints, *options, profile),
            NodeKind::Stack(children) => children.iter().fold(Size::default(), |size, child| {
                let child = child.measure(constraints, profile);
                Size::new(size.width.max(child.width), size.height.max(child.height))
            }),
            NodeKind::Overlay { base, .. } => base.measure(constraints, profile),
            NodeKind::AnchoredOverlay { base, .. } => base.measure(constraints, profile),
            NodeKind::Padding { insets, child } => add_size(
                child.measure(shrink_constraints(constraints, *insets), profile),
                insets.left.saturating_add(insets.right),
                insets.top.saturating_add(insets.bottom),
            ),
            NodeKind::Border { child, .. } => add_size(
                child.measure(shrink_constraints(constraints, Insets::all(1)), profile),
                2,
                2,
            ),
            NodeKind::Panel { options, child, .. } => {
                let insets = panel_content_insets(*options);
                add_size(
                    child.measure(shrink_constraints(constraints, insets), profile),
                    insets.left.saturating_add(insets.right),
                    insets.top.saturating_add(insets.bottom),
                )
            }
            NodeKind::Align { child, .. }
            | NodeKind::Clip(child)
            | NodeKind::Modal { child, .. } => child.measure(constraints, profile),
            NodeKind::ScrollViewport { child, .. } => child.measure(constraints, profile),
            NodeKind::VirtualScrollViewport(virtual_node) => virtual_node.content_size,
            NodeKind::VirtualFlow(_) => virtual_flow_intrinsic_size(constraints),
        };
        clamp_size(measured, constraints)
    }

    fn render(
        &self,
        surface: &mut Surface,
        rect: Rect,
        clip: Rect,
        interaction: &InteractionState,
        profile: WidthProfile<'static>,
    ) {
        match &self.kind {
            NodeKind::Text { content, style } => {
                render_text(surface, rect, clip, content, *style, profile);
            }
            NodeKind::RichText {
                spans,
                options,
                cache,
            } => {
                render_rich_text(surface, rect, clip, spans, *options, cache, profile);
            }
            NodeKind::Surface(source) => render_surface_node(surface, rect, clip, source, profile),
            NodeKind::Spacer(_) | NodeKind::Gap(_) => {}
            NodeKind::CursorAnchor { focus_owner } => {
                render_cursor_anchor(surface, rect, clip, focus_owner, interaction);
            }
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
                profile,
            ),
            NodeKind::Row(linear) => {
                render_linear(surface, rect, clip, linear, true, interaction, profile)
            }
            NodeKind::Column(linear) => {
                render_linear(surface, rect, clip, linear, false, interaction, profile)
            }
            NodeKind::ResponsiveRow(responsive) => {
                let layout = responsive.layout(rect, profile);
                for (item, item_rect) in responsive.items.iter().zip(layout.as_slice()) {
                    if let Some(item_rect) = item_rect {
                        item.node
                            .render(surface, *item_rect, clip, interaction, profile);
                    }
                }
            }
            NodeKind::SplitPane {
                primary,
                secondary,
                options,
            } => render_split_pane(
                surface,
                rect,
                clip,
                primary,
                secondary,
                *options,
                interaction,
                profile,
            ),
            NodeKind::Stack(children) => {
                for child in children {
                    child.render(surface, rect, clip, interaction, profile);
                }
            }
            NodeKind::Overlay { base, layer } => {
                base.render(surface, rect, clip, interaction, profile);
                layer.render(surface, rect, clip, interaction, profile);
            }
            NodeKind::AnchoredOverlay {
                base,
                anchor,
                overlay,
                options,
                cache,
            } => {
                base.render(surface, rect, clip, interaction, profile);
                if let Some(overlay_rect) = anchored_overlay_rect(
                    base,
                    anchor,
                    overlay,
                    *options,
                    cache,
                    rect,
                    clip,
                    interaction,
                    profile,
                ) {
                    overlay.render(
                        surface,
                        overlay_rect,
                        clip.intersection(rect),
                        interaction,
                        profile,
                    );
                }
            }
            NodeKind::Padding { insets, child } => {
                let child_rect = inset(rect, insets.left, insets.top, insets.right, insets.bottom);
                child.render(surface, child_rect, clip, interaction, profile);
            }
            NodeKind::Border { style, child } => {
                render_border(surface, rect, clip, *style, profile);
                child.render(surface, inset(rect, 1, 1, 1, 1), clip, interaction, profile);
            }
            NodeKind::Align {
                horizontal,
                vertical,
                child,
            } => {
                child.render(
                    surface,
                    aligned_child_rect(rect, child, *horizontal, *vertical, profile),
                    clip,
                    interaction,
                    profile,
                );
            }
            NodeKind::Clip(child) => {
                child.render(surface, rect, clip.intersection(rect), interaction, profile)
            }
            NodeKind::ScrollViewport { child, options } => {
                let id = self
                    .id
                    .as_ref()
                    .expect("ScrollViewport always has a NodeId");
                let child_rect = scroll_child_rect(
                    rect,
                    child,
                    interaction.scroll_offset(id),
                    options.axis,
                    profile,
                );
                child.render(
                    surface,
                    child_rect,
                    clip.intersection(rect),
                    interaction,
                    profile,
                );
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
                    let fragment_rect = virtual_fragment_rect(rect, &fragment, profile);
                    fragment.fragment.node.render(
                        surface,
                        fragment_rect,
                        clip.intersection(rect),
                        interaction,
                        profile,
                    );
                }
            }
            NodeKind::VirtualFlow(flow) => {
                if let Some(frame) = flow.cache.borrow().as_ref() {
                    for item in &frame.items {
                        item.node.render(
                            surface,
                            virtual_flow_item_rect(rect, frame.offset, item.origin, item.height),
                            clip.intersection(rect),
                            interaction,
                            profile,
                        );
                    }
                }
            }
            NodeKind::Modal { child, .. } => {
                child.render(surface, rect, clip, interaction, profile)
            }
            NodeKind::Panel {
                title,
                options,
                child,
            } => render_panel(
                surface,
                RenderRegion { rect, clip },
                child,
                title,
                *options,
                interaction,
                profile,
            ),
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
        actions: &mut ActionIndex<Message>,
        profile: WidthProfile<'static>,
    ) -> Result<(), NodeId> {
        let bounds = Rect::new(0, 0, size.width, size.height);
        index.clear();
        actions.clear();
        self.build_index(
            bounds,
            bounds,
            None,
            true,
            None,
            interaction,
            index,
            actions,
            profile,
        )
    }

    pub(crate) fn prepare_interaction(
        &self,
        size: Size,
        interaction: &mut InteractionState,
        profile: WidthProfile<'static>,
    ) -> bool {
        let bounds = Rect::new(0, 0, size.width, size.height);
        self.prepare_at(bounds, interaction, profile)
    }

    pub(crate) fn prepare_virtual_flows(
        &self,
        size: Size,
        interaction: &mut InteractionState,
        profile: WidthProfile<'static>,
    ) {
        let bounds = Rect::new(0, 0, size.width, size.height);
        self.prepare_virtual_flows_at(bounds, interaction, profile);
    }

    pub(crate) fn handle_event(&self, id: &NodeId, event: &Event) -> Option<EventResult<Message>> {
        self.visit(id, &mut |node| {
            node.handler.as_ref().map(|handler| handler(event))
        })
    }

    pub(crate) fn handle_pointer_event(
        &self,
        id: &NodeId,
        context: PointerEventContext,
    ) -> Option<EventResult<Message>> {
        let mut context = Some(context);
        self.visit(id, &mut |node| {
            let handler = node.pointer_handler.as_ref()?;
            let mut context = context.take().expect("a NodeId is unique within one view");
            if let NodeKind::RichText {
                spans,
                options,
                cache,
            } = &node.kind
            {
                let hit = cache.borrow_mut().text_hit(
                    spans,
                    context.bounds().width,
                    *options,
                    context.width_profile(),
                    context.local_position(),
                );
                context.set_text_hit(Some(hit));
            }
            Some(handler(&context))
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
                NodeKind::VirtualFlow(flow) => {
                    return Some(ScrollBehavior {
                        axis: ScrollAxis::Vertical,
                        ensure_focused_visible: flow.options.ensure_focused_visible,
                    });
                }
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
                NodeKind::VirtualFlow(flow) => {
                    return flow.options.on_scroll.as_ref().map(|map| map(state));
                }
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
            NodeKind::ResponsiveRow(responsive) => responsive
                .items
                .iter()
                .find_map(|item| item.node.visit(id, operation)),
            NodeKind::SplitPane {
                primary, secondary, ..
            } => primary
                .visit(id, operation)
                .or_else(|| secondary.visit(id, operation)),
            NodeKind::Stack(children) => {
                children.iter().find_map(|child| child.visit(id, operation))
            }
            NodeKind::Overlay { base, layer } => base
                .visit(id, operation)
                .or_else(|| layer.visit(id, operation)),
            NodeKind::AnchoredOverlay { base, overlay, .. } => base
                .visit(id, operation)
                .or_else(|| overlay.visit(id, operation)),
            NodeKind::Padding { child, .. }
            | NodeKind::Border { child, .. }
            | NodeKind::Align { child, .. }
            | NodeKind::Clip(child)
            | NodeKind::Modal { child, .. }
            | NodeKind::Panel { child, .. }
            | NodeKind::ScrollViewport { child, .. } => child.visit(id, operation),
            NodeKind::VirtualScrollViewport(virtual_node) => virtual_node
                .cache
                .borrow()
                .as_ref()
                .and_then(|cached| cached.fragment.node.visit(id, operation)),
            NodeKind::VirtualFlow(flow) => flow.cache.borrow().as_ref().and_then(|frame| {
                frame
                    .items
                    .iter()
                    .find_map(|item| item.node.visit(id, operation))
            }),
            NodeKind::Text { .. }
            | NodeKind::RichText { .. }
            | NodeKind::Surface(_)
            | NodeKind::Spacer(_)
            | NodeKind::Gap(_)
            | NodeKind::CursorAnchor { .. }
            | NodeKind::TextInput { .. } => None,
        }
    }

    fn find_node_geometry(
        &self,
        id: &NodeId,
        rect: Rect,
        clip: Rect,
        interaction: &InteractionState,
        profile: WidthProfile<'static>,
    ) -> Option<(Rect, Rect)> {
        if self.id.as_ref() == Some(id) {
            return Some((rect, clip));
        }
        match &self.kind {
            NodeKind::Row(linear) => linear
                .children
                .iter()
                .zip(linear.layout(rect, true, profile).rects(rect))
                .find_map(|(child, child_rect)| {
                    child.find_node_geometry(id, child_rect, clip, interaction, profile)
                }),
            NodeKind::Column(linear) => linear
                .children
                .iter()
                .zip(linear.layout(rect, false, profile).rects(rect))
                .find_map(|(child, child_rect)| {
                    child.find_node_geometry(id, child_rect, clip, interaction, profile)
                }),
            NodeKind::ResponsiveRow(responsive) => {
                let layout = responsive.layout(rect, profile);
                responsive
                    .items
                    .iter()
                    .zip(layout.as_slice())
                    .find_map(|(item, item_rect)| {
                        item_rect.and_then(|item_rect| {
                            item.node
                                .find_node_geometry(id, item_rect, clip, interaction, profile)
                        })
                    })
            }
            NodeKind::SplitPane {
                primary,
                secondary,
                options,
            } => {
                let layout = resolve_split_pane_layout(rect, *options);
                let primary_result = (layout.collapsed != Some(SplitPaneCollapse::Primary))
                    .then(|| {
                        primary.find_node_geometry(id, layout.primary, clip, interaction, profile)
                    })
                    .flatten();
                primary_result.or_else(|| {
                    (layout.collapsed != Some(SplitPaneCollapse::Secondary))
                        .then(|| {
                            secondary.find_node_geometry(
                                id,
                                layout.secondary,
                                clip,
                                interaction,
                                profile,
                            )
                        })
                        .flatten()
                })
            }
            NodeKind::Stack(children) => children
                .iter()
                .find_map(|child| child.find_node_geometry(id, rect, clip, interaction, profile)),
            NodeKind::Overlay { base, layer } => base
                .find_node_geometry(id, rect, clip, interaction, profile)
                .or_else(|| layer.find_node_geometry(id, rect, clip, interaction, profile)),
            NodeKind::AnchoredOverlay {
                base,
                anchor,
                overlay,
                options,
                cache,
            } => base
                .find_node_geometry(id, rect, clip, interaction, profile)
                .or_else(|| {
                    let overlay_rect = anchored_overlay_rect(
                        base,
                        anchor,
                        overlay,
                        *options,
                        cache,
                        rect,
                        clip,
                        interaction,
                        profile,
                    )?;
                    overlay.find_node_geometry(
                        id,
                        overlay_rect,
                        clip.intersection(rect),
                        interaction,
                        profile,
                    )
                }),
            NodeKind::Padding { insets, child } => child.find_node_geometry(
                id,
                inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                clip,
                interaction,
                profile,
            ),
            NodeKind::Border { child, .. } => {
                child.find_node_geometry(id, inset(rect, 1, 1, 1, 1), clip, interaction, profile)
            }
            NodeKind::Align {
                horizontal,
                vertical,
                child,
            } => child.find_node_geometry(
                id,
                aligned_child_rect(rect, child, *horizontal, *vertical, profile),
                clip,
                interaction,
                profile,
            ),
            NodeKind::Clip(child) => {
                child.find_node_geometry(id, rect, clip.intersection(rect), interaction, profile)
            }
            NodeKind::ScrollViewport { child, options } => child.find_node_geometry(
                id,
                scroll_child_rect(
                    rect,
                    child,
                    interaction.scroll_offset(
                        self.id
                            .as_ref()
                            .expect("ScrollViewport always has a NodeId"),
                    ),
                    options.axis,
                    profile,
                ),
                clip.intersection(rect),
                interaction,
                profile,
            ),
            NodeKind::VirtualScrollViewport(virtual_node) => {
                let id_owner = self
                    .id
                    .as_ref()
                    .expect("VirtualScrollViewport always has a NodeId");
                let state = interaction.preview_scroll(
                    id_owner,
                    virtual_scroll_maximum(
                        virtual_node.content_size,
                        rect,
                        virtual_node.options.axis,
                    ),
                    virtual_node.options.axis,
                    virtual_node.options.stick_to_end,
                );
                virtual_fragment(
                    virtual_node.content_size,
                    virtual_node.options.axis,
                    virtual_node.builder.as_ref(),
                    &virtual_node.cache,
                    rect,
                    state.offset,
                )
                .and_then(|fragment| {
                    fragment.fragment.node.find_node_geometry(
                        id,
                        virtual_fragment_rect(rect, &fragment, profile),
                        clip.intersection(rect),
                        interaction,
                        profile,
                    )
                })
            }
            NodeKind::VirtualFlow(flow) => flow.cache.borrow().as_ref().and_then(|frame| {
                frame.items.iter().find_map(|item| {
                    item.node.find_node_geometry(
                        id,
                        virtual_flow_item_rect(rect, frame.offset, item.origin, item.height),
                        clip.intersection(rect),
                        interaction,
                        profile,
                    )
                })
            }),
            NodeKind::Modal { child, .. } => {
                child.find_node_geometry(id, rect, clip, interaction, profile)
            }
            NodeKind::Panel { options, child, .. } => {
                let insets = panel_content_insets(*options);
                child.find_node_geometry(
                    id,
                    inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                    clip,
                    interaction,
                    profile,
                )
            }
            NodeKind::Text { .. }
            | NodeKind::RichText { .. }
            | NodeKind::Surface(_)
            | NodeKind::Spacer(_)
            | NodeKind::Gap(_)
            | NodeKind::CursorAnchor { .. }
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
        inherited_focus_fallback: Option<&NodeId>,
        interaction: &InteractionState,
        index: &mut TreeIndex,
        actions: &mut ActionIndex<Message>,
        profile: WidthProfile<'static>,
    ) -> Result<(), NodeId> {
        let focus_fallback = self
            .key_interaction
            .as_deref()
            .and_then(NodeKeyInteraction::focus_fallback)
            .or(inherited_focus_fallback);
        let mut child_parent = parent.cloned();
        if let Some(id) = &self.id {
            let kind = match &self.kind {
                NodeKind::TextInput { .. } => InteractiveKind::TextInput,
                NodeKind::ScrollViewport { options, .. } => scroll_interactive_kind(options.axis),
                NodeKind::VirtualScrollViewport(virtual_node) => {
                    scroll_interactive_kind(virtual_node.options.axis)
                }
                NodeKind::VirtualFlow(_) => scroll_interactive_kind(ScrollAxis::Vertical),
                NodeKind::Modal { .. } => InteractiveKind::Modal,
                _ => InteractiveKind::Generic,
            };
            index.register(
                NodeRecord {
                    id: id.clone(),
                    parent: parent.cloned(),
                    rect,
                    clip,
                    focusable: self.focusable,
                    has_handler: self.handler.is_some() || self.pointer_handler.is_some(),
                    blocks_unhandled_events: self.blocks_unhandled_events,
                    kind,
                },
                is_root,
            )?;
            if let Some(target) = focus_fallback {
                index.register_focus_fallback(id, target.clone());
            }
            if let NodeKind::Modal { focus, .. } = &self.kind {
                index.set_active_modal_focus(id, focus.clone());
            }
            if let Some(key_interaction) = &self.key_interaction {
                actions.register(id, key_interaction);
            }
            if kind.is_scroll_viewport() {
                if let Some(target) = self
                    .key_interaction
                    .as_deref()
                    .and_then(NodeKeyInteraction::reveal_target)
                {
                    index.register_reveal(id.clone(), target.clone());
                }
            }
            child_parent = Some(id.clone());
        }
        let parent = child_parent.as_ref();
        match &self.kind {
            NodeKind::Text { .. }
            | NodeKind::RichText { .. }
            | NodeKind::Surface(_)
            | NodeKind::Spacer(_)
            | NodeKind::Gap(_)
            | NodeKind::CursorAnchor { .. }
            | NodeKind::TextInput { .. } => {}
            NodeKind::Row(linear) => {
                let layout = linear.layout(rect, true, profile);
                for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
                    child.build_index(
                        child_rect,
                        clip,
                        parent,
                        false,
                        focus_fallback,
                        interaction,
                        index,
                        actions,
                        profile,
                    )?;
                }
            }
            NodeKind::Column(linear) => {
                let layout = linear.layout(rect, false, profile);
                for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
                    child.build_index(
                        child_rect,
                        clip,
                        parent,
                        false,
                        focus_fallback,
                        interaction,
                        index,
                        actions,
                        profile,
                    )?;
                }
            }
            NodeKind::ResponsiveRow(responsive) => {
                let layout = responsive.layout(rect, profile);
                for (item, item_rect) in responsive.items.iter().zip(layout.as_slice()) {
                    if let Some(item_rect) = item_rect {
                        item.node.build_index(
                            *item_rect,
                            clip,
                            parent,
                            false,
                            focus_fallback,
                            interaction,
                            index,
                            actions,
                            profile,
                        )?;
                    }
                }
            }
            NodeKind::SplitPane {
                primary,
                secondary,
                options,
            } => {
                let layout = resolve_split_pane_layout(rect, *options);
                if layout.collapsed != Some(SplitPaneCollapse::Primary) {
                    primary.build_index(
                        layout.primary,
                        clip,
                        parent,
                        false,
                        focus_fallback,
                        interaction,
                        index,
                        actions,
                        profile,
                    )?;
                }
                if layout.collapsed != Some(SplitPaneCollapse::Secondary) {
                    secondary.build_index(
                        layout.secondary,
                        clip,
                        parent,
                        false,
                        focus_fallback,
                        interaction,
                        index,
                        actions,
                        profile,
                    )?;
                }
            }
            NodeKind::Stack(children) => {
                for child in children {
                    child.build_index(
                        rect,
                        clip,
                        parent,
                        false,
                        focus_fallback,
                        interaction,
                        index,
                        actions,
                        profile,
                    )?;
                }
            }
            NodeKind::Overlay { base, layer } => {
                base.build_index(
                    rect,
                    clip,
                    parent,
                    false,
                    focus_fallback,
                    interaction,
                    index,
                    actions,
                    profile,
                )?;
                layer.build_index(
                    rect,
                    clip,
                    parent,
                    false,
                    focus_fallback,
                    interaction,
                    index,
                    actions,
                    profile,
                )?;
            }
            NodeKind::AnchoredOverlay {
                base,
                anchor,
                overlay,
                options,
                cache,
            } => {
                base.build_index(
                    rect,
                    clip,
                    parent,
                    false,
                    focus_fallback,
                    interaction,
                    index,
                    actions,
                    profile,
                )?;
                *cache.borrow_mut() = None;
                if let Some(overlay_rect) = anchored_overlay_rect(
                    base,
                    anchor,
                    overlay,
                    *options,
                    cache,
                    rect,
                    clip,
                    interaction,
                    profile,
                ) {
                    overlay.build_index(
                        overlay_rect,
                        clip.intersection(rect),
                        parent,
                        false,
                        focus_fallback,
                        interaction,
                        index,
                        actions,
                        profile,
                    )?;
                }
            }
            NodeKind::Padding { insets, child } => child.build_index(
                inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                clip,
                parent,
                false,
                focus_fallback,
                interaction,
                index,
                actions,
                profile,
            )?,
            NodeKind::Border { child, .. } => child.build_index(
                inset(rect, 1, 1, 1, 1),
                clip,
                parent,
                false,
                focus_fallback,
                interaction,
                index,
                actions,
                profile,
            )?,
            NodeKind::Align {
                horizontal,
                vertical,
                child,
            } => child.build_index(
                aligned_child_rect(rect, child, *horizontal, *vertical, profile),
                clip,
                parent,
                false,
                focus_fallback,
                interaction,
                index,
                actions,
                profile,
            )?,
            NodeKind::Clip(child) => child.build_index(
                rect,
                clip.intersection(rect),
                parent,
                false,
                focus_fallback,
                interaction,
                index,
                actions,
                profile,
            )?,
            NodeKind::ScrollViewport { child, options } => {
                let id = self
                    .id
                    .as_ref()
                    .expect("ScrollViewport always has a NodeId");
                child.build_index(
                    scroll_child_rect(
                        rect,
                        child,
                        interaction.scroll_offset(id),
                        options.axis,
                        profile,
                    ),
                    clip.intersection(rect),
                    parent,
                    false,
                    focus_fallback,
                    interaction,
                    index,
                    actions,
                    profile,
                )?;
            }
            NodeKind::VirtualScrollViewport(virtual_node) => {
                let id = self
                    .id
                    .as_ref()
                    .expect("VirtualScrollViewport always has a NodeId");
                let state = interaction.preview_scroll(
                    id,
                    virtual_scroll_maximum(
                        virtual_node.content_size,
                        rect,
                        virtual_node.options.axis,
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
                    fragment.fragment.node.build_index(
                        virtual_fragment_rect(rect, &fragment, profile),
                        clip.intersection(rect),
                        parent,
                        false,
                        focus_fallback,
                        interaction,
                        index,
                        actions,
                        profile,
                    )?;
                }
            }
            NodeKind::VirtualFlow(flow) => {
                if let Some(frame) = flow.cache.borrow().as_ref() {
                    for item in &frame.items {
                        item.node.build_index(
                            virtual_flow_item_rect(rect, frame.offset, item.origin, item.height),
                            clip.intersection(rect),
                            parent,
                            false,
                            focus_fallback,
                            interaction,
                            index,
                            actions,
                            profile,
                        )?;
                    }
                }
            }
            NodeKind::Modal { child, .. } => child.build_index(
                rect,
                clip,
                parent,
                false,
                focus_fallback,
                interaction,
                index,
                actions,
                profile,
            )?,
            NodeKind::Panel { options, child, .. } => {
                let insets = panel_content_insets(*options);
                child.build_index(
                    inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                    clip,
                    parent,
                    false,
                    focus_fallback,
                    interaction,
                    index,
                    actions,
                    profile,
                )?;
            }
        }
        Ok(())
    }

    fn prepare_virtual_flows_at(
        &self,
        rect: Rect,
        interaction: &mut InteractionState,
        profile: WidthProfile<'static>,
    ) {
        match &self.kind {
            NodeKind::VirtualFlow(flow) => {
                let id = self.id.as_ref().expect("VirtualFlow always has a NodeId");
                prepare_virtual_flow_node(id, flow, rect, interaction, profile);
            }
            NodeKind::Row(linear) => {
                let layout = linear.layout(rect, true, profile);
                for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
                    child.prepare_virtual_flows_at(child_rect, interaction, profile);
                }
            }
            NodeKind::Column(linear) => {
                let layout = linear.layout(rect, false, profile);
                for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
                    child.prepare_virtual_flows_at(child_rect, interaction, profile);
                }
            }
            NodeKind::ResponsiveRow(responsive) => {
                let layout = responsive.layout(rect, profile);
                for (item, item_rect) in responsive.items.iter().zip(layout.as_slice()) {
                    if let Some(item_rect) = item_rect {
                        item.node
                            .prepare_virtual_flows_at(*item_rect, interaction, profile);
                    }
                }
            }
            NodeKind::SplitPane {
                primary,
                secondary,
                options,
            } => {
                let layout = resolve_split_pane_layout(rect, *options);
                if layout.collapsed != Some(SplitPaneCollapse::Primary) {
                    primary.prepare_virtual_flows_at(layout.primary, interaction, profile);
                }
                if layout.collapsed != Some(SplitPaneCollapse::Secondary) {
                    secondary.prepare_virtual_flows_at(layout.secondary, interaction, profile);
                }
            }
            NodeKind::Stack(children) => {
                for child in children {
                    child.prepare_virtual_flows_at(rect, interaction, profile);
                }
            }
            NodeKind::Overlay { base, layer } => {
                base.prepare_virtual_flows_at(rect, interaction, profile);
                layer.prepare_virtual_flows_at(rect, interaction, profile);
            }
            NodeKind::AnchoredOverlay {
                base,
                anchor,
                overlay,
                options,
                cache,
            } => {
                base.prepare_virtual_flows_at(rect, interaction, profile);
                *cache.borrow_mut() = None;
                if let Some(overlay_rect) = anchored_overlay_rect(
                    base,
                    anchor,
                    overlay,
                    *options,
                    cache,
                    rect,
                    rect,
                    interaction,
                    profile,
                ) {
                    overlay.prepare_virtual_flows_at(overlay_rect, interaction, profile);
                }
            }
            NodeKind::Padding { insets, child } => child.prepare_virtual_flows_at(
                inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                interaction,
                profile,
            ),
            NodeKind::Border { child, .. } => {
                child.prepare_virtual_flows_at(inset(rect, 1, 1, 1, 1), interaction, profile)
            }
            NodeKind::Align {
                horizontal,
                vertical,
                child,
            } => child.prepare_virtual_flows_at(
                aligned_child_rect(rect, child, *horizontal, *vertical, profile),
                interaction,
                profile,
            ),
            NodeKind::Clip(child) | NodeKind::Modal { child, .. } => {
                child.prepare_virtual_flows_at(rect, interaction, profile);
            }
            NodeKind::Panel { options, child, .. } => {
                let insets = panel_content_insets(*options);
                child.prepare_virtual_flows_at(
                    inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                    interaction,
                    profile,
                );
            }
            NodeKind::ScrollViewport { child, options } => {
                let id = self
                    .id
                    .as_ref()
                    .expect("ScrollViewport always has a NodeId");
                child.prepare_virtual_flows_at(
                    scroll_child_rect(
                        rect,
                        child,
                        interaction.scroll_offset(id),
                        options.axis,
                        profile,
                    ),
                    interaction,
                    profile,
                );
            }
            NodeKind::VirtualScrollViewport(virtual_node) => {
                let id = self
                    .id
                    .as_ref()
                    .expect("VirtualScrollViewport always has a NodeId");
                let state = interaction.preview_scroll(
                    id,
                    virtual_scroll_maximum(
                        virtual_node.content_size,
                        rect,
                        virtual_node.options.axis,
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
                    fragment.fragment.node.prepare_virtual_flows_at(
                        virtual_fragment_rect(rect, &fragment, profile),
                        interaction,
                        profile,
                    );
                }
            }
            NodeKind::Text { .. }
            | NodeKind::RichText { .. }
            | NodeKind::Surface(_)
            | NodeKind::Spacer(_)
            | NodeKind::Gap(_)
            | NodeKind::CursorAnchor { .. }
            | NodeKind::TextInput { .. } => {}
        }
    }

    fn prepare_at(
        &self,
        rect: Rect,
        interaction: &mut InteractionState,
        profile: WidthProfile<'static>,
    ) -> bool {
        match &self.kind {
            NodeKind::TextInput { value, .. } => {
                interaction.ensure_text_input(
                    self.id.as_ref().expect("TextInput always has a NodeId"),
                    value,
                );
                return false;
            }
            NodeKind::ScrollViewport { child, options } => {
                let id = self
                    .id
                    .as_ref()
                    .expect("ScrollViewport always has a NodeId");
                let previous_offset = interaction.scroll_offset(id);
                let content = child.measure(scroll_constraints(rect, options.axis), profile);
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
                let child_changed = child.prepare_at(
                    scroll_child_rect(rect, child, state.offset, options.axis, profile),
                    interaction,
                    profile,
                );
                return state.offset != previous_offset || child_changed;
            }
            NodeKind::VirtualScrollViewport(virtual_node) => {
                let id = self
                    .id
                    .as_ref()
                    .expect("VirtualScrollViewport always has a NodeId");
                let previous_request = virtual_node
                    .cache
                    .borrow()
                    .as_ref()
                    .map(|cached| cached.request);
                let state = interaction.prepare_scroll(
                    id,
                    virtual_scroll_maximum(
                        virtual_node.content_size,
                        rect,
                        virtual_node.options.axis,
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
                    let request_changed = previous_request != Some(fragment.request);
                    let child_changed = fragment.fragment.node.prepare_at(
                        virtual_fragment_rect(rect, &fragment, profile),
                        interaction,
                        profile,
                    );
                    return request_changed || child_changed;
                }
                return previous_request.is_some();
            }
            NodeKind::VirtualFlow(flow) => {
                let id = self.id.as_ref().expect("VirtualFlow always has a NodeId");
                return prepare_virtual_flow_node(id, flow, rect, interaction, profile);
            }
            _ => {}
        }
        match &self.kind {
            NodeKind::Row(linear) => {
                let layout = linear.layout(rect, true, profile);
                let mut changed = false;
                for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
                    changed |= child.prepare_at(child_rect, interaction, profile);
                }
                changed
            }
            NodeKind::Column(linear) => {
                let layout = linear.layout(rect, false, profile);
                let mut changed = false;
                for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
                    changed |= child.prepare_at(child_rect, interaction, profile);
                }
                changed
            }
            NodeKind::ResponsiveRow(responsive) => {
                let layout = responsive.layout(rect, profile);
                let mut changed = false;
                for (item, item_rect) in responsive.items.iter().zip(layout.as_slice()) {
                    if let Some(item_rect) = item_rect {
                        changed |= item.node.prepare_at(*item_rect, interaction, profile);
                    }
                }
                changed
            }
            NodeKind::SplitPane {
                primary,
                secondary,
                options,
            } => {
                let layout = resolve_split_pane_layout(rect, *options);
                let mut changed = false;
                if layout.collapsed != Some(SplitPaneCollapse::Primary) {
                    changed |= primary.prepare_at(layout.primary, interaction, profile);
                }
                if layout.collapsed != Some(SplitPaneCollapse::Secondary) {
                    changed |= secondary.prepare_at(layout.secondary, interaction, profile);
                }
                changed
            }
            NodeKind::Stack(children) => {
                let mut changed = false;
                for child in children {
                    changed |= child.prepare_at(rect, interaction, profile);
                }
                changed
            }
            NodeKind::Overlay { base, layer } => {
                base.prepare_at(rect, interaction, profile)
                    | layer.prepare_at(rect, interaction, profile)
            }
            NodeKind::AnchoredOverlay {
                base,
                anchor,
                overlay,
                options,
                cache,
            } => {
                let mut changed = base.prepare_at(rect, interaction, profile);
                *cache.borrow_mut() = None;
                if let Some(overlay_rect) = anchored_overlay_rect(
                    base,
                    anchor,
                    overlay,
                    *options,
                    cache,
                    rect,
                    rect,
                    interaction,
                    profile,
                ) {
                    changed |= overlay.prepare_at(overlay_rect, interaction, profile);
                }
                changed
            }
            NodeKind::Padding { insets, child } => child.prepare_at(
                inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                interaction,
                profile,
            ),
            NodeKind::Border { child, .. } => {
                child.prepare_at(inset(rect, 1, 1, 1, 1), interaction, profile)
            }
            NodeKind::Align {
                horizontal,
                vertical,
                child,
            } => child.prepare_at(
                aligned_child_rect(rect, child, *horizontal, *vertical, profile),
                interaction,
                profile,
            ),
            NodeKind::Clip(child) => child.prepare_at(rect, interaction, profile),
            NodeKind::Modal { child, .. } => child.prepare_at(rect, interaction, profile),
            NodeKind::Panel { options, child, .. } => {
                let insets = panel_content_insets(*options);
                child.prepare_at(
                    inset(rect, insets.left, insets.top, insets.right, insets.bottom),
                    interaction,
                    profile,
                )
            }
            NodeKind::Text { .. }
            | NodeKind::RichText { .. }
            | NodeKind::Surface(_)
            | NodeKind::Spacer(_)
            | NodeKind::Gap(_)
            | NodeKind::CursorAnchor { .. }
            | NodeKind::TextInput { .. }
            | NodeKind::ScrollViewport { .. }
            | NodeKind::VirtualScrollViewport(_)
            | NodeKind::VirtualFlow(_) => false,
        }
    }
}

const fn scroll_interactive_kind(axis: ScrollAxis) -> InteractiveKind {
    if matches!(axis, ScrollAxis::Horizontal) {
        InteractiveKind::ScrollViewportHorizontal
    } else {
        InteractiveKind::ScrollViewportVertical
    }
}

fn measure_text(content: &str, constraints: Constraints, profile: WidthProfile<'static>) -> Size {
    let max_width = match constraints.width {
        Limit::Bounded(width) => width as usize,
        Limit::Unbounded => usize::MAX,
    };
    let mut width = 0_usize;
    let mut height = 0_usize;
    for line in wrapped_lines(content, max_width, profile) {
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
    cache: &RefCell<ParagraphLayoutCache>,
    constraints: Constraints,
    profile: WidthProfile<'static>,
) -> Size {
    let (max_width, bounded) = match constraints.width {
        Limit::Bounded(width) => (width, true),
        Limit::Unbounded => (0, false),
    };
    cache
        .borrow_mut()
        .resolve(spans, max_width, bounded, options.wrap, profile)
        .size
}

fn measure_split_pane<Message>(
    primary: &Node<Message>,
    secondary: &Node<Message>,
    constraints: Constraints,
    options: SplitPaneOptions,
    profile: WidthProfile<'static>,
) -> Size {
    let child_constraints = match options.axis {
        crate::SplitPaneAxis::Horizontal => Constraints {
            width: Limit::Unbounded,
            height: constraints.height,
        },
        crate::SplitPaneAxis::Vertical => Constraints {
            width: constraints.width,
            height: Limit::Unbounded,
        },
    };
    let primary = primary.measure(child_constraints, profile);
    let secondary = secondary.measure(child_constraints, profile);
    match options.axis {
        crate::SplitPaneAxis::Horizontal => Size::new(
            primary
                .width
                .saturating_add(1)
                .saturating_add(secondary.width),
            primary.height.max(secondary.height),
        ),
        crate::SplitPaneAxis::Vertical => Size::new(
            primary.width.max(secondary.width),
            primary
                .height
                .saturating_add(1)
                .saturating_add(secondary.height),
        ),
    }
}

fn measure_linear<Message>(
    children: &[Node<Message>],
    constraints: Constraints,
    horizontal: bool,
    profile: WidthProfile<'static>,
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
        let mut measured = child.measure(child_constraints, profile);
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

#[allow(clippy::too_many_arguments)]
fn render_split_pane<Message>(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    primary: &Node<Message>,
    secondary: &Node<Message>,
    options: SplitPaneOptions,
    interaction: &InteractionState,
    profile: WidthProfile<'static>,
) {
    let layout = resolve_split_pane_layout(rect, options);
    if layout.collapsed != Some(SplitPaneCollapse::Primary) {
        primary.render(surface, layout.primary, clip, interaction, profile);
    }
    if let Some(divider) = layout.divider {
        let glyphs = profile_aware_border_glyphs(border_glyphs(BorderKind::Single), profile);
        match options.axis {
            crate::SplitPaneAxis::Horizontal => {
                for y in i64::from(divider.y)
                    ..i64::from(divider.y).saturating_add(i64::from(divider.height))
                {
                    write_border_cell(
                        surface,
                        clip,
                        i64::from(divider.x),
                        y,
                        glyphs.vertical,
                        options.divider_style,
                        profile,
                    );
                }
            }
            crate::SplitPaneAxis::Vertical => {
                for x in i64::from(divider.x)
                    ..i64::from(divider.x).saturating_add(i64::from(divider.width))
                {
                    write_border_cell(
                        surface,
                        clip,
                        x,
                        i64::from(divider.y),
                        glyphs.horizontal,
                        options.divider_style,
                        profile,
                    );
                }
            }
        }
    }
    if layout.collapsed != Some(SplitPaneCollapse::Secondary) {
        secondary.render(surface, layout.secondary, clip, interaction, profile);
    }
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

#[allow(clippy::too_many_arguments)]
fn anchored_overlay_rect<Message>(
    base: &Node<Message>,
    anchor: &NodeId,
    overlay: &Node<Message>,
    options: AnchoredOverlayOptions,
    cache: &RefCell<Option<AnchoredOverlayFrame>>,
    rect: Rect,
    clip: Rect,
    interaction: &InteractionState,
    profile: WidthProfile<'static>,
) -> Option<Rect> {
    if let Some(frame) = cache.borrow().as_ref() {
        if frame.rect == rect && frame.clip == clip {
            return frame.overlay;
        }
    }
    let boundary = rect.intersection(clip);
    let anchor_geometry = base.find_node_geometry(anchor, rect, clip, interaction, profile);
    let resolved = anchor_geometry.and_then(|(anchor_rect, anchor_clip)| {
        resolve_anchored_overlay_rect(
            anchor_rect,
            anchor_clip,
            boundary,
            overlay,
            options,
            profile,
        )
    });
    *cache.borrow_mut() = Some(AnchoredOverlayFrame {
        rect,
        clip,
        overlay: resolved,
    });
    resolved
}

fn resolve_anchored_overlay_rect<Message>(
    anchor: Rect,
    anchor_clip: Rect,
    boundary: Rect,
    overlay: &Node<Message>,
    options: AnchoredOverlayOptions,
    profile: WidthProfile<'static>,
) -> Option<Rect> {
    if !anchored_overlay_anchor_visible(anchor, anchor_clip, boundary) {
        return None;
    }
    let width_limit = if options.maximum_width == 0 {
        boundary.width
    } else {
        boundary.width.min(options.maximum_width)
    };
    let height_limit = if options.maximum_height == 0 {
        boundary.height
    } else {
        boundary.height.min(options.maximum_height)
    };
    let desired = overlay.measure(
        Constraints::bounded(Size::new(width_limit, height_limit)),
        profile,
    );
    if desired.is_empty() {
        return None;
    }

    let boundary_top = i64::from(boundary.y);
    let boundary_bottom = boundary_top.saturating_add(i64::from(boundary.height));
    let anchor_top = i64::from(anchor.y);
    let anchor_bottom = anchor_top.saturating_add(i64::from(anchor.height));
    let gap = i64::from(options.gap);
    let below_start = anchor_bottom.saturating_add(gap);
    let above_end = anchor_top.saturating_sub(gap);
    let below = extent_to_u32(below_start, boundary_bottom);
    let above = extent_to_u32(boundary_top, above_end);
    let preferred = match options.side {
        AnchoredOverlaySide::Below => below,
        AnchoredOverlaySide::Above => above,
    };
    let opposite = match options.side {
        AnchoredOverlaySide::Below => above,
        AnchoredOverlaySide::Above => below,
    };
    let side = if options.fallback == AnchoredOverlayFallback::Flip
        && desired.height > preferred
        && opposite > preferred
    {
        match options.side {
            AnchoredOverlaySide::Below => AnchoredOverlaySide::Above,
            AnchoredOverlaySide::Above => AnchoredOverlaySide::Below,
        }
    } else {
        options.side
    };
    let available_height = match side {
        AnchoredOverlaySide::Below => below,
        AnchoredOverlaySide::Above => above,
    };
    let height = desired.height.min(available_height);
    let width = desired.width.min(boundary.width);
    if width == 0 || height == 0 {
        return None;
    }

    let anchor_left = i64::from(anchor.x);
    let anchor_right = anchor_left.saturating_add(i64::from(anchor.width));
    let candidate_x = match options.alignment {
        HorizontalAlignment::Start => anchor_left,
        HorizontalAlignment::Center => anchor_left
            .saturating_add(i64::from(anchor.width) / 2)
            .saturating_sub(i64::from(width) / 2),
        HorizontalAlignment::End => anchor_right.saturating_sub(i64::from(width)),
    };
    let boundary_left = i64::from(boundary.x);
    let boundary_right = boundary_left.saturating_add(i64::from(boundary.width));
    let maximum_x = boundary_right.saturating_sub(i64::from(width));
    let x = candidate_x.clamp(boundary_left, maximum_x);
    let y = match side {
        AnchoredOverlaySide::Below => below_start.max(boundary_top),
        AnchoredOverlaySide::Above => above_end
            .saturating_sub(i64::from(height))
            .max(boundary_top),
    };
    Some(Rect::new(
        clamp_i64_to_i32(x),
        clamp_i64_to_i32(y),
        width,
        height,
    ))
}

fn anchored_overlay_anchor_visible(anchor: Rect, anchor_clip: Rect, boundary: Rect) -> bool {
    if anchor.height == 0 || anchor_clip.is_empty() || boundary.is_empty() {
        return false;
    }
    let anchor_top = i64::from(anchor.y);
    let anchor_bottom = anchor_top.saturating_add(i64::from(anchor.height));
    let visible_top = anchor_top
        .max(i64::from(anchor_clip.y))
        .max(i64::from(boundary.y));
    let visible_bottom = anchor_bottom
        .min(i64::from(anchor_clip.y).saturating_add(i64::from(anchor_clip.height)))
        .min(i64::from(boundary.y).saturating_add(i64::from(boundary.height)));
    if visible_bottom <= visible_top {
        return false;
    }
    if anchor.width != 0 {
        return !anchor
            .intersection(anchor_clip)
            .intersection(boundary)
            .is_empty();
    }
    let anchor_x = i64::from(anchor.x);
    let visible_left = i64::from(anchor_clip.x).max(i64::from(boundary.x));
    let visible_right = i64::from(anchor_clip.x)
        .saturating_add(i64::from(anchor_clip.width))
        .min(i64::from(boundary.x).saturating_add(i64::from(boundary.width)));
    anchor_x >= visible_left && anchor_x < visible_right
}

fn extent_to_u32(start: i64, end: i64) -> u32 {
    u32::try_from(end.saturating_sub(start).max(0)).unwrap_or(u32::MAX)
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
    profile: WidthProfile<'static>,
) {
    let layout = linear.layout(rect, horizontal, profile);
    for (child, child_rect) in linear.children.iter().zip(layout.rects(rect)) {
        child.render(surface, child_rect, clip, interaction, profile);
    }
}

fn measure_responsive_row<Message>(
    responsive: &ResponsiveRowNode<Message>,
    constraints: Constraints,
    profile: WidthProfile<'static>,
) -> Size {
    let height = match (responsive.options.height, constraints.height) {
        (0, limit) => limit,
        (height, Limit::Unbounded) => Limit::Bounded(height),
        (height, Limit::Bounded(limit)) => Limit::Bounded(height.min(limit)),
    };
    let child_constraints = Constraints {
        width: Limit::Unbounded,
        height,
    };
    let mut measured = Size::default();
    for item in &responsive.items {
        let child = item.node.measure(child_constraints, profile);
        measured.width = measured.width.saturating_add(child.width.max(1));
        measured.height = measured.height.max(child.height);
    }
    measured.width = measured.width.saturating_add(
        responsive
            .options
            .gap
            .saturating_mul(responsive.items.len().saturating_sub(1) as u32),
    );
    if responsive.options.height != 0 {
        measured.height = responsive.options.height;
    }
    measured
}

impl<Message> ResponsiveRowNode<Message> {
    fn layout(
        &self,
        rect: Rect,
        profile: WidthProfile<'static>,
    ) -> Ref<'_, ResolvedResponsiveRowLayout> {
        let rebuild = self
            .cache
            .borrow()
            .as_ref()
            .is_none_or(|cached| cached.rect != rect);
        if rebuild {
            let layout_rect = Rect::new(
                rect.x,
                rect.y,
                rect.width,
                if self.options.height == 0 {
                    rect.height
                } else {
                    rect.height.min(self.options.height)
                },
            );
            let constraints = Constraints {
                width: Limit::Unbounded,
                height: Limit::Bounded(layout_rect.height),
            };
            let mut inline_metrics = [ResponsiveRowMetric::default(); INLINE_RESPONSIVE_ROW_ITEMS];
            let mut overflow_metrics = if self.items.len() > INLINE_RESPONSIVE_ROW_ITEMS {
                vec![ResponsiveRowMetric::default(); self.items.len()]
            } else {
                Vec::new()
            };
            let metrics = if overflow_metrics.is_empty() {
                &mut inline_metrics[..self.items.len()]
            } else {
                overflow_metrics.as_mut_slice()
            };
            for (metric, item) in metrics.iter_mut().zip(&self.items) {
                *metric = ResponsiveRowMetric {
                    placement: item.placement,
                    priority: item.priority,
                    desired_width: item.node.measure(constraints, profile).width.max(1),
                };
            }
            let layout = resolve_responsive_row_layout(layout_rect, metrics, self.options.gap);
            *self.cache.borrow_mut() = Some(Box::new(CachedResponsiveRowLayout { rect, layout }));
        }
        Ref::map(self.cache.borrow(), |cached| {
            &cached
                .as_ref()
                .expect("responsive row layout cache is initialized")
                .layout
        })
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
    fn layout(
        &self,
        rect: Rect,
        horizontal: bool,
        profile: WidthProfile<'static>,
    ) -> Ref<'_, LinearLayout> {
        let rebuild = self
            .cache
            .borrow()
            .as_ref()
            .is_none_or(|cached| cached.rect != rect || cached.layout.horizontal != horizontal);
        if rebuild {
            let layout = resolve_linear_layout(&self.children, rect, horizontal, profile);
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
    profile: WidthProfile<'static>,
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
        fill_linear_tracks(
            children,
            rect,
            horizontal,
            &mut tracks[..children.len()],
            profile,
        );
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
    fill_linear_tracks(children, rect, horizontal, &mut tracks, profile);
    allocate_into(available, &tracks, &mut layout.overflow, &mut minimums);
    layout
}

fn fill_linear_tracks<Message>(
    children: &[Node<Message>],
    rect: Rect,
    horizontal: bool,
    tracks: &mut [Track],
    profile: WidthProfile<'static>,
) {
    for (index, child) in children.iter().enumerate() {
        let measured = child.measure(Constraints::bounded(rect.size()), profile);
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

fn render_text(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    content: &str,
    style: Style,
    profile: WidthProfile<'static>,
) {
    if rect.is_empty() {
        return;
    }
    let lines = wrapped_lines(content, rect.width as usize, profile);
    for (line_index, line) in lines.take(rect.height as usize).enumerate() {
        let y = add_coordinate(rect.y, line_index as u32);
        let mut x = i64::from(rect.x);
        for grapheme in graphemes(line.text()) {
            let span = grapheme_width(grapheme.text(), profile).max(1) as i64;
            let end = x.saturating_add(span);
            if contains_unit(clip, x, i64::from(y), end) {
                surface.write(clamp_i64_to_i32(x), y, grapheme.text(), style, profile);
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
    cache: &RefCell<ParagraphLayoutCache>,
    profile: WidthProfile<'static>,
) {
    if rect.is_empty() {
        return;
    }
    let mut cache = cache.borrow_mut();
    let layout = cache.resolve(spans, rect.width, true, options.wrap, profile);
    for line_index in 0..layout.lines.len().min(rect.height as usize) {
        render_rich_text_line(
            surface,
            RenderRegion { rect, clip },
            line_index as u32,
            &layout,
            spans,
            options,
            profile,
        );
    }
}

#[derive(Clone, Copy)]
struct RenderRegion {
    rect: Rect,
    clip: Rect,
}

fn render_rich_text_line(
    surface: &mut Surface,
    region: RenderRegion,
    line_index: u32,
    layout: &crate::rich_text::ParagraphLayout<'_>,
    spans: &[TextSpan],
    options: ParagraphOptions,
    profile: WidthProfile<'static>,
) {
    let RenderRegion { rect, clip } = region;
    let line = &layout.lines[line_index as usize];
    let desired = line.width.min(rect.width);
    let mut x = i64::from(add_coordinate(
        rect.x,
        alignment_offset(rect.width, desired, options.alignment),
    ));
    let right = i64::from(rect.x) + i64::from(rect.width);
    let y = add_coordinate(rect.y, line_index);
    for unit in &layout.units[line.start..line.end] {
        let end = x.saturating_add(i64::from(unit.width));
        if end > right {
            break;
        }
        if contains_unit(clip, x, i64::from(y), end) {
            surface.write(
                clamp_i64_to_i32(x),
                y,
                unit.text(spans),
                unit.style(spans),
                profile,
            );
        }
        x = end;
    }
}

fn render_cursor_anchor(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    focus_owner: &NodeId,
    interaction: &InteractionState,
) {
    if interaction.focused() != Some(focus_owner) || rect.x < clip.x || rect.y < clip.y {
        return;
    }
    let mut x = i64::from(rect.x);
    let right = i64::from(clip.x) + i64::from(clip.width);
    if x == right && clip.width > 0 {
        x -= 1;
    }
    let point = crate::Point::new(clamp_i64_to_i32(x), rect.y);
    if !clip.contains(point) || point.x < 0 || point.y < 0 {
        return;
    }
    let _ = surface.set_cursor(Some(Cursor::new(point.x as u32, point.y as u32)));
}

fn render_surface_node(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    source: &Surface,
    profile: WidthProfile<'static>,
) {
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
                if grapheme_width(cell.content(), profile) != span as usize {
                    continue;
                }
                surface.write(target_x, target_y, cell.content(), cell.style(), profile);
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
    profile: WidthProfile<'static>,
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
        cell_at_byte(value, cursor, profile).unwrap_or(0)
    };
    let visible_width = rect.width as usize;
    let requested_start = cursor_cell.saturating_sub(visible_width.saturating_sub(1));
    let mut start_cell = requested_start;
    let start_byte = loop {
        if let Some(byte) = byte_at_cell(content, start_cell, profile) {
            break byte;
        }
        if start_cell == 0 {
            break 0;
        }
        start_cell -= 1;
    };
    render_single_line(
        surface,
        rect,
        clip,
        &content[start_byte..],
        content_style,
        profile,
    );

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

fn render_single_line(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    content: &str,
    style: Style,
    profile: WidthProfile<'static>,
) {
    let mut x = i64::from(rect.x);
    let right = i64::from(rect.x) + i64::from(rect.width);
    for grapheme in graphemes(content) {
        let span = grapheme_width(grapheme.text(), profile).max(1) as i64;
        let end = x.saturating_add(span);
        if end > right {
            break;
        }
        if contains_unit(clip, x, i64::from(rect.y), end) {
            surface.write(clamp_i64_to_i32(x), rect.y, grapheme.text(), style, profile);
        }
        x = end;
    }
}

fn scroll_child_rect<Message>(
    viewport: Rect,
    child: &Node<Message>,
    requested: ScrollOffset,
    axis: ScrollAxis,
    profile: WidthProfile<'static>,
) -> Rect {
    let content = child.measure(scroll_constraints(viewport, axis), profile);
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

fn virtual_flow_intrinsic_size(constraints: Constraints) -> Size {
    let width = match constraints.width {
        Limit::Bounded(width) => width,
        Limit::Unbounded => 0,
    };
    Size::new(width, 0)
}

fn prepare_virtual_flow_node<Message>(
    id: &NodeId,
    flow: &VirtualFlowNode<Message>,
    rect: Rect,
    interaction: &mut InteractionState,
    profile: WidthProfile<'static>,
) -> bool {
    let previous = flow.cache.borrow_mut().take();
    let previous_signature = previous.as_ref().map(|frame| {
        (
            frame.rect,
            frame.offset,
            frame.content_height,
            frame.generation,
            frame.items.first().map(|item| item.index),
            frame.items.last().map(|item| item.index),
        )
    });
    let mut reusable = HashMap::new();
    if let Some(frame) = previous {
        if frame.rect.width == rect.width {
            reusable.extend(frame.items.into_iter().map(|item| (item.index, item.node)));
        }
    }

    let mut window = interaction.prepare_virtual_flow(
        id,
        &flow.source,
        rect.width,
        rect.height,
        flow.options.overscan,
        flow.options.stick_to_end,
    );
    loop {
        for index in window.built.clone() {
            reusable
                .entry(index)
                .or_insert_with(|| flow.source.build(index, rect.width));
        }
        let mut measurements = Vec::new();
        for index in window.built.clone() {
            let Some((_, _, measured)) = interaction.virtual_flow_item_layout(id, index) else {
                continue;
            };
            if measured {
                continue;
            }
            let height = reusable
                .get(&index)
                .expect("a requested virtual flow item was built")
                .measure(
                    Constraints {
                        width: Limit::Bounded(rect.width),
                        height: Limit::Unbounded,
                    },
                    profile,
                )
                .height
                .max(1);
            measurements.push((index, height));
        }
        if measurements.is_empty() {
            break;
        }
        let Some(next) = interaction.apply_virtual_flow_measurements(
            id,
            &measurements,
            rect.height,
            flow.options.overscan,
            flow.options.stick_to_end,
        ) else {
            break;
        };
        window = next;
    }

    let mut items = Vec::with_capacity(window.built.len());
    let mut child_changed = false;
    for index in window.built.clone() {
        let node = reusable
            .remove(&index)
            .unwrap_or_else(|| flow.source.build(index, rect.width));
        let Some((origin, height, _)) = interaction.virtual_flow_item_layout(id, index) else {
            continue;
        };
        child_changed |= node.prepare_at(
            virtual_flow_item_rect(rect, window.offset, origin, height),
            interaction,
            profile,
        );
        items.push(VirtualFlowBuiltItem {
            index,
            origin,
            height,
            node,
        });
    }
    let frame = VirtualFlowFrame {
        rect,
        offset: window.offset,
        content_height: window.content_height,
        generation: window.generation,
        items,
    };
    let signature = (
        frame.rect,
        frame.offset,
        frame.content_height,
        frame.generation,
        frame.items.first().map(|item| item.index),
        frame.items.last().map(|item| item.index),
    );
    *flow.cache.borrow_mut() = Some(frame);
    previous_signature != Some(signature) || child_changed
}

fn virtual_flow_item_rect(viewport: Rect, offset: u32, origin: u32, height: u32) -> Rect {
    Rect::new(
        viewport.x,
        clamp_i64_to_i32(i64::from(viewport.y) + i64::from(origin) - i64::from(offset)),
        viewport.width,
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

fn virtual_scroll_maximum(declared: Size, viewport: Rect, axis: ScrollAxis) -> ScrollOffset {
    let content = resolved_virtual_content_size(declared, viewport.size(), axis);
    ScrollOffset::new(
        content.width.saturating_sub(viewport.width),
        content.height.saturating_sub(viewport.height),
    )
}

fn virtual_content_is_empty(content: Size, axis: ScrollAxis) -> bool {
    match axis {
        ScrollAxis::Both => content.is_empty(),
        ScrollAxis::Vertical => content.height == 0,
        ScrollAxis::Horizontal => content.width == 0,
    }
}

fn virtual_fragment_rect<Message>(
    viewport: Rect,
    cached: &VirtualCache<Message>,
    profile: WidthProfile<'static>,
) -> Rect {
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
        .measure(Constraints::bounded(remaining), profile);
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

fn render_border(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    style: Style,
    profile: WidthProfile<'static>,
) {
    render_border_with_glyphs(
        surface,
        rect,
        clip,
        style,
        &profile_aware_border_glyphs(border_glyphs(BorderKind::Single), profile),
        profile,
    );
}

fn render_border_with_glyphs(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    style: Style,
    glyphs: &BorderGlyphs,
    profile: WidthProfile<'static>,
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
            profile,
        );
        if bottom != i64::from(rect.y) {
            write_border_cell(surface, clip, x, bottom, glyphs.horizontal, style, profile);
        }
    }
    for y in i64::from(rect.y)..=bottom {
        write_border_cell(
            surface,
            clip,
            i64::from(rect.x),
            y,
            glyphs.vertical,
            style,
            profile,
        );
        if right != i64::from(rect.x) {
            write_border_cell(surface, clip, right, y, glyphs.vertical, style, profile);
        }
    }
    write_border_cell(
        surface,
        clip,
        i64::from(rect.x),
        i64::from(rect.y),
        glyphs.top_left,
        style,
        profile,
    );
    if right != i64::from(rect.x) {
        write_border_cell(
            surface,
            clip,
            right,
            i64::from(rect.y),
            glyphs.top_right,
            style,
            profile,
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
            profile,
        );
        if right != i64::from(rect.x) {
            write_border_cell(
                surface,
                clip,
                right,
                bottom,
                glyphs.bottom_right,
                style,
                profile,
            );
        }
    }
}

fn render_panel<Message>(
    surface: &mut Surface,
    region: RenderRegion,
    child: &Node<Message>,
    title: &str,
    options: PanelOptions,
    interaction: &InteractionState,
    profile: WidthProfile<'static>,
) {
    let RenderRegion { rect, clip } = region;
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
        &profile_aware_border_glyphs(border_glyphs(options.border), profile),
        profile,
    );
    render_panel_title(surface, rect, clip, title, options.style.title, profile);
    let insets = panel_content_insets(options);
    child.render(
        surface,
        inset(rect, insets.left, insets.top, insets.right, insets.bottom),
        clip,
        interaction,
        profile,
    );
}

fn render_panel_title(
    surface: &mut Surface,
    rect: Rect,
    clip: Rect,
    title: &str,
    style: Style,
    profile: WidthProfile<'static>,
) {
    if title.is_empty() || rect.width < 4 {
        return;
    }
    let title = truncate(title, (rect.width - 4) as usize, profile);
    let content = format!(" {title} ");
    render_single_line(
        surface,
        Rect::new(add_coordinate(rect.x, 1), rect.y, rect.width - 2, 1),
        clip,
        &content,
        style,
        profile,
    );
}

fn write_border_cell(
    surface: &mut Surface,
    clip: Rect,
    x: i64,
    y: i64,
    text: &str,
    style: Style,
    profile: WidthProfile<'static>,
) {
    if contains_unit(clip, x, y, x + 1) {
        surface.write(
            clamp_i64_to_i32(x),
            clamp_i64_to_i32(y),
            text,
            style,
            profile,
        );
    }
}

fn profile_aware_border_glyphs(
    glyphs: BorderGlyphs,
    profile: WidthProfile<'static>,
) -> BorderGlyphs {
    if [
        glyphs.top_left,
        glyphs.horizontal,
        glyphs.top_right,
        glyphs.vertical,
        glyphs.bottom_left,
        glyphs.bottom_right,
    ]
    .into_iter()
    .all(|glyph| grapheme_width(glyph, profile) == 1)
    {
        glyphs
    } else {
        BorderGlyphs {
            top_left: "+",
            horizontal: "-",
            top_right: "+",
            vertical: "|",
            bottom_left: "+",
            bottom_right: "+",
        }
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
    profile: WidthProfile<'static>,
) -> Rect {
    let desired = child.measure(Constraints::bounded(rect.size()), profile);
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
    fn anchored_overlay_placement_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "layout/anchored-overlay.txt",
            "anchored-overlay-placement",
            &[
                "boundary",
                "anchor",
                "anchor-clip",
                "overlay",
                "side",
                "alignment",
                "gap",
                "fallback",
                "max-width",
                "max-height",
                "expected",
            ],
        ) else {
            return;
        };

        for record in records {
            let overlay = fixture_size(record.field("overlay"));
            let actual = resolve_anchored_overlay_rect(
                fixture_rect(record.field("anchor")),
                fixture_rect(record.field("anchor-clip")),
                fixture_rect(record.field("boundary")),
                &Node::<Message>::spacer(overlay.width, overlay.height),
                AnchoredOverlayOptions {
                    side: match record.field("side") {
                        "below" => AnchoredOverlaySide::Below,
                        "above" => AnchoredOverlaySide::Above,
                        value => panic!("case {} has invalid side {value}", record.id),
                    },
                    alignment: match record.field("alignment") {
                        "start" => HorizontalAlignment::Start,
                        "center" => HorizontalAlignment::Center,
                        "end" => HorizontalAlignment::End,
                        value => panic!("case {} has invalid alignment {value}", record.id),
                    },
                    gap: fixture_number(record.field("gap")),
                    fallback: match record.field("fallback") {
                        "flip" => AnchoredOverlayFallback::Flip,
                        "clip" => AnchoredOverlayFallback::Clip,
                        value => panic!("case {} has invalid fallback {value}", record.id),
                    },
                    maximum_width: fixture_number(record.field("max-width")),
                    maximum_height: fixture_number(record.field("max-height")),
                },
                WidthProfile::MODERN,
            );
            let expected = (record.field("expected") != "none")
                .then(|| fixture_rect(record.field("expected")));
            assert_eq!(actual, expected, "case {}", record.id);
        }
    }

    #[test]
    fn anchored_overlay_measurement_is_exactly_the_base_measurement() {
        let base = Node::<()>::spacer(2, 3);
        let overlay = Node::<()>::spacer(20, 30);
        let anchored = Node::anchored_overlay(base, "anchor", overlay);
        assert_eq!(
            anchored.measure(
                Constraints {
                    width: Limit::Unbounded,
                    height: Limit::Unbounded,
                },
                WidthProfile::MODERN,
            ),
            Size::new(2, 3)
        );
    }

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
    fn cursor_anchor_uses_no_layout_width_and_follows_focus() {
        let node = Node::<Message>::row([
            Node::text("A"),
            Node::cursor_anchor("editor"),
            Node::text("B"),
        ]);
        let mut surface = Surface::new(2, 1).unwrap();
        let mut interaction = InteractionState::new();
        interaction.focused = Some(NodeId::from("editor"));

        node.render_to(&mut surface, &interaction);

        assert_eq!(surface.cell(0, 0).unwrap().content(), "A");
        assert_eq!(surface.cell(1, 0).unwrap().content(), "B");
        assert_eq!(surface.cursor(), Some(Cursor::new(1, 0)));

        let mut unfocused = Surface::new(2, 1).unwrap();
        node.render_to(&mut unfocused, &InteractionState::new());
        assert_eq!(unfocused.cursor(), None);
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

    fn fixture_rect(value: &str) -> Rect {
        let mut parts = value.split(':');
        let x = parts.next().unwrap().parse().unwrap();
        let y = parts.next().unwrap().parse().unwrap();
        let width = parts.next().unwrap().parse().unwrap();
        let height = parts.next().unwrap().parse().unwrap();
        assert!(parts.next().is_none(), "invalid fixture Rect {value}");
        Rect::new(x, y, width, height)
    }

    fn fixture_size(value: &str) -> Size {
        let mut parts = value.split(':');
        let width = parts.next().unwrap().parse().unwrap();
        let height = parts.next().unwrap().parse().unwrap();
        assert!(parts.next().is_none(), "invalid fixture Size {value}");
        Size::new(width, height)
    }
}
