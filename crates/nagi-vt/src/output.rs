/// A color used by the SGR output encoder
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SgrColor {
    /// Terminal default color
    #[default]
    Default,
    /// Indexed terminal palette entry
    Indexed(u8),
    /// 24-bit RGB color
    Rgb {
        /// Red component
        red: u8,
        /// Green component
        green: u8,
        /// Blue component
        blue: u8,
    },
}

/// A complete terminal SGR style
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SgrStyle {
    /// Foreground color
    pub foreground: SgrColor,
    /// Background color
    pub background: SgrColor,
    /// Optional underline color
    pub underline_color: Option<SgrColor>,
    /// Bold intensity
    pub bold: bool,
    /// Dim intensity
    pub dim: bool,
    /// Italic text
    pub italic: bool,
    /// Underlined text
    pub underline: bool,
    /// Blinking text
    pub blink: bool,
    /// Reversed foreground and background
    pub reverse: bool,
    /// Hidden text
    pub hidden: bool,
    /// Struck-through text
    pub strikethrough: bool,
}

/// Optional terminal encoder capabilities
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Capabilities {
    /// Terminal supports 24-bit SGR colors
    pub true_color: bool,
    /// Terminal supports SGR underline color
    pub underline_color: bool,
    /// Terminal supports DECSCUSR cursor shape changes
    pub cursor_shape: bool,
    /// Terminal supports synchronized update mode
    pub synchronized_updates: bool,
}

impl Capabilities {
    /// Safe xterm-compatible baseline with RGB reduced to the indexed palette
    pub const BASELINE: Self = Self {
        true_color: false,
        underline_color: false,
        cursor_shape: false,
        synchronized_updates: false,
    };

    /// Modern capabilities used when support has been established
    pub const MODERN: Self = Self {
        true_color: true,
        underline_color: true,
        cursor_shape: true,
        synchronized_updates: true,
    };
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::BASELINE
    }
}

/// Erasure range relative to the cursor
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EraseMode {
    /// From cursor through the end
    After,
    /// From start through the cursor
    Before,
    /// The complete line or display
    All,
}

/// DECSCUSR cursor shape
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CursorShape {
    /// Terminal default
    Default,
    /// Blinking block
    BlinkingBlock,
    /// Steady block
    SteadyBlock,
    /// Blinking underline
    BlinkingUnderline,
    /// Steady underline
    SteadyUnderline,
    /// Blinking vertical bar
    BlinkingBar,
    /// Steady vertical bar
    SteadyBar,
}

/// Xterm mouse tracking policy
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MouseTracking {
    /// Button press and release
    Press,
    /// Button events and movement while a button is held
    Button,
    /// All pointer movement
    Any,
}

/// A pure terminal output operation
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalOp {
    /// Move to a zero-based absolute cell position
    MoveTo {
        /// Zero-based horizontal coordinate
        x: u32,
        /// Zero-based vertical coordinate
        y: u32,
    },
    /// Move by signed cell deltas, vertical before horizontal
    MoveRelative {
        /// Horizontal delta, positive to the right
        dx: i32,
        /// Vertical delta, positive downward
        dy: i32,
    },
    /// Apply a complete SGR style after resetting prior style state
    SetStyle(SgrStyle),
    /// Reset all SGR style state
    ResetStyle,
    /// Write text after replacing terminal control characters with U+FFFD
    WriteText(String),
    /// Erase part or all of the current line
    EraseLine(EraseMode),
    /// Erase part or all of the display
    EraseDisplay(EraseMode),
    /// Show the cursor
    ShowCursor,
    /// Hide the cursor
    HideCursor,
    /// Set the cursor shape when supported
    SetCursorShape(CursorShape),
    /// Enter the alternate screen buffer
    EnterAlternateScreen,
    /// Leave the alternate screen buffer
    LeaveAlternateScreen,
    /// Enable bracketed paste mode
    EnableBracketedPaste,
    /// Disable bracketed paste mode
    DisableBracketedPaste,
    /// Enable SGR mouse reporting
    EnableMouse(MouseTracking),
    /// Disable known mouse reporting modes and SGR encoding
    DisableMouse,
    /// Enable terminal focus reports
    EnableFocus,
    /// Disable terminal focus reports
    DisableFocus,
    /// Begin a synchronized update when supported
    BeginSynchronizedUpdate,
    /// End a synchronized update when supported
    EndSynchronizedUpdate,
}

