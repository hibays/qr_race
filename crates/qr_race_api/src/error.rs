//! API errors become status codes and a one-field JSON body.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use qr_race_core::CoreError;
use qr_race_hub::{AuthError, HubError};
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    BadRequest(String),
    Conflict(String),
    TooManyRequests(String),
    ServiceUnavailable(String),
    Internal(String),
}

impl ApiError {
    fn parts(&self) -> (StatusCode, &str) {
        match self {
            ApiError::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message),
            ApiError::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            ApiError::NotFound(message) => (StatusCode::NOT_FOUND, message),
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Conflict(message) => (StatusCode::CONFLICT, message),
            ApiError::TooManyRequests(message) => (StatusCode::TOO_MANY_REQUESTS, message),
            ApiError::ServiceUnavailable(message) => (StatusCode::SERVICE_UNAVAILABLE, message),
            ApiError::Internal(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = self.parts();
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        let message = error.to_string();
        match error {
            AuthError::WeakToken | AuthError::BadSecret(_) => ApiError::BadRequest(message),
            AuthError::DuplicateAccount => ApiError::Conflict(message),
            AuthError::UnknownToken | AuthError::InvalidTotp | AuthError::ReplayedTotp => {
                ApiError::Unauthorized(message)
            }
            AuthError::Locked { .. } => ApiError::TooManyRequests(message),
        }
    }
}

impl From<HubError> for ApiError {
    fn from(error: HubError) -> Self {
        let message = error.to_string();
        match error {
            HubError::RoomExists(_) => ApiError::Conflict(message),
            HubError::RoomNotFound(_) => ApiError::NotFound(message),
            HubError::RoomNotLit(_) => ApiError::Conflict(message),
            HubError::NotAScanner | HubError::NotAParticipant => ApiError::Forbidden(message),
            HubError::InvalidPin(_) => ApiError::BadRequest(message),
            HubError::RelayNotFound(_) => ApiError::NotFound(message),
            HubError::TooManyRooms => ApiError::ServiceUnavailable(message),
            HubError::Storage(_) => ApiError::Internal(message),
            HubError::Core(core) => ApiError::from(core),
        }
    }
}

impl From<CoreError> for ApiError {
    fn from(error: CoreError) -> Self {
        let message = error.to_string();
        match error {
            CoreError::NotHost { .. } => ApiError::Forbidden(message),
            CoreError::HostCannotLeave | CoreError::AlreadyLit | CoreError::AlreadyCoHost => {
                ApiError::Conflict(message)
            }
            CoreError::Address(_)
            | CoreError::Empty(_)
            | CoreError::RelayTooLarge { .. }
            | CoreError::NotAMember
            | CoreError::NotACoHost
            | CoreError::AlreadyHost => ApiError::BadRequest(message),
        }
    }
}
