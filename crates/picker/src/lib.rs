//! A box to type in and a list to pick from: commands, files, or anything else a program can name.

mod entry;
mod picker;
mod rank;

pub use entry::Entry;
pub use picker::{Outcome, Picker};

use ratatui::style::Style;

/// The styles the picker is drawn in, so it takes the colours of the program
/// that shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Styles {
    pub base: Style,
    pub muted: Style,
    pub key: Style,
    pub selected: Style,
}
