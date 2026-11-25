//! Every changed file at once: a list beside the diff of the selected one.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use unicode_width::UnicodeWidthStr;

use crate::color::Rgb;
use crate::command::Command;
use crate::diff::{FileDiff, Side, Whitespace};
use crate::git::{Content, FileChange, Status};
use crate::theme::{Tints, TokyoNight};
use crate::ui::annotation::has_body;
use crate::ui::{Anchor, Annotation, Control, Error, Landing, Mode, Result, Row, Tree, Viewer};

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
    peek: Option<Peek>,
    showing: bool,
    listing: u16,
    listed: Option<Rect>,
    mode: Mode,
    wrap: bool,
    whitespace: Whitespace,
    tab_width: usize,
    viewed: HashMap<PathBuf, u64>,
    revealed: HashSet<PathBuf>,
    annotations: Vec<Annotation>,
    overrides: HashMap<String, bool>,
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
            peek: None,
            showing: true,
            listing: SIDEBAR,
            listed: None,
            mode,
            wrap,
            whitespace: Whitespace::Compare,
            tab_width,
            viewed: HashMap::new(),
            revealed: HashSet::new(),
            annotations: Vec::new(),
            overrides: HashMap::new(),
            tints: TokyoNight::TINTS,
        };
        review.open()?;

        Ok(review)
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let style = Style::new().bg(color(self.tints.background));
        frame.buffer_mut().set_style(area, style);

        let titled = match (&self.peek, self.changes.get(self.opened)) {
            (Some(peek), _) => format!("  {}", peek.path.display()),
            (None, Some(change)) => match &change.previous {
                Some(previous) => format!(
                    "{} {} → {}",
                    change.status.letter(),
                    previous.display(),
                    change.path.display()
                ),
                None => format!("{} {}", change.status.letter(), change.path.display()),
            },
            (None, None) => {
                let style = style.fg(color(self.tints.foreground));
                let buffer = frame.buffer_mut();
                buffer.set_stringn(area.x, area.y, "no changes", area.width.into(), style);
                return;
            }
        };

        let header = Rect::new(area.x, area.y, area.width, 1.min(area.height));
        let rest =
            Rect::new(area.x, area.y + header.height, area.width, area.height - header.height);
        let (sidebar, body) = self.panes(rest);
        self.listed = sidebar;

        let buffer = frame.buffer_mut();
        buffer.set_style(header, Style::new().bg(color(self.tints.filler)));

        let heading = match sidebar {
            Some(sidebar) => {
                self.title(buffer, Rect::new(header.x, header.y, sidebar.width, 1), "CHANGES");

                Rect::new(header.x + sidebar.width, header.y, header.width - sidebar.width, 1)
            }
            None => header,
        };

        let position = self
            .tree
            .rows()
            .iter()
            .filter_map(|row| row.change)
            .position(|change| change == self.opened)
            .map_or(0, |position| position + 1);
        let counter = if self.peek.is_some() {
            "not changed · Esc back ".to_owned()
        } else {
            format!("{position}/{} ", self.changes.len())
        };
        let room = u16::try_from(counter.width()).unwrap_or(0);
        let end = heading.x + heading.width.saturating_sub(room + 1);
        let (x, _) = buffer.set_stringn(
            heading.x + 1,
            heading.y,
            titled,
            end.saturating_sub(heading.x + 1).into(),
            Style::new().fg(color(self.tints.foreground)).bg(color(self.tints.filler)),
        );
        if self.peek.is_none()
            && let Some(change) = self.changes.get(self.opened)
            && let Content::Text(diff) = &change.content
        {
            let counts = diff.counts();
            let wanted = format!("+{} −{}", counts.added, counts.removed).width();
            let wanted = u16::try_from(wanted).unwrap_or(u16::MAX);
            let area = Rect::new(x + 2, heading.y, wanted.min(end.saturating_sub(x + 2)), 1);
            self.counts(buffer, area, change, self.tints.filler);
        }
        buffer.set_stringn(
            heading.x + heading.width.saturating_sub(room),
            heading.y,
            counter,
            room.into(),
            Style::new().fg(color(self.tints.gutter)).bg(color(self.tints.filler)),
        );

        if let Some(sidebar) = sidebar {
            self.list(frame.buffer_mut(), sidebar);
        }

        let placeholder =
            if self.collapsed() { "generated file · Enter to show" } else { "binary file" };
        if let Some(viewer) = self.active() {
            viewer.draw(frame, body);
        } else {
            let style = Style::new().fg(color(self.tints.gutter)).bg(color(self.tints.background));
            frame.buffer_mut().set_stringn(
                body.x + 1,
                body.y,
                placeholder,
                body.width.into(),
                style,
            );
        }
    }

    fn active(&mut self) -> Option<&mut Viewer> {
        match &mut self.peek {
            Some(peek) => peek.viewer.as_mut(),
            None => self.viewer.as_mut(),
        }
    }

    fn collapsed(&self) -> bool {
        self.peek.is_none()
            && self.changes.get(self.opened).is_some_and(|change| {
                change.generated
                    && !self.revealed.contains(&change.path)
                    && !self.annotations.iter().any(|annotation| lands(change, annotation))
            })
    }

    fn counts(&self, buffer: &mut Buffer, area: Rect, change: &FileChange, row: Rgb) {
        let Content::Text(diff) = &change.content else {
            return;
        };
        let counts = diff.counts();
        let added = format!("+{}", counts.added);
        let removed = format!("−{}", counts.removed);
        let wanted = u16::try_from(added.width() + 1 + removed.width()).unwrap_or(u16::MAX);
        if wanted > area.width {
            return;
        }
        let y = area.y;
        let (x, _) = buffer.set_stringn(
            area.x + area.width - wanted,
            y,
            added,
            usize::MAX,
            Style::new().fg(color(self.tints.added_text)).bg(color(row)),
        );
        buffer.set_string(
            x + 1,
            y,
            removed,
            Style::new().fg(color(self.tints.removed_text)).bg(color(row)),
        );
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file moved to"
    )]
    pub fn run(&mut self, command: Command) -> Result<()> {
        if let Some(peek) = &mut self.peek {
            match command {
                Command::NextFile
                | Command::PreviousFile
                | Command::NextHunk
                | Command::PreviousHunk
                | Command::ToggleViewed
                | Command::NextUnviewed
                | Command::NextAnnotation
                | Command::PreviousAnnotation => self.peek = None,
                Command::ToggleFileList => {}
                command => {
                    if let Some(viewer) = &mut peek.viewer {
                        viewer.run(command)?;
                    }
                    return Ok(());
                }
            }
        }

        match command {
            Command::NextFile => {
                self.step(Step::Forward)?;
            }
            Command::PreviousFile => {
                self.step(Step::Backward)?;
            }
            Command::ToggleFileList => self.showing = !self.showing,
            Command::ToggleViewed => {
                self.toggle_viewed()?;
            }
            Command::NextUnviewed => {
                self.next_unviewed()?;
            }
            Command::ToggleWhitespace => {
                self.whitespace = match self.whitespace {
                    Whitespace::Compare => Whitespace::Ignore,
                    Whitespace::Ignore => Whitespace::Compare,
                };
                let place = self.viewer.as_ref().map(Viewer::place);
                self.remember();
                self.open()?;
                if let (Some(place), Some(viewer)) = (place, &mut self.viewer) {
                    viewer.return_to(place);
                }
            }
            Command::ExpandFold if self.collapsed() => {
                if let Some(change) = self.changes.get(self.opened) {
                    self.revealed.insert(change.path.clone());
                }
                self.open()?;
            }
            Command::ExpandFold => self.expand()?,
            Command::ToggleAnnotations => self.collapse_every()?,
            Command::NextAnnotation => {
                let moved = self.viewer.as_mut().is_some_and(Viewer::next_annotation);
                if !moved && self.step_where(Step::Forward, Self::annotated_change)? {
                    self.land(Landing::FirstAnnotation)?;
                }
            }
            Command::PreviousAnnotation => {
                let moved = self.viewer.as_mut().is_some_and(Viewer::previous_annotation);
                if !moved && self.step_where(Step::Backward, Self::annotated_change)? {
                    self.land(Landing::LastAnnotation)?;
                }
            }
            Command::NextHunk => {
                let moved = self.viewer.as_mut().is_some_and(Viewer::next_hunk);
                if !moved && self.step(Step::Forward)? {
                    self.land(Landing::FirstHunk)?;
                }
            }
            Command::PreviousHunk => {
                let moved = self.viewer.as_mut().is_some_and(Viewer::previous_hunk);
                if !moved && self.step(Step::Backward)? {
                    self.land(Landing::LastHunk)?;
                }
            }
            command => {
                if let Some(viewer) = &mut self.viewer {
                    viewer.run(command)?;
                }
            }
        }

        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        match &self.peek {
            Some(peek) => Some(&peek.path),
            None => self.changes.get(self.opened).map(|change| change.path.as_path()),
        }
    }

    pub fn line(&self) -> Option<usize> {
        match &self.peek {
            Some(peek) => peek.viewer.as_ref().and_then(Viewer::line),
            None => self.viewer.as_ref().and_then(Viewer::line),
        }
    }

    pub fn cursor(&self) -> Option<Anchor> {
        let viewer = match &self.peek {
            Some(peek) => peek.viewer.as_ref(),
            None => self.viewer.as_ref(),
        };
        let (side, line) = viewer?.cursor()?;

        Some(Anchor { path: self.path()?.to_path_buf(), side, line })
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the failures, a repeated id and highlighting the file on screen"
    )]
    pub fn annotate(&mut self, annotations: Vec<Annotation>) -> Result<()> {
        let mut seen = HashSet::new();
        if let Some(repeated) = annotations.iter().find(|annotation| !seen.insert(&annotation.id)) {
            return Err(Error::DuplicateAnnotation(repeated.id.clone()));
        }
        self.annotations = annotations;

        let showable = self.peek.is_none()
            && self
                .changes
                .get(self.opened)
                .is_some_and(|change| matches!(change.content, Content::Text(_)));
        if showable && self.collapsed() == self.viewer.is_some() {
            let place = self.viewer.as_ref().map(Viewer::place);
            self.open()?;
            if let (Some(place), Some(viewer)) = (place, &mut self.viewer) {
                viewer.return_to(place);
            }
            return Ok(());
        }
        let group = self.group();
        match &mut self.viewer {
            Some(viewer) => viewer.annotate(group),
            None => Ok(()),
        }
    }

    pub fn annotation(&self) -> Option<&Annotation> {
        if self.peek.is_some() {
            return None;
        }
        self.viewer.as_ref()?.annotation()
    }

    pub fn unplaced(&self) -> Vec<&Annotation> {
        let changes: HashMap<&Path, &FileChange> =
            self.changes.iter().map(|change| (change.path.as_path(), change)).collect();

        self.annotations
            .iter()
            .filter(|annotation| {
                !changes
                    .get(annotation.anchor.path.as_path())
                    .is_some_and(|change| lands(change, annotation))
            })
            .collect()
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file moved to"
    )]
    pub fn go_to_annotation(&mut self, id: &str) -> Result<bool> {
        let Some(annotation) = self.annotations.iter().find(|annotation| annotation.id == id)
        else {
            return Ok(false);
        };
        if !self.changes.iter().any(|change| lands(change, annotation)) {
            return Ok(false);
        }
        let path = annotation.anchor.path.clone();
        self.select_path(&path, None)?;
        self.land(Landing::Annotation(id.to_owned()))?;

        Ok(true)
    }

    pub fn annotated(&self) -> (Option<usize>, usize) {
        if self.peek.is_some() {
            return (None, 0);
        }
        self.viewer.as_ref().map_or((None, 0), Viewer::annotated)
    }

    fn expand(&mut self) -> Result<()> {
        let Some(viewer) = &mut self.viewer else {
            return Ok(());
        };
        let before =
            viewer.annotation().map(|annotation| (annotation.id.clone(), annotation.collapsed));
        viewer.run(Command::ExpandFold)?;
        if let Some((id, was)) = before
            && let Some(now) = viewer.annotation().filter(|annotation| annotation.id == id)
            && now.collapsed != was
        {
            self.overrides.insert(id, now.collapsed);
        }

        Ok(())
    }

    fn collapse_every(&mut self) -> Result<()> {
        let collapse = self
            .group()
            .iter()
            .any(|annotation| !annotation.collapsed && has_body(&annotation.body));
        for annotation in &self.annotations {
            self.overrides.insert(annotation.id.clone(), collapse);
        }
        match &mut self.viewer {
            Some(viewer) => viewer.run(Command::ToggleAnnotations),
            None => Ok(()),
        }
    }

    fn group(&self) -> Vec<Annotation> {
        let Some(change) = self.changes.get(self.opened) else {
            return Vec::new();
        };

        self.annotations
            .iter()
            .filter(|annotation| annotation.anchor.path == change.path)
            .map(|annotation| Annotation {
                collapsed: self
                    .overrides
                    .get(&annotation.id)
                    .copied()
                    .unwrap_or(annotation.collapsed),
                ..annotation.clone()
            })
            .collect()
    }

    fn annotated_change(&self, change: usize) -> bool {
        self.changes.get(change).is_some_and(|change| {
            self.annotations.iter().any(|annotation| lands(change, annotation))
        })
    }

    pub fn hunk(&self) -> (Option<usize>, usize) {
        if self.peek.is_some() {
            return (None, 0);
        }
        self.viewer.as_ref().map_or((None, 0), Viewer::hunk)
    }

    pub fn is_viewed(&self, path: &Path) -> bool {
        self.viewed.contains_key(path)
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file moved to"
    )]
    pub fn toggle_viewed(&mut self) -> Result<Control> {
        self.peek = None;
        let Some(change) = self.changes.get(self.opened) else {
            return Ok(Control::Continue);
        };
        let path = change.path.clone();
        if self.viewed.remove(&path).is_some() {
            return Ok(Control::Unviewed(path));
        }
        self.viewed.insert(path.clone(), fingerprint(change));
        self.next_unviewed()?;

        Ok(Control::Viewed(path))
    }

    pub fn replace_viewed(&mut self, paths: Vec<PathBuf>) {
        let paths: HashSet<PathBuf> = paths.into_iter().collect();
        self.viewed = self
            .changes
            .iter()
            .filter(|change| paths.contains(&change.path))
            .map(|change| (change.path.clone(), fingerprint(change)))
            .collect();
    }

    fn next_unviewed(&mut self) -> Result<bool> {
        let rows = self.tree.rows();
        let unviewed = |index: &usize| {
            rows[*index]
                .change
                .and_then(|change| self.changes.get(change))
                .is_some_and(|change| !self.viewed.contains_key(&change.path))
        };
        let after = (self.selected + 1..rows.len()).find(unviewed);
        let Some(index) = after.or_else(|| (0..self.selected).find(unviewed)) else {
            return Ok(false);
        };
        self.select(index)?;

        Ok(true)
    }

    pub fn is_changed(&self, path: &Path) -> bool {
        self.changes.iter().any(|change| change.path == path)
    }

    pub fn peeked(&self) -> Option<&Path> {
        self.peek.as_ref().map(|peek| peek.path.as_path())
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the file opened"
    )]
    pub fn peek(&mut self, path: PathBuf, content: &Content, line: Option<usize>) -> Result<()> {
        let place = self
            .peek
            .as_ref()
            .filter(|peek| peek.path == path && line.is_none())
            .and_then(|peek| peek.viewer.as_ref())
            .map(Viewer::place);
        let wrap = self.viewer.as_ref().map_or(self.wrap, Viewer::wrap);
        let mut viewer = match content {
            Content::Text(diff) => {
                Some(Viewer::new(diff.clone(), &path, Mode::Unified, wrap, self.tab_width)?)
            }
            Content::Binary => None,
        };
        if let Some(viewer) = &mut viewer {
            match (place, line) {
                (Some(place), _) => viewer.return_to(place),
                (None, Some(line)) => viewer.land_on(Landing::Line(line))?,
                (None, None) => {}
            }
        }
        self.peek = Some(Peek { path, viewer });

        Ok(())
    }

    pub fn close_peek(&mut self) -> bool {
        self.peek.take().is_some()
    }

    fn step(&mut self, step: Step) -> Result<bool> {
        self.step_where(step, |_, _| true)
    }

    fn step_where(&mut self, step: Step, wanted: impl Fn(&Self, usize) -> bool) -> Result<bool> {
        let rows = self.tree.rows();
        let wanted =
            |(_, row): &(usize, &Row)| row.change.is_some_and(|change| wanted(self, change));
        let target = match step {
            Step::Forward => rows.iter().enumerate().skip(self.selected + 1).find(wanted),
            Step::Backward => rows.iter().enumerate().take(self.selected).rev().find(wanted),
        };
        let Some((index, _)) = target else {
            return Ok(false);
        };
        self.select(index)?;

        Ok(true)
    }

    fn land(&mut self, landing: Landing) -> Result<()> {
        match &mut self.viewer {
            Some(viewer) => viewer.land_on(landing),
            None => Ok(()),
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
                if let (None, Some(viewer)) = (listed, self.active()) {
                    viewer.click(mouse.column, mouse.row)?;
                }
                if let Some(area) = listed {
                    self.peek = None;
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
            MouseEventKind::ScrollDown if mouse.modifiers.contains(KeyModifiers::SHIFT) => {
                self.slide(Command::ScrollRight)?;
            }
            MouseEventKind::ScrollUp if mouse.modifiers.contains(KeyModifiers::SHIFT) => {
                self.slide(Command::ScrollLeft)?;
            }
            MouseEventKind::ScrollRight => self.slide(Command::ScrollRight)?,
            MouseEventKind::ScrollLeft => self.slide(Command::ScrollLeft)?,
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
        self.remember();
        self.viewed.retain(|path, seen| {
            changes.iter().any(|change| change.path == *path && fingerprint(change) == *seen)
        });

        let promoted = self
            .peek
            .as_ref()
            .filter(|peek| changes.iter().any(|change| change.path == peek.path))
            .map(|peek| peek.path.clone());

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
        if let Some(path) = promoted {
            self.select_path(&path, None)?;
        }

        Ok(())
    }

    fn roll(&mut self, rows: isize) {
        if let Some(viewer) = self.active() {
            viewer.scroll_by(rows);
        }
    }

    fn slide(&mut self, command: Command) -> Result<()> {
        if let Some(viewer) = self.active() {
            for _ in 0..2 {
                viewer.run(command)?;
            }
        }

        Ok(())
    }

    fn remember(&mut self) {
        if let Some(viewer) = &self.viewer {
            self.mode = viewer.mode();
            self.wrap = viewer.wrap();
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

        self.remember();
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
        let collapsed = self.collapsed();
        self.viewer = match self.changes.get(self.opened) {
            Some(FileChange { content: Content::Text(diff), path, .. }) if !collapsed => {
                let diff = match self.whitespace {
                    Whitespace::Compare => diff.clone(),
                    Whitespace::Ignore => {
                        FileDiff::compute_with(diff.old.clone(), diff.new.clone(), self.whitespace)
                    }
                };
                Some(Viewer::new(diff, path, self.mode, self.wrap, self.tab_width)?)
            }
            _ => None,
        };
        let group = self.group();
        match &mut self.viewer {
            Some(viewer) => viewer.annotate(group),
            None => Ok(()),
        }
    }

    pub fn counted(&self) -> String {
        let files = match self.changes.len() {
            1 => "1 file".to_owned(),
            many => format!("{many} files"),
        };
        match self.viewed.len() {
            0 => files,
            viewed => format!("{files} · {viewed} viewed"),
        }
    }

    pub fn laid(&self) -> String {
        if self.peek.is_some() {
            return "read only".to_owned();
        }
        let layout = match self.viewer.as_ref().map_or(self.mode, Viewer::mode) {
            Mode::SideBySide => "side by side",
            Mode::Unified => "unified",
        };
        match self.whitespace {
            Whitespace::Compare => layout.to_owned(),
            Whitespace::Ignore => format!("{layout} · ignoring whitespace"),
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

    pub fn paths(&self) -> Vec<PathBuf> {
        self.changes.iter().map(|change| change.path.clone()).collect()
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, selecting a changed file"
    )]
    pub fn select_path(&mut self, path: &Path, line: Option<usize>) -> Result<()> {
        let Some(index) = self.tree.row_of(path) else {
            return Ok(());
        };
        self.peek = None;
        self.select(index)?;
        if let (Some(line), Some(viewer)) = (line, &mut self.viewer) {
            viewer.land_on(Landing::Line(line))?;
        }

        Ok(())
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
                buffer.set_string(
                    area.x + 1 + indent,
                    area.y + y,
                    listed.icon.glyph.to_string(),
                    Style::new().fg(color(listed.icon.color)).bg(color(row)),
                );
                buffer.set_stringn(
                    area.x + 3 + indent,
                    area.y + y,
                    &listed.label,
                    usize::from(area.width.saturating_sub(4 + indent)),
                    Style::new().fg(color(self.tints.muted)).bg(color(row)),
                );
                continue;
            };

            buffer.set_string(
                area.x + 1 + indent,
                area.y + y,
                listed.icon.glyph.to_string(),
                Style::new()
                    .fg(color(listed.icon.color))
                    .bg(color(self.swatch(change.status, selected))),
            );
            let viewed = self.viewed.contains_key(&change.path);
            let label = if viewed { self.tints.muted } else { self.tints.foreground };
            let end = area.x + area.width - 2;
            let (x, _) = buffer.set_stringn(
                area.x + 3 + indent,
                area.y + y,
                &listed.label,
                usize::from(end.saturating_sub(area.x + 3 + indent)),
                Style::new().fg(color(label)).bg(color(row)),
            );
            if viewed {
                buffer.set_string(
                    end,
                    area.y + y,
                    "✓",
                    Style::new().fg(color(self.tints.muted)).bg(color(row)),
                );
            } else {
                let room = Rect::new(x + 1, area.y + y, end.saturating_sub(x + 1), 1);
                self.counts(buffer, room, change, row);
            }
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
            (Status::Modified | Status::Renamed, false) => self.tints.background,
            (Status::Modified | Status::Renamed, true) => self.tints.filler,
        }
    }
}

/// A file opened over the review without joining it, read as it stands.
struct Peek {
    path: PathBuf,
    viewer: Option<Viewer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Step {
    Forward,
    Backward,
}

fn lands(change: &FileChange, annotation: &Annotation) -> bool {
    let Content::Text(diff) = &change.content else {
        return false;
    };
    let length = match annotation.anchor.side {
        Side::Old => diff.old.len(),
        Side::New => diff.new.len(),
    };

    annotation.anchor.path == change.path && (1..=length).contains(&annotation.anchor.line)
}

fn fingerprint(change: &FileChange) -> u64 {
    let mut hasher = DefaultHasher::new();
    change.content.hash(&mut hasher);
    hasher.finish()
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
