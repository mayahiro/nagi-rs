use nagi_tui::{Event, KeyAction, KeyCode, Modifiers, MouseButton, MouseKind};

pub(crate) fn is_activation_event(event: &Event) -> bool {
    match event {
        Event::Key(key) if key.action != KeyAction::Release => match key.code {
            KeyCode::Enter => true,
            KeyCode::Character(' ') => key.modifiers == Modifiers::NONE,
            _ => false,
        },
        Event::Text(text) => text == " ",
        Event::Mouse(mouse) => mouse.kind == MouseKind::Press && mouse.button == MouseButton::Left,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use nagi_tui::{KeyEvent, KeyProtocol, MouseEvent};

    use super::*;

    #[test]
    fn activation_matches_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "widgets/activation.txt",
            "widget-activation",
            &["widget", "event", "enabled", "activate"],
        ) else {
            return;
        };
        for record in records {
            assert!(matches!(
                record.field("widget"),
                "button"
                    | "list"
                    | "checkbox"
                    | "radio"
                    | "tabs"
                    | "select"
                    | "table"
                    | "tree"
                    | "command-palette"
            ));
            let enabled = boolean(record.field("enabled"));
            assert_eq!(
                enabled && is_activation_event(&event(record.field("event"))),
                boolean(record.field("activate")),
                "case {}",
                record.id
            );
        }
    }

    fn event(value: &str) -> Event {
        let key = |code, modifiers, action| {
            Event::Key(KeyEvent {
                code,
                modifiers,
                action,
                text: None,
                protocol: KeyProtocol::Legacy,
            })
        };
        match value {
            "enter" => key(KeyCode::Enter, Modifiers::NONE, KeyAction::Press),
            "space" => Event::Text(" ".to_owned()),
            "control-space" => key(
                KeyCode::Character(' '),
                Modifiers {
                    control: true,
                    ..Modifiers::NONE
                },
                KeyAction::Press,
            ),
            "key-release" => key(KeyCode::Enter, Modifiers::NONE, KeyAction::Release),
            "mouse-left-press" => mouse(MouseKind::Press, MouseButton::Left),
            "mouse-left-release" => mouse(MouseKind::Release, MouseButton::Left),
            "mouse-right-press" => mouse(MouseKind::Press, MouseButton::Right),
            _ => panic!("unknown fixture event {value}"),
        }
    }

    fn mouse(kind: MouseKind, button: MouseButton) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            button,
            x: 0,
            y: 0,
            modifiers: Modifiers::NONE,
        })
    }

    fn boolean(value: &str) -> bool {
        match value {
            "true" => true,
            "false" => false,
            _ => panic!("invalid Boolean {value}"),
        }
    }
}
