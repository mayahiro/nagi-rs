//! Shared runtime vertical-slice conformance fixtures

mod support;

use nagi_tui::{
    Action, ActionAvailability, ActionDescriptor, ActionId, App, BindingConflictKind, Capabilities,
    Effect, Event, EventResult, FOCUS_NEXT_ACTION_ID, FOCUS_PREVIOUS_ACTION_ID, Insets, KeyAction,
    KeyBinding, KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyScopePropagation, KeyStroke,
    Length, ModalFocusOptions, ModalInitialFocus, ModalReturnFocus, Modifiers, MouseButton,
    MouseEvent, MouseKind, Node, NodeId, Runtime, RuntimeError, RuntimeEventError,
    SCROLL_PAGE_DOWN_ACTION_ID, ScrollAxis, ScrollOffset, ScrollViewportOptions, Size, Style,
    Subscription, VirtualClock, encode,
};

#[derive(Default)]
struct Echo {
    text: String,
}

impl App for Echo {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.text.push_str(&message);
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::border(Node::text(&self.text), Style::default())
    }
}

#[derive(Default)]
struct SchedulingApp {
    messages: Vec<String>,
}

impl App for SchedulingApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::text(self.messages.join(","))
    }
}

#[test]
fn input_update_surface_and_vt_output_match_shared_fixtures() {
    let Some(records) = support::load(
        "runtime/roundtrip.txt",
        "runtime-roundtrip",
        &["width", "height", "input", "expected"],
    ) else {
        return;
    };

    for record in records {
        let width = number(record.field("width"));
        let height = number(record.field("height"));
        let input = record.decoded("input");
        let expected = record.text("expected");
        let mut runtime = Runtime::with_clock(
            Echo::default(),
            nagi_tui::RuntimeConfig::new(Size::new(width, height)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap().unwrap();
        let mut decoder = nagi_tui::TimedInputDecoder::new(
            VirtualClock::new(),
            std::time::Duration::from_millis(25),
        );

        for event in decoder.feed(&input) {
            if let Event::Text(text) = event {
                runtime.enqueue(text).unwrap();
            }
        }
        let frame = runtime.step().unwrap().unwrap();

        assert_eq!(frame.surface().snapshot(), expected, "case {}", record.id);
        let output = encode(frame.operations(), Capabilities::BASELINE);
        assert!(
            output.windows(input.len()).any(|window| window == input),
            "case {} did not reach VT output",
            record.id
        );
    }
}

#[test]
fn bounded_scheduling_cycles_match_shared_fixtures() {
    let Some(records) = support::load(
        "runtime/scheduling.txt",
        "runtime-scheduling",
        &[
            "maximum",
            "messages",
            "expected-cycle",
            "expected-remaining",
            "expected-final",
        ],
    ) else {
        return;
    };

    for record in records {
        let mut config = nagi_tui::RuntimeConfig::new(Size::new(8, 1));
        config.max_updates_per_cycle = number(record.field("maximum")) as usize;
        let mut runtime =
            Runtime::with_clock(SchedulingApp::default(), config, VirtualClock::new()).unwrap();
        for message in fixture_list(record.field("messages")) {
            runtime.enqueue(message).unwrap();
        }

        runtime.process_pending().unwrap();
        assert_eq!(
            runtime.app().messages,
            fixture_list(record.field("expected-cycle")),
            "case {} cycle",
            record.id
        );
        assert_eq!(
            runtime.queued_messages(),
            number(record.field("expected-remaining")) as usize,
            "case {} remaining",
            record.id
        );

        runtime.process_queued().unwrap();
        assert_eq!(
            runtime.app().messages,
            fixture_list(record.field("expected-final")),
            "case {} final",
            record.id
        );
    }
}

#[derive(Default)]
struct TextInputApp {
    value: String,
}

impl App for TextInputApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.value = message;
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::text_input("input", &self.value, |value| value)
    }
}

