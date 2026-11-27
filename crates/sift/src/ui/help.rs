//! Every key, grouped, drawn straight from the command catalog.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Clear, Widget};
use unicode_width::UnicodeWidthStr;

use crate::command::{Binding, Catalog, Command};
use crate::theme::Tints;

const GAP: usize = 3;
const CLOSING: &str = "any key closes";

/// The overlay listing every binding, open or not, and how far it is
/// scrolled when it is taller than the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Help {
    open: bool,
    scrolled: usize,
    overflow: usize,
    page: usize,
    tints: Tints,
}

impl Default for Help {
    fn default() -> Self {
        Self::new(Tints::default())
    }
}

impl Help {
    pub const fn new(tints: Tints) -> Self {
        Self { open: false, scrolled: 0, overflow: 0, page: 0, tints }
    }
}

/// One category and its bindings, as the rows it takes up.
struct Section<'catalog> {
    title: &'static str,
    bindings: Vec<&'catalog Binding>,
}

impl Section<'_> {
    fn height(&self) -> usize {
        self.bindings.len() + 1
    }
}

/// One row of a help column: a section's title, a binding, or the gap between sections.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Entry<'catalog> {
    Heading(&'static str),
    Binding(&'catalog Binding),
    Gap,
}

impl Help {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.scrolled = 0;
    }

    pub fn scroll(&mut self, command: Command, count: Option<usize>) -> bool {
        if self.overflow == 0 {
            return false;
        }
        let times = count.unwrap_or(1);
        let page = self.page.max(2);
        let target = match command {
            Command::NextLine | Command::ScrollLineDown => self.scrolled.saturating_add(times),
            Command::PreviousLine | Command::ScrollLineUp => self.scrolled.saturating_sub(times),
            Command::HalfPageDown => self.scrolled.saturating_add(page / 2 * times),
            Command::HalfPageUp => self.scrolled.saturating_sub(page / 2 * times),
            Command::PageDown => self.scrolled.saturating_add((page - 1) * times),
            Command::PageUp => self.scrolled.saturating_sub((page - 1) * times),
            Command::Top => 0,
            Command::Bottom => self.overflow,
            _ => return false,
        };
        self.scrolled = target.min(self.overflow);

        true
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn draw(&mut self, area: Rect, buffer: &mut Buffer, catalog: &Catalog) {
        if !self.open {
            return;
        }

        let titles = catalog.sections();
        let sections: Vec<Section> = titles
            .iter()
            .map(|&title| Section {
                title,
                bindings: catalog
                    .bindings()
                    .iter()
                    .filter(|binding| binding.section == title && !binding.keys.is_empty())
                    .collect(),
            })
            .filter(|section| !section.bindings.is_empty())
            .collect();
        let keys_width =
            catalog.bindings().iter().map(|binding| binding.shown().width()).max().unwrap_or(0);
        let label_width = catalog
            .bindings()
            .iter()
            .map(|binding| binding.label.width())
            .chain(titles.iter().map(|title| title.width()))
            .max()
            .unwrap_or(0);
        let column_width = keys_width + 2 + label_width;

        let room_wide = usize::from(area.width.saturating_sub(4));
        let room_tall = usize::from(area.height.saturating_sub(4));
        let most_columns = ((room_wide + GAP) / (column_width + GAP)).max(1);
        let whole = (1..=most_columns)
            .map(|count| flow(&sections, count))
            .find(|columns| tallest(columns) <= room_tall);
        let spilling = spilled(&sections, room_tall);
        let columns: Vec<Vec<Entry>> = match whole {
            Some(columns) => columns.into_iter().map(|column| entries(&column)).collect(),
            None if spilling.len() <= most_columns => spilling,
            None => flow(&sections, most_columns).iter().map(|column| entries(column)).collect(),
        };
        let tallest = columns.iter().map(Vec::len).max().unwrap_or(0);
        self.overflow = tallest.saturating_sub(room_tall);
        self.page = room_tall;
        self.scrolled = self.scrolled.min(self.overflow);

        let width = columns.len() * column_width + (columns.len() - 1) * GAP + 2;
        let height = tallest.min(room_tall.max(1)) + 2;
        let width = u16::try_from(width).unwrap_or(u16::MAX).min(area.width);
        let height = u16::try_from(height).unwrap_or(u16::MAX).min(area.height);
        let popup = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        Clear.render(popup, buffer);

        let painter = Painter { tints: self.tints, popup, keys_width, label_width };
        buffer.set_style(popup, painter.base());

        for (index, column) in columns.iter().enumerate() {
            let x = popup.x + 1 + u16::try_from(index * (column_width + GAP)).unwrap_or(u16::MAX);
            painter.column(buffer, x, column.get(self.scrolled..).unwrap_or_default());
        }
        let closing = if self.overflow == 0 {
            CLOSING.to_owned()
        } else {
            let first = |command| {
                catalog
                    .bindings()
                    .iter()
                    .find(|binding| binding.command == command)
                    .and_then(|binding| binding.keys.first())
                    .map(ToString::to_string)
            };
            let down = first(Command::NextLine).unwrap_or_else(|| "↓".to_owned());
            let up = first(Command::PreviousLine).unwrap_or_else(|| "↑".to_owned());
            format!("{down} {up} scroll · any other key closes")
        };
        painter.closing(buffer, &closing);
    }
}

fn flow<'section, 'catalog>(
    sections: &'section [Section<'catalog>],
    count: usize,
) -> Vec<Vec<&'section Section<'catalog>>> {
    let total: usize = sections.iter().map(|section| section.height() + 1).sum();
    let target = total.div_ceil(count);
    let mut columns: Vec<Vec<&Section>> = vec![Vec::new()];
    let mut filled = 0;

    for section in sections {
        let last = columns.last().expect("columns starts with one column");
        if !last.is_empty() && filled + section.height() > target && columns.len() < count {
            columns.push(Vec::new());
            filled = 0;
        }
        columns.last_mut().expect("columns starts with one column").push(section);
        filled += section.height() + 1;
    }

    columns
}

