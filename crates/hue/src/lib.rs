//! The colours a terminal reports of its own theme.

mod palette;
mod rgb;
mod terminal;

pub use palette::{Ansi, Palette, Slot};
pub use rgb::Rgb;
pub use terminal::{Error, Query, Result};
