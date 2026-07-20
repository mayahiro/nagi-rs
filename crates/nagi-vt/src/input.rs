use std::mem;
use std::str;

use crate::{Event, KeyCode, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseKind};

const ESC: u8 = 0x1B;
const PASTE_START: &[u8] = b"\x1B[200~";
const PASTE_END: &[u8] = b"\x1B[201~";
const REPLACEMENT: &str = "\u{FFFD}";

/// Maximum buffered CSI, SS3, or control-string sequence size
pub const MAX_SEQUENCE_BYTES: usize = 4_096;

/// Maximum buffered bracketed paste payload size
pub const MAX_PASTE_BYTES: usize = 1_048_576;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlKind {
    Osc,
    Other,
}

#[derive(Debug)]
enum State {
    Ground,
    Escape,
    Csi(Vec<u8>),
    Ss3(Vec<u8>),
    AltUtf8(Vec<u8>),
    ControlString {
        kind: ControlKind,
        bytes: Vec<u8>,
        saw_escape: bool,
    },
    Paste(Vec<u8>),
    DiscardPaste(Vec<u8>),
}

/// A pure streaming VT input decoder
///
/// Ordinary text produces one `Event::Text` per Unicode scalar so event
/// boundaries do not depend on input byte chunks. Call [`flush_pending`](Self::flush_pending)
/// when an external ESC deadline expires or when no more input will arrive
#[derive(Debug)]
pub struct Decoder {
    state: State,
    utf8_pending: Vec<u8>,
    invalid_text_run: bool,
}

