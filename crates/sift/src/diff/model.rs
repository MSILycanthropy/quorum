//! What changed between two snapshots, as line ranges.

use gix::diff::blob::{Algorithm, Diff, InternedInput};
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

    pub fn slice(&self, lines: Range<usize>) -> &str {
        match lines.end.checked_sub(1) {
            Some(last) if lines.start <= last => {
                &self.text[self.lines[lines.start].start..self.lines[last].end]
            }
            _ => "",
        }
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

        let mut input = InternedInput::default();
        input.before = old_keys.iter().map(|key| input.interner.intern(key.as_ref())).collect();
        input.after = new_keys.iter().map(|key| input.interner.intern(key.as_ref())).collect();
        let mut diff = Diff::compute(Algorithm::Histogram, &input);
        diff.postprocess_lines(&input);

        let changes = from_hunks(&diff, old.len(), new.len());

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

fn from_hunks(diff: &Diff, old_len: usize, new_len: usize) -> Vec<Change> {
    let mut changes = Vec::new();
    let (mut old_at, mut new_at) = (0, 0);
    for hunk in diff.hunks() {
        let old = hunk.before.start as usize..hunk.before.end as usize;
        let new = hunk.after.start as usize..hunk.after.end as usize;
        if old.start > old_at {
            changes.push(Change {
                kind: ChangeKind::Equal,
                old: old_at..old.start,
                new: new_at..new.start,
            });
        }
        let kind = match (old.is_empty(), new.is_empty()) {
            (true, _) => ChangeKind::Insert,
            (_, true) => ChangeKind::Delete,
            _ => ChangeKind::Replace,
        };
        (old_at, new_at) = (old.end, new.end);
        changes.push(Change { kind, old, new });
    }
    if old_at < old_len {
        changes.push(Change {
            kind: ChangeKind::Equal,
            old: old_at..old_len,
            new: new_at..new_len,
        });
    }

    changes
}
