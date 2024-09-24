use axum::response::{IntoResponse, Response};
use http::StatusCode;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Failed to parse url: {0}.")]
    FailedToParse(String),
    #[error("Not found: {0}.")]
    NotFound(String),
    #[error("Failed to shorten: {0}.")]
    CannotShorten(String),
    #[error("Oops! Something went wrong: {0}")]
    DbError(String),
    #[error("Fail to retry.")]
    RetryFailed,
    #[error("Oops! Something went wrong. Please try again later.")]
    Unknown,
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let t = match self {
            ServiceError::FailedToParse(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            },
            ServiceError::NotFound(_) => {
                (StatusCode::NOT_FOUND, self.to_string())
            },
            ServiceError::CannotShorten(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, self.to_string())
            }
            ServiceError::DbError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Oops! Something went wrong.".to_string())
            },
            ServiceError::RetryFailed => {
                (StatusCode::SERVICE_UNAVAILABLE, "Please try again.".to_string())
            },
            ServiceError::Unknown => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            },
        };

        t.into_response()
    }
}
