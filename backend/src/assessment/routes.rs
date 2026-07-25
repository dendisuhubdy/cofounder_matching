use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::app::AppState;
use crate::assessment::questions::QUESTIONS;
use crate::assessment::repo::Response;
use crate::assessment::service::{self, ResponsesView};
use crate::auth::extractor::CurrentUser;
use crate::error::ApiResult;

/// The client-facing shape of a question. Note the absence of `reverse`.
#[derive(serde::Serialize)]
struct PublicQuestion {
    id: &'static str,
    text: &'static str,
    axis: &'static str,
}

#[derive(serde::Serialize)]
struct ScalePoint {
    value: i16,
    label: &'static str,
}

#[derive(serde::Serialize)]
struct QuestionnaireView {
    questions: Vec<PublicQuestion>,
    scale: Vec<ScalePoint>,
}

#[derive(serde::Deserialize)]
pub struct ResponsesRequest {
    pub responses: Vec<Response>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/questions", get(questionnaire))
        .route("/me/responses", get(my_responses).put(submit_responses))
}

async fn questionnaire(CurrentUser(_user): CurrentUser) -> Json<QuestionnaireView> {
    // The labels ship with the questions so the wording of the scale lives in
    // one place rather than being retyped in the frontend.
    let scale = vec![
        ScalePoint {
            value: 1,
            label: "Strongly disagree",
        },
        ScalePoint {
            value: 2,
            label: "Disagree",
        },
        ScalePoint {
            value: 3,
            label: "Neutral",
        },
        ScalePoint {
            value: 4,
            label: "Agree",
        },
        ScalePoint {
            value: 5,
            label: "Strongly agree",
        },
    ];

    Json(QuestionnaireView {
        questions: QUESTIONS
            .iter()
            .map(|question| PublicQuestion {
                id: question.id,
                text: question.text,
                axis: question.axis.slug(),
            })
            .collect(),
        scale,
    })
}

async fn my_responses(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<ResponsesView>> {
    Ok(Json(service::view(&state, user.id).await?))
}

async fn submit_responses(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(payload): Json<ResponsesRequest>,
) -> ApiResult<Json<ResponsesView>> {
    Ok(Json(
        service::record(&state, user.id, payload.responses).await?,
    ))
}
