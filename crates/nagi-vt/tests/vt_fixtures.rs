//! Shared VT codec conformance fixtures

mod support;

use std::fmt::Write;

use nagi_vt::{
    Capabilities, CursorShape, Decoder, EraseMode, Event, KeyAction, KeyCode, KeyProtocol,
    Modifiers, MouseButton, MouseKind, MouseTracking, SgrColor, SgrStyle, TerminalOp, encode,
};

#[test]
fn input_and_every_single_split_match_shared_fixtures() {
    let Some(records) = support::load("vt/input.txt", "vt-input", &["input", "expected"]) else {
        return;
    };

    for record in records {
        let input = record.decoded("input");
        let expected = record.field("expected");
        let whole = decode_chunks([input.as_slice()]);
        assert_eq!(
            canonical_events(&whole),
            expected,
            "case {} whole",
            record.id
        );

        let bytes = decode_chunks(input.iter().map(std::slice::from_ref));
        assert_eq!(bytes, whole, "case {} byte chunks", record.id);
        for split in 0..=input.len() {
            let split_events = decode_chunks([&input[..split], &input[split..]]);
            assert_eq!(split_events, whole, "case {} split {split}", record.id);
        }
    }
}

#[test]
fn output_matches_shared_fixtures() {
    let Some(records) = support::load(
        "vt/output.txt",
        "vt-output",
        &["capabilities", "operations", "expected"],
    ) else {
        return;
    };

    for record in records {
        let capabilities = match record.field("capabilities") {
            "modern" => Capabilities::MODERN,
            "baseline" => Capabilities::BASELINE,
            value => panic!("unknown capabilities {value}"),
        };
        assert_eq!(
            encode(
                &fixture_operations(record.field("operations")),
                capabilities
            ),
            record.decoded("expected"),
            "case {}",
            record.id
        );
    }
}

fn decode_chunks<'a>(chunks: impl IntoIterator<Item = &'a [u8]>) -> Vec<Event> {
    let mut decoder = Decoder::new();
    let mut events = Vec::new();
    for chunk in chunks {
        events.extend(decoder.feed(chunk));
    }
    events.extend(decoder.flush_pending());
    events
}

fn canonical_events(events: &[Event]) -> String {
    events
        .iter()
        .map(canonical_event)
        .collect::<Vec<_>>()
        .join("|")
}

fn canonical_event(event: &Event) -> String {
    match event {
        Event::Text(text) => format!("text:{}", scalar_text(text)),
        Event::Paste(text) => format!("paste:{}", scalar_text(text)),
        Event::Key(key) => format!(
            "key:{}:{}:{}:{}:{}",
            key_code(key.code),
            modifiers(key.modifiers),
            key.text
                .as_deref()
                .map_or_else(|| "-".to_owned(), scalar_text),
            match key.action {
                KeyAction::Press => "press",
                KeyAction::Repeat => "repeat",
                KeyAction::Release => "release",
                KeyAction::Unknown => "unknown",
            },
            match key.protocol {
                KeyProtocol::Legacy => "legacy",
                KeyProtocol::Unknown => "unknown",
            }
        ),
        Event::Mouse(mouse) => format!(
            "mouse:{}:{}:{},{}:{}",
            match mouse.kind {
                MouseKind::Press => "press",
                MouseKind::Release => "release",
                MouseKind::Move => "move",
                MouseKind::Scroll => "scroll",
            },
            mouse_button(mouse.button),
            mouse.x,
            mouse.y,
            modifiers(mouse.modifiers)
        ),
        Event::FocusIn => "focus-in".to_owned(),
        Event::FocusOut => "focus-out".to_owned(),
        Event::TerminalResponse(bytes) => format!("response:{}", hexadecimal_bytes(bytes)),
        Event::UnknownSequence(bytes) => format!("unknown:{}", hexadecimal_bytes(bytes)),
    }
}

