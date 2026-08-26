//! Tab expansion, wrapping, horizontal scroll, and span slicing.

use super::align::{Cell, Row, Span};
use super::model::{FileDiff, Source};
use crate::color::Rgb;
use std::iter;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// How wide a grapheme cluster is, in the unit the frontend paints in.
pub trait Metric {
    fn advance(&self, grapheme: &str) -> usize;
}

/// Terminal cells: zero for combining marks, two for East Asian wide.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cells;

impl Metric for Cells {
    fn advance(&self, grapheme: &str) -> usize {
        grapheme.width()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Viewport {
    pub width: usize,
    pub tab_width: usize,
    pub wrap: bool,
    pub scroll: usize,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { width: 80, tab_width: 4, wrap: false, scroll: 0 }
    }
}

/// Everything the flow layer needs besides the diff itself.
pub struct Layout<'a, M> {
    pub viewport: Viewport,
    pub metric: &'a M,
    pub old_syntax: &'a Highlighting,
    pub new_syntax: &'a Highlighting,
}

impl<M: Metric> Layout<'_, M> {
    pub fn screen_rows(&self, diff: &FileDiff, rows: &[Row]) -> Vec<ScreenRow> {
        let painter = Painter { viewport: &self.viewport, metric: self.metric };
        let mut screen = Vec::with_capacity(rows.len());

        for (index, row) in rows.iter().enumerate() {
            let old = painter.side(&diff.old, &row.old, self.old_syntax);
            let new = painter.side(&diff.new, &row.new, self.new_syntax);

            for line in 0..old.len().max(new.len()).max(1) {
                screen.push(ScreenRow {
                    row: index,
                    continuation: line > 0,
                    old: old.get(line).cloned().unwrap_or_default(),
                    new: new.get(line).cloned().unwrap_or_default(),
                });
            }
        }
        screen
    }
}

/// A byte range within a line, and the colour to paint it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Highlight {
    pub span: Span,
    pub color: Rgb,
}

/// One snapshot's syntax highlighting, indexed by line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Highlighting {
    lines: Vec<Vec<Highlight>>,
}

impl Highlighting {
    pub const EMPTY: Self = Self { lines: Vec::new() };

    pub fn new(lines: Vec<Vec<Highlight>>) -> Self {
        Self { lines }
    }

    pub fn line(&self, index: usize) -> &[Highlight] {
        self.lines.get(index).map_or(&[], Vec::as_slice)
    }
}

/// A stretch of text sharing one set of styles, so a renderer walks runs and
/// never indexes into the text itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Run {
    pub text: String,
    pub emphasis: bool,
    pub color: Option<Rgb>,
}

/// One side of one painted row, ready to draw left to right.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Painted {
    pub runs: Vec<Run>,
    pub width: usize,
}

/// One row of the screen. A wrapped line produces several sharing a `row`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ScreenRow {
    pub row: usize,
    pub continuation: bool,
    pub old: Painted,
    pub new: Painted,
}

struct Painter<'a, M> {
    viewport: &'a Viewport,
    metric: &'a M,
}

impl<M: Metric> Painter<'_, M> {
    fn side(&self, source: &Source, cell: &Cell, syntax: &Highlighting) -> Vec<Painted> {
        let Some(line) = cell.line else {
            return Vec::new();
        };

        let layout = LineLayout::new(source.line(line), self.viewport.tab_width, self.metric);

        let windows = if self.viewport.wrap {
            layout.wrap(self.viewport.width)
        } else {
            iter::once(self.viewport.scroll..self.viewport.scroll + self.viewport.width).collect()
        };

        let highlights = syntax.line(line);
        windows.into_iter().map(|window| layout.paint(window, &cell.emphasis, highlights)).collect()
    }
}

/// A line measured out into columns, with tabs already resolved to spaces.
struct LineLayout {
    expanded: String,
    clusters: Vec<Cluster>,
    width: usize,
}

/// One grapheme cluster. Tab expansion produces several sharing a `byte`.
struct Cluster {
    byte: usize,
    expanded: Range<usize>,
    column: usize,
    advance: usize,
}

impl LineLayout {
    fn new<M: Metric>(line: &str, tab_width: usize, metric: &M) -> Self {
        let mut expanded = String::with_capacity(line.len());
        let mut clusters = Vec::new();
        let mut column = 0;

        let mut push = |text: &str, byte: usize, column: usize, advance: usize| {
            let start = expanded.len();
            expanded.push_str(text);
            clusters.push(Cluster { byte, expanded: start..expanded.len(), column, advance });
        };

        for (byte, grapheme) in line.grapheme_indices(true) {
            if grapheme == "\t" {
                let stop = ((column / tab_width) + 1) * tab_width;
                while column < stop {
                    push(" ", byte, column, 1);
                    column += 1;
                }
                continue;
            }
            let advance = metric.advance(grapheme).max(1);
            push(grapheme, byte, column, advance);
            column += advance;
        }

        Self { expanded, clusters, width: column }
    }

    fn wrap(&self, width: usize) -> Vec<Range<usize>> {
        if width == 0 || self.clusters.is_empty() {
            return iter::once(0..0).collect();
        }

        let mut windows = Vec::new();
        let mut start = 0;
        for cluster in &self.clusters {
            if cluster.column + cluster.advance > start + width {
                windows.push(start..cluster.column);
                start = cluster.column;
            }
        }
        windows.push(start..self.width.max(start));
        windows
    }

    fn paint(&self, columns: Range<usize>, emphasis: &[Span], highlights: &[Highlight]) -> Painted {
        let mut runs: Vec<Run> = Vec::new();
        let mut width = 0;
        let mut highlight = 0;

        for cluster in &self.clusters {
            while highlight < highlights.len() && highlights[highlight].span.end <= cluster.byte {
                highlight += 1;
            }

            let end = cluster.column + cluster.advance;
            if end <= columns.start || cluster.column >= columns.end {
                continue;
            }

            let emphasised =
                emphasis.iter().any(|span| cluster.byte >= span.start && cluster.byte < span.end);
            let color = highlights
                .get(highlight)
                .filter(|found| cluster.byte >= found.span.start)
                .map(|found| found.color);
            let straddles = cluster.column < columns.start || end > columns.end;
            let text = if straddles { " " } else { &self.expanded[cluster.expanded.clone()] };

            match runs.last_mut() {
                Some(run) if run.emphasis == emphasised && run.color == color => {
                    run.text.push_str(text);
                }
                _ => runs.push(Run { text: text.to_owned(), emphasis: emphasised, color }),
            }
            width += if straddles { 1 } else { cluster.advance };
        }

        Painted { runs, width }
    }
}
