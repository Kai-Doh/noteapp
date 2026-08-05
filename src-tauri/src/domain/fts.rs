use rusqlite::{params, Transaction};

/// Removes a rowid's previously-indexed tokens using FTS5's special `'delete'`
/// command, which takes the OLD column values explicitly. This is required for
/// external-content tables: a bare `DELETE FROM nodes_fts WHERE rowid = ?`
/// makes FTS5 look up the row via the content table (`nodes`) to figure out what
/// to detokenize — but by the time we call this, `nodes` already holds the *new*
/// values (or, for a brand-new row, was never indexed at all), so that implicit
/// lookup detokenizes the wrong text and corrupts the shadow tables' token
/// counts (observed as `SQLITE_CORRUPT` / "database disk image is malformed" on
/// the very next FTS5 operation). Passing the old values explicitly avoids the
/// lookup entirely.
fn remove_old_index(txn: &Transaction, rowid: i64, old_title: &str, old_content: &str) -> rusqlite::Result<()> {
    txn.execute(
        "INSERT INTO nodes_fts (nodes_fts, rowid, title, content) VALUES ('delete', ?1, ?2, ?3)",
        params![rowid, old_title, old_content],
    )?;
    Ok(())
}

/// Indexes a brand-new node. No prior entry exists, so this is a plain insert —
/// never preceded by a delete (see `remove_old_index` for why that would corrupt
/// the index rather than being a harmless no-op).
pub fn index_new_node(txn: &Transaction, node_id: &str, title: &str, content: &str) -> rusqlite::Result<()> {
    let rowid: i64 = txn.query_row(
        "SELECT rowid FROM nodes WHERE id = ?1",
        params![node_id],
        |row| row.get(0),
    )?;
    txn.execute(
        "INSERT INTO nodes_fts (rowid, title, content) VALUES (?1, ?2, ?3)",
        params![rowid, title, content],
    )?;
    Ok(())
}

/// Re-indexes an existing node whose title/content changed. Requires the OLD
/// title/content (captured by the caller before applying the update) so the
/// previous tokens can be correctly removed before the new ones are indexed.
pub fn reindex_updated_node(
    txn: &Transaction,
    node_id: &str,
    old_title: &str,
    old_content: &str,
    new_title: &str,
    new_content: &str,
) -> rusqlite::Result<()> {
    let rowid: i64 = txn.query_row(
        "SELECT rowid FROM nodes WHERE id = ?1",
        params![node_id],
        |row| row.get(0),
    )?;
    remove_old_index(txn, rowid, old_title, old_content)?;
    txn.execute(
        "INSERT INTO nodes_fts (rowid, title, content) VALUES (?1, ?2, ?3)",
        params![rowid, new_title, new_content],
    )?;
    Ok(())
}

/// Removes a deleted node's entry from the index. Requires the OLD title/content
/// for the same reason as `reindex_updated_node`.
pub fn deindex_node(txn: &Transaction, node_id: &str, old_title: &str, old_content: &str) -> rusqlite::Result<()> {
    let rowid: Option<i64> = txn
        .query_row(
            "SELECT rowid FROM nodes WHERE id = ?1",
            params![node_id],
            |row| row.get(0),
        )
        .ok();
    if let Some(rowid) = rowid {
        remove_old_index(txn, rowid, old_title, old_content)?;
    }
    Ok(())
}
