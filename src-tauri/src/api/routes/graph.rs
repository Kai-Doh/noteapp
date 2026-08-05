use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Extension, Json, Router};

use crate::api::error::ApiError;
use crate::api::state::AppState;
use crate::auth::scope::Scope;
use crate::auth::AuthedActor;
use crate::domain::graph::{self, GraphDto, GraphQuery};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_graph))
}

async fn get_graph(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Query(q): Query<GraphQuery>,
) -> Result<Json<GraphDto>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    let dto = graph::fetch_graph(&conn, &q)?;
    Ok(Json(dto))
}
