//! Every changed file at once: a list beside the diff of the selected one.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};

use crate::color::Rgb;
use crate::command::Command;
use crate::git::{Content, FileChange, Status};
use crate::theme::{Tints, TokyoNight};
use crate::ui::{Landing, Mode, Result, Tree, Viewer};

const SIDEBAR: u16 = 34;
const NARROWEST: u16 = 40;

/// The files under review, and the one on screen.
pub struct Review {
    changes: Vec<FileChange>,
    tree: Tree,
    selected: usize,
    opened: usize,
    first: usize,
    viewer: Option<Viewer>,
    showing: bool,
    listing: u16,
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
        let tree = Tree::of(&changes);
        let (selected, opened) = tree
            .rows()
            .iter()
            .enumerate()
            .find_map(|(index, row)| row.change.map(|change| (index, change)))
            .unwrap_or_default();
        let mut review = Self {
            tree,
            changes,
            selected,
            opened,
            first: 0,
            viewer: None,
            showing: true,
            listing: SIDEBAR,
            listed: None,
            mode,
            wrap,
            tab_width,
            tints: TokyoNight::TINTS,
        };
        review.open()?;

        Ok(review)
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let style = Style::new().bg(color(self.tints.background));
        frame.buffer_mut().set_style(area, style);

        let Some(change) = self.changes.get(self.opened) else {
            let style = style.fg(color(self.tints.foreground));
            frame.buffer_mut().set_stringn(area.x, area.y, "no changes", area.width.into(), style);
            return;
        };

        let header = Rect::new(area.x, area.y, area.width, 1.min(area.height));
        let rest =
            Rect::new(area.x, area.y + header.height, area.width, area.height - header.height);
        let (sidebar, body) = self.panes(rest);
        self.listed = sidebar;

        let buffer = frame.buffer_mut();
        buffer.set_style(header, Style::new().bg(color(self.tints.filler)));

        let titled = match sidebar {
            Some(sidebar) => {
                self.title(buffer, Rect::new(header.x, header.y, sidebar.width, 1), "CHANGES");

                Rect::new(header.x + sidebar.width, header.y, header.width - sidebar.width, 1)
            }
            None => header,
        };

        let line = format!("{} {}", change.status.letter(), change.path.display());
        let position = self
            .tree
            .rows()
            .iter()
            .filter_map(|row| row.change)
            .position(|change| change == self.opened)
            .map_or(0, |position| position + 1);
        let counter = format!("{position}/{} ", self.changes.len());
        let room = u16::try_from(counter.len()).unwrap_or(0);
        buffer.set_stringn(
            titled.x + 1,
            titled.y,
            line,
            titled.width.saturating_sub(room + 2).into(),
            Style::new().fg(color(self.tints.foreground)).bg(color(self.tints.filler)),
        );
        buffer.set_stringn(
            titled.x + titled.width.saturating_sub(room),
            titled.y,
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
    pub fn run(&mut self, command: Command) -> Result<()> {
        match command {
            Command::NextFile => {
                self.step(Step::Forward)?;
            }
            Command::PreviousFile => {
                self.step(Step::Backward)?;
            }
            Command::ToggleFileList => self.showing = !self.showing,
            Command::NextHunk => {
                let moved = self.viewer.as_mut().is_some_and(Viewer::next_hunk);
                if !moved && self.step(Step::Forward)? {
                    self.land(Landing::FirstHunk);
                }
            }
            Command::PreviousHunk => {
                let moved = self.viewer.as_mut().is_some_and(Viewer::previous_hunk);
                if !moved && self.step(Step::Backward)? {
                    self.land(Landing::LastHunk);
                }
            }
            command => {
                if let Some(viewer) = &mut self.viewer {
                    viewer.run(command);
                }
            }
        }

        Ok(())
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        self.changes.get(self.opened).map(|change| change.path.as_path())
    }

    pub fn line(&self) -> Option<usize> {
        self.viewer.as_ref().and_then(Viewer::line)
    }

    pub fn hunk(&self) -> (Option<usize>, usize) {
        self.viewer.as_ref().map_or((None, 0), Viewer::hunk)
    }

    fn step(&mut self, step: Step) -> Result<bool> {
        let rows = self.tree.rows();
        let target = match step {
            Step::Forward => rows
                .iter()
                .enumerate()
                .skip(self.selected + 1)
                .find(|(_, row)| row.change.is_some()),
            Step::Backward => rows
                .iter()
                .enumerate()
                .take(self.selected)
                .rev()
                .find(|(_, row)| row.change.is_some()),
        };
        let Some((index, _)) = target else {
            return Ok(false);
        };
        self.select(index)?;

        Ok(true)
    }

    fn land(&mut self, landing: Landing) {
        if let Some(viewer) = &mut self.viewer {
            viewer.land_on(landing);
        }
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
                    if index < self.tree.rows().len() {
                        let already = index == self.selected;
                        self.select(index)?;
                        if already {
                            self.fold();
                        }
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

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file left selected"
    )]
    pub fn refresh(&mut self, changes: Vec<FileChange>) -> Result<()> {
        let reading = self.changes.get(self.opened).map(|change| change.path.clone());
        let place = self.viewer.as_ref().map(Viewer::place);
        if let Some(viewer) = &self.viewer {
            self.mode = viewer.mode();
            self.wrap = viewer.wrap();
        }

        self.changes = changes;
        self.tree = Tree::of(&self.changes);
        let kept =
            reading.and_then(|path| self.changes.iter().position(|change| change.path == path));
        self.opened = kept.unwrap_or_default();
        self.selected = self
            .changes
            .get(self.opened)
            .and_then(|change| self.tree.row_of(&change.path))
            .unwrap_or_default();
        self.first = 0;

        self.open()?;
        if let (Some(_), Some(place), Some(viewer)) = (kept, place, &mut self.viewer) {
            viewer.return_to(place);
        }

        Ok(())
    }

    fn roll(&mut self, rows: isize) {
        if let Some(viewer) = &mut self.viewer {
            viewer.scroll_by(rows);
        }
    }

    fn select(&mut self, index: usize) -> Result<()> {
        let index = index.min(self.tree.rows().len().saturating_sub(1));
        if index == self.selected {
            return Ok(());
        }
        self.selected = index;

        let Some(change) = self.tree.rows().get(index).and_then(|row| row.change) else {
            return Ok(());
        };
        if change == self.opened {
            return Ok(());
        }

        if let Some(viewer) = &self.viewer {
            self.mode = viewer.mode();
            self.wrap = viewer.wrap();
        }
        self.opened = change;

        self.open()
    }

    fn fold(&mut self) {
        let path = self.tree.rows().get(self.selected).map(|row| row.path.clone());
        self.tree.toggle(self.selected);

        if let Some(index) = path.and_then(|path| self.tree.row_of(&path)) {
            self.selected = index;
        }
    }

    fn open(&mut self) -> Result<()> {
        self.viewer = match self.changes.get(self.opened) {
            Some(FileChange { content: Content::Text(diff), path, .. }) => {
                Some(Viewer::new(diff.clone(), path, self.mode, self.wrap, self.tab_width)?)
            }
            _ => None,
        };

        Ok(())
    }

    pub fn counted(&self) -> String {
        match self.changes.len() {
            1 => "1 file".to_owned(),
            many => format!("{many} files"),
        }
    }

    pub fn laid(&self) -> &'static str {
        match self.viewer.as_ref().map_or(self.mode, Viewer::mode) {
            Mode::SideBySide => "side by side",
            Mode::Unified => "unified",
        }
    }

