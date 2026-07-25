use std::collections::HashSet;

use uuid::Uuid;

use crate::app::AppState;
use crate::assessment::questions::{self, QUESTIONS};
use crate::assessment::repo::{self, Response};
use crate::assessment::scoring;
use crate::error::{ApiError, ApiResult, FieldError};

pub const TOTAL_QUESTIONS: usize = QUESTIONS.len();

#[derive(Debug, serde::Serialize)]
pub struct ResponsesView {
    pub responses: Vec<Response>,
    pub answered: usize,
    pub total: usize,
    pub complete: bool,
}

fn validate(submitted: &[Response]) -> ApiResult<()> {
    let mut errors = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();

    for response in submitted {
        if questions::find(&response.question_id).is_none() {
            errors.push(FieldError {
                field: response.question_id.clone(),
                message: "is not a question in this assessment".into(),
            });
            continue;
        }

        if !(1..=5).contains(&response.value) {
            errors.push(FieldError {
                field: response.question_id.clone(),
                message: "must be between 1 and 5".into(),
            });
        }

        if !seen.insert(&response.question_id) {
            errors.push(FieldError {
                field: response.question_id.clone(),
                message: "was answered twice in one submission".into(),
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::Validation(errors))
    }
}

pub async fn view(state: &AppState, user_id: Uuid) -> ApiResult<ResponsesView> {
    let responses = repo::responses_for(&state.db, user_id).await?;
    let answered = responses.len();

    Ok(ResponsesView {
        responses,
        answered,
        total: TOTAL_QUESTIONS,
        complete: answered == TOTAL_QUESTIONS,
    })
}

/// Saves a partial or complete batch of answers, then brings `trait_scores`
/// back in step. The scores table is derived state: it is written when the
/// assessment is complete and removed the moment it stops being complete, so
/// its presence is always a reliable signal.
pub async fn record(
    state: &AppState,
    user_id: Uuid,
    submitted: Vec<Response>,
) -> ApiResult<ResponsesView> {
    validate(&submitted)?;

    repo::upsert_responses(&state.db, user_id, &submitted).await?;

    let answers = repo::answers_map(&state.db, user_id).await?;
    match scoring::compute(&answers) {
        Some(scores) => repo::save_trait_scores(&state.db, user_id, &scores).await?,
        None => repo::delete_trait_scores(&state.db, user_id).await?,
    }

    view(state, user_id).await
}
