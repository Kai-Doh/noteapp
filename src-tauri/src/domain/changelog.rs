use chrono::Utc;
use rusqlite::{params, Connection, Row, Transaction};
use serde::Serialize;
use uuid::Uuid;

use crate::db::writer::{Actor, MutationOutcome, NodeSnapshot};

/// Inserts the append-only audit row for a mutation. Called exactly once per
/// write, inside the same transaction as the mutation itself, from
/// `db::writer::run_job` — never exposed as its own route.
pub fn insert_changelog_row(
    txn: &Transaction,
    actor: &Actor,
    outcome: &MutationOutcome,
) -> rusqlite::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    txn.execute(
        "INSERT INTO changelog (
            id, timestamp, actor, action, entity_type, entity_id,
            before_hash, after_hash, diff_json, reason,
            source_session_id, source_task_id, request_id, compiler_version
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            id,
            now,
            actor.kind.as_db_str(),
            outcome.action.as_db_str(),
            outcome.entity_type,
            outcome.entity_id,
            outcome.before_hash,
            outcome.after_hash,
            outcome.diff_json.to_string(),
            outcome.reason,
            actor.source_session_id,
            actor.source_task_id,
            actor.request_id,
            outcome.compiler_version,
        ],
    )?;
    Ok(id)
}

/// Full historical snapshot for rollback/diff, separate from the changelog's
/// diff-only record. Only called when a mutation touches a `nodes` row.
pub fn insert_node_revision_row(
    txn: &Transaction,
    actor: &Actor,
    node_id: &str,
    snapshot: &NodeSnapshot,
    changelog_id: &str,
) -> rusqlite::Result<i64> {
    let next_revision: i64 = txn.query_row(
        "SELECT COALESCE(MAX(revision_number), 0) + 1 FROM node_revisions WHERE node_id = ?1",
        params![node_id],
        |row| row.get(0),
    )?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    txn.execute(
        "INSERT INTO node_revisions (
            id, node_id, revision_number, title, content,
            properties_snapshot_json, content_hash, changelog_id, created_at, created_by
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            node_id,
            next_revision,
            snapshot.title,
            snapshot.content,
            snapshot
                .properties_snapshot_json
                .as_ref()
                .map(|v| v.to_string()),
            snapshot.content_hash,
            changelog_id,
            now,
            actor.kind.as_db_str(),
        ],
    )?;
    Ok(next_revision)
}

#[derive(Debug, Serialize)]
pub struct ChangelogEntryDto {
    pub id: String,
    pub timestamp: String,
    pub actor: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: String,
    pub diff_json: serde_json::Value,
    pub reason: Option<String>,
    pub compiler_version: Option<String>,
}

fn changelog_entry_row(row: &Row) -> rusqlite::Result<ChangelogEntryDto> {
    let diff_raw: String = row.get("diff_json")?;
    Ok(ChangelogEntryDto {
        id: row.get("id")?,
        timestamp: row.get("timestamp")?,
        actor: row.get("actor")?,
        action: row.get("action")?,
        entity_type: row.get("entity_type")?,
        entity_id: row.get("entity_id")?,
        diff_json: serde_json::from_str(&diff_raw).unwrap_or(serde_json::Value::Null),
        reason: row.get("reason")?,
        compiler_version: row.get("compiler_version")?,
    })
}

/// Read-only view over the append-only changelog — this table has no
/// update/delete route, and never will; this is the one and only way it's
/// exposed externally. Powers the AI activity feed (`actor='ai'`) but stays
/// general enough to filter by any actor.
pub fn list_changelog(conn: &Connection, actor: Option<&str>, limit: i64) -> rusqlite::Result<Vec<ChangelogEntryDto>> {
    let rows = if let Some(actor) = actor {
        let mut stmt = conn.prepare(
            "SELECT * FROM changelog WHERE actor = ?1 ORDER BY timestamp DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![actor, limit], changelog_entry_row)?.collect();
        rows
    } else {
        let mut stmt = conn.prepare("SELECT * FROM changelog ORDER BY timestamp DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit], changelog_entry_row)?.collect();
        rows
    };
    rows
}
