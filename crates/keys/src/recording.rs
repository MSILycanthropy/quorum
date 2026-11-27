//! Keys pressed one by one for a binding, until Enter finishes them or Esc gives up.

use std::fmt;

use ratatui::crossterm::event::KeyCode;

use crate::key::shown;
use crate::{Key, Sequence};

/// The keys pressed so far for a binding being recorded. Enter finishes them,
/// Esc gives up, and Backspace takes the last one back, so none of those three
/// can be recorded; they can still be written by name in a settings file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Recording {
    keys: Vec<Key>,
}

/// What a key pressed while recording amounted to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Recorded {
    Typing,
    Done(Sequence),
    Cancelled,
}

impl Recording {
    pub fn press(&mut self, key: Key) -> Recorded {
        let plain = key.modifiers().is_empty();
        match key.code() {
            KeyCode::Esc if plain => {
                self.keys.clear();
                Recorded::Cancelled
            }
            KeyCode::Enter if plain => match Sequence::new(std::mem::take(&mut self.keys)) {
                Ok(sequence) => Recorded::Done(sequence),
                Err(_) => Recorded::Typing,
            },
            KeyCode::Backspace if plain => {
                self.keys.pop();
                Recorded::Typing
            }
            _ => {
                self.keys.push(key);
                Recorded::Typing
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

impl fmt::Display for Recording {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&shown(&self.keys))
    }
}
