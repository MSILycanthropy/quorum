//! One file's diff, and where the reader is looking at it.

use std::collections::HashSet;
use std::ops::{Range, RangeInclusive};
use std::path::{Path, PathBuf};

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::command::Command;
use crate::diff::{
    Align, Cell, Cells, ChangeKind, FileDiff, Highlighting, Layout, Painted, Row, RowKind,
    ScreenRow, Side, Viewport, unified,
};
use crate::syntax::Syntax;
use crate::theme::{Theme, Tints};
use crate::ui::annotation::{cut, has_body, heading};
use crate::ui::{Annotation, Body, Draw, Indicators, Mode, Presentation, Result};
use hue::Rgb;

/// How many rows a hunk jump leaves above the hunk, and the cursor keeps from
/// the edges of the screen.
const CONTEXT: usize = 3;

/// Where to land once the diff has been laid out for the first time: a hunk,
/// a line of the newer side counted from one, or an annotation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Landing {
    FirstHunk,
    LastHunk,
    Line(usize),
    FirstAnnotation,
    LastAnnotation,
    Annotation(String),
}

/// Where the reader was scrolled to, where the cursor was, and where a
/// selection was held from, kept across a reload of the same file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Place {
    top: usize,
    left: usize,
    mark: Option<Mark>,
    held: Option<Mark>,
    chosen: Side,
}

/// The line under the cursor, named by side and line so that it outlives any
/// one layout. A fold is marked by the first newer-side line it hides, and an
/// annotation row by the line it is anchored to as well as by its own name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
struct Mark {
    side: Side,
    line: usize,
    within: Option<Within>,
}

/// A row of an annotation, counted from its heading, named by the annotation's
/// id so that it outlives any one set of annotations.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
struct Within {
    id: String,
    row: usize,
}

/// An annotation and the diff row it hangs under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Placement {
    row: usize,
    annotation: usize,
}

/// A view of one diff, and where the reader is looking at it.
pub struct Viewer {
    diff: FileDiff,
    side_by_side: Vec<Row>,
    unified: Vec<Row>,
    presentation: Presentation,
    plain: bool,
    path: PathBuf,
    syntax: Syntax,
    old_syntax: Highlighting,
    new_syntax: Highlighting,
    tints: Tints,
    top: usize,
    left: usize,
    height: usize,
    area: Rect,
    screen: Vec<Line>,
    folding: bool,
    opened: HashSet<usize>,
    hunks: Vec<usize>,
    cursor: usize,
    mark: Option<Mark>,
    held: Option<Mark>,
    pressed: Option<Mark>,
    chosen: Side,
    laid_out: Option<Geometry>,
    landing: Option<Landing>,
    annotations: Vec<Annotation>,
    bodies: Vec<Buffer>,
    bodies_width: Option<u16>,
    headings: Vec<usize>,
}

