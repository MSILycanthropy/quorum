//! Persistence keeps projects, workspaces, and transcript history across launches.

use std::path::{Path, PathBuf};

use quorum::agent::{Call, Choice, Entry, Permission, Progress, State, Transcript, Update, Usage};
use quorum::project::{
    Project, ProjectId, Workspace, WorkspaceId, WorkspaceKind, Worktree, WorktreeState,
};
use quorum::store::{CURRENT_SCHEMA_VERSION, Error, Store, TranscriptEvent};
use rusqlite::Connection;

fn database_path(name: &str) -> PathBuf {
    let directory = Path::new("target").join("store-tests");
    std::fs::create_dir_all(&directory).expect("test fixture directory can be created");
    let path = directory.join(format!("{name}.sqlite3"));
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(path.with_extension(format!("sqlite3{suffix}")));
    }
    path
}

fn project_id(value: &str) -> ProjectId {
    ProjectId::new(value).expect("test project identity is valid")
}

fn workspace_id(value: &str) -> WorkspaceId {
    WorkspaceId::new(value).expect("test workspace identity is valid")
}

fn project(id: &str, order: usize) -> Project {
    Project::new(
        project_id(id),
        format!("Project {id}"),
        PathBuf::from(format!("/work/{id}")),
        order,
    )
}

fn workspace(id: &str, project_id: &ProjectId, order: usize) -> Workspace {
    Workspace::new(
        workspace_id(id),
        project_id.clone(),
        WorkspaceKind::Linked,
        order,
        WorktreeState::unavailable(format!("/worktrees/{id}"), None),
    )
}

#[test]
fn opening_a_new_database_creates_the_current_schema_and_reopening_is_idempotent()
-> Result<(), Box<dyn std::error::Error>> {
    let path = database_path("schema");
    {
        let store = Store::open_at(&path)?;
        assert_eq!(store.schema_version()?, CURRENT_SCHEMA_VERSION);
    }

    let store = Store::open_at(&path)?;
    assert_eq!(store.schema_version()?, CURRENT_SCHEMA_VERSION);
    assert!(store.load_projects()?.is_empty());
    Ok(())
}

#[test]
fn projects_and_workspaces_round_trip_in_explicit_order() -> Result<(), Box<dyn std::error::Error>>
{
    let path = database_path("round-trip");
    let store = Store::open_at(&path)?;
    let first = project("first", 8);
    let second = project("second", 2);
    store.save_project(&first)?;
    store.save_project(&second)?;

    let first_workspace = workspace("first-workspace", first.id(), 9);
    let second_workspace = workspace("second-workspace", first.id(), 1);
    store.save_workspace(&first_workspace)?;
    store.save_workspace(&second_workspace)?;

    assert_eq!(store.load_projects()?, [second, first.clone()]);
    assert_eq!(store.load_workspaces(first.id())?, [second_workspace, first_workspace]);
    Ok(())
}

#[test]
fn an_unavailable_worktree_can_be_attached_again_without_changing_identity_or_order()
-> Result<(), Box<dyn std::error::Error>> {
    let path = database_path("availability");
    let store = Store::open_at(&path)?;
    let project = project("project", 0);
    store.save_project(&project)?;
    let mut workspace = Workspace::new(
        workspace_id("workspace"),
        project.id().clone(),
        WorkspaceKind::Linked,
        4,
        WorktreeState::attached(Worktree::new("/worktree/old", Some("old-branch".to_owned()))),
    );
    store.save_workspace(&workspace)?;

    workspace.mark_unavailable();
    store.save_workspace(&workspace)?;
    let unavailable =
        store.load_workspace(workspace.id())?.expect("the unavailable workspace was saved");
    assert_eq!(
        unavailable.worktree(),
        &WorktreeState::unavailable("/worktree/old", Some("old-branch".to_owned()))
    );

    workspace.attach(Worktree::new("/worktree/new", Some("new-branch".to_owned())));
    store.save_workspace(&workspace)?;
    let attached = store.load_workspace(workspace.id())?.expect("the attached workspace was saved");
    assert_eq!(attached.id(), workspace.id());
    assert_eq!(attached.order(), 4);
    assert_eq!(
        attached.worktree(),
        &WorktreeState::attached(Worktree::new("/worktree/new", Some("new-branch".to_owned())))
    );
    Ok(())
}

#[test]
fn foreign_keys_reject_orphans_and_delete_project_children()
-> Result<(), Box<dyn std::error::Error>> {
    let path = database_path("foreign-keys");
    let store = Store::open_at(&path)?;
    let missing_project = project_id("missing-project");
    let orphan = workspace("orphan", &missing_project, 0);
    assert!(matches!(store.save_workspace(&orphan), Err(Error::Sqlite(_))));

    let project = project("project", 0);
    store.save_project(&project)?;
    let workspace = workspace("workspace", project.id(), 0);
    store.save_workspace(&workspace)?;
    store.append_prompt(workspace.id(), "persisted".to_owned())?;
    assert!(store.delete_project(project.id())?);
    assert!(store.load_workspace(workspace.id())?.is_none());
    assert!(store.load_events(workspace.id())?.is_empty());
    Ok(())
}

