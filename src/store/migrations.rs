//! Embedded schema migrations for Quorum's local database.

pub const CURRENT_SCHEMA_VERSION: i64 = 1;

pub const MIGRATIONS: &[(i64, &str)] = &[(
    1,
    "
    CREATE TABLE projects (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        common_directory TEXT UNIQUE NOT NULL,
        display_order INTEGER NOT NULL
    );

    CREATE TABLE workspaces (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        display_order INTEGER NOT NULL,
        worktree_state TEXT NOT NULL,
        worktree_path TEXT NOT NULL,
        branch TEXT
    );

    CREATE TABLE transcript_events (
        workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
        sequence INTEGER NOT NULL,
        event_kind TEXT NOT NULL,
        encoding_version INTEGER NOT NULL,
        payload TEXT NOT NULL,
        PRIMARY KEY (workspace_id, sequence)
    );
    ",
)];