/// One screen row: a wrapped piece of a diff row, a run of unchanged lines
/// folded away, or a row of an annotation. A fold is named by the first
/// newer-side line it hides. An annotation row names the annotation, its row
/// counted from the heading, and the diff row it hangs under.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Line {
    Text(ScreenRow),
    Fold { first: usize, first_old: usize, hidden: usize },
    Annotation { index: usize, row: usize, anchored: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Geometry {
    pane: u16,
    wrap: bool,
    left: usize,
    mode: Mode,
}

impl Viewer {
    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failures, reading the file and highlighting it"
    )]
    pub fn new(
        diff: FileDiff,
        path: &Path,
        presentation: Presentation,
        theme: &Theme,
    ) -> Result<Self> {
        let side_by_side =
            Align { emphasis: presentation.emphasis, ..Align::default() }.rows(&diff);
        let unified = unified(&side_by_side);
        let plain = diff.changes.iter().all(|change| change.kind == ChangeKind::Equal);

        let mut viewer = Self {
            plain,
            diff,
            unified,
            presentation,
            side_by_side,
            path: path.to_owned(),
            syntax: theme.syntax.clone(),
            old_syntax: Highlighting::EMPTY,
            new_syntax: Highlighting::EMPTY,
            tints: theme.tints,
            top: 0,
            left: 0,
            height: 0,
            area: Rect::default(),
            screen: Vec::new(),
            folding: true,
            opened: HashSet::new(),
            hunks: Vec::new(),
            cursor: 0,
            mark: None,
            held: None,
            pressed: None,
            chosen: Side::New,
            laid_out: None,
            landing: None,
            annotations: Vec::new(),
            bodies: Vec::new(),
            bodies_width: None,
            headings: Vec::new(),
        };
        viewer.highlight()?;

        Ok(viewer)
    }

    pub fn mode(&self) -> Mode {
        self.presentation.mode
    }

    pub fn wrap(&self) -> bool {
        self.presentation.wrap
    }

    pub fn scroll_by(&mut self, rows: isize) {
        self.top = self.top.saturating_add_signed(rows).min(self.screen.len().saturating_sub(1));
        self.drag();
    }

    pub fn place(&self) -> Place {
        Place {
            top: self.top,
            left: self.left,
            mark: self.mark.clone(),
            held: self.held.clone(),
            chosen: self.chosen,
        }
    }

    pub fn return_to(&mut self, place: Place) {
        self.top = place.top;
        self.left = place.left;
        self.mark = place.mark;
        self.held = place.held;
        self.chosen = place.chosen;
        self.cursor = self.resolve();
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the fold landed in"
    )]
    pub fn land_on(&mut self, landing: Landing) -> Result<()> {
        let wanted = match &landing {
            Landing::Line(line) => Some(*line),
            _ => None,
        };
        self.landing = Some(landing);
        let Some(line) = wanted else {
            return Ok(());
        };
        let wanted = line.saturating_sub(1);
        let rows = self.rows();
        let folded = self
            .closed()
            .iter()
            .map(|range| first_line(rows, range)..first_line(rows, range) + range.len())
            .find(|lines| lines.contains(&wanted));
        match folded {
            Some(lines) => self.open_fold(lines.start),
            None => Ok(()),
        }
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the lines unfolded"
    )]
    pub fn run(&mut self, command: Command) -> Result<()> {
        let page = self.height.max(2);
        match command {
            Command::NextLine => {
                if let Some(next) = self.next_entry(self.cursor) {
                    self.place_cursor(next);
                    self.follow();
                }
            }
            Command::PreviousLine => {
                if let Some(previous) = self.previous_entry(self.cursor) {
                    self.place_cursor(previous);
                    self.follow();
                }
            }
            Command::HalfPageDown => self.page_down(page / 2),
            Command::HalfPageUp => self.page_up(page / 2),
            Command::PageDown => self.page_down(page - 1),
            Command::PageUp => self.page_up(page - 1),
            Command::Top => {
                self.place_cursor(0);
                self.follow();
            }
            Command::Bottom => {
                if let Some(last) = self.screen.len().checked_sub(1) {
                    self.place_cursor(self.entry_containing(last));
                    self.follow();
                }
            }
            Command::Select => {
                self.held = match self.held {
                    Some(_) => None,
                    None => self.text_mark(self.cursor),
                };
            }
            Command::OldSide => self.choose(Side::Old),
            Command::NewSide => self.choose(Side::New),
            Command::ScrollLeft => self.left = self.left.saturating_sub(4),
            Command::ScrollRight => self.left += 4,
            Command::ToggleWrapping => {
                self.presentation.wrap = !self.presentation.wrap;
                self.left = 0;
            }
            Command::ToggleLayout if self.plain => {}
            Command::ToggleLayout => {
                self.presentation.mode = match self.presentation.mode {
                    Mode::SideBySide => Mode::Unified,
                    Mode::Unified => Mode::SideBySide,
                };
            }
            Command::NextHunk => {
                self.next_hunk();
            }
            Command::PreviousHunk => {
                self.previous_hunk();
            }
            Command::ExpandFold if self.annotation().is_some() => {
                if let Some(Line::Annotation { index, .. }) = self.screen.get(self.cursor) {
                    self.toggle(*index);
                }
            }
            Command::ToggleAnnotations => self.toggle_every(),
            Command::NextAnnotation => {
                self.next_annotation();
            }
            Command::PreviousAnnotation => {
                self.previous_annotation();
            }
            Command::ExpandFold => {
                let bottom = (self.top + self.height).min(self.screen.len());
                let visible = self.screen.get(self.top..bottom).unwrap_or_default();
                let under = self.screen.get(self.cursor).and_then(Line::fold);
                if let Some(first) = under.or_else(|| visible.iter().find_map(Line::fold)) {
                    self.open_fold(first)?;
                }
            }
            Command::ToggleFolds => {
                self.folding = !self.folding;
                self.opened.clear();
                self.laid_out = None;
                self.highlight()?;
            }
            _ => {}
        }
        self.lay_out();

        Ok(())
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting the fold clicked open"
    )]
    pub fn click(&mut self, column: u16, row: u16) -> Result<()> {
        if !self.area.contains(Position::new(column, row)) {
            return Ok(());
        }
        self.held = None;
        self.pressed = None;
        let index = self.top + usize::from(row - self.area.y);
        let clicked = match self.screen.get(index) {
            None => return Ok(()),
            Some(Line::Annotation { .. }) => {
                self.place_cursor(index);
                return Ok(());
            }
            Some(Line::Fold { first, .. }) => Err(*first),
            Some(Line::Text(painted)) => Ok(&self.rows()[painted.row]),
        };
        match clicked {
            Err(first) => {
                self.place_cursor(index);
                self.open_fold(first)?;
                self.lay_out();
            }
            Ok(row) => {
                let divider = self.area.x + self.margin() + self.pane();
                let side = if column < divider { Side::Old } else { Side::New };
                if self.shown() == Mode::SideBySide && cell(row, side).line.is_some() {
                    self.chosen = side;
                }
                self.place_cursor(self.entry_containing(index));
                self.pressed = self.text_mark(self.cursor);
            }
        }

        Ok(())
    }

    /// Moves the cursor to the row a drag has reached, holding the selection
    /// from the row the button went down on. Past either edge, it scrolls a
    /// row instead.
    pub fn drag_to(&mut self, row: u16) {
        let Some(pressed) = self.pressed.clone() else {
            return;
        };
        let Some(last) = self.screen.len().checked_sub(1) else {
            return;
        };
        if self.height == 0 {
            return;
        }
        let index = if row < self.area.y {
            self.top = self.top.saturating_sub(1);
            self.top
        } else if row >= self.area.bottom() {
            self.top = (self.top + 1).min(self.screen.len().saturating_sub(self.height));
            self.top + self.height - 1
        } else {
            self.top + usize::from(row - self.area.y)
        };
        let index = self.entry_containing(index.min(last));
        if self.held.is_none() && index == self.find(&pressed) {
            return;
        }
        self.held.get_or_insert(pressed);
        self.place_cursor(index);
    }

    pub fn release(&mut self) {
        self.pressed = None;
    }

    /// Grows the selection from the cursor to the row clicked, or starts one
    /// there, keeping the side it is on.
    pub fn extend_to(&mut self, column: u16, row: u16) {
        if !self.area.contains(Position::new(column, row)) {
            return;
        }
        let index = self.top + usize::from(row - self.area.y);
        if index >= self.screen.len() {
            return;
        }
        if self.held.is_none() {
            self.held = self.text_mark(self.cursor);
        }
        if self.held.is_some() {
            self.place_cursor(self.entry_containing(index));
        }
    }

    pub fn clear_selection(&mut self) -> bool {
        self.pressed = None;
        self.held.take().is_some()
    }

    pub fn selecting(&self) -> bool {
        self.held.is_some()
    }

    /// The selected lines of one side, counted from one, or the cursor's line
    /// on its own when nothing is selected.
    pub fn selection(&self) -> Option<(Side, RangeInclusive<usize>)> {
        match self.selected() {
            Some((side, indices)) => Some((side, self.lines_on(side, indices)?)),
            None => self.cursor().map(|(side, line)| (side, line..=line)),
        }
    }

    /// The newer side's lines the selection covers, counted from one, or the
    /// newer-side line nearest the cursor.
    pub fn located(&self) -> Option<RangeInclusive<usize>> {
        let newer = self.selected().and_then(|(_, indices)| self.lines_on(Side::New, indices));

        newer.or_else(|| self.line().map(|line| line..=line))
    }

    fn selected(&self) -> Option<(Side, RangeInclusive<usize>)> {
        let held = self.held.as_ref()?;
        let from = self.find(held);
        let indices = if from <= self.cursor {
            from..=self.end_of(self.cursor)
        } else {
            self.cursor..=self.end_of(from)
        };

        Some((held.side, indices))
    }

    fn lines_on(
        &self,
        side: Side,
        indices: RangeInclusive<usize>,
    ) -> Option<RangeInclusive<usize>> {
        let rows = self.rows();
        let covered = self.screen.get(indices)?.iter().filter_map(|line| covered(line, rows, side));
        let (first, end) = covered.fold(None, |spanned, lines| match spanned {
            None => Some((lines.start, lines.end)),
            Some((first, end)) => Some((lines.start.min(first), lines.end.max(end))),
        })?;

        Some(first + 1..=end)
    }

    fn text_mark(&self, index: usize) -> Option<Mark> {
        match self.screen.get(index)? {
            Line::Text(_) => self.mark_at(index),
            Line::Fold { .. } | Line::Annotation { .. } => None,
        }
    }

    fn open_fold(&mut self, first: usize) -> Result<()> {
        self.opened.insert(first);
        self.laid_out = None;

        self.highlight()
    }

    fn highlight(&mut self) -> Result<()> {
        let rows = self.rows();
        let closed = self.closed();
        let old = closed.iter().map(|range| {
            let first = first_old_line(rows, range);
            first..first + range.len()
        });
        let new = closed.iter().map(|range| {
            let first = first_line(rows, range);
            first..first + range.len()
        });
        let old = between(old, self.diff.old.len());
        let new = between(new, self.diff.new.len());

        self.old_syntax = self.syntax.highlight_lines(&self.diff.old, &self.path, &old)?;
        self.new_syntax = self.syntax.highlight_lines(&self.diff.new, &self.path, &new)?;

        Ok(())
    }

    pub fn next_hunk(&mut self) -> bool {
        let next = self.hunks.iter().copied().find(|&start| start > self.cursor);
        self.land(next)
    }

    pub fn previous_hunk(&mut self) -> bool {
        let previous = self.hunks.iter().copied().rfind(|&start| start < self.cursor);
        self.land(previous)
    }

    pub fn next_annotation(&mut self) -> bool {
        let next = self.headings.iter().copied().find(|&start| start > self.cursor);
        self.land(next)
    }

    pub fn previous_annotation(&mut self) -> bool {
        let above = match self.screen.get(self.cursor) {
            Some(Line::Annotation { row, .. }) => self.cursor - row,
            _ => self.cursor,
        };
        let previous = self.headings.iter().copied().rfind(|&start| start < above);
        self.land(previous)
    }

    pub fn annotated(&self) -> (Option<usize>, usize) {
        let current = self.headings.iter().rposition(|&start| start <= self.cursor);

        (current.map(|index| index + 1), self.headings.len())
    }

    #[expect(
        clippy::missing_errors_doc,
        reason = "the Error enum names the only failure, highlighting lines a fold gives up"
    )]
    pub fn annotate(&mut self, annotations: Vec<Annotation>) -> Result<()> {
        let annotations: Vec<Annotation> = annotations
            .into_iter()
            .filter(|annotation| annotation.anchor.path == self.path)
            .collect();
        if annotations == self.annotations {
            return Ok(());
        }
        let shape = |annotation: &Annotation| {
            (
                annotation.id.clone(),
                annotation.anchor.clone(),
                annotation.body.clone(),
                annotation.collapsed,
            )
        };
        let same_shape = annotations.len() == self.annotations.len()
            && annotations.iter().zip(&self.annotations).all(|(new, old)| shape(new) == shape(old));
        if same_shape {
            self.annotations = annotations;
            return Ok(());
        }

        let closed = self.closed();
        self.annotations = annotations;
        self.bodies_width = None;
        self.laid_out = None;
        if self.closed() != closed {
            self.highlight()?;
        }
        if self.area.is_empty() {
            self.screen.clear();
        }
        self.lay_out();

        Ok(())
    }

    pub fn annotation(&self) -> Option<&Annotation> {
        match self.screen.get(self.cursor)? {
            Line::Annotation { index, .. } => self.annotations.get(*index),
            Line::Text(_) | Line::Fold { .. } => None,
        }
    }

    fn toggle(&mut self, index: usize) {
        let Some(annotation) = self.annotations.get_mut(index) else {
            return;
        };
        if !has_body(&annotation.body) {
            return;
        }
        annotation.collapsed = !annotation.collapsed;
        self.rest_on_heading();
        self.laid_out = None;
    }

    fn toggle_every(&mut self) {
        let collapse = self
            .annotations
            .iter()
            .any(|annotation| !annotation.collapsed && has_body(&annotation.body));
        for annotation in &mut self.annotations {
            annotation.collapsed = collapse;
        }
        self.rest_on_heading();
        self.laid_out = None;
    }

    fn rest_on_heading(&mut self) {
        if let Some(within) = self.mark.as_mut().and_then(|mark| mark.within.as_mut()) {
            within.row = 0;
        }
    }

    fn height_of(&self, heading: usize) -> usize {
        let Some(Line::Annotation { index: wanted, .. }) = self.screen.get(heading) else {
            return 0;
        };

        self.screen[heading..]
            .iter()
            .take_while(|line| matches!(line, Line::Annotation { index, .. } if index == wanted))
            .count()
    }

    fn heading_of(&self, id: &str) -> Option<usize> {
        self.headings.iter().copied().find(|&start| match &self.screen[start] {
            Line::Annotation { index, .. } => {
                self.annotations.get(*index).is_some_and(|annotation| annotation.id == id)
            }
            Line::Text(_) | Line::Fold { .. } => false,
        })
    }

    fn boxed(&self, index: usize, row: usize, cursor: bool) -> Option<Boxed<'_>> {
        let annotation = self.annotations.get(index)?;
        let body = self.bodies.get(index)?;
        let width = usize::from(self.pane()).saturating_sub(2);
        let (marker, text) = if row == 0 {
            let marker = match (body.area.height == 0, annotation.collapsed) {
                (true, _) => "▎",
                (false, true) => "▸",
                (false, false) => "▾",
            };
            let heading = cut(&heading(&annotation.heading, self.presentation.tab_width), width);
            (marker, Filling::Heading(heading))
        } else {
            let row = row - 1;
            if row >= usize::from(body.area.height) {
                return None;
            }
            let width = usize::from(body.area.width);
            ("▎", Filling::Body(body.content.get(row * width..(row + 1) * width)?))
        };

        let paper =
            if cursor { self.tints.under_cursor(self.tints.filler) } else { self.tints.filler };
        let (marker_ink, ink) = if annotation.faded {
            (self.tints.gutter, self.tints.muted)
        } else {
            (self.tints.attention, self.tints.foreground)
        };

        Some(Boxed {
            at: self.annotation_at(annotation.anchor.side),
            divider: (!self.plain && self.presentation.mode == Mode::SideBySide)
                .then(|| self.area.x + self.margin() + self.pane()),
            marker,
            text,
            paper: Color::from(paper),
            ink: Color::from(ink),
            marker_ink: Color::from(marker_ink),
            cursor,
            faded: annotation.faded,
        })
    }

    fn annotation_at(&self, side: Side) -> u16 {
        let margin = self.margin();
        let left = self.area.x;
        match (self.plain, self.presentation.mode, side) {
            (true, _, _) | (false, Mode::SideBySide, Side::Old) => left + margin,
            (false, Mode::SideBySide, Side::New) => left + 2 * margin + 1 + self.pane(),
            (false, Mode::Unified, _) => left + margin + self.gutter() + 1,
        }
    }

    pub fn hunk(&self) -> (Option<usize>, usize) {
        let current = self.hunks.iter().rposition(|&start| start <= self.cursor);

        (current.map(|index| index + 1), self.hunks.len())
    }

    pub fn line(&self) -> Option<usize> {
        let rows = self.rows();
        let reading = self.cursor.min(self.screen.len());

        self.screen
            .iter()
            .skip(reading)
            .chain(self.screen.iter().take(reading).rev())
            .find_map(|line| match line {
                Line::Text(painted) => rows[painted.row].new.line,
                Line::Fold { first, .. } => Some(*first),
                Line::Annotation { anchored, .. } => rows[*anchored].new.line,
            })
            .map(|line| line + 1)
    }

    pub fn cursor(&self) -> Option<(Side, usize)> {
        let painted = match self.screen.get(self.cursor)? {
            Line::Text(painted) => painted,
            Line::Fold { .. } => return None,
            Line::Annotation { index, .. } => {
                let annotation = self.annotations.get(*index)?;
                return Some((annotation.anchor.side, annotation.anchor.line));
            }
        };
        let row = &self.rows()[painted.row];
        let side = self.side_of(row);

        cell(row, side).line.map(|line| (side, line + 1))
    }

    fn go_to(&mut self, line: usize) {
        let wanted = line.saturating_sub(1);
        let rows = self.rows();
        let target = self.screen.iter().position(|line| match line {
            Line::Text(painted) => {
                !painted.continuation && rows[painted.row].new.line == Some(wanted)
            }
            Line::Fold { .. } | Line::Annotation { .. } => false,
        });
        if let Some(target) = target {
            self.chosen = Side::New;
            self.place_cursor(target);
            self.top = target.saturating_sub(CONTEXT);
        }
    }

    fn land(&mut self, start: Option<usize>) -> bool {
        let Some(start) = start else {
            return false;
        };
        self.place_cursor(start);
        self.top = start.saturating_sub(CONTEXT);

        true
    }

    fn choose(&mut self, side: Side) {
        if self.shown() != Mode::SideBySide {
            return;
        }
        let Some(Line::Text(painted)) = self.screen.get(self.cursor) else {
            return;
        };
        let row = &self.rows()[painted.row];
        if row.old.line.is_none() || row.new.line.is_none() {
            return;
        }
        self.chosen = side;
        self.mark = self.mark_at(self.cursor);
        if let Some(held) = &self.held {
            let index = self.find(held);
            if let Some(Line::Text(painted)) = self.screen.get(index)
                && let Some(line) = cell(&self.rows()[painted.row], side).line
            {
                self.held = Some(Mark { side, line, within: None });
            }
        }
    }

    fn page_down(&mut self, rows: usize) {
        let Some(last) = self.screen.len().checked_sub(1) else {
            return;
        };
        let target = self.entry_containing((self.cursor + rows).min(last));
        let target = if target > self.cursor {
            target
        } else {
            self.next_entry(self.cursor).unwrap_or(self.cursor)
        };
        self.top += rows;
        self.place_cursor(target);
    }

    fn page_up(&mut self, rows: usize) {
        if self.screen.is_empty() {
            return;
        }
        let target = self.entry_containing(self.cursor.saturating_sub(rows));
        self.top = self.top.saturating_sub(rows);
        self.place_cursor(target);
    }

    fn place_cursor(&mut self, index: usize) {
        self.cursor = index;
        self.mark = self.mark_at(index);
    }

    fn mark_at(&self, index: usize) -> Option<Mark> {
        match self.screen.get(index)? {
            Line::Text(painted) => {
                let row = &self.rows()[painted.row];
                let side = self.side_of(row);
                cell(row, side).line.map(|line| Mark { side, line, within: None })
            }
            Line::Fold { first, .. } => Some(Mark { side: Side::New, line: *first, within: None }),
            Line::Annotation { index, row, .. } => {
                let annotation = self.annotations.get(*index)?;
                Some(Mark {
                    side: annotation.anchor.side,
                    line: annotation.anchor.line.saturating_sub(1),
                    within: Some(Within { id: annotation.id.clone(), row: *row }),
                })
            }
        }
    }

    fn resolve(&self) -> usize {
        self.mark.as_ref().map_or(0, |mark| self.find(mark))
    }

    fn find(&self, mark: &Mark) -> usize {
        if let Some(within) = &mark.within
            && let Some(heading) = self.heading_of(&within.id)
        {
            return heading + within.row.min(self.height_of(heading).saturating_sub(1));
        }
        let rows = self.rows();

        self.screen
            .iter()
            .enumerate()
            .filter_map(|(index, line)| Some((index, covered(line, rows, mark.side)?)))
            .min_by_key(|(_, lines)| distance(lines, mark.line))
            .map_or(0, |(index, _)| index)
    }

    fn side_of(&self, row: &Row) -> Side {
        let preferred = match self.shown() {
            Mode::SideBySide => self.chosen,
            Mode::Unified => Side::New,
        };
        if cell(row, preferred).line.is_some() { preferred } else { opposite(preferred) }
    }

    fn follow(&mut self) {
        if self.height == 0 || self.screen.is_empty() {
            return;
        }
        let margin = CONTEXT.min(self.height.saturating_sub(1) / 2);
        let end = self.end_of(self.cursor);

        let below = (end + 1 + margin).saturating_sub(self.height);
        if self.top < below {
            let last_top = self.screen.len().saturating_sub(self.height);
            let seen = (end + 1).saturating_sub(self.height);
            self.top = below.min(last_top.max(self.top)).max(seen);
        }
        self.top = self.top.min(self.cursor.saturating_sub(margin));
    }

    fn drag(&mut self) {
        if self.height == 0 || self.shows_cursor() {
            return;
        }
        let bottom = self.top + self.height - 1;
        let target = if self.cursor < self.top {
            (self.top..self.screen.len())
                .find(|&index| !self.continues(index))
                .unwrap_or_else(|| self.entry_containing(self.top))
        } else {
            self.entry_containing(bottom.min(self.screen.len().saturating_sub(1)))
        };
        self.place_cursor(target);
    }

    fn shows_cursor(&self) -> bool {
        self.screen.is_empty() || (self.top..self.top + self.height.max(1)).contains(&self.cursor)
    }

    fn continues(&self, index: usize) -> bool {
        matches!(self.screen.get(index), Some(Line::Text(painted)) if painted.continuation)
    }

    fn entry_containing(&self, index: usize) -> usize {
        (0..=index).rev().find(|&index| !self.continues(index)).unwrap_or_default()
    }

    fn next_entry(&self, index: usize) -> Option<usize> {
        (index + 1..self.screen.len()).find(|&index| !self.continues(index))
    }

    fn previous_entry(&self, index: usize) -> Option<usize> {
        (0..index).rev().find(|&index| !self.continues(index))
    }

    fn end_of(&self, index: usize) -> usize {
        self.next_entry(index).unwrap_or(self.screen.len()).saturating_sub(1).max(index)
    }

    fn shown(&self) -> Mode {
        if self.plain { Mode::Unified } else { self.presentation.mode }
    }

    fn rows(&self) -> &[Row] {
        match self.shown() {
            Mode::SideBySide => &self.side_by_side,
            Mode::Unified => &self.unified,
        }
    }

    fn gutter(&self) -> u16 {
        if !self.presentation.line_numbers {
            return 0;
        }
        let widest = self.diff.old.len().max(self.diff.new.len()).max(1);
        u16::try_from(widest.to_string().len()).unwrap_or(u16::MAX)
    }

    fn marker(&self) -> u16 {
        u16::from(!self.plain && self.presentation.indicators != Indicators::None)
    }

    fn margin(&self) -> u16 {
        self.gutter() + 1 + self.marker()
    }

    fn pane(&self) -> u16 {
        let margin = self.margin();
        match (self.plain, self.presentation.mode) {
            (true, _) => self.area.width.saturating_sub(margin),
            (false, Mode::SideBySide) => self.area.width.saturating_sub(2 * margin + 1) / 2,
            (false, Mode::Unified) => self.area.width.saturating_sub(margin + self.gutter() + 1),
        }
    }

    fn lay_out(&mut self) {
        if self.area.is_empty() {
            return;
        }
        let pane = self.pane();
        let geometry =
            Geometry { pane, wrap: self.presentation.wrap, left: self.left, mode: self.shown() };
        if self.laid_out == Some(geometry) {
            return;
        }
        let offset = if self.screen.is_empty() { None } else { self.cursor.checked_sub(self.top) };
        let text_width = pane.saturating_sub(2);
        if self.bodies_width != Some(text_width) {
            self.bodies = self
                .annotations
                .iter()
                .map(|annotation| drawn(&annotation.body, text_width))
                .collect();
            self.bodies_width = Some(text_width);
        }

        let layout = Layout {
            viewport: Viewport {
                width: usize::from(pane),
                tab_width: self.presentation.tab_width,
                wrap: self.presentation.wrap,
                scroll: self.left,
            },
            metric: &Cells,
            old_syntax: &self.old_syntax,
            new_syntax: &self.new_syntax,
        };
        let screen = self.fold(&layout);
        self.screen = self.interleave(screen);
        self.hunks = hunks(&self.screen, self.rows());
        self.headings = headings(&self.screen);
        self.cursor = self.resolve();
        if let Some(offset) = offset {
            self.top = self.cursor.saturating_sub(offset);
        }
        self.laid_out = Some(geometry);
    }

    fn fold(&self, layout: &Layout<Cells>) -> Vec<Line> {
        let rows = self.rows();
        let text = |shown: Range<usize>| {
            let start = shown.start;
            layout
                .screen_rows(&self.diff, &rows[shown])
                .into_iter()
                .map(move |painted| Line::Text(ScreenRow { row: start + painted.row, ..painted }))
        };

        let mut screen = Vec::new();
        let mut start = 0;
        for range in self.closed() {
            screen.extend(text(start..range.start));
            screen.push(Line::Fold {
                first: first_line(rows, &range),
                first_old: first_old_line(rows, &range),
                hidden: range.len(),
            });
            start = range.end;
        }
        screen.extend(text(start..rows.len()));

        screen
    }

    fn interleave(&self, screen: Vec<Line>) -> Vec<Line> {
        let placed = self.placed();
        if placed.is_empty() {
            return screen;
        }

        let mut interleaved = Vec::with_capacity(screen.len() + placed.len());
        let mut next = 0;
        let mut lines = screen.into_iter().peekable();
        while let Some(line) = lines.next() {
            let anchored = match &line {
                Line::Text(painted) => Some(painted.row),
                Line::Fold { .. } | Line::Annotation { .. } => None,
            };
            interleaved.push(line);
            let Some(anchored) = anchored else {
                continue;
            };
            if matches!(lines.peek(), Some(Line::Text(painted)) if painted.continuation) {
                continue;
            }
            while placed.get(next).is_some_and(|placement| placement.row < anchored) {
                next += 1;
            }
            while let Some(placement) =
                placed.get(next).filter(|placement| placement.row == anchored)
            {
                let index = placement.annotation;
                let body = if self.annotations[index].collapsed {
                    0
                } else {
                    self.bodies.get(index).map_or(0, |body| usize::from(body.area.height))
                };
                interleaved.extend((0..=body).map(|row| Line::Annotation { index, row, anchored }));
                next += 1;
            }
        }

        interleaved
    }

    fn placed(&self) -> Vec<Placement> {
        if self.annotations.is_empty() {
            return Vec::new();
        }
        let rows = self.rows();
        let mut old = vec![None; self.diff.old.len()];
        let mut new = vec![None; self.diff.new.len()];
        for (index, row) in rows.iter().enumerate() {
            if let Some(slot) = row.old.line.and_then(|line| old.get_mut(line)) {
                slot.get_or_insert(index);
            }
            if let Some(slot) = row.new.line.and_then(|line| new.get_mut(line)) {
                slot.get_or_insert(index);
            }
        }

        let mut placed: Vec<Placement> = self
            .annotations
            .iter()
            .enumerate()
            .filter_map(|(annotation, placing)| {
                let lines = match placing.anchor.side {
                    Side::Old => &old,
                    Side::New => &new,
                };
                let row = lines.get(placing.anchor.line.checked_sub(1)?).copied().flatten()?;
                Some(Placement { row, annotation })
            })
            .collect();
        placed.sort_by_key(|placement| {
            let side = self.annotations[placement.annotation].anchor.side;
            (placement.row, side == Side::New, placement.annotation)
        });

        placed
    }

    fn closed(&self) -> Vec<Range<usize>> {
        if !self.folding {
            return Vec::new();
        }
        let rows = self.rows();
        let annotated: Vec<usize> = self.placed().iter().map(|placement| placement.row).collect();

        foldable(rows, self.presentation.context)
            .into_iter()
            .flat_map(|range| around(range, &annotated))
            .filter(|range| !self.opened.contains(&first_line(rows, range)))
            .collect()
    }
}

