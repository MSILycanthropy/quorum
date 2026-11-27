//! A settings page: every setting a program has and every key it answers, changed in place.

mod page;
mod value;

pub use page::Page;

use config::Field;
use keys::Rebindings;
use ratatui::style::Style;
use toml::Table;

/// One group of settings on the page: its title, the table it lives under in
/// the settings file, and its fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Section {
    pub title: &'static str,
    pub path: Vec<&'static str>,
    pub fields: Vec<Field>,
}

/// Every setting as it stands: the value in effect, what the settings file
/// says, what a flag set for this run alone, and what each would be with
/// nothing set. Each is keyed the way the settings file is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Values {
    pub effective: Table,
    pub written: Table,
    pub flagged: Table,
    pub defaults: Table,
}

/// The styles the page is drawn in, so it takes the colours of the program
/// that shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Styles {
    pub base: Style,
    pub muted: Style,
    pub key: Style,
    pub heading: Style,
    pub selected: Style,
    pub warning: Style,
}

/// What the page asks of the program after a key: write one setting, or put
/// it back to its default, take a new `[keys]` table, or close the page.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Setting { path: Vec<String>, value: Option<toml::Value> },
    Keys(Rebindings),
    Closed,
}
