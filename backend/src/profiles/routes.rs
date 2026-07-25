use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::app::AppState;
use crate::auth::extractor::CurrentUser;
use crate::error::ApiResult;
use crate::profiles::repo::ProfileInput;
use crate::profiles::service::{self, ProfileView};
use crate::profiles::vocab::{self, Choice};

#[derive(serde::Serialize)]
struct OptionsView {
    roles: &'static [Choice],
    idea_statuses: &'static [Choice],
    stages: &'static [Choice],
    commitments: &'static [Choice],
    interests: &'static [Choice],
    report_reasons: &'static [Choice],
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/options", get(options))
        .route("/me/profile", get(my_profile).put(update_profile))
}

/// Not in the spec's API table, but the form cannot be built without it: the
/// alternative is duplicating five vocabularies in TypeScript and waiting for
/// them to drift out of step with the CHECK constraints.
async fn options(CurrentUser(_user): CurrentUser) -> Json<OptionsView> {
    Json(OptionsView {
        roles: &vocab::ROLES,
        idea_statuses: &vocab::IDEA_STATUSES,
        stages: &vocab::STAGES,
        commitments: &vocab::COMMITMENTS,
        interests: &vocab::INTERESTS,
        report_reasons: &crate::moderation::vocab::REPORT_REASONS,
    })
}

async fn my_profile(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<ProfileView>> {
    Ok(Json(service::view(&state, user.id).await?))
}

async fn update_profile(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<ProfileInput>,
) -> ApiResult<Json<ProfileView>> {
    Ok(Json(service::update(&state, user.id, input).await?))
}
