use axum::extract::Path;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};

use crate::api::error::ApiError;
use crate::api::state::AppState;
use crate::auth::scope::Scope;
use crate::auth::AuthedActor;
use crate::backup::{self, engine::RestoreError};
use crate::config;
use crate::db::writer::WriteError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_backups).post(create_backup))
        .route("/{id}/restore", post(restore_backup))
}

fn restore_error_to_api(e: RestoreError) -> ApiError {
    match e {
        RestoreError::NotFound(id) => ApiError::Write(WriteError::NotFound(format!("backup {id} not found"))),
        other => ApiError::BadRequest(other.to_string()),
    }
}

async fn list_backups(Extension(actor): Extension<AuthedActor>) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Backup)?;
    let items = backup::engine::list_backups(&config::backups_dir());
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn create_backup(
    Extension(actor): Extension<AuthedActor>,
) -> Result<Json<backup::engine::BackupManifest>, ApiError> {
    actor.require(Scope::Backup)?;
    let manifest = backup::run_backup_now(&config::db_path(), &config::backups_dir())
        .map_err(WriteError::Sqlite)?;
    Ok(Json(manifest))
}

async fn restore_backup(
    Extension(actor): Extension<AuthedActor>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Backup)?;
    backup::engine::stage_restore(&config::backups_dir(), &id, &config::pending_restore_path())
        .map_err(restore_error_to_api)?;
    Ok(Json(serde_json::json!({
        "staged": true,
        "backup_id": id,
        "note": "Restore is staged, not yet applied — the live database is never swapped out from under an open connection. Restart the app to apply it; this happens automatically on the next clean startup."
    })))
}
