use nagi_tui::{Node, ResponsiveRowItem, ResponsiveRowOptions, ResponsiveRowPlacement};

/// Retention priority used by a [`StatusBarSlot`] under insufficient width
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum StatusBarPriority {
    /// Omit before normal-priority slots
    Low,
    /// Standard status information
    #[default]
    Normal,
    /// Retain before normal-priority slots
    High,
    /// Retain before every other category
    Critical,
}

impl StatusBarPriority {
    const fn value(self) -> u16 {
        match self {
            Self::Low => 0,
            Self::Normal => 100,
            Self::High => 200,
            Self::Critical => 300,
        }
    }
}

/// One arbitrary semantic Node displayed by a [`StatusBar`]
pub struct StatusBarSlot<Message> {
    node: Node<Message>,
    placement: ResponsiveRowPlacement,
    priority: StatusBarPriority,
}

impl<Message> StatusBarSlot<Message> {
    /// Creates a start-aligned slot with normal retention priority
    #[must_use]
    pub const fn new(node: Node<Message>) -> Self {
        Self {
            node,
            placement: ResponsiveRowPlacement::Start,
            priority: StatusBarPriority::Normal,
        }
    }

    /// Sets the start, center, or end region used by the slot
    #[must_use]
    pub const fn placement(mut self, placement: ResponsiveRowPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// Sets the slot retention priority
    #[must_use]
    pub const fn priority(mut self, priority: StatusBarPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Returns the configured placement
    #[must_use]
    pub const fn configured_placement(&self) -> ResponsiveRowPlacement {
        self.placement
    }

    /// Returns the configured retention priority
    #[must_use]
    pub const fn configured_priority(&self) -> StatusBarPriority {
        self.priority
    }
}

/// A one-row responsive container for application-defined status Nodes
///
/// StatusBar owns no status meaning, state, timer, task, or I/O. Hidden slots
/// follow the Core ResponsiveRow semantic omission contract
pub struct StatusBar<Message> {
    slots: Vec<StatusBarSlot<Message>>,
    gap: u32,
}

impl<Message> StatusBar<Message> {
    /// Creates a status bar with one empty Cell between retained slots
    #[must_use]
    pub fn new(slots: impl IntoIterator<Item = StatusBarSlot<Message>>) -> Self {
        Self {
            slots: slots.into_iter().collect(),
            gap: 1,
        }
    }

    /// Sets the empty Cells required between retained slots
    #[must_use]
    pub const fn gap(mut self, gap: u32) -> Self {
        self.gap = gap;
        self
    }

    /// Builds the one-row public semantic node
    #[must_use]
    pub fn into_node(self) -> Node<Message> {
        Node::responsive_row(
            self.slots.into_iter().map(|slot| {
                ResponsiveRowItem::new(slot.node)
                    .placement(slot.placement)
                    .priority(slot.priority.value())
            }),
            ResponsiveRowOptions {
                gap: self.gap,
                height: 1,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use nagi_tui::{App, Effect, Runtime, RuntimeConfig, Size, ViewContext, VirtualClock};

    use super::*;

    struct StatusApp;

    impl App for StatusApp {
        type Message = ();

        fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: ViewContext) -> Node<Self::Message> {
            StatusBar::new([
                StatusBarSlot::new(Node::text("normal")),
                StatusBarSlot::new(Node::text("!"))
                    .placement(ResponsiveRowPlacement::End)
                    .priority(StatusBarPriority::Critical),
            ])
            .into_node()
        }
    }

    #[test]
    fn critical_slot_survives_a_narrow_bar() {
        let mut runtime = Runtime::with_clock(
            StatusApp,
            RuntimeConfig::new(Size::new(3, 1)),
            VirtualClock::new(),
        )
        .unwrap();
        let frame = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(frame.surface().cell(0, 0).unwrap().content(), " ");
        assert_eq!(frame.surface().cell(2, 0).unwrap().content(), "!");
    }

    struct OneRowApp;

    impl App for OneRowApp {
        type Message = ();

        fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: ViewContext) -> Node<Self::Message> {
            StatusBar::new([StatusBarSlot::new(Node::text("first\nsecond"))]).into_node()
        }
    }

    #[test]
    fn status_bar_is_one_row_even_as_the_root() {
        let mut runtime = Runtime::with_clock(
            OneRowApp,
            RuntimeConfig::new(Size::new(8, 3)),
            VirtualClock::new(),
        )
        .unwrap();
        let frame = runtime.render_if_dirty().unwrap().unwrap();

        assert_eq!(frame.surface().cell(0, 0).unwrap().content(), "f");
        assert_eq!(frame.surface().cell(0, 1).unwrap().content(), " ");
    }

    struct FixtureApp {
        labels: Vec<String>,
        placements: Vec<ResponsiveRowPlacement>,
        priorities: Vec<StatusBarPriority>,
        gap: u32,
    }

    impl App for FixtureApp {
        type Message = ();

        fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
            Effect::none()
        }

        fn view(&self, _context: ViewContext) -> Node<Self::Message> {
            StatusBar::new(
                self.labels
                    .iter()
                    .zip(&self.placements)
                    .zip(&self.priorities)
                    .map(|((label, placement), priority)| {
                        StatusBarSlot::new(Node::text(label.as_str()))
                            .placement(*placement)
                            .priority(*priority)
                    }),
            )
            .gap(self.gap)
            .into_node()
        }
    }

    #[test]
    fn status_bar_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/status-bar.txt",
            "widget-status-bar",
            &[
                "width",
                "gap",
                "labels",
                "placements",
                "priorities",
                "expected",
            ],
        ) else {
            return;
        };
        for record in records {
            let labels = list(record.field("labels"), str::to_owned);
            let placements = list(record.field("placements"), |value| match value {
                "start" => ResponsiveRowPlacement::Start,
                "center" => ResponsiveRowPlacement::Center,
                "end" => ResponsiveRowPlacement::End,
                _ => panic!("invalid placement {value}"),
            });
            let priorities = list(record.field("priorities"), |value| match value {
                "low" => StatusBarPriority::Low,
                "normal" => StatusBarPriority::Normal,
                "high" => StatusBarPriority::High,
                "critical" => StatusBarPriority::Critical,
                _ => panic!("invalid priority {value}"),
            });
            let width = record.field("width").parse().expect("width");
            let mut runtime = Runtime::with_clock(
                FixtureApp {
                    labels,
                    placements,
                    priorities,
                    gap: record.field("gap").parse().expect("gap"),
                },
                RuntimeConfig::new(Size::new(width, 1)),
                VirtualClock::new(),
            )
            .unwrap();
            let frame = runtime.render_if_dirty().unwrap().unwrap();
            let mut actual = String::new();
            for x in 0..width {
                actual.push_str(
                    frame
                        .surface()
                        .cell(i32::try_from(x).expect("fixture x"), 0)
                        .unwrap()
                        .content(),
                );
            }
            assert_eq!(actual, record.text("expected"), "case {}", record.id);
        }
    }

    fn list<T>(value: &str, parse: impl Fn(&str) -> T) -> Vec<T> {
        value.split(',').map(parse).collect()
    }
}
