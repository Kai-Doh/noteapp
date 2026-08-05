pub mod engine;

use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_RETENTION: usize = 14;
const BACKUP_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub fn run_backup_now(db_path: &std::path::Path, backups_dir: &std::path::Path) -> rusqlite::Result<engine::BackupManifest> {
    let manifest = engine::create_backup(db_path, backups_dir)?;
    engine::prune_old_backups(backups_dir, DEFAULT_RETENTION);
    Ok(manifest)
}

/// Rolling backups, per the plan: taken on a schedule (this task), kept for
/// the last N, auto-pruned. Plain `tokio::spawn` (not `tauri::async_runtime`)
/// so this works identically in the headless server binary and any future
/// Tauri-embedded use — both run on a tokio runtime either way. Independent
/// of the writer thread — `create_backup` reads through a fresh read-only
/// connection, so it never contends with the single writer.
pub fn spawn_scheduled_backups(db_path: PathBuf, backups_dir: PathBuf) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(BACKUP_INTERVAL);
        interval.tick().await; // first tick fires immediately; skip it, startup isn't a great first backup moment
        loop {
            interval.tick().await;
            match run_backup_now(&db_path, &backups_dir) {
                Ok(m) => tracing::info!(
                    "scheduled backup created: {} (integrity: {})",
                    m.id,
                    m.integrity_check
                ),
                Err(e) => tracing::error!("scheduled backup failed: {e}"),
            }
        }
    });
}
