//! Every key, grouped, drawn straight from the command catalog.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Clear, Widget};
use unicode_width::UnicodeWidthStr;

use crate::color::Rgb;
use crate::command::{Binding, Catalog, Category};
use crate::theme::{Tints, TokyoNight};

const GAP: usize = 3;
const CLOSING: &str = "any key closes";

/// The overlay listing every binding, open or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Help {
    open: bool,
    tints: Tints,
}

impl Default for Help {
    fn default() -> Self {
        Self { open: false, tints: TokyoNight::TINTS }
    }
}

/// One category and its bindings, as the rows it takes up.
struct Section<'catalog> {
    category: Category,
    bindings: Vec<&'catalog Binding>,
}

impl Section<'_> {
    fn height(&self) -> usize {
        self.bindings.len() + 1
    }
}

impl Help {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn draw(&self, area: Rect, buffer: &mut Buffer, catalog: &Catalog) {
        if !self.open {
            return;
        }

        let categories = catalog.sections();
        let sections: Vec<Section> = categories
            .iter()
            .map(|&category| Section {
                category,
                bindings: catalog
                    .bindings()
                    .iter()
                    .filter(|binding| binding.category == category)
                    .collect(),
            })
            .filter(|section| !section.bindings.is_empty())
            .collect();
        let keys_width =
            catalog.bindings().iter().map(|binding| binding.keys().width()).max().unwrap_or(0);
        let label_width = catalog
            .bindings()
            .iter()
            .map(|binding| binding.label.width())
            .chain(categories.iter().map(|category| category.title().width()))
            .max()
            .unwrap_or(0);
        let column_width = keys_width + 2 + label_width;

        let room_wide = usize::from(area.width.saturating_sub(4));
        let room_tall = usize::from(area.height.saturating_sub(4));
        let most_columns = ((room_wide + GAP) / (column_width + GAP)).max(1);
        let columns = (1..=most_columns)
            .map(|count| flow(&sections, count))
            .find(|columns| tallest(columns) <= room_tall)
            .unwrap_or_else(|| flow(&sections, most_columns));

        let width = columns.len() * column_width + (columns.len() - 1) * GAP + 2;
        let height = tallest(&columns) + 2;
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
            painter.column(buffer, x, column);
        }
        painter.closing(buffer);
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
        Style::new().bg(color(self.tints.filler)).fg(color(self.tints.foreground))
    }

    fn column(&self, buffer: &mut Buffer, x: u16, sections: &[&Section]) {
        let heading = self.base().fg(color(self.tints.muted)).add_modifier(Modifier::BOLD);
        let key = self.base().fg(color(self.tints.attention));
        let bottom = self.popup.y + self.popup.height.saturating_sub(1);
        let label_x = x + u16::try_from(self.keys_width + 2).unwrap_or(u16::MAX);
        let mut y = self.popup.y + 1;

        for section in sections {
            if y >= bottom {
                return;
            }
            self.put(buffer, x, y, section.category.title(), self.label_width, heading);
            y += 1;
            for binding in &section.bindings {
                if y >= bottom {
                    return;
                }
                self.put(buffer, x, y, &binding.keys(), self.keys_width, key);
                self.put(buffer, label_x, y, binding.label, self.label_width, self.base());
                y += 1;
            }
            y += 1;
        }
    }

    fn put(&self, buffer: &mut Buffer, x: u16, y: u16, text: &str, width: usize, style: Style) {
        let room = usize::from(self.popup.right().saturating_sub(x));
        buffer.set_stringn(x, y, text, width.min(room), style);
    }

    fn closing(&self, buffer: &mut Buffer) {
        let width = u16::try_from(CLOSING.width() + 1).unwrap_or(u16::MAX);
        if width > self.popup.width {
            return;
        }
        let x = (self.popup.x + self.popup.width).saturating_sub(width);
        let y = self.popup.y + self.popup.height.saturating_sub(1);
        buffer.set_string(x, y, CLOSING, self.base().fg(color(self.tints.muted)));
    }
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