impl Widget for &mut Viewer {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        self.height = usize::from(area.height);
        self.area = area;
        self.lay_out();
        match self.landing.take() {
            Some(Landing::FirstHunk) => {
                self.land(self.hunks.first().copied());
            }
            Some(Landing::LastHunk) => {
                self.land(self.hunks.last().copied());
            }
            Some(Landing::Line(line)) => self.go_to(line),
            Some(Landing::FirstAnnotation) => {
                self.land(self.headings.first().copied());
            }
            Some(Landing::LastAnnotation) => {
                self.land(self.headings.last().copied());
            }
            Some(Landing::Annotation(id)) => {
                self.land(self.heading_of(&id));
            }
            None => {}
        }
        self.top = self.top.min(self.screen.len().saturating_sub(1));
        if !self.shows_cursor() {
            self.follow();
        }

        buffer.set_style(area, Style::new().bg(Color::from(self.tints.background)));
        if self.pane() == 0 {
            return;
        }

        let mut screen = Screen {
            buffer,
            tints: self.tints,
            gutter: self.gutter(),
            marker: self.marker(),
            indicators: self.presentation.indicators,
            backgrounds: self.presentation.backgrounds,
            line_numbers: self.presentation.line_numbers,
            pane: self.pane(),
            left: area.x,
            width: area.width,
        };
        let rows = self.rows();
        let under = self.cursor..=self.end_of(self.cursor);
        let selected = self.selected();
        let rows_on_screen = self.screen.iter().enumerate().skip(self.top);
        for (y, (index, line)) in (area.y..area.y + area.height).zip(rows_on_screen) {
            let cursor = under.contains(&index);
            let chosen = selected
                .as_ref()
                .filter(|(_, indices)| indices.contains(&index))
                .map(|(side, _)| *side);
            let painted = match line {
                Line::Text(painted) => painted,
                Line::Fold { hidden, .. } => {
                    screen.fold_row(y, *hidden, cursor || chosen.is_some());
                    continue;
                }
                Line::Annotation { index, row, .. } => {
                    if let Some(boxed) = self.boxed(*index, *row, cursor) {
                        screen.annotation_row(y, &boxed);
                    }
                    continue;
                }
            };
            let row = &rows[painted.row];
            let side = chosen
                .filter(|&side| cell(row, side).line.is_some())
                .or_else(|| cursor.then(|| self.side_of(row)));
            match (self.plain, self.presentation.mode) {
                (true, _) => screen.plain_row(y, painted, row, side),
                (false, Mode::SideBySide) => screen.split_row(y, painted, row, side),
                (false, Mode::Unified) => screen.unified_row(y, painted, row, side),
            }
        }
    }
}

