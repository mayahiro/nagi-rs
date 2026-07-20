//! Private Linux and macOS terminal-session integration

mod session;
mod system;

#[allow(unused_imports)]
pub(crate) use session::{TerminalError, TerminalSession};
