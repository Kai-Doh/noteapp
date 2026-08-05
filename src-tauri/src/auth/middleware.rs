use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::api::state::AppState;

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = token else {
        return (StatusCode::UNAUTHORIZED, "missing Authorization: Bearer <token> header").into_response();
    };

    let Some(actor) = state.token_cache.lookup(token).cloned() else {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    };

    req.extensions_mut().insert(actor);
    next.run(req).await
}
