//! The overlay: what has been typed, and the entries that match it.

use hints::Hint;
use keys::{Key, Keymap, Pending, Pressed, keymap};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Widget};

use crate::rank::ranked;
use crate::{Entry, Styles};

/// What a key amounted to. After `Typed` the program may put rows of its own
/// above the matches, worked out from the query, or have the entries matched
/// against something other than the query.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Outcome<T> {
    Nothing,
    Typed,
    Chosen { target: T, query: String },
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Action {
    Choose,
    Close,
    Up,
    Down,
}

/// A box to type in over a list to pick from: a title, what has been typed,
/// and the entries that match it, best first. Entries keep the order they
/// were given in when they match equally well.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Picker<T> {
    title: String,
    entries: Vec<Entry<T>>,
    leading: Vec<Entry<T>>,
    needle: Option<String>,
    ranked: Vec<usize>,
    query: String,
    selected: usize,
    open: bool,
    controls: Keymap<Action>,
    pending: Pending,
    styles: Styles,
}

impl<T> Default for Picker<T> {
    fn default() -> Self {
        Self::new(Styles::default())
    }
}

impl<T> Picker<T> {
    pub fn new(styles: Styles) -> Self {
        Self {
            title: String::new(),
            entries: Vec::new(),
            leading: Vec::new(),
            needle: None,
            ranked: Vec::new(),
            query: String::new(),
            selected: 0,
            open: false,
            controls: controls(),
            pending: Pending::default(),
            styles,
        }
    }

    pub fn open(&mut self, title: impl Into<String>, entries: Vec<Entry<T>>) {
        self.title = title.into();
        self.entries = entries;
        self.query.clear();
        self.open = true;
        self.forget();
    }

    pub fn close(&mut self) {
        self.entries.clear();
        self.query.clear();
        self.open = false;
        self.forget();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn entries(&self) -> &[Entry<T>] {
        &self.entries
    }

    pub fn selected(&self) -> Option<&Entry<T>> {
        self.shown().nth(self.selected)
    }

    pub fn lead(&mut self, entries: Vec<Entry<T>>) {
        self.leading = entries;
        self.selected = 0;
    }

    pub fn search(&mut self, needle: &str) {
        self.needle = Some(needle.to_owned());
        self.rank();
    }

    pub fn hints(&self) -> Vec<Hint> {
        self.controls.bindings().iter().filter_map(|binding| Hint::of(binding, &[])).collect()
    }

    pub fn key(&mut self, key: Key) -> Outcome<T> {
        if !self.open {
            return Outcome::Nothing;
        }
        let plain = !key.modifiers().intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code() {
            KeyCode::Char(character) if plain => {
                self.query.push(character);
                return self.typed();
            }
            KeyCode::Backspace if plain => {
                self.query.pop();
                return self.typed();
            }
            _ => {}
        }

        match self.controls.press(&mut self.pending, key, &[]) {
            Pressed::Command { command: Action::Choose, .. } => self.choose(),
            Pressed::Command { command: Action::Close, .. } => {
                self.close();
                Outcome::Closed
            }
            Pressed::Command { command: Action::Up, .. } => {
                self.selected = self.selected.saturating_sub(1);
                Outcome::Nothing
            }
            Pressed::Command { command: Action::Down, .. } => {
                let last = self.leading.len() + self.ranked.len();
                self.selected = (self.selected + 1).min(last.saturating_sub(1));
                Outcome::Nothing
            }
            Pressed::Pending | Pressed::Nothing => Outcome::Nothing,
        }
    }

    fn typed(&mut self) -> Outcome<T> {
        self.forget();

        Outcome::Typed
    }

    fn forget(&mut self) {
        self.leading.clear();
        self.needle = None;
        self.pending.clear();
        self.rank();
    }

    fn rank(&mut self) {
        let needle = self.needle.as_deref().unwrap_or(&self.query);
        self.ranked = ranked(needle, &self.entries);
        self.selected = 0;
    }

    fn choose(&mut self) -> Outcome<T> {
        let query = std::mem::take(&mut self.query);
        let leading = self.leading.len();
        let target = if self.selected < leading {
            Some(self.leading.swap_remove(self.selected).target)
        } else {
            let index = self.ranked.get(self.selected - leading).copied();
            index.map(|index| self.entries.swap_remove(index).target)
        };
        self.close();

        match target {
            Some(target) => Outcome::Chosen { target, query },
            None => Outcome::Closed,
        }
    }

    fn shown(&self) -> impl Iterator<Item = &Entry<T>> {
        self.leading.iter().chain(self.ranked.iter().map(|index| &self.entries[*index]))
    }

    fn row(&self, buffer: &mut Buffer, area: Rect, entry: &Entry<T>, style: Style) {
        buffer.set_style(area, style);
        let detail = u16::try_from(Span::raw(entry.detail.as_str()).width()).unwrap_or(0);
        let label_room = area.width.saturating_sub(detail + 4);
        buffer.set_stringn(area.x + 1, area.y, &entry.label, usize::from(label_room), style);
        if detail > 0 && detail + 4 < area.width {
            let key = style.patch(Style { bg: None, ..self.styles.key });
            buffer.set_string(area.x + area.width - detail - 1, area.y, &entry.detail, key);
        }
    }
}

impl<T> Widget for &Picker<T> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if !self.open {
            return;
        }
        let width = area.width.saturating_sub(4).min(80);
        let count = self.leading.len() + self.ranked.len();
        let wanted = u16::try_from(count.max(1) + 1).unwrap_or(u16::MAX);
        let height = area.height.saturating_sub(4).min(18).min(wanted);
        let left = area.x + area.width.saturating_sub(width) / 2;
        let top = area.y + area.height.saturating_sub(height) / 3;
        let popup = Rect::new(left, top, width, height);
        Clear.render(popup, buffer);

        let base = self.styles.base;
        buffer.set_style(popup, base);
        let inner = popup.width.saturating_sub(2);

        let (x, _) = buffer.set_stringn(
            popup.x + 1,
            popup.y,
            format!("{} › ", self.title),
            usize::from(inner),
            self.styles.muted,
        );
        let typed = popup.x + 1 + inner;
        let (cursor, _) =
            buffer.set_stringn(x, popup.y, &self.query, usize::from(typed.saturating_sub(x)), base);
        if cursor < typed {
            buffer.set_string(cursor, popup.y, " ", base.add_modifier(Modifier::REVERSED));
        }

        let room = usize::from(popup.height.saturating_sub(1));
        let first = (self.selected + 1).saturating_sub(room);
        for (offset, entry) in self.shown().enumerate().skip(first).take(room) {
            let y = popup.y + 1 + u16::try_from(offset - first).unwrap_or(u16::MAX);
            let style = if offset == self.selected { self.styles.selected } else { base };
            self.row(buffer, Rect::new(popup.x, y, popup.width, 1), entry, style);
        }
        if count == 0 && popup.height > 1 {
            buffer.set_stringn(
                popup.x + 1,
                popup.y + 1,
                "no matches",
                usize::from(inner),
                self.styles.muted,
            );
        }
    }
}

fn controls() -> Keymap<Action> {
    keymap! {
        "Picker" {
            "choose" => Action::Choose, "Choose", ["Enter"], hint: "choose";
            "close" => Action::Close, "Close", ["Esc", "C-c"], hint: "close", pinned: true;
            "up" => Action::Up, "Up", ["↑", "S-Tab", "C-p", "C-k"];
            "down" => Action::Down, "Down", ["↓", "Tab", "C-n", "C-j"];
        }
    }
    .expect("the picker's own keys are spelt right and do not clash")
}
