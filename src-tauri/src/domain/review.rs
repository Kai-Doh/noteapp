use chrono::Utc;
use rusqlite::{params, Connection, Row, Transaction};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::writer::{Actor, ActorKind, ChangeAction, MutationOutcome, WriteError};
use crate::domain::memory::{HotMemoryInput, PatchHotMemoryInput, PatchUserProfileInput, UserProfileInput};
use crate::domain::node::{CreateNodeInput, PatchNodeInput};

const ACTIONS: [&str; 3] = ["create", "update", "delete"];
const ENTITY_TYPES: [&str; 3] = ["node", "hot_memory", "user_profile"];
const CONFIDENCE: [&str; 3] = ["high", "medium", "low"];

fn validate_in(value: &str, allowed: &[&str], field: &str) -> Result<(), WriteError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(WriteError::Invalid(format!("unknown {field} '{value}'")))
    }
}

fn json_err(e: serde_json::Error) -> WriteError {
    WriteError::Invalid(format!("invalid proposed_diff_json: {e}"))
}

#[derive(Debug)]
struct ReviewItem {
    proposed_action: String,
    entity_type: String,
    entity_id: Option<String>,
    proposed_diff_json: String,
    status: String,
}

fn required_entity_id(item: &ReviewItem) -> Result<String, WriteError> {
    item.entity_id
        .clone()
        .ok_or_else(|| WriteError::Invalid("proposal is missing entity_id".into()))
}

#[derive(Debug, Serialize)]
pub struct ReviewItemDto {
    pub id: String,
    pub created_at: String,
    pub actor: String,
    pub proposed_action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub proposed_diff_json: serde_json::Value,
    pub reason: Option<String>,
    pub confidence: Option<String>,
    pub status: String,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<String>,
    pub applied_changelog_id: Option<String>,
}

fn review_item_dto_row(row: &Row) -> rusqlite::Result<ReviewItemDto> {
    let diff_raw: String = row.get("proposed_diff_json")?;
    Ok(ReviewItemDto {
        id: row.get("id")?,
        created_at: row.get("created_at")?,
        actor: row.get("actor")?,
        proposed_action: row.get("proposed_action")?,
        entity_type: row.get("entity_type")?,
        entity_id: row.get("entity_id")?,
        proposed_diff_json: serde_json::from_str(&diff_raw).unwrap_or(serde_json::Value::Null),
        reason: row.get("reason")?,
        confidence: row.get("confidence")?,
        status: row.get("status")?,
        resolved_by: row.get("resolved_by")?,
        resolved_at: row.get("resolved_at")?,
        applied_changelog_id: row.get("applied_changelog_id")?,
    })
}

