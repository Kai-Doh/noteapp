use chrono::Utc;
use rusqlite::{params, Connection, Transaction};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::writer::{ActorKind, ChangeAction, MutationOutcome, NodeSnapshot, WriteError};
use crate::domain::fts;
use crate::domain::links;
use crate::domain::normalize::{normalize_title, slugify};
use crate::domain::properties::{self, PropertyInput};

const ALLOWED_NODE_TYPES: [&str; 7] = [
    "page", "wiki", "journal", "project", "index", "decision", "research",
];
const ALLOWED_EXPORT_POLICIES: [&str; 3] = ["export", "exclude", "redact"];

fn content_hash(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex().to_string()
}

/// Finds a slug that doesn't collide with an existing node, appending -2, -3, ...
/// as needed. Runs inside the write transaction, so no race is possible against
/// this single writer.
pub(crate) fn unique_slug(txn: &Transaction, base: &str) -> rusqlite::Result<String> {
    let mut candidate = base.to_string();
    let mut suffix = 2;
    loop {
        let exists: bool = txn.query_row(
            "SELECT EXISTS(SELECT 1 FROM nodes WHERE slug = ?1)",
            params![candidate],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(candidate);
        }
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateNodeInput {
    pub title: String,
    #[serde(default = "default_node_type")]
    pub node_type: String,
    #[serde(default)]
    pub content: String,
    pub vault_code: Option<String>,
    pub export_policy: Option<String>,
    #[serde(default)]
    pub properties: Vec<PropertyInput>,
}

fn default_node_type() -> String {
    "page".to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct PatchNodeInput {
    pub title: Option<String>,
    pub content: Option<String>,
    pub export_policy: Option<String>,
    #[serde(default)]
    pub properties: Vec<PropertyInput>,
}

#[derive(Debug, Serialize)]
pub struct NodeDto {
    pub id: String,
    pub title: String,
    pub title_normalized: String,
    pub slug: String,
    pub vault_code: Option<String>,
    pub content: String,
    pub node_type: String,
    pub export_policy: String,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub properties: Vec<PropertyInput>,
    pub links: Vec<links::OutgoingLinkDto>,
}

#[derive(Debug, Serialize)]
pub struct NodeSummaryDto {
    pub id: String,
    pub title: String,
    pub node_type: String,
    pub vault_code: Option<String>,
    pub updated_at: String,
    pub created_by: String,
    /// Backed by a `properties` row with `key = 'folder'` — freeform,
    /// slash-separated (e.g. "Fitness/Nutrition"), not a real filesystem
    /// path. Same "designated property, never real directories" convention
    /// node_type-based virtual folders already used.
    pub folder: Option<String>,
}

pub fn create_mutation(
    input: CreateNodeInput,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        if input.title.trim().is_empty() {
            return Err(WriteError::Invalid("title must not be empty".into()));
        }
        if !ALLOWED_NODE_TYPES.contains(&input.node_type.as_str()) {
            return Err(WriteError::Invalid(format!(
                "unknown node_type '{}'",
                input.node_type
            )));
        }
        let export_policy = input.export_policy.unwrap_or_else(|| "export".to_string());
        if !ALLOWED_EXPORT_POLICIES.contains(&export_policy.as_str()) {
            return Err(WriteError::Invalid(format!(
                "unknown export_policy '{export_policy}'"
            )));
        }

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let title_normalized = normalize_title(&input.title);
        let base_slug = slugify(&input.title);
        let slug = unique_slug(txn, &base_slug)?;
        let by = actor_kind.as_db_str();

        let insert_result = txn.execute(
            "INSERT INTO nodes (
                id, title, title_normalized, slug, vault_code, content, node_type,
                export_policy, created_by, updated_by, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                input.title,
                title_normalized,
                slug,
                input.vault_code,
                input.content,
                input.node_type,
                export_policy,
                by,
                by,
                now,
                now
            ],
        );
        match insert_result {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                return Err(WriteError::Conflict(
                    "vault_code already in use by another node".into(),
                ))
            }
            Err(e) => return Err(e.into()),
        }

        if !input.properties.is_empty() {
            properties::upsert_properties(txn, &id, &input.properties)?;
        }

        fts::index_new_node(txn, &id, &input.title, &input.content)?;
        links::reparse_links(txn, &id, &input.content)?;

        let hash = content_hash(&input.content);
        Ok(MutationOutcome {
            entity_type: "node",
            entity_id: id.clone(),
            action: ChangeAction::Create,
            diff_json: serde_json::json!({
                "title": input.title,
                "node_type": input.node_type,
                "vault_code": input.vault_code,
            }),
            reason: None,
            before_hash: None,
            after_hash: Some(hash.clone()),
            node_snapshot: Some(NodeSnapshot {
                title: input.title,
                content: input.content,
                properties_snapshot_json: None,
                content_hash: hash,
            }),
            compiler_version: None,
        })
    }
}