impl Decoder {
    /// Creates a decoder in the ground state
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            utf8_pending: Vec::with_capacity(4),
            invalid_text_run: false,
        }
    }

    /// Consumes one arbitrary byte chunk and returns all completed events
    pub fn feed(&mut self, input: &[u8]) -> Vec<Event> {
        let mut events = Vec::new();
        for &byte in input {
            self.process(byte, &mut events);
        }
        events
    }

    /// Reports whether any incomplete UTF-8 or VT input is buffered
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !matches!(self.state, State::Ground)
            || !self.utf8_pending.is_empty()
            || self.invalid_text_run
    }

    /// Reports whether the only VT prefix is a lone ESC byte
    #[must_use]
    pub fn has_pending_escape(&self) -> bool {
        matches!(self.state, State::Escape)
    }

    /// Resolves all currently incomplete input without waiting or sleeping
    ///
    /// A lone ESC becomes an Escape key, incomplete UTF-8 becomes one U+FFFD,
    /// and any other incomplete VT sequence becomes `UnknownSequence`
    pub fn flush_pending(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        self.finish_text(&mut events);
        match mem::replace(&mut self.state, State::Ground) {
            State::Ground => {}
            State::Escape => events.push(escape_event()),
            State::Csi(bytes)
            | State::Ss3(bytes)
            | State::AltUtf8(bytes)
            | State::ControlString { bytes, .. } => {
                events.push(Event::UnknownSequence(bytes));
            }
            State::Paste(_) | State::DiscardPaste(_) => {
                events.push(Event::UnknownSequence(PASTE_START.to_vec()));
            }
        }
        events
    }

    fn process(&mut self, byte: u8, events: &mut Vec<Event>) {
        match mem::replace(&mut self.state, State::Ground) {
            State::Ground => self.process_ground(byte, events),
            State::Escape => self.process_escape(byte, events),
            State::Csi(mut bytes) => {
                bytes.push(byte);
                if bytes.len() > MAX_SEQUENCE_BYTES {
                    events.push(Event::UnknownSequence(bytes));
                } else if (0x40..=0x7E).contains(&byte) {
                    if bytes.as_slice() == PASTE_START {
                        self.state = State::Paste(Vec::new());
                    } else {
                        events.push(parse_csi(bytes));
                    }
                } else if !(0x20..=0x3F).contains(&byte) {
                    events.push(Event::UnknownSequence(bytes));
                } else {
                    self.state = State::Csi(bytes);
                }
            }
            State::Ss3(mut bytes) => {
                bytes.push(byte);
                if (0x40..=0x7E).contains(&byte) {
                    events.push(parse_ss3(bytes));
                } else if bytes.len() > MAX_SEQUENCE_BYTES || !(0x20..=0x3F).contains(&byte) {
                    events.push(Event::UnknownSequence(bytes));
                } else {
                    self.state = State::Ss3(bytes);
                }
            }
            State::AltUtf8(mut bytes) => {
                bytes.push(byte);
                let expected = utf8_expected(bytes[1]);
                let payload = &bytes[1..];
                if expected.is_none() || !valid_utf8_prefix(payload) {
                    events.push(Event::UnknownSequence(bytes));
                } else if payload.len() == expected.unwrap_or_default() {
                    match str::from_utf8(payload)
                        .ok()
                        .and_then(|text| text.chars().next())
                    {
                        Some(character) => events.push(alt_character_event(character)),
                        None => events.push(Event::UnknownSequence(bytes)),
                    }
                } else {
                    self.state = State::AltUtf8(bytes);
                }
            }
            State::ControlString {
                kind,
                mut bytes,
                saw_escape,
            } => {
                bytes.push(byte);
                let terminated =
                    (kind == ControlKind::Osc && byte == 0x07) || (saw_escape && byte == b'\\');
                if terminated {
                    events.push(Event::TerminalResponse(bytes));
                } else if bytes.len() > MAX_SEQUENCE_BYTES {
                    events.push(Event::UnknownSequence(bytes));
                } else {
                    self.state = State::ControlString {
                        kind,
                        bytes,
                        saw_escape: byte == ESC,
                    };
                }
            }
            State::Paste(mut bytes) => {
                bytes.push(byte);
                if bytes.ends_with(PASTE_END) {
                    bytes.truncate(bytes.len() - PASTE_END.len());
                    if bytes.len() > MAX_PASTE_BYTES {
                        events.push(Event::UnknownSequence(PASTE_START.to_vec()));
                    } else {
                        events.push(Event::Paste(normalize_utf8(&bytes)));
                    }
                } else if bytes.len() > MAX_PASTE_BYTES + PASTE_END.len() - 1 {
                    let keep_from = bytes.len().saturating_sub(PASTE_END.len() - 1);
                    let tail = bytes.split_off(keep_from);
                    events.push(Event::UnknownSequence(PASTE_START.to_vec()));
                    self.state = State::DiscardPaste(tail);
                } else {
                    self.state = State::Paste(bytes);
                }
            }
            State::DiscardPaste(mut tail) => {
                tail.push(byte);
                if !tail.ends_with(PASTE_END) {
                    if tail.len() >= PASTE_END.len() {
                        tail.remove(0);
                    }
                    self.state = State::DiscardPaste(tail);
                }
            }
        }
    }

    fn process_ground(&mut self, byte: u8, events: &mut Vec<Event>) {
        if byte == ESC {
            self.finish_text(events);
            self.state = State::Escape;
        } else if byte < 0x20 || byte == 0x7F {
            self.finish_text(events);
            events.push(control_event(byte));
        } else {
            self.process_text_byte(byte, events);
        }
    }

    fn process_escape(&mut self, byte: u8, events: &mut Vec<Event>) {
        match byte {
            b'[' => self.state = State::Csi(vec![ESC, b'[']),
            b'O' => self.state = State::Ss3(vec![ESC, b'O']),
            b']' => {
                self.state = State::ControlString {
                    kind: ControlKind::Osc,
                    bytes: vec![ESC, b']'],
                    saw_escape: false,
                };
            }
            b'P' | b'^' | b'_' => {
                self.state = State::ControlString {
                    kind: ControlKind::Other,
                    bytes: vec![ESC, byte],
                    saw_escape: false,
                };
            }
            ESC => {
                events.push(escape_event());
                self.state = State::Escape;
            }
            0x00..=0x1F | 0x7F => {
                events.push(escape_event());
                self.process_ground(byte, events);
            }
            0x20..=0x7E => events.push(alt_character_event(char::from(byte))),
            _ if utf8_expected(byte).is_some() => {
                self.state = State::AltUtf8(vec![ESC, byte]);
            }
            _ => events.push(Event::UnknownSequence(vec![ESC, byte])),
        }
    }

    fn process_text_byte(&mut self, byte: u8, events: &mut Vec<Event>) {
        if byte.is_ascii() {
            if !self.utf8_pending.is_empty() {
                self.invalid_text_run = true;
                self.utf8_pending.clear();
            }
            self.flush_invalid_text(events);
            events.push(Event::Text(char::from(byte).to_string()));
            return;
        }

        if self.utf8_pending.is_empty() {
            if utf8_expected(byte).is_some() {
                self.utf8_pending.push(byte);
            } else {
                self.invalid_text_run = true;
            }
            return;
        }

        self.utf8_pending.push(byte);
        if !valid_utf8_prefix(&self.utf8_pending) {
            self.invalid_text_run = true;
            self.utf8_pending.clear();
            self.process_text_byte(byte, events);
            return;
        }
        let expected = utf8_expected(self.utf8_pending[0]).unwrap_or_default();
        if self.utf8_pending.len() != expected {
            return;
        }
        let character = str::from_utf8(&self.utf8_pending)
            .ok()
            .and_then(|text| text.chars().next());
        self.utf8_pending.clear();
        if let Some(character) = character {
            self.flush_invalid_text(events);
            events.push(Event::Text(character.to_string()));
        } else {
            self.invalid_text_run = true;
        }
    }

    fn finish_text(&mut self, events: &mut Vec<Event>) {
        if !self.utf8_pending.is_empty() {
            self.invalid_text_run = true;
            self.utf8_pending.clear();
        }
        self.flush_invalid_text(events);
    }

    fn flush_invalid_text(&mut self, events: &mut Vec<Event>) {
        if self.invalid_text_run {
            events.push(Event::Text(REPLACEMENT.to_owned()));
            self.invalid_text_run = false;
        }
    }
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