#[test]
fn text_input_cursor_snapshot_matches_shared_fixture() {
    let Some(records) = support::load(
        "interaction/text-input-runtime.txt",
        "text-input-runtime",
        &["width", "height", "input", "expected"],
    ) else {
        return;
    };
    for record in records {
        let mut runtime = Runtime::with_clock(
            TextInputApp::default(),
            nagi_tui::RuntimeConfig::new(Size::new(
                number(record.field("width")),
                number(record.field("height")),
            )),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        runtime
            .request_focus(&nagi_tui::NodeId::from("input"))
            .unwrap();
        let mut decoder = nagi_tui::TimedInputDecoder::new(
            VirtualClock::new(),
            std::time::Duration::from_millis(25),
        );
        for event in decoder.feed(&record.decoded("input")) {
            runtime.dispatch_event(&event).unwrap();
        }

        let frame = runtime.step().unwrap().unwrap();

        assert_eq!(
            frame.surface().snapshot(),
            record.text("expected"),
            "case {}",
            record.id
        );
    }
}

struct ModalFocusFixtureApp {
    view: String,
    first: ModalFocusOptions,
    second: ModalFocusOptions,
}

impl App for ModalFocusFixtureApp {
    type Message = String;

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let background = modal_focus_background();
        match self.view.as_str() {
            "base" => background,
            "a" => Node::stack([background, modal_focus_node("a", self.first.clone(), false)]),
            "a-empty" => Node::stack([background, modal_focus_node("a", self.first.clone(), true)]),
            "b" => Node::stack([
                background,
                modal_focus_node("b", self.second.clone(), false),
            ]),
            "a+b" => Node::stack([
                background,
                modal_focus_node("a", self.first.clone(), false),
                modal_focus_node("b", self.second.clone(), false),
            ]),
            "a>b" => Node::stack([
                background,
                Node::modal_with_focus(
                    "modal-a",
                    Node::column([
                        modal_focus_content("a"),
                        modal_focus_node("b", self.second.clone(), false),
                    ]),
                    self.first.clone(),
                ),
            ]),
            view => panic!("unknown modal focus fixture view {view}"),
        }
    }
}

