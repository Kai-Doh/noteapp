use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::Deserialize;

use crate::api::error::ApiError;
use crate::api::state::AppState;
use crate::auth::scope::Scope;
use crate::auth::AuthedActor;
use crate::domain::search;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(search_nodes))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    node_type: Option<String>,
    limit: Option<i64>,
}

async fn search_nodes(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let items = search::search_nodes(&conn, &q.q, q.node_type.as_deref(), limit)?;
    Ok(Json(serde_json::json!({ "items": items })))
}
