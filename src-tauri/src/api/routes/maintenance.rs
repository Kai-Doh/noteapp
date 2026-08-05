use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use crate::api::dto::WriteResultDto;
use crate::api::error::ApiError;
use crate::api::request_ctx::RequestContext;
use crate::api::state::AppState;
use crate::auth::scope::Scope;
use crate::auth::AuthedActor;
use crate::domain::maintenance;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/run", post(run_maintenance))
        .route("/findings", get(list_findings))
}

async fn run_maintenance(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::Maintenance)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state.writer.write_tx(write_actor, maintenance::run_lint_pass()).await?;
    Ok(Json(outcome.into()))
}

#[derive(Deserialize)]
struct FindingsQuery {
    status: Option<String>,
    finding_type: Option<String>,
}

async fn list_findings(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Query(q): Query<FindingsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    let items = maintenance::list_findings(&conn, q.status.as_deref(), q.finding_type.as_deref())?;
    Ok(Json(serde_json::json!({ "items": items })))
}