#[test]
fn transcript_events_keep_sequence_and_replay_a_partial_conversation()
-> Result<(), Box<dyn std::error::Error>> {
    let path = database_path("transcript");
    let store = Store::open_at(&path)?;
    let project = project("project", 0);
    store.save_project(&project)?;
    let workspace = workspace("workspace", project.id(), 0);
    store.save_workspace(&workspace)?;

    let permission = Permission {
        id: "write".to_owned(),
        title: "write a file".to_owned(),
        choices: vec![Choice { id: "allow".to_owned(), name: "Allow".to_owned() }],
    };
    let call = Update::Ran {
        id: "call".to_owned(),
        title: Some("read a file".to_owned()),
        progress: Some(Progress::Running),
        output: Some(vec!["reading".to_owned()]),
    };
    let events = [
        TranscriptEvent::Prompt("make the change".to_owned()),
        TranscriptEvent::Update(Update::Titled("change".to_owned())),
        TranscriptEvent::Update(Update::Said("first ".to_owned())),
        TranscriptEvent::Update(Update::Said("second".to_owned())),
        TranscriptEvent::Update(Update::Thought("considering".to_owned())),
        TranscriptEvent::Update(Update::Asked(permission.clone())),
        TranscriptEvent::PermissionAnswer {
            permission: "write".to_owned(),
            choice: "allow".to_owned(),
        },
        TranscriptEvent::Update(call.clone()),
        TranscriptEvent::Update(Update::Ran {
            id: "call".to_owned(),
            title: None,
            progress: Some(Progress::Done),
            output: None,
        }),
    ];

    for event in &events {
        store.append_event(workspace.id(), event)?;
    }

    let stored = store.load_events(workspace.id())?;
    assert_eq!(
        stored.iter().map(|event| event.sequence).collect::<Vec<_>>(),
        (1..=i64::try_from(events.len()).expect("test event count fits in SQLite integer"))
            .collect::<Vec<_>>()
    );
    assert!(stored.iter().all(|event| event.encoding_version == 1));
    assert_eq!(
        stored.iter().map(|event| &event.event).collect::<Vec<_>>(),
        events.iter().collect::<Vec<_>>()
    );

    let mut expected = Transcript::default();
    expected.prompted("make the change".to_owned());
    expected.apply(Update::Titled("change".to_owned()));
    expected.apply(Update::Said("first ".to_owned()));
    expected.apply(Update::Said("second".to_owned()));
    expected.apply(Update::Thought("considering".to_owned()));
    expected.apply(Update::Asked(permission));
    expected.answered("write", "allow");
    expected.apply(call);
    expected.apply(Update::Ran {
        id: "call".to_owned(),
        title: None,
        progress: Some(Progress::Done),
        output: None,
    });
    assert_eq!(store.load_transcript(workspace.id())?, expected);
    assert_eq!(expected.state(), State::Working);
    assert_eq!(
        expected.entries(),
        [
            Entry::Prompt("make the change".to_owned()),
            Entry::Said("first second".to_owned()),
            Entry::Thought("considering".to_owned()),
            Entry::Answered("Allow".to_owned()),
            Entry::Ran(Call {
                id: "call".to_owned(),
                title: "read a file".to_owned(),
                progress: Progress::Done,
                output: vec!["reading".to_owned()],
            }),
        ]
    );
    Ok(())
}

#[test]
fn newer_schema_versions_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let path = database_path("newer-schema");
    {
        let store = Store::open_at(&path)?;
        assert_eq!(store.schema_version()?, CURRENT_SCHEMA_VERSION);
    }
    let connection = Connection::open(&path)?;
    connection.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)?;

    assert!(matches!(Store::open_at(&path), Err(Error::NewerSchema { found: 2, current: 1 })));
    Ok(())
}

#[test]
fn two_connections_can_open_and_use_the_same_wal_database() -> Result<(), Box<dyn std::error::Error>>
{
    let path = database_path("wal");
    let first = Store::open_at(&path)?;
    let second = Store::open_at(&path)?;
    let project = project("project", 0);
    first.save_project(&project)?;
    assert_eq!(second.load_project(project.id())?, Some(project));

    let connection = Connection::open(&path)?;
    let journal_mode: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    assert_eq!(journal_mode, "wal");
    Ok(())
}

#[test]
fn transcript_payloads_can_encode_usage_and_other_owned_update_values()
-> Result<(), Box<dyn std::error::Error>> {
    let path = database_path("transcript-values");
    let store = Store::open_at(&path)?;
    let project = project("project", 0);
    store.save_project(&project)?;
    let workspace = workspace("workspace", project.id(), 0);
    store.save_workspace(&workspace)?;

    let update = Update::Used(Usage {
        used: 12,
        size: 100,
        cost: Some(quorum::agent::Cost { amount: 0.25, currency: "USD".to_owned() }),
    });
    store.append_update(workspace.id(), update.clone())?;
    assert_eq!(store.load_events(workspace.id())?[0].event, TranscriptEvent::Update(update));
    Ok(())
}
