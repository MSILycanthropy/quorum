//! What changed between two snapshots, as line ranges.

use similar::{Algorithm, DiffOp, TextDiff};
use std::borrow::Cow;
use std::ops::Range;

/// One side of a diff: owned text, indexed by line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Source {
    text: String,
    lines: Vec<Range<usize>>,
    final_newline: bool,
}

impl Source {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let bytes = text.as_bytes();
        let final_newline = bytes.last() == Some(&b'\n');

        let mut lines = Vec::new();
        if !bytes.is_empty() {
            let mut start = 0;
            for (position, byte) in bytes.iter().enumerate() {
                if *byte != b'\n' {
                    continue;
                }
                let end = if position > start && bytes[position - 1] == b'\r' {
                    position - 1
                } else {
                    position
                };
                lines.push(start..end);
                start = position + 1;
            }
            lines.push(start..bytes.len());
        }

        Self { text, lines, final_newline }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn line(&self, index: usize) -> &str {
        &self.text[self.lines[index].clone()]
    }

    pub fn lines(&self) -> impl ExactSizeIterator<Item = &str> {
        self.lines.iter().map(|range| &self.text[range.clone()])
    }

    pub fn final_newline(&self) -> bool {
        self.final_newline
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Equal,
    Delete,
    Insert,
    Replace,
}

/// A contiguous run of lines sharing one change kind. Both ranges are always
/// positioned, so an empty one still says where its filler hangs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Change {
    pub kind: ChangeKind,
    pub old: Range<usize>,
    pub new: Range<usize>,
}

/// Whether lines that differ only in whitespace count as changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Whitespace {
    #[default]
    Compare,
    Ignore,
}

impl Whitespace {
    fn key(self, line: &str) -> Cow<'_, str> {
        match self {
            Self::Compare => Cow::Borrowed(line),
            Self::Ignore => Cow::Owned(line.split_whitespace().collect()),
        }
    }
}

/// How many lines a diff adds and removes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Counts {
    pub added: usize,
    pub removed: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FileDiff {
    pub old: Source,
    pub new: Source,
    pub changes: Vec<Change>,
}

impl FileDiff {
    pub fn compute(old: Source, new: Source) -> Self {
        Self::compute_with(old, new, Whitespace::Compare)
    }

    pub fn compute_with(old: Source, new: Source, whitespace: Whitespace) -> Self {
        let old_keys: Vec<Cow<str>> = old.lines().map(|line| whitespace.key(line)).collect();
        let new_keys: Vec<Cow<str>> = new.lines().map(|line| whitespace.key(line)).collect();
        let old_lines: Vec<&str> = old_keys.iter().map(AsRef::as_ref).collect();
        let new_lines: Vec<&str> = new_keys.iter().map(AsRef::as_ref).collect();

        let changes = TextDiff::configure()
            .algorithm(Algorithm::Histogram)
            .diff_slices(&old_lines, &new_lines)
            .ops()
            .iter()
            .map(Change::from_operation)
            .collect();

        Self { old, new, changes }
    }

    pub fn counts(&self) -> Counts {
        let written = |source: &Source, range: &Range<usize>| {
            let trailing = source.final_newline() && range.contains(&(source.len() - 1));
            range.len() - usize::from(trailing)
        };

        self.changes.iter().fold(Counts::default(), |counts, change| match change.kind {
            ChangeKind::Equal => counts,
            ChangeKind::Delete | ChangeKind::Insert | ChangeKind::Replace => Counts {
                added: counts.added + written(&self.new, &change.new),
                removed: counts.removed + written(&self.old, &change.old),
            },
        })
    }
}

impl Change {
    fn from_operation(operation: &DiffOp) -> Self {
        let (kind, old, new) = match *operation {
            DiffOp::Equal { old_index, new_index, len } => {
                (ChangeKind::Equal, old_index..old_index + len, new_index..new_index + len)
            }
            DiffOp::Delete { old_index, old_len, new_index } => {
                (ChangeKind::Delete, old_index..old_index + old_len, new_index..new_index)
            }
            DiffOp::Insert { old_index, new_index, new_len } => {
                (ChangeKind::Insert, old_index..old_index, new_index..new_index + new_len)
            }
            DiffOp::Replace { old_index, old_len, new_index, new_len } => (
                ChangeKind::Replace,
                old_index..old_index + old_len,
                new_index..new_index + new_len,
            ),
        };
        Self { kind, old, new }
    }
}