#[test]
fn modal_focus_lifecycle_matches_shared_fixtures() {
    let Some(records) = support::load(
        "interaction/modal-focus-lifecycle.txt",
        "modal-focus-lifecycle",
        &[
            "views",
            "focus",
            "a-initial",
            "a-return",
            "b-initial",
            "b-return",
            "expected",
        ],
    ) else {
        return;
    };

    for record in records {
        let views = fixture_list(record.field("views"));
        let expected = fixture_list(record.field("expected"));
        assert_eq!(views.len(), expected.len(), "case {}", record.id);
        let mut runtime = Runtime::with_clock(
            ModalFocusFixtureApp {
                view: views[0].clone(),
                first: modal_focus_options(record.field("a-initial"), record.field("a-return")),
                second: modal_focus_options(record.field("b-initial"), record.field("b-return")),
            },
            nagi_tui::RuntimeConfig::new(Size::new(30, 8)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        if record.field("focus") != "none" {
            assert!(
                runtime
                    .request_focus(&NodeId::from(record.field("focus")))
                    .unwrap(),
                "case {} initial focus",
                record.id
            );
        }
        assert_modal_fixture_focus(&runtime, &expected[0], &record.id, 0);

        for (step, view) in views.iter().enumerate().skip(1) {
            runtime.app_mut().view.clone_from(view);
            runtime.request_frame();
            runtime.render_if_dirty().unwrap();
            assert_modal_fixture_focus(&runtime, &expected[step], &record.id, step);
        }
    }
}

fn modal_focus_background() -> Node<String> {
    Node::column([
        Node::text("background first").focusable("background-first"),
        Node::text("opener").focusable("opener"),
        Node::text("background target").focusable("background-target"),
    ])
}

fn modal_focus_content(prefix: &str) -> Node<String> {
    Node::column([
        Node::text(format!("{prefix} first")).focusable(format!("{prefix}-first")),
        Node::text(format!("{prefix} target")).focusable(format!("{prefix}-target")),
    ])
}

fn modal_focus_node(prefix: &str, focus: ModalFocusOptions, empty: bool) -> Node<String> {
    let child = if empty {
        Node::text(format!("{prefix} empty"))
    } else {
        modal_focus_content(prefix)
    };
    Node::modal_with_focus(format!("modal-{prefix}"), child, focus)
}

fn modal_focus_options(initial: &str, return_focus: &str) -> ModalFocusOptions {
    ModalFocusOptions {
        initial: match initial {
            "first" => ModalInitialFocus::First,
            "none" => ModalInitialFocus::None,
            value => ModalInitialFocus::Target(NodeId::from(
                value
                    .strip_prefix("target/")
                    .unwrap_or_else(|| panic!("invalid modal initial focus {value}")),
            )),
        },
        return_focus: match return_focus {
            "previous" => ModalReturnFocus::Previous,
            "none" => ModalReturnFocus::None,
            value => ModalReturnFocus::Target(NodeId::from(
                value
                    .strip_prefix("target/")
                    .unwrap_or_else(|| panic!("invalid modal return focus {value}")),
            )),
        },
    }
}

fn assert_modal_fixture_focus(
    runtime: &Runtime<ModalFocusFixtureApp, VirtualClock>,
    expected: &str,
    case: &str,
    step: usize,
) {
    let actual = runtime.interaction().focused().map(NodeId::as_str);
    let expected = (expected != "none").then_some(expected);
    assert_eq!(actual, expected, "case {case} step {step}");
}

struct KeyRoutingApp {
    scenario: String,
    updates: Vec<String>,
}

impl App for KeyRoutingApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.updates.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        match self.scenario.as_str() {
            "child-precedence" => {
                let child = Node::text("child")
                    .focusable("child")
                    .on_event("child", |_| {
                        EventResult::message("unexpected-raw".to_owned())
                    })
                    .on_actions("child", [message_action("app.child", 'x', "child-action")]);
                Node::padding(child, Insets::all(0))
                    .on_event("root", |_| {
                        EventResult::message("unexpected-root".to_owned())
                    })
                    .on_actions("root", [message_action("app.root", 'x', "root-action")])
            }
            "ignored-action-routing" => {
                let child = Node::text("child")
                    .focusable("child")
                    .on_event("child", |_| {
                        EventResult::ignored().emit("child-raw".to_owned())
                    })
                    .on_actions("child", [ignored_action("app.child", 'x', "child-action")]);
                Node::padding(child, Insets::all(0))
                    .on_event("root", |_| {
                        EventResult::message("unexpected-root".to_owned())
                    })
                    .on_actions("root", [message_action("app.root", 'x', "root-action")])
            }
            "stop-action-propagation" => {
                let outer_map = KeyMap::new()
                    .rebind(ActionId::from("app.child"), [binding('y')])
                    .unwrap();
                let child = Node::text("child").focusable("child").on_actions(
                    "child",
                    [message_action("app.child", 'x', "unexpected-child-action")],
                );
                let scope = Node::padding(child, Insets::all(0))
                    .with_key_scope(
                        KeyScope::new("scope", KeyMap::new())
                            .with_propagation(KeyScopePropagation::StopAtScope),
                    )
                    .on_event("scope", |_| {
                        EventResult::ignored().emit("scope-raw".to_owned())
                    });
                Node::padding(scope, Insets::all(0))
                    .with_key_scope(KeyScope::new("root", outer_map))
                    .on_event("root", |_| EventResult::message("root-raw".to_owned()))
                    .on_actions(
                        "root",
                        [message_action("app.root", 'x', "unexpected-action")],
                    )
            }
            "scope-rebind" => {
                let map = KeyMap::new()
                    .rebind(ActionId::from("app.child"), [binding('y')])
                    .unwrap();
                let child = Node::text("child").focusable("child").on_actions(
                    "child",
                    [routed_action("app.child", 'x', 'y', "child-action")],
                );
                Node::padding(child, Insets::all(0)).with_key_scope(KeyScope::new("scope", map))
            }
            "disabled-consume" => {
                let action = Action::new(
                    descriptor("app.child", 'x')
                        .with_availability(ActionAvailability::DisabledConsume),
                    |_| EventResult::message("unexpected-action".to_owned()),
                );
                Node::text("child")
                    .focusable("child")
                    .on_event("child", |_| {
                        EventResult::message("unexpected-raw".to_owned())
                    })
                    .on_actions("child", [action])
            }
            "paste-bypasses-actions" => Node::text("child")
                .focusable("child")
                .on_event("child", |_| EventResult::message("child-raw".to_owned()))
                .on_actions(
                    "child",
                    [message_action("app.child", 'x', "unexpected-action")],
                ),
            "text-input-local-action" => {
                Node::text_input("input", "", |_| "input-change".to_owned())
                    .on_actions("input", [message_action("app.input", 'x', "input-action")])
            }
            "text-input-core-before-ancestor" => Node::padding(
                Node::text_input("input", "", |_| "input-change".to_owned()),
                Insets::all(0),
            )
            .on_actions(
                "root",
                [message_action("app.root", 'x', "unexpected-action")],
            ),
            scenario => panic!("unknown key routing scenario {scenario}"),
        }
    }
}

