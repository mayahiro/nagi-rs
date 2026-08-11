use nagi_tui::{
    HorizontalAlignment, Length, Node, NodeId, ScrollState, VerticalAlignment, VirtualFlowOptions,
    VirtualFlowSource,
};

/// A variable-height feed with application-owned status slots
///
/// `VirtualFeed` follows the end by default and delegates item identity,
/// invalidation, measurement estimates, unread state, and paging decisions to
/// the application. Only the Core [`nagi_tui::VirtualFlowState`] is retained by
/// the runtime
pub struct VirtualFeed<Message> {
    id: NodeId,
    source: VirtualFlowSource<Message>,
    options: VirtualFlowOptions<Message>,
    empty: Option<Node<Message>>,
    loading_before: Option<Node<Message>>,
    loading_after: Option<Node<Message>>,
    unread_indicator: Option<Node<Message>>,
}

impl<Message> VirtualFeed<Message> {
    /// Creates a feed that follows its end with one Cell of overscan
    #[must_use]
    pub fn new(id: impl Into<NodeId>, source: VirtualFlowSource<Message>) -> Self {
        let options = VirtualFlowOptions {
            stick_to_end: true,
            ..VirtualFlowOptions::default()
        };
        Self {
            id: id.into(),
            source,
            options,
            empty: None,
            loading_before: None,
            loading_after: None,
            unread_indicator: None,
        }
    }

    /// Sets whether the feed follows growth while its viewport is at the end
    #[must_use]
    pub const fn follow_end(mut self, follow: bool) -> Self {
        self.options.stick_to_end = follow;
        self
    }

    /// Sets extra terminal Cells built before and after the visible range
    #[must_use]
    pub const fn overscan(mut self, cells: u32) -> Self {
        self.options.overscan = cells;
        self
    }

    /// Sets whether focus movement reveals a built focused descendant
    #[must_use]
    pub const fn ensure_focused_visible(mut self, ensure: bool) -> Self {
        self.options.ensure_focused_visible = ensure;
        self
    }

    /// Sets an application message created after user scrolling changes state
    #[must_use]
    pub fn on_scroll(mut self, handler: impl Fn(ScrollState) -> Message + 'static) -> Self {
        self.options.on_scroll = Some(Box::new(handler));
        self
    }

    /// Sets the centered overlay shown when the source has no items
    #[must_use]
    pub fn empty(mut self, node: Node<Message>) -> Self {
        self.empty = Some(node);
        self
    }

    /// Sets a status slot pinned before the scrollable body
    #[must_use]
    pub fn loading_before(mut self, node: Node<Message>) -> Self {
        self.loading_before = Some(node);
        self
    }

    /// Sets a status slot pinned after the scrollable body
    #[must_use]
    pub fn loading_after(mut self, node: Node<Message>) -> Self {
        self.loading_after = Some(node);
        self
    }

    /// Sets a bottom-end overlay whose visibility is application-owned
    #[must_use]
    pub fn unread_indicator(mut self, node: Node<Message>) -> Self {
        self.unread_indicator = Some(node);
        self
    }

    /// Builds the public semantic node for this feed
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        let empty = self.source.items().is_empty();
        let flow = Node::virtual_flow_with_options(self.id, self.source, self.options)
            .with_length(Length::Flex(1));
        let mut layers = vec![flow];
        if empty {
            if let Some(node) = self.empty {
                layers.push(Node::align(
                    node,
                    HorizontalAlignment::Center,
                    VerticalAlignment::Center,
                ));
            }
        }
        if let Some(node) = self.unread_indicator {
            layers.push(Node::align(
                node,
                HorizontalAlignment::End,
                VerticalAlignment::End,
            ));
        }
        let body = Node::stack(layers).with_length(Length::Flex(1));
        let mut children = Vec::with_capacity(3);
        if let Some(node) = self.loading_before {
            children.push(node);
        }
        children.push(body);
        if let Some(node) = self.loading_after {
            children.push(node);
        }
        Node::column(children)
    }
}

#[cfg(test)]
mod tests {
    use nagi_tui::{
        App, Effect, NodeId, Runtime, RuntimeConfig, Size, VirtualClock, VirtualFlowItem,
        VirtualFlowItems,
    };

    use super::*;

    struct EmptyFeedApp;

    impl App for EmptyFeedApp {
        type Message = ();

        fn update(&mut self, (): ()) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
            VirtualFeed::new(
                "feed",
                VirtualFlowSource::new(VirtualFlowItems::default(), |_| Node::spacer(0, 1)),
            )
            .empty(Node::text("empty"))
            .loading_before(Node::text("before"))
            .loading_after(Node::text("after"))
            .unread_indicator(Node::text("unread"))
            .into_node()
        }
    }

    #[test]
    fn slots_are_pinned_and_empty_is_centered() {
        let mut runtime = Runtime::with_clock(
            EmptyFeedApp,
            RuntimeConfig::new(Size::new(10, 7)),
            VirtualClock::new(),
        )
        .unwrap();

        let frame = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(frame.surface().cell(0, 0).unwrap().content(), "b");
        assert_eq!(frame.surface().cell(2, 3).unwrap().content(), "e");
        assert_eq!(frame.surface().cell(4, 5).unwrap().content(), "u");
        assert_eq!(frame.surface().cell(0, 6).unwrap().content(), "a");
        assert_eq!(
            runtime
                .interaction()
                .virtual_flow_state(&NodeId::from("feed"))
                .unwrap()
                .item_count(),
            0
        );
    }

    struct FollowingFeedApp;

    impl App for FollowingFeedApp {
        type Message = ();

        fn update(&mut self, (): ()) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
            let items = VirtualFlowItems::new([
                VirtualFlowItem::new("a"),
                VirtualFlowItem::new("b"),
                VirtualFlowItem::new("c"),
            ])
            .unwrap();
            let source = VirtualFlowSource::new(items, |context| {
                Node::column([
                    Node::text(context.key().as_str()),
                    Node::text(context.key().as_str()),
                ])
            })
            .estimated_height(|_| 2);
            VirtualFeed::new("feed", source).into_node()
        }
    }

    #[test]
    fn feed_follows_the_end_by_default() {
        let mut runtime = Runtime::with_clock(
            FollowingFeedApp,
            RuntimeConfig::new(Size::new(4, 3)),
            VirtualClock::new(),
        )
        .unwrap();

        let frame = runtime.render_if_dirty().unwrap().unwrap();
        let state = runtime
            .interaction()
            .virtual_flow_state(&NodeId::from("feed"))
            .unwrap();

        assert_eq!(state.scroll().offset.y, 3);
        assert!(state.scroll().at_end);
        assert_eq!(frame.surface().cell(0, 0).unwrap().content(), "b");
        assert_eq!(frame.surface().cell(0, 1).unwrap().content(), "c");
    }
}
