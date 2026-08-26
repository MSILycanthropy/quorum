//! Every changed file at once: a list beside the diff of the selected one.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};

use crate::color::Rgb;
use crate::git::{Content, FileChange, Status};
use crate::theme::{Tints, TokyoNight};
use crate::ui::{Control, Mode, Result, Viewer};

const SIDEBAR: u16 = 34;
const NARROWEST: u16 = 40;

/// The files under review, and the one on screen.
pub struct Review {
    changes: Vec<FileChange>,
    selected: usize,
    first: usize,
    viewer: Option<Viewer>,
    sidebar: bool,
    listed: Option<Rect>,
    mode: Mode,
    wrap: bool,
    tab_width: usize,
    tints: Tints,
}

impl Review {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the selected file"
    )]
    pub fn new(changes: Vec<FileChange>, mode: Mode, wrap: bool, tab_width: usize) -> Result<Self> {
        let mut review = Self {
            changes,
            selected: 0,
            first: 0,
            viewer: None,
            sidebar: true,
            listed: None,
            mode,
            wrap,
            tab_width,
            tints: TokyoNight::TINTS,
        };
        review.open()?;

        Ok(review)
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let style = Style::new().bg(color(self.tints.background));
        frame.buffer_mut().set_style(area, style);

        let Some(change) = self.changes.get(self.selected) else {
            let style = style.fg(color(self.tints.foreground));
            frame.buffer_mut().set_stringn(area.x, area.y, "no changes", area.width.into(), style);
            return;
        };

        let header = Rect::new(area.x, area.y, area.width, 1.min(area.height));
        let rest =
            Rect::new(area.x, area.y + header.height, area.width, area.height - header.height);
        let (sidebar, body) = self.panes(rest);
        self.listed = sidebar;

        let line = format!("{} {}", change.status.letter(), change.path.display());
        let counter = format!("{}/{} ", self.selected + 1, self.changes.len());
        let room = u16::try_from(counter.len()).unwrap_or(0);
        let buffer = frame.buffer_mut();
        buffer.set_style(header, Style::new().bg(color(self.tints.filler)));
        buffer.set_stringn(
            header.x + 1,
            header.y,
            line,
            header.width.saturating_sub(room + 2).into(),
            Style::new().fg(color(self.tints.foreground)).bg(color(self.tints.filler)),
        );
        buffer.set_stringn(
            header.x + header.width.saturating_sub(room),
            header.y,
            counter,
            room.into(),
            Style::new().fg(color(self.tints.gutter)).bg(color(self.tints.filler)),
        );

        if let Some(sidebar) = sidebar {
            self.list(frame.buffer_mut(), sidebar);
        }

        if let Some(viewer) = &mut self.viewer {
            viewer.draw(frame, body);
        } else {
            let style = Style::new().fg(color(self.tints.gutter)).bg(color(self.tints.background));
            frame.buffer_mut().set_stringn(
                body.x + 1,
                body.y,
                "binary file",
                body.width.into(),
                style,
            );
        }
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file moved to"
    )]
    pub fn key(&mut self, key: KeyEvent) -> Result<Control> {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Control::Quit);
            }
            KeyCode::Char('q') | KeyCode::Esc => return Ok(Control::Quit),
            KeyCode::Char(']') => self.select(self.selected.saturating_add(1))?,
            KeyCode::Char('[') => self.select(self.selected.saturating_sub(1))?,
            KeyCode::Char('F') => self.sidebar = !self.sidebar,
            _ => {
                if let Some(viewer) = &mut self.viewer {
                    viewer.key(key);
                }
            }
        }

        Ok(Control::Continue)
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file clicked on"
    )]
    pub fn mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        let at = Position::new(mouse.column, mouse.row);
        let listed = self.listed.filter(|area| area.contains(at));

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(area) = listed {
                    let index = self.first + usize::from(mouse.row - area.y);
                    if index < self.changes.len() {
                        self.select(index)?;
                    }
                }
            }
            MouseEventKind::ScrollDown if listed.is_some() => {
                self.select(self.selected.saturating_add(1))?;
            }
            MouseEventKind::ScrollUp if listed.is_some() => {
                self.select(self.selected.saturating_sub(1))?;
            }
            MouseEventKind::ScrollDown => self.roll(3),
            MouseEventKind::ScrollUp => self.roll(-3),
            _ => {}
        }

        Ok(())
    }

    fn roll(&mut self, rows: isize) {
        if let Some(viewer) = &mut self.viewer {
            viewer.scroll_by(rows);
        }
    }

    fn select(&mut self, index: usize) -> Result<()> {
        let index = index.min(self.changes.len().saturating_sub(1));
        if index == self.selected {
            return Ok(());
        }

        if let Some(viewer) = &self.viewer {
            self.mode = viewer.mode();
            self.wrap = viewer.wrap();
        }
        self.selected = index;

        self.open()
    }

    fn open(&mut self) -> Result<()> {
        self.viewer = match self.changes.get(self.selected) {
            Some(FileChange { content: Content::Text(diff), path, .. }) => {
                Some(Viewer::new(diff.clone(), path, self.mode, self.wrap, self.tab_width)?)
            }
            _ => None,
        };

        Ok(())
    }

    fn panes(&self, area: Rect) -> (Option<Rect>, Rect) {
        if !self.sidebar || area.width < SIDEBAR + NARROWEST {
            return (None, area);
        }

        let sidebar = Rect::new(area.x, area.y, SIDEBAR, area.height);
        let rest = Rect::new(area.x + SIDEBAR, area.y, area.width - SIDEBAR, area.height);

        (Some(sidebar), rest)
    }

    fn list(&mut self, buffer: &mut Buffer, area: Rect) {
        let height = usize::from(area.height);
        if self.selected < self.first {
            self.first = self.selected;
        } else if height > 0 && self.selected >= self.first + height {
            self.first = self.selected + 1 - height;
        }

        for (y, index) in (0..area.height).zip(self.first..self.changes.len()) {
            let change = &self.changes[index];
            let selected = index == self.selected;
            let row = if selected { self.tints.filler } else { self.tints.background };

            buffer.set_style(
                Rect::new(area.x, area.y + y, area.width, 1),
                Style::new().bg(color(row)),
            );
            buffer.set_string(
                area.x + 1,
                area.y + y,
                change.status.letter().to_string(),
                Style::new()
                    .fg(color(self.tints.foreground))
                    .bg(color(self.swatch(change.status, selected))),
            );
            buffer.set_stringn(
                area.x + 3,
                area.y + y,
                shortened(change, area.width),
                usize::from(area.width.saturating_sub(4)),
                Style::new().fg(color(self.tints.foreground)).bg(color(row)),
            );
        }

        for y in 0..area.height {
            buffer.set_string(
                area.x + area.width - 1,
                area.y + y,
                "│",
                Style::new().fg(color(self.tints.gutter)).bg(color(self.tints.background)),
            );
        }
    }

    fn swatch(&self, status: Status, selected: bool) -> Rgb {
        match (status, selected) {
            (Status::Added, false) => self.tints.added,
            (Status::Added, true) => self.tints.added_emphasis,
            (Status::Deleted, false) => self.tints.removed,
            (Status::Deleted, true) => self.tints.removed_emphasis,
            (Status::Modified, false) => self.tints.background,
            (Status::Modified, true) => self.tints.filler,
        }
    }
}

fn shortened(change: &FileChange, width: u16) -> String {
    let path = change.path.display().to_string();
    let room = usize::from(width).saturating_sub(4);
    if path.chars().count() <= room {
        return path;
    }

    let tail: String = path.chars().skip(path.chars().count().saturating_sub(room - 1)).collect();

    format!("…{tail}")
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
