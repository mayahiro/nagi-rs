//! Pure VT input decoder and output encoder primitives for Nagi terminal applications
//!
//! The codec owns no I/O, file descriptors, timers, threads, or terminal
//! session state

mod event;
mod input;
mod output;
mod style;

pub use event::{
    Event, KeyAction, KeyCode, KeyEvent, KeyProtocol, Modifiers, MouseButton, MouseEvent, MouseKind,
};
pub use input::{Decoder, MAX_PASTE_BYTES, MAX_SEQUENCE_BYTES};
pub use output::{
    Capabilities, CursorShape, EraseMode, MouseTracking, SgrColor, SgrStyle, TerminalOp, encode,
};
pub use style::{Attributes, Color, Style};
