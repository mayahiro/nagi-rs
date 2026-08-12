use std::sync::Arc;

use nagi_tui::{HorizontalAlignment, Length, Node, NodeId, Style, VerticalAlignment};

use crate::{Button, ButtonStyle};

/// Visual tone selected by an application for one [`Toast`]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ToastTone {
    /// Status without additional severity
    #[default]
    Neutral,
    /// Informational status
    Info,
    /// Successful completion
    Success,
    /// Warning status
    Warning,
    /// Error status
    Error,
}

/// Viewport corner used by a [`ToastRegion`]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ToastPlacement {
    /// Place the visible group at the top-right corner
    #[default]
    TopEnd,
    /// Place the visible group at the top-left corner
    TopStart,
    /// Place the visible group at the bottom-right corner
    BottomEnd,
    /// Place the visible group at the bottom-left corner
    BottomStart,
}

/// Replaceable visual styles used by a [`ToastRegion`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToastStyle {
    /// Border style for neutral status
    pub neutral: Style,
    /// Border style for informational status
    pub info: Style,
    /// Border style for successful completion
    pub success: Style,
    /// Border style for warnings
    pub warning: Style,
    /// Border style for errors
    pub error: Style,
    /// Style used by an optional focusable dismissal button
    pub dismiss: ButtonStyle,
}

impl Default for ToastStyle {
    fn default() -> Self {
        Self {
            neutral: Style::default(),
            info: Style {
                bold: true,
                ..Style::default()
            },
            success: Style {
                underline: true,
                ..Style::default()
            },
            warning: Style {
                bold: true,
                underline: true,
                ..Style::default()
            },
            error: Style {
                reverse: true,
                ..Style::default()
            },
            dismiss: ButtonStyle::default(),
        }
    }
}

struct ToastDismiss<Message> {
    id: NodeId,
    handler: Arc<dyn Fn() -> Message>,
}

/// One controlled application notification with a lazily constructed body
///
/// Toast owns no timeout or lifecycle state. Applications remove records from
/// their controlled collection and may use an After Effect with the Toast ID
/// or an application generation to ignore stale completion messages
pub struct Toast<Message> {
    id: NodeId,
    tone: ToastTone,
    body: Option<Box<dyn FnOnce() -> Node<Message>>>,
    dismiss: Option<ToastDismiss<Message>>,
}

impl<Message: 'static> Toast<Message> {
    /// Creates a neutral Toast with one lazy body builder
    #[must_use]
    pub fn new(id: impl Into<NodeId>, body: impl FnOnce() -> Node<Message> + 'static) -> Self {
        Self {
            id: id.into(),
            tone: ToastTone::Neutral,
            body: Some(Box::new(body)),
            dismiss: None,
        }
    }

    /// Sets the application-defined visual tone
    #[must_use]
    pub const fn tone(mut self, tone: ToastTone) -> Self {
        self.tone = tone;
        self
    }

    /// Adds a focusable dismissal button with an explicit stable Node ID
    #[must_use]
    pub fn on_dismiss(
        mut self,
        button_id: impl Into<NodeId>,
        handler: impl Fn() -> Message + 'static,
    ) -> Self {
        self.dismiss = Some(ToastDismiss {
            id: button_id.into(),
            handler: Arc::new(handler),
        });
        self
    }

    /// Returns the stable Toast root ID
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns the configured visual tone
    #[must_use]
    pub const fn configured_tone(&self) -> ToastTone {
        self.tone
    }

    fn into_node(mut self, style: ToastStyle) -> Node<Message> {
        let body = self
            .body
            .take()
            .map_or_else(|| Node::column([]), |body| body());
        let content = if let Some(dismiss) = self.dismiss {
            let handler = dismiss.handler;
            Node::row([
                body.with_length(Length::Flex(1)),
                Node::gap(1),
                Button::new(dismiss.id, "x", move || handler())
                    .style(style.dismiss)
                    .into_node(),
            ])
        } else {
            body
        };
        Node::border(content, style.for_tone(self.tone)).with_id(self.id)
    }
}