fn key_code(code: KeyCode) -> String {
    match code {
        KeyCode::Character(character) => format!("char-U+{:04X}", u32::from(character)),
        KeyCode::Enter => "enter".to_owned(),
        KeyCode::Tab => "tab".to_owned(),
        KeyCode::Backspace => "backspace".to_owned(),
        KeyCode::Escape => "escape".to_owned(),
        KeyCode::Up => "up".to_owned(),
        KeyCode::Down => "down".to_owned(),
        KeyCode::Right => "right".to_owned(),
        KeyCode::Left => "left".to_owned(),
        KeyCode::Home => "home".to_owned(),
        KeyCode::End => "end".to_owned(),
        KeyCode::Insert => "insert".to_owned(),
        KeyCode::Delete => "delete".to_owned(),
        KeyCode::PageUp => "page-up".to_owned(),
        KeyCode::PageDown => "page-down".to_owned(),
        KeyCode::Function(number) => format!("f{number}"),
        KeyCode::Unknown => "unknown".to_owned(),
    }
}

fn mouse_button(button: MouseButton) -> String {
    match button {
        MouseButton::Left => "left".to_owned(),
        MouseButton::Middle => "middle".to_owned(),
        MouseButton::Right => "right".to_owned(),
        MouseButton::None => "none".to_owned(),
        MouseButton::WheelUp => "wheel-up".to_owned(),
        MouseButton::WheelDown => "wheel-down".to_owned(),
        MouseButton::WheelLeft => "wheel-left".to_owned(),
        MouseButton::WheelRight => "wheel-right".to_owned(),
        MouseButton::Other(number) => format!("other-{number}"),
    }
}

fn modifiers(modifiers: Modifiers) -> String {
    let mut names = Vec::new();
    if modifiers.shift {
        names.push("shift");
    }
    if modifiers.alt {
        names.push("alt");
    }
    if modifiers.control {
        names.push("control");
    }
    if modifiers.meta {
        names.push("meta");
    }
    if names.is_empty() {
        "-".to_owned()
    } else {
        names.join("+")
    }
}

fn scalar_text(text: &str) -> String {
    if text.is_empty() {
        return "-".to_owned();
    }
    text.chars()
        .map(|character| format!("U+{:04X}", u32::from(character)))
        .collect::<Vec<_>>()
        .join("+")
}

fn hexadecimal_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02X}").expect("writing to a String cannot fail");
    }
    output
}

fn fixture_operations(value: &str) -> Vec<TerminalOp> {
    value
        .split(';')
        .map(|operation| {
            let fields: Vec<_> = operation.split(',').collect();
            match fields.as_slice() {
                ["move-to", x, y] => TerminalOp::MoveTo {
                    x: unsigned(x),
                    y: unsigned(y),
                },
                ["move-relative", dx, dy] => TerminalOp::MoveRelative {
                    dx: signed(dx),
                    dy: signed(dy),
                },
                ["set-style", style] => TerminalOp::SetStyle(fixture_style(style)),
                ["reset-style"] => TerminalOp::ResetStyle,
                ["write", text] => TerminalOp::WriteText(fixture_scalar_text(text)),
                ["erase-line", mode] => TerminalOp::EraseLine(erase_mode(mode)),
                ["erase-display", mode] => TerminalOp::EraseDisplay(erase_mode(mode)),
                ["show-cursor"] => TerminalOp::ShowCursor,
                ["hide-cursor"] => TerminalOp::HideCursor,
                ["cursor-shape", shape] => TerminalOp::SetCursorShape(cursor_shape(shape)),
                ["enter-alternate"] => TerminalOp::EnterAlternateScreen,
                ["leave-alternate"] => TerminalOp::LeaveAlternateScreen,
                ["enable-paste"] => TerminalOp::EnableBracketedPaste,
                ["disable-paste"] => TerminalOp::DisableBracketedPaste,
                ["enable-mouse", mode] => TerminalOp::EnableMouse(mouse_tracking(mode)),
                ["disable-mouse"] => TerminalOp::DisableMouse,
                ["enable-focus"] => TerminalOp::EnableFocus,
                ["disable-focus"] => TerminalOp::DisableFocus,
                ["begin-sync"] => TerminalOp::BeginSynchronizedUpdate,
                ["end-sync"] => TerminalOp::EndSynchronizedUpdate,
                _ => panic!("invalid terminal operation {operation}"),
            }
        })
        .collect()
}