fn utf8_expected(lead: u8) -> Option<usize> {
    match lead {
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

fn valid_utf8_prefix(bytes: &[u8]) -> bool {
    let Some(expected) = bytes.first().copied().and_then(utf8_expected) else {
        return false;
    };
    if bytes.len() > expected {
        return false;
    }
    for (index, byte) in bytes.iter().copied().enumerate().skip(1) {
        if !(0x80..=0xBF).contains(&byte) {
            return false;
        }
        if index == 1 {
            match bytes[0] {
                0xE0 if byte < 0xA0 => return false,
                0xED if byte > 0x9F => return false,
                0xF0 if byte < 0x90 => return false,
                0xF4 if byte > 0x8F => return false,
                _ => {}
            }
        }
    }
    true
}

fn parse_csi(bytes: Vec<u8>) -> Event {
    let final_byte = *bytes.last().unwrap_or(&0);
    let body = &bytes[2..bytes.len().saturating_sub(1)];
    if body.first() == Some(&b'<') && matches!(final_byte, b'M' | b'm') {
        return parse_sgr_mouse(&bytes, &body[1..], final_byte)
            .map(Event::Mouse)
            .unwrap_or(Event::UnknownSequence(bytes));
    }

    if body.is_empty() {
        let event = match final_byte {
            b'A' => Some(key_event(KeyCode::Up, Modifiers::NONE)),
            b'B' => Some(key_event(KeyCode::Down, Modifiers::NONE)),
            b'C' => Some(key_event(KeyCode::Right, Modifiers::NONE)),
            b'D' => Some(key_event(KeyCode::Left, Modifiers::NONE)),
            b'H' => Some(key_event(KeyCode::Home, Modifiers::NONE)),
            b'F' => Some(key_event(KeyCode::End, Modifiers::NONE)),
            b'Z' => Some(key_event(
                KeyCode::Tab,
                Modifiers {
                    shift: true,
                    ..Modifiers::NONE
                },
            )),
            _ => None,
        };
        if let Some(event) = event {
            return event;
        }
        if final_byte == b'I' {
            return Event::FocusIn;
        }
        if final_byte == b'O' {
            return Event::FocusOut;
        }
    }

    let params = parse_params(body);
    if matches!(final_byte, b'A' | b'B' | b'C' | b'D' | b'H' | b'F') {
        if let Some(params) = params.as_deref() {
            if let Some(modifiers) = key_modifiers(params) {
                let code = match final_byte {
                    b'A' => KeyCode::Up,
                    b'B' => KeyCode::Down,
                    b'C' => KeyCode::Right,
                    b'D' => KeyCode::Left,
                    b'H' => KeyCode::Home,
                    _ => KeyCode::End,
                };
                return key_event(code, modifiers);
            }
        }
    }
    if final_byte == b'~' {
        if let Some(params) = params.as_deref() {
            if let Some((&code, rest)) = params.split_first() {
                let modifiers = match rest {
                    [] => Some(Modifiers::NONE),
                    [value] => xterm_modifiers(*value),
                    _ => None,
                };
                if let (Some(code), Some(modifiers)) = (tilde_key(code), modifiers) {
                    return key_event(code, modifiers);
                }
            }
        }
    }
    if final_byte == b'R'
        && params
            .as_ref()
            .is_some_and(|params| params.len() == 2 && params.iter().all(|value| *value != 0))
    {
        return Event::TerminalResponse(bytes);
    }
    if matches!(final_byte, b'c' | b'n' | b't') {
        return Event::TerminalResponse(bytes);
    }
    Event::UnknownSequence(bytes)
}

fn parse_ss3(bytes: Vec<u8>) -> Event {
    let final_byte = *bytes.last().unwrap_or(&0);
    let code = match final_byte {
        b'A' => Some(KeyCode::Up),
        b'B' => Some(KeyCode::Down),
        b'C' => Some(KeyCode::Right),
        b'D' => Some(KeyCode::Left),
        b'H' => Some(KeyCode::Home),
        b'F' => Some(KeyCode::End),
        b'P' => Some(KeyCode::Function(1)),
        b'Q' => Some(KeyCode::Function(2)),
        b'R' => Some(KeyCode::Function(3)),
        b'S' => Some(KeyCode::Function(4)),
        _ => None,
    };
    code.map(|code| key_event(code, Modifiers::NONE))
        .unwrap_or(Event::UnknownSequence(bytes))
}

fn parse_params(body: &[u8]) -> Option<Vec<u32>> {
    if body.is_empty() {
        return Some(Vec::new());
    }
    let text = str::from_utf8(body).ok()?;
    text.split(';')
        .map(|part| {
            if part.is_empty() {
                Some(0)
            } else {
                part.parse().ok()
            }
        })
        .collect()
}

fn key_modifiers(params: &[u32]) -> Option<Modifiers> {
    match params {
        [] | [_] => Some(Modifiers::NONE),
        [_, value] => xterm_modifiers(*value),
        _ => None,
    }
}

fn xterm_modifiers(value: u32) -> Option<Modifiers> {
    if !(1..=16).contains(&value) {
        return None;
    }
    let bits = value - 1;
    Some(Modifiers {
        shift: bits & 1 != 0,
        alt: bits & 2 != 0,
        control: bits & 4 != 0,
        meta: bits & 8 != 0,
    })
}

fn tilde_key(code: u32) -> Option<KeyCode> {
    match code {
        1 | 7 => Some(KeyCode::Home),
        2 => Some(KeyCode::Insert),
        3 => Some(KeyCode::Delete),
        4 | 8 => Some(KeyCode::End),
        5 => Some(KeyCode::PageUp),
        6 => Some(KeyCode::PageDown),
        11 => Some(KeyCode::Function(1)),
        12 => Some(KeyCode::Function(2)),
        13 => Some(KeyCode::Function(3)),
        14 => Some(KeyCode::Function(4)),
        15 => Some(KeyCode::Function(5)),
        17 => Some(KeyCode::Function(6)),
        18 => Some(KeyCode::Function(7)),
        19 => Some(KeyCode::Function(8)),
        20 => Some(KeyCode::Function(9)),
        21 => Some(KeyCode::Function(10)),
        23 => Some(KeyCode::Function(11)),
        24 => Some(KeyCode::Function(12)),
        _ => None,
    }
}

fn parse_sgr_mouse(raw: &[u8], body: &[u8], final_byte: u8) -> Option<MouseEvent> {
    let params = parse_params(body)?;
    if params.len() != 3 || params[1] == 0 || params[2] == 0 {
        return None;
    }
    let encoded = params[0];
    let modifiers = Modifiers {
        shift: encoded & 4 != 0,
        alt: encoded & 8 != 0,
        control: encoded & 16 != 0,
        meta: false,
    };
    let base = encoded & 3;
    let motion = encoded & 32 != 0;
    let wheel = encoded & 64 != 0;
    let (kind, button) = if wheel {
        let button = match base {
            0 => MouseButton::WheelUp,
            1 => MouseButton::WheelDown,
            2 => MouseButton::WheelLeft,
            _ => MouseButton::WheelRight,
        };
        (MouseKind::Scroll, button)
    } else if motion {
        (MouseKind::Move, base_button(base))
    } else if final_byte == b'm' {
        (MouseKind::Release, base_button(base))
    } else {
        (MouseKind::Press, base_button(base))
    };
    let _ = raw;
    Some(MouseEvent {
        kind,
        button,
        x: params[1] - 1,
        y: params[2] - 1,
        modifiers,
    })
}

fn base_button(value: u32) -> MouseButton {
    match value {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        3 => MouseButton::None,
        other => MouseButton::Other(other as u16),
    }
}

fn control_event(byte: u8) -> Event {
    let (code, modifiers) = match byte {
        b'\r' | b'\n' => (KeyCode::Enter, Modifiers::NONE),
        b'\t' => (KeyCode::Tab, Modifiers::NONE),
        0x08 | 0x7F => (KeyCode::Backspace, Modifiers::NONE),
        0x00 => (KeyCode::Character(' '), control_modifiers()),
        0x01..=0x1A => (
            KeyCode::Character(char::from(b'a' + byte - 1)),
            control_modifiers(),
        ),
        0x1C => (KeyCode::Character('\\'), control_modifiers()),
        0x1D => (KeyCode::Character(']'), control_modifiers()),
        0x1E => (KeyCode::Character('^'), control_modifiers()),
        0x1F => (KeyCode::Character('_'), control_modifiers()),
        _ => (KeyCode::Unknown, control_modifiers()),
    };
    key_event(code, modifiers)
}

fn control_modifiers() -> Modifiers {
    Modifiers {
        control: true,
        ..Modifiers::NONE
    }
}

fn key_event(code: KeyCode, modifiers: Modifiers) -> Event {
    Event::Key(KeyEvent::legacy(code, modifiers, None))
}

fn escape_event() -> Event {
    key_event(KeyCode::Escape, Modifiers::NONE)
}

fn alt_character_event(character: char) -> Event {
    Event::Key(KeyEvent::legacy(
        KeyCode::Character(character),
        Modifiers {
            alt: true,
            ..Modifiers::NONE
        },
        Some(character.to_string()),
    ))
}

fn normalize_utf8(input: &[u8]) -> String {
    if let Ok(valid) = str::from_utf8(input) {
        return valid.to_owned();
    }
    let mut output = String::with_capacity(input.len());
    let mut offset = 0;
    while offset < input.len() {
        match str::from_utf8(&input[offset..]) {
            Ok(valid) => {
                output.push_str(valid);
                break;
            }
            Err(error) if error.valid_up_to() != 0 => {
                let valid_end = offset + error.valid_up_to();
                if let Ok(valid) = str::from_utf8(&input[offset..valid_end]) {
                    output.push_str(valid);
                }
                offset = valid_end;
            }
            Err(_) => {
                output.push_str(REPLACEMENT);
                loop {
                    let length = str::from_utf8(&input[offset..]).map_or_else(
                        |error| error.error_len().unwrap_or(input.len() - offset),
                        |_| 0,
                    );
                    if length == 0 {
                        break;
                    }
                    offset = offset.saturating_add(length).min(input.len());
                    if offset == input.len() {
                        break;
                    }
                    match str::from_utf8(&input[offset..]) {
                        Ok(_) => break,
                        Err(next) if next.valid_up_to() != 0 => break,
                        Err(_) => {}
                    }
                }
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{Decoder, Event, MAX_PASTE_BYTES, PASTE_END, PASTE_START};

    #[test]
    fn text_event_boundaries_do_not_follow_chunks() {
        let mut decoder = Decoder::new();
        let mut events = decoder.feed(&[0xE6]);
        events.extend(decoder.feed(&[0x97, 0xA5]));
        events.extend(decoder.flush_pending());

        assert_eq!(events, [Event::Text("日".to_owned())]);
    }

    #[test]
    fn invalid_runs_cross_chunk_boundaries() {
        let mut decoder = Decoder::new();
        let mut events = decoder.feed(&[0xFF]);
        events.extend(decoder.feed(&[0xFE, b'A']));

        assert_eq!(
            events,
            [
                Event::Text("\u{FFFD}".to_owned()),
                Event::Text("A".to_owned())
            ]
        );
    }

    #[test]
    fn oversized_paste_is_bounded_and_recovers_at_terminator() {
        let mut input = PASTE_START.to_vec();
        input.resize(PASTE_START.len() + MAX_PASTE_BYTES + 1, b'x');
        input.extend_from_slice(PASTE_END);
        let mut decoder = Decoder::new();

        let events = decoder.feed(&input);

        assert_eq!(events, [Event::UnknownSequence(PASTE_START.to_vec())]);
        assert!(!decoder.has_pending());
    }

    #[test]
    fn every_byte_value_and_incomplete_suffix_is_non_panicking() {
        let input: Vec<_> = (u8::MIN..=u8::MAX).collect();
        let mut decoder = Decoder::new();

        let _ = decoder.feed(&input);
        let _ = decoder.flush_pending();

        assert!(!decoder.has_pending());
    }
}
