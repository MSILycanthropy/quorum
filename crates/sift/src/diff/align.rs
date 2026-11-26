//! Which old line sits across from which new line.

use super::model::{Change, ChangeKind, FileDiff};
use similar::{Algorithm, DiffOp, TextDiff};
use std::cmp::Ordering;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// A byte range within a single line, relative to that line's start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// How alike two lines are, bounded to `0.0..=1.0`.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Similarity(f32);

impl Similarity {
    pub const NONE: Self = Self(0.0);
    pub const IDENTICAL: Self = Self(1.0);
    pub const PAIRING: Self = Self(0.4);
    pub const EMPHASIS: Self = Self(0.6);

    pub fn new(score: f32) -> Self {
        Self(score.clamp(Self::NONE.0, Self::IDENTICAL.0))
    }
}

/// What within a changed line is picked out from the rest of it: its changed
/// words with one-character gaps between them closed, each changed word on
/// its own, each changed character, or nothing.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Deserialize,
    serde::Serialize,
    clap::ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Emphasis {
    #[default]
    Words,
    EachWord,
    Characters,
    None,
}

/// One side of a row. `line` is `None` for filler.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cell {
    pub line: Option<usize>,
    pub emphasis: Vec<Span>,
}

impl Cell {
    const FILLER: Self = Self { line: None, emphasis: Vec::new() };

    fn plain(line: usize) -> Self {
        Self { line: Some(line), emphasis: Vec::new() }
    }
}

/// Which snapshot of a file a line belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Side {
    Old,
    #[default]
    New,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowKind {
    Equal,
    Delete,
    Insert,
    Replace,
}

/// One logical row of a side-by-side view, before any wrapping.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Row {
    pub kind: RowKind,
    pub old: Cell,
    pub new: Cell,
}

