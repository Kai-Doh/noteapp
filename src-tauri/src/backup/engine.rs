use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub id: String,
    pub created_at: String,
    pub db_path: String,
    pub schema_version: i64,
    pub app_version: String,
    pub integrity_check: String, // "ok" | "failed"
    pub included_config: Vec<String>,
    pub excluded_secrets: bool,
    pub size_bytes: u64,
}

fn snapshot_path(backups_dir: &Path, id: &str) -> std::path::PathBuf {
    backups_dir.join(format!("{id}.db"))
}

fn manifest_path(backups_dir: &Path, id: &str) -> std::path::PathBuf {
    backups_dir.join(format!("{id}.manifest.json"))
}

/// Copies the live database to a new timestamped snapshot via SQLite's Online
/// Backup API (safe to run against a WAL database with an active writer — it
/// reads through SQLite's own consistent-snapshot mechanism, not a raw file
/// copy), then runs `PRAGMA integrity_check` against the snapshot itself and
/// records the result in its manifest rather than assuming success.
pub fn create_backup(db_path: &Path, backups_dir: &Path) -> rusqlite::Result<BackupManifest> {
    std::fs::create_dir_all(backups_dir).expect("failed to create backups directory");

    let id = Utc::now().format("%Y%m%dT%H%M%S%3fZ").to_string();
    let dest_path = snapshot_path(backups_dir, &id);

    let src = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut dest = Connection::open(&dest_path)?;
    {
        let backup = rusqlite::backup::Backup::new(&src, &mut dest)?;
        backup.run_to_completion(5, Duration::from_millis(250), None)?;
    }

    // The backup copies the source's on-disk page format byte-for-byte,
    // including its WAL-mode header — so the snapshot comes out of
    // run_to_completion still flagged as a WAL database despite having no
    // -wal file of its own yet. A snapshot is meant to be a single portable
    // file; switching it to a rollback journal here forces a full checkpoint
    // and permanently drops the WAL requirement, so every later open
    // (including a read-only one with no sidecar files present, as
    // `stage_restore` needs) just works instead of SQLite having to
    // materialize -wal/-shm files for a plain read.
    dest.pragma_update(None, "journal_mode", "DELETE")?;

    let schema_version: i64 = dest.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let integrity_check: String = dest.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    drop(dest);

    let size_bytes = std::fs::metadata(&dest_path).map(|m| m.len()).unwrap_or(0);

    let manifest = BackupManifest {
        id: id.clone(),
        created_at: Utc::now().to_rfc3339(),
        db_path: db_path.display().to_string(),
        schema_version,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        integrity_check,
        included_config: vec![
            "nodes".into(),
            "properties".into(),
            "links".into(),
            "changelog".into(),
            "node_revisions".into(),
            "hot_memory".into(),
            "user_profile".into(),
        ],
        excluded_secrets: true, // auth tokens live outside the DB (see auth::store) — never backed up here
        size_bytes,
    };

    std::fs::write(
        manifest_path(backups_dir, &id),
        serde_json::to_string_pretty(&manifest).expect("serialize backup manifest"),
    )
    .expect("failed to write backup manifest");

    Ok(manifest)
}

pub fn list_backups(backups_dir: &Path) -> Vec<BackupManifest> {
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(backups_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".manifest.json") {
                continue;
            }
            if let Ok(data) = std::fs::read_to_string(entry.path()) {
                if let Ok(manifest) = serde_json::from_str::<BackupManifest>(&data) {
                    items.push(manifest);
                }
            }
        }
    }
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    items
}

/// Deletes every backup beyond the most recent `keep_last_n` (by creation time).
pub fn prune_old_backups(backups_dir: &Path, keep_last_n: usize) {
    for old in list_backups(backups_dir).into_iter().skip(keep_last_n) {
        let _ = std::fs::remove_file(snapshot_path(backups_dir, &old.id));
        let _ = std::fs::remove_file(manifest_path(backups_dir, &old.id));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("no backup found with id '{0}'")]
    NotFound(String),
    #[error("backup snapshot failed its own integrity check — refusing to stage it")]
    FailedIntegrityCheck,
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Copies a snapshot to the pending-restore marker path and re-verifies its
/// integrity there (belt-and-braces on top of the check already recorded at
/// backup time). Never touches the live database file directly — the actual
/// swap happens at the next clean startup, before anything has it open (see
/// `lib.rs::apply_pending_restore_if_any`), matching the plan's "never
/// in-place over the live DB while it's open."
pub fn stage_restore(backups_dir: &Path, id: &str, pending_path: &Path) -> Result<(), RestoreError> {
    let src = snapshot_path(backups_dir, id);
    if !src.exists() {
        return Err(RestoreError::NotFound(id.to_string()));
    }
    std::fs::copy(&src, pending_path)?;

    // Not read-only: `PRAGMA integrity_check` needs to write transient state to
    // validate FTS5's shadow-table inverted index, so a strictly read-only
    // connection fails with "attempt to write a readonly database" even when
    // the file is perfectly fine — this is our own private copy either way
    // (accepted as the restore, or deleted below), so read-write is safe here.
    let conn = Connection::open(pending_path)?;
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    drop(conn);
    if integrity != "ok" {
        let _ = std::fs::remove_file(pending_path);
        return Err(RestoreError::FailedIntegrityCheck);
    }
    Ok(())
}
