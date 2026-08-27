//! Projects, workspaces and the worktrees they reconcile.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum IdentityError {
    #[error("an identity cannot be empty")]
    Empty,
    #[error("an identity cannot contain whitespace")]
    Whitespace,
}

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            #[expect(
                clippy::missing_errors_doc,
                reason = "the identity error names the only invalid values"
            )]
            pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(IdentityError::Empty);
                }
                if value.chars().any(char::is_whitespace) {
                    return Err(IdentityError::Whitespace);
                }

                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = IdentityError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

identity!(ProjectId);

identity!(WorkspaceId);

/// A worktree discovered at a location and its current branch.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Worktree {
    location: PathBuf,
    branch: Option<String>,
}

impl Worktree {
    pub fn new(location: impl Into<PathBuf>, branch: Option<String>) -> Self {
        Self { location: location.into(), branch }
    }

    pub fn location(&self) -> &Path {
        &self.location
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }
}

/// Whether a workspace currently has its worktree available.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WorktreeState {
    Attached(Worktree),
    Unavailable { last_location: PathBuf, branch: Option<String> },
}

impl WorktreeState {
    pub fn attached(worktree: Worktree) -> Self {
        Self::Attached(worktree)
    }

    pub fn unavailable(last_location: impl Into<PathBuf>, branch: Option<String>) -> Self {
        Self::Unavailable { last_location: last_location.into(), branch }
    }

    pub fn is_attached(&self) -> bool {
        matches!(self, Self::Attached(_))
    }

    pub fn last_known_location(&self) -> &Path {
        match self {
            Self::Attached(worktree) => worktree.location(),
            Self::Unavailable { last_location, .. } => last_location,
        }
    }

    pub fn branch(&self) -> Option<&str> {
        match self {
            Self::Attached(worktree) => worktree.branch(),
            Self::Unavailable { branch, .. } => branch.as_deref(),
        }
    }
}

/// The role of a workspace in its project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkspaceKind {
    Main,
    Linked,
}

/// A repository known to Quorum, identified by its Git common directory.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Project {
    id: ProjectId,
    display_name: String,
    common_directory: PathBuf,
    order: usize,
}

impl Project {
    pub fn new(
        id: ProjectId,
        display_name: impl Into<String>,
        common_directory: impl Into<PathBuf>,
        order: usize,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            common_directory: common_directory.into(),
            order,
        }
    }

    pub fn id(&self) -> &ProjectId {
        &self.id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn common_directory(&self) -> &Path {
        &self.common_directory
    }

    pub fn order(&self) -> usize {
        self.order
    }
}

/// A stable, ordered workspace and its currently known worktree.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Workspace {
    id: WorkspaceId,
    project_id: ProjectId,
    kind: WorkspaceKind,
    order: usize,
    worktree: WorktreeState,
}

impl Workspace {
    pub fn new(
        id: WorkspaceId,
        project_id: ProjectId,
        kind: WorkspaceKind,
        order: usize,
        worktree: WorktreeState,
    ) -> Self {
        Self { id, project_id, kind, order, worktree }
    }

    pub fn main(id: WorkspaceId, project_id: ProjectId, order: usize, worktree: Worktree) -> Self {
        Self::new(id, project_id, WorkspaceKind::Main, order, WorktreeState::attached(worktree))
    }

    pub fn id(&self) -> &WorkspaceId {
        &self.id
    }

    pub fn project_id(&self) -> &ProjectId {
        &self.project_id
    }

    pub fn kind(&self) -> WorkspaceKind {
        self.kind
    }

    pub fn order(&self) -> usize {
        self.order
    }

    pub fn worktree(&self) -> &WorktreeState {
        &self.worktree
    }

    pub fn is_attached(&self) -> bool {
        self.worktree.is_attached()
    }

    pub fn branch(&self) -> Option<&str> {
        self.worktree.branch()
    }

    pub fn last_known_location(&self) -> &Path {
        self.worktree.last_known_location()
    }

    pub fn attach(&mut self, worktree: Worktree) {
        self.worktree = WorktreeState::attached(worktree);
    }

    pub fn attach_as(&mut self, kind: WorkspaceKind, worktree: Worktree) {
        self.kind = kind;
        self.attach(worktree);
    }

    pub fn mark_unavailable(&mut self) {
        let WorktreeState::Attached(worktree) = &self.worktree else {
            return;
        };
        self.worktree = WorktreeState::unavailable(
            worktree.location().to_owned(),
            worktree.branch().map(str::to_owned),
        );
    }
}
