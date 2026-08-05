use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use crate::api::dto::WriteResultDto;
use crate::api::error::ApiError;
use crate::api::state::AppState;
use crate::auth::scope::Scope;
use crate::auth::AuthedActor;
use crate::api::request_ctx::RequestContext;
use crate::domain::aliases::{self, CreateAliasInput};
use crate::domain::node::{self, CreateNodeInput, PatchNodeInput};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_node).get(list_nodes))
        .route("/{id}", get(get_node).patch(patch_node).delete(delete_node))
        .route("/{id}/append", post(append_node))
        .route("/{id}/backlinks", get(get_backlinks))
        .route("/{id}/aliases", post(create_alias).get(list_aliases))
        .route("/{id}/aliases/{alias_id}", axum::routing::delete(delete_alias))
}

async fn create_node(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Json(input): Json<CreateNodeInput>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteNotes)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, node::create_mutation(input, actor.kind))
        .await?;
    Ok(Json(outcome.into()))
}

#[derive(Deserialize)]
struct ListNodesQuery {
    node_type: Option<String>,
    created_by: Option<String>,
    limit: Option<i64>,
}

async fn list_nodes(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Query(q): Query<ListNodesQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let items = node::list_nodes(&conn, q.node_type.as_deref(), q.created_by.as_deref(), limit)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn get_node(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Path(id): Path<String>,
) -> Result<Json<node::NodeDto>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    match node::get_node(&conn, &id)? {
        Some(dto) => Ok(Json(dto)),
        None => Err(ApiError::Write(crate::db::writer::WriteError::NotFound(format!(
            "node {id} not found"
        )))),
    }
}

async fn patch_node(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(input): Json<PatchNodeInput>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteNotes)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, node::patch_mutation(id, input, actor.kind))
        .await?;
    Ok(Json(outcome.into()))
}

#[derive(Deserialize)]
struct AppendNodeBody {
    content_to_append: String,
}

async fn delete_node(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteNotes)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state.writer.write_tx(write_actor, node::delete_mutation(id)).await?;
    Ok(Json(outcome.into()))
}

async fn append_node(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(body): Json<AppendNodeBody>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteNotes)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(
            write_actor,
            node::append_mutation(id, body.content_to_append, actor.kind),
        )
        .await?;
    Ok(Json(outcome.into()))
}

async fn get_backlinks(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    let items = node::get_backlinks(&conn, &id)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn create_alias(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(input): Json<CreateAliasInput>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteNotes)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, aliases::create_mutation(id, input, actor.kind))
        .await?;
    Ok(Json(outcome.into()))
}

async fn list_aliases(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    let items = aliases::list_aliases(&conn, &id)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn delete_alias(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path((_id, alias_id)): Path<(String, String)>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteNotes)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, aliases::delete_mutation(alias_id))
        .await?;
    Ok(Json(outcome.into()))
}
