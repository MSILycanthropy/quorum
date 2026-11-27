//! The page itself: which tab and row are chosen, and what a key or a click does there.

mod draw;

use keys::{Key, Keymap, Pending, Pressed, Rebindings, Recorded, Recording, Sequence, keymap};
use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::value::{Kind, at, shown, stepped, typed};
use crate::{Change, Section, Styles, Values};

const SCROLL: isize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Action {
    Down,
    Up,
    Top,
    Bottom,
    PageDown,
    PageUp,
    NextTab,
    PreviousTab,
    Choose,
    Next,
    Previous,
    Reset,
    Add,
    Clear,
    Close,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Tab {
    #[default]
    Settings,
    Keys,
}

/// A change being typed or pressed in: a setting's value as text, or new
/// keys for a command, in place of its keys or beside them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Editing {
    Text(String),
    Keys { adding: bool, recording: Recording },
}

/// One row the cursor can rest on: a setting, by its section and field, or
/// a binding, by its place in the keymap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Row {
    Setting { section: usize, field: usize },
    Binding(usize),
}

/// One line of a tab: a section's title, the gap before it, or a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Item {
    Heading(&'static str),
    Gap,
    Row(Row),
}

/// A settings page for one program: its settings by section, and every
/// command in its keymap with the keys it answers to.
#[derive(Clone, Debug)]
pub struct Page<C> {
    sections: Vec<Section>,
    values: Values,
    keymap: Keymap<C>,
    rebindings: Rebindings,
    before: Rebindings,
    styles: Styles,
    controls: Keymap<Action>,
    pending: Pending,
    tab: Tab,
    chosen: [usize; 2],
    top: usize,
    height: usize,
    tabs: [Rect; 2],
    list: Rect,
    editing: Option<Editing>,
    message: Option<String>,
}

impl<C: Clone> Page<C> {
    pub fn new(
        sections: Vec<Section>,
        values: Values,
        keymap: Keymap<C>,
        rebindings: Rebindings,
        styles: Styles,
    ) -> Self {
        Self {
            sections,
            values,
            keymap,
            before: rebindings.clone(),
            rebindings,
            styles,
            controls: controls(),
            pending: Pending::default(),
            tab: Tab::Settings,
            chosen: [0, 0],
            top: 0,
            height: 0,
            tabs: [Rect::default(); 2],
            list: Rect::default(),
            editing: None,
            message: None,
        }
    }

    pub fn update(&mut self, values: Values) {
        self.values = values;
    }