impl Line {
    fn fold(&self) -> Option<usize> {
        match self {
            Self::Fold { first, .. } => Some(*first),
            Self::Text(_) | Self::Annotation { .. } => None,
        }
    }
}

/// One row of an annotation, ready to paint: where its box starts, what it
/// says, and how it is lit.
struct Boxed<'a> {
    at: u16,
    divider: Option<u16>,
    marker: &'static str,
    text: Filling<'a>,
    paper: Color,
    ink: Color,
    marker_ink: Color,
    cursor: bool,
    faded: bool,
}

/// What fills an annotation row after its marker: the heading, or one row of
/// the body as its host drew it.
enum Filling<'a> {
    Heading(String),
    Body(&'a [ratatui::buffer::Cell]),
}

/// The two backgrounds a row is painted in: its tint, and the stronger tint
/// its intraline changes take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Wash {
    tint: Rgb,
    emphasis: Rgb,
}

struct Screen<'a> {
    buffer: &'a mut Buffer,
    tints: Tints,
    gutter: u16,
    marker: u16,
    indicators: Indicators,
    backgrounds: bool,
    line_numbers: bool,
    pane: u16,
    left: u16,
    width: u16,
}

impl Screen<'_> {
    fn fold_row(&mut self, y: u16, hidden: usize, cursor: bool) {
        let (foreground, background) = if cursor {
            (self.tints.attention, self.tints.under_cursor(self.tints.filler))
        } else {
            (self.tints.muted, self.tints.filler)
        };
        let style = Style::new().fg(Color::from(foreground)).bg(Color::from(background));
        self.buffer.set_style(Rect::new(self.left, y, self.width, 1), style);
        let label = match hidden {
            1 => "⋯ 1 unchanged line".to_owned(),
            many => format!("⋯ {many} unchanged lines"),
        };
        self.buffer.set_stringn(
            self.left + self.gutter + 1,
            y,
            label,
            usize::from(self.width.saturating_sub(self.gutter + 1)),
            style,
        );
    }

    fn annotation_row(&mut self, y: u16, boxed: &Boxed) {
        if let Some(divider) = boxed.divider {
            self.buffer.set_string(
                divider,
                y,
                "│",
                Style::new()
                    .fg(Color::from(self.tints.gutter))
                    .bg(Color::from(self.tints.background)),
            );
        }
        let pane = usize::from(self.pane);
        let base = Style::new().bg(boxed.paper);

        self.buffer.set_style(Rect::new(boxed.at, y, self.pane, 1), base);
        self.buffer.set_stringn(boxed.at, y, boxed.marker, pane, base.fg(boxed.marker_ink));
        match boxed.text {
            Filling::Heading(ref heading) => {
                self.buffer.set_stringn(
                    boxed.at + 2,
                    y,
                    heading,
                    pane.saturating_sub(2),
                    base.fg(boxed.ink).add_modifier(Modifier::BOLD),
                );
            }
            Filling::Body(cells) => self.body_row(boxed.at + 2, y, cells, boxed),
        }
    }

    fn body_row(&mut self, x: u16, y: u16, cells: &[ratatui::buffer::Cell], boxed: &Boxed) {
        for (x, cell) in (x..).zip(cells) {
            let Some(target) = self.buffer.cell_mut((x, y)) else {
                break;
            };
            *target = cell.clone();
            if target.symbol().contains(char::is_control) {
                target.set_symbol(" ");
            }
            if target.fg == Color::Reset {
                target.fg = boxed.ink;
            } else if boxed.faded {
                target.modifier.insert(Modifier::DIM);
            }
            if boxed.cursor || target.bg == Color::Reset {
                target.bg = boxed.paper;
            }
        }
    }

    fn split_row(&mut self, y: u16, painted: &ScreenRow, row: &Row, cursor: Option<Side>) {
        let (mut old, mut new) = self.split_washes(row.kind);
        match cursor {
            Some(Side::Old) => old = self.under_cursor(old),
            Some(Side::New) => new = self.under_cursor(new),
            None => {}
        }
        let (old_number, new_number) = numbers(painted, row);

        let (old_change, new_change) = match row.kind {
            RowKind::Equal => (None, None),
            RowKind::Delete => (Some(Side::Old), None),
            RowKind::Insert => (None, Some(Side::New)),
            RowKind::Replace => (Some(Side::Old), Some(Side::New)),
        };

        self.number(self.left, y, old_number, old.tint, cursor == Some(Side::Old));
        let old_at = self.left + self.gutter + 1;
        self.indicator(old_at, y, old_change, old.tint, painted.continuation);
        self.text(old_at + self.marker, y, &painted.old, old);

        let divider = old_at + self.marker + self.pane;
        self.buffer.set_string(
            divider,
            y,
            "│",
            Style::new().fg(Color::from(self.tints.gutter)).bg(Color::from(self.tints.background)),
        );

        self.number(divider + 1, y, new_number, new.tint, cursor == Some(Side::New));
        let new_at = divider + self.gutter + 2;
        self.indicator(new_at, y, new_change, new.tint, painted.continuation);
        self.text(new_at + self.marker, y, &painted.new, new);
    }

    fn unified_row(&mut self, y: u16, painted: &ScreenRow, row: &Row, cursor: Option<Side>) {
        let mut wash = self.unified_wash(row.kind);
        if cursor.is_some() {
            wash = self.under_cursor(wash);
        }
        let (old_number, new_number) = numbers(painted, row);

        self.number(self.left, y, old_number, wash.tint, cursor == Some(Side::Old));
        self.number(
            self.left + self.gutter + 1,
            y,
            new_number,
            wash.tint,
            cursor == Some(Side::New),
        );

        let change = match row.kind {
            RowKind::Equal => None,
            RowKind::Delete | RowKind::Replace => Some(Side::Old),
            RowKind::Insert => Some(Side::New),
        };
        let side = if row.old.line.is_some() { &painted.old } else { &painted.new };
        let at = self.left + 2 * (self.gutter + 1);
        self.indicator(at, y, change, wash.tint, painted.continuation);
        self.text(at + self.marker, y, side, wash);
    }

    fn plain_row(&mut self, y: u16, painted: &ScreenRow, row: &Row, cursor: Option<Side>) {
        let mut wash = Wash { tint: self.tints.background, emphasis: self.tints.background };
        if cursor.is_some() {
            wash = self.under_cursor(wash);
        }
        let (_, number) = numbers(painted, row);

        self.number(self.left, y, number, wash.tint, cursor.is_some());
        self.text(self.left + self.gutter + 1, y, &painted.new, wash);
    }

    fn under_cursor(&self, wash: Wash) -> Wash {
        Wash {
            tint: self.tints.under_cursor(wash.tint),
            emphasis: self.tints.under_cursor(wash.emphasis),
        }
    }

    fn split_washes(&self, kind: RowKind) -> (Wash, Wash) {
        let plain = Wash { tint: self.tints.background, emphasis: self.tints.background };
        let filler = Wash { tint: self.tints.filler, emphasis: self.tints.filler };
        let removed = self.changed(self.tints.removed, self.tints.removed_emphasis);
        let added = self.changed(self.tints.added, self.tints.added_emphasis);

        match kind {
            RowKind::Equal => (plain, plain),
            RowKind::Delete => (removed, filler),
            RowKind::Insert => (filler, added),
            RowKind::Replace => (removed, added),
        }
    }

    fn unified_wash(&self, kind: RowKind) -> Wash {
        match kind {
            RowKind::Equal => Wash { tint: self.tints.background, emphasis: self.tints.background },
            RowKind::Delete | RowKind::Replace => {
                self.changed(self.tints.removed, self.tints.removed_emphasis)
            }
            RowKind::Insert => self.changed(self.tints.added, self.tints.added_emphasis),
        }
    }

    fn changed(&self, tint: Rgb, emphasis: Rgb) -> Wash {
        let tint = if self.backgrounds { tint } else { self.tints.background };
        Wash { tint, emphasis }
    }

    fn number(&mut self, at: u16, y: u16, line: Option<usize>, tint: Rgb, cursor: bool) {
        let number = line
            .filter(|_| self.line_numbers)
            .map_or_else(String::new, |line| (line + 1).to_string());
        let gutter = usize::from(self.gutter);
        let foreground = if cursor { self.tints.attention } else { self.tints.gutter };
        self.buffer.set_stringn(
            at,
            y,
            format!("{number:>gutter$} "),
            gutter + 1,
            Style::new().fg(Color::from(foreground)).bg(Color::from(tint)),
        );
    }

    fn indicator(&mut self, at: u16, y: u16, change: Option<Side>, tint: Rgb, continuation: bool) {
        if self.marker == 0 {
            return;
        }
        let symbol = match (self.indicators, change) {
            (Indicators::None, _) | (_, None) => " ",
            (Indicators::Bars, Some(_)) => "▎",
            (Indicators::Signs, Some(_)) if continuation => " ",
            (Indicators::Signs, Some(Side::Old)) => "-",
            (Indicators::Signs, Some(Side::New)) => "+",
        };
        let foreground = match change {
            Some(Side::Old) => self.tints.removed_text,
            Some(Side::New) | None => self.tints.added_text,
        };
        self.buffer.set_string(
            at,
            y,
            symbol,
            Style::new().fg(Color::from(foreground)).bg(Color::from(tint)),
        );
    }

    fn text(&mut self, at: u16, y: u16, painted: &Painted, wash: Wash) {
        self.buffer
            .set_style(Rect::new(at, y, self.pane, 1), Style::new().bg(Color::from(wash.tint)));

        let mut x = at;
        for run in &painted.runs {
            let background = if run.emphasis { wash.emphasis } else { wash.tint };
            let foreground = run.color.unwrap_or(self.tints.foreground);
            let remaining = usize::from(at + self.pane - x);
            (x, _) = self.buffer.set_stringn(
                x,
                y,
                &run.text,
                remaining,
                Style::new().fg(Color::from(foreground)).bg(Color::from(background)),
            );
        }
    }
}

