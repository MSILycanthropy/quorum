//! The terminal a TUI runs in: raw mode, the alternate screen, input and the clipboard.

mod clipboard;
mod event;
mod reader;
mod session;

pub use event::Event;
pub use session::{Error, Result, Session};
