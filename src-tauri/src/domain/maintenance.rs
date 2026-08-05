use chrono::{Duration, Utc};
use rusqlite::{params, Row, Transaction};
use serde::Serialize;
use uuid::Uuid;

use crate::db::writer::{ChangeAction, MutationOutcome, WriteError};
use crate::domain::memory::{self, MemoryTable};

const FINDING_TYPES: [&str; 4] = ["orphan_node", "unresolved_link", "stale_memory", "conflicting_memory"];
// Engineering default (not specified by the roadmap) for how long an
// unpinned memory entry can go uncompiled before it's flagged as stale.
const STALE_MEMORY_DAYS: i64 = 30;

#[derive(Debug, Serialize)]
pub struct FindingDto {
    pub id: String,
    pub finding_type: String,
    pub severity: String,
    pub entity_id: Option<String>,
    pub description: String,
    pub suggested_fix: Option<String>,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

fn finding_row(row: &Row) -> rusqlite::Result<FindingDto> {
    Ok(FindingDto {
        id: row.get("id")?,
        finding_type: row.get("finding_type")?,
        severity: row.get("severity")?,
        entity_id: row.get("entity_id")?,
        description: row.get("description")?,
        suggested_fix: row.get("suggested_fix")?,
        status: row.get("status")?,
        created_at: row.get("created_at")?,
        resolved_at: row.get("resolved_at")?,
    })
}

pub fn list_findings(
    conn: &rusqlite::Connection,
    status: Option<&str>,
    finding_type: Option<&str>,
) -> rusqlite::Result<Vec<FindingDto>> {
    let mut sql = "SELECT * FROM maintenance_findings WHERE 1=1".to_string();
    let mut params_vec: Vec<String> = Vec::new();
    if let Some(s) = status {
        sql.push_str(" AND status = ?");
        params_vec.push(s.to_string());
    }
    if let Some(ft) = finding_type {
        sql.push_str(" AND finding_type = ?");
        params_vec.push(ft.to_string());
    }
    sql.push_str(" ORDER BY created_at DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params_vec.iter()), finding_row)?
        .collect();
    rows
}

struct NewFinding {
    finding_type: &'static str,
    severity: &'static str,
    entity_id: Option<String>,
    description: String,
    suggested_fix: Option<String>,
}

fn find_orphan_nodes(txn: &Transaction) -> rusqlite::Result<Vec<NewFinding>> {
    let mut stmt = txn.prepare(
        "SELECT id, title FROM nodes n
         WHERE deleted_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM links l WHERE l.source_node_id = n.id AND l.status = 'resolved')
           AND NOT EXISTS (SELECT 1 FROM links l WHERE l.target_node_id = n.id AND l.status = 'resolved')",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        Ok(NewFinding {
            finding_type: "orphan_node",
            severity: "info",
            entity_id: Some(id),
            description: format!("\"{title}\" has no resolved incoming or outgoing links"),
            suggested_fix: Some("Link it from a related note, or confirm it's meant to stand alone.".into()),
        })
    })?;
    rows.collect()
}

fn find_unresolved_links(txn: &Transaction) -> rusqlite::Result<Vec<NewFinding>> {
    let mut stmt = txn.prepare(
        "SELECT l.id, l.target_raw, n.title FROM links l
         JOIN nodes n ON n.id = l.source_node_id
         WHERE l.status = 'unresolved' AND n.deleted_at IS NULL",
    )?;
    let rows = stmt.query_map([], |row| {
        let link_id: String = row.get(0)?;
        let target_raw: String = row.get(1)?;
        let source_title: String = row.get(2)?;
        Ok(NewFinding {
            finding_type: "unresolved_link",
            severity: "warning",
            entity_id: Some(link_id),
            description: format!("\"{source_title}\" links to [[{target_raw}]], which doesn't resolve to any note or alias"),
            suggested_fix: Some("Create the target note, add an alias for it, or fix the link text.".into()),
        })
    })?;
    rows.collect()
}