pub fn patch_mutation(
    node_id: String,
    input: PatchNodeInput,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let (mut title, mut content, deleted_at): (String, String, Option<String>) = txn
            .query_row(
                "SELECT title, content, deleted_at FROM nodes WHERE id = ?1",
                params![node_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| WriteError::NotFound(format!("node {node_id} not found")))?;
        if deleted_at.is_some() {
            return Err(WriteError::NotFound(format!("node {node_id} not found")));
        }

        let before_hash = content_hash(&content);
        let old_title = title.clone();
        let old_content = content.clone();

        if let Some(new_title) = &input.title {
            if new_title.trim().is_empty() {
                return Err(WriteError::Invalid("title must not be empty".into()));
            }
            title = new_title.clone();
        }
        if let Some(new_content) = &input.content {
            content = new_content.clone();
        }
        let export_policy_clause = if let Some(ep) = &input.export_policy {
            if !ALLOWED_EXPORT_POLICIES.contains(&ep.as_str()) {
                return Err(WriteError::Invalid(format!("unknown export_policy '{ep}'")));
            }
            Some(ep.clone())
        } else {
            None
        };

        let now = Utc::now().to_rfc3339();
        let by = actor_kind.as_db_str();
        // title_normalized is recomputed on rename; slug is intentionally left
        // stable once created so export paths / permalinks don't churn on rename.
        let title_normalized = normalize_title(&title);

        if let Some(export_policy) = &export_policy_clause {
            txn.execute(
                "UPDATE nodes SET title = ?1, title_normalized = ?2, content = ?3,
                    export_policy = ?4, updated_by = ?5, updated_at = ?6 WHERE id = ?7",
                params![title, title_normalized, content, export_policy, by, now, node_id],
            )?;
        } else {
            txn.execute(
                "UPDATE nodes SET title = ?1, title_normalized = ?2, content = ?3,
                    updated_by = ?4, updated_at = ?5 WHERE id = ?6",
                params![title, title_normalized, content, by, now, node_id],
            )?;
        }

        if !input.properties.is_empty() {
            properties::upsert_properties(txn, &node_id, &input.properties)?;
        }

        fts::reindex_updated_node(txn, &node_id, &old_title, &old_content, &title, &content)?;
        links::reparse_links(txn, &node_id, &content)?;

        let after_hash = content_hash(&content);
        Ok(MutationOutcome {
            entity_type: "node",
            entity_id: node_id,
            action: ChangeAction::Update,
            diff_json: serde_json::json!({
                "title_changed": input.title.is_some(),
                "content_changed": input.content.is_some(),
            }),
            reason: None,
            before_hash: Some(before_hash),
            after_hash: Some(after_hash.clone()),
            node_snapshot: Some(NodeSnapshot {
                title,
                content,
                properties_snapshot_json: None,
                content_hash: after_hash,
            }),
            compiler_version: None,
        })
    }
}

pub fn append_mutation(
    node_id: String,
    content_to_append: String,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let (title, existing_content, deleted_at): (String, String, Option<String>) = txn
            .query_row(
                "SELECT title, content, deleted_at FROM nodes WHERE id = ?1",
                params![node_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| WriteError::NotFound(format!("node {node_id} not found")))?;
        if deleted_at.is_some() {
            return Err(WriteError::NotFound(format!("node {node_id} not found")));
        }

        let before_hash = content_hash(&existing_content);
        let new_content = if existing_content.is_empty() {
            content_to_append.clone()
        } else {
            format!("{existing_content}\n{content_to_append}")
        };
        let now = Utc::now().to_rfc3339();
        let by = actor_kind.as_db_str();

        txn.execute(
            "UPDATE nodes SET content = ?1, updated_by = ?2, updated_at = ?3 WHERE id = ?4",
            params![new_content, by, now, node_id],
        )?;

        fts::reindex_updated_node(txn, &node_id, &title, &existing_content, &title, &new_content)?;
        links::reparse_links(txn, &node_id, &new_content)?;

        let after_hash = content_hash(&new_content);
        Ok(MutationOutcome {
            entity_type: "node",
            entity_id: node_id,
            action: ChangeAction::Append,
            diff_json: serde_json::json!({ "appended_chars": content_to_append.len() }),
            reason: None,
            before_hash: Some(before_hash),
            after_hash: Some(after_hash.clone()),
            node_snapshot: Some(NodeSnapshot {
                title,
                content: new_content,
                properties_snapshot_json: None,
                content_hash: after_hash,
            }),
            compiler_version: None,
        })
    }
}

