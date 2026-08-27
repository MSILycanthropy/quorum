//! Synchronous `SQLite` persistence for Quorum's user-local state.
#![expect(
    clippy::missing_errors_doc,
    reason = "the module's public fallible API uses its Error type to name failures"
)]

mod migrations;

use std::path::{Path, PathBuf};
use std::time::Duration;

use directories::ProjectDirs;
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::agent::{Transcript, Update};
use crate::project::{
    Project, ProjectId, Workspace, WorkspaceId, WorkspaceKind, Worktree, WorktreeState,
};

pub use migrations::CURRENT_SCHEMA_VERSION;

pub const CURRENT_EVENT_ENCODING_VERSION: i64 = 1;
const USER_LOCAL_DATABASE: &str = "quorum.sqlite3";
pub const DEFAULT_LEASE_POLICY: LeasePolicy = LeasePolicy { duration: Duration::from_secs(30) };

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "the database schema version {found} is newer than this Quorum build supports (maximum {current})"
    )]
    NewerSchema { found: i64, current: i64 },
    #[error(
        "the transcript event encoding version {found} is newer than this Quorum build supports (maximum {current})"
    )]
    NewerEventEncoding { found: i64, current: i64 },
    #[error("unsupported transcript event encoding version {found}")]
    UnsupportedEventEncoding { found: i64 },
    #[error("the platform has no user-local data directory")]
    NoUserDataDirectory,
    #[error("invalid {field} value in the database: {value}")]
    InvalidValue { field: &'static str, value: String },
    #[error("workspace does not exist: {0}")]
    WorkspaceNotFound(WorkspaceId),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The fixed lifetime used by a workspace ownership lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LeasePolicy {
    pub duration: Duration,
}

impl LeasePolicy {
    pub const fn new(duration: Duration) -> Self {
        Self { duration }
    }

    pub fn expires_at(self, now: i64) -> Result<i64> {
        let duration =
            i64::try_from(self.duration.as_millis()).map_err(|_| Error::InvalidValue {
                field: "lease duration",
                value: self.duration.as_millis().to_string(),
            })?;
        now.checked_add(duration)
            .ok_or_else(|| Error::InvalidValue { field: "lease expiry", value: now.to_string() })
    }
}

/// A live workspace lease as it is stored in `SQLite`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LeaseInfo {
    pub workspace_id: WorkspaceId,
    pub owner: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LeaseAcquisition {
    Acquired(LeaseInfo),
    Renewed(LeaseInfo),
    Reclaimed(LeaseInfo),
    TakenOver(LeaseInfo),
    Held(LeaseInfo),
}

impl Store {
    pub fn inspect_lease(&self, workspace_id: &WorkspaceId, now: i64) -> Result<Option<LeaseInfo>> {
        self.connection
            .query_row(
                "SELECT owner, expires_at FROM workspace_leases WHERE workspace_id = ?1",
                [workspace_id.as_str()],
                |row| {
                    Ok(LeaseInfo {
                        workspace_id: workspace_id.clone(),
                        owner: row.get(0)?,
                        expires_at: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
            .map(|lease| lease.filter(|lease| lease.expires_at > now))
    }

    pub fn acquire_lease(
        &self,
        workspace_id: &WorkspaceId,
        owner: &str,
        now: i64,
        policy: LeasePolicy,
        takeover: bool,
    ) -> Result<LeaseAcquisition> {
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)",
            [workspace_id.as_str()],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(Error::WorkspaceNotFound(workspace_id.clone()));
        }
        let expires_at = policy.expires_at(now)?;
        let expired = self
            .connection
            .query_row(
                "SELECT expires_at <= ?2 FROM workspace_leases WHERE workspace_id = ?1",
                params![workspace_id.as_str(), now],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(false);
        self.connection.execute(
            "DELETE FROM workspace_leases WHERE workspace_id = ?1 AND expires_at <= ?2",
            params![workspace_id.as_str(), now],
        )?;

        let inserted = self.connection.execute(
            "
            INSERT OR IGNORE INTO workspace_leases (workspace_id, owner, expires_at)
            VALUES (?1, ?2, ?3)
            ",
            params![workspace_id.as_str(), owner, expires_at],
        )?;
        if inserted != 0 {
            let lease = LeaseInfo {
                workspace_id: workspace_id.clone(),
                owner: owner.to_owned(),
                expires_at,
            };
            return Ok(if expired {
                LeaseAcquisition::Reclaimed(lease)
            } else {
                LeaseAcquisition::Acquired(lease)
            });
        }

        let current = self.inspect_lease(workspace_id, now)?;
        let Some(current) = current else {
            return self.acquire_lease(workspace_id, owner, now, policy, takeover);
        };
        if current.owner == owner {
            self.connection.execute(
                "UPDATE workspace_leases SET expires_at = ?2 WHERE workspace_id = ?1 AND owner = ?3",
                params![workspace_id.as_str(), expires_at, owner],
            )?;
            return Ok(LeaseAcquisition::Renewed(LeaseInfo {
                workspace_id: workspace_id.clone(),
                owner: owner.to_owned(),
                expires_at,
            }));
        }
        if !takeover {
            return Ok(LeaseAcquisition::Held(current));
        }

        self.connection.execute(
            "UPDATE workspace_leases SET owner = ?2, expires_at = ?3 WHERE workspace_id = ?1",
            params![workspace_id.as_str(), owner, expires_at],
        )?;
        Ok(LeaseAcquisition::TakenOver(LeaseInfo {
            workspace_id: workspace_id.clone(),
            owner: owner.to_owned(),
            expires_at,
        }))
    }

    pub fn renew_lease(
        &self,
        workspace_id: &WorkspaceId,
        owner: &str,
        now: i64,
        policy: LeasePolicy,
    ) -> Result<bool> {
        let expires_at = policy.expires_at(now)?;
        Ok(self.connection.execute(
            "
            UPDATE workspace_leases
            SET expires_at = ?3
            WHERE workspace_id = ?1 AND owner = ?2 AND expires_at > ?4
            ",
            params![workspace_id.as_str(), owner, expires_at, now],
        )? != 0)
    }

    pub fn release_lease(&self, workspace_id: &WorkspaceId, owner: &str) -> Result<bool> {
        Ok(self.connection.execute(
            "DELETE FROM workspace_leases WHERE workspace_id = ?1 AND owner = ?2",
            params![workspace_id.as_str(), owner],
        )? != 0)
    }
}

/// A Quorum-owned event that can be replayed into a transcript.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum TranscriptEvent {
    Prompt(String),
    PermissionAnswer { permission: String, choice: String },
    Update(Update),
}

impl TranscriptEvent {
    fn kind(&self) -> &'static str {
        match self {
            Self::Prompt(_) => "prompt",
            Self::PermissionAnswer { .. } => "permission_answer",
            Self::Update(_) => "update",
        }
    }

    fn apply(self, transcript: &mut Transcript) {
        match self {
            Self::Prompt(prompt) => transcript.prompted(prompt),
            Self::PermissionAnswer { permission, choice } => {
                transcript.answered(&permission, &choice);
            }
            Self::Update(update) => {
                transcript.apply(update);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredTranscriptEvent {
    pub sequence: i64,
    pub encoding_version: i64,
    pub event: TranscriptEvent,
}

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_at(path.as_ref())
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        create_parent(path)?;
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_user_local() -> Result<Self> {
        Self::open_at(&Self::user_local_path()?)
    }

    pub fn user_local_path() -> Result<PathBuf> {
        let project =
            ProjectDirs::from("com", "quorum", "Quorum").ok_or(Error::NoUserDataDirectory)?;
        Ok(project.data_dir().join(USER_LOCAL_DATABASE))
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn save_project(&self, project: &Project) -> Result<()> {
        let common_directory = project.common_directory().to_string_lossy().into_owned();
        let order = stored_order("project order", project.order())?;
        self.connection.execute(
            "
            INSERT INTO projects (id, name, common_directory, display_order)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                common_directory = excluded.common_directory,
                display_order = excluded.display_order
            ",
            params![project.id().as_str(), project.display_name(), common_directory, order],
        )?;
        Ok(())
    }

    pub fn load_project(&self, id: &ProjectId) -> Result<Option<Project>> {
        self.connection
            .query_row(
                "SELECT id, name, common_directory, display_order FROM projects WHERE id = ?1",
                [id.as_str()],
                project_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn load_projects(&self) -> Result<Vec<Project>> {
        let mut statement = self.connection.prepare(
            "
            SELECT id, name, common_directory, display_order
            FROM projects
            ORDER BY display_order, id
            ",
        )?;
        let projects = statement
            .query_map([], project_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(projects)
    }

    pub fn delete_project(&self, id: &ProjectId) -> Result<bool> {
        Ok(self.connection.execute("DELETE FROM projects WHERE id = ?1", [id.as_str()])? != 0)
    }

    pub fn save_workspace(&self, workspace: &Workspace) -> Result<()> {
        let kind = workspace_kind_to_string(workspace.kind());
        let (state, location, branch) = stored_worktree(workspace.worktree());
        let order = stored_order("workspace order", workspace.order())?;
        let location = location.to_string_lossy().into_owned();
        self.connection.execute(
            "
            INSERT INTO workspaces
                (id, project_id, kind, display_order, worktree_state, worktree_path, branch)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                kind = excluded.kind,
                display_order = excluded.display_order,
                worktree_state = excluded.worktree_state,
                worktree_path = excluded.worktree_path,
                branch = excluded.branch
            ",
            params![
                workspace.id().as_str(),
                workspace.project_id().as_str(),
                kind,
                order,
                state,
                location,
                branch
            ],
        )?;
        Ok(())
    }

    pub fn load_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>> {
        self.connection
            .query_row(
                "
                SELECT id, project_id, kind, display_order, worktree_state, worktree_path, branch
                FROM workspaces WHERE id = ?1
                ",
                [id.as_str()],
                workspace_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn load_workspaces(&self, project_id: &ProjectId) -> Result<Vec<Workspace>> {
        let mut statement = self.connection.prepare(
            "
            SELECT id, project_id, kind, display_order, worktree_state, worktree_path, branch
            FROM workspaces
            WHERE project_id = ?1
            ORDER BY display_order, id
            ",
        )?;
        let workspaces = statement
            .query_map([project_id.as_str()], workspace_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(workspaces)
    }

    pub fn delete_workspace(&self, id: &WorkspaceId) -> Result<bool> {
        Ok(self.connection.execute("DELETE FROM workspaces WHERE id = ?1", [id.as_str()])? != 0)
    }

    pub fn append_prompt(&self, workspace_id: &WorkspaceId, prompt: String) -> Result<i64> {
        self.append_event(workspace_id, &TranscriptEvent::Prompt(prompt))
    }

    pub fn append_permission_answer(
        &self,
        workspace_id: &WorkspaceId,
        permission: String,
        choice: String,
    ) -> Result<i64> {
        self.append_event(workspace_id, &TranscriptEvent::PermissionAnswer { permission, choice })
    }

    pub fn append_update(&self, workspace_id: &WorkspaceId, update: Update) -> Result<i64> {
        self.append_event(workspace_id, &TranscriptEvent::Update(update))
    }

    pub fn append_event(&self, workspace_id: &WorkspaceId, event: &TranscriptEvent) -> Result<i64> {
        let kind = event.kind();
        let payload = serde_json::to_string(event)?;
        Ok(self.connection.query_row(
            "
            INSERT INTO transcript_events
                (workspace_id, sequence, event_kind, encoding_version, payload)
            SELECT ?1, COALESCE(MAX(sequence), 0) + 1, ?2, ?3, ?4
            FROM transcript_events WHERE workspace_id = ?1
            RETURNING sequence
            ",
            params![workspace_id.as_str(), kind, CURRENT_EVENT_ENCODING_VERSION, payload],
            |row| row.get(0),
        )?)
    }

    pub fn load_events(&self, workspace_id: &WorkspaceId) -> Result<Vec<StoredTranscriptEvent>> {
        let mut statement = self.connection.prepare(
            "
            SELECT sequence, event_kind, encoding_version, payload
            FROM transcript_events
            WHERE workspace_id = ?1
            ORDER BY sequence
            ",
        )?;
        let mut rows = statement.query([workspace_id.as_str()])?;
        let mut events = Vec::new();
        while let Some(row) = rows.next()? {
            let sequence = row.get(0)?;
            let kind: String = row.get(1)?;
            let encoding_version = row.get(2)?;
            if encoding_version != CURRENT_EVENT_ENCODING_VERSION {
                if encoding_version > CURRENT_EVENT_ENCODING_VERSION {
                    return Err(Error::NewerEventEncoding {
                        found: encoding_version,
                        current: CURRENT_EVENT_ENCODING_VERSION,
                    });
                }
                return Err(Error::UnsupportedEventEncoding { found: encoding_version });
            }
            let event: TranscriptEvent = serde_json::from_str(&row.get::<_, String>(3)?)?;
            if event.kind() != kind {
                return Err(Error::InvalidValue { field: "event kind", value: kind });
            }
            events.push(StoredTranscriptEvent { sequence, encoding_version, event });
        }
        Ok(events)
    }

    pub fn load_transcript(&self, workspace_id: &WorkspaceId) -> Result<Transcript> {
        let mut transcript = Transcript::default();
        for event in self.load_events(workspace_id)? {
            event.event.apply(&mut transcript);
        }
        Ok(transcript)
    }

    fn from_connection(mut connection: Connection) -> Result<Self> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&mut connection)?;
        Ok(Self { connection })
    }
}

fn create_parent(path: &Path) -> Result<()> {
    if path.as_os_str() == ":memory:" {
        return Ok(());
    }
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn migrate(connection: &mut Connection) -> Result<()> {
    let current: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > CURRENT_SCHEMA_VERSION {
        return Err(Error::NewerSchema { found: current, current: CURRENT_SCHEMA_VERSION });
    }
    for &(version, sql) in migrations::MIGRATIONS.iter().filter(|(version, _)| *version > current) {
        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
    }
    Ok(())
}

fn project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    let id = project_id_from_row(row, 0)?;
    let order = order_from_row(row, 3)?;
    Ok(Project::new(id, row.get::<_, String>(1)?, PathBuf::from(row.get::<_, String>(2)?), order))
}

fn workspace_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    let id = workspace_id_from_row(row, 0)?;
    let project_id = project_id_from_row(row, 1)?;
    let kind = workspace_kind_from_string(&row.get::<_, String>(2)?, 2)?;
    let order = order_from_row(row, 3)?;
    let state: String = row.get(4)?;
    let location = PathBuf::from(row.get::<_, String>(5)?);
    let branch = row.get(6)?;
    let worktree = worktree_state_from_stored(&state, location, branch, 4)?;
    Ok(Workspace::new(id, project_id, kind, order, worktree))
}

fn project_id_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<ProjectId> {
    let value: String = row.get(index)?;
    ProjectId::new(value.clone())
        .map_err(|_| invalid_stored_value(index, "project identity", value))
}

fn workspace_id_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<WorkspaceId> {
    let value: String = row.get(index)?;
    WorkspaceId::new(value.clone())
        .map_err(|_| invalid_stored_value(index, "workspace identity", value))
}

fn stored_order(field: &'static str, order: usize) -> Result<i64> {
    i64::try_from(order).map_err(|_| Error::InvalidValue { field, value: order.to_string() })
}

fn order_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<usize> {
    let order: i64 = row.get(index)?;
    usize::try_from(order).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, order))
}

fn workspace_kind_to_string(kind: WorkspaceKind) -> &'static str {
    match kind {
        WorkspaceKind::Main => "main",
        WorkspaceKind::Linked => "linked",
    }
}

fn workspace_kind_from_string(value: &str, index: usize) -> rusqlite::Result<WorkspaceKind> {
    match value {
        "main" => Ok(WorkspaceKind::Main),
        "linked" => Ok(WorkspaceKind::Linked),
        _ => Err(invalid_stored_value(index, "workspace kind", value.to_owned())),
    }
}

fn stored_worktree(state: &WorktreeState) -> (&'static str, &Path, Option<&str>) {
    match state {
        WorktreeState::Attached(worktree) => ("attached", worktree.location(), worktree.branch()),
        WorktreeState::Unavailable { last_location, branch } => {
            ("unavailable", last_location, branch.as_deref())
        }
    }
}

fn worktree_state_from_stored(
    state: &str,
    location: PathBuf,
    branch: Option<String>,
    index: usize,
) -> rusqlite::Result<WorktreeState> {
    match state {
        "attached" => Ok(WorktreeState::attached(Worktree::new(location, branch))),
        "unavailable" => Ok(WorktreeState::unavailable(location, branch)),
        _ => Err(invalid_stored_value(index, "worktree state", state.to_owned())),
    }
}

fn invalid_stored_value(index: usize, field: &'static str, value: String) -> rusqlite::Error {
    let source = Error::InvalidValue { field, value };
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(source))
}
