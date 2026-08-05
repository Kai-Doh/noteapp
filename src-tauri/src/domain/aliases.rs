use chrono::Utc;
use rusqlite::{params, Transaction};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::writer::{ActorKind, ChangeAction, MutationOutcome, WriteError};
use crate::domain::normalize::normalize_title;

#[derive(Debug, Serialize)]
pub struct AliasDto {
    pub id: String,
    pub node_id: String,
    pub alias: String,
    pub normalized_alias: String,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAliasInput {
    pub alias: String,
}

pub fn create_mutation(
    node_id: String,
    input: CreateAliasInput,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        if input.alias.trim().is_empty() {
            return Err(WriteError::Invalid("alias must not be empty".into()));
        }
        let node_exists: bool = txn.query_row(
            "SELECT EXISTS(SELECT 1 FROM nodes WHERE id = ?1 AND deleted_at IS NULL)",
            params![node_id],
            |row| row.get(0),
        )?;
        if !node_exists {
            return Err(WriteError::NotFound(format!("node {node_id} not found")));
        }

        let normalized_alias = normalize_title(&input.alias);
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let by = actor_kind.as_db_str();

        let result = txn.execute(
            "INSERT INTO aliases (id, node_id, alias, normalized_alias, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, node_id, input.alias, normalized_alias, by, now],
        );
        match result {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                return Err(WriteError::Conflict(format!(
                    "alias '{}' is already in use",
                    input.alias
                )))
            }
            Err(e) => return Err(e.into()),
        }

        Ok(MutationOutcome {
            entity_type: "alias",
            entity_id: id,
            action: ChangeAction::Create,
            diff_json: serde_json::json!({ "node_id": node_id, "alias": input.alias }),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

pub fn delete_mutation(
    alias_id: String,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let node_id: String = txn
            .query_row(
                "SELECT node_id FROM aliases WHERE id = ?1",
                params![alias_id],
                |row| row.get(0),
            )
            .map_err(|_| WriteError::NotFound(format!("alias {alias_id} not found")))?;
        txn.execute("DELETE FROM aliases WHERE id = ?1", params![alias_id])?;

        Ok(MutationOutcome {
            entity_type: "alias",
            entity_id: alias_id,
            action: ChangeAction::Delete,
            diff_json: serde_json::json!({ "node_id": node_id }),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

pub fn list_aliases(conn: &rusqlite::Connection, node_id: &str) -> rusqlite::Result<Vec<AliasDto>> {
    let mut stmt = conn.prepare(
        "SELECT id, node_id, alias, normalized_alias, created_by, created_at
         FROM aliases WHERE node_id = ?1 ORDER BY alias",
    )?;
    let rows = stmt.query_map(params![node_id], |row| {
        Ok(AliasDto {
            id: row.get(0)?,
            node_id: row.get(1)?,
            alias: row.get(2)?,
            normalized_alias: row.get(3)?,
            created_by: row.get(4)?,
            created_at: row.get(5)?,
        })
    })?;
    rows.collect()
}
