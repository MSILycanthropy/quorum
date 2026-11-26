//! The terminal surfaces: the file list, the diff, and the overlays over them.

mod annotation;
mod help;
mod icon;
mod palette;
mod review;
mod surface;
mod tree;
mod viewer;

pub use help::Help;
pub use icon::Icon;
pub use palette::{Palette, PaletteAction};
pub use review::Review;
pub use surface::{Opening, Surface};
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
    #[error(transparent)]
    Binding(#[from] crate::command::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Two panes against each other, or one column with deletions above insertions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    SideBySide,
    Unified,
}

/// One line of one side of a file, counted from one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Anchor {
    pub path: std::path::PathBuf,
    pub side: crate::diff::Side,
    pub line: usize,
}

/// Text the host anchors under one line of the diff. Sift draws it and never
/// reads meaning into it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Annotation {
    pub id: String,
    pub anchor: Anchor,
    pub heading: String,
    pub body: String,
    pub faded: bool,
    pub collapsed: bool,
}

/// What the surface asks of the program around it after a key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Control {
    #[default]
    Continue,
    Quit,
    Reload,
    Edit {
        path: std::path::PathBuf,
        line: Option<usize>,
    },
    Copy(String),
    Open {
        path: std::path::PathBuf,
        line: Option<usize>,
    },
    Host(&'static str),
    Viewed(std::path::PathBuf),
    Unviewed(std::path::PathBuf),
}
