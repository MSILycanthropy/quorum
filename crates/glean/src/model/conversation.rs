//! What happened on a pull request: comments, reviews, threads on lines, and the timeline.

use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::time::SystemTime;

use sift::diff::Side;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub created: SystemTime,
    pub edited: bool,
    pub pending: bool,
}

/// Where a thread hangs: on lines of one side of a file, on the file as a
/// whole, or on lines the diff no longer has, kept where they were when written.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Placement {
    Lines { side: Side, lines: RangeInclusive<usize> },
    File,
    Outdated { side: Side, lines: RangeInclusive<usize> },
}

/// A conversation about one file of the diff, opened by its first comment.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Thread {
    pub id: String,
    pub path: PathBuf,
    pub placement: Placement,
    pub comments: Vec<Comment>,
    pub resolved: bool,
}

/// Where a review stands. A pending review is a draft only its author can see.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ReviewState {
    Pending,
    #[default]
    Commented,
    Approved,
    ChangesRequested,
    Dismissed,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Review {
    pub id: String,
    pub author: String,
    pub state: ReviewState,
    pub body: String,
    pub submitted: Option<SystemTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Commit {
    pub id: String,
    pub headline: String,
    pub author: String,
    pub created: SystemTime,
}

/// Something done to the pull request itself, rather than said on it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Change {
    pub actor: String,
    pub at: SystemTime,
    pub kind: ChangeKind,
}

/// What a change did. A forge's events with no kind of their own here arrive
/// as `Other`, described in words.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Closed,
    Reopened,
    Merged,
    ReadyForReview,
    ConvertedToDraft,
    ReviewRequested { reviewer: String },
    Retitled { from: String, to: String },
    ForcePushed,
    Other(String),
}

/// One entry in a pull request's timeline, which runs oldest first.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Activity {
    Comment(Comment),
    Review(Review),
    Commit(Commit),
    Change(Change),
}
