use chrono::Utc;
use rusqlite::{params, Connection, Row, Transaction};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::writer::{ActorKind, ChangeAction, MutationOutcome, WriteError};

// ============================================================
// Shared validation
// ============================================================

const HOT_CATEGORIES: [&str; 9] = [
    "hard_constraint",
    "preference",
    "identity",
    "environment",
    "routing_rule",
    "project_pointer",
    "tool_quirk",
    "domain_model",
    "temporary_context",
];
const SENSITIVITY: [&str; 3] = ["normal", "sensitive", "secret"];
const SOURCE_TYPE: [&str; 5] = [
    "explicit_user_statement",
    "observed_behavior",
    "imported_note",
    "system_config",
    "ai_inference",
];
const CONFIDENCE: [&str; 3] = ["high", "medium", "low"];

fn validate_enum(value: &str, allowed: &[&str], field: &str) -> Result<(), WriteError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(WriteError::Invalid(format!("unknown {field} '{value}'")))
    }
}

/// Core principle #6: inferred/sensitive facts are lower-trust and must go
/// through the review queue's propose -> approve -> apply lifecycle, not be
/// written directly. A human writing directly is always fine (they *are* the
/// approval); an AI agent writing directly is only fine for safe, explicit,
/// normal-sensitivity entries — anything `ai_inference`-sourced or above
/// `normal` sensitivity has to be proposed instead. This is what makes
/// "governed, not raw" a real constraint rather than a comment.
fn guard_direct_write(actor_kind: ActorKind, sensitivity: &str, source_type: &str) -> Result<(), WriteError> {
    if actor_kind == ActorKind::Ai && (sensitivity != "normal" || source_type == "ai_inference") {
        return Err(WriteError::Invalid(
            "AI-inferred or sensitive memory entries must go through the review queue (POST /review), not written directly".into(),
        ));
    }
    Ok(())
}

// ============================================================
// hot_memory
// ============================================================

#[derive(Debug, Deserialize)]
pub struct HotMemoryInput {
    pub key: String,
    pub value: String,
    pub category: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default = "default_priority")]
    pub priority: i64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default = "default_sensitivity")]
    pub sensitivity: String,
    pub source_node_id: Option<String>,
    #[serde(default = "default_source_type")]
    pub source_type: String,
    #[serde(default = "default_confidence")]
    pub confidence: String,
    pub expires_at: Option<String>,
}

fn default_scope() -> String {
    "general".to_string()
}
fn default_priority() -> i64 {
    50
}
fn default_sensitivity() -> String {
    "normal".to_string()
}
fn default_source_type() -> String {
    "explicit_user_statement".to_string()
}
fn default_confidence() -> String {
    "high".to_string()
}

#[derive(Debug, Serialize)]
pub struct HotMemoryDto {
    pub id: String,
    pub key: String,
    pub value: String,
    pub category: String,
    pub status: String,
    pub scope: String,
    pub priority: i64,
    pub pinned: bool,
    pub sensitivity: String,
    pub source_node_id: Option<String>,
    pub source_type: String,
    pub confidence: String,
    pub char_count: i64,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
    pub last_included_at: Option<String>,
    pub include_count: i64,
}

fn hot_memory_row(row: &Row) -> rusqlite::Result<HotMemoryDto> {
    Ok(HotMemoryDto {
        id: row.get("id")?,
        key: row.get("key")?,
        value: row.get("value")?,
        category: row.get("category")?,
        status: row.get("status")?,
        scope: row.get("scope")?,
        priority: row.get("priority")?,
        pinned: row.get::<_, i64>("pinned")? != 0,
        sensitivity: row.get("sensitivity")?,
        source_node_id: row.get("source_node_id")?,
        source_type: row.get("source_type")?,
        confidence: row.get("confidence")?,
        char_count: row.get("char_count")?,
        created_by: row.get("created_by")?,
        updated_by: row.get("updated_by")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        expires_at: row.get("expires_at")?,
        last_included_at: row.get("last_included_at")?,
        include_count: row.get("include_count")?,
    })
}

