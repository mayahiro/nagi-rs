//! Private Linux and macOS terminal-session integration

mod session;
mod system;
mod wake;

#[allow(unused_imports)]
pub(crate) use session::{TerminalError, TerminalSession};
