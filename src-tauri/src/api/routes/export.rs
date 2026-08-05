use axum::extract::State;
use axum::routing::post;
use axum::{Extension, Json, Router};

use crate::api::error::ApiError;
use crate::api::state::AppState;
use crate::auth::scope::Scope;
use crate::auth::AuthedActor;
use crate::config;
use crate::export::markdown;

pub fn router() -> Router<AppState> {
    Router::new().route("/", post(run_export))
}

async fn run_export(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
) -> Result<Json<markdown::ExportResult>, ApiError> {
    actor.require(Scope::Export)?;
    let conn = state.ro_pool.get()?;
    let result = markdown::export_vault(&conn, &config::export_dir())?;
    Ok(Json(result))
}