pub fn delete_mutation(
    node_id: String,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let (title, content, deleted_at): (String, String, Option<String>) = txn
            .query_row(
                "SELECT title, content, deleted_at FROM nodes WHERE id = ?1",
                params![node_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| WriteError::NotFound(format!("node {node_id} not found")))?;
        if deleted_at.is_some() {
            return Err(WriteError::NotFound(format!("node {node_id} not found")));
        }

        let now = Utc::now().to_rfc3339();
        txn.execute(
            "UPDATE nodes SET deleted_at = ?1 WHERE id = ?2",
            params![now, node_id],
        )?;

        // This node's own outgoing links have nothing left to resolve from a
        // deleted note, so they're removed; other notes' links that still
        // point *at* this node are left as-is — every read path already
        // filters the target side on deleted_at, so they behave like a
        // dangling/unresolved reference rather than needing a second cleanup.
        txn.execute("DELETE FROM links WHERE source_node_id = ?1", params![node_id])?;
        fts::deindex_node(txn, &node_id, &title, &content)?;

        Ok(MutationOutcome {
            entity_type: "node",
            entity_id: node_id,
            action: ChangeAction::Delete,
            diff_json: serde_json::json!({ "title": title }),
            reason: None,
            before_hash: Some(content_hash(&content)),
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

// ---- Reads: run directly against a pooled read-only connection, no writer involved ----

pub fn get_node(conn: &Connection, node_id: &str) -> Result<Option<NodeDto>, WriteError> {
    let node = conn
        .query_row(
            "SELECT id, title, title_normalized, slug, vault_code, content, node_type,
                    export_policy, created_by, updated_by, created_at, updated_at
             FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
            params![node_id],
            |row| {
                Ok(NodeDto {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    title_normalized: row.get(2)?,
                    slug: row.get(3)?,
                    vault_code: row.get(4)?,
                    content: row.get(5)?,
                    node_type: row.get(6)?,
                    export_policy: row.get(7)?,
                    created_by: row.get(8)?,
                    updated_by: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    properties: Vec::new(),
                    links: Vec::new(),
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => WriteError::NotFound(format!("node {node_id} not found")),
            other => WriteError::Sqlite(other),
        });
    match node {
        Ok(mut dto) => {
            dto.properties = properties::list_properties(conn, node_id)?;
            dto.links = links::list_outgoing_links(conn, node_id)?;
            Ok(Some(dto))
        }
        Err(WriteError::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn list_nodes(
    conn: &Connection,
    node_type: Option<&str>,
    created_by: Option<&str>,
    limit: i64,
) -> rusqlite::Result<Vec<NodeSummaryDto>> {
    let mut where_clauses = vec!["n.deleted_at IS NULL".to_string()];
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(nt) = node_type {
        where_clauses.push("n.node_type = ?".to_string());
        params.push(rusqlite::types::Value::Text(nt.to_string()));
    }
    if let Some(by) = created_by {
        where_clauses.push("n.created_by = ?".to_string());
        params.push(rusqlite::types::Value::Text(by.to_string()));
    }
    params.push(rusqlite::types::Value::Integer(limit));

    let sql = format!(
        "SELECT n.id, n.title, n.node_type, n.vault_code, n.updated_at, n.created_by, f.value_text
         FROM nodes n
         LEFT JOIN properties f ON f.node_id = n.id AND f.key = 'folder'
         WHERE {} ORDER BY n.updated_at DESC LIMIT ?",
        where_clauses.join(" AND ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(NodeSummaryDto {
                id: row.get(0)?,
                title: row.get(1)?,
                node_type: row.get(2)?,
                vault_code: row.get(3)?,
                updated_at: row.get(4)?,
                created_by: row.get(5)?,
                folder: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Serialize)]
pub struct BacklinkDto {
    pub source_node_id: String,
    pub source_title: String,
    pub display_text: Option<String>,
    pub link_type: String,
}

/// Phase 1 populates `links`; until then this always returns an empty list, which
/// is the correct behavior (no links have been parsed yet) rather than an error.
pub fn get_backlinks(conn: &Connection, node_id: &str) -> rusqlite::Result<Vec<BacklinkDto>> {
    let mut stmt = conn.prepare(
        "SELECT l.source_node_id, n.title, l.display_text, l.link_type
         FROM links l JOIN nodes n ON n.id = l.source_node_id
         WHERE l.target_node_id = ?1 AND l.status = 'resolved' AND n.deleted_at IS NULL
         ORDER BY n.updated_at DESC",
    )?;
    let rows = stmt.query_map(params![node_id], |row| {
        Ok(BacklinkDto {
            source_node_id: row.get(0)?,
            source_title: row.get(1)?,
            display_text: row.get(2)?,
            link_type: row.get(3)?,
        })
    })?;
    rows.collect()
}