#[test]
fn scoped_key_routing_matches_shared_fixtures() {
    let Some(records) = support::load(
        "interaction/key-routing-runtime.txt",
        "key-routing-runtime",
        &["event", "expected", "consumed", "groups"],
    ) else {
        return;
    };
    for record in records {
        let scenario = record.id.clone();
        let mut runtime = Runtime::with_clock(
            KeyRoutingApp {
                scenario,
                updates: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(20, 3)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        let target = if record.id.starts_with("text-input") {
            "input"
        } else {
            "child"
        };
        assert!(
            runtime.request_focus(&target.into()).unwrap(),
            "case {}",
            record.id
        );

        let resolved_groups = runtime.active_action_groups().unwrap();
        let groups: Vec<_> = resolved_groups
            .iter()
            .map(|group| group.owner().as_str().to_owned())
            .collect();
        assert_eq!(
            groups,
            fixture_list(record.field("groups")),
            "case {}",
            record.id
        );
        if record.id == "stop-action-propagation" {
            assert_eq!(
                resolved_groups[0]
                    .scope_path()
                    .iter()
                    .map(NodeId::as_str)
                    .collect::<Vec<_>>(),
                ["root", "scope"]
            );
            assert_eq!(resolved_groups[0].actions()[0].bindings(), [binding('y')]);
        }

        let dispatch = runtime
            .dispatch_event(&key_routing_event(record.field("event")))
            .unwrap();
        runtime.process_pending().unwrap();
        assert_eq!(
            dispatch.consumed(),
            record.field("consumed") == "true",
            "case {}",
            record.id
        );
        assert_eq!(
            runtime.app().updates,
            fixture_list(record.field("expected")),
            "case {}",
            record.id
        );
    }
}

struct CoreNavigationApp {
    scenario: String,
    updates: Vec<String>,
}

impl App for CoreNavigationApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.updates.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        if self.scenario.starts_with("focus-") {
            focus_navigation_view(&self.scenario)
        } else {
            scroll_navigation_view(&self.scenario)
        }
    }
}

fn focus_navigation_view(scenario: &str) -> Node<String> {
    let mut first = Node::text("a").focusable("a");
    match scenario {
        "focus-unbind-raw" => {
            first = first.on_event("a", |_| EventResult::message("raw".to_owned()));
        }
        "focus-declared-shadow" => {
            first = first.on_actions("a", [core_fixture_action(KeyCode::Tab, false)]);
        }
        "focus-declared-ignore" => {
            first = first.on_actions("a", [core_fixture_action(KeyCode::Tab, true)]);
        }
        _ => {}
    }
    let content = Node::column([first, Node::text("b").focusable("b")]);
    match scenario {
        "focus-rebind" => {
            let key_map = KeyMap::new()
                .rebind(ActionId::from(FOCUS_NEXT_ACTION_ID), [binding('x')])
                .unwrap();
            content.with_key_scope(KeyScope::new("root", key_map))
        }
        "focus-unbind-raw" => {
            let key_map = KeyMap::new()
                .rebind(
                    ActionId::from(FOCUS_NEXT_ACTION_ID),
                    std::iter::empty::<KeyBinding>(),
                )
                .unwrap();
            content.with_key_scope(KeyScope::new("root", key_map))
        }
        "focus-modal-no-focus" => Node::padding(
            Node::modal_with_focus(
                "modal",
                content,
                ModalFocusOptions {
                    initial: ModalInitialFocus::None,
                    ..ModalFocusOptions::default()
                },
            ),
            Insets::all(0),
        ),
        _ => content,
    }
}

fn scroll_navigation_view(scenario: &str) -> Node<String> {
    if matches!(
        scenario,
        "scroll-stop-boundary" | "scroll-wheel-through-stop"
    ) {
        let child = Node::text("child").focusable("child");
        let scope = Node::padding(child, Insets::all(0))
            .with_key_scope(
                KeyScope::new("scope", KeyMap::new())
                    .with_propagation(KeyScopePropagation::StopAtScope),
            )
            .with_length(Length::Fixed(2));
        let content = Node::column([
            scope,
            Node::text("o0\no1\no2\no3").with_length(Length::Fixed(4)),
        ]);
        return Node::scroll_viewport_with_options(
            "outer",
            content,
            core_scroll_options(ScrollAxis::Vertical, "outer-scroll"),
        )
        .on_event("outer", |_| EventResult::message("outer-raw".to_owned()));
    }

    let inner_axis = if scenario == "scroll-horizontal-pass" {
        ScrollAxis::Horizontal
    } else {
        ScrollAxis::Vertical
    };
    let inner_content = if inner_axis == ScrollAxis::Horizontal {
        Node::text("abcdefghijklmnop")
    } else {
        Node::text("i0\ni1\ni2\ni3\ni4\ni5")
    };
    let mut inner = Node::scroll_viewport_with_options(
        "inner",
        inner_content,
        core_scroll_options(inner_axis, "inner-scroll"),
    )
    .with_length(Length::Fixed(2));
    match scenario {
        "scroll-unbind-raw" | "scroll-modified-raw" => {
            inner = inner.on_event("inner", |_| EventResult::message("raw".to_owned()));
        }
        "scroll-declared-shadow" => {
            inner = inner.on_actions("inner", [core_fixture_action(KeyCode::PageDown, false)]);
        }
        "scroll-declared-ignore" => {
            inner = inner.on_actions("inner", [core_fixture_action(KeyCode::PageDown, true)]);
        }
        _ => {}
    }
    let content = Node::column([
        inner,
        Node::text("o0\no1\no2\no3").with_length(Length::Fixed(4)),
    ]);
    let mut outer = Node::scroll_viewport_with_options(
        "outer",
        content,
        core_scroll_options(ScrollAxis::Vertical, "outer-scroll"),
    );
    match scenario {
        "scroll-rebind" => {
            let key_map = KeyMap::new()
                .rebind(ActionId::from(SCROLL_PAGE_DOWN_ACTION_ID), [binding('x')])
                .unwrap();
            outer = outer.with_key_scope(KeyScope::new("outer", key_map));
        }
        "scroll-unbind-raw" => {
            let key_map = KeyMap::new()
                .rebind(
                    ActionId::from(SCROLL_PAGE_DOWN_ACTION_ID),
                    std::iter::empty::<KeyBinding>(),
                )
                .unwrap();
            outer = outer.with_key_scope(KeyScope::new("outer", key_map));
        }
        _ => {}
    }
    outer
}

