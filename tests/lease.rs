//! Workspace leases prevent concurrent control and reclaim stale ownership.

use std::path::PathBuf;
use std::time::Duration;

use quorum::project::{Project, ProjectId, Workspace, WorkspaceId, WorkspaceKind, WorktreeState};
use quorum::store::{LeaseAcquisition, LeasePolicy, Store};

fn workspace() -> Workspace {
    let project = ProjectId::new("project").expect("test identity is valid");
    Workspace::new(
        WorkspaceId::new("workspace").expect("test identity is valid"),
        project,
        WorkspaceKind::Main,
        0,
        WorktreeState::unavailable("/missing", None),
    )
}

#[test]
fn a_live_foreign_lease_is_not_taken_without_explicit_takeover()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(":memory:")?;
    let project = Project::new(
        ProjectId::new("project").expect("test identity is valid"),
        "project",
        PathBuf::from("/project"),
        0,
    );
    store.save_project(&project)?;
    let workspace = workspace();
    store.save_workspace(&workspace)?;
    let policy = LeasePolicy::new(Duration::from_millis(10));
    assert!(matches!(
        store.acquire_lease(workspace.id(), "first", 100, policy, false)?,
        LeaseAcquisition::Acquired(_)
    ));
    assert!(matches!(
        store.acquire_lease(workspace.id(), "second", 101, policy, false)?,
        LeaseAcquisition::Held(_)
    ));
    assert_eq!(
        store.inspect_lease(workspace.id(), 101)?.expect("foreign lease is inspectable").owner,
        "first"
    );
    assert!(!store.renew_lease(workspace.id(), "second", 101, policy)?);
    assert!(store.renew_lease(workspace.id(), "first", 101, policy)?);
    assert!(!store.release_lease(workspace.id(), "second")?);
    assert!(store.release_lease(workspace.id(), "first")?);
    assert!(store.inspect_lease(workspace.id(), 101)?.is_none());
    Ok(())
}

#[test]
fn an_expired_lease_is_reclaimed_and_takeover_is_distinguishable()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(":memory:")?;
    let project = Project::new(
        ProjectId::new("project").expect("test identity is valid"),
        "project",
        PathBuf::from("/project"),
        0,
    );
    store.save_project(&project)?;
    let workspace = workspace();
    store.save_workspace(&workspace)?;
    let policy = LeasePolicy::new(Duration::from_millis(10));
    store.acquire_lease(workspace.id(), "first", 100, policy, false)?;
    assert!(matches!(
        store.acquire_lease(workspace.id(), "second", 111, policy, false)?,
        LeaseAcquisition::Reclaimed(_)
    ));
    store.acquire_lease(workspace.id(), "first", 200, policy, true)?;
    assert!(matches!(
        store.acquire_lease(workspace.id(), "second", 201, policy, true)?,
        LeaseAcquisition::TakenOver(_)
    ));
    Ok(())
}