    pub fn tabbed(&self) -> usize {
        self.tab_width
    }

    fn title(&self, buffer: &mut Buffer, area: Rect, name: &str) {
        buffer.set_stringn(
            area.x + 1,
            area.y,
            name,
            usize::from(area.width.saturating_sub(2)),
            Style::new().fg(color(self.tints.muted)).bg(color(self.tints.filler)),
        );
    }

    pub fn sidebar(&self) -> Option<u16> {
        self.showing.then_some(self.listing)
    }

    pub fn paths(&self) -> Vec<std::path::PathBuf> {
        self.changes.iter().map(|change| change.path.clone()).collect()
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, selecting a changed file"
    )]
    pub fn select_path(&mut self, path: &std::path::Path) -> Result<()> {
        let Some(index) = self.tree.row_of(path) else {
            return Ok(());
        };
        self.select(index)
    }

    pub fn resize(&mut self, listing: u16) {
        if listing > 0 {
            self.listing = listing;
        }
    }

    fn panes(&self, area: Rect) -> (Option<Rect>, Rect) {
        let Some(listing) =
            self.sidebar().filter(|listing| area.width >= listing.saturating_add(NARROWEST))
        else {
            return (None, area);
        };

        let sidebar = Rect::new(area.x, area.y, listing, area.height);
        let rest = Rect::new(area.x + listing, area.y, area.width - listing, area.height);

        (Some(sidebar), rest)
    }

    fn list(&mut self, buffer: &mut Buffer, area: Rect) {
        let height = usize::from(area.height);
        if self.selected < self.first {
            self.first = self.selected;
        } else if height > 0 && self.selected >= self.first + height {
            self.first = self.selected + 1 - height;
        }

        for (y, index) in (0..area.height).zip(self.first..self.tree.rows().len()) {
            let listed = &self.tree.rows()[index];
            let selected = index == self.selected;
            let row = if selected { self.tints.filler } else { self.tints.background };
            let indent = u16::try_from(listed.depth * 2).unwrap_or(0);

            buffer.set_style(
                Rect::new(area.x, area.y + y, area.width, 1),
                Style::new().bg(color(row)),
            );

            let Some(change) = listed.change.and_then(|change| self.changes.get(change)) else {
                buffer.set_stringn(
                    area.x + 1 + indent,
                    area.y + y,
                    format!("{} {}", if listed.open { "▾" } else { "▸" }, listed.label),
                    usize::from(area.width.saturating_sub(2 + indent)),
                    Style::new().fg(color(self.tints.muted)).bg(color(row)),
                );
                continue;
            };

            buffer.set_string(
                area.x + 1 + indent,
                area.y + y,
                change.status.letter().to_string(),
                Style::new()
                    .fg(color(self.tints.foreground))
                    .bg(color(self.swatch(change.status, selected))),
            );
            buffer.set_stringn(
                area.x + 3 + indent,
                area.y + y,
                &listed.label,
                usize::from(area.width.saturating_sub(4 + indent)),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Step {
    Forward,
    Backward,
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
