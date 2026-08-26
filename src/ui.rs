//! The review surface: a list of changed files, and the diff of one of them.

mod viewer;

pub use viewer::Viewer;

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
