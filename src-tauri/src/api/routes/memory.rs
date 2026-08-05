use axum::extract::{Path, Query, State};
use axum::routing::{get, patch, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use crate::api::dto::WriteResultDto;
use crate::api::error::ApiError;
use crate::api::request_ctx::RequestContext;
use crate::api::state::AppState;
use crate::auth::scope::Scope;
use crate::auth::AuthedActor;
use crate::db::writer::ActorKind;
use crate::domain::memory::{
    self, HotMemoryInput, MemoryTable, PatchHotMemoryInput, PatchUserProfileInput, UserProfileInput,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/context", get(get_context))
        .route("/hot", post(create_hot).get(list_hot))
        .route("/hot/{id}", patch(patch_hot).delete(delete_hot))
        .route("/profile", post(create_profile).get(list_profile))
        .route("/profile/{id}", patch(patch_profile).delete(delete_profile))
}

#[derive(Deserialize)]
struct ContextQuery {
    budget_hot: Option<i64>,
    budget_profile: Option<i64>,
}

async fn get_context(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Query(q): Query<ContextQuery>,
) -> Result<Json<memory::MemoryContextDto>, ApiError> {
    actor.require(Scope::Read)?;
    let budget_hot = q.budget_hot.unwrap_or(2200);
    let budget_profile = q.budget_profile.unwrap_or(1375);

    let compiled = {
        let conn = state.ro_pool.get()?;
        memory::compile_context(&conn, budget_hot, budget_profile)?
    };

    let hot_used: i64 = compiled.hot_included.iter().map(|c| c.char_count).sum();
    let profile_used: i64 = compiled.profile_included.iter().map(|c| c.char_count).sum();
    let hot_over = (hot_used as f64) > 0.8 * budget_hot as f64;
    let profile_over = (profile_used as f64) > 0.8 * budget_profile as f64;

    if !hot_over && !profile_over {
        return Ok(Json(compiled.dto));
    }

    // The overflow protocol is system-triggered maintenance, not something
    // the caller of GET /memory/context asked for — attribute it to
    // 'system' regardless of who (human/AI) happened to trigger this read.
    let system_actor = ctx.into_actor(ActorKind::System);
    if hot_over {
        memory::run_overflow_protocol(
            &state.writer,
            system_actor.clone(),
            MemoryTable::Hot,
            &compiled.hot_included,
            budget_hot,
        )
        .await?;
    }
    if profile_over {
        memory::run_overflow_protocol(
            &state.writer,
            system_actor,
            MemoryTable::Profile,
            &compiled.profile_included,
            budget_profile,
        )
        .await?;
    }

    // Recompile so the response reflects the just-evicted state, per the
    // plan's "recompile, verify under budget."
    let conn = state.ro_pool.get()?;
    let recompiled = memory::compile_context(&conn, budget_hot, budget_profile)?;
    Ok(Json(recompiled.dto))
}

async fn create_hot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Json(input): Json<HotMemoryInput>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteMemory)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, memory::create_hot_mutation(input, actor.kind))
        .await?;
    Ok(Json(outcome.into()))
}

async fn list_hot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    let items = memory::list_hot_memory(&conn)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn patch_hot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(input): Json<PatchHotMemoryInput>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteMemory)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, memory::update_hot_mutation(id, input, actor.kind))
        .await?;
    Ok(Json(outcome.into()))
}

async fn delete_hot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteMemory)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state.writer.write_tx(write_actor, memory::delete_hot_mutation(id)).await?;
    Ok(Json(outcome.into()))
}

async fn create_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Json(input): Json<UserProfileInput>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteMemory)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, memory::create_profile_mutation(input, actor.kind))
        .await?;
    Ok(Json(outcome.into()))
}

async fn list_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
) -> Result<Json<serde_json::Value>, ApiError> {
    actor.require(Scope::Read)?;
    let conn = state.ro_pool.get()?;
    let items = memory::list_user_profile(&conn)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn patch_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(input): Json<PatchUserProfileInput>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteMemory)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, memory::update_profile_mutation(id, input, actor.kind))
        .await?;
    Ok(Json(outcome.into()))
}

async fn delete_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthedActor>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<WriteResultDto>, ApiError> {
    actor.require(Scope::WriteMemory)?;
    let write_actor = ctx.into_actor(actor.kind);
    let outcome = state
        .writer
        .write_tx(write_actor, memory::delete_profile_mutation(id))
        .await?;
    Ok(Json(outcome.into()))
}
