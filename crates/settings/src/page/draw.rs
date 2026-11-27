//! How the page is drawn: its tabs, its rows, and the help and keys along the bottom.

use hints::{Footer, hints};
use keys::Sequence;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Widget;
use toml::Value;

use super::{Editing, Item, Page, Row, Tab};
use crate::value::{Kind, at, shown};

const RULE: &str = "─";

impl<C: Clone> Widget for &mut Page<C> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, self.styles.base);
        if area.height < 6 || area.width < 12 {
            return;
        }
        let left = area.x + 1;
        let width = usize::from(area.width - 2);

        self.draw_tabs(buffer, area);
        rule(buffer, left, area.y + 1, width, ink(self.styles.muted));

        let list = Rect::new(area.x, area.y + 2, area.width, area.height - 5);
        self.list = list;
        self.height = usize::from(list.height);
        let items = self.items();
        let chosen = self.row();
        let at = items.iter().position(|item| Some(*item) == chosen.map(Item::Row)).unwrap_or(0);
        if at < self.top {
            self.top = at.saturating_sub(1);
        }
        if at >= self.top + self.height {
            self.top = at + 1 - self.height;
        }

        let name_width = self.name_width();
        for (y, item) in (list.y..list.bottom()).zip(items.iter().skip(self.top)) {
            match item {
                Item::Heading(title) => {
                    let title = format!("{title} ");
                    let rest = width.saturating_sub(Span::raw(title.as_str()).width());
                    let line = [
                        (title, ink(self.styles.heading)),
                        (RULE.repeat(rest), ink(self.styles.muted)),
                    ];
                    put(buffer, left, y, width, &line);
                }
                Item::Gap => {}
                Item::Row(row) => {
                    let selected = Some(*row) == chosen;
                    let fill = if selected { self.styles.selected } else { self.styles.base };
                    buffer.set_style(Rect::new(area.x, y, area.width, 1), fill);
                    put(buffer, left, y, width, &self.pieces(*row, selected, name_width));
                }
            }
        }

        let bottom = area.bottom() - 1;
        rule(buffer, left, bottom - 2, width, ink(self.styles.muted));
        put(buffer, left, bottom - 1, width, &self.description());
        self.footer().render(Rect::new(area.x, bottom, area.width, 1), buffer);
    }
}

impl<C: Clone> Page<C> {
    fn draw_tabs(&mut self, buffer: &mut Buffer, area: Rect) {
        let mut x = area.x + 1;
        for (index, (tab, title)) in
            [(Tab::Settings, " Settings "), (Tab::Keys, " Keys ")].into_iter().enumerate()
        {
            let style = if tab == self.tab {
                self.styles.selected.add_modifier(Modifier::BOLD)
            } else {
                self.styles.muted
            };
            let wide = u16::try_from(Span::raw(title).width()).unwrap_or(u16::MAX);
            buffer.set_stringn(
                x,
                area.y,
                title,
                usize::from(area.right().saturating_sub(x)),
                style,
            );
            self.tabs[index] = Rect::new(x, area.y, wide, 1).intersection(area);
            x = x.saturating_add(wide + 1);
        }
    }

    fn default(&self, section: usize, name: &str) -> Option<&Value> {
        at(&self.values.defaults, &self.sections[section].path, name)
    }

    fn name_width(&self) -> usize {
        let width = |text: &str| Span::raw(text).width();
        match self.tab {
            Tab::Settings => self
                .sections
                .iter()
                .flat_map(|section| &section.fields)
                .map(|field| width(&field.name))
                .max(),
            Tab::Keys => self.keymap.bindings().iter().map(|binding| width(binding.label)).max(),
        }
        .unwrap_or(0)
    }

    fn pieces(&self, row: Row, selected: bool, name_width: usize) -> Vec<(String, Style)> {
        let editing = self.editing.as_ref().filter(|_| selected);
        let marker =
            |marked: bool| (if marked { "• " } else { "  " }.to_owned(), ink(self.styles.key));
        match row {
            Row::Setting { section, field } => {
                let group = &self.sections[section];
                let (field, value) = self.setting(section, field);
                let written = at(&self.values.written, &group.path, &field.name)
                    .is_some_and(|written| Some(written) != self.default(section, &field.name));
                let flagged = at(&self.values.flagged, &group.path, &field.name).is_some();
                let mut pieces = vec![
                    marker(written),
                    (format!("{:<name_width$}   ", field.name), ink(self.styles.base)),
                ];
                let mut value = match editing {
                    Some(Editing::Text(text)) => vec![(format!("{text}▏"), ink(self.styles.key))],
                    _ => self.value(Kind::of(field, value), value, selected),
                };
                if flagged {
                    for (_, style) in &mut value {
                        *style = ink(self.styles.warning);
                    }
                }
                pieces.extend(value);
                pieces
            }
            Row::Binding(index) => {
                let binding = &self.keymap.bindings()[index];
                let rebound = self.keys_of(index) != binding.keys.as_slice();
                let mut pieces = vec![
                    marker(rebound),
                    (format!("{:<name_width$}   ", binding.label), ink(self.styles.base)),
                ];
                match editing {
                    Some(Editing::Keys { recording, .. }) if recording.is_empty() => {
                        pieces.push(("press the keys…".to_owned(), ink(self.styles.muted)));
                    }
                    Some(Editing::Keys { recording, .. }) => {
                        pieces.push((format!("{recording}…"), ink(self.styles.key)));
                    }
                    _ => pieces.extend(self.keys(self.keys_of(index))),
                }
                pieces
            }
        }
    }

