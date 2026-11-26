//! What Glean shows, in its own terms, whichever forge it came from.

mod check;
mod conversation;
mod pull_request;
mod repository;

use std::path::PathBuf;

pub use check::{Check, CheckState};
pub use conversation::{
    Activity, Change, ChangeKind, Comment, Commit, Placement, Review, ReviewState, Thread,
};
pub use pull_request::{
    Branch, Details, Filter, MergeMethod, Mergeability, PullRequest, ReviewDecision, Standing,
    State, Verdict,
};
pub use repository::Repository;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a git repository: {0}")]
    NotARepository(PathBuf),
    #[error("not a repository name, which looks like owner/name: {0}")]
    NotARepositoryName(String),
    #[error("no GitHub remote named upstream or origin")]
    NoGitHubRemote,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
