//! The keyboard-driven overlay for running a command or jumping to a file by name.

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Clear, Widget};
use unicode_width::UnicodeWidthStr;

use crate::color::Rgb;
use crate::command::{PaletteEntry, PaletteModel, PaletteTarget, located};
use crate::theme::Tints;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteAction {
    None,
    Close,
    Execute(PaletteTarget),
}

#[derive(Clone, Debug)]
pub struct Palette {
    entries: Vec<PaletteEntry>,
    title: &'static str,
    query: String,
    selected: usize,
    model: PaletteModel,
    tints: Tints,
}

impl Default for Palette {
    fn default() -> Self {
        Self::new(Tints::default())
    }
}

impl Palette {
    pub fn new(tints: Tints) -> Self {
        Self {
            entries: Vec::new(),
            title: "",
            query: String::new(),
            selected: 0,
            model: PaletteModel,
            tints,
        }
    }
}

impl Palette {
    pub fn open(&mut self, title: &'static str, entries: Vec<PaletteEntry>) {
        self.entries = entries;
        self.title = title;
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
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                self.close();
                PaletteAction::Close
            }
            KeyCode::Char('c') if control => {
                self.close();
                PaletteAction::Close
            }
            KeyCode::Enter => {
                let line = located(&self.query).1;
                let target = self.selected().map(|entry| match &entry.target {
                    PaletteTarget::File { path, .. } => {
                        PaletteTarget::File { path: path.clone(), line }
                    }
                    target @ PaletteTarget::Command(_) => target.clone(),
                });
                self.close();
                target.map_or(PaletteAction::Close, PaletteAction::Execute)
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.selected = self.selected.saturating_sub(1);
                PaletteAction::None
            }
            KeyCode::Char('p' | 'k') if control => {
                self.selected = self.selected.saturating_sub(1);
                PaletteAction::None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.down();
                PaletteAction::None
            }
            KeyCode::Char('n' | 'j') if control => {
                self.down();
                PaletteAction::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
                PaletteAction::None
            }
            KeyCode::Char(character) if !control => {
                self.query.push(character);
                self.selected = 0;
                PaletteAction::None
            }
            _ => PaletteAction::None,
        }
    }

    fn down(&mut self) {
        let count = self.model.rank(&self.query, &self.entries).len();
        self.selected = self.selected.saturating_add(1).min(count.saturating_sub(1));
    }

    fn row(&self, buffer: &mut Buffer, area: Rect, entry: &PaletteEntry, style: Style) {
        buffer.set_style(area, style);
        let keys = u16::try_from(entry.keys.width()).unwrap_or(0);
        let label_room = area.width.saturating_sub(keys + 4);
        buffer.set_stringn(area.x + 1, area.y, &entry.label, usize::from(label_room), style);
        if keys > 0 && keys + 4 < area.width {
            buffer.set_string(
                area.x + area.width - keys - 1,
                area.y,
                &entry.keys,
                style.fg(color(self.tints.attention)),
            );
        }
    }
}

impl Widget for &Palette {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if !self.is_open() {
            return;
        }
        let width = area.width.saturating_sub(4).min(80);
        let ranked = self.model.rank(&self.query, &self.entries);
        let wanted = u16::try_from(ranked.len().max(1) + 1).unwrap_or(u16::MAX);
        let height = area.height.saturating_sub(4).min(18).min(wanted);
        let left = area.x + area.width.saturating_sub(width) / 2;
        let top = area.y + area.height.saturating_sub(height) / 3;
        let popup = Rect::new(left, top, width, height);
        Clear.render(popup, buffer);

        let base = Style::new().bg(color(self.tints.filler)).fg(color(self.tints.foreground));
        let muted = base.fg(color(self.tints.muted));
        buffer.set_style(popup, base);
        let inner = popup.width.saturating_sub(2);

        let (x, _) = buffer.set_stringn(
            popup.x + 1,
            popup.y,
            format!("{} › ", self.title),
            usize::from(inner),
            muted,
        );
        let typed = popup.x + 1 + inner;
        let (cursor, _) =
            buffer.set_stringn(x, popup.y, &self.query, usize::from(typed.saturating_sub(x)), base);
        if cursor < typed {
            buffer.set_string(cursor, popup.y, " ", base.add_modifier(Modifier::REVERSED));
        }

        let room = usize::from(popup.height.saturating_sub(1));
        let first = (self.selected + 1).saturating_sub(room);
        for (offset, entry) in ranked.iter().enumerate().skip(first).take(room) {
            let y = popup.y + 1 + u16::try_from(offset - first).unwrap_or(u16::MAX);
            let selected = offset == self.selected;
            let style = if selected { base.bg(color(self.tints.gutter)) } else { base };
            self.row(buffer, Rect::new(popup.x, y, popup.width, 1), entry, style);
        }
        if ranked.is_empty() {
            buffer.set_stringn(popup.x + 1, popup.y + 1, "no matches", usize::from(inner), muted);
        }
    }
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
