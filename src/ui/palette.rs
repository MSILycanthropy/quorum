//! The keyboard-driven overlay for commands and workspace targets.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::command::{PaletteEntry, PaletteModel, PaletteTarget};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteAction {
    None,
    Close,
    Execute(PaletteTarget),
}

#[derive(Clone, Debug, Default)]
pub struct Palette {
    entries: Vec<PaletteEntry>,
    query: String,
    selected: usize,
    model: PaletteModel,
}

impl Palette {
    pub fn open(&mut self, entries: Vec<PaletteEntry>) {
        self.entries = entries;
        self.query.clear();
        self.selected = 0;
    }

    pub fn is_open(&self) -> bool {
        !self.entries.is_empty()
    }

    pub fn close(&mut self) {
        self.entries.clear();
        self.query.clear();
        self.selected = 0;
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn selected(&self) -> Option<&PaletteEntry> {
        self.model.rank(&self.query, &self.entries).get(self.selected).copied()
    }

    pub fn key(&mut self, key: KeyEvent) -> PaletteAction {
        if !self.is_open() {
            return PaletteAction::None;
        }
        match key.code {
            KeyCode::Esc => {
                self.close();
                PaletteAction::Close
            }
            KeyCode::Enter => {
                let target = self.selected().map(|entry| entry.target().clone());
                self.close();
                target.map_or(PaletteAction::None, PaletteAction::Execute)
            }
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                PaletteAction::None
            }
            KeyCode::Down => {
                let count = self.model.rank(&self.query, &self.entries).len();
                self.selected = self.selected.saturating_add(1).min(count.saturating_sub(1));
                PaletteAction::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
                PaletteAction::None
            }
            KeyCode::Char(character) => {
                self.query.push(character);
                self.selected = 0;
                PaletteAction::None
            }
            _ => PaletteAction::None,
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        if !self.is_open() {
            return;
        }
        let area = frame.area();
        let width = area.width.saturating_sub(8).min(90);
        let height = area.height.saturating_sub(4).min(16);
        let left = area.x + area.width.saturating_sub(width) / 2;
        let top = area.y + area.height.saturating_sub(height) / 2;
        let popup = Rect::new(left, top, width, height);
        let buffer = frame.buffer_mut();
        buffer.set_style(popup, Style::default().bg(Color::Black).fg(Color::White));
        write(
            buffer,
            popup.x + 1,
            popup.y,
            popup.width.saturating_sub(2),
            &format!("> {}", self.query),
        );

        for (offset, entry) in self.model.rank(&self.query, &self.entries).iter().enumerate() {
            let row = u16::try_from(offset + 1).unwrap_or(u16::MAX);
            if row >= popup.height {
                break;
            }
            let selected = offset == self.selected;
            let style = if selected {
                Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            buffer.set_style(Rect::new(popup.x, popup.y + row, popup.width, 1), style);
            let shortcuts = entry
                .shortcuts()
                .iter()
                .map(|shortcut| shortcut.display())
                .collect::<Vec<_>>()
                .join(", ");
            let label = if shortcuts.is_empty() {
                entry.label().to_owned()
            } else {
                format!("{}  {}", entry.label(), shortcuts)
            };
            write_styled(
                buffer,
                popup.x + 1,
                popup.y + row,
                popup.width.saturating_sub(2),
                &label,
                style,
            );
        }
    }
}

fn write(buffer: &mut Buffer, x: u16, y: u16, width: u16, text: &str) {
    write_styled(buffer, x, y, width, text, Style::default());
}

fn write_styled(buffer: &mut Buffer, x: u16, y: u16, width: u16, text: &str, style: Style) {
    buffer.set_stringn(x, y, text, usize::from(width), style);
}