fn core_scroll_options(axis: ScrollAxis, message: &'static str) -> ScrollViewportOptions<String> {
    ScrollViewportOptions {
        axis,
        on_scroll: Some(Box::new(move |_| message.to_owned())),
        ..ScrollViewportOptions::default()
    }
}

struct RevealRuntimeApp {
    lines: u32,
    viewport: u32,
    prior_reveal: Option<NodeId>,
    reveal: Option<NodeId>,
    focus: Option<NodeId>,
    ensure_focus: bool,
    stick_to_end: bool,
    updates: Vec<String>,
}

impl App for RevealRuntimeApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.updates.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let rows = (0..self.lines).map(|index| {
            let id = NodeId::new(format!("row-{index}"));
            let row = Node::text(index.to_string()).with_id(id.clone());
            let row = if self.focus.as_ref() == Some(&id) {
                row.focusable(id)
            } else {
                row
            };
            row.with_length(Length::Fixed(1))
        });
        let mut viewport = Node::scroll_viewport_with_options(
            "viewport",
            Node::column(rows),
            ScrollViewportOptions {
                axis: ScrollAxis::Vertical,
                stick_to_end: self.stick_to_end,
                ensure_focused_visible: self.ensure_focus,
                on_scroll: Some(Box::new(|_| "user-scroll".to_owned())),
            },
        );
        if let Some(target) = &self.prior_reveal {
            viewport = viewport.reveal_descendant(target.clone());
        }
        if let Some(target) = &self.reveal {
            viewport = viewport.reveal_descendant(target.clone());
        }
        Node::column([
            viewport.with_length(Length::Fixed(self.viewport)),
            Node::text("outside")
                .with_id("outside")
                .with_length(Length::Fixed(1)),
        ])
    }
}

#[test]
fn explicit_reveal_targets_match_shared_fixtures() {
    let Some(records) = support::load(
        "interaction/reveal-runtime.txt",
        "interaction-reveal-runtime",
        &[
            "lines",
            "viewport",
            "prior-reveal",
            "reveal",
            "focus",
            "ensure-focus",
            "stick-end",
            "expected-offset",
            "expected-top",
        ],
    ) else {
        return;
    };

    for record in records {
        let viewport = number(record.field("viewport"));
        let focus = fixture_node_id(record.field("focus"));
        let mut runtime = Runtime::with_clock(
            RevealRuntimeApp {
                lines: number(record.field("lines")),
                viewport,
                prior_reveal: fixture_node_id(record.field("prior-reveal")),
                reveal: fixture_node_id(record.field("reveal")),
                focus: focus.clone(),
                ensure_focus: record.field("ensure-focus") == "true",
                stick_to_end: record.field("stick-end") == "true",
                updates: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(8, viewport.saturating_add(1))),
            VirtualClock::new(),
        )
        .unwrap();
        let mut frame = runtime.render_if_dirty().unwrap().unwrap();
        if let Some(focus) = focus {
            assert!(runtime.request_focus(&focus).unwrap(), "case {}", record.id);
            frame = runtime.render_if_dirty().unwrap().unwrap();
        }

        assert_eq!(
            runtime
                .interaction()
                .scroll_offset(&NodeId::from("viewport"))
                .y,
            number(record.field("expected-offset")),
            "case {}",
            record.id
        );
        assert_eq!(
            frame.surface().cell(0, 0).unwrap().content(),
            record.field("expected-top"),
            "case {}",
            record.id
        );
        assert_eq!(runtime.queued_messages(), 0, "case {}", record.id);
        assert!(runtime.app().updates.is_empty(), "case {}", record.id);
    }
}

