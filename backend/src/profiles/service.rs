use std::collections::HashSet;

use uuid::Uuid;

use crate::app::AppState;
use crate::assessment::repo as assessment_repo;
use crate::assessment::service::TOTAL_QUESTIONS;
use crate::error::{ApiError, ApiResult, FieldError};
use crate::profiles::repo::{self, ProfileInput, ProfileRow};
use crate::profiles::timezone;
use crate::profiles::vocab::{self, Choice};

const MAX_DISPLAY_NAME: usize = 80;
const MAX_HEADLINE: usize = 140;
const MAX_BIO: usize = 2000;
const MAX_PLACE: usize = 80;
const MAX_TIMEZONE: usize = 64;
const MAX_INTERESTS: usize = 10;

/// What the client sees. `ProfileRow` plus the interests, which live in their
/// own table but are edited as part of the same document.
#[derive(Debug, serde::Serialize)]
pub struct ProfileBody {
    #[serde(flatten)]
    pub profile: ProfileRow,
    pub interests: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ProfileView {
    pub profile: ProfileBody,
    pub complete: bool,
    pub missing: Vec<String>,
}

fn empty_row() -> ProfileRow {
    ProfileRow {
        display_name: String::new(),
        headline: String::new(),
        bio: String::new(),
        city: String::new(),
        country: String::new(),
        timezone: String::new(),
        utc_offset_minutes: None,
        linkedin_url: None,
        github_url: None,
        website_url: None,
        roles: Vec::new(),
        seeking_roles: Vec::new(),
        idea_status: None,
        stage: None,
        commitment: None,
    }
}

fn check_length(errors: &mut Vec<FieldError>, field: &str, value: &str, max: usize) {
    if value.chars().count() > max {
        errors.push(FieldError {
            field: field.into(),
            message: format!("must be {max} characters or fewer"),
        });
    }
}

fn check_choice(
    errors: &mut Vec<FieldError>,
    field: &str,
    value: &Option<String>,
    choices: &[Choice],
) {
    if let Some(id) = value {
        if !vocab::contains(choices, id) {
            errors.push(FieldError {
                field: field.into(),
                message: "is not one of the available options".into(),
            });
        }
    }
}

fn check_tags(
    errors: &mut Vec<FieldError>,
    field: &str,
    values: &[String],
    choices: &[Choice],
    max: usize,
) {
    if values.len() > max {
        errors.push(FieldError {
            field: field.into(),
            message: format!("may hold at most {max} selections"),
        });
        return;
    }

    let mut seen: HashSet<&str> = HashSet::new();
    for value in values {
        if !vocab::contains(choices, value) {
            errors.push(FieldError {
                field: field.into(),
                message: format!("contains an unknown option: {value}"),
            });
            return;
        }
        if !seen.insert(value) {
            errors.push(FieldError {
                field: field.into(),
                message: format!("lists {value} more than once"),
            });
            return;
        }
    }
}

/// A blank link means "not set", not "invalid". Anything else must be an
/// ordinary web address: rendering an attacker-supplied `javascript:` URL as
/// an anchor is a stored XSS.
fn normalize_link(errors: &mut Vec<FieldError>, field: &str, value: &mut Option<String>) {
    let trimmed = value.as_deref().unwrap_or("").trim().to_string();

    if trimmed.is_empty() {
        *value = None;
        return;
    }

    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        errors.push(FieldError {
            field: field.into(),
            message: "must start with http:// or https://".into(),
        });
    }

    if trimmed.chars().count() > 300 {
        errors.push(FieldError {
            field: field.into(),
            message: "must be 300 characters or fewer".into(),
        });
    }

    *value = Some(trimmed);
}

