use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::api::state::AppState;
use crate::db::writer::{Actor, ActorKind};

/// Per-request identifiers stamped onto every `changelog` row this request causes.
/// `source_session_id`/`source_task_id` let an external AI caller correlate its
/// own session/task with the audit log via optional headers.
#[derive(Clone, Debug)]
pub struct RequestContext {
    pub request_id: String,
    pub source_session_id: Option<String>,
    pub source_task_id: Option<String>,
}

impl RequestContext {
    pub fn into_actor(self, kind: ActorKind) -> Actor {
        Actor {
            kind,
            request_id: self.request_id,
            source_session_id: self.source_session_id,
            source_task_id: self.source_task_id,
        }
    }
}

/// Takes `State<AppState>` only so it can be layered via `from_fn_with_state`
/// alongside the auth middleware (it doesn't otherwise need app state).
pub async fn request_context_middleware(
    State(_state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let source_session_id = req
        .headers()
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let source_task_id = req
        .headers()
        .get("x-task-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let ctx = RequestContext {
        request_id: Uuid::new_v4().to_string(),
        source_session_id,
        source_task_id,
    };
    req.extensions_mut().insert(ctx);
    next.run(req).await
}
