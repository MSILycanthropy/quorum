//! Reconciliation adopts, preserves and detaches repository worktrees.

use std::path::{Path, PathBuf};

use quorum::application::reconcile;
use quorum::git::{ProjectInventory, WorktreeInventory};
use quorum::project::{ProjectId, WorkspaceKind, Worktree};
use quorum::store::Store;

fn database_path(name: &str) -> PathBuf {
    let directory = Path::new("target").join("application-tests");
    std::fs::create_dir_all(&directory).expect("test fixture directory can be created");
    let path = directory.join(format!("{name}.sqlite3"));
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(path.with_extension(format!("sqlite3{suffix}")));
    }
    path
}

fn inventory(common: &Path, worktrees: &[(&Path, WorkspaceKind)]) -> ProjectInventory {
    ProjectInventory {
        common_directory: common.to_path_buf(),
        worktrees: worktrees
            .iter()
            .map(|(location, kind)| WorktreeInventory {
                kind: *kind,
                worktree: Worktree::new(*location, Some(location.display().to_string())),
            })
            .collect(),
    }
}

#[test]
fn first_reconciliation_adopts_main_and_linked_worktrees() -> Result<(), Box<dyn std::error::Error>>
{
    let root = PathBuf::from("target/application-tests/first");
    let main = root.join("main");
    let linked = root.join("linked");
    std::fs::create_dir_all(&main)?;
    std::fs::create_dir_all(&linked)?;
    let store = Store::open(database_path("first"))?;

    let catalog = reconcile(
        &store,
        &inventory(&root, &[(&main, WorkspaceKind::Main), (&linked, WorkspaceKind::Linked)]),
    )?;
    let project = &catalog.projects[0];
    assert_eq!(project.workspaces.len(), 2);
    assert_eq!(project.workspaces[0].kind(), WorkspaceKind::Main);
    assert_eq!(project.workspaces[1].kind(), WorkspaceKind::Linked);
    assert!(project.workspaces.iter().all(quorum::project::Workspace::is_attached));
    Ok(())
}

#[test]
fn repeating_reconciliation_preserves_ids_order_and_transcript()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from("target/application-tests/repeat");
    let main = root.join("main");
    let linked = root.join("linked");
    std::fs::create_dir_all(&main)?;
    std::fs::create_dir_all(&linked)?;
    let store = Store::open(database_path("repeat"))?;
    let first_inventory =
        inventory(&root, &[(&main, WorkspaceKind::Main), (&linked, WorkspaceKind::Linked)]);
    let first = reconcile(&store, &first_inventory)?;
    let first_project = &first.projects[0];
    let first_ids =
        first_project.workspaces.iter().map(|workspace| workspace.id().clone()).collect::<Vec<_>>();
    store.append_prompt(&first_ids[0], "keep this".to_owned())?;

    let second = reconcile(&store, &first_inventory)?;
    let second_project = &second.projects[0];
    assert_eq!(second_project.project.id(), first_project.project.id());
    assert_eq!(
        second_project.workspaces.iter().map(quorum::project::Workspace::id).collect::<Vec<_>>(),
        first_ids.iter().collect::<Vec<_>>()
    );
    assert_eq!(store.load_transcript(&first_ids[0])?.entries().len(), 1);
    Ok(())
}

#[test]
fn missing_and_returning_worktrees_keep_the_same_workspace_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from("target/application-tests/return");
    let main = root.join("main");
    let linked = root.join("linked");
    std::fs::create_dir_all(&main)?;
    std::fs::create_dir_all(&linked)?;
    let store = Store::open(database_path("return"))?;
    let full = inventory(&root, &[(&main, WorkspaceKind::Main), (&linked, WorkspaceKind::Linked)]);
    let first = reconcile(&store, &full)?;
    let linked_id = first.projects[0].workspaces[1].id().clone();

    let missing = reconcile(&store, &inventory(&root, &[(&main, WorkspaceKind::Main)]))?;
    let unavailable = missing.projects[0]
        .workspaces
        .iter()
        .find(|workspace| workspace.id() == &linked_id)
        .expect("missing worktree remains visible");
    assert!(!unavailable.is_attached());
    assert_eq!(unavailable.id(), &linked_id);

    let returned = reconcile(&store, &full)?;
    let attached = returned.projects[0]
        .workspaces
        .iter()
        .find(|workspace| workspace.id() == &linked_id)
        .expect("returning worktree is retained");
    assert!(attached.is_attached());
    Ok(())
}

#[test]
fn reconciling_two_repositories_keeps_both_projects_selectable()
-> Result<(), Box<dyn std::error::Error>> {
    let first_root = PathBuf::from("target/application-tests/project-one");
    let second_root = PathBuf::from("target/application-tests/project-two");
    let first_main = first_root.join("main");
    let second_main = second_root.join("main");
    std::fs::create_dir_all(&first_main)?;
    std::fs::create_dir_all(&second_main)?;
    let store = Store::open(database_path("projects"))?;
    reconcile(&store, &inventory(&first_root, &[(&first_main, WorkspaceKind::Main)]))?;
    let catalog =
        reconcile(&store, &inventory(&second_root, &[(&second_main, WorkspaceKind::Main)]))?;
    assert_eq!(catalog.projects.len(), 2);
    assert_ne!(catalog.projects[0].project.id(), catalog.projects[1].project.id());
    assert!(
        catalog
            .projects
            .iter()
            .all(|project| { ProjectId::new(project.project.id().as_str()).is_ok() })
    );
    Ok(())
}
