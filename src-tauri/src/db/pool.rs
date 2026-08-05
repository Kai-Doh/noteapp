use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::Path;

pub type ReadPool = r2d2::Pool<SqliteConnectionManager>;

/// Pragmas that only take effect outside a pending transaction (journal_mode,
/// foreign_keys) are applied here directly on each raw connection, never inside
/// a refinery migration transaction where SQLite would silently no-op them.
fn apply_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    Ok(())
}

/// Opens the single connection the writer thread will own for the lifetime of the app.
pub fn open_write_connection(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    apply_pragmas(&conn)?;
    Ok(conn)
}

/// Read-only connection pool. Safe for concurrent use under WAL — reads never
/// contend with the single writer thread.
pub fn build_read_pool(db_path: &Path) -> Result<ReadPool, r2d2::Error> {
    let manager = SqliteConnectionManager::file(db_path).with_init(|conn| {
        apply_pragmas(conn)?;
        conn.pragma_update(None, "query_only", true)?;
        Ok(())
    });
    r2d2::Pool::builder().max_size(8).build(manager)
}