fn normalize_and_validate(input: &mut ProfileInput) -> ApiResult<()> {
    input.display_name = input.display_name.trim().to_string();
    input.headline = input.headline.trim().to_string();
    input.bio = input.bio.trim().to_string();
    input.city = input.city.trim().to_string();
    input.country = input.country.trim().to_string();
    input.timezone = input.timezone.trim().to_string();

    let mut errors = Vec::new();

    check_length(
        &mut errors,
        "display_name",
        &input.display_name,
        MAX_DISPLAY_NAME,
    );
    check_length(&mut errors, "headline", &input.headline, MAX_HEADLINE);
    check_length(&mut errors, "bio", &input.bio, MAX_BIO);
    check_length(&mut errors, "city", &input.city, MAX_PLACE);
    check_length(&mut errors, "country", &input.country, MAX_PLACE);
    check_length(&mut errors, "timezone", &input.timezone, MAX_TIMEZONE);

    // A named zone that cannot be resolved is a validation failure rather
    // than a silent null: the user typed something, and geography scoring
    // would quietly ignore it.
    if input.timezone.is_empty() {
        input.utc_offset_minutes = None;
    } else {
        match timezone::utc_offset_minutes(&input.timezone) {
            Some(offset) => input.utc_offset_minutes = Some(offset),
            None => {
                input.utc_offset_minutes = None;
                errors.push(FieldError {
                    field: "timezone".into(),
                    message: "is not a known timezone, for example Europe/London".into(),
                });
            }
        }
    }

    check_tags(
        &mut errors,
        "roles",
        &input.roles,
        &vocab::ROLES,
        vocab::ROLES.len(),
    );
    check_tags(
        &mut errors,
        "seeking_roles",
        &input.seeking_roles,
        &vocab::ROLES,
        vocab::ROLES.len(),
    );
    check_tags(
        &mut errors,
        "interests",
        &input.interests,
        &vocab::INTERESTS,
        MAX_INTERESTS,
    );

    check_choice(
        &mut errors,
        "idea_status",
        &input.idea_status,
        &vocab::IDEA_STATUSES,
    );
    check_choice(&mut errors, "stage", &input.stage, &vocab::STAGES);
    check_choice(
        &mut errors,
        "commitment",
        &input.commitment,
        &vocab::COMMITMENTS,
    );

    normalize_link(&mut errors, "linkedin_url", &mut input.linkedin_url);
    normalize_link(&mut errors, "github_url", &mut input.github_url);
    normalize_link(&mut errors, "website_url", &mut input.website_url);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::Validation(errors))
    }
}

/// The spec's definition, in order: bio, at least one role, at least one
/// sought role, a commitment level, and all 18 answers. Incomplete profiles
/// never enter a deck and cannot message — this is the primary spam filter.
fn missing_requirements(profile: &ProfileRow, answered: i64) -> Vec<String> {
    let mut missing = Vec::new();

    if profile.bio.trim().is_empty() {
        missing.push("bio".to_string());
    }
    if profile.roles.is_empty() {
        missing.push("roles".to_string());
    }
    if profile.seeking_roles.is_empty() {
        missing.push("seeking_roles".to_string());
    }
    if profile.commitment.is_none() {
        missing.push("commitment".to_string());
    }
    if answered < TOTAL_QUESTIONS as i64 {
        missing.push("responses".to_string());
    }

    missing
}

async fn build_view(
    state: &AppState,
    user_id: Uuid,
    profile: ProfileRow,
    interests: Vec<String>,
) -> ApiResult<ProfileView> {
    let answered = assessment_repo::answered_count(&state.db, user_id).await?;
    let missing = missing_requirements(&profile, answered);

    Ok(ProfileView {
        profile: ProfileBody { profile, interests },
        complete: missing.is_empty(),
        missing,
    })
}

/// A user who has never saved gets a blank profile rather than a 404: the
/// form needs something to render, and a first visit is not an error.
pub async fn view(state: &AppState, user_id: Uuid) -> ApiResult<ProfileView> {
    let profile = repo::find_by_user_id(&state.db, user_id)
        .await?
        .unwrap_or_else(empty_row);
    let interests = repo::interests_for(&state.db, user_id).await?;

    build_view(state, user_id, profile, interests).await
}

pub async fn update(
    state: &AppState,
    user_id: Uuid,
    mut input: ProfileInput,
) -> ApiResult<ProfileView> {
    normalize_and_validate(&mut input)?;

    let (profile, interests) = repo::save(&state.db, user_id, &input).await?;

    build_view(state, user_id, profile, interests).await
}
