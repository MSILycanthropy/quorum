//! What the terminal tells a TUI.

use ratatui::crossterm::event::{self, KeyEvent, KeyEventKind, MouseEvent};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize,
    Paste(String),
}

impl Event {
    pub(crate) fn from_crossterm(event: event::Event) -> Option<Self> {
        match event {
            event::Event::Key(key) if key.kind == KeyEventKind::Press => Some(Self::Key(key)),
            event::Event::Mouse(mouse) => Some(Self::Mouse(mouse)),
            event::Event::Resize(_, _) => Some(Self::Resize),
            event::Event::Paste(text) => Some(Self::Paste(text)),
            _ => None,
        }
    }
}