fn validate_hot_input(input: &HotMemoryInput) -> Result<(), WriteError> {
    validate_enum(&input.category, &HOT_CATEGORIES, "category")?;
    validate_enum(&input.sensitivity, &SENSITIVITY, "sensitivity")?;
    validate_enum(&input.source_type, &SOURCE_TYPE, "source_type")?;
    validate_enum(&input.confidence, &CONFIDENCE, "confidence")?;
    if input.key.trim().is_empty() || input.value.trim().is_empty() {
        return Err(WriteError::Invalid("key and value must not be empty".into()));
    }
    Ok(())
}

/// The raw insert, shared by the direct-write path (`create_hot_mutation`,
/// gated by `guard_direct_write`) and the review-queue apply path
/// (`review::apply_proposed_mutation`, which *is* the governed route around
/// that guard — applying an approved proposal is not a "direct" write).
/// `author_kind` sets the row's own `created_by`/`updated_by` (who authored
/// the fact); the caller's `write_tx` actor separately attributes the
/// changelog row (who committed this write) — those two are deliberately
/// allowed to differ when a human applies an AI-authored proposal.
fn insert_hot_memory_row(txn: &Transaction, input: &HotMemoryInput, author_kind: ActorKind) -> Result<String, WriteError> {
    validate_hot_input(input)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let by = author_kind.as_db_str();
    let char_count = input.value.chars().count() as i64;

    txn.execute(
        "INSERT INTO hot_memory (
            id, key, value, category, status, scope, priority, pinned, sensitivity,
            source_node_id, source_type, confidence, char_count, created_by, updated_by,
            created_at, updated_at, expires_at
        ) VALUES (?1,?2,?3,?4,'approved',?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
        params![
            id,
            input.key,
            input.value,
            input.category,
            input.scope,
            input.priority,
            input.pinned as i64,
            input.sensitivity,
            input.source_node_id,
            input.source_type,
            input.confidence,
            char_count,
            by,
            by,
            now,
            now,
            input.expires_at,
        ],
    )?;
    Ok(id)
}

