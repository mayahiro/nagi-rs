//! Shared Dialog and ConfirmDialog contract tests

mod support;

use std::cell::Cell;
use std::rc::Rc;

use nagi_tui::{
    ActionAvailability, ActionDescriptor, App, Color, Effect, Event, EventDispatch, Insets,
    KeyAction, KeyBinding, KeyCode, KeyEvent, KeyMap, KeyProtocol, KeyScope, KeyStroke,
    ModalInitialFocus, ModalReturnFocus, Modifiers, MouseButton, MouseEvent, MouseKind, Node,
    NodeId, Runtime, Size, Style, Surface, VirtualClock,
};
use nagi_tui_widgets::{
    ButtonStyle, CONFIRM_ACTION_ID, ConfirmDialog, ConfirmDialogDefault, DISMISS_ACTION_ID, Dialog,
    DialogAction, Disclosure,
};

struct DialogFixtureApp {
    kind: String,
    default: String,
    cancel: String,
    confirm_enabled: bool,
    cancel_enabled: bool,
    details: String,
    initial: String,
    wrap: Option<u32>,
    destructive: bool,
    key_map: KeyMap,
    outer: bool,
    body_builds: Rc<Cell<usize>>,
    messages: Vec<String>,
}

impl App for DialogFixtureApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        self.messages.push(message);
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let confirm = DialogAction::new("confirm", "Confirm", || "confirm".to_owned())
            .enabled(self.confirm_enabled);
        let cancel = DialogAction::new("cancel", "Cancel", || "cancel".to_owned())
            .enabled(self.cancel_enabled);
        let body = Node::text("Body").focusable("body");
        let mut node = if self.kind == "confirm" {
            let default = match self.default.as_str() {
                "confirm" => ConfirmDialogDefault::Confirm,
                "cancel" => ConfirmDialogDefault::Cancel,
                value => panic!("invalid ConfirmDialog default {value}"),
            };
            let mut dialog = ConfirmDialog::new("dialog", body, confirm, cancel, default).title(
                Node::styled_text(
                    "Question",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                ),
            );
            if let Some(details) = fixture_details(&self.details, &self.body_builds) {
                dialog = dialog.details(details);
            }
            if let Some(width) = self.wrap {
                dialog = dialog.action_wrap_width(width);
            }
            dialog = match self.initial.as_str() {
                "derived" => dialog,
                "body" => dialog.initial_focus(ModalInitialFocus::Target("body".into())),
                "none" => dialog.initial_focus(ModalInitialFocus::None),
                value => panic!("invalid Dialog initial policy {value}"),
            };
            if self.destructive {
                dialog = dialog.destructive_style(destructive_button_style());
            }
            dialog.into_node()
        } else {
            let actions = [
                confirm,
                cancel,
                DialogAction::new("later", "Later", || "later".to_owned()),
            ];
            let mut dialog = Dialog::new("dialog", body, actions).title(Node::styled_text(
                "Question",
                Style {
                    bold: true,
                    ..Style::default()
                },
            ));
            if let Some(target) = fixture_target(&self.default) {
                dialog = dialog.default_action(target);
            }
            if let Some(target) = fixture_target(&self.cancel) {
                dialog = dialog.cancel_action(target);
            }
            if let Some(details) = fixture_details(&self.details, &self.body_builds) {
                dialog = dialog.details(details);
            }
            if let Some(width) = self.wrap {
                dialog = dialog.action_wrap_width(width);
            }
            dialog = match self.initial.as_str() {
                "derived" => dialog,
                "body" => dialog.initial_focus(ModalInitialFocus::Target("body".into())),
                "none" => dialog.initial_focus(ModalInitialFocus::None),
                value => panic!("invalid Dialog initial policy {value}"),
            };
            dialog.into_node()
        };
        node = node.with_key_scope(KeyScope::new("dialog-scope", self.key_map.clone()));
        if self.outer {
            Node::padding(node, Insets::all(0)).on_event("outer", |_| {
                nagi_tui::EventResult::message("raw".to_owned())
            })
        } else {
            node
        }
    }
}