    fn value(&self, kind: Kind, value: Option<&Value>, selected: bool) -> Vec<(String, Style)> {
        let key = ink(self.styles.key);
        let muted = ink(self.styles.muted);
        match (kind, value) {
            (Kind::Switch, Some(Value::Boolean(true))) => vec![("● on".to_owned(), key)],
            (Kind::Switch, _) => vec![("○ off".to_owned(), muted)],
            (Kind::Choice, value) if selected => {
                vec![("‹ ".to_owned(), muted), (shown(value), key), (" ›".to_owned(), muted)]
            }
            (Kind::Text, value) if shown(value).is_empty() => vec![("default".to_owned(), muted)],
            (Kind::Choice | Kind::Number | Kind::Text, value) => vec![(shown(value), key)],
        }
    }

    fn keys(&self, keys: &[Sequence]) -> Vec<(String, Style)> {
        if keys.is_empty() {
            return vec![("none".to_owned(), ink(self.styles.muted))];
        }
        let mut pieces = Vec::new();
        for (index, sequence) in keys.iter().enumerate() {
            if index > 0 {
                pieces.push(("  ".to_owned(), ink(self.styles.base)));
            }
            pieces.push((sequence.to_string(), ink(self.styles.key)));
        }

        pieces
    }

    fn description(&self) -> Vec<(String, Style)> {
        let muted = ink(self.styles.muted);
        if let Some(message) = &self.message {
            return vec![(message.clone(), ink(self.styles.warning))];
        }
        match self.row() {
            Some(Row::Setting { section, field }) => {
                let group = &self.sections[section];
                let field = &group.fields[field];
                let mut pieces = vec![(field.help.clone(), ink(self.styles.base))];
                let default = shown(self.default(section, &field.name));
                if !default.is_empty() {
                    pieces.push(("   default ".to_owned(), muted));
                    pieces.push((default, ink(self.styles.key)));
                }
                if at(&self.values.flagged, &group.path, &field.name).is_some() {
                    pieces.push((
                        format!("   --{} on the command line wins for this run", field.name),
                        ink(self.styles.warning),
                    ));
                }
                pieces
            }
            Some(Row::Binding(index)) => {
                let binding = &self.keymap.bindings()[index];
                let mut pieces = vec![
                    ("[keys] ".to_owned(), muted),
                    (binding.name.to_owned(), ink(self.styles.key)),
                ];
                if self.keys_of(index) != binding.keys.as_slice() {
                    pieces.push(("   default ".to_owned(), muted));
                    pieces.extend(self.keys(&binding.keys));
                }
                pieces
            }
            None => Vec::new(),
        }
    }

    fn footer(&self) -> Footer {
        let hints = match (&self.editing, self.tab) {
            (Some(Editing::Text(_)), _) => hints![
                "type the value",
                "Enter" => "keep it",
                "Esc" => "leave it" pinned,
            ],
            (Some(Editing::Keys { .. }), _) => hints![
                "press the keys",
                "Enter" => "done",
                "Backspace" => "undo",
                "Esc" => "give up" pinned,
            ],
            (None, Tab::Settings) => hints![
                "Enter" => "change",
                "h l" => "step",
                "Backspace" => "default",
                "Tab" => "keys",
                "q" => "close" pinned,
            ],
            (None, Tab::Keys) => hints![
                "Enter" => "new keys",
                "a" => "add keys",
                "d" => "no keys",
                "Backspace" => "default",
                "Tab" => "settings",
                "q" => "close" pinned,
            ],
        };
        let styles = hints::Styles {
            base: self.styles.base,
            muted: self.styles.muted,
            key: self.styles.key,
        };

        Footer { hints, styles, ..Footer::default() }
    }
}

fn put(buffer: &mut Buffer, left: u16, y: u16, width: usize, pieces: &[(String, Style)]) {
    let end = left.saturating_add(u16::try_from(width).unwrap_or(u16::MAX));
    let mut x = left;
    for (text, style) in pieces {
        let room = usize::from(end.saturating_sub(x));
        (x, _) = buffer.set_stringn(x, y, text, room, *style);
    }
}

fn rule(buffer: &mut Buffer, left: u16, y: u16, width: usize, style: Style) {
    buffer.set_stringn(left, y, RULE.repeat(width), width, style);
}

fn ink(style: Style) -> Style {
    Style { bg: None, ..style }
}
