/// Keyboard modifier state supplied by a terminal protocol
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Modifiers {
    /// Shift modifier
    pub shift: bool,
    /// Alt modifier
    pub alt: bool,
    /// Control modifier
    pub control: bool,
    /// Meta modifier when distinguished from Alt
    pub meta: bool,
}

impl Modifiers {
    /// No modifiers
    pub const NONE: Self = Self {
        shift: false,
        alt: false,
        control: false,
        meta: false,
    };
}

/// A normalized logical key
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyCode {
    /// A Unicode scalar key
    Character(char),
    /// Enter or return
    Enter,
    /// Horizontal tab
    Tab,
    /// Backspace
    Backspace,
    /// Escape
    Escape,
    /// Up arrow
    Up,
    /// Down arrow
    Down,
    /// Right arrow
    Right,
    /// Left arrow
    Left,
    /// Home
    Home,
    /// End
    End,
    /// Insert
    Insert,
    /// Delete
    Delete,
    /// Page Up
    PageUp,
    /// Page Down
    PageDown,
    /// A numbered function key
    Function(u8),
    /// A control key without a more precise logical identity
    Unknown,
}

/// Keyboard action information supplied by the input protocol
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum KeyAction {
    /// A key press
    Press,
    /// An automatic repeat
    Repeat,
    /// A key release
    Release,
    /// The legacy protocol did not supply an action
    #[default]
    Unknown,
}

/// The keyboard protocol that supplied an event
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum KeyProtocol {
    /// Traditional C0, CSI, or SS3 terminal input
    Legacy,
    /// The protocol was not identifiable
    #[default]
    Unknown,
}

/// A normalized keyboard event
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyEvent {
    /// Logical key
    pub code: KeyCode,
    /// Supplied modifiers
    pub modifiers: Modifiers,
    /// Supplied press, repeat, or release information
    pub action: KeyAction,
    /// Associated text when the protocol supplied it
    pub text: Option<String>,
    /// Source keyboard protocol
    pub protocol: KeyProtocol,
}

impl KeyEvent {
    pub(crate) fn legacy(code: KeyCode, modifiers: Modifiers, text: Option<String>) -> Self {
        Self {
            code,
            modifiers,
            action: KeyAction::Unknown,
            text,
            protocol: KeyProtocol::Legacy,
        }
    }
}

/// A normalized mouse event category
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MouseKind {
    /// Button press
    Press,
    /// Button release
    Release,
    /// Pointer movement
    Move,
    /// Wheel or scrolling action
    Scroll,
}

/// A normalized mouse button
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MouseButton {
    /// Left button
    Left,
    /// Middle button
    Middle,
    /// Right button
    Right,
    /// No button is held during movement
    None,
    /// Wheel up
    WheelUp,
    /// Wheel down
    WheelDown,
    /// Wheel left
    WheelLeft,
    /// Wheel right
    WheelRight,
    /// A protocol button number without a standard mapping
    Other(u16),
}

/// A zero-based SGR mouse event
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MouseEvent {
    /// Event category
    pub kind: MouseKind,
    /// Button or wheel direction
    pub button: MouseButton,
    /// Zero-based horizontal cell coordinate
    pub x: u32,
    /// Zero-based vertical cell coordinate
    pub y: u32,
    /// Supplied modifiers
    pub modifiers: Modifiers,
}

/// A normalized VT input event
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    /// A non-text keyboard event
    Key(KeyEvent),
    /// One valid Unicode scalar of ordinary text input
    Text(String),
    /// One complete bracketed paste payload
    Paste(String),
    /// An SGR mouse event
    Mouse(MouseEvent),
    /// Terminal gained focus
    FocusIn,
    /// Terminal lost focus
    FocusOut,
    /// A recognized terminal response retained as original bytes
    TerminalResponse(Vec<u8>),
    /// An unsupported, malformed, or bounded sequence retained for diagnosis
    UnknownSequence(Vec<u8>),
}