#[test]
fn dialog_actions_match_shared_fixtures() {
    let Some(records) = support::load(
        "widgets/dialog.txt",
        "widget-dialog",
        &[
            "kind",
            "default",
            "cancel",
            "confirm-enabled",
            "cancel-enabled",
            "details",
            "initial",
            "start",
            "event",
            "scope",
            "outer",
            "wrap",
            "destructive",
            "message",
            "consumed",
            "focus",
            "builds",
            "rows",
            "availability",
        ],
    ) else {
        return;
    };

    for record in records {
        let descriptors = fixture_descriptors(
            record.field("kind"),
            record.field("default"),
            record.field("cancel"),
            fixture_boolean(record.field("confirm-enabled")),
            fixture_boolean(record.field("cancel-enabled")),
        );
        assert_descriptor_contract(&descriptors, &record.id);
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| fixture_availability(descriptor.availability()))
                .collect::<Vec<_>>(),
            fixture_list(record.field("availability")),
            "case {} availability",
            record.id
        );

        let body_builds = Rc::new(Cell::new(0));
        let mut runtime = Runtime::with_clock(
            DialogFixtureApp {
                kind: record.field("kind").to_owned(),
                default: record.field("default").to_owned(),
                cancel: record.field("cancel").to_owned(),
                confirm_enabled: fixture_boolean(record.field("confirm-enabled")),
                cancel_enabled: fixture_boolean(record.field("cancel-enabled")),
                details: record.field("details").to_owned(),
                initial: record.field("initial").to_owned(),
                wrap: fixture_wrap(record.field("wrap")),
                destructive: fixture_boolean(record.field("destructive")),
                key_map: fixture_key_map(record.field("scope")),
                outer: record.field("outer") == "raw",
                body_builds: Rc::clone(&body_builds),
                messages: Vec::new(),
            },
            nagi_tui::RuntimeConfig::new(Size::new(50, 14)),
            VirtualClock::new(),
        )
        .unwrap();
        let frame = runtime.render_if_dirty().unwrap().unwrap();

        if record.field("start") != "auto" {
            assert!(
                runtime
                    .request_focus(&NodeId::from(record.field("start")))
                    .unwrap(),
                "case {} start focus",
                record.id
            );
        }

        let dispatch = if record.field("event") == "none" {
            None
        } else {
            let event = fixture_event(record.field("event"), frame.surface());
            let dispatch = runtime.dispatch_event(&event).unwrap();
            runtime.process_pending().unwrap();
            Some(dispatch)
        };

        let expected_messages = fixture_list(record.field("message"));
        assert_eq!(
            runtime.app().messages,
            expected_messages,
            "case {} messages",
            record.id
        );
        assert_dispatch(
            dispatch.as_ref(),
            expected_messages.len(),
            fixture_boolean(record.field("consumed")),
            &record.id,
        );
        let actual_focus = runtime.interaction().focused().map(NodeId::as_str);
        let expected_focus = match record.field("focus") {
            "none" => None,
            focus => Some(focus),
        };
        assert_eq!(actual_focus, expected_focus, "case {} focus", record.id);
        assert_eq!(
            body_builds.get(),
            fixture_usize(record.field("builds")),
            "case {} detail builds",
            record.id
        );
        assert_eq!(
            action_row_count(frame.surface()),
            fixture_usize(record.field("rows")),
            "case {} action rows",
            record.id
        );
        if fixture_boolean(record.field("destructive")) {
            let (x, y) = find_text(frame.surface(), "Confirm").expect("Confirm label");
            assert_eq!(
                frame
                    .surface()
                    .cell(i32::try_from(x).unwrap(), i32::try_from(y).unwrap())
                    .unwrap()
                    .style()
                    .foreground,
                Color::Indexed(1),
                "case {} destructive foreground",
                record.id
            );
        }
    }
}

struct DialogReturnFocusApp {
    open: bool,
}

impl App for DialogReturnFocusApp {
    type Message = String;

    fn update(&mut self, message: Self::Message) -> Effect<Self::Message> {
        if message == "cancel" {
            self.open = false;
        }
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        let background = Node::row([
            Node::text("Opener").focusable("opener"),
            Node::text("Return").focusable("return-target"),
        ]);
        if !self.open {
            return background;
        }
        let dialog = Dialog::new(
            "dialog",
            Node::text("Body"),
            [
                DialogAction::new("confirm", "Confirm", || "confirm".to_owned()),
                DialogAction::new("cancel", "Cancel", || "cancel".to_owned()),
            ],
        )
        .default_action("confirm")
        .cancel_action("cancel")
        .return_focus(ModalReturnFocus::Target("return-target".into()))
        .into_node();
        Node::stack([background, dialog])
    }
}

#[test]
fn dialog_delegates_entry_and_return_focus_to_the_modal_lifecycle() {
    let mut runtime = Runtime::with_clock(
        DialogReturnFocusApp { open: false },
        nagi_tui::RuntimeConfig::new(Size::new(30, 6)),
        VirtualClock::new(),
    )
    .unwrap();
    runtime.render_if_dirty().unwrap();
    assert!(runtime.request_focus(&NodeId::from("opener")).unwrap());

    runtime.app_mut().open = true;
    runtime.request_frame();
    runtime.render_if_dirty().unwrap();
    assert_eq!(
        runtime.interaction().focused().map(NodeId::as_str),
        Some("confirm")
    );

    runtime
        .dispatch_event(&Event::Key(KeyEvent {
            code: KeyCode::Escape,
            modifiers: Modifiers::NONE,
            action: KeyAction::Press,
            text: None,
            protocol: KeyProtocol::Legacy,
        }))
        .unwrap();
    runtime.process_pending().unwrap();
    runtime.render_if_dirty().unwrap();
    assert_eq!(
        runtime.interaction().focused().map(NodeId::as_str),
        Some("return-target")
    );
}

