//! A pull request: the row it gets in a list, and everything loaded when it is opened.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::model::{Activity, Check, CheckState, Thread};

/// Where a pull request stands. A draft is open but not yet asking for review.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum State {
    Draft,
    #[default]
    Open,
    Closed,
    Merged,
}

/// What the reviews add up to, in a repository that requires them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ReviewDecision {
    #[default]
    ReviewRequired,
    Approved,
    ChangesRequested,
}

/// Whether the pull request merges cleanly into its base as it stands. A forge
/// works this out in the background, so it can be unknown for a while.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mergeability {
    #[default]
    Unknown,
    Clean,
    Conflicting,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MergeMethod {
    #[default]
    Merge,
    Squash,
    Rebase,
}

/// How a review lands when it is submitted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Verdict {
    #[default]
    Comment,
    Approve,
    RequestChanges,
}

/// Which of a repository's pull requests a list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Filter {
    #[default]
    Open,
    Mine,
    ReviewRequested,
    Drafts,
    Closed,
}

/// A branch, and the commit it pointed at when it was read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Branch {
    pub name: String,
    pub commit: String,
}

/// A pull request as a list shows it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub state: State,
    pub base: Branch,
    pub head: Branch,
    pub created: SystemTime,
    pub updated: SystemTime,
    pub review_decision: Option<ReviewDecision>,
    pub checks: Option<CheckState>,
    pub mergeability: Mergeability,
    pub labels: Vec<String>,
    pub requested_reviewers: Vec<String>,
    pub added: usize,
    pub removed: usize,
    pub changed_files: usize,
    pub comments: usize,
}

/// Everything about a pull request beyond its row in a list, loaded when it is opened.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Details {
    pub body: String,
    pub timeline: Vec<Activity>,
    pub threads: Vec<Thread>,
    pub checks: Vec<Check>,
    pub viewed: Vec<PathBuf>,
    pub merge_methods: Vec<MergeMethod>,
}