impl Row {
    pub fn anchor(&self) -> (Option<usize>, Option<usize>) {
        (self.old.line, self.new.line)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Strategy {
    #[default]
    BestMatch,
    Positional,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Align {
    pub pair_threshold: Similarity,
    pub emphasis_threshold: Similarity,
    pub max_block_lines: usize,
    pub strategy: Strategy,
    pub emphasis: Emphasis,
}

impl Default for Align {
    fn default() -> Self {
        Self {
            pair_threshold: Similarity::PAIRING,
            emphasis_threshold: Similarity::EMPHASIS,
            max_block_lines: 100,
            strategy: Strategy::default(),
            emphasis: Emphasis::default(),
        }
    }
}

impl Align {
    pub fn rows(&self, diff: &FileDiff) -> Vec<Row> {
        let mut rows = Vec::new();
        for change in &diff.changes {
            self.change_rows(diff, change, &mut rows);
        }
        rows
    }

    fn change_rows(&self, diff: &FileDiff, change: &Change, rows: &mut Vec<Row>) {
        match change.kind {
            ChangeKind::Equal => {
                for (old, new) in change.old.clone().zip(change.new.clone()) {
                    rows.push(Row {
                        kind: RowKind::Equal,
                        old: Cell::plain(old),
                        new: Cell::plain(new),
                    });
                }
            }
            ChangeKind::Delete => {
                for old in change.old.clone() {
                    rows.push(Row {
                        kind: RowKind::Delete,
                        old: Cell::plain(old),
                        new: Cell::FILLER,
                    });
                }
            }
            ChangeKind::Insert => {
                for new in change.new.clone() {
                    rows.push(Row {
                        kind: RowKind::Insert,
                        old: Cell::FILLER,
                        new: Cell::plain(new),
                    });
                }
            }
            ChangeKind::Replace => self.replace_rows(diff, change, rows),
        }
    }

    fn replace_rows(&self, diff: &FileDiff, change: &Change, rows: &mut Vec<Row>) {
        let (old, new) = (change.old.clone(), change.new.clone());
        let oversized = old.len() > self.max_block_lines || new.len() > self.max_block_lines;

        if self.strategy == Strategy::Positional || oversized {
            zip_unpaired(old, new, rows);
            return;
        }

        let mut pairing =
            Pairing { align: self, diff, scores: ScoreMatrix::new(diff, &old, &new), rows };
        pairing.walk(old, new);
    }

    fn paired_row(&self, diff: &FileDiff, old: usize, new: usize, score: Similarity) -> Row {
        let (old_spans, new_spans) = if score >= self.emphasis_threshold {
            emphasized(self.emphasis, diff.old.line(old), diff.new.line(new))
        } else {
            (Vec::new(), Vec::new())
        };
        Row {
            kind: RowKind::Replace,
            old: Cell { line: Some(old), emphasis: old_spans },
            new: Cell { line: Some(new), emphasis: new_spans },
        }
    }
}

/// Restack side-by-side rows into one column, splitting each modified row into
/// its deletion and its insertion.
#[must_use]
pub fn unified(rows: &[Row]) -> Vec<Row> {
    let mut unified = Vec::with_capacity(rows.len());
    for row in rows {
        if row.kind != RowKind::Replace {
            unified.push(row.clone());
            continue;
        }
        if row.old.line.is_some() {
            unified.push(Row { kind: RowKind::Delete, old: row.old.clone(), new: Cell::FILLER });
        }
        if row.new.line.is_some() {
            unified.push(Row { kind: RowKind::Insert, old: Cell::FILLER, new: row.new.clone() });
        }
    }
    unified
}

struct Pairing<'a> {
    align: &'a Align,
    diff: &'a FileDiff,
    scores: ScoreMatrix,
    rows: &'a mut Vec<Row>,
}

impl Pairing<'_> {
    fn walk(&mut self, old: Range<usize>, new: Range<usize>) {
        if old.is_empty() || new.is_empty() {
            zip_unpaired(old, new, self.rows);
            return;
        }

        let mut best: Option<(usize, usize, Similarity)> = None;
        for old_line in old.clone() {
            for new_line in new.clone() {
                let score = self.scores.score(old_line, new_line);
                if score >= self.align.pair_threshold
                    && best.is_none_or(|(_, _, highest)| score > highest)
                {
                    best = Some((old_line, new_line, score));
                }
            }
        }

        let Some((old_line, new_line, score)) = best else {
            zip_unpaired(old, new, self.rows);
            return;
        };

        self.walk(old.start..old_line, new.start..new_line);
        self.rows.push(self.align.paired_row(self.diff, old_line, new_line, score));
        self.walk(old_line + 1..old.end, new_line + 1..new.end);
    }
}

fn zip_unpaired(old: Range<usize>, new: Range<usize>, rows: &mut Vec<Row>) {
    for offset in 0..old.len().max(new.len()) {
        let old_line = (old.start + offset < old.end).then_some(old.start + offset);
        let new_line = (new.start + offset < new.end).then_some(new.start + offset);
        let kind = match (old_line, new_line) {
            (Some(_), Some(_)) => RowKind::Replace,
            (Some(_), None) => RowKind::Delete,
            (None, Some(_)) => RowKind::Insert,
            (None, None) => unreachable!("row count is the longer side"),
        };
        rows.push(Row {
            kind,
            old: old_line.map_or(Cell::FILLER, Cell::plain),
            new: new_line.map_or(Cell::FILLER, Cell::plain),
        });
    }
}

struct ScoreMatrix {
    old_start: usize,
    new_start: usize,
    width: usize,
    scores: Vec<Similarity>,
}

impl ScoreMatrix {
    fn new(diff: &FileDiff, old: &Range<usize>, new: &Range<usize>) -> Self {
        let old_lines: Vec<Bigrams> =
            old.clone().map(|line| Bigrams::new(diff.old.line(line))).collect();
        let new_lines: Vec<Bigrams> =
            new.clone().map(|line| Bigrams::new(diff.new.line(line))).collect();

        let mut scores = Vec::with_capacity(old_lines.len() * new_lines.len());
        for old_line in &old_lines {
            for new_line in &new_lines {
                scores.push(old_line.similarity(new_line));
            }
        }

        Self { old_start: old.start, new_start: new.start, width: new_lines.len(), scores }
    }