struct WideLabelDialogApp;

impl App for WideLabelDialogApp {
    type Message = String;

    fn update(&mut self, _message: Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn view(&self, _context: nagi_tui::ViewContext) -> Node<Self::Message> {
        Dialog::new(
            "dialog",
            Node::text("Body"),
            [
                DialogAction::new("run", "実行", || "run".to_owned()),
                DialogAction::new("back", "戻る", || "back".to_owned()),
            ],
        )
        .default_action("run")
        .cancel_action("back")
        .action_wrap_width(17)
        .into_node()
    }
}

#[test]
fn dialog_wraps_actions_by_terminal_cells_instead_of_utf8_bytes() {
    let mut runtime = Runtime::with_clock(
        WideLabelDialogApp,
        nagi_tui::RuntimeConfig::new(Size::new(30, 6)),
        VirtualClock::new(),
    )
    .unwrap();
    let frame = runtime.render_if_dirty().unwrap().unwrap();
    assert_eq!(
        find_row(frame.surface(), "実行"),
        find_row(frame.surface(), "戻る")
    );
}

fn fixture_descriptors(
    kind: &str,
    default: &str,
    cancel_target: &str,
    confirm_enabled: bool,
    cancel_enabled: bool,
) -> [ActionDescriptor; 2] {
    let confirm =
        DialogAction::new("confirm", "Confirm", || "confirm".to_owned()).enabled(confirm_enabled);
    let cancel =
        DialogAction::new("cancel", "Cancel", || "cancel".to_owned()).enabled(cancel_enabled);
    if kind == "confirm" {
        let default = match default {
            "confirm" => ConfirmDialogDefault::Confirm,
            "cancel" => ConfirmDialogDefault::Cancel,
            value => panic!("invalid ConfirmDialog default {value}"),
        };
        return ConfirmDialog::new("dialog", Node::text("Body"), confirm, cancel, default)
            .action_descriptors();
    }
    let mut dialog = Dialog::new(
        "dialog",
        Node::text("Body"),
        [
            confirm,
            cancel,
            DialogAction::new("later", "Later", || "later".to_owned()),
        ],
    );
    if let Some(target) = fixture_target(default) {
        dialog = dialog.default_action(target);
    }
    if let Some(target) = fixture_target(cancel_target) {
        dialog = dialog.cancel_action(target);
    }
    dialog.action_descriptors()
}

fn assert_descriptor_contract(descriptors: &[ActionDescriptor; 2], case: &str) {
    assert_eq!(
        descriptors
            .iter()
            .map(|descriptor| descriptor.id().as_str())
            .collect::<Vec<_>>(),
        [CONFIRM_ACTION_ID, DISMISS_ACTION_ID],
        "case {case} action IDs"
    );
    assert_eq!(descriptors[0].label(), "Confirm", "case {case}");
    assert_eq!(descriptors[1].label(), "Dismiss", "case {case}");
    assert_eq!(
        descriptors[0].default_bindings()[0].stroke().notation(),
        "Enter",
        "case {case} confirm binding"
    );
    assert_eq!(
        descriptors[1].default_bindings()[0].stroke().notation(),
        "Escape",
        "case {case} dismiss binding"
    );
}

fn fixture_details(details: &str, builds: &Rc<Cell<usize>>) -> Option<Disclosure<String>> {
    if details == "none" {
        return None;
    }
    let expanded = match details {
        "collapsed" => false,
        "expanded" => true,
        value => panic!("invalid Dialog details {value}"),
    };
    let builds = Rc::clone(builds);
    Some(
        Disclosure::new("details", Node::text("Details"), expanded, |next| {
            format!("details:{next}")
        })
        .body(move || {
            builds.set(builds.get() + 1);
            Node::text("Detail body")
        }),
    )
}

fn fixture_target(value: &str) -> Option<NodeId> {
    match value {
        "none" => None,
        value => Some(NodeId::from(value)),
    }
}

fn fixture_wrap(value: &str) -> Option<u32> {
    match value {
        "none" => None,
        value => Some(value.parse().unwrap()),
    }
}

fn fixture_key_map(value: &str) -> KeyMap {
    let binding = |character| {
        [KeyBinding::new(KeyStroke::character(
            character,
            Modifiers::NONE,
        ))]
    };
    match value {
        "default" => KeyMap::new(),
        "rebind" => KeyMap::new()
            .rebind(CONFIRM_ACTION_ID, binding('x'))
            .unwrap()
            .rebind(DISMISS_ACTION_ID, binding('y'))
            .unwrap(),
        "unbind-confirm" => KeyMap::new()
            .rebind(CONFIRM_ACTION_ID, std::iter::empty())
            .unwrap(),
        "unbind-dismiss" => KeyMap::new()
            .rebind(DISMISS_ACTION_ID, std::iter::empty())
            .unwrap(),
        _ => panic!("invalid Dialog key scope {value}"),
    }
}

fn fixture_event(value: &str, surface: &Surface) -> Event {
    let keyboard = |code, action| {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::NONE,
            action,
            text: None,
            protocol: KeyProtocol::Legacy,
        })
    };
    match value {
        "enter" => keyboard(KeyCode::Enter, KeyAction::Press),
        "repeat-enter" => keyboard(KeyCode::Enter, KeyAction::Repeat),
        "escape" => keyboard(KeyCode::Escape, KeyAction::Press),
        "space" => keyboard(KeyCode::Character(' '), KeyAction::Press),
        "x" => keyboard(KeyCode::Character('x'), KeyAction::Press),
        "y" => keyboard(KeyCode::Character('y'), KeyAction::Press),
        "pointer-confirm" => pointer_event(surface, "Confirm"),
        "pointer-cancel" => pointer_event(surface, "Cancel"),
        _ => panic!("invalid Dialog event {value}"),
    }
}