fn fixture_style(value: &str) -> SgrStyle {
    let mut style = SgrStyle::default();
    if value == "-" {
        return style;
    }
    for token in value.split('+') {
        if let Some(color) = token.strip_prefix("fg-") {
            style.foreground = fixture_color(color);
        } else if let Some(color) = token.strip_prefix("bg-") {
            style.background = fixture_color(color);
        } else if let Some(color) = token.strip_prefix("underline-color-") {
            style.underline_color = Some(fixture_color(color));
        } else {
            match token {
                "bold" => style.bold = true,
                "dim" => style.dim = true,
                "italic" => style.italic = true,
                "underline" => style.underline = true,
                "blink" => style.blink = true,
                "reverse" => style.reverse = true,
                "hidden" => style.hidden = true,
                "strikethrough" => style.strikethrough = true,
                _ => panic!("unknown fixture style {token}"),
            }
        }
    }
    style
}

fn fixture_color(value: &str) -> SgrColor {
    if value == "default" {
        return SgrColor::Default;
    }
    if let Some(index) = value.strip_prefix("indexed-") {
        return SgrColor::Indexed(
            index
                .parse()
                .unwrap_or_else(|error| panic!("invalid color index {index}: {error}")),
        );
    }
    if let Some(rgb) = value.strip_prefix("rgb-") {
        assert_eq!(rgb.len(), 6, "invalid RGB color {rgb}");
        return SgrColor::Rgb {
            red: hexadecimal(&rgb[0..2]) as u8,
            green: hexadecimal(&rgb[2..4]) as u8,
            blue: hexadecimal(&rgb[4..6]) as u8,
        };
    }
    panic!("unknown fixture color {value}");
}

fn fixture_scalar_text(value: &str) -> String {
    value
        .split('+')
        .map(|scalar| {
            char::from_u32(hexadecimal(scalar))
                .unwrap_or_else(|| panic!("invalid Unicode scalar {scalar}"))
        })
        .collect()
}

fn erase_mode(value: &str) -> EraseMode {
    match value {
        "after" => EraseMode::After,
        "before" => EraseMode::Before,
        "all" => EraseMode::All,
        _ => panic!("unknown erase mode {value}"),
    }
}

fn cursor_shape(value: &str) -> CursorShape {
    match value {
        "default" => CursorShape::Default,
        "blinking-block" => CursorShape::BlinkingBlock,
        "steady-block" => CursorShape::SteadyBlock,
        "blinking-underline" => CursorShape::BlinkingUnderline,
        "steady-underline" => CursorShape::SteadyUnderline,
        "blinking-bar" => CursorShape::BlinkingBar,
        "steady-bar" => CursorShape::SteadyBar,
        _ => panic!("unknown cursor shape {value}"),
    }
}

fn mouse_tracking(value: &str) -> MouseTracking {
    match value {
        "press" => MouseTracking::Press,
        "button" => MouseTracking::Button,
        "any" => MouseTracking::Any,
        _ => panic!("unknown mouse tracking {value}"),
    }
}

fn unsigned(value: &str) -> u32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid unsigned integer {value}: {error}"))
}

fn signed(value: &str) -> i32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid signed integer {value}: {error}"))
}

fn hexadecimal(value: &str) -> u32 {
    u32::from_str_radix(value, 16)
        .unwrap_or_else(|error| panic!("invalid hexadecimal integer {value}: {error}"))
}
