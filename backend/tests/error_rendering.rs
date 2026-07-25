use axum::response::IntoResponse;
use cofounder_api::error::{ApiError, FieldError};
use http_body_util::BodyExt;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn not_found_renders_404_problem_json() {
    let response = ApiError::NotFound.into_response();

    assert_eq!(response.status(), 404);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/problem+json"
    );

    let body = body_json(response).await;
    assert_eq!(body["type"], "not_found");
}

#[tokio::test]
async fn validation_renders_422_with_field_details() {
    let response = ApiError::Validation(vec![FieldError {
        field: "email".into(),
        message: "must be a valid email address".into(),
    }])
    .into_response();

    assert_eq!(response.status(), 422);

    let body = body_json(response).await;
    assert_eq!(body["type"], "validation_failed");
    assert_eq!(body["errors"][0]["field"], "email");
    assert_eq!(body["errors"][0]["message"], "must be a valid email address");
}

#[tokio::test]
async fn rate_limited_sets_retry_after_header_and_field() {
    let response = ApiError::RateLimited {
        retry_after_seconds: 60,
    }
    .into_response();

    assert_eq!(response.status(), 429);
    assert_eq!(response.headers().get("retry-after").unwrap(), "60");

    let body = body_json(response).await;
    assert_eq!(body["retry_after"], 60);
}

#[tokio::test]
async fn internal_error_body_does_not_leak_the_cause() {
    let response =
        ApiError::Internal(anyhow::anyhow!("connection string was postgres://secret")).into_response();

    assert_eq!(response.status(), 500);

    let body = body_json(response).await;
    let serialized = body.to_string();
    assert!(!serialized.contains("secret"));
    assert_eq!(body["type"], "internal_error");
}

#[tokio::test]
async fn unauthorized_renders_401() {
    let response = ApiError::Unauthorized.into_response();

    assert_eq!(response.status(), 401);
    assert_eq!(body_json(response).await["type"], "unauthorized");
}

#[tokio::test]
async fn invalid_token_is_indistinguishable_regardless_of_reason() {
    // Expired, consumed, and unknown tokens all map to this single variant, so
    // the response cannot be used to probe for registered addresses.
    let response = ApiError::InvalidToken.into_response();

    assert_eq!(response.status(), 400);

    let body = body_json(response).await;
    assert_eq!(body["type"], "invalid_token");
    assert_eq!(body["title"], "this login link is no longer valid");
}