fn cell(row: &Row, side: Side) -> &Cell {
    match side {
        Side::Old => &row.old,
        Side::New => &row.new,
    }
}

fn opposite(side: Side) -> Side {
    match side {
        Side::Old => Side::New,
        Side::New => Side::Old,
    }
}

fn covered(line: &Line, rows: &[Row], side: Side) -> Option<Range<usize>> {
    match line {
        Line::Text(painted) if painted.continuation => None,
        Line::Annotation { .. } => None,
        Line::Text(painted) => cell(&rows[painted.row], side).line.map(|line| line..line + 1),
        Line::Fold { first, first_old, hidden } => {
            let start = match side {
                Side::Old => *first_old,
                Side::New => *first,
            };
            Some(start..start + hidden)
        }
    }
}

fn distance(lines: &Range<usize>, line: usize) -> usize {
    if line < lines.start {
        lines.start - line
    } else if line >= lines.end {
        line + 1 - lines.end
    } else {
        0
    }
}

fn foldable(rows: &[Row], context: usize) -> Vec<Range<usize>> {
    if rows.iter().all(|row| row.kind == RowKind::Equal) {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut start = 0;
    while start < rows.len() {
        let length = rows[start..].iter().take_while(|row| row.kind == RowKind::Equal).count();
        if length == 0 {
            start += 1;
            continue;
        }
        let end = start + length;
        let hidden_start = if start == 0 { start } else { start + context };
        let hidden_end = if end == rows.len() { end } else { end.saturating_sub(context) };
        if hidden_end > hidden_start + 1 {
            ranges.push(hidden_start..hidden_end);
        }
        start = end;
    }

    ranges
}

fn drawn(body: &Body, width: u16) -> Buffer {
    let area = Rect::new(0, 0, width, body.height(width));
    let mut buffer = Buffer::empty(area);
    body.render(area, &mut buffer);

    buffer
}

fn around(range: Range<usize>, annotated: &[usize]) -> Vec<Range<usize>> {
    let mut pieces = Vec::new();
    let mut start = range.start;
    for &row in annotated.iter().filter(|row| range.contains(row)) {
        pieces.push(start..row);
        start = row + 1;
    }
    pieces.push(start..range.end);
    pieces.retain(|piece| piece.len() > 1);

    pieces
}

fn between(hidden: impl Iterator<Item = Range<usize>>, length: usize) -> Vec<Range<usize>> {
    let mut shown = Vec::new();
    let mut start = 0;
    for range in hidden {
        shown.push(start..range.start);
        start = range.end;
    }
    shown.push(start..length);

    shown
}

fn first_line(rows: &[Row], range: &Range<usize>) -> usize {
    rows[range.start].new.line.expect("a fold hides only unchanged rows, which have both sides")
}

fn first_old_line(rows: &[Row], range: &Range<usize>) -> usize {
    rows[range.start].old.line.expect("a fold hides only unchanged rows, which have both sides")
}

fn hunks(screen: &[Line], rows: &[Row]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut inside = false;
    for (index, line) in screen.iter().enumerate() {
        let changed = match line {
            Line::Text(painted) => rows[painted.row].kind != RowKind::Equal,
            Line::Fold { .. } => false,
            Line::Annotation { .. } => continue,
        };
        if changed && !inside {
            starts.push(index);
        }
        inside = changed;
    }

    starts
}

fn headings(screen: &[Line]) -> Vec<usize> {
    screen
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            matches!(line, Line::Annotation { row: 0, .. }).then_some(index)
        })
        .collect()
}

fn numbers(painted: &ScreenRow, row: &Row) -> (Option<usize>, Option<usize>) {
    if painted.continuation { (None, None) } else { (row.old.line, row.new.line) }
}
