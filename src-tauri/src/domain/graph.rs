use rusqlite::{types::Value as SqlValue, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Default)]
pub struct GraphQuery {
    pub node_type: Option<String>,
    pub actor: Option<String>, // maps to created_by
    pub updated_after: Option<String>,
    pub updated_before: Option<String>,
    pub tag: Option<String>, // matches a property with key='tag'
    #[serde(default)]
    pub hide_daily: bool,
    #[serde(default)]
    pub unresolved_only: bool,
    #[serde(default)]
    pub ai_written_only: bool,
    #[serde(default)]
    pub pending_review_only: bool,
}

#[derive(Debug, Serialize)]
pub struct GraphNodeDto {
    pub id: String,
    pub title: String,
    pub node_type: String,
    pub created_by: String,
    /// Small, stable palette key so the frontend never has to update its color
    /// map when a new node_type is introduced — driven by `created_by` (only
    /// ever 'user'/'ai'/'system', unlike node_type which can grow over time).
    /// `node_type` is exposed separately above for any secondary encoding
    /// (shape, opacity) the frontend wants without overloading color_key.
    pub color_key: String,
}

#[derive(Debug, Serialize)]
pub struct GraphEdgeDto {
    pub source: String,
    /// None for unresolved links — there's no second node to connect to, but
    /// the edge is still surfaced so the UI can render it as a dangling stub
    /// on the source node rather than silently dropping it.
    pub target: Option<String>,
    pub link_type: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct GraphDto {
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
}

fn color_key(created_by: &str) -> &'static str {
    match created_by {
        "ai" => "ai",
        "system" => "system",
        _ => "user",
    }
}

/// Builds the filtered node id set first (all filters applied in SQL), then
/// scopes the edge query to that set — keeps graph queries proportional to
/// the filtered result rather than the whole vault, per the plan.
pub fn fetch_graph(conn: &Connection, q: &GraphQuery) -> rusqlite::Result<GraphDto> {
    let mut where_clauses = vec!["deleted_at IS NULL".to_string()];
    let mut params: Vec<SqlValue> = Vec::new();

    if let Some(nt) = &q.node_type {
        where_clauses.push("node_type = ?".to_string());
        params.push(SqlValue::Text(nt.clone()));
    }
    if let Some(actor) = &q.actor {
        where_clauses.push("created_by = ?".to_string());
        params.push(SqlValue::Text(actor.clone()));
    }
    if let Some(after) = &q.updated_after {
        where_clauses.push("updated_at >= ?".to_string());
        params.push(SqlValue::Text(after.clone()));
    }
    if let Some(before) = &q.updated_before {
        where_clauses.push("updated_at <= ?".to_string());
        params.push(SqlValue::Text(before.clone()));
    }
    if q.hide_daily {
        where_clauses.push("node_type != 'journal'".to_string());
    }
    if q.ai_written_only {
        where_clauses.push("created_by = 'ai'".to_string());
    }
    if let Some(tag) = &q.tag {
        where_clauses.push(
            "EXISTS (SELECT 1 FROM properties p WHERE p.node_id = nodes.id AND p.key = 'tag' AND p.value_text = ?)"
                .to_string(),
        );
        params.push(SqlValue::Text(tag.clone()));
    }
    if q.pending_review_only {
        where_clauses.push(
            "EXISTS (SELECT 1 FROM ai_review_queue q WHERE q.entity_type = 'node' AND q.entity_id = nodes.id AND q.status IN ('pending','approved'))"
                .to_string(),
        );
    }
    if q.unresolved_only {
        where_clauses.push(
            "EXISTS (SELECT 1 FROM links l WHERE l.source_node_id = nodes.id AND l.status = 'unresolved')"
                .to_string(),
        );
    }

    let sql = format!(
        "SELECT id, title, node_type, created_by FROM nodes WHERE {} ORDER BY updated_at DESC",
        where_clauses.join(" AND ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let nodes: Vec<GraphNodeDto> = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            let created_by: String = row.get(3)?;
            Ok(GraphNodeDto {
                id: row.get(0)?,
                title: row.get(1)?,
                node_type: row.get(2)?,
                color_key: color_key(&created_by).to_string(),
                created_by,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    if nodes.is_empty() {
        return Ok(GraphDto { nodes, edges: Vec::new() });
    }

    let id_placeholders = nodes.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let mut edge_params: Vec<SqlValue> = Vec::new();
    // source must be in the filtered set
    for n in &nodes {
        edge_params.push(SqlValue::Text(n.id.clone()));
    }
    // target must be in the filtered set OR null (dangling/unresolved edges
    // are always surfaced — there's nothing to filter them against)
    for n in &nodes {
        edge_params.push(SqlValue::Text(n.id.clone()));
    }

    let mut edge_sql = format!(
        "SELECT source_node_id, target_node_id, link_type, status FROM links
         WHERE source_node_id IN ({id_placeholders})
           AND (target_node_id IS NULL OR target_node_id IN ({id_placeholders}))"
    );
    if q.unresolved_only {
        edge_sql.push_str(" AND status = 'unresolved'");
    }

    let mut edge_stmt = conn.prepare(&edge_sql)?;
    let edges: Vec<GraphEdgeDto> = edge_stmt
        .query_map(rusqlite::params_from_iter(edge_params.iter()), |row| {
            Ok(GraphEdgeDto {
                source: row.get(0)?,
                target: row.get(1)?,
                link_type: row.get(2)?,
                status: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(GraphDto { nodes, edges })
}
