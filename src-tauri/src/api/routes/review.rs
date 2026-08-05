use axum::extract::{Path, Query, State};
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::Deserialize;

use crate::api::dto::WriteResultDto;
use crate::api::error::ApiError;
use crate::api::request_ctx::RequestContext;
use crate::api::state::AppState;
use crate::auth::scope::Scope;
use crate::auth::AuthedActor;
use crate::domain::review::{self, ProposeInput};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(propose).get(list_review))
        .route("/{id}/approve", post(approve))
        .route("/{id}/reject", post(reject))
        .route("/{id}/apply", post(apply))
}

#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
}

async fn list_review(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    let items = review::list_review_items(&conn, q.status.as_deref())?;
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn propose(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Json(input): Json<ProposeInput>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteMemory)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, review::propose_mutation(input, actor.kind))
        .await?;
    Ok(Json(outcome.into()))
}

#[derive(Deserialize, Default)]
struct RejectBody {
    reason: Option<String>,
}

async fn approve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteMemory)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, review::approve_mutation(id, actor.kind))
        .await?;
    Ok(Json(outcome.into()))
}

async fn reject(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    body: Option<Json<RejectBody>>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteMemory)?;
    let write_actor = ctx.into_actor(actor.kind);
    let reason = body.map(|b| b.0.reason).unwrap_or(None);
    let outcome = state
        .writer
        .write_tx(write_actor, review::reject_mutation(id, reason, actor.kind))
        .await?;
    Ok(Json(outcome.into()))
}

async fn apply(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteMemory)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor.clone(), review::apply_mutation(id, write_actor))
        .await?;
    Ok(Json(outcome.into()))
}
