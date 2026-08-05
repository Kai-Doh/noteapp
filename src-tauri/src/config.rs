use std::path::PathBuf;

/// Default port when `NOTEAPP_BIND_ADDR` isn't set — matches the port the
/// desktop app defaults to suggesting in its connection settings for local
/// development.
pub const DEFAULT_PORT: u16 = 47823;

/// Address the server binds to. Desktop-embedded use (if any) and local dev
/// default to loopback-only; the Docker image sets `NOTEAPP_BIND_ADDR=0.0.0.0:47823`
/// explicitly via its Dockerfile/compose env, since a container's loopback
/// isn't reachable from outside it.
pub fn bind_addr() -> String {
    std::env::var("NOTEAPP_BIND_ADDR").unwrap_or_else(|_| format!("127.0.0.1:{DEFAULT_PORT}"))
}

/// Where the database, backups, export, and token store live.
/// `NOTEAPP_DATA_DIR` lets the Docker image point this at its mounted
/// volume; without it, falls back to the OS data dir (desktop-embedded/dev use).
fn app_data_dir() -> PathBuf {
    let dir = match std::env::var_os("NOTEAPP_DATA_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => dirs::data_dir()
            .expect("could not resolve OS data directory")
            .join("noteapp"),
    };
    std::fs::create_dir_all(&dir).expect("could not create app data directory");
    dir
}

pub fn db_path() -> PathBuf {
    app_data_dir().join("vault.db")
}

pub fn token_store_path() -> PathBuf {
    app_data_dir().join("auth_tokens.json")
}

pub fn backups_dir() -> PathBuf {
    app_data_dir().join("backups")
}

pub fn export_dir() -> PathBuf {
    app_data_dir().join("export")
}

/// Appends a literal suffix to `db.db`'s filename (not `PathBuf::with_extension`,
/// which would replace ".db" rather than append after it) — used for SQLite's
/// own `-wal`/`-shm` sidecar files and the restore-staging marker.
fn sibling_with_suffix(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().expect("db path must have a file name").to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

pub fn wal_path() -> PathBuf {
    sibling_with_suffix(&db_path(), "-wal")
}

pub fn shm_path() -> PathBuf {
    sibling_with_suffix(&db_path(), "-shm")
}

/// A staged restore lands here (see `backup::engine::stage_restore`) and is
/// swapped into `db_path()` the next time the app boots, before anything opens
/// the live file — see `server.rs`'s `apply_pending_restore_if_any`.
pub fn pending_restore_path() -> PathBuf {
    sibling_with_suffix(&db_path(), ".restore-pending")
}
