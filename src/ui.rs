//! A terminal workspace for directing parallel coding agents.

mod conversation;
mod navigator;
mod palette;
mod prompt;
mod review;
mod split;
mod tab;
mod tree;
mod viewer;

pub use conversation::Conversation;
pub use navigator::{
    Navigator, NavigatorSnapshot, ProjectId, ProjectPresentation, WorkspaceId,
    WorkspacePresentation, WorkspaceState,
};
pub use palette::{Palette, PaletteAction};
pub use prompt::Prompt;
pub use review::Review;
pub use split::{Columns, Split};
pub use tab::{Activity, Opening, Tab};
pub use tree::{Row, Tree};
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
