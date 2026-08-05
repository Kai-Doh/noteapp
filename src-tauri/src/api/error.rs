use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::auth::ScopeError;
use crate::db::writer::WriteError;

pub enum ApiError {
    Write(WriteError),
    Pool(r2d2::Error),
    Forbidden(String),
    BadRequest(String),
}

impl From<WriteError> for ApiError {
    fn from(e: WriteError) -> Self {
        ApiError::Write(e)
    }
}

impl From<r2d2::Error> for ApiError {
    fn from(e: r2d2::Error) -> Self {
        ApiError::Pool(e)
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        ApiError::Write(WriteError::Sqlite(e))
    }
}

impl From<ScopeError> for ApiError {
    fn from(e: ScopeError) -> Self {
        ApiError::Forbidden(e.0)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::Write(WriteError::NotFound(m)) => (StatusCode::NOT_FOUND, m),
            ApiError::Write(WriteError::Conflict(m)) => (StatusCode::CONFLICT, m),
            ApiError::Write(WriteError::Invalid(m)) => (StatusCode::BAD_REQUEST, m),
            ApiError::Write(WriteError::QueueClosed) => {
                (StatusCode::SERVICE_UNAVAILABLE, "writer queue is unavailable".to_string())
            }
            ApiError::Write(WriteError::Sqlite(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::Pool(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::Forbidden(m) => (StatusCode::FORBIDDEN, m),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
