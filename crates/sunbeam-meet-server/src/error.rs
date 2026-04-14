//! Unified error type and mapping to `tonic::Status`.

use serde::Serialize;
use thiserror::Error;

/// Service-level error type.
///
/// Also available as [`MeetError`] alias for test/consumer clarity.
#[derive(Debug, Error)]
pub enum Error {
    /// Validation failure on an incoming request.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// Authentication failed or was missing.
    #[error("unauthenticated")]
    Unauthenticated,
    /// Authenticated identity lacks the required relation.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    /// Entity not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// Entity already exists.
    #[error("already exists: {0}")]
    AlreadyExists(String),
    /// Failed precondition (state guard, e.g. room not active).
    #[error("failed precondition: {0}")]
    FailedPrecondition(String),
    /// Resource exhausted (quota / limit).
    #[error("resource exhausted: {0}")]
    ResourceExhausted(String),
    /// Upstream / dependency unavailable.
    #[error("unavailable: {0}")]
    Unavailable(String),
    /// Backpressure on a streaming subscriber.
    #[error("backpressure")]
    Backpressure,
    /// PostgreSQL error.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    /// Valkey error.
    #[error("cache error: {0}")]
    Cache(#[from] redis::RedisError),
    /// NATS error.
    #[error("event bus error: {0}")]
    Nats(String),
    /// HTTP client error.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// JWT error.
    #[error("jwt error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
    /// Anything else, carried as an opaque `anyhow::Error` so we never leak
    /// the inner chain into tonic::Status messages (see `From<Error>`).
    #[error("internal error")]
    Internal(anyhow::Error),
}

/// Test-facing alias — handlers and tests refer to this name interchangeably.
pub type MeetError = Error;

impl Error {
    /// Short, stable code string used in the JSON error body. Do not rename
    /// without updating the `unit_error_payload_shape` snapshot.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Error::InvalidArgument(_) => "invalid_argument",
            Error::Unauthenticated => "unauthenticated",
            Error::PermissionDenied(_) => "permission_denied",
            Error::NotFound(_) => "not_found",
            Error::AlreadyExists(_) => "already_exists",
            Error::FailedPrecondition(_) => "failed_precondition",
            Error::ResourceExhausted(_) => "resource_exhausted",
            Error::Unavailable(_) => "unavailable",
            Error::Backpressure => "backpressure",
            Error::Database(_)
            | Error::Cache(_)
            | Error::Nats(_)
            | Error::Http(_)
            | Error::Jwt(_)
            | Error::Internal(_) => "internal",
        }
    }

    /// Operator-friendly message: safe to surface on the wire. Never includes
    /// the inner anyhow chain of `Internal` — that stays in logs only.
    #[must_use]
    pub fn public_message(&self) -> String {
        match self {
            Error::InvalidArgument(m)
            | Error::PermissionDenied(m)
            | Error::NotFound(m)
            | Error::AlreadyExists(m)
            | Error::FailedPrecondition(m)
            | Error::ResourceExhausted(m)
            | Error::Unavailable(m)
            | Error::Nats(m) => m.clone(),
            Error::Unauthenticated => "unauthenticated".into(),
            Error::Backpressure => "backpressure".into(),
            Error::Database(_) => "database error".into(),
            Error::Cache(_) => "cache error".into(),
            Error::Http(_) => "upstream http error".into(),
            Error::Jwt(_) => "jwt error".into(),
            Error::Internal(_) => "internal error".into(),
        }
    }
}

impl From<Error> for tonic::Status {
    fn from(e: Error) -> Self {
        tonic::Status::from(&e)
    }
}

impl From<&Error> for tonic::Status {
    fn from(e: &Error) -> Self {
        use tonic::Code;
        let code = match e {
            Error::InvalidArgument(_) => Code::InvalidArgument,
            Error::Unauthenticated => Code::Unauthenticated,
            Error::PermissionDenied(_) => Code::PermissionDenied,
            Error::NotFound(_) => Code::NotFound,
            Error::AlreadyExists(_) => Code::AlreadyExists,
            Error::FailedPrecondition(_) => Code::FailedPrecondition,
            Error::Backpressure | Error::ResourceExhausted(_) => Code::ResourceExhausted,
            Error::Unavailable(_) => Code::Unavailable,
            Error::Database(_)
            | Error::Cache(_)
            | Error::Nats(_)
            | Error::Http(_)
            | Error::Jwt(_)
            | Error::Internal(_) => Code::Internal,
        };
        tonic::Status::new(code, e.public_message())
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Internal(e)
    }
}

/// JSON body returned to REST / Connect-JSON clients.
///
/// The on-wire shape is pinned by `unit_error_payload_shape`. Any rename here
/// must be accompanied by an updated snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    /// Stable machine-readable code (e.g. `not_found`).
    pub code: &'static str,
    /// Operator-friendly message.
    pub message: String,
}

impl From<&Error> for ErrorBody {
    fn from(e: &Error) -> Self {
        Self {
            code: e.code(),
            message: e.public_message(),
        }
    }
}

impl From<Error> for ErrorBody {
    fn from(e: Error) -> Self {
        ErrorBody::from(&e)
    }
}

/// Convenient `Result` alias.
pub type Result<T> = std::result::Result<T, Error>;
