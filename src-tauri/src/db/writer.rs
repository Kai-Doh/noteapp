use rusqlite::{Connection, Transaction, TransactionBehavior};
use tokio::sync::{mpsc, oneshot};

use crate::domain::changelog;

/// Who performed a write. Values map 1:1 onto every `actor`/`created_by`/`updated_by`
/// CHECK constraint in the schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
    User,
    Ai,
    System,
}

impl ActorKind {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            ActorKind::User => "user",
            ActorKind::Ai => "ai",
            ActorKind::System => "system",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Actor {
    pub kind: ActorKind,
    pub request_id: String,
    pub source_session_id: Option<String>,
    pub source_task_id: Option<String>,
}

impl Actor {
    pub fn system(request_id: impl Into<String>) -> Self {
        Actor {
            kind: ActorKind::System,
            request_id: request_id.into(),
            source_session_id: None,
            source_task_id: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeAction {
    Create,
    Update,
    Append,
    Delete,
}

impl ChangeAction {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            ChangeAction::Create => "create",
            ChangeAction::Update => "update",
            ChangeAction::Append => "append",
            ChangeAction::Delete => "delete",
        }
    }
}

/// Populated only when `entity_type == "node"`; drives the `node_revisions` insert
/// that the writer performs atomically alongside the changelog row.
pub struct NodeSnapshot {
    pub title: String,
    pub content: String,
    pub properties_snapshot_json: Option<serde_json::Value>,
    pub content_hash: String,
}

pub struct MutationOutcome {
    pub entity_type: &'static str,
    pub entity_id: String,
    pub action: ChangeAction,
    pub diff_json: serde_json::Value,
    pub reason: Option<String>,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub node_snapshot: Option<NodeSnapshot>,
    /// Set only when this mutation was produced by the memory compiler's overflow
    /// eviction protocol (Phase 3), never by a direct user/AI edit.
    pub compiler_version: Option<&'static str>,
}

#[derive(thiserror::Error, Debug)]
pub enum WriteError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("writer queue is unavailable")]
    QueueClosed,
}

type MutationFn = Box<dyn FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send>;

/// What a completed write produced. `revision_number` is `Some` only when the
/// mutation touched a `nodes` row (i.e. `outcome.node_snapshot.is_some()`).
pub struct WriteResult {
    pub outcome: MutationOutcome,
    pub revision_number: Option<i64>,
}

struct WriteJob {
    actor: Actor,
    mutation: MutationFn,
    reply: oneshot::Sender<Result<WriteResult, WriteError>>,
}

/// Handle cloned into every axum handler that needs to write. The only way to
/// mutate the database — no handler ever touches a `Connection` directly.
#[derive(Clone)]
pub struct WriterHandle {
    tx: mpsc::Sender<WriteJob>,
}

impl WriterHandle {
    pub async fn write_tx<F>(&self, actor: Actor, mutation: F) -> Result<WriteResult, WriteError>
    where
        F: FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(WriteJob {
                actor,
                mutation: Box::new(mutation),
                reply: reply_tx,
            })
            .await
            .map_err(|_| WriteError::QueueClosed)?;
        reply_rx.await.map_err(|_| WriteError::QueueClosed)?
    }
}

/// Spawns the single dedicated OS thread that owns the one write connection for
/// the lifetime of the app. A channel + dedicated thread (rather than a
/// `Mutex<Connection>`) gives FIFO write ordering — important for changelog and
/// node_revisions sequencing — without `spawn_blocking` at every call site, and
/// keeps the single natural point where every mutation gets logged.
pub fn spawn_writer(mut conn: Connection) -> WriterHandle {
    let (tx, mut rx) = mpsc::channel::<WriteJob>(256);

    std::thread::spawn(move || {
        while let Some(job) = rx.blocking_recv() {
            let result = run_job(&mut conn, &job.actor, job.mutation);
            let _ = job.reply.send(result);
        }
    });

    WriterHandle { tx }
}

fn run_job(conn: &mut Connection, actor: &Actor, mutation: MutationFn) -> Result<WriteResult, WriteError> {
    let txn = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let outcome = mutation(&txn)?;
    let changelog_id = changelog::insert_changelog_row(&txn, actor, &outcome)?;
    let mut revision_number = None;
    if let Some(snapshot) = &outcome.node_snapshot {
        revision_number = Some(changelog::insert_node_revision_row(
            &txn,
            actor,
            &outcome.entity_id,
            snapshot,
            &changelog_id,
        )?);
    }
    txn.commit()?;
    Ok(WriteResult { outcome, revision_number })
}
