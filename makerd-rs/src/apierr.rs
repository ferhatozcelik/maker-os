//! API hata gövdesi (`{"error":{"code","message"}}`). Modüller bu türü döndürür;
//! httpd onu HTTP durumuna çevirir. İletiler Türkçedir.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: u16, code: &'static str, msg: impl Into<String>) -> Self {
        Self { status, code, message: msg.into() }
    }
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::new(400, "invalid", msg)
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(401, "unauthorized", msg)
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(403, "forbidden", msg)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(404, "not_found", msg)
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(409, "conflict", msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(500, "internal", msg)
    }
    pub fn unavailable(msg: impl Into<String>) -> Self {
        Self::new(503, "unavailable", msg)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::internal(e.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::internal(e.to_string())
    }
}
