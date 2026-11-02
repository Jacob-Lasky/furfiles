use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Application error that renders as a JSON body the frontend and API clients
/// can rely on: `{ "error": "..." }` with an appropriate status code.
#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    /// Payload exceeds a size limit (e.g. an oversized upload) → 413.
    TooLarge(String),
    /// An upstream dependency (the LLM provider) failed → 502. The message is
    /// the provider's own, which carries no secret and helps diagnose a bad
    /// model ID or key.
    Upstream(String),
    /// A server-side failure that isn't the client's fault (e.g. writing an
    /// attachment to disk) → 500.
    Internal(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(m) => write!(f, "{m}"),
            AppError::BadRequest(m) => write!(f, "{m}"),
            AppError::TooLarge(m) => write!(f, "{m}"),
            AppError::Upstream(m) => write!(f, "{m}"),
            AppError::Internal(m) => write!(f, "{m}"),
            AppError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => AppError::NotFound("not found".into()),
            // A constraint violation is the CLIENT's mistake, not ours: a
            // `created_by` naming a user that doesn't exist is a foreign-key
            // failure, and reporting that as a 500 tells the caller the server
            // broke when it was their payload. 400 with the constraint message.
            sqlx::Error::Database(ref db) if db.is_foreign_key_violation() || db.is_check_violation() => {
                AppError::BadRequest(format!("constraint violation: {}", db.message()))
            }
            sqlx::Error::Database(ref db) if db.is_unique_violation() => {
                AppError::BadRequest(format!("already exists: {}", db.message()))
            }
            other => AppError::Db(other),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::TooLarge(m) => (StatusCode::PAYLOAD_TOO_LARGE, m),
            AppError::Upstream(m) => (StatusCode::BAD_GATEWAY, m),
            AppError::Internal(m) => {
                tracing::error!("internal error: {m}");
                (StatusCode::INTERNAL_SERVER_ERROR, m)
            }
            AppError::Db(e) => {
                tracing::error!("db error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
