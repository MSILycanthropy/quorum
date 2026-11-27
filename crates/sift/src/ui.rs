//! The terminal surfaces: the file list, the diff, and the overlays over them.

mod annotation;
mod icon;
mod review;
mod tree;
mod viewer;

pub use annotation::{Body, Draw, Text};
pub use icon::Icon;
pub use review::Review;
pub use tree::{Row, Tree};
pub use viewer::{Landing, Place, Viewer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Syntax(#[from] crate::syntax::Error),
    #[error("two annotations share the id {0}")]
    DuplicateAnnotation(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Two panes against each other, or one column with deletions above insertions.
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
pub enum Mode {
    #[default]
    SideBySide,
    Unified,
}

/// What marks a changed line beside its number, besides its background: a
/// bar, git's `+` and `-`, or nothing.
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
pub enum Indicators {
    #[default]
    None,
    Bars,
    Signs,
}

/// How a diff is first drawn. Keys change the layout, the wrapping and the
/// whitespace from there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Presentation {
    pub mode: Mode,
    pub wrap: bool,
    pub tab_width: usize,
    pub indicators: Indicators,
    pub backgrounds: bool,
    pub line_numbers: bool,
    pub whitespace: crate::diff::Whitespace,
    pub context: usize,
    pub emphasis: crate::diff::Emphasis,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            wrap: false,
            tab_width: 4,
            indicators: Indicators::default(),
            backgrounds: true,
            line_numbers: true,
            whitespace: crate::diff::Whitespace::default(),
            context: 3,
            emphasis: crate::diff::Emphasis::default(),
        }
    }
}

/// One line of one side of a file, counted from one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Anchor {
    pub path: std::path::PathBuf,
    pub side: crate::diff::Side,
    pub line: usize,
}

/// Lines of one side of a file, counted from one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Selection {
    pub path: std::path::PathBuf,
    pub side: crate::diff::Side,
    pub lines: std::ops::RangeInclusive<usize>,
}

/// A heading and a body the host anchors under one line of the diff. Sift
/// draws them and never reads meaning into them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Annotation {
    pub id: String,
    pub anchor: Anchor,
    pub heading: String,
    pub body: Body,
    pub faded: bool,
    pub collapsed: bool,
}

/// What the review needs from the program after a command: text for the
/// clipboard, a file it has no content for, or word that a file was marked
/// viewed or not.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Request {
    #[default]
    Nothing,
    Copy(String),
    Open {
        path: std::path::PathBuf,
        line: Option<usize>,
    },
    Viewed(std::path::PathBuf),
    Unviewed(std::path::PathBuf),
}

/// The file on screen and the lines the cursor or selection covers on its new
/// side, written `path`, `path:line` or `path:first-last`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Location {
    pub path: std::path::PathBuf,
    pub lines: Option<std::ops::RangeInclusive<usize>>,
}

impl std::fmt::Display for Location {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.path.display())?;
        match &self.lines {
            Some(lines) if lines.start() == lines.end() => write!(formatter, ":{}", lines.start()),
            Some(lines) => write!(formatter, ":{}-{}", lines.start(), lines.end()),
            None => Ok(()),
        }
    }
}

/// The condition that holds while lines are selected.
pub const SELECTING: &str = "selecting";

/// The condition that holds while a file outside the review is open.
pub const PEEKING: &str = "peeking";