pub fn create_hot_mutation(
    input: HotMemoryInput,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        validate_hot_input(&input)?;
        guard_direct_write(actor_kind, &input.sensitivity, &input.source_type)?;
        let key = input.key.clone();
        let category = input.category.clone();
        let id = insert_hot_memory_row(txn, &input, actor_kind)?;

        Ok(MutationOutcome {
            entity_type: "hot_memory",
            entity_id: id,
            action: ChangeAction::Create,
            diff_json: serde_json::json!({ "key": key, "category": category }),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

/// Entry point for the review queue's apply step — skips `guard_direct_write`
/// since going through approval *is* the governance, and always authors the
/// row as `ai` (the schema's `ai_review_queue.actor` CHECK constraint means
/// every proposal originated from AI).
pub fn apply_create_hot(
    input: HotMemoryInput,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let key = input.key.clone();
        let category = input.category.clone();
        let id = insert_hot_memory_row(txn, &input, ActorKind::Ai)?;
        Ok(MutationOutcome {
            entity_type: "hot_memory",
            entity_id: id,
            action: ChangeAction::Create,
            diff_json: serde_json::json!({ "key": key, "category": category, "via_review_queue": true }),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct PatchHotMemoryInput {
    pub value: Option<String>,
    pub priority: Option<i64>,
    pub pinned: Option<bool>,
    pub status: Option<String>,
    pub expires_at: Option<String>,
}

pub fn update_hot_mutation(
    id: String,
    input: PatchHotMemoryInput,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let exists: bool = txn.query_row(
            "SELECT EXISTS(SELECT 1 FROM hot_memory WHERE id = ?1 AND deleted_at IS NULL)",
            params![id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(WriteError::NotFound(format!("hot_memory {id} not found")));
        }
        if let Some(status) = &input.status {
            validate_enum(status, &["candidate", "approved", "rejected", "deprecated"], "status")?;
        }

        let now = Utc::now().to_rfc3339();
        let by = actor_kind.as_db_str();
        let char_count = input.value.as_ref().map(|v| v.chars().count() as i64);

        txn.execute(
            "UPDATE hot_memory SET
                value = COALESCE(?1, value),
                char_count = COALESCE(?2, char_count),
                priority = COALESCE(?3, priority),
                pinned = COALESCE(?4, pinned),
                status = COALESCE(?5, status),
                expires_at = COALESCE(?6, expires_at),
                updated_by = ?7, updated_at = ?8
             WHERE id = ?9",
            params![
                input.value,
                char_count,
                input.priority,
                input.pinned.map(|b| b as i64),
                input.status,
                input.expires_at,
                by,
                now,
                id
            ],
        )?;

        Ok(MutationOutcome {
            entity_type: "hot_memory",
            entity_id: id,
            action: ChangeAction::Update,
            diff_json: serde_json::json!({ "fields_changed": true }),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

pub fn delete_hot_mutation(
    id: String,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let now = Utc::now().to_rfc3339();
        let changed = txn.execute(
            "UPDATE hot_memory SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )?;
        if changed == 0 {
            return Err(WriteError::NotFound(format!("hot_memory {id} not found")));
        }
        Ok(MutationOutcome {
            entity_type: "hot_memory",
            entity_id: id,
            action: ChangeAction::Delete,
            diff_json: serde_json::json!({}),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

pub fn list_hot_memory(conn: &Connection) -> rusqlite::Result<Vec<HotMemoryDto>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM hot_memory WHERE deleted_at IS NULL ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], hot_memory_row)?.collect();
    rows
}

// ============================================================
// user_profile
// ============================================================

#[derive(Debug, Deserialize)]
pub struct UserProfileInput {
    pub key: String,
    pub value: String,
    pub source_quote: Option<String>,
    pub category: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default = "default_priority")]
    pub priority: i64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default = "default_sensitivity")]
    pub sensitivity: String,
    pub source_node_id: Option<String>,
    #[serde(default = "default_source_type")]
    pub source_type: String,
    #[serde(default = "default_confidence")]
    pub confidence: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UserProfileDto {
    pub id: String,
    pub key: String,
    pub value: String,
    pub source_quote: Option<String>,
    pub category: String,
    pub status: String,
    pub scope: String,
    pub priority: i64,
    pub pinned: bool,
    pub sensitivity: String,
    pub source_node_id: Option<String>,
    pub source_type: String,
    pub confidence: String,
    pub char_count: i64,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
    pub last_included_at: Option<String>,
    pub include_count: i64,
}

fn user_profile_row(row: &Row) -> rusqlite::Result<UserProfileDto> {
    Ok(UserProfileDto {
        id: row.get("id")?,
        key: row.get("key")?,
        value: row.get("value")?,
        source_quote: row.get("source_quote")?,
        category: row.get("category")?,
        status: row.get("status")?,
        scope: row.get("scope")?,
        priority: row.get("priority")?,
        pinned: row.get::<_, i64>("pinned")? != 0,
        sensitivity: row.get("sensitivity")?,
        source_node_id: row.get("source_node_id")?,
        source_type: row.get("source_type")?,
        confidence: row.get("confidence")?,
        char_count: row.get("char_count")?,
        created_by: row.get("created_by")?,
        updated_by: row.get("updated_by")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        expires_at: row.get("expires_at")?,
        last_included_at: row.get("last_included_at")?,
        include_count: row.get("include_count")?,
    })
}

fn validate_profile_input(input: &UserProfileInput) -> Result<(), WriteError> {
    // user_profile.category has no CHECK constraint in the schema (free text),
    // unlike hot_memory — intentional, not an oversight.
    validate_enum(&input.sensitivity, &SENSITIVITY, "sensitivity")?;
    validate_enum(&input.source_type, &SOURCE_TYPE, "source_type")?;
    validate_enum(&input.confidence, &CONFIDENCE, "confidence")?;
    if input.key.trim().is_empty() || input.value.trim().is_empty() {
        return Err(WriteError::Invalid("key and value must not be empty".into()));
    }
    Ok(())
}

/// See `insert_hot_memory_row` for why this is split out from
/// `create_profile_mutation` — same reasoning, mirrored for user_profile.
fn insert_user_profile_row(txn: &Transaction, input: &UserProfileInput, author_kind: ActorKind) -> Result<String, WriteError> {
    validate_profile_input(input)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let by = author_kind.as_db_str();
    let char_count = input.value.chars().count() as i64;

    txn.execute(
        "INSERT INTO user_profile (
            id, key, value, source_quote, category, status, scope, priority, pinned,
            sensitivity, source_node_id, source_type, confidence, char_count,
            created_by, updated_by, created_at, updated_at, expires_at
        ) VALUES (?1,?2,?3,?4,?5,'approved',?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
        params![
            id,
            input.key,
            input.value,
            input.source_quote,
            input.category,
            input.scope,
            input.priority,
            input.pinned as i64,
            input.sensitivity,
            input.source_node_id,
            input.source_type,
            input.confidence,
            char_count,
            by,
            by,
            now,
            now,
            input.expires_at,
        ],
    )?;
    Ok(id)
}

pub fn create_profile_mutation(
    input: UserProfileInput,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        validate_profile_input(&input)?;
        guard_direct_write(actor_kind, &input.sensitivity, &input.source_type)?;
        let key = input.key.clone();
        let category = input.category.clone();
        let id = insert_user_profile_row(txn, &input, actor_kind)?;

        Ok(MutationOutcome {
            entity_type: "user_profile",
            entity_id: id,
            action: ChangeAction::Create,
            diff_json: serde_json::json!({ "key": key, "category": category }),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

/// Review-queue apply entry point — see `apply_create_hot`.
pub fn apply_create_profile(
    input: UserProfileInput,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let key = input.key.clone();
        let category = input.category.clone();
        let id = insert_user_profile_row(txn, &input, ActorKind::Ai)?;
        Ok(MutationOutcome {
            entity_type: "user_profile",
            entity_id: id,
            action: ChangeAction::Create,
            diff_json: serde_json::json!({ "key": key, "category": category, "via_review_queue": true }),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct PatchUserProfileInput {
    pub value: Option<String>,
    pub priority: Option<i64>,
    pub pinned: Option<bool>,
    pub status: Option<String>,
    pub expires_at: Option<String>,
}

pub fn update_profile_mutation(
    id: String,
    input: PatchUserProfileInput,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let exists: bool = txn.query_row(
            "SELECT EXISTS(SELECT 1 FROM user_profile WHERE id = ?1 AND deleted_at IS NULL)",
            params![id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(WriteError::NotFound(format!("user_profile {id} not found")));
        }
        if let Some(status) = &input.status {
            validate_enum(status, &["candidate", "approved", "rejected", "deprecated"], "status")?;
        }

        let now = Utc::now().to_rfc3339();
        let by = actor_kind.as_db_str();
        let char_count = input.value.as_ref().map(|v| v.chars().count() as i64);

        txn.execute(
            "UPDATE user_profile SET
                value = COALESCE(?1, value),
                char_count = COALESCE(?2, char_count),
                priority = COALESCE(?3, priority),
                pinned = COALESCE(?4, pinned),
                status = COALESCE(?5, status),
                expires_at = COALESCE(?6, expires_at),
                updated_by = ?7, updated_at = ?8
             WHERE id = ?9",
            params![
                input.value,
                char_count,
                input.priority,
                input.pinned.map(|b| b as i64),
                input.status,
                input.expires_at,
                by,
                now,
                id
            ],
        )?;

        Ok(MutationOutcome {
            entity_type: "user_profile",
            entity_id: id,
            action: ChangeAction::Update,
            diff_json: serde_json::json!({ "fields_changed": true }),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

pub fn delete_profile_mutation(
    id: String,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let now = Utc::now().to_rfc3339();
        let changed = txn.execute(
            "UPDATE user_profile SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )?;
        if changed == 0 {
            return Err(WriteError::NotFound(format!("user_profile {id} not found")));
        }
        Ok(MutationOutcome {
            entity_type: "user_profile",
            entity_id: id,
            action: ChangeAction::Delete,
            diff_json: serde_json::json!({}),
            reason: None,
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}

pub fn list_user_profile(conn: &Connection) -> rusqlite::Result<Vec<UserProfileDto>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM user_profile WHERE deleted_at IS NULL ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], user_profile_row)?.collect();
    rows
}

// ============================================================
// Compiler: selection -> deterministic sort -> budget -> conflicts
//
// Pure functions over already-fetched candidates wherever possible. The only
// place that reads hot_memory/user_profile rows directly is
// gather_candidates — no other file should query these tables directly; the
// compiled MemoryContextDto is the only sanctioned way anything downstream
// sees this data, matching "memory is governed, not raw."
// ============================================================

pub const COMPILER_VERSION: &str = "1.0.0";
// "High-priority" isn't pinned to a number in the roadmap — this is an
// engineering default for conflict detection, tunable later without
// changing the conflict-detection *behavior* (same category+key,
// disagreeing values).
const HIGH_PRIORITY_THRESHOLD: i64 = 70;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryTable {
    Hot,
    Profile,
}

impl MemoryTable {
    fn table_name(&self) -> &'static str {
        match self {
            MemoryTable::Hot => "hot_memory",
            MemoryTable::Profile => "user_profile",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryCandidate {
    pub id: String,
    pub table: MemoryTable,
    pub key: String,
    pub value: String,
    pub category: String,
    pub priority: i64,
    pub pinned: bool,
    pub source_type: String,
    pub confidence: String,
    pub char_count: i64,
    pub updated_at: String,
    pub source_node_id: Option<String>,
    pub last_included_at: Option<String>,
}

/// `pub(crate)` rather than private: `domain::maintenance`'s conflicting_memory
/// lint check reuses this to build the same candidate set `detect_conflicts`
/// expects, independent of any single `/memory/context` compile call.
pub(crate) fn gather_candidates(conn: &Connection, table: MemoryTable) -> rusqlite::Result<Vec<MemoryCandidate>> {
    let now = Utc::now().to_rfc3339();
    let sql = format!(
        "SELECT id, key, value, category, priority, pinned, source_type, confidence,
                char_count, updated_at, source_node_id, last_included_at
         FROM {} WHERE status = 'approved' AND deleted_at IS NULL
           AND (expires_at IS NULL OR expires_at > ?1)",
        table.table_name()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![now], |row| {
        Ok(MemoryCandidate {
            id: row.get(0)?,
            table,
            key: row.get(1)?,
            value: row.get(2)?,
            category: row.get(3)?,
            priority: row.get(4)?,
            pinned: row.get::<_, i64>(5)? != 0,
            source_type: row.get(6)?,
            confidence: row.get(7)?,
            char_count: row.get(8)?,
            updated_at: row.get(9)?,
            source_node_id: row.get(10)?,
            last_included_at: row.get(11)?,
        })
    })?;
    rows.collect()
}

fn category_safety_rank(c: &str) -> u8 {
    match c {
        "hard_constraint" => 0,
        "identity" => 1,
        "preference" => 2,
        "environment" => 3,
        "routing_rule" => 4,
        "project_pointer" => 5,
        "tool_quirk" => 6,
        "domain_model" => 7,
        "temporary_context" => 8,
        _ => 9,
    }
}
fn source_type_rank(s: &str) -> u8 {
    match s {
        "explicit_user_statement" => 0,
        "observed_behavior" => 1,
        "imported_note" | "system_config" => 2,
        "ai_inference" => 3,
        _ => 4,
    }
}
fn confidence_rank(c: &str) -> u8 {
    match c {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

/// The compiler's deterministic sort — exactly the 7-key order specified by
/// the roadmap, in this precise order, so output never reshuffles between
/// sessions on its own: (1) pinned desc, (2) category safety rank, (3)
/// priority desc, (4) source_type rank, (5) confidence rank, (6) updated_at
/// desc, (7) id asc as the final, always-decisive tie-breaker.
pub fn sort_deterministic(mut candidates: Vec<MemoryCandidate>) -> Vec<MemoryCandidate> {
    candidates.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then(category_safety_rank(&a.category).cmp(&category_safety_rank(&b.category)))
            .then(b.priority.cmp(&a.priority))
            .then(source_type_rank(&a.source_type).cmp(&source_type_rank(&b.source_type)))
            .then(confidence_rank(&a.confidence).cmp(&confidence_rank(&b.confidence)))
            .then(b.updated_at.cmp(&a.updated_at))
            .then(a.id.cmp(&b.id))
    });
    candidates
}

/// Greedily includes candidates (already in deterministic order) until the
/// next one would exceed budget. Returns indices into `ordered` rather than
/// references, to sidestep borrow-splitting for the included/excluded split.
pub fn apply_budget(ordered: &[MemoryCandidate], budget_chars: i64) -> (Vec<usize>, Vec<usize>) {
    let mut used = 0i64;
    let mut included = Vec::new();
    let mut excluded = Vec::new();
    for (i, c) in ordered.iter().enumerate() {
        if used + c.char_count <= budget_chars {
            included.push(i);
            used += c.char_count;
        } else {
            excluded.push(i);
        }
    }
    (included, excluded)
}

#[derive(Debug, Serialize)]
pub struct ConflictWarning {
    #[serde(rename = "type")]
    pub warning_type: &'static str,
    pub category: String,
    pub key: String,
    pub entry_ids: Vec<String>,
}

/// v1 scope: flags when multiple *approved* entries share a category+key and
/// disagree in value, with at least one of them high-priority. Runs over all
/// candidates (pre-budget) so a conflict is surfaced even if one side got
/// excluded by budget. Full conflict-resolution (a `conflicts_with` table) is
/// deliberately deferred — this is a warning, not enforcement.
pub fn detect_conflicts(candidates: &[MemoryCandidate]) -> Vec<ConflictWarning> {
    use std::collections::{HashMap, HashSet};
    let mut groups: HashMap<(String, String), Vec<&MemoryCandidate>> = HashMap::new();
    for c in candidates {
        groups.entry((c.category.clone(), c.key.clone())).or_default().push(c);
    }
    let mut warnings: Vec<ConflictWarning> = groups
        .into_iter()
        .filter_map(|((category, key), members)| {
            if members.len() < 2 {
                return None;
            }
            let distinct_values: HashSet<&str> = members.iter().map(|m| m.value.as_str()).collect();
            let has_high_priority = members.iter().any(|m| m.priority >= HIGH_PRIORITY_THRESHOLD);
            if distinct_values.len() > 1 && has_high_priority {
                Some(ConflictWarning {
                    warning_type: "conflict",
                    category,
                    key,
                    entry_ids: members.iter().map(|m| m.id.clone()).collect(),
                })
            } else {
                None
            }
        })
        .collect();
    warnings.sort_by(|a, b| (a.category.as_str(), a.key.as_str()).cmp(&(b.category.as_str(), b.key.as_str())));
    warnings
}

#[derive(Debug, Serialize)]
pub struct IncludedEntryDto {
    pub id: String,
    pub table: &'static str,
    pub key: String,
    pub source_node_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExcludedEntryDto {
    pub id: String,
    pub table: &'static str,
    pub key: String,
    pub reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct BudgetUsageDto {
    pub hot_used: i64,
    pub hot_budget: i64,
    pub profile_used: i64,
    pub profile_budget: i64,
}

#[derive(Debug, Serialize)]
pub struct MemoryContextDto {
    pub compiled_hot_memory: String,
    pub compiled_user_profile: String,
    pub included: Vec<IncludedEntryDto>,
    pub excluded: Vec<ExcludedEntryDto>,
    pub budget_usage: BudgetUsageDto,
    pub warnings: Vec<ConflictWarning>,
    pub compiler_version: String,
}

fn render_compiled_text(ordered: &[MemoryCandidate], included_idx: &[usize]) -> String {
    included_idx
        .iter()
        .map(|&i| format!("- [{}] {}", ordered[i].key, ordered[i].value))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `compile_context`'s full result: the response DTO plus the raw included
/// candidate lists, which the route handler needs separately to decide
/// whether to run the overflow protocol (see `run_overflow_protocol`) — the
/// DTO alone doesn't carry enough (e.g. `pinned`, `last_included_at`) to
/// select eviction candidates from.
pub struct CompiledContext {
    pub dto: MemoryContextDto,
    pub hot_included: Vec<MemoryCandidate>,
    pub profile_included: Vec<MemoryCandidate>,
}

/// Gathers, sorts, budgets, and flags conflicts for both tables — the full
/// `GET /memory/context` response. Does not run the overflow/eviction
/// protocol itself (that mutates the DB and needs the writer, so it's
/// orchestrated by the route handler, which decides whether to recompile
/// afterward — see `routes/memory.rs`).
pub fn compile_context(conn: &Connection, budget_hot: i64, budget_profile: i64) -> rusqlite::Result<CompiledContext> {
    let hot_candidates = sort_deterministic(gather_candidates(conn, MemoryTable::Hot)?);
    let profile_candidates = sort_deterministic(gather_candidates(conn, MemoryTable::Profile)?);

    let (hot_included_idx, hot_excluded_idx) = apply_budget(&hot_candidates, budget_hot);
    let (profile_included_idx, profile_excluded_idx) = apply_budget(&profile_candidates, budget_profile);

    let hot_used: i64 = hot_included_idx.iter().map(|&i| hot_candidates[i].char_count).sum();
    let profile_used: i64 = profile_included_idx.iter().map(|&i| profile_candidates[i].char_count).sum();

    let mut warnings = detect_conflicts(&hot_candidates);
    warnings.extend(detect_conflicts(&profile_candidates));

    let mut included = Vec::new();
    for &i in &hot_included_idx {
        included.push(IncludedEntryDto {
            id: hot_candidates[i].id.clone(),
            table: hot_candidates[i].table.table_name(),
            key: hot_candidates[i].key.clone(),
            source_node_id: hot_candidates[i].source_node_id.clone(),
        });
    }
    for &i in &profile_included_idx {
        included.push(IncludedEntryDto {
            id: profile_candidates[i].id.clone(),
            table: profile_candidates[i].table.table_name(),
            key: profile_candidates[i].key.clone(),
            source_node_id: profile_candidates[i].source_node_id.clone(),
        });
    }
    let mut excluded = Vec::new();
    for &i in &hot_excluded_idx {
        excluded.push(ExcludedEntryDto {
            id: hot_candidates[i].id.clone(),
            table: hot_candidates[i].table.table_name(),
            key: hot_candidates[i].key.clone(),
            reason: "budget",
        });
    }
    for &i in &profile_excluded_idx {
        excluded.push(ExcludedEntryDto {
            id: profile_candidates[i].id.clone(),
            table: profile_candidates[i].table.table_name(),
            key: profile_candidates[i].key.clone(),
            reason: "budget",
        });
    }

    let dto = MemoryContextDto {
        compiled_hot_memory: render_compiled_text(&hot_candidates, &hot_included_idx),
        compiled_user_profile: render_compiled_text(&profile_candidates, &profile_included_idx),
        included,
        excluded,
        budget_usage: BudgetUsageDto {
            hot_used,
            hot_budget: budget_hot,
            profile_used,
            profile_budget: budget_profile,
        },
        warnings,
        compiler_version: COMPILER_VERSION.to_string(),
    };

    let hot_included = hot_included_idx.iter().map(|&i| hot_candidates[i].clone()).collect();
    let profile_included = profile_included_idx.iter().map(|&i| profile_candidates[i].clone()).collect();

    Ok(CompiledContext {
        dto,
        hot_included,
        profile_included,
    })
}

// ============================================================
// Overflow / eviction protocol
//
// Fires when a table's *included* (post-budget) usage exceeds ~80% of its
// budget — a proactive compaction trigger, not a reaction to something
// already being excluded. Moves an entry's full text into a `nodes` row (if
// it doesn't already have one) and replaces its `value` with a short
// pointer, persisted through the normal write_tx path so each eviction gets
// its own changelog row with `compiler_version` set. Pinned and
// `hard_constraint` entries are never eviction candidates.
// ============================================================

fn pointer_text(key: &str) -> String {
    format!("See [[{key}]] for full detail.")
}

fn evict_to_pointer_mutation(
    table: MemoryTable,
    candidate: MemoryCandidate,
    actor_kind: ActorKind,
) -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let source_node_id = match &candidate.source_node_id {
            Some(id) => id.clone(),
            None => {
                let id = Uuid::new_v4().to_string();
                let now = Utc::now().to_rfc3339();
                let title = candidate.key.clone();
                let title_normalized = crate::domain::normalize::normalize_title(&title);
                let base_slug = crate::domain::normalize::slugify(&title);
                let slug = crate::domain::node::unique_slug(txn, &base_slug)?;
                let by = actor_kind.as_db_str();
                txn.execute(
                    "INSERT INTO nodes (
                        id, title, title_normalized, slug, content, node_type,
                        export_policy, created_by, updated_by, created_at, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, 'page', 'export', ?6, ?6, ?7, ?7)",
                    params![id, title, title_normalized, slug, candidate.value, by, now],
                )?;
                crate::domain::fts::index_new_node(txn, &id, &title, &candidate.value)?;
                id
            }
        };

        let pointer = pointer_text(&candidate.key);
        let new_char_count = pointer.chars().count() as i64;
        let now = Utc::now().to_rfc3339();

        let sql = format!(
            "UPDATE {} SET value = ?1, char_count = ?2, source_node_id = ?3, updated_at = ?4 WHERE id = ?5",
            table.table_name()
        );
        txn.execute(&sql, params![pointer, new_char_count, source_node_id, now, candidate.id])?;

        Ok(MutationOutcome {
            entity_type: table.table_name(),
            entity_id: candidate.id.clone(),
            action: ChangeAction::Update,
            diff_json: serde_json::json!({ "evicted_to_pointer": true, "source_node_id": source_node_id }),
            reason: Some("memory compiler overflow eviction".to_string()),
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: Some(COMPILER_VERSION),
        })
    }
}

/// Runs the eviction loop for one table's included candidates. Returns the
/// number of entries evicted (0 if usage was already under the 80%
/// threshold, or nothing eligible remained).
pub async fn run_overflow_protocol(
    writer: &crate::db::writer::WriterHandle,
    actor: crate::db::writer::Actor,
    table: MemoryTable,
    included: &[MemoryCandidate],
    budget_chars: i64,
) -> Result<usize, WriteError> {
    let mut used: i64 = included.iter().map(|c| c.char_count).sum();
    if (used as f64) <= 0.8 * budget_chars as f64 {
        return Ok(0);
    }

    let mut candidates: Vec<&MemoryCandidate> = included
        .iter()
        .filter(|c| !c.pinned && c.category != "hard_constraint")
        .collect();
    candidates.sort_by(|a, b| {
        b.char_count
            .cmp(&a.char_count) // longest first
            .then_with(|| match (&a.last_included_at, &b.last_included_at) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less, // never-included sorts first (least recently included)
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(x), Some(y)) => x.cmp(y),
            })
            .then(b.source_node_id.is_some().cmp(&a.source_node_id.is_some())) // prefer ones that already have a source node
    });

    let mut evicted = 0usize;
    for candidate in candidates {
        if (used as f64) <= 0.8 * budget_chars as f64 {
            break;
        }
        let new_len = pointer_text(&candidate.key).chars().count() as i64;
        writer
            .write_tx(
                actor.clone(),
                evict_to_pointer_mutation(table, candidate.clone(), actor.kind),
            )
            .await?;
        used = used - candidate.char_count + new_len;
        evicted += 1;
    }
    Ok(evicted)
}