fn fixture_node_id(value: &str) -> Option<NodeId> {
    (value != "-").then(|| NodeId::new(value))
}

fn core_fixture_action(code: KeyCode, ignored: bool) -> Action<String> {
    Action::new(
        ActionDescriptor::new(
            "app.declared",
            "Declared",
            [named_binding(code, Modifiers::NONE)],
        ),
        move |_| {
            if ignored {
                EventResult::ignored().emit("declared".to_owned())
            } else {
                EventResult::message("declared".to_owned())
            }
        },
    )
}

#[test]
fn core_navigation_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "interaction/core-navigation-runtime.txt",
        "core-navigation-runtime",
        &[
            "event",
            "focus",
            "expected-focus",
            "inner",
            "outer",
            "expected-inner",
            "expected-outer",
            "messages",
            "consumed",
            "groups",
        ],
    ) else {
        return;
    };
    for record in records {
        let mut runtime = Runtime::with_clock(
            CoreNavigationApp {
                scenario: record.id.clone(),
                updates: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(8, 3)),
            VirtualClock::new(),
        )
        .unwrap();
        runtime.render_if_dirty().unwrap();
        if record.field("focus") != "none" {
            assert!(
                runtime
                    .request_focus(&NodeId::from(record.field("focus")))
                    .unwrap(),
                "case {}",
                record.id
            );
        }
        for field in ["inner", "outer"] {
            if let Some(offset) = optional_number(record.field(field)) {
                assert!(
                    runtime.set_scroll_offset(&NodeId::from(field), ScrollOffset::new(0, offset)),
                    "case {} field {field}",
                    record.id
                );
            }
        }
        runtime.render_if_dirty().unwrap();

        let groups: Vec<_> = runtime
            .active_action_groups()
            .unwrap()
            .iter()
            .map(|group| group.owner().as_str().to_owned())
            .collect();
        assert_eq!(
            groups,
            fixture_list(record.field("groups")),
            "case {}",
            record.id
        );

        let dispatch = runtime
            .dispatch_event(&core_navigation_event(record.field("event")))
            .unwrap();
        runtime.process_pending().unwrap();

        assert_eq!(
            dispatch.consumed(),
            record.field("consumed") == "true",
            "case {}",
            record.id
        );
        assert_eq!(
            runtime
                .interaction()
                .focused()
                .map_or("none", NodeId::as_str),
            record.field("expected-focus"),
            "case {}",
            record.id
        );
        for field in ["inner", "outer"] {
            let expected_field = format!("expected-{field}");
            if let Some(expected) = optional_number(record.field(&expected_field)) {
                assert_eq!(
                    runtime.interaction().scroll_offset(&NodeId::from(field)).y,
                    expected,
                    "case {} field {field}",
                    record.id
                );
            }
        }
        assert_eq!(
            runtime.app().updates,
            fixture_list(record.field("messages")),
            "case {}",
            record.id
        );
    }
}

fn core_navigation_event(value: &str) -> Event {
    match value {
        "tab" => named_key_event(KeyCode::Tab, Modifiers::NONE),
        "shift-tab" => named_key_event(
            KeyCode::Tab,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
        ),
        "repeat-tab" => key_event_with_action(KeyCode::Tab, Modifiers::NONE, KeyAction::Repeat),
        "release-tab" => key_event_with_action(KeyCode::Tab, Modifiers::NONE, KeyAction::Release),
        "ctrl-tab" => named_key_event(
            KeyCode::Tab,
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
        ),
        "page-up" => named_key_event(KeyCode::PageUp, Modifiers::NONE),
        "page-down" => named_key_event(KeyCode::PageDown, Modifiers::NONE),
        "repeat-page-down" => {
            key_event_with_action(KeyCode::PageDown, Modifiers::NONE, KeyAction::Repeat)
        }
        "unknown-page-down" => {
            key_event_with_action(KeyCode::PageDown, Modifiers::NONE, KeyAction::Unknown)
        }
        "release-page-down" => {
            key_event_with_action(KeyCode::PageDown, Modifiers::NONE, KeyAction::Release)
        }
        "home" => named_key_event(KeyCode::Home, Modifiers::NONE),
        "end" => named_key_event(KeyCode::End, Modifiers::NONE),
        "ctrl-page-down" => named_key_event(
            KeyCode::PageDown,
            Modifiers {
                control: true,
                ..Modifiers::NONE
            },
        ),
        "key/x" => Event::Key(KeyEvent {
            code: KeyCode::Character('x'),
            modifiers: Modifiers::NONE,
            action: KeyAction::Press,
            text: Some("x".to_owned()),
            protocol: KeyProtocol::Legacy,
        }),
        "wheel-down" => Event::Mouse(MouseEvent {
            kind: MouseKind::Scroll,
            button: MouseButton::WheelDown,
            x: 0,
            y: 0,
            modifiers: Modifiers::NONE,
        }),
        _ => panic!("unknown core navigation event {value}"),
    }
}