/// Encodes terminal operations deterministically for `capabilities`
#[must_use]
pub fn encode(operations: &[TerminalOp], capabilities: Capabilities) -> Vec<u8> {
    let mut output = Vec::new();
    append_encoded(&mut output, operations, capabilities);
    output
}

/// Appends deterministic terminal encoding to `destination`
pub fn append_encoded(
    destination: &mut Vec<u8>,
    operations: &[TerminalOp],
    capabilities: Capabilities,
) {
    for operation in operations {
        encode_operation(destination, operation, capabilities);
    }
}

fn encode_operation(output: &mut Vec<u8>, operation: &TerminalOp, capabilities: Capabilities) {
    match operation {
        TerminalOp::MoveTo { x, y } => {
            output.extend_from_slice(b"\x1B[");
            push_decimal(output, u64::from(*y) + 1);
            output.push(b';');
            push_decimal(output, u64::from(*x) + 1);
            output.push(b'H');
        }
        TerminalOp::MoveRelative { dx, dy } => {
            if *dy < 0 {
                write_csi_count(output, dy.unsigned_abs(), 'A');
            } else if *dy > 0 {
                write_csi_count(output, *dy as u32, 'B');
            }
            if *dx > 0 {
                write_csi_count(output, *dx as u32, 'C');
            } else if *dx < 0 {
                write_csi_count(output, dx.unsigned_abs(), 'D');
            }
        }
        TerminalOp::SetStyle(style) => write_style(output, *style, capabilities),
        TerminalOp::ResetStyle => output.extend_from_slice(b"\x1B[0m"),
        TerminalOp::WriteText(text) => write_safe_text(output, text),
        TerminalOp::EraseLine(mode) => write_erase(output, *mode, 'K'),
        TerminalOp::EraseDisplay(mode) => write_erase(output, *mode, 'J'),
        TerminalOp::ShowCursor => output.extend_from_slice(b"\x1B[?25h"),
        TerminalOp::HideCursor => output.extend_from_slice(b"\x1B[?25l"),
        TerminalOp::SetCursorShape(shape) if capabilities.cursor_shape => {
            output.extend_from_slice(b"\x1B[");
            push_decimal(output, u64::from(cursor_shape_code(*shape)));
            output.extend_from_slice(b" q");
        }
        TerminalOp::SetCursorShape(_) => {}
        TerminalOp::EnterAlternateScreen => output.extend_from_slice(b"\x1B[?1049h"),
        TerminalOp::LeaveAlternateScreen => output.extend_from_slice(b"\x1B[?1049l"),
        TerminalOp::EnableBracketedPaste => output.extend_from_slice(b"\x1B[?2004h"),
        TerminalOp::DisableBracketedPaste => output.extend_from_slice(b"\x1B[?2004l"),
        TerminalOp::EnableMouse(tracking) => {
            let mode = match tracking {
                MouseTracking::Press => 1000,
                MouseTracking::Button => 1002,
                MouseTracking::Any => 1003,
            };
            output.extend_from_slice(b"\x1B[?");
            push_decimal(output, mode);
            output.extend_from_slice(b"h\x1B[?1006h");
        }
        TerminalOp::DisableMouse => {
            output.extend_from_slice(b"\x1B[?1000l\x1B[?1002l\x1B[?1003l\x1B[?1006l");
        }
        TerminalOp::EnableFocus => output.extend_from_slice(b"\x1B[?1004h"),
        TerminalOp::DisableFocus => output.extend_from_slice(b"\x1B[?1004l"),
        TerminalOp::BeginSynchronizedUpdate if capabilities.synchronized_updates => {
            output.extend_from_slice(b"\x1B[?2026h");
        }
        TerminalOp::EndSynchronizedUpdate if capabilities.synchronized_updates => {
            output.extend_from_slice(b"\x1B[?2026l");
        }
        TerminalOp::BeginSynchronizedUpdate | TerminalOp::EndSynchronizedUpdate => {}
    }
}

fn write_csi_count(output: &mut Vec<u8>, count: u32, final_character: char) {
    output.extend_from_slice(b"\x1B[");
    push_decimal(output, u64::from(count));
    output.push(final_character as u8);
}

fn write_erase(output: &mut Vec<u8>, mode: EraseMode, final_character: char) {
    let parameter = match mode {
        EraseMode::After => 0,
        EraseMode::Before => 1,
        EraseMode::All => 2,
    };
    output.extend_from_slice(b"\x1B[");
    output.push(b'0' + parameter);
    output.push(final_character as u8);
}