pub fn list_review_items(conn: &Connection, status: Option<&str>) -> rusqlite::Result<Vec<ReviewItemDto>> {
    let rows = if let Some(status) = status {
        let mut stmt = conn.prepare(
            "SELECT * FROM ai_review_queue WHERE status = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![status], review_item_dto_row)?.collect();
        rows
    } else {
        let mut stmt = conn.prepare("SELECT * FROM ai_review_queue ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], review_item_dto_row)?.collect();
        rows
    };
    rows
}

#[derive(Debug, Deserialize)]
pub struct ProposeInput {
    pub proposed_action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub proposed_diff_json: serde_json::Value,
    pub reason: Option<String>,
    pub confidence: Option<String>,
}

/// Only an AI-scoped caller can propose — the schema's `actor` CHECK
/// constraint on `ai_review_queue` only allows `'ai'`, and semantically a
/// human/system writing directly wouldn't need the review queue at all (they
/// already have direct-write access).
pub fn propose_mutation(
    input: ProposeInput,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        if actor_kind != ActorKind::Ai {
            return Err(WriteError::Invalid(
                "only an AI-scoped token can propose review items".into(),
            ));
        }
        validate_in(&input.proposed_action, &ACTIONS, "proposed_action")?;
        validate_in(&input.entity_type, &ENTITY_TYPES, "entity_type")?;
        if let Some(confidence) = &input.confidence {
            validate_in(confidence, &CONFIDENCE, "confidence")?;
        }
        if input.proposed_action != "create" && input.entity_id.is_none() {
            return Err(WriteError::Invalid(
                "update/delete proposals must include entity_id".into(),
            ));
        }

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let diff_json = input.proposed_diff_json.to_string();

        txn.execute(
            "INSERT INTO ai_review_queue (
                id, created_at, actor, proposed_action, entity_type, entity_id,
                proposed_diff_json, reason, confidence, status
            ) VALUES (?1, ?2, 'ai', ?3, ?4, ?5, ?6, ?7, ?8, 'pending')",
            params![
                id,
                now,
                input.proposed_action,
                input.entity_type,
                input.entity_id,
                diff_json,
                input.reason,
                input.confidence,
            ],
        )?;

        Ok(MutationOutcome {
            entity_type: "ai_review_queue",
            entity_id: id,
            action: ChangeAction::Create,
            diff_json: serde_json::json!({
                "proposed_action": input.proposed_action,
                "entity_type": input.entity_type,
            }),
            reason: input.reason,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

fn load_pending_item(txn: &Transaction, id: &str) -> Result<ReviewItem, WriteError> {
    txn.query_row(
        "SELECT proposed_action, entity_type, entity_id, proposed_diff_json, status
         FROM ai_review_queue WHERE id = ?1",
        params![id],
        |row| {
            Ok(ReviewItem {
                proposed_action: row.get(0)?,
                entity_type: row.get(1)?,
                entity_id: row.get(2)?,
                proposed_diff_json: row.get(3)?,
                status: row.get(4)?,
            })
        },
    )
    .map_err(|_| WriteError::NotFound(format!("review item {id} not found")))
}

/// Approving/rejecting/applying is deliberately never available to an AI
/// actor — the entire point of the review queue is a human (or a pre-approved
/// system automation) in the loop between "AI proposed this" and "this is now
/// durable truth."
fn require_non_ai(actor_kind: ActorKind) -> Result<(), WriteError> {
    if actor_kind == ActorKind::Ai {
        return Err(WriteError::Invalid(
            "an AI actor cannot resolve its own review proposals".into(),
        ));
    }
    Ok(())
}

pub fn approve_mutation(
    id: String,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        require_non_ai(actor_kind)?;
        let item = load_pending_item(txn, &id)?;
        if item.status != "pending" {
            return Err(WriteError::Conflict(format!(
                "review item {id} is '{}', not 'pending'",
                item.status
            )));
        }
        let now = Utc::now().to_rfc3339();
        txn.execute(
            "UPDATE ai_review_queue SET status = 'approved', resolved_by = ?1, resolved_at = ?2 WHERE id = ?3",
            params![actor_kind.as_db_str(), now, id],
        )?;
        Ok(MutationOutcome {
            entity_type: "ai_review_queue",
            entity_id: id,
            action: ChangeAction::Update,
            diff_json: serde_json::json!({ "status": "approved" }),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

pub fn reject_mutation(
    id: String,
    reason: Option<String>,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        require_non_ai(actor_kind)?;
        let item = load_pending_item(txn, &id)?;
        if item.status != "pending" {
            return Err(WriteError::Conflict(format!(
                "review item {id} is '{}', not 'pending'",
                item.status
            )));
        }
        let now = Utc::now().to_rfc3339();
        txn.execute(
            "UPDATE ai_review_queue SET status = 'rejected', resolved_by = ?1, resolved_at = ?2 WHERE id = ?3",
            params![actor_kind.as_db_str(), now, id],
        )?;
        Ok(MutationOutcome {
            entity_type: "ai_review_queue",
            entity_id: id,
            action: ChangeAction::Update,
            diff_json: serde_json::json!({ "status": "rejected" }),
            reason,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

/// Checks the proposal's target still exists — *inside* the same transaction
/// as the apply itself, not as a pre-check beforehand, closing the TOCTOU gap
/// between approve and apply (e.g. the target node getting deleted in
/// between).
fn assert_target_still_valid(txn: &Transaction, item: &ReviewItem) -> Result<(), WriteError> {
    if item.proposed_action == "create" {
        return Ok(());
    }
    let entity_id = required_entity_id(item)?;
    let table = match item.entity_type.as_str() {
        "node" => "nodes",
        "hot_memory" => "hot_memory",
        "user_profile" => "user_profile",
        other => return Err(WriteError::Invalid(format!("unknown entity_type '{other}'"))),
    };
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1 AND deleted_at IS NULL)");
    let exists: bool = txn.query_row(&sql, params![entity_id], |r| r.get(0))?;
    if !exists {
        return Err(WriteError::Conflict(format!(
            "target {entity_id} in {table} no longer exists — it may have been deleted since this proposal was made"
        )));
    }
    Ok(())
}

/// Runs the actual mutation a proposal describes, dispatching on
/// (entity_type, proposed_action). Reuses the exact same mutation builders as
/// the direct-write routes — `apply_create_hot`/`apply_create_profile` skip
/// the "AI can't write sensitive facts directly" guard (approval *is* that
/// governance for this path); updates/deletes have no such guard to begin
/// with.
fn apply_proposed_mutation(txn: &Transaction, item: &ReviewItem) -> Result<MutationOutcome, WriteError> {
    match (item.entity_type.as_str(), item.proposed_action.as_str()) {
        ("hot_memory", "create") => {
            let input: HotMemoryInput = serde_json::from_str(&item.proposed_diff_json).map_err(json_err)?;
            crate::domain::memory::apply_create_hot(input)(txn)
        }
        ("hot_memory", "update") => {
            let entity_id = required_entity_id(item)?;
            let input: PatchHotMemoryInput = serde_json::from_str(&item.proposed_diff_json).map_err(json_err)?;
            crate::domain::memory::update_hot_mutation(entity_id, input, ActorKind::Ai)(txn)
        }
        ("hot_memory", "delete") => {
            crate::domain::memory::delete_hot_mutation(required_entity_id(item)?)(txn)
        }
        ("user_profile", "create") => {
            let input: UserProfileInput = serde_json::from_str(&item.proposed_diff_json).map_err(json_err)?;
            crate::domain::memory::apply_create_profile(input)(txn)
        }
        ("user_profile", "update") => {
            let entity_id = required_entity_id(item)?;
            let input: PatchUserProfileInput = serde_json::from_str(&item.proposed_diff_json).map_err(json_err)?;
            crate::domain::memory::update_profile_mutation(entity_id, input, ActorKind::Ai)(txn)
        }
        ("user_profile", "delete") => {
            crate::domain::memory::delete_profile_mutation(required_entity_id(item)?)(txn)
        }
        ("node", "create") => {
            let input: CreateNodeInput = serde_json::from_str(&item.proposed_diff_json).map_err(json_err)?;
            crate::domain::node::create_mutation(input, ActorKind::Ai)(txn)
        }
        ("node", "update") => {
            let entity_id = required_entity_id(item)?;
            let input: PatchNodeInput = serde_json::from_str(&item.proposed_diff_json).map_err(json_err)?;
            crate::domain::node::patch_mutation(entity_id, input, ActorKind::Ai)(txn)
        }
        ("node", "delete") => crate::domain::node::delete_mutation(required_entity_id(item)?)(txn),
        (entity_type, action) => Err(WriteError::Invalid(format!(
            "unsupported proposal: entity_type={entity_type} action={action}"
        ))),
    }
}

/// `POST /review/:id/apply`. Reuses `write_tx` exactly like any other
/// mutation, but — unlike every other mutation in the app — this closure
/// produces *two* changelog rows in one transaction: one for the underlying
/// entity write (inserted here, manually, so its id can be captured for
/// `applied_changelog_id`) and one for the `ai_review_queue` status change
/// itself (inserted automatically by `db::writer::run_job`, from this
/// function's own return value). Both are attributed to `actor` — the human
/// or system that clicked apply — while the underlying row's `created_by`
/// stays `ai`, preserving that the content was AI-authored.
pub fn apply_mutation(
    id: String,
    actor: Actor,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        require_non_ai(actor.kind)?;
        let item = load_pending_item(txn, &id)?;
        if item.status != "pending" && item.status != "approved" {
            return Err(WriteError::Conflict(format!(
                "review item {id} is '{}', cannot be applied",
                item.status
            )));
        }
        assert_target_still_valid(txn, &item)?;

        let entity_outcome = apply_proposed_mutation(txn, &item)?;
        let applied_changelog_id = crate::domain::changelog::insert_changelog_row(txn, &actor, &entity_outcome)?;
        if let Some(snapshot) = &entity_outcome.node_snapshot {
            crate::domain::changelog::insert_node_revision_row(
                txn,
                &actor,
                &entity_outcome.entity_id,
                snapshot,
                &applied_changelog_id,
            )?;
        }

        let now = Utc::now().to_rfc3339();
        txn.execute(
            "UPDATE ai_review_queue SET status = 'applied', resolved_by = ?1, resolved_at = ?2, applied_changelog_id = ?3 WHERE id = ?4",
            params![actor.kind.as_db_str(), now, applied_changelog_id, id],
        )?;

        Ok(MutationOutcome {
            entity_type: "ai_review_queue",
            entity_id: id,
            action: ChangeAction::Update,
            diff_json: serde_json::json!({
                "status": "applied",
                "applied_entity_type": entity_outcome.entity_type,
                "applied_entity_id": entity_outcome.entity_id,
            }),
            reason: Some("review item applied".to_string()),
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}