fn find_stale_memory(txn: &Transaction) -> rusqlite::Result<Vec<NewFinding>> {
    let cutoff = (Utc::now() - Duration::days(STALE_MEMORY_DAYS)).to_rfc3339();
    let mut out = Vec::new();
    for (table, label) in [("hot_memory", "hot memory"), ("user_profile", "user profile")] {
        let sql = format!(
            "SELECT id, key FROM {table}
             WHERE deleted_at IS NULL AND status = 'approved' AND pinned = 0
               AND COALESCE(last_included_at, created_at) < ?1"
        );
        let mut stmt = txn.prepare(&sql)?;
        let rows = stmt.query_map(params![cutoff], |row| {
            let id: String = row.get(0)?;
            let key: String = row.get(1)?;
            Ok(NewFinding {
                finding_type: "stale_memory",
                severity: "info",
                entity_id: Some(id),
                description: format!(
                    "{label} entry \"{key}\" hasn't been included in a compiled context in over {STALE_MEMORY_DAYS} days"
                ),
                suggested_fix: Some("Pin it if still relevant, or delete it if it's no longer useful.".into()),
            })
        })?;
        out.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
    }
    Ok(out)
}

fn find_conflicting_memory(txn: &Transaction) -> Result<Vec<NewFinding>, WriteError> {
    let mut out = Vec::new();
    for table in [MemoryTable::Hot, MemoryTable::Profile] {
        let candidates = memory::gather_candidates(txn, table)?;
        for warning in memory::detect_conflicts(&candidates) {
            out.push(NewFinding {
                finding_type: "conflicting_memory",
                severity: "warning",
                entity_id: None,
                description: format!(
                    "Multiple approved entries disagree for category '{}', key '{}': {}",
                    warning.category,
                    warning.key,
                    warning.entry_ids.join(", ")
                ),
                suggested_fix: Some("Review the conflicting entries and deprecate/update the stale one.".into()),
            });
        }
    }
    Ok(out)
}

/// Runs the full lint pass inside one write transaction: clears prior `open`
/// findings for the types being re-checked (so re-running doesn't accumulate
/// duplicates and stops flagging issues that got fixed), then inserts
/// whatever's found now. One changelog row summarizes the whole run, matching
/// how every other mutation in the app is recorded.
pub fn run_lint_pass() -> impl FnOnce(&Transaction) -> Result<MutationOutcome, WriteError> + Send + 'static {
    move |txn| {
        let placeholders = FINDING_TYPES.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "DELETE FROM maintenance_findings WHERE status = 'open' AND finding_type IN ({placeholders})"
        );
        txn.execute(&sql, rusqlite::params_from_iter(FINDING_TYPES.iter()))?;

        let mut findings = Vec::new();
        findings.extend(find_orphan_nodes(txn)?);
        findings.extend(find_unresolved_links(txn)?);
        findings.extend(find_stale_memory(txn)?);
        findings.extend(find_conflicting_memory(txn)?);

        let now = Utc::now().to_rfc3339();
        for f in &findings {
            let id = Uuid::new_v4().to_string();
            txn.execute(
                "INSERT INTO maintenance_findings (
                    id, finding_type, severity, entity_id, description, suggested_fix, status, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open', ?7)",
                params![id, f.finding_type, f.severity, f.entity_id, f.description, f.suggested_fix, now],
            )?;
        }

        let mut by_type = serde_json::Map::new();
        for ft in FINDING_TYPES {
            let count = findings.iter().filter(|f| f.finding_type == ft).count();
            by_type.insert(ft.to_string(), serde_json::json!(count));
        }

        Ok(MutationOutcome {
            entity_type: "maintenance_findings",
            entity_id: Uuid::new_v4().to_string(),
            action: ChangeAction::Create,
            diff_json: serde_json::json!({
                "findings_count": findings.len(),
                "by_type": by_type,
            }),
            reason: Some("maintenance lint pass".to_string()),
            before_hash: None,
            after_hash: None,
            node_snapshot: None,
            compiler_version: None,
        })
    }
}