fn entries<'catalog>(column: &[&Section<'catalog>]) -> Vec<Entry<'catalog>> {
    let mut entries = Vec::new();
    for section in column {
        if !entries.is_empty() {
            entries.push(Entry::Gap);
        }
        entries.push(Entry::Heading(section.title));
        entries.extend(section.bindings.iter().map(|binding| Entry::Binding(binding)));
    }

    entries
}

fn spilled<'catalog>(sections: &[Section<'catalog>], limit: usize) -> Vec<Vec<Entry<'catalog>>> {
    let limit = limit.max(2);
    let mut columns: Vec<Vec<Entry>> = vec![Vec::new()];

    for section in sections {
        let used = columns.last().map_or(0, Vec::len);
        let room = limit.saturating_sub(used);
        let cramped = room < section.height() + 1 && (section.height() <= limit || room < 3);
        if used > 0 && cramped {
            columns.push(Vec::new());
        } else if used > 0 {
            columns.last_mut().expect("columns starts with one column").push(Entry::Gap);
        }
        columns
            .last_mut()
            .expect("columns starts with one column")
            .push(Entry::Heading(section.title));
        for binding in &section.bindings {
            if columns.last().map_or(0, Vec::len) >= limit {
                columns.push(vec![Entry::Heading(section.title)]);
            }
            columns
                .last_mut()
                .expect("columns starts with one column")
                .push(Entry::Binding(binding));
        }
    }

    columns
}

fn tallest(columns: &[Vec<&Section>]) -> usize {
    columns
        .iter()
        .map(|column| column.iter().map(|section| section.height() + 1).sum::<usize>())
        .max()
        .unwrap_or(0)
}

struct Painter {
    tints: Tints,
    popup: Rect,
    keys_width: usize,
    label_width: usize,
}

impl Painter {
    fn base(&self) -> Style {
        Style::new().bg(Color::from(self.tints.filler)).fg(Color::from(self.tints.foreground))
    }

    fn column(&self, buffer: &mut Buffer, x: u16, entries: &[Entry]) {
        let heading = self.base().fg(Color::from(self.tints.muted)).add_modifier(Modifier::BOLD);
        let key = self.base().fg(Color::from(self.tints.attention));
        let bottom = self.popup.y + self.popup.height.saturating_sub(1);
        let label_x = x + u16::try_from(self.keys_width + 2).unwrap_or(u16::MAX);

        for (y, entry) in (self.popup.y + 1..bottom).zip(entries) {
            match entry {
                Entry::Heading(title) => self.put(buffer, x, y, title, self.label_width, heading),
                Entry::Binding(binding) => {
                    self.put(buffer, x, y, &binding.shown(), self.keys_width, key);
                    self.put(buffer, label_x, y, binding.label, self.label_width, self.base());
                }
                Entry::Gap => {}
            }
        }
    }

    fn put(&self, buffer: &mut Buffer, x: u16, y: u16, text: &str, width: usize, style: Style) {
        let room = usize::from(self.popup.right().saturating_sub(x));
        buffer.set_stringn(x, y, text, width.min(room), style);
    }

    fn closing(&self, buffer: &mut Buffer, closing: &str) {
        let width = u16::try_from(closing.width() + 1).unwrap_or(u16::MAX);
        if width > self.popup.width {
            return;
        }
        let x = (self.popup.x + self.popup.width).saturating_sub(width);
        let y = self.popup.y + self.popup.height.saturating_sub(1);
        buffer.set_string(x, y, closing, self.base().fg(Color::from(self.tints.muted)));
    }
}
