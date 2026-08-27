//! That projects and workspaces retain their persisted identity and order.

use std::path::Path;
use std::str::FromStr;

use quorum::project::{
    IdentityError, Project, ProjectId, Workspace, WorkspaceId, WorkspaceKind, Worktree,
    WorktreeState,
};

#[test]
fn an_identity_is_nonempty_and_whitespace_free() {
    assert_eq!(ProjectId::new(""), Err(IdentityError::Empty));
    assert_eq!(WorkspaceId::new("workspace id"), Err(IdentityError::Whitespace));
}

#[test]
fn an_identity_round_trips_through_its_persisted_text() {
    let project = ProjectId::new("project-42").expect("the identity is valid");
    let persisted = project.to_string();

    assert_eq!(ProjectId::from_str(&persisted), Ok(project));
}

#[test]
fn a_project_keeps_its_identity_name_common_directory_and_order_separate() {
    let project_id = ProjectId::new("project-42").expect("the identity is valid");
    let project = Project::new(project_id.clone(), "Quorum", "/repos/quorum/.git", 3);

    assert_eq!(project.id(), &project_id);
    assert_eq!(project.display_name(), "Quorum");
    assert_eq!(project.common_directory(), Path::new("/repos/quorum/.git"));
    assert_eq!(project.order(), 3);
}

#[test]
fn a_workspace_keeps_its_project_identity_and_explicit_order_when_reattached() {
    let project_id = ProjectId::new("project-42").expect("the identity is valid");
    let workspace_id = WorkspaceId::new("workspace-7").expect("the identity is valid");
    let mut workspace = Workspace::new(
        workspace_id.clone(),
        project_id.clone(),
        WorkspaceKind::Linked,
        8,
        WorktreeState::attached(Worktree::new("/repos/quorum-feature", Some("feature".to_owned()))),
    );

    workspace.attach(Worktree::new("/repos/quorum-renamed", Some("updated".to_owned())));

    assert_eq!(workspace.id(), &workspace_id);
    assert_eq!(workspace.project_id(), &project_id);
    assert_eq!(workspace.order(), 8);
    assert_eq!(workspace.branch(), Some("updated"));
    assert_eq!(workspace.last_known_location(), Path::new("/repos/quorum-renamed"));
}

#[test]
fn marking_a_workspace_unavailable_keeps_its_identity_and_last_known_location() {
    let project_id = ProjectId::new("project-42").expect("the identity is valid");
    let workspace_id = WorkspaceId::new("workspace-7").expect("the identity is valid");
    let mut workspace = Workspace::new(
        workspace_id.clone(),
        project_id,
        WorkspaceKind::Linked,
        8,
        WorktreeState::attached(Worktree::new("/repos/quorum-feature", Some("feature".to_owned()))),
    );

    workspace.mark_unavailable();

    assert_eq!(workspace.id(), &workspace_id);
    assert!(!workspace.is_attached());
    assert_eq!(workspace.branch(), Some("feature"));
    assert_eq!(workspace.last_known_location(), Path::new("/repos/quorum-feature"));
}

#[test]
fn the_main_worktree_is_an_ordinary_main_workspace() {
    let project_id = ProjectId::new("project-42").expect("the identity is valid");
    let workspace = Workspace::main(
        WorkspaceId::new("main").expect("the identity is valid"),
        project_id,
        0,
        Worktree::new("/repos/quorum", Some("main".to_owned())),
    );

    assert_eq!(workspace.kind(), WorkspaceKind::Main);
    assert!(workspace.is_attached());
    assert_eq!(workspace.last_known_location(), Path::new("/repos/quorum"));
}
