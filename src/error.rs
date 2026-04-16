use actix_web::{HttpResponse, ResponseError};
use serde::Serialize;
use std::fmt;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Conflict(String),
    Internal(String),
    Database(rusqlite::Error),
    Json(serde_json::Error),
    Io(std::io::Error),
    Request(reqwest::Error),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: u16,
    message: String,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::NotFound(msg) => write!(f, "Not Found: {}", msg),
            AppError::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            AppError::Conflict(msg) => write!(f, "Conflict: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal Error: {}", msg),
            AppError::Database(e) => write!(f, "Database Error: {}", e),
            AppError::Json(e) => write!(f, "JSON Error: {}", e),
            AppError::Io(e) => write!(f, "IO Error: {}", e),
            AppError::Request(e) => write!(f, "Request Error: {}", e),
        }
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let (code, message) = match self {
            AppError::NotFound(msg) => (404, msg.clone()),
            AppError::BadRequest(msg) => (400, msg.clone()),
            AppError::Conflict(msg) => (409, msg.clone()),
            AppError::Internal(msg) => (500, msg.clone()),
            AppError::Database(e) => (500, format!("Database error: {}", e)),
            AppError::Json(e) => (400, format!("JSON error: {}", e)),
            AppError::Io(e) => (500, format!("IO error: {}", e)),
            AppError::Request(e) => (502, format!("Upstream error: {}", e)),
        };
        HttpResponse::build(actix_web::http::StatusCode::from_u16(code).unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR))
            .json(ErrorResponse {
                error: ErrorDetail { code, message },
            })
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Json(e)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Request(e)
    }
}

impl From<String> for AppError {
    fn from(e: String) -> Self {
        AppError::Internal(e)
    }
}

pub type AppResult<T> = Result<T, AppError>;