fn pointer_event(surface: &Surface, label: &str) -> Event {
    let (x, y) = find_text(surface, label).unwrap_or_else(|| panic!("missing {label} button"));
    Event::Mouse(MouseEvent {
        kind: MouseKind::Press,
        button: MouseButton::Left,
        x,
        y,
        modifiers: Modifiers::NONE,
    })
}

fn action_row_count(surface: &Surface) -> usize {
    (0..surface.height())
        .filter(|y| {
            let row = surface_row(surface, *y);
            row.contains("Confirm") || row.contains("Cancel") || row.contains("Later")
        })
        .count()
}

fn find_text(surface: &Surface, needle: &str) -> Option<(u32, u32)> {
    let characters = needle.chars().collect::<Vec<_>>();
    for y in 0..surface.height() {
        for x in 0..=surface
            .width()
            .saturating_sub(u32::try_from(characters.len()).unwrap())
        {
            let matches = characters.iter().enumerate().all(|(offset, character)| {
                surface
                    .cell(
                        i32::try_from(x + u32::try_from(offset).unwrap()).unwrap(),
                        i32::try_from(y).unwrap(),
                    )
                    .is_some_and(|cell| cell.content() == character.to_string())
            });
            if matches {
                return Some((x, y));
            }
        }
    }
    None
}

fn surface_row(surface: &Surface, y: u32) -> String {
    let mut row = String::new();
    for x in 0..surface.width() {
        row.push_str(
            surface
                .cell(i32::try_from(x).unwrap(), i32::try_from(y).unwrap())
                .unwrap()
                .content(),
        );
    }
    row
}

fn find_row(surface: &Surface, needle: &str) -> Option<u32> {
    (0..surface.height()).find(|y| surface_row(surface, *y).contains(needle))
}

fn destructive_button_style() -> ButtonStyle {
    let default = ButtonStyle::default();
    ButtonStyle {
        normal: Style {
            foreground: Color::Indexed(1),
            ..default.normal
        },
        focused: default.focused,
        disabled: default.disabled,
    }
}

fn assert_dispatch(dispatch: Option<&EventDispatch>, messages: usize, consumed: bool, case: &str) {
    match dispatch {
        Some(dispatch) => {
            assert_eq!(dispatch.messages(), messages, "case {case} messages");
            assert_eq!(dispatch.consumed(), consumed, "case {case} consumed");
        }
        None => {
            assert_eq!(messages, 0, "case {case} missing dispatch messages");
            assert!(!consumed, "case {case} missing dispatch consumed");
        }
    }
}

fn fixture_boolean(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => panic!("invalid fixture Boolean {value}"),
    }
}

fn fixture_usize(value: &str) -> usize {
    value.parse().unwrap()
}

fn fixture_list(value: &str) -> Vec<String> {
    if value == "-" {
        Vec::new()
    } else {
        value.split(',').map(str::to_owned).collect()
    }
}

fn fixture_availability(value: ActionAvailability) -> String {
    match value {
        ActionAvailability::Enabled => "enabled",
        ActionAvailability::DisabledPassThrough => "pass",
        ActionAvailability::DisabledConsume => "consume",
    }
    .to_owned()
}
