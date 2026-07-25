use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, Clone, serde::Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("authentication required")]
    Unauthorized,

    #[error("not permitted")]
    Forbidden,

    #[error("not found")]
    NotFound,

    /// Deliberately identical for expired, already-consumed, and unknown
    /// tokens so that the response cannot be used to probe for registered
    /// email addresses.
    #[error("this login link is no longer valid")]
    InvalidToken,

    #[error("too many requests")]
    RateLimited { retry_after_seconds: u64 },

    #[error("validation failed")]
    Validation(Vec<FieldError>),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::Internal(err.into())
    }
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::InvalidToken => StatusCode::BAD_REQUEST,
            ApiError::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            ApiError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Stable machine-readable discriminator. The frontend branches on this,
    /// never on the human-readable message.
    fn type_slug(&self) -> &'static str {
        match self {
            ApiError::Unauthorized => "unauthorized",
            ApiError::Forbidden => "forbidden",
            ApiError::NotFound => "not_found",
            ApiError::InvalidToken => "invalid_token",
            ApiError::RateLimited { .. } => "rate_limited",
            ApiError::Validation(_) => "validation_failed",
            ApiError::Internal(_) => "internal_error",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let slug = self.type_slug();

        let mut body = json!({
            "type": slug,
            "title": self.to_string(),
            "status": status.as_u16(),
        });

        match &self {
            ApiError::Validation(errors) => {
                body["errors"] = serde_json::to_value(errors).unwrap_or(json!([]));
            }
            ApiError::RateLimited {
                retry_after_seconds,
            } => {
                body["retry_after"] = json!(retry_after_seconds);
            }
            ApiError::Internal(cause) => {
                // The cause is logged, never returned: it can contain
                // connection strings and other internals.
                let correlation_id = uuid::Uuid::new_v4();
                tracing::error!(%correlation_id, error = ?cause, "internal error");
                body["title"] = json!("something went wrong");
                body["correlation_id"] = json!(correlation_id);
            }
            _ => {}
        }

        let mut response = (status, axum::Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/problem+json"),
        );

        if let ApiError::RateLimited {
            retry_after_seconds,
        } = &self
        {
            if let Ok(value) = header::HeaderValue::from_str(&retry_after_seconds.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }

        response
    }
}
