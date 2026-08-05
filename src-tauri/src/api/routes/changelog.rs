use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::Deserialize;

use crate::api::error::ApiError;
use crate::api::state::AppState;
use crate::auth::scope::Scope;
use crate::auth::AuthedActor;
use crate::domain::changelog;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_changelog))
}

#[derive(Deserialize)]
struct ChangelogQuery {
    actor: Option<String>,
    limit: Option<i64>,
}

async fn list_changelog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Query(q): Query<ChangelogQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let items = changelog::list_changelog(&conn, q.actor.as_deref(), limit)?;
    Ok(Json(serde_json::json!({ "items": items })))
}