fn named_key_event(code: KeyCode, modifiers: Modifiers) -> Event {
    key_event_with_action(code, modifiers, KeyAction::Press)
}

fn key_event_with_action(code: KeyCode, modifiers: Modifiers, action: KeyAction) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers,
        action,
        text: None,
        protocol: KeyProtocol::Legacy,
    })
}

fn named_binding(code: KeyCode, modifiers: Modifiers) -> KeyBinding {
    KeyBinding::new(KeyStroke::new(code, modifiers))
}

fn optional_number(value: &str) -> Option<u32> {
    (value != "-").then(|| number(value))
}

struct ConflictingCoreActions;

impl App for ConflictingCoreActions {
    type Message = ();

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let key_map = KeyMap::new()
            .rebind(ActionId::from(FOCUS_NEXT_ACTION_ID), [binding('x')])
            .unwrap()
            .rebind(ActionId::from(SCROLL_PAGE_DOWN_ACTION_ID), [binding('x')])
            .unwrap();
        Node::scroll_viewport("viewport", Node::text("a\nb\nc"))
            .with_key_scope(KeyScope::new("viewport", key_map))
    }
}

#[test]
fn runtime_rejects_same_owner_core_action_conflicts() {
    let mut runtime = Runtime::with_clock(
        ConflictingCoreActions,
        nagi_tui::RuntimeConfig::new(Size::new(8, 1)),
        VirtualClock::new(),
    )
    .unwrap();
    let error = runtime.render_if_dirty().unwrap_err();
    let RuntimeError::BindingConflict(conflict) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.kind(), BindingConflictKind::AmbiguousBinding);
    assert_eq!(conflict.owner().as_str(), "viewport");
    assert_eq!(
        conflict
            .actions()
            .iter()
            .map(ActionId::as_str)
            .collect::<Vec<_>>(),
        [FOCUS_NEXT_ACTION_ID, SCROLL_PAGE_DOWN_ACTION_ID]
    );
}

struct FutureFocusConflict;

impl App for FutureFocusConflict {
    type Message = ();

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let key_map = KeyMap::new()
            .rebind(ActionId::from(FOCUS_NEXT_ACTION_ID), [binding('x')])
            .unwrap()
            .rebind(ActionId::from(FOCUS_PREVIOUS_ACTION_ID), [binding('x')])
            .unwrap();
        Node::column([
            Node::text("a").focusable("a"),
            Node::text("b")
                .focusable("b")
                .with_key_scope(KeyScope::new("b", key_map))
                .on_event("b", |_| EventResult::message(())),
        ])
    }
}

#[test]
fn newly_focused_core_route_is_resolved_before_the_next_handler() {
    let mut runtime = Runtime::with_clock(
        FutureFocusConflict,
        nagi_tui::RuntimeConfig::new(Size::new(8, 2)),
        VirtualClock::new(),
    )
    .unwrap();
    runtime.render_if_dirty().unwrap();
    assert!(runtime.request_focus(&NodeId::from("a")).unwrap());
    runtime
        .dispatch_event(&named_key_event(KeyCode::Tab, Modifiers::NONE))
        .unwrap();
    assert_eq!(runtime.interaction().focused(), Some(&NodeId::from("b")));

    let error = runtime
        .dispatch_event(&named_key_event(KeyCode::Character('z'), Modifiers::NONE))
        .unwrap_err();
    let RuntimeEventError::Runtime(RuntimeError::BindingConflict(conflict)) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.owner().as_str(), "b");
    assert_eq!(runtime.queued_messages(), 0);
}

struct ConflictingActions;