    pub fn notify(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    pub fn refuse(&mut self, message: impl Into<String>) {
        self.rebindings = self.before.clone();
        self.message = Some(message.into());
    }

    pub fn key(&mut self, key: Key) -> Option<Change> {
        self.message = None;
        if let Some(editing) = self.editing.take() {
            return self.edit(editing, key);
        }
        match self.controls.press(&mut self.pending, key, &[]) {
            Pressed::Command { command, count } => self.act(command, count.unwrap_or(1)),
            Pressed::Pending | Pressed::Nothing => None,
        }
    }

    pub fn mouse(&mut self, mouse: MouseEvent) -> Option<Change> {
        let at = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.message = None;
                self.editing = None;
                if let Some(tab) = [Tab::Settings, Tab::Keys]
                    .into_iter()
                    .zip(self.tabs)
                    .find_map(|(tab, area)| area.contains(at).then_some(tab))
                {
                    self.show(tab);
                    return None;
                }
                if !self.list.contains(at) {
                    return None;
                }
                let index = self.top + usize::from(mouse.row - self.list.y);
                let Some(Item::Row(clicked)) = self.items().get(index).copied() else {
                    return None;
                };
                let position = self.rows().iter().position(|row| *row == clicked)?;
                if position == self.chosen[self.tab as usize] {
                    return self.change(Action::Choose);
                }
                self.chosen[self.tab as usize] = position;
                None
            }
            MouseEventKind::ScrollDown => {
                self.move_by(SCROLL);
                None
            }
            MouseEventKind::ScrollUp => {
                self.move_by(-SCROLL);
                None
            }
            _ => None,
        }
    }

    fn show(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.top = 0;
        }
    }

    fn act(&mut self, action: Action, count: usize) -> Option<Change> {
        let page = self.height.max(2) - 1;
        match action {
            Action::Down => self.move_by(count.cast_signed()),
            Action::Up => self.move_by(-count.cast_signed()),
            Action::PageDown => self.move_by((page * count).cast_signed()),
            Action::PageUp => self.move_by(-(page * count).cast_signed()),
            Action::Top => self.chosen[self.tab as usize] = 0,
            Action::Bottom => self.chosen[self.tab as usize] = self.rows().len().saturating_sub(1),
            Action::NextTab | Action::PreviousTab => self.show(match self.tab {
                Tab::Settings => Tab::Keys,
                Tab::Keys => Tab::Settings,
            }),
            Action::Close => return Some(Change::Closed),
            Action::Choose
            | Action::Next
            | Action::Previous
            | Action::Reset
            | Action::Add
            | Action::Clear => {
                return self.change(action);
            }
        }

        None
    }

    fn change(&mut self, action: Action) -> Option<Change> {
        match (self.row()?, action) {
            (Row::Setting { section, field }, Action::Choose) => {
                let (field, value) = self.setting(section, field);
                match Kind::of(field, value) {
                    Kind::Switch | Kind::Choice => {
                        Some(self.set(section, field.name.clone(), stepped(field, value, true)))
                    }
                    Kind::Number | Kind::Text => {
                        self.editing = Some(Editing::Text(shown(value)));
                        None
                    }
                }
            }
            (Row::Setting { section, field }, Action::Next | Action::Previous) => {
                let (field, value) = self.setting(section, field);
                let stepped = stepped(field, value, action == Action::Next)?;
                Some(self.set(section, field.name.clone(), Some(stepped)))
            }
            (Row::Setting { section, field }, Action::Reset) => {
                let name = self.sections[section].fields[field].name.clone();
                Some(self.set(section, name, None))
            }
            (Row::Binding(_), Action::Choose | Action::Add) => {
                let adding = action == Action::Add;
                self.editing = Some(Editing::Keys { adding, recording: Recording::default() });
                None
            }
            (Row::Binding(index), Action::Reset) => {
                let mut candidate = self.rebindings.clone();
                candidate.commands.remove(self.keymap.bindings()[index].name);
                self.propose(candidate)
            }
            (Row::Binding(index), Action::Clear) => {
                let mut candidate = self.rebindings.clone();
                candidate
                    .commands
                    .insert(self.keymap.bindings()[index].name.to_owned(), Vec::new());
                self.propose(candidate)
            }
            _ => None,
        }
    }

    fn edit(&mut self, editing: Editing, key: Key) -> Option<Change> {
        match editing {
            Editing::Text(mut text) => match (key.code(), key.modifiers().is_empty()) {
                (KeyCode::Esc, true) => None,
                (KeyCode::Enter, true) => self.commit(text),
                (KeyCode::Backspace, true) => {
                    text.pop();
                    self.editing = Some(Editing::Text(text));
                    None
                }
                (KeyCode::Char(character), _)
                    if !key.modifiers().contains(KeyModifiers::CONTROL) =>
                {
                    text.push(character);
                    self.editing = Some(Editing::Text(text));
                    None
                }
                _ => {
                    self.editing = Some(Editing::Text(text));
                    None
                }
            },
            Editing::Keys { adding, mut recording } => match recording.press(key) {
                Recorded::Typing => {
                    self.editing = Some(Editing::Keys { adding, recording });
                    None
                }
                Recorded::Cancelled => None,
                Recorded::Done(sequence) => self.record(sequence, adding),
            },
        }
    }

    fn commit(&mut self, text: String) -> Option<Change> {
        let Some(Row::Setting { section, field }) = self.row() else {
            return None;
        };
        let (field, value) = self.setting(section, field);
        match typed(Kind::of(field, value), &text) {
            Ok(value) => Some(self.set(section, field.name.clone(), Some(value))),
            Err(message) => {
                self.message = Some(message);
                self.editing = Some(Editing::Text(text));
                None
            }
        }
    }

    fn record(&mut self, sequence: Sequence, adding: bool) -> Option<Change> {
        let Some(Row::Binding(index)) = self.row() else {
            return None;
        };
        let name = self.keymap.bindings()[index].name;
        let mut keys = if adding { self.keys_of(index).to_vec() } else { Vec::new() };
        if !keys.contains(&sequence) {
            keys.push(sequence);
        }
        let mut candidate = self.rebindings.clone();
        candidate.commands.insert(name.to_owned(), keys);

        self.propose(candidate)
    }

    fn propose(&mut self, candidate: Rebindings) -> Option<Change> {
        match self.keymap.clone().rebound(&candidate) {
            Ok(_) => {
                self.before = std::mem::replace(&mut self.rebindings, candidate.clone());
                Some(Change::Keys(candidate))
            }
            Err(error) => {
                self.message = Some(error.to_string());
                None
            }
        }
    }

    fn set(&self, section: usize, name: String, value: Option<toml::Value>) -> Change {
        let mut path: Vec<String> =
            self.sections[section].path.iter().map(|key| (*key).to_owned()).collect();
        path.push(name);

        Change::Setting { path, value }
    }

    fn setting(&self, section: usize, field: usize) -> (&config::Field, Option<&toml::Value>) {
        let section = &self.sections[section];
        let field = &section.fields[field];

        (field, at(&self.values.effective, &section.path, &field.name))
    }

    fn keys_of(&self, index: usize) -> &[Sequence] {
        let binding = &self.keymap.bindings()[index];
        self.rebindings.commands.get(binding.name).map_or(binding.keys.as_slice(), Vec::as_slice)
    }

    fn move_by(&mut self, rows: isize) {
        let last = self.rows().len().saturating_sub(1);
        let chosen = &mut self.chosen[self.tab as usize];
        *chosen = chosen.saturating_add_signed(rows).min(last);
    }

    fn row(&self) -> Option<Row> {
        self.rows().get(self.chosen[self.tab as usize]).copied()
    }

    fn rows(&self) -> Vec<Row> {
        self.items()
            .into_iter()
            .filter_map(|item| match item {
                Item::Row(row) => Some(row),
                Item::Heading(_) | Item::Gap => None,
            })
            .collect()
    }

    fn items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        let heading = |items: &mut Vec<Item>, title| {
            if !items.is_empty() {
                items.push(Item::Gap);
            }
            items.push(Item::Heading(title));
        };
        match self.tab {
            Tab::Settings => {
                for (section, group) in self.sections.iter().enumerate() {
                    heading(&mut items, group.title);
                    items.extend(
                        (0..group.fields.len())
                            .map(|field| Item::Row(Row::Setting { section, field })),
                    );
                }
            }
            Tab::Keys => {
                let mut last = None;
                for (index, binding) in self.keymap.bindings().iter().enumerate() {
                    if last != Some(binding.section) {
                        heading(&mut items, binding.section);
                        last = Some(binding.section);
                    }
                    items.push(Item::Row(Row::Binding(index)));
                }
            }
        }

        items
    }
}

fn controls() -> Keymap<Action> {
    keymap! {
        "Page" {
            "down" => Action::Down, "Down", ["j", "↓"];
            "up" => Action::Up, "Up", ["k", "↑"];
            "top" => Action::Top, "Top", ["gg", "Home"];
            "bottom" => Action::Bottom, "Bottom", ["G", "End"];
            "page-down" => Action::PageDown, "Down a page", ["PgDn", "C-d"];
            "page-up" => Action::PageUp, "Up a page", ["PgUp", "C-u"];
            "next-tab" => Action::NextTab, "Next tab", ["Tab"];
            "previous-tab" => Action::PreviousTab, "Previous tab", ["S-Tab"];
            "choose" => Action::Choose, "Change", ["Enter", "Space"];
            "next" => Action::Next, "Step forward", ["l", "→"];
            "previous" => Action::Previous, "Step back", ["h", "←"];
            "reset" => Action::Reset, "Back to the default", ["Backspace", "Delete"];
            "add" => Action::Add, "Add keys", ["a"];
            "clear" => Action::Clear, "No keys", ["d"];
            "close" => Action::Close, "Close", ["q", "Esc"];
        }
    }
    .expect("the page's own keys are spelt right and do not clash")
}
