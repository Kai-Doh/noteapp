use axum::http::{HeaderValue, Method};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::api::request_ctx::request_context_middleware;
use crate::api::routes;
use crate::api::state::AppState;
use crate::auth::middleware::auth_middleware;

/// The webview's page origin, not 127.0.0.1:<API_PORT> — the frontend and the
/// local API are on different origins (different port in dev, different
/// scheme entirely once bundled), so without explicit CORS the browser blocks
/// every fetch from the frontend before it ever reaches a route handler.
/// Listed explicitly (not a wildcard) since this API accepts a bearer token —
/// only these known first-party origins should ever be allowed to send it.
fn allowed_origins() -> [HeaderValue; 3] {
    [
        HeaderValue::from_static("http://localhost:1420"), // `npm run dev` / `tauri dev`
        HeaderValue::from_static("tauri://localhost"),      // bundled app, macOS/Linux
        HeaderValue::from_static("http://tauri.localhost"), // bundled app, Windows
    ]
}

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(allowed_origins())
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers(Any);

    Router::new()
        .nest("/nodes", routes::nodes::router())
        .nest("/search", routes::search::router())
        .nest("/backup", routes::backup::router())
        .nest("/export", routes::export::router())
        .nest("/memory", routes::memory::router())
        .nest("/review", routes::review::router())
        .nest("/changelog", routes::changelog::router())
        .nest("/graph", routes::graph::router())
        .nest("/maintenance", routes::maintenance::router())
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            request_context_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