impl App for ConflictingActions {
    type Message = ();

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Node::text("conflict").on_actions(
            "owner",
            [
                Action::new(descriptor("app.first", 'x'), |_| EventResult::consumed()),
                Action::new(descriptor("app.second", 'x'), |_| EventResult::consumed()),
            ],
        )
    }
}

#[test]
fn runtime_rejects_action_conflicts_before_publishing_a_frame() {
    let mut runtime = Runtime::with_clock(
        ConflictingActions,
        nagi_tui::RuntimeConfig::new(Size::new(20, 1)),
        VirtualClock::new(),
    )
    .unwrap();
    let error = runtime.render_if_dirty().unwrap_err();
    let RuntimeError::BindingConflict(conflict) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.kind(), BindingConflictKind::AmbiguousBinding);
    assert_eq!(conflict.owner().as_str(), "owner");
    assert_eq!(
        conflict
            .actions()
            .iter()
            .map(ActionId::as_str)
            .collect::<Vec<_>>(),
        ["app.first", "app.second"]
    );
}

struct RouteConflictingActions;

impl App for RouteConflictingActions {
    type Message = ();

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let map = KeyMap::new()
            .rebind(ActionId::from("app.second"), [binding('x')])
            .unwrap();
        let child = Node::text("child")
            .focusable("child")
            .with_key_scope(KeyScope::new("child", map))
            .on_event("child", |_| EventResult::message(()));
        Node::padding(child, Insets::all(0))
            .on_event("root", |_| EventResult::message(()))
            .on_actions(
                "root",
                [
                    Action::new(descriptor("app.first", 'x'), |_| EventResult::consumed()),
                    Action::new(descriptor("app.second", 'y'), |_| EventResult::consumed()),
                ],
            )
    }
}

#[test]
fn runtime_rejects_route_specific_conflicts_before_any_handler() {
    let mut runtime = Runtime::with_clock(
        RouteConflictingActions,
        nagi_tui::RuntimeConfig::new(Size::new(20, 1)),
        VirtualClock::new(),
    )
    .unwrap();
    runtime.render_if_dirty().unwrap();
    runtime.request_focus(&NodeId::from("child")).unwrap();

    let error = runtime
        .dispatch_event(&key_routing_event("key/z"))
        .unwrap_err();

    let RuntimeEventError::Runtime(RuntimeError::BindingConflict(conflict)) = error else {
        panic!("unexpected error {error}");
    };
    assert_eq!(conflict.owner().as_str(), "root");
    assert_eq!(
        conflict
            .scope_path()
            .iter()
            .map(NodeId::as_str)
            .collect::<Vec<_>>(),
        ["child"]
    );
    assert_eq!(runtime.queued_messages(), 0);
}

fn descriptor(id: &'static str, character: char) -> ActionDescriptor {
    ActionDescriptor::new(id, id, [binding(character)])
}

fn binding(character: char) -> KeyBinding {
    KeyBinding::new(KeyStroke::character(character, Modifiers::NONE))
}

fn message_action(id: &'static str, character: char, message: &'static str) -> Action<String> {
    routed_action(id, character, character, message)
}

fn routed_action(
    id: &'static str,
    default_character: char,
    event_character: char,
    message: &'static str,
) -> Action<String> {
    Action::new(descriptor(id, default_character), move |event| {
        assert_eq!(event.action().as_str(), id);
        assert_eq!(
            event.stroke(),
            KeyStroke::character(event_character, Modifiers::NONE)
        );
        EventResult::message(message.to_owned())
    })
}

fn ignored_action(id: &'static str, character: char, message: &'static str) -> Action<String> {
    Action::new(descriptor(id, character), move |event| {
        assert_eq!(event.action().as_str(), id);
        assert_eq!(
            event.stroke(),
            KeyStroke::character(character, Modifiers::NONE)
        );
        EventResult::ignored().emit(message.to_owned())
    })
}

fn key_routing_event(value: &str) -> Event {
    let (kind, scalar) = value.split_once('/').expect("fixture event has a slash");
    let character = scalar.chars().next().expect("fixture event has a scalar");
    match kind {
        "key" => Event::Key(KeyEvent {
            code: KeyCode::Character(character),
            modifiers: Modifiers::NONE,
            action: KeyAction::Press,
            text: Some(character.to_string()),
            protocol: KeyProtocol::Legacy,
        }),
        "text" => Event::Text(character.to_string()),
        "paste" => Event::Paste(character.to_string()),
        _ => panic!("unknown fixture event kind {kind}"),
    }
}

fn fixture_list(value: &str) -> Vec<String> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').map(str::to_owned).collect()
    }
}

fn number(value: &str) -> u32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid number {value}: {error}"))
}