    fn score(&self, old: usize, new: usize) -> Similarity {
        self.scores[(old - self.old_start) * self.width + (new - self.new_start)]
    }
}

struct Bigrams<'a> {
    line: &'a str,
    sorted: Vec<(char, char)>,
}

impl Bigrams<'_> {
    fn new(line: &str) -> Bigrams<'_> {
        let mut sorted: Vec<(char, char)> = line.chars().zip(line.chars().skip(1)).collect();
        sorted.sort_unstable();
        Bigrams { line, sorted }
    }

    #[expect(clippy::cast_precision_loss, reason = "shared <= total bigram count")]
    fn similarity(&self, other: &Self) -> Similarity {
        if self.sorted.is_empty() || other.sorted.is_empty() {
            return if self.line == other.line { Similarity::IDENTICAL } else { Similarity::NONE };
        }

        let (mut left, mut right, mut shared) = (0, 0, 0usize);
        while left < self.sorted.len() && right < other.sorted.len() {
            match self.sorted[left].cmp(&other.sorted[right]) {
                Ordering::Less => left += 1,
                Ordering::Greater => right += 1,
                Ordering::Equal => {
                    shared += 1;
                    left += 1;
                    right += 1;
                }
            }
        }

        let total = self.sorted.len() + other.sorted.len();
        Similarity::new(2.0 * shared as f32 / total as f32)
    }
}

fn emphasized(emphasis: Emphasis, old: &str, new: &str) -> (Vec<Span>, Vec<Span>) {
    match emphasis {
        Emphasis::Words => {
            let (old_spans, new_spans) = word_spans(old, new);
            (close_gaps(old, old_spans), close_gaps(new, new_spans))
        }
        Emphasis::EachWord => word_spans(old, new),
        Emphasis::Characters => {
            spans(&TextDiff::configure().algorithm(Algorithm::Myers).diff_graphemes(old, new))
        }
        Emphasis::None => (Vec::new(), Vec::new()),
    }
}

fn word_spans(old: &str, new: &str) -> (Vec<Span>, Vec<Span>) {
    spans(&TextDiff::configure().algorithm(Algorithm::Patience).diff_unicode_words(old, new))
}

fn spans(diff: &TextDiff<'_, '_, str>) -> (Vec<Span>, Vec<Span>) {
    let old_offsets = word_offsets(diff.iter_old_slices());
    let new_offsets = word_offsets(diff.iter_new_slices());

    let (mut old_spans, mut new_spans) = (Vec::new(), Vec::new());
    for operation in diff.ops() {
        match *operation {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete { old_index, old_len, .. } => {
                old_spans.push(span(&old_offsets, old_index, old_len));
            }
            DiffOp::Insert { new_index, new_len, .. } => {
                new_spans.push(span(&new_offsets, new_index, new_len));
            }
            DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                old_spans.push(span(&old_offsets, old_index, old_len));
                new_spans.push(span(&new_offsets, new_index, new_len));
            }
        }
    }
    (merge_touching(old_spans), merge_touching(new_spans))
}

fn word_offsets<'a>(slices: impl Iterator<Item = &'a str>) -> Vec<usize> {
    let mut offsets = vec![0];
    let mut total = 0;
    for slice in slices {
        total += slice.len();
        offsets.push(total);
    }
    offsets
}

fn span(offsets: &[usize], index: usize, len: usize) -> Span {
    Span { start: offsets[index], end: offsets[index + len] }
}

fn close_gaps(line: &str, spans: Vec<Span>) -> Vec<Span> {
    let mut closed: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans {
        match closed.last_mut() {
            Some(last) if line[last.end..span.start].graphemes(true).count() == 1 => {
                last.end = span.end;
            }
            _ => closed.push(span),
        }
    }
    closed
}

fn merge_touching(spans: Vec<Span>) -> Vec<Span> {
    let mut merged: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans {
        match merged.last_mut() {
            Some(last) if last.end == span.start => last.end = span.end,
            _ => merged.push(span),
        }
    }
    merged
}
