//! UI-independent project and workspace reconciliation.
#![expect(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "the public reconciliation surface has a single fallible operation and generated identities have an internal invariant"
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::git::ProjectInventory;
use crate::project::{Project, ProjectId, Workspace, WorkspaceId};
use crate::store::{Error as StoreError, Store};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("the system clock is before the Unix epoch")]
    Clock,
}

pub type Result<T> = std::result::Result<T, Error>;

/// A persisted project and its workspaces in explicit navigator order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectState {
    pub project: Project,
    pub workspaces: Vec<Workspace>,
}

/// The result of reconciling one discovered repository with local state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceCatalog {
    pub projects: Vec<ProjectState>,
}

impl WorkspaceCatalog {
    pub fn project(&self, id: &ProjectId) -> Option<&ProjectState> {
        self.projects.iter().find(|state| state.project.id() == id)
    }

    pub fn workspace(&self, id: &WorkspaceId) -> Option<&Workspace> {
        self.projects
            .iter()
            .flat_map(|state| &state.workspaces)
            .find(|workspace| workspace.id() == id)
    }

    pub fn workspaces(&self) -> impl Iterator<Item = &Workspace> {
        self.projects.iter().flat_map(|state| &state.workspaces)
    }
}

/// Reconciles discovered worktrees without changing persisted identities or order.
pub fn reconcile(store: &Store, inventory: &ProjectInventory) -> Result<WorkspaceCatalog> {
    let mut projects = store.load_projects()?;
    let project = projects
        .iter()
        .position(|project| same_path(project.common_directory(), &inventory.common_directory));
    let project_index = if let Some(index) = project {
        index
    } else {
        let project = Project::new(
            ProjectId::new(new_identity("project")?).expect("generated project identity is valid"),
            project_name(&inventory.common_directory),
            inventory.common_directory.clone(),
            next_order(projects.iter().map(Project::order)),
        );
        store.save_project(&project)?;
        projects.push(project);
        projects.len() - 1
    };

    let project_id = projects[project_index].id().clone();
    let mut workspaces = store.load_workspaces(&project_id)?;
    for workspace in &mut workspaces {
        workspace.mark_unavailable();
    }

    for discovered in &inventory.worktrees {
        let existing = workspaces.iter_mut().find(|workspace| {
            same_path(workspace.last_known_location(), discovered.worktree.location())
        });
        if let Some(workspace) = existing {
            workspace.attach_as(discovered.kind, discovered.worktree.clone());
        } else {
            let workspace = Workspace::new(
                WorkspaceId::new(new_identity("workspace")?)
                    .expect("generated workspace identity is valid"),
                project_id.clone(),
                discovered.kind,
                next_order(workspaces.iter().map(Workspace::order)),
                crate::project::WorktreeState::attached(discovered.worktree.clone()),
            );
            workspaces.push(workspace);
        }
    }
    for workspace in &workspaces {
        store.save_workspace(workspace)?;
    }

    projects.sort_by_key(Project::order);
    let mut states = Vec::with_capacity(projects.len());
    for project in projects {
        let workspaces = if project.id() == &project_id {
            workspaces.clone()
        } else {
            store.load_workspaces(project.id())?
        };
        states.push(ProjectState { project, workspaces });
    }

    Ok(WorkspaceCatalog { projects: states })
}

/// Reconciles the launched repository and every other persisted repository that is reachable.
pub fn reconcile_all(store: &Store, current: &ProjectInventory) -> Result<WorkspaceCatalog> {
    reconcile(store, current)?;
    let projects = store.load_projects()?;
    for project in projects {
        if same_path(project.common_directory(), &current.common_directory) {
            continue;
        }
        let workspaces = store.load_workspaces(project.id())?;
        let inventory = workspaces
            .iter()
            .map(|workspace| workspace.last_known_location().to_owned())
            .find_map(|location| {
                crate::git::Repository::discover(&location).ok().and_then(|repository| {
                    repository.inventory().ok().filter(|inventory| {
                        same_path(&inventory.common_directory, project.common_directory())
                    })
                })
            });
        if let Some(inventory) = inventory {
            reconcile(store, &inventory)?;
        } else {
            let mut workspaces = workspaces;
            for workspace in &mut workspaces {
                workspace.mark_unavailable();
                store.save_workspace(workspace)?;
            }
        }
    }

    let projects = store.load_projects()?;
    let projects = projects
        .into_iter()
        .map(|project| {
            let workspaces = store.load_workspaces(project.id())?;
            Ok(ProjectState { project, workspaces })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(WorkspaceCatalog { projects })
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize(left) == normalize(right)
}

fn normalize(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_or_else(|_| path.to_path_buf(), |current| current.join(path))
        }
    })
}

fn project_name(path: &Path) -> String {
    let display = if path.file_name().is_some_and(|name| name == ".git") {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    display
        .file_name()
        .map_or_else(|| display.display().to_string(), |name| name.to_string_lossy().into_owned())
}

fn next_order(orders: impl Iterator<Item = usize>) -> usize {
    orders.max().map_or(0, |order| order.saturating_add(1))
}

fn new_identity(prefix: &str) -> Result<String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let time = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| Error::Clock)?.as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(format!("{prefix}-{time}-{sequence}"))
}