impl ToastStyle {
    const fn for_tone(self, tone: ToastTone) -> Style {
        match tone {
            ToastTone::Neutral => self.neutral,
            ToastTone::Info => self.info,
            ToastTone::Success => self.success,
            ToastTone::Warning => self.warning,
            ToastTone::Error => self.error,
        }
    }
}

/// A corner-aligned overlay for a controlled sequence of [`Toast`] values
///
/// Input order is oldest to newest. Only the newest `visible_limit` bodies are
/// constructed, and retained values preserve source order from top to bottom
pub struct ToastRegion<Message> {
    base: Node<Message>,
    toasts: Vec<Toast<Message>>,
    placement: ToastPlacement,
    visible_limit: usize,
    gap: u32,
    style: ToastStyle,
}

impl<Message: 'static> ToastRegion<Message> {
    /// Creates a top-end region showing at most three Toasts
    #[must_use]
    pub fn new(base: Node<Message>, toasts: impl IntoIterator<Item = Toast<Message>>) -> Self {
        Self {
            base,
            toasts: toasts.into_iter().collect(),
            placement: ToastPlacement::TopEnd,
            visible_limit: 3,
            gap: 1,
            style: ToastStyle::default(),
        }
    }

    /// Sets the viewport corner used by the visible group
    #[must_use]
    pub const fn placement(mut self, placement: ToastPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// Sets the greatest number of Toast bodies constructed and displayed
    #[must_use]
    pub const fn visible_limit(mut self, visible_limit: usize) -> Self {
        self.visible_limit = visible_limit;
        self
    }

    /// Sets the empty rows inserted between visible Toasts
    #[must_use]
    pub const fn gap(mut self, gap: u32) -> Self {
        self.gap = gap;
        self
    }

    /// Replaces all visual styles used by the region
    #[must_use]
    pub const fn style(mut self, style: ToastStyle) -> Self {
        self.style = style;
        self
    }

    /// Builds the public semantic overlay without constructing omitted bodies
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let visible = self.visible_limit.min(self.toasts.len());
        if visible == 0 {
            return self.base;
        }
        let first = self.toasts.len() - visible;
        let mut children = Vec::with_capacity(visible.saturating_mul(2).saturating_sub(1));
        for (index, toast) in self.toasts.into_iter().skip(first).enumerate() {
            if index != 0 {
                children.push(Node::gap(self.gap));
            }
            children.push(toast.into_node(self.style));
        }
        let (horizontal, vertical) = match self.placement {
            ToastPlacement::TopEnd => (HorizontalAlignment::End, VerticalAlignment::Start),
            ToastPlacement::TopStart => (HorizontalAlignment::Start, VerticalAlignment::Start),
            ToastPlacement::BottomEnd => (HorizontalAlignment::End, VerticalAlignment::End),
            ToastPlacement::BottomStart => (HorizontalAlignment::Start, VerticalAlignment::End),
        };
        let layer = Node::align(Node::column(children), horizontal, vertical);
        Node::overlay(self.base, layer)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    use nagi_tui::{App, Effect, Runtime, RuntimeConfig, Size, ViewContext, VirtualClock};

    use super::*;

    #[test]
    fn visible_body_selection_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/toast.txt",
            "widget-toast",
            &["count", "limit", "built"],
        ) else {
            return;
        };
        for record in records {
            let builds = Rc::new(RefCell::new(Vec::new()));
            let count: usize = record.field("count").parse().expect("count");
            let toasts = (0..count).map(|index| {
                let observed = Rc::clone(&builds);
                Toast::<()>::new(format!("toast-{index}"), move || {
                    observed.borrow_mut().push(index);
                    Node::text(index.to_string())
                })
            });
            let _node = ToastRegion::new(Node::text("base"), toasts)
                .visible_limit(record.field("limit").parse().expect("limit"))
                .into_node();
            let expected = if record.field("built") == "-" {
                Vec::new()
            } else {
                record
                    .field("built")
                    .split(',')
                    .map(|value| value.parse().expect("built index"))
                    .collect()
            };
            assert_eq!(*builds.borrow(), expected, "case {}", record.id);
        }
    }

    #[test]
    fn only_newest_visible_bodies_are_built_in_source_order() {
        let builds = Rc::new(RefCell::new(Vec::new()));
        let toasts = (0..5).map(|index| {
            let observed = Rc::clone(&builds);
            Toast::<()>::new(format!("toast-{index}"), move || {
                observed.borrow_mut().push(index);
                Node::text(index.to_string())
            })
        });

        let _node = ToastRegion::new(Node::text("base"), toasts)
            .visible_limit(2)
            .into_node();

        assert_eq!(*builds.borrow(), [3, 4]);
    }

    #[test]
    fn zero_limit_returns_base_without_building_bodies() {
        let builds = Rc::new(RefCell::new(0));
        let observed = Rc::clone(&builds);
        let toast = Toast::<()>::new("toast", move || {
            *observed.borrow_mut() += 1;
            Node::text("body")
        });

        let _node = ToastRegion::new(Node::text("base"), [toast])
            .visible_limit(0)
            .into_node();

        assert_eq!(*builds.borrow(), 0);
    }

    enum ExpiryMessage {
        Show(u64),
        Expire(u64),
    }

    #[derive(Default)]
    struct ExpiryApp {
        current: Option<u64>,
    }

    impl App for ExpiryApp {
        type Message = ExpiryMessage;

        fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
            match message {
                ExpiryMessage::Show(generation) => {
                    self.current = Some(generation);
                    Effect::after(Duration::from_millis(10), ExpiryMessage::Expire(generation))
                }
                ExpiryMessage::Expire(generation) => {
                    if self.current == Some(generation) {
                        self.current = None;
                    }
                    Effect::none()
                }
            }
        }

        fn view(&self, _context: ViewContext) -> Node<Self::Message> {
            let toast = self
                .current
                .map(|generation| Toast::new("toast", move || Node::text(generation.to_string())));
            ToastRegion::new(Node::text("base"), toast).into_node()
        }
    }

    #[test]
    fn application_generation_ignores_a_stale_virtual_timeout() {
        let clock = VirtualClock::new();
        let mut runtime = Runtime::with_clock(
            ExpiryApp::default(),
            RuntimeConfig::new(Size::new(8, 3)),
            clock.clone(),
        )
        .unwrap();

        runtime.enqueue(ExpiryMessage::Show(1)).unwrap();
        runtime.process_pending().unwrap();
        clock.advance(Duration::from_millis(5));
        runtime.enqueue(ExpiryMessage::Show(2)).unwrap();
        runtime.process_pending().unwrap();
        clock.advance(Duration::from_millis(5));
        runtime.process_pending().unwrap();

        assert_eq!(runtime.app().current, Some(2));

        clock.advance(Duration::from_millis(5));
        runtime.process_pending().unwrap();

        assert_eq!(runtime.app().current, None);
    }

    struct PlacementApp;

    impl App for PlacementApp {
        type Message = ();

        fn update(&mut self, (): ()) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: ViewContext) -> Node<Self::Message> {
            ToastRegion::new(
                Node::text("base"),
                [Toast::new("toast", || Node::text("X"))],
            )
            .placement(ToastPlacement::BottomEnd)
            .into_node()
        }
    }

    #[test]
    fn bottom_end_placement_uses_the_base_rectangle() {
        let mut runtime = Runtime::with_clock(
            PlacementApp,
            RuntimeConfig::new(Size::new(8, 5)),
            VirtualClock::new(),
        )
        .unwrap();
        let frame = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(frame.surface().cell(5, 2).unwrap().content(), "┌");
        assert_eq!(frame.surface().cell(6, 3).unwrap().content(), "X");
        assert_eq!(frame.surface().cell(7, 4).unwrap().content(), "┘");
    }
}
