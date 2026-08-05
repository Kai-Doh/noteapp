use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SearchHitDto {
    pub id: String,
    pub title: String,
    pub node_type: String,
    pub snippet: String,
}

/// Quotes each whitespace-separated token as an FTS5 phrase so user input can't
/// inject FTS5 query syntax (column filters, `NEAR`, boolean operators, bare `-`,
/// unbalanced quotes/parens). Phrase-quoted tokens still combine with FTS5's
/// default implicit AND.
fn sanitize_fts_query(input: &str) -> String {
    input
        .split_whitespace()
        .map(|tok| format!("\"{}\"", tok.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn search_nodes(
    conn: &Connection,
    query: &str,
    node_type: Option<&str>,
    limit: i64,
) -> rusqlite::Result<Vec<SearchHitDto>> {
    let fts_query = sanitize_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let sql = if node_type.is_some() {
        "SELECT n.id, n.title, n.node_type,
                snippet(nodes_fts, 1, '', '', '...', 10) AS snippet
         FROM nodes_fts
         JOIN nodes n ON n.rowid = nodes_fts.rowid
         WHERE nodes_fts MATCH ?1 AND n.deleted_at IS NULL AND n.node_type = ?2
         ORDER BY rank LIMIT ?3"
    } else {
        "SELECT n.id, n.title, n.node_type,
                snippet(nodes_fts, 1, '', '', '...', 10) AS snippet
         FROM nodes_fts
         JOIN nodes n ON n.rowid = nodes_fts.rowid
         WHERE nodes_fts MATCH ?1 AND n.deleted_at IS NULL
         ORDER BY rank LIMIT ?2"
    };

    let mut stmt = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row| {
        Ok(SearchHitDto {
            id: row.get(0)?,
            title: row.get(1)?,
            node_type: row.get(2)?,
            snippet: row.get(3)?,
        })
    };
    let rows = if let Some(nt) = node_type {
        stmt.query_map(params![fts_query, nt, limit], map_row)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(params![fts_query, limit], map_row)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}