fn cursor_shape_code(shape: CursorShape) -> u8 {
    match shape {
        CursorShape::Default => 0,
        CursorShape::BlinkingBlock => 1,
        CursorShape::SteadyBlock => 2,
        CursorShape::BlinkingUnderline => 3,
        CursorShape::SteadyUnderline => 4,
        CursorShape::BlinkingBar => 5,
        CursorShape::SteadyBar => 6,
    }
}

fn write_style(output: &mut Vec<u8>, style: SgrStyle, capabilities: Capabilities) {
    output.extend_from_slice(b"\x1B[0");
    push_color(output, style.foreground, true, capabilities);
    push_color(output, style.background, false, capabilities);
    if capabilities.underline_color {
        if let Some(color) = style.underline_color {
            push_extended_color(output, color, 58, capabilities);
        }
    }
    let attributes = [
        (style.bold, 1),
        (style.dim, 2),
        (style.italic, 3),
        (style.underline, 4),
        (style.blink, 5),
        (style.reverse, 7),
        (style.hidden, 8),
        (style.strikethrough, 9),
    ];
    for (enabled, code) in attributes {
        if enabled {
            push_parameter(output, code);
        }
    }
    output.push(b'm');
}

fn push_color(output: &mut Vec<u8>, color: SgrColor, foreground: bool, capabilities: Capabilities) {
    if color == SgrColor::Default {
        return;
    }
    push_extended_color(
        output,
        color,
        if foreground { 38 } else { 48 },
        capabilities,
    );
}

fn push_extended_color(
    output: &mut Vec<u8>,
    color: SgrColor,
    prefix: u8,
    capabilities: Capabilities,
) {
    match color {
        SgrColor::Default => push_parameter(
            output,
            match prefix {
                48 => 49,
                58 => 59,
                _ => 39,
            },
        ),
        SgrColor::Indexed(index) => {
            push_parameter(output, u64::from(prefix));
            push_parameter(output, 5);
            push_parameter(output, u64::from(index));
        }
        SgrColor::Rgb { red, green, blue } if capabilities.true_color => {
            push_parameter(output, u64::from(prefix));
            push_parameter(output, 2);
            push_parameter(output, u64::from(red));
            push_parameter(output, u64::from(green));
            push_parameter(output, u64::from(blue));
        }
        SgrColor::Rgb { red, green, blue } => {
            push_parameter(output, u64::from(prefix));
            push_parameter(output, 5);
            push_parameter(output, u64::from(indexed_rgb(red, green, blue)));
        }
    }
}

fn indexed_rgb(red: u8, green: u8, blue: u8) -> u8 {
    let level = |component: u8| ((u16::from(component) * 5 + 127) / 255) as u8;
    16 + 36 * level(red) + 6 * level(green) + level(blue)
}

fn write_safe_text(output: &mut Vec<u8>, text: &str) {
    for character in text.chars() {
        let character = if matches!(u32::from(character), 0x00..=0x1F | 0x7F..=0x9F) {
            '\u{FFFD}'
        } else {
            character
        };
        let mut buffer = [0; 4];
        output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
    }
}

fn push_parameter(output: &mut Vec<u8>, value: u64) {
    output.push(b';');
    push_decimal(output, value);
}

fn push_decimal(output: &mut Vec<u8>, mut value: u64) {
    let mut buffer = [0_u8; 20];
    let mut start = buffer.len();
    loop {
        start -= 1;
        buffer[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    output.extend_from_slice(&buffer[start..]);
}

#[cfg(test)]
mod tests {
    use super::{Capabilities, TerminalOp, append_encoded, encode};

    #[test]
    fn text_cannot_inject_terminal_controls() {
        assert_eq!(
            encode(
                &[TerminalOp::WriteText("a\x1B[31m".to_owned())],
                Capabilities::MODERN,
            ),
            "a\u{FFFD}[31m".as_bytes()
        );
    }

    #[test]
    fn minimum_relative_delta_does_not_overflow() {
        assert_eq!(
            encode(
                &[TerminalOp::MoveRelative {
                    dx: i32::MIN,
                    dy: i32::MIN,
                }],
                Capabilities::BASELINE,
            ),
            b"\x1B[2147483648A\x1B[2147483648D"
        );
    }

    #[test]
    fn append_encoded_preserves_destination_prefix() {
        let mut output = b"prefix:".to_vec();
        append_encoded(
            &mut output,
            &[
                TerminalOp::MoveTo { x: 2, y: 3 },
                TerminalOp::WriteText("ok".to_owned()),
            ],
            Capabilities::BASELINE,
        );
        assert_eq!(output, b"prefix:\x1B[4;3Hok");
    }
}
