//! Painting screen rows into a terminal.

use std::path::Path;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::color::Rgb;
use crate::diff::{
    Align, Cells, FileDiff, Highlighting, Layout, Painted, Row, RowKind, ScreenRow, Viewport,
    unified,
};
use crate::syntax::Syntax;
use crate::theme::{Tints, TokyoNight};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Syntax(#[from] crate::syntax::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Two panes against each other, or one column with deletions above insertions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    SideBySide,
    Unified,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Control {
    #[default]
    Continue,
    Quit,
}

/// A view of one diff, and where the reader is looking at it.
pub struct Viewer {
    diff: FileDiff,
    side_by_side: Vec<Row>,
    unified: Vec<Row>,
    mode: Mode,
    old_syntax: Highlighting,
    new_syntax: Highlighting,
    tints: Tints,
    wrap: bool,
    tab_width: usize,
    top: usize,
    left: usize,
    screen: Vec<ScreenRow>,
    laid_out: Option<Geometry>,
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

        Ok(Self {
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
            screen: Vec::new(),
            laid_out: None,
        })
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let gutter = self.gutter();
        let pane = pane_width(area, gutter, self.mode);
        self.lay_out(pane);
        self.top = self.top.min(self.screen.len().saturating_sub(1));

        let buffer = frame.buffer_mut();
        buffer.set_style(area, Style::new().bg(color(self.tints.background)));

        let mut screen = Screen { buffer, tints: self.tints, gutter, pane };
        let rows = self.rows();
        for (y, painted) in (0..area.height).zip(self.screen.iter().skip(self.top)) {
            match self.mode {
                Mode::SideBySide => screen.split_row(y, painted, &rows[painted.row]),
                Mode::Unified => screen.unified_row(y, painted, &rows[painted.row]),
            }
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> Control {
        let page = 10;
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Control::Quit;
            }
            KeyCode::Char('q') | KeyCode::Esc => return Control::Quit,
            KeyCode::Char('j') | KeyCode::Down => self.top += 1,
            KeyCode::Char('k') | KeyCode::Up => self.top = self.top.saturating_sub(1),
            KeyCode::PageDown => self.top += page,
            KeyCode::PageUp => self.top = self.top.saturating_sub(page),
            KeyCode::Char('g') | KeyCode::Home => self.top = 0,
            KeyCode::Char('G') | KeyCode::End => self.top = self.screen.len(),
            KeyCode::Char('h') | KeyCode::Left => self.left = self.left.saturating_sub(4),
            KeyCode::Char('l') | KeyCode::Right => self.left += 4,
            KeyCode::Char('w') => {
                self.wrap = !self.wrap;
                self.left = 0;
            }
            KeyCode::Char('s') => {
                self.mode = match self.mode {
                    Mode::SideBySide => Mode::Unified,
                    Mode::Unified => Mode::SideBySide,
                };
                self.top = 0;
            }
            _ => {}
        }
        Control::Continue
    }

    fn rows(&self) -> &[Row] {
        match self.mode {
            Mode::SideBySide => &self.side_by_side,
            Mode::Unified => &self.unified,
        }
    }

    fn gutter(&self) -> u16 {
        let widest = self.diff.old.len().max(self.diff.new.len()).max(1);
        u16::try_from(widest.to_string().len()).unwrap_or(u16::MAX)
    }

    fn lay_out(&mut self, pane: u16) {
        let geometry = Geometry { pane, wrap: self.wrap, left: self.left, mode: self.mode };
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
}

impl Screen<'_> {
    fn split_row(&mut self, y: u16, painted: &ScreenRow, row: &Row) {
        let (old, new) = self.split_washes(row.kind);
        let (old_number, new_number) = numbers(painted, row);

        self.number(0, y, old_number, old.tint);
        self.text(self.gutter + 1, y, &painted.old, old);

        let divider = self.gutter + 1 + self.pane;
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

        self.number(0, y, old_number, wash.tint);
        self.number(self.gutter + 1, y, new_number, wash.tint);

        let side = if row.old.line.is_some() { &painted.old } else { &painted.new };
        self.text(2 * (self.gutter + 1), y, side, wash);
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
