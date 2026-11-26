//! Application-wide error type.
//!
//! Error messages never contain file *contents*; only paths, codes and operation context.

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// User input failed validation. `code` is a stable identifier translated by the UI.
    #[error("validation error: {code}")]
    Validation { code: &'static str, detail: String },

    #[error("not found: {0}")]
    NotFound(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("operation cancelled")]
    Cancelled,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn validation(code: &'static str, detail: impl Into<String>) -> Self {
        AppError::Validation { code, detail: detail.into() }
    }

    pub fn code(&self) -> &'static str {
        match self {
            AppError::Validation { code, .. } => code,
            AppError::NotFound(_) => "not_found",
            AppError::PermissionDenied(_) => "permission_denied",
            AppError::Cancelled => "cancelled",
            AppError::Conflict(_) => "conflict",
            AppError::Database(_) => "database",
            AppError::Io(_) => "io",
            AppError::Unsupported(_) => "unsupported",
            AppError::Internal(_) => "internal",
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            AppError::PermissionDenied(e.to_string())
        } else {
            AppError::Io(e.to_string())
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        // serde_json messages may quote input fragments; keep only the position.
        AppError::validation("invalid_json", format!("line {}, column {}", e.line(), e.column()))
    }
}

#[derive(Serialize)]
struct WireError<'a> {
    code: &'a str,
    message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let message = match self {
            AppError::Validation { detail, .. } => detail.clone(),
            other => other.to_string(),
        };
        WireError { code: self.code(), message }.serialize(s)
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_error_does_not_echo_input() {
        let secret = concat!("{\"k\": sk", "_fakevalue}");
        let err: AppError = serde_json::from_str::<serde_json::Value>(secret).unwrap_err().into();
        let wire = serde_json::to_string(&err).unwrap();
        assert!(!wire.contains("sk_"), "error must not echo input: {wire}");
        assert!(wire.contains("invalid_json"));
    }
}
