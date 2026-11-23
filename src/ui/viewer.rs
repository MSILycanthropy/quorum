//! One file's diff, and where the reader is looking at it.

use std::path::Path;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::color::Rgb;
use crate::command::Command;
use crate::diff::{
    Align, Cells, ChangeKind, FileDiff, Highlighting, Layout, Painted, Row, RowKind, ScreenRow,
    Viewport, unified,
};
use crate::syntax::Syntax;
use crate::theme::{Tints, TokyoNight};
use crate::ui::{Mode, Result};

/// How many rows of context a hunk jump leaves above the hunk.
const CONTEXT: usize = 3;

/// Where to land once the diff has been laid out for the first time: a hunk,
/// or a line of the newer side, counted from one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Landing {
    FirstHunk,
    LastHunk,
    Line(usize),
}

/// Where the reader was scrolled to, kept across a reload of the same file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Place {
    pub top: usize,
    pub left: usize,
}

/// A view of one diff, and where the reader is looking at it.
pub struct Viewer {
    diff: FileDiff,
    side_by_side: Vec<Row>,
    unified: Vec<Row>,
    mode: Mode,
    plain: bool,
    old_syntax: Highlighting,
    new_syntax: Highlighting,
    tints: Tints,
    wrap: bool,
    tab_width: usize,
    top: usize,
    left: usize,
    height: usize,
    screen: Vec<ScreenRow>,
    hunks: Vec<usize>,
    focus: Option<usize>,
    laid_out: Option<Geometry>,
    landing: Option<Landing>,
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
        mode: Mode,
        wrap: bool,
        tab_width: usize,
    ) -> Result<Self> {
        let syntax = Syntax::default();
        let old_syntax = syntax.highlight(&diff.old, path)?;
        let new_syntax = syntax.highlight(&diff.new, path)?;
        let side_by_side = Align::default().rows(&diff);
        let unified = unified(&side_by_side);
        let plain = diff.changes.iter().all(|change| change.kind == ChangeKind::Equal);

        Ok(Self {
            plain,
            diff,
            unified,
            mode,
            side_by_side,
            old_syntax,
            new_syntax,
            tints: TokyoNight::TINTS,
            wrap,
            tab_width,
            top: 0,
            left: 0,
            height: 0,
            screen: Vec::new(),
            hunks: Vec::new(),
            focus: None,
            laid_out: None,
            landing: None,
        })
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let gutter = self.gutter();
        let pane = if self.plain {
            area.width.saturating_sub(gutter + 1)
        } else {
            pane_width(area, gutter, self.mode)
        };
        self.height = usize::from(area.height);
        self.lay_out(pane);
        match self.landing.take() {
            Some(Landing::FirstHunk) => {
                self.land((!self.hunks.is_empty()).then_some(0));
            }
            Some(Landing::LastHunk) => {
                self.land(self.hunks.len().checked_sub(1));
            }
            Some(Landing::Line(line)) => self.go_to(line),
            None => {}
        }
        self.top = self.top.min(self.screen.len().saturating_sub(1));

        let buffer = frame.buffer_mut();
        buffer.set_style(area, Style::new().bg(color(self.tints.background)));

        let mut screen = Screen { buffer, tints: self.tints, gutter, pane, left: area.x };
        let rows = self.rows();
        let rows_on_screen = self.screen.iter().skip(self.top);
        for (y, painted) in (area.y..area.y + area.height).zip(rows_on_screen) {
            match (self.plain, self.mode) {
                (true, _) => screen.plain_row(y, painted, &rows[painted.row]),
                (false, Mode::SideBySide) => screen.split_row(y, painted, &rows[painted.row]),
                (false, Mode::Unified) => screen.unified_row(y, painted, &rows[painted.row]),
            }
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn wrap(&self) -> bool {
        self.wrap
    }

    pub fn scroll_by(&mut self, rows: isize) {
        self.top = self.top.saturating_add_signed(rows);
    }

    pub fn place(&self) -> Place {
        Place { top: self.top, left: self.left }
    }

    pub fn return_to(&mut self, place: Place) {
        self.top = place.top;
        self.left = place.left;
    }

    pub fn land_on(&mut self, landing: Landing) {
        self.landing = Some(landing);
    }

    pub fn run(&mut self, command: Command) {
        let page = self.height.max(2);
        if !matches!(command, Command::NextHunk | Command::PreviousHunk) {
            self.focus = None;
        }
        match command {
            Command::NextLine => self.top += 1,
            Command::PreviousLine => self.top = self.top.saturating_sub(1),
            Command::HalfPageDown => self.top += page / 2,
            Command::HalfPageUp => self.top = self.top.saturating_sub(page / 2),
            Command::PageDown => self.top += page - 1,
            Command::PageUp => self.top = self.top.saturating_sub(page - 1),
            Command::Top => self.top = 0,
            Command::Bottom => self.top = self.screen.len(),
            Command::ScrollLeft => self.left = self.left.saturating_sub(4),
            Command::ScrollRight => self.left += 4,
            Command::ToggleWrapping => {
                self.wrap = !self.wrap;
                self.left = 0;
            }
            Command::ToggleLayout if self.plain => {}
            Command::ToggleLayout => {
                self.mode = match self.mode {
                    Mode::SideBySide => Mode::Unified,
                    Mode::Unified => Mode::SideBySide,
                };
                self.top = 0;
            }
            Command::NextHunk => {
                self.next_hunk();
            }
            Command::PreviousHunk => {
                self.previous_hunk();
            }
            _ => {}
        }
    }

    pub fn next_hunk(&mut self) -> bool {
        let reading = self.top + CONTEXT;
        let next = self.focus.map_or_else(
            || self.hunks.iter().position(|&start| start > reading),
            |focus| Some(focus + 1).filter(|&next| next < self.hunks.len()),
        );
        self.land(next)
    }

    pub fn previous_hunk(&mut self) -> bool {
        let previous = self.focus.map_or_else(
            || self.hunks.iter().rposition(|&start| start < self.top),
            |focus| focus.checked_sub(1),
        );
        self.land(previous)
    }

    pub fn hunk(&self) -> (Option<usize>, usize) {
        let current = self.focus.or_else(|| {
            let reading = self.top + CONTEXT;
            self.hunks.iter().rposition(|&start| start <= reading)
        });

        (current.map(|index| index + 1), self.hunks.len())
    }

    pub fn line(&self) -> Option<usize> {
        let rows = self.rows();
        let bottom = (self.top + self.height.max(1)).min(self.screen.len());
        let visible = self.top.min(bottom)..bottom;
        let focused = self.focus.and_then(|focus| self.hunks.get(focus).copied());
        let changed = self.hunks.iter().copied().find(|start| visible.contains(start));
        let reading = focused.or(changed).unwrap_or(visible.start);

        self.screen
            .iter()
            .skip(reading)
            .chain(self.screen.iter().take(reading).rev())
            .find_map(|painted| rows[painted.row].new.line)
            .map(|line| line + 1)
    }

    fn go_to(&mut self, line: usize) {
        let rows = self.rows();
        let target = self.screen.iter().position(|painted| {
            !painted.continuation && rows[painted.row].new.line == Some(line.saturating_sub(1))
        });
        if let Some(target) = target {
            self.top = target.saturating_sub(CONTEXT);
        }
    }

    fn land(&mut self, hunk: Option<usize>) -> bool {
        let Some(start) = hunk.and_then(|hunk| self.hunks.get(hunk).copied()) else {
            return false;
        };
        self.focus = hunk;
        self.top = start.saturating_sub(CONTEXT);

        true
    }

    fn shown(&self) -> Mode {
        if self.plain { Mode::Unified } else { self.mode }
    }

    fn rows(&self) -> &[Row] {
        match self.shown() {
            Mode::SideBySide => &self.side_by_side,
            Mode::Unified => &self.unified,
        }
    }

    fn gutter(&self) -> u16 {
        let widest = self.diff.old.len().max(self.diff.new.len()).max(1);
        u16::try_from(widest.to_string().len()).unwrap_or(u16::MAX)
    }

    fn lay_out(&mut self, pane: u16) {
        let geometry = Geometry { pane, wrap: self.wrap, left: self.left, mode: self.shown() };
        if self.laid_out == Some(geometry) {
            return;
        }

        let layout = Layout {
            viewport: Viewport {
                width: usize::from(pane),
                tab_width: self.tab_width,
                wrap: self.wrap,
                scroll: self.left,
            },
            metric: &Cells,
            old_syntax: &self.old_syntax,
            new_syntax: &self.new_syntax,
        };
        self.screen = layout.screen_rows(&self.diff, self.rows());
        self.hunks = hunks(&self.screen, self.rows());
        self.focus = None;
        self.laid_out = Some(geometry);
    }
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
    pane: u16,
    left: u16,
}

impl Screen<'_> {
    fn split_row(&mut self, y: u16, painted: &ScreenRow, row: &Row) {
        let (old, new) = self.split_washes(row.kind);
        let (old_number, new_number) = numbers(painted, row);

        self.number(self.left, y, old_number, old.tint);
        self.text(self.left + self.gutter + 1, y, &painted.old, old);

        let divider = self.left + self.gutter + 1 + self.pane;
        self.buffer.set_string(
            divider,
            y,
            "│",
            Style::new().fg(color(self.tints.gutter)).bg(color(self.tints.background)),
        );

        self.number(divider + 1, y, new_number, new.tint);
        self.text(divider + self.gutter + 2, y, &painted.new, new);
    }

    fn unified_row(&mut self, y: u16, painted: &ScreenRow, row: &Row) {
        let wash = self.unified_wash(row.kind);
        let (old_number, new_number) = numbers(painted, row);

        self.number(self.left, y, old_number, wash.tint);
        self.number(self.left + self.gutter + 1, y, new_number, wash.tint);

        let side = if row.old.line.is_some() { &painted.old } else { &painted.new };
        self.text(self.left + 2 * (self.gutter + 1), y, side, wash);
    }

    fn plain_row(&mut self, y: u16, painted: &ScreenRow, row: &Row) {
        let wash = Wash { tint: self.tints.background, emphasis: self.tints.background };
        let (_, number) = numbers(painted, row);

        self.number(self.left, y, number, wash.tint);
        self.text(self.left + self.gutter + 1, y, &painted.new, wash);
    }

    fn split_washes(&self, kind: RowKind) -> (Wash, Wash) {
        let plain = Wash { tint: self.tints.background, emphasis: self.tints.background };
        let filler = Wash { tint: self.tints.filler, emphasis: self.tints.filler };
        let removed = Wash { tint: self.tints.removed, emphasis: self.tints.removed_emphasis };
        let added = Wash { tint: self.tints.added, emphasis: self.tints.added_emphasis };

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
                Wash { tint: self.tints.removed, emphasis: self.tints.removed_emphasis }
            }
            RowKind::Insert => Wash { tint: self.tints.added, emphasis: self.tints.added_emphasis },
        }
    }

    fn number(&mut self, at: u16, y: u16, line: Option<usize>, tint: Rgb) {
        let number = line.map_or_else(String::new, |line| (line + 1).to_string());
        let gutter = usize::from(self.gutter);
        self.buffer.set_stringn(
            at,
            y,
            format!("{number:>gutter$} "),
            gutter + 1,
            Style::new().fg(color(self.tints.gutter)).bg(color(tint)),
        );
    }

    fn text(&mut self, at: u16, y: u16, painted: &Painted, wash: Wash) {
        self.buffer.set_style(Rect::new(at, y, self.pane, 1), Style::new().bg(color(wash.tint)));

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
                Style::new().fg(color(foreground)).bg(color(background)),
            );
        }
    }
}

fn hunks(screen: &[ScreenRow], rows: &[Row]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut inside = false;
    for (index, painted) in screen.iter().enumerate() {
        let changed = rows[painted.row].kind != RowKind::Equal;
        if changed && !inside {
            starts.push(index);
        }
        inside = changed;
    }

    starts
}

fn numbers(painted: &ScreenRow, row: &Row) -> (Option<usize>, Option<usize>) {
    if painted.continuation { (None, None) } else { (row.old.line, row.new.line) }
}

fn pane_width(area: Rect, gutter: u16, mode: Mode) -> u16 {
    match mode {
        Mode::SideBySide => area.width.saturating_sub(2 * gutter + 3) / 2,
        Mode::Unified => area.width.saturating_sub(2 * gutter + 2),
    }
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}
