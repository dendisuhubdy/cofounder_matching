# Profile & Assessment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A signed-in founder can fill in their profile, answer the 18-question work-style assessment one click at a time, and see their profile go from incomplete to complete.

**Architecture:** Two new Rust modules — `profiles` and `assessment` — each following the established `vocab/questions → repo → service → routes` layering. The question bank and the trait scorer are pure Rust with no I/O, so they are exhaustively unit-testable. `trait_scores` is a derived table maintained by the backend: its row exists if and only if all 18 answers exist, which is the invariant slice 3's deck will join against. The frontend adds two client-rendered pages that call the API through the existing `/api` rewrite.

**Tech Stack:** Rust (axum 0.8, sqlx 0.8, tokio), Postgres 16, Next.js 16.2 (App Router, TypeScript, Tailwind 4), Playwright.

This plan is slice 2 of 4 derived from `docs/superpowers/specs/2026-07-25-cofounder-matching-design.md`. Slice 1 (foundation & auth) is merged. Slice 3 adds the scorer and deck; slice 4 adds messaging.

## Global Constraints

These carry over unchanged from slice 1 and apply to every task below.

- Rust edition 2021. Crate name `cofounder_api`, living in `backend/`.
- **Use `sqlx::query_as` / `sqlx::query` / `sqlx::query_scalar`, never the `query!` macros.** The macros need a live database at compile time; the runtime-checked functions do not. This keeps `cargo build` working without a database.
- All database access lives in `repo.rs` files. Handlers and services never write SQL.
- Timestamps are `TIMESTAMPTZ` in Postgres and `chrono::DateTime<chrono::Utc>` in Rust.
- Every route except `/auth/*` requires a session, obtained by taking `CurrentUser` as a handler argument.
- Errors are returned as `ApiError`; validation failures are `ApiError::Validation(Vec<FieldError>)`, which renders 422 with a per-field body. Never construct ad-hoc error responses.
- Frontend lives in `frontend/`. It never holds database credentials and never calls the backend on any path other than `/api/*`.
- Commit after every task. Conventional-commit prefixes (`feat:`, `test:`, `chore:`).

Constraints specific to this slice:

- **No photo.** The spec lists a profile photo under Identity; it is deliberately out of scope for this slice and no `photo_url` column is created. Real uploads need object storage, which is its own sub-project. Deck cards in slice 3 will show initials.
- **No Server Actions.** Slice 1 established that Next holds no domain logic: pages are client components calling the Rust API through the `/api` rewrite with `apiFetch`. A Server Action would add a second server hop and a second place to duplicate validation. Keep the existing pattern.
- **Before writing any frontend code, read the relevant guide under `frontend/node_modules/next/dist/docs/`,** as `frontend/AGENTS.md` requires. This Next.js version has breaking changes relative to training data.
- Fixed vocabularies (roles, stages, commitments, interest tags) live in Rust and are served to the frontend by `GET /options`. The frontend never hardcodes them — that is how the form and the database `CHECK` constraints stay in agreement.
- The assessment autosaves. `PUT /me/responses` accepts a **partial** list and upserts, so a half-finished assessment survives a closed tab.
- `trait_scores` holds a row for a user **iff** that user has answered all 18 questions. Writing a partial row would let slice 3 compare an axis mean over one item against one over three.
- `GET /questions` **must not expose the `reverse` flag.** It is what stops a user from answering uniformly to manufacture a flattering profile.

## File Structure

```
backend/
  migrations/
    0004_question_responses.sql   one row per (user, question)
    0005_trait_scores.sql         derived axis scores, present only when complete
    0006_profiles.sql             one row per user
    0007_profile_interests.sql    user → industry tag
  src/
    lib.rs                        + pub mod assessment; pub mod profiles;
    app.rs                        + merge the two new routers
    assessment/
      mod.rs
      questions.rs                Axis, Question, the 18-question constant (pure)
      scoring.rs                  responses → TraitScores (pure)
      repo.rs                     question_responses and trait_scores persistence
      service.rs                  validate, upsert, recompute
      routes.rs                   GET /questions, GET+PUT /me/responses
    profiles/
      mod.rs
      vocab.rs                    roles, stages, commitments, interest tags (pure)
      repo.rs                     profiles and profile_interests persistence
      service.rs                  normalize, validate, completeness
      routes.rs                   GET /options, GET+PUT /me/profile
  tests/
    questions.rs                  question-bank invariants
    trait_scoring.rs              axis maths, reverse items, boundaries
    assessment_repo.rs            response upsert, trait-score lifecycle
    assessment_api.rs             /questions and /me/responses over the router
    profiles_repo.rs              profile upsert, interest replacement
    profile_api.rs                /options and /me/profile, completeness

frontend/
  lib/profile.ts                  shared TypeScript types for both pages
  app/(app)/layout.tsx            + navigation links
  app/(app)/home/page.tsx         completeness checklist
  app/(app)/profile/page.tsx      server shell
  app/(app)/profile/profile-form.tsx    client form
  app/(app)/assessment/page.tsx   server shell
  app/(app)/assessment/assessment-client.tsx  client autosaving form
  e2e/helpers.ts                  signIn() used by new specs
  e2e/profile.spec.ts             profile + assessment journey
```

`questions.rs` and `scoring.rs` have no database access and no clock, so the largest body of tests in this slice needs no Postgres. `repo.rs` owns SQL, `service.rs` owns workflow and validation, `routes.rs` owns only HTTP shape.

---

### Task 1: The 18-question bank

**Files:**
- Create: `backend/src/assessment/mod.rs`, `backend/src/assessment/questions.rs`
- Modify: `backend/src/lib.rs`
- Test: `backend/tests/questions.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `cofounder_api::assessment::questions::{Axis, Question, QUESTIONS, find}`
  - `Axis` — a `Copy` enum with variants `RiskTolerance`, `PaceVsRigor`, `ConflictStyle`, `DecisionBasis`, `WorkMode`, `Orientation`; `Axis::ALL: [Axis; 6]`; `Axis::slug(self) -> &'static str`
  - `Question { id: &'static str, text: &'static str, axis: Axis, reverse: bool }`
  - `QUESTIONS: [Question; 18]`
  - `find(id: &str) -> Option<&'static Question>`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/questions.rs`:

```rust
use std::collections::HashSet;

use cofounder_api::assessment::questions::{find, Axis, QUESTIONS};

#[test]
fn there_are_eighteen_questions() {
    assert_eq!(QUESTIONS.len(), 18);
}

#[test]
fn every_axis_has_exactly_three_questions() {
    for axis in Axis::ALL {
        let count = QUESTIONS.iter().filter(|q| q.axis == axis).count();
        assert_eq!(count, 3, "axis {} has {} questions", axis.slug(), count);
    }
}

#[test]
fn every_axis_has_at_least_one_reverse_item() {
    // Without a reverse item, answering straight down the page produces a
    // coherent-looking profile that means nothing.
    for axis in Axis::ALL {
        let reversed = QUESTIONS
            .iter()
            .filter(|q| q.axis == axis && q.reverse)
            .count();
        assert!(reversed >= 1, "axis {} has no reverse item", axis.slug());
    }
}

#[test]
fn question_ids_are_unique() {
    let unique: HashSet<&str> = QUESTIONS.iter().map(|q| q.id).collect();
    assert_eq!(unique.len(), QUESTIONS.len());
}

#[test]
fn every_question_has_text() {
    for question in QUESTIONS.iter() {
        assert!(!question.text.trim().is_empty(), "{} has no text", question.id);
    }
}

#[test]
fn axis_slugs_are_unique_and_snake_case() {
    let unique: HashSet<&str> = Axis::ALL.iter().map(|a| a.slug()).collect();
    assert_eq!(unique.len(), 6);
    for axis in Axis::ALL {
        assert!(axis
            .slug()
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_'));
    }
}

#[test]
fn find_locates_a_question_by_id() {
    let question = find("risk_1").expect("risk_1 should exist");
    assert_eq!(question.axis, Axis::RiskTolerance);
}

#[test]
fn find_rejects_an_unknown_id() {
    assert!(find("not_a_question").is_none());
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test questions`
Expected: FAIL — `unresolved import` / `could not find assessment in cofounder_api`.

- [ ] **Step 3: Declare the module**

Create `backend/src/assessment/mod.rs`:

```rust
pub mod questions;
```

Modify `backend/src/lib.rs` — add `pub mod assessment;` so the list reads, alphabetically:

```rust
pub mod app;
pub mod assessment;
pub mod auth;
pub mod config;
pub mod db;
pub mod email;
pub mod error;
pub mod users;
```

- [ ] **Step 4: Write the question bank**

Create `backend/src/assessment/questions.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    RiskTolerance,
    PaceVsRigor,
    ConflictStyle,
    DecisionBasis,
    WorkMode,
    Orientation,
}

impl Axis {
    pub const ALL: [Axis; 6] = [
        Axis::RiskTolerance,
        Axis::PaceVsRigor,
        Axis::ConflictStyle,
        Axis::DecisionBasis,
        Axis::WorkMode,
        Axis::Orientation,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Axis::RiskTolerance => "risk_tolerance",
            Axis::PaceVsRigor => "pace_vs_rigor",
            Axis::ConflictStyle => "conflict_style",
            Axis::DecisionBasis => "decision_basis",
            Axis::WorkMode => "work_mode",
            Axis::Orientation => "orientation",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Question {
    pub id: &'static str,
    pub text: &'static str,
    pub axis: Axis,
    /// A high agreement on this item means a *low* axis score. Never sent to
    /// the client: knowing which items are flipped is enough to fake a
    /// coherent profile.
    pub reverse: bool,
}

/// The instrument itself. Versioned with the code rather than stored in the
/// database, so a change to the wording is a reviewable diff and the scoring
/// tests move with it.
pub const QUESTIONS: [Question; 18] = [
    // risk_tolerance — low: de-risk before committing, high: bet big early
    Question {
        id: "risk_1",
        text: "I would rather launch an unproven idea than spend months validating it.",
        axis: Axis::RiskTolerance,
        reverse: false,
    },
    Question {
        id: "risk_2",
        text: "I need strong evidence that a market exists before I commit to building for it.",
        axis: Axis::RiskTolerance,
        reverse: true,
    },
    Question {
        id: "risk_3",
        text: "A big, uncertain outcome appeals to me more than a safe, modest one.",
        axis: Axis::RiskTolerance,
        reverse: false,
    },
    // pace_vs_rigor — low: build it right, high: ship it now
    Question {
        id: "pace_1",
        text: "Shipping something rough this week beats shipping something polished next month.",
        axis: Axis::PaceVsRigor,
        reverse: false,
    },
    Question {
        id: "pace_2",
        text: "I would hold a release back to fix problems most users would never notice.",
        axis: Axis::PaceVsRigor,
        reverse: true,
    },
    Question {
        id: "pace_3",
        text: "I am comfortable taking on technical debt to hit a deadline.",
        axis: Axis::PaceVsRigor,
        reverse: false,
    },
    // conflict_style — low: seek harmony, high: address directly
    Question {
        id: "conflict_1",
        text: "When a teammate's work falls short, I tell them plainly and quickly.",
        axis: Axis::ConflictStyle,
        reverse: false,
    },
    Question {
        id: "conflict_2",
        text: "I would rather let a small disagreement pass than risk souring the mood.",
        axis: Axis::ConflictStyle,
        reverse: true,
    },
    Question {
        id: "conflict_3",
        text: "I raise disagreements in the room rather than working around them afterwards.",
        axis: Axis::ConflictStyle,
        reverse: false,
    },
    // decision_basis — low: trust intuition, high: require data
    Question {
        id: "decision_1",
        text: "I want to see the numbers before I make a call.",
        axis: Axis::DecisionBasis,
        reverse: false,
    },
    Question {
        id: "decision_2",
        text: "When the evidence is ambiguous, I trust my instinct and move.",
        axis: Axis::DecisionBasis,
        reverse: true,
    },
    Question {
        id: "decision_3",
        text: "A decision with no measurable evidence behind it makes me uncomfortable.",
        axis: Axis::DecisionBasis,
        reverse: false,
    },
    // work_mode — low: deep solo work, high: constant collaboration
    Question {
        id: "work_1",
        text: "I do my best work in long stretches of uninterrupted solo time.",
        axis: Axis::WorkMode,
        reverse: true,
    },
    Question {
        id: "work_2",
        text: "I would rather think a problem through out loud with someone than alone.",
        axis: Axis::WorkMode,
        reverse: false,
    },
    Question {
        id: "work_3",
        text: "A day full of conversations energizes me more than it drains me.",
        axis: Axis::WorkMode,
        reverse: false,
    },
    // orientation — low: near-term execution, high: long-range vision
    Question {
        id: "orientation_1",
        text: "I spend more time on where we will be in five years than on what ships this week.",
        axis: Axis::Orientation,
        reverse: false,
    },
    Question {
        id: "orientation_2",
        text: "I would rather hit this quarter's targets than debate the ten-year plan.",
        axis: Axis::Orientation,
        reverse: true,
    },
    Question {
        id: "orientation_3",
        text: "Painting a compelling long-term picture is where I add the most value.",
        axis: Axis::Orientation,
        reverse: false,
    },
];

pub fn find(id: &str) -> Option<&'static Question> {
    QUESTIONS.iter().find(|question| question.id == id)
}
```

- [ ] **Step 5: Run the tests and verify they pass**

Run: `cd backend && cargo test --test questions`
Expected: PASS — 8 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assessment backend/src/lib.rs backend/tests/questions.rs
git commit -m "feat: the eighteen-question work-style instrument"
```

---

### Task 2: Trait scoring

**Files:**
- Create: `backend/src/assessment/scoring.rs`
- Modify: `backend/src/assessment/mod.rs`
- Test: `backend/tests/trait_scoring.rs`

**Interfaces:**
- Consumes: `assessment::questions::{Axis, QUESTIONS}`
- Produces: `cofounder_api::assessment::scoring::{TraitScores, compute}`
  - `TraitScores` — `Copy` struct with six `i16` fields named for the axis slugs: `risk_tolerance`, `pace_vs_rigor`, `conflict_style`, `decision_basis`, `work_mode`, `orientation`. Derives `sqlx::FromRow` and `serde::Serialize`.
  - `compute(answers: &HashMap<String, i16>) -> Option<TraitScores>` — `None` unless all 18 questions are answered.

- [ ] **Step 1: Write the failing test**

Create `backend/tests/trait_scoring.rs`:

```rust
use std::collections::HashMap;

use cofounder_api::assessment::questions::QUESTIONS;
use cofounder_api::assessment::scoring::compute;

/// Every question answered with the same value.
fn uniform(value: i16) -> HashMap<String, i16> {
    QUESTIONS
        .iter()
        .map(|q| (q.id.to_string(), value))
        .collect()
}

#[test]
fn the_midpoint_answer_scores_fifty_on_every_axis() {
    let scores = compute(&uniform(3)).expect("all 18 answered");

    assert_eq!(scores.risk_tolerance, 50);
    assert_eq!(scores.pace_vs_rigor, 50);
    assert_eq!(scores.conflict_style, 50);
    assert_eq!(scores.decision_basis, 50);
    assert_eq!(scores.work_mode, 50);
    assert_eq!(scores.orientation, 50);
}

#[test]
fn answering_uniformly_does_not_produce_an_extreme_profile() {
    // This is the whole point of reverse items. Agreeing with everything must
    // not read as "maximum on every axis".
    let high = compute(&uniform(5)).expect("all 18 answered");

    assert_ne!(high.risk_tolerance, 100);
    assert_ne!(high.pace_vs_rigor, 100);
    assert_ne!(high.work_mode, 100);
}

#[test]
fn a_reverse_item_is_flipped_before_averaging() {
    // risk_2 is the reverse item on the risk axis. Answering 5 to the two
    // forward items and 1 to the reverse one is maximal risk tolerance.
    let mut answers = uniform(3);
    answers.insert("risk_1".into(), 5);
    answers.insert("risk_2".into(), 1);
    answers.insert("risk_3".into(), 5);

    let scores = compute(&answers).expect("all 18 answered");
    assert_eq!(scores.risk_tolerance, 100);
}

#[test]
fn the_opposite_answers_score_zero() {
    let mut answers = uniform(3);
    answers.insert("risk_1".into(), 1);
    answers.insert("risk_2".into(), 5);
    answers.insert("risk_3".into(), 1);

    let scores = compute(&answers).expect("all 18 answered");
    assert_eq!(scores.risk_tolerance, 0);
}

#[test]
fn an_axis_mean_is_mapped_onto_zero_to_one_hundred() {
    // work_1 is the reverse item. 4, 2 (-> 4), 5 gives a mean of 13/3 = 4.333,
    // which maps to (4.333 - 1) / 4 * 100 = 83.33, rounded to 83.
    let mut answers = uniform(3);
    answers.insert("work_1".into(), 2);
    answers.insert("work_2".into(), 4);
    answers.insert("work_3".into(), 5);

    let scores = compute(&answers).expect("all 18 answered");
    assert_eq!(scores.work_mode, 83);
}

#[test]
fn scoring_returns_none_when_any_answer_is_missing() {
    let mut answers = uniform(3);
    answers.remove("orientation_3");

    assert!(compute(&answers).is_none());
}

#[test]
fn scoring_returns_none_for_no_answers_at_all() {
    assert!(compute(&HashMap::new()).is_none());
}

#[test]
fn unknown_answer_keys_are_ignored() {
    let mut answers = uniform(3);
    answers.insert("a_question_we_deleted".into(), 5);

    let scores = compute(&answers).expect("all 18 real questions answered");
    assert_eq!(scores.risk_tolerance, 50);
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test trait_scoring`
Expected: FAIL — `could not find scoring in assessment`.

- [ ] **Step 3: Implement the scorer**

Create `backend/src/assessment/scoring.rs`:

```rust
use std::collections::HashMap;

use crate::assessment::questions::{Axis, QUESTIONS};

/// One 0–100 score per axis. Derives `FromRow` so `trait_scores` can be read
/// straight back into it; that is a column mapping, not I/O, and `compute`
/// below stays a pure function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct TraitScores {
    pub risk_tolerance: i16,
    pub pace_vs_rigor: i16,
    pub conflict_style: i16,
    pub decision_basis: i16,
    pub work_mode: i16,
    pub orientation: i16,
}

fn axis_score(answers: &HashMap<String, i16>, axis: Axis) -> Option<i16> {
    let mut sum = 0.0_f64;
    let mut count = 0.0_f64;

    for question in QUESTIONS.iter().filter(|q| q.axis == axis) {
        let raw = *answers.get(question.id)?;
        // A 1–5 Likert value flips around its midpoint: 1 becomes 5, 5 becomes 1.
        let oriented = if question.reverse { 6 - raw } else { raw };
        sum += f64::from(oriented);
        count += 1.0;
    }

    let mean = sum / count;
    Some((((mean - 1.0) / 4.0) * 100.0).round() as i16)
}

/// `None` unless every one of the 18 questions has an answer. An axis mean
/// over one item is not comparable with one over three, and a partial score
/// would quietly skew every match the deck produces.
pub fn compute(answers: &HashMap<String, i16>) -> Option<TraitScores> {
    Some(TraitScores {
        risk_tolerance: axis_score(answers, Axis::RiskTolerance)?,
        pace_vs_rigor: axis_score(answers, Axis::PaceVsRigor)?,
        conflict_style: axis_score(answers, Axis::ConflictStyle)?,
        decision_basis: axis_score(answers, Axis::DecisionBasis)?,
        work_mode: axis_score(answers, Axis::WorkMode)?,
        orientation: axis_score(answers, Axis::Orientation)?,
    })
}
```

Modify `backend/src/assessment/mod.rs`:

```rust
pub mod questions;
pub mod scoring;
```

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cd backend && cargo test --test trait_scoring`
Expected: PASS — 8 tests.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assessment backend/tests/trait_scoring.rs
git commit -m "feat: axis scoring with reverse-item handling"
```

---

### Task 3: Assessment persistence

**Files:**
- Create: `backend/migrations/0004_question_responses.sql`, `backend/migrations/0005_trait_scores.sql`, `backend/src/assessment/repo.rs`
- Modify: `backend/src/assessment/mod.rs`
- Test: `backend/tests/assessment_repo.rs`

**Interfaces:**
- Consumes: `assessment::scoring::TraitScores`, `users::repo::find_or_create_by_email`
- Produces: `cofounder_api::assessment::repo::{Response, responses_for, answers_map, upsert_responses, answered_count, save_trait_scores, delete_trait_scores, trait_scores_for}`
  - `Response { question_id: String, value: i16 }`
  - `responses_for(&PgPool, Uuid) -> sqlx::Result<Vec<Response>>`
  - `answers_map(&PgPool, Uuid) -> sqlx::Result<HashMap<String, i16>>`
  - `upsert_responses(&PgPool, Uuid, &[Response]) -> sqlx::Result<()>`
  - `answered_count(&PgPool, Uuid) -> sqlx::Result<i64>`
  - `save_trait_scores(&PgPool, Uuid, &TraitScores) -> sqlx::Result<()>`
  - `delete_trait_scores(&PgPool, Uuid) -> sqlx::Result<()>`
  - `trait_scores_for(&PgPool, Uuid) -> sqlx::Result<Option<TraitScores>>`

- [ ] **Step 1: Write the migrations**

Create `backend/migrations/0004_question_responses.sql`:

```sql
CREATE TABLE question_responses (
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    question_id TEXT NOT NULL,
    value       SMALLINT NOT NULL CHECK (value BETWEEN 1 AND 5),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, question_id)
);
```

`question_id` carries no foreign key: the question bank lives in Rust, not in a
table. The set of valid ids is enforced in `assessment::service`.

Create `backend/migrations/0005_trait_scores.sql`:

```sql
-- Derived from question_responses. A row exists only when all 18 questions
-- have been answered, so slice 3's deck can join here and be certain every
-- axis is comparable.
CREATE TABLE trait_scores (
    user_id        UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    risk_tolerance SMALLINT NOT NULL CHECK (risk_tolerance BETWEEN 0 AND 100),
    pace_vs_rigor  SMALLINT NOT NULL CHECK (pace_vs_rigor BETWEEN 0 AND 100),
    conflict_style SMALLINT NOT NULL CHECK (conflict_style BETWEEN 0 AND 100),
    decision_basis SMALLINT NOT NULL CHECK (decision_basis BETWEEN 0 AND 100),
    work_mode      SMALLINT NOT NULL CHECK (work_mode BETWEEN 0 AND 100),
    orientation    SMALLINT NOT NULL CHECK (orientation BETWEEN 0 AND 100),
    computed_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

- [ ] **Step 2: Write the failing test**

Create `backend/tests/assessment_repo.rs`:

```rust
use cofounder_api::assessment::repo;
use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::users;
use sqlx::PgPool;
use uuid::Uuid;

async fn a_user(pool: &PgPool, email: &str) -> Uuid {
    users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id
}

fn response(question_id: &str, value: i16) -> repo::Response {
    repo::Response {
        question_id: question_id.to_string(),
        value,
    }
}

#[sqlx::test]
async fn responses_start_empty(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    let responses = repo::responses_for(&pool, user_id).await.unwrap();
    assert!(responses.is_empty());
    assert_eq!(repo::answered_count(&pool, user_id).await.unwrap(), 0);
}

#[sqlx::test]
async fn responses_are_saved_and_read_back(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    repo::upsert_responses(&pool, user_id, &[response("risk_1", 4), response("pace_1", 2)])
        .await
        .unwrap();

    let answers = repo::answers_map(&pool, user_id).await.unwrap();
    assert_eq!(answers.get("risk_1"), Some(&4));
    assert_eq!(answers.get("pace_1"), Some(&2));
    assert_eq!(repo::answered_count(&pool, user_id).await.unwrap(), 2);
}

#[sqlx::test]
async fn answering_the_same_question_again_overwrites(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    repo::upsert_responses(&pool, user_id, &[response("risk_1", 1)])
        .await
        .unwrap();
    repo::upsert_responses(&pool, user_id, &[response("risk_1", 5)])
        .await
        .unwrap();

    let answers = repo::answers_map(&pool, user_id).await.unwrap();
    assert_eq!(answers.get("risk_1"), Some(&5));
    assert_eq!(repo::answered_count(&pool, user_id).await.unwrap(), 1);
}

#[sqlx::test]
async fn responses_are_scoped_to_one_user(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    repo::upsert_responses(&pool, ada, &[response("risk_1", 4)])
        .await
        .unwrap();

    assert_eq!(repo::answered_count(&pool, grace).await.unwrap(), 0);
}

#[sqlx::test]
async fn trait_scores_are_saved_and_read_back(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    let scores = TraitScores {
        risk_tolerance: 80,
        pace_vs_rigor: 60,
        conflict_style: 40,
        decision_basis: 20,
        work_mode: 0,
        orientation: 100,
    };

    repo::save_trait_scores(&pool, user_id, &scores).await.unwrap();

    let loaded = repo::trait_scores_for(&pool, user_id).await.unwrap();
    assert_eq!(loaded, Some(scores));
}

#[sqlx::test]
async fn saving_trait_scores_twice_updates_in_place(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    let first = TraitScores {
        risk_tolerance: 10,
        pace_vs_rigor: 10,
        conflict_style: 10,
        decision_basis: 10,
        work_mode: 10,
        orientation: 10,
    };
    let second = TraitScores {
        risk_tolerance: 90,
        ..first
    };

    repo::save_trait_scores(&pool, user_id, &first).await.unwrap();
    repo::save_trait_scores(&pool, user_id, &second).await.unwrap();

    assert_eq!(
        repo::trait_scores_for(&pool, user_id).await.unwrap(),
        Some(second)
    );

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM trait_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test]
async fn trait_scores_can_be_removed(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    let scores = TraitScores {
        risk_tolerance: 50,
        pace_vs_rigor: 50,
        conflict_style: 50,
        decision_basis: 50,
        work_mode: 50,
        orientation: 50,
    };

    repo::save_trait_scores(&pool, user_id, &scores).await.unwrap();
    repo::delete_trait_scores(&pool, user_id).await.unwrap();

    assert_eq!(repo::trait_scores_for(&pool, user_id).await.unwrap(), None);
}

#[sqlx::test]
async fn removing_absent_trait_scores_is_not_an_error(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    repo::delete_trait_scores(&pool, user_id).await.unwrap();
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cd backend && cargo test --test assessment_repo`
Expected: FAIL — `could not find repo in assessment`.

- [ ] **Step 4: Implement the repository**

Create `backend/src/assessment/repo.rs`:

```rust
use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::assessment::scoring::TraitScores;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Response {
    pub question_id: String,
    pub value: i16,
}

pub async fn responses_for(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<Response>> {
    sqlx::query_as::<_, Response>(
        r#"
        SELECT question_id, value
        FROM question_responses
        WHERE user_id = $1
        ORDER BY question_id
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn answers_map(pool: &PgPool, user_id: Uuid) -> sqlx::Result<HashMap<String, i16>> {
    let responses = responses_for(pool, user_id).await?;

    Ok(responses
        .into_iter()
        .map(|response| (response.question_id, response.value))
        .collect())
}

pub async fn upsert_responses(
    pool: &PgPool,
    user_id: Uuid,
    responses: &[Response],
) -> sqlx::Result<()> {
    if responses.is_empty() {
        return Ok(());
    }

    let ids: Vec<String> = responses.iter().map(|r| r.question_id.clone()).collect();
    let values: Vec<i16> = responses.iter().map(|r| r.value).collect();

    // One statement rather than a loop: the whole batch lands or none of it does.
    sqlx::query(
        r#"
        INSERT INTO question_responses (user_id, question_id, value, updated_at)
        SELECT $1, submitted.id, submitted.value, now()
        FROM UNNEST($2::TEXT[], $3::SMALLINT[]) AS submitted(id, value)
        ON CONFLICT (user_id, question_id)
        DO UPDATE SET value = EXCLUDED.value, updated_at = now()
        "#,
    )
    .bind(user_id)
    .bind(&ids)
    .bind(&values)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn answered_count(pool: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM question_responses WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
}

pub async fn save_trait_scores(
    pool: &PgPool,
    user_id: Uuid,
    scores: &TraitScores,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO trait_scores (
            user_id, risk_tolerance, pace_vs_rigor, conflict_style,
            decision_basis, work_mode, orientation, computed_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, now())
        ON CONFLICT (user_id) DO UPDATE SET
            risk_tolerance = EXCLUDED.risk_tolerance,
            pace_vs_rigor  = EXCLUDED.pace_vs_rigor,
            conflict_style = EXCLUDED.conflict_style,
            decision_basis = EXCLUDED.decision_basis,
            work_mode      = EXCLUDED.work_mode,
            orientation    = EXCLUDED.orientation,
            computed_at    = now()
        "#,
    )
    .bind(user_id)
    .bind(scores.risk_tolerance)
    .bind(scores.pace_vs_rigor)
    .bind(scores.conflict_style)
    .bind(scores.decision_basis)
    .bind(scores.work_mode)
    .bind(scores.orientation)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_trait_scores(pool: &PgPool, user_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM trait_scores WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn trait_scores_for(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Option<TraitScores>> {
    sqlx::query_as::<_, TraitScores>(
        r#"
        SELECT risk_tolerance, pace_vs_rigor, conflict_style,
               decision_basis, work_mode, orientation
        FROM trait_scores
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}
```

Modify `backend/src/assessment/mod.rs`:

```rust
pub mod questions;
pub mod repo;
pub mod scoring;
```

- [ ] **Step 5: Run the tests and verify they pass**

Run: `cd backend && cargo test --test assessment_repo`
Expected: PASS — 8 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/migrations backend/src/assessment backend/tests/assessment_repo.rs
git commit -m "feat: question response and trait score persistence"
```

---

### Task 4: Assessment API

**Files:**
- Create: `backend/src/assessment/service.rs`, `backend/src/assessment/routes.rs`
- Modify: `backend/src/assessment/mod.rs`, `backend/src/app.rs`
- Test: `backend/tests/assessment_api.rs`

**Interfaces:**
- Consumes: `assessment::{questions, repo, scoring}`, `app::AppState`, `auth::extractor::CurrentUser`, `error::{ApiError, ApiResult, FieldError}`
- Produces:
  - `cofounder_api::assessment::routes::router() -> Router<AppState>` mounting `GET /questions`, `GET /me/responses`, `PUT /me/responses`
  - `cofounder_api::assessment::service::{ResponsesView, view, record}`
  - `ResponsesView { responses: Vec<Response>, answered: usize, total: usize, complete: bool }`
  - `record(&AppState, Uuid, Vec<Response>) -> ApiResult<ResponsesView>` — validates, upserts, then recomputes and materializes or clears `trait_scores`
  - `view(&AppState, Uuid) -> ApiResult<ResponsesView>`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/assessment_api.rs`:

```rust
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use cofounder_api::assessment::questions::QUESTIONS;
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;

fn state_with(pool: PgPool, mailer: Arc<RecordingMailer>) -> AppState {
    AppState {
        db: pool,
        mailer,
        base_url: "http://localhost:3000".into(),
        secure_cookies: false,
        test_mailer: None,
    }
}

fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Signs a user in and returns the `session=...` cookie pair.
async fn sign_in(state: AppState, mailer: &RecordingMailer, email: &str) -> String {
    router(state.clone())
        .oneshot(post_json(
            "/auth/magic-link",
            serde_json::json!({ "email": email }),
        ))
        .await
        .unwrap();

    let link = mailer.sent().last().unwrap().1.clone();
    let token = link.split("token=").nth(1).unwrap().to_string();

    let response = router(state)
        .oneshot(post_json(
            "/auth/verify",
            serde_json::json!({ "token": token }),
        ))
        .await
        .unwrap();

    let cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();

    cookie.split(';').next().unwrap().to_string()
}

fn get(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

fn put_json(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Every question answered with the same value.
fn all_answers(value: i16) -> serde_json::Value {
    let responses: Vec<serde_json::Value> = QUESTIONS
        .iter()
        .map(|q| serde_json::json!({ "question_id": q.id, "value": value }))
        .collect();
    serde_json::json!({ "responses": responses })
}

#[sqlx::test]
async fn questions_are_listed(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state).oneshot(get("/questions", &cookie)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["questions"].as_array().unwrap().len(), 18);
    assert_eq!(body["scale"].as_array().unwrap().len(), 5);
    assert!(body["questions"][0]["text"].is_string());
    assert!(body["questions"][0]["axis"].is_string());
}

#[sqlx::test]
async fn the_reverse_flag_is_never_exposed(pool: PgPool) {
    // Knowing which items are flipped is enough to fake a coherent profile.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state).oneshot(get("/questions", &cookie)).await.unwrap();
    let body = json_body(response).await;

    for question in body["questions"].as_array().unwrap() {
        assert!(question.get("reverse").is_none(), "reverse leaked: {question}");
    }
}

#[sqlx::test]
async fn questions_require_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(Request::builder().uri("/questions").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn responses_start_empty(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(get("/me/responses", &cookie))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["answered"], 0);
    assert_eq!(body["total"], 18);
    assert_eq!(body["complete"], false);
}

#[sqlx::test]
async fn a_partial_submission_is_accepted(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json(
            "/me/responses",
            &cookie,
            serde_json::json!({ "responses": [{ "question_id": "risk_1", "value": 4 }] }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["answered"], 1);
    assert_eq!(body["complete"], false);
}

#[sqlx::test]
async fn answering_everything_completes_the_assessment(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json("/me/responses", &cookie, all_answers(3)))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["answered"], 18);
    assert_eq!(body["complete"], true);

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM trait_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1, "trait scores should be materialized once complete");
}

#[sqlx::test]
async fn trait_scores_are_not_written_while_incomplete(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    router(state)
        .oneshot(put_json(
            "/me/responses",
            &cookie,
            serde_json::json!({ "responses": [{ "question_id": "risk_1", "value": 4 }] }),
        ))
        .await
        .unwrap();

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM trait_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test]
async fn changing_an_answer_recomputes_the_scores(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    router(state.clone())
        .oneshot(put_json("/me/responses", &cookie, all_answers(3)))
        .await
        .unwrap();

    let before: i16 = sqlx::query_scalar("SELECT risk_tolerance FROM trait_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, 50);

    // risk_2 is the reverse item; 5 / 1 / 5 is maximal risk tolerance.
    router(state)
        .oneshot(put_json(
            "/me/responses",
            &cookie,
            serde_json::json!({ "responses": [
                { "question_id": "risk_1", "value": 5 },
                { "question_id": "risk_2", "value": 1 },
                { "question_id": "risk_3", "value": 5 }
            ]}),
        ))
        .await
        .unwrap();

    let after: i16 = sqlx::query_scalar("SELECT risk_tolerance FROM trait_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, 100);
}

#[sqlx::test]
async fn an_unknown_question_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json(
            "/me/responses",
            &cookie,
            serde_json::json!({ "responses": [{ "question_id": "made_up", "value": 3 }] }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "made_up");
}

#[sqlx::test]
async fn a_value_outside_one_to_five_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json(
            "/me/responses",
            &cookie,
            serde_json::json!({ "responses": [{ "question_id": "risk_1", "value": 9 }] }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Nothing in the batch is written when any part of it is invalid.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM question_responses")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test]
async fn one_users_answers_are_invisible_to_another(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());

    let ada = sign_in(state.clone(), &mailer, "ada@example.com").await;
    router(state.clone())
        .oneshot(put_json("/me/responses", &ada, all_answers(5)))
        .await
        .unwrap();

    let grace = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let response = router(state)
        .oneshot(get("/me/responses", &grace))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["answered"], 0);
}

#[sqlx::test]
async fn submitting_responses_requires_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/me/responses")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "responses": [] }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test assessment_api`
Expected: FAIL — `could not find routes in assessment`.

- [ ] **Step 3: Implement the service**

Create `backend/src/assessment/service.rs`:

```rust
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
```

- [ ] **Step 4: Implement the routes**

Create `backend/src/assessment/routes.rs`:

```rust
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
        ScalePoint { value: 1, label: "Strongly disagree" },
        ScalePoint { value: 2, label: "Disagree" },
        ScalePoint { value: 3, label: "Neutral" },
        ScalePoint { value: 4, label: "Agree" },
        ScalePoint { value: 5, label: "Strongly agree" },
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
```

Modify `backend/src/assessment/mod.rs`:

```rust
pub mod questions;
pub mod repo;
pub mod routes;
pub mod scoring;
pub mod service;
```

- [ ] **Step 5: Mount the router**

Modify `backend/src/app.rs` — extend the `router` function's merge chain:

```rust
pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/health", get(health))
        .merge(crate::auth::routes::router())
        .merge(crate::assessment::routes::router());

    if state.test_mailer.is_some() {
        app = app.merge(test_router());
    }

    app.with_state(state)
}
```

- [ ] **Step 6: Run the tests and verify they pass**

Run: `cd backend && cargo test --test assessment_api`
Expected: PASS — 12 tests.

Then run the whole suite to confirm nothing regressed:

Run: `cd backend && cargo test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/src backend/tests/assessment_api.rs
git commit -m "feat: questionnaire and response endpoints"
```

---

### Task 5: Profile persistence

**Files:**
- Create: `backend/migrations/0006_profiles.sql`, `backend/migrations/0007_profile_interests.sql`, `backend/src/profiles/mod.rs`, `backend/src/profiles/repo.rs`
- Modify: `backend/src/lib.rs`
- Test: `backend/tests/profiles_repo.rs`

**Interfaces:**
- Consumes: `users::repo::find_or_create_by_email`
- Produces: `cofounder_api::profiles::repo::{ProfileRow, ProfileInput, find_by_user_id, interests_for, save}`
  - `ProfileRow` — the persisted columns, minus timestamps
  - `ProfileInput` — the deserializable write shape, including `interests: Vec<String>`
  - `find_by_user_id(&PgPool, Uuid) -> sqlx::Result<Option<ProfileRow>>`
  - `interests_for(&PgPool, Uuid) -> sqlx::Result<Vec<String>>`
  - `save(&PgPool, Uuid, &ProfileInput) -> sqlx::Result<(ProfileRow, Vec<String>)>` — profile and interests written in one transaction

- [ ] **Step 1: Write the migrations**

Create `backend/migrations/0006_profiles.sql`:

```sql
CREATE TABLE profiles (
    user_id       UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    display_name  TEXT NOT NULL DEFAULT '',
    headline      TEXT NOT NULL DEFAULT '',
    bio           TEXT NOT NULL DEFAULT '',
    city          TEXT NOT NULL DEFAULT '',
    country       TEXT NOT NULL DEFAULT '',
    timezone      TEXT NOT NULL DEFAULT '',
    linkedin_url  TEXT,
    github_url    TEXT,
    website_url   TEXT,
    roles         TEXT[] NOT NULL DEFAULT '{}',
    seeking_roles TEXT[] NOT NULL DEFAULT '{}',
    idea_status   TEXT CHECK (idea_status IN ('committed_idea', 'flexible_idea', 'looking_to_join')),
    stage         TEXT CHECK (stage IN ('idea', 'prototype', 'users', 'revenue')),
    commitment    TEXT CHECK (commitment IN ('full_time_now', 'full_time_when_funded', 'part_time', 'exploring')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- The role vocabulary is fixed by the design and feeds the scorer, so the
    -- database enforces it too rather than trusting the service layer alone.
    CONSTRAINT roles_are_known CHECK (
        roles <@ ARRAY['engineering', 'design', 'product', 'gtm', 'ops_finance', 'research']::TEXT[]
    ),
    CONSTRAINT seeking_roles_are_known CHECK (
        seeking_roles <@ ARRAY['engineering', 'design', 'product', 'gtm', 'ops_finance', 'research']::TEXT[]
    )
);
```

Create `backend/migrations/0007_profile_interests.sql`:

```sql
-- Industry tags are a curated list expected to grow, so unlike roles they are
-- validated in Rust only: adding a tag should not require a migration.
CREATE TABLE profile_interests (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tag     TEXT NOT NULL,
    PRIMARY KEY (user_id, tag)
);

CREATE INDEX profile_interests_tag_idx ON profile_interests (tag);
```

- [ ] **Step 2: Write the failing test**

Create `backend/tests/profiles_repo.rs`:

```rust
use cofounder_api::profiles::repo::{self, ProfileInput};
use cofounder_api::users;
use sqlx::PgPool;
use uuid::Uuid;

async fn a_user(pool: &PgPool, email: &str) -> Uuid {
    users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id
}

fn an_input() -> ProfileInput {
    ProfileInput {
        display_name: "Ada Lovelace".into(),
        headline: "Building tools for analytical engines".into(),
        bio: "Twenty years in numerical computing.".into(),
        city: "London".into(),
        country: "United Kingdom".into(),
        timezone: "Europe/London".into(),
        linkedin_url: Some("https://linkedin.com/in/ada".into()),
        github_url: None,
        website_url: None,
        roles: vec!["engineering".into(), "research".into()],
        seeking_roles: vec!["gtm".into()],
        idea_status: Some("committed_idea".into()),
        stage: Some("prototype".into()),
        commitment: Some("full_time_now".into()),
        interests: vec!["developer_tools".into(), "ai_ml".into()],
    }
}

#[sqlx::test]
async fn a_user_without_a_profile_has_none(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    assert!(repo::find_by_user_id(&pool, user_id).await.unwrap().is_none());
    assert!(repo::interests_for(&pool, user_id).await.unwrap().is_empty());
}

#[sqlx::test]
async fn saving_creates_the_profile(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    let (row, interests) = repo::save(&pool, user_id, &an_input()).await.unwrap();

    assert_eq!(row.display_name, "Ada Lovelace");
    assert_eq!(row.roles, vec!["engineering", "research"]);
    assert_eq!(row.seeking_roles, vec!["gtm"]);
    assert_eq!(row.commitment.as_deref(), Some("full_time_now"));
    assert_eq!(row.github_url, None);
    assert_eq!(interests, vec!["ai_ml", "developer_tools"]);
}

#[sqlx::test]
async fn saving_returns_what_reading_returns(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    let (saved, saved_interests) = repo::save(&pool, user_id, &an_input()).await.unwrap();

    let loaded = repo::find_by_user_id(&pool, user_id).await.unwrap().unwrap();
    let loaded_interests = repo::interests_for(&pool, user_id).await.unwrap();

    assert_eq!(loaded, saved);
    assert_eq!(loaded_interests, saved_interests);
}

#[sqlx::test]
async fn saving_twice_updates_rather_than_duplicating(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    repo::save(&pool, user_id, &an_input()).await.unwrap();

    let mut changed = an_input();
    changed.headline = "Now working on compilers".into();
    repo::save(&pool, user_id, &changed).await.unwrap();

    let row = repo::find_by_user_id(&pool, user_id).await.unwrap().unwrap();
    assert_eq!(row.headline, "Now working on compilers");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM profiles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test]
async fn interests_are_replaced_not_appended(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    repo::save(&pool, user_id, &an_input()).await.unwrap();

    let mut changed = an_input();
    changed.interests = vec!["climate".into()];
    let (_, interests) = repo::save(&pool, user_id, &changed).await.unwrap();

    assert_eq!(interests, vec!["climate"]);
}

#[sqlx::test]
async fn interests_can_be_cleared(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    repo::save(&pool, user_id, &an_input()).await.unwrap();

    let mut changed = an_input();
    changed.interests = vec![];
    let (_, interests) = repo::save(&pool, user_id, &changed).await.unwrap();

    assert!(interests.is_empty());
}

#[sqlx::test]
async fn profiles_are_scoped_to_one_user(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    repo::save(&pool, ada, &an_input()).await.unwrap();

    assert!(repo::find_by_user_id(&pool, grace).await.unwrap().is_none());
}

#[sqlx::test]
async fn the_database_refuses_an_unknown_role(pool: PgPool) {
    // The service layer validates too; this proves the constraint is really
    // there, so a future code path cannot write a role the scorer cannot read.
    let user_id = a_user(&pool, "ada@example.com").await;

    let mut input = an_input();
    input.roles = vec!["astrology".into()];

    assert!(repo::save(&pool, user_id, &input).await.is_err());
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cd backend && cargo test --test profiles_repo`
Expected: FAIL — `could not find profiles in cofounder_api`.

- [ ] **Step 4: Implement the repository**

Create `backend/src/profiles/mod.rs`:

```rust
pub mod repo;
```

Modify `backend/src/lib.rs` — add `pub mod profiles;`:

```rust
pub mod app;
pub mod assessment;
pub mod auth;
pub mod config;
pub mod db;
pub mod email;
pub mod error;
pub mod profiles;
pub mod users;
```

Create `backend/src/profiles/repo.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct ProfileRow {
    pub display_name: String,
    pub headline: String,
    pub bio: String,
    pub city: String,
    pub country: String,
    pub timezone: String,
    pub linkedin_url: Option<String>,
    pub github_url: Option<String>,
    pub website_url: Option<String>,
    pub roles: Vec<String>,
    pub seeking_roles: Vec<String>,
    pub idea_status: Option<String>,
    pub stage: Option<String>,
    pub commitment: Option<String>,
}

/// The write shape. Every field is replaced on save: the profile is edited as
/// one document, so a partial update would silently discard whatever the form
/// did not send.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProfileInput {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub headline: String,
    #[serde(default)]
    pub bio: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub linkedin_url: Option<String>,
    #[serde(default)]
    pub github_url: Option<String>,
    #[serde(default)]
    pub website_url: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub seeking_roles: Vec<String>,
    #[serde(default)]
    pub idea_status: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub commitment: Option<String>,
    #[serde(default)]
    pub interests: Vec<String>,
}

const COLUMNS: &str = "display_name, headline, bio, city, country, timezone, \
     linkedin_url, github_url, website_url, roles, seeking_roles, \
     idea_status, stage, commitment";

pub async fn find_by_user_id(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Option<ProfileRow>> {
    sqlx::query_as::<_, ProfileRow>(&format!(
        "SELECT {COLUMNS} FROM profiles WHERE user_id = $1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn interests_for(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT tag FROM profile_interests WHERE user_id = $1 ORDER BY tag")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

/// Writes the profile and its interests together. A transaction because a
/// half-applied save would leave the interests of the previous version
/// attached to the new one.
pub async fn save(
    pool: &PgPool,
    user_id: Uuid,
    input: &ProfileInput,
) -> sqlx::Result<(ProfileRow, Vec<String>)> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query_as::<_, ProfileRow>(&format!(
        r#"
        INSERT INTO profiles (
            user_id, display_name, headline, bio, city, country, timezone,
            linkedin_url, github_url, website_url, roles, seeking_roles,
            idea_status, stage, commitment, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now())
        ON CONFLICT (user_id) DO UPDATE SET
            display_name  = EXCLUDED.display_name,
            headline      = EXCLUDED.headline,
            bio           = EXCLUDED.bio,
            city          = EXCLUDED.city,
            country       = EXCLUDED.country,
            timezone      = EXCLUDED.timezone,
            linkedin_url  = EXCLUDED.linkedin_url,
            github_url    = EXCLUDED.github_url,
            website_url   = EXCLUDED.website_url,
            roles         = EXCLUDED.roles,
            seeking_roles = EXCLUDED.seeking_roles,
            idea_status   = EXCLUDED.idea_status,
            stage         = EXCLUDED.stage,
            commitment    = EXCLUDED.commitment,
            updated_at    = now()
        RETURNING {COLUMNS}
        "#
    ))
    .bind(user_id)
    .bind(&input.display_name)
    .bind(&input.headline)
    .bind(&input.bio)
    .bind(&input.city)
    .bind(&input.country)
    .bind(&input.timezone)
    .bind(&input.linkedin_url)
    .bind(&input.github_url)
    .bind(&input.website_url)
    .bind(&input.roles)
    .bind(&input.seeking_roles)
    .bind(&input.idea_status)
    .bind(&input.stage)
    .bind(&input.commitment)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM profile_interests WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    if !input.interests.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO profile_interests (user_id, tag)
            SELECT $1, tag FROM UNNEST($2::TEXT[]) AS tag
            "#,
        )
        .bind(user_id)
        .bind(&input.interests)
        .execute(&mut *tx)
        .await?;
    }

    let interests: Vec<String> =
        sqlx::query_scalar("SELECT tag FROM profile_interests WHERE user_id = $1 ORDER BY tag")
            .bind(user_id)
            .fetch_all(&mut *tx)
            .await?;

    tx.commit().await?;

    Ok((row, interests))
}
```

- [ ] **Step 5: Run the tests and verify they pass**

Run: `cd backend && cargo test --test profiles_repo`
Expected: PASS — 8 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/migrations backend/src/profiles backend/src/lib.rs backend/tests/profiles_repo.rs
git commit -m "feat: profile and interest persistence"
```

---

### Task 6: Profile API, vocabularies, and completeness

**Files:**
- Create: `backend/src/profiles/vocab.rs`, `backend/src/profiles/service.rs`, `backend/src/profiles/routes.rs`
- Modify: `backend/src/profiles/mod.rs`, `backend/src/app.rs`
- Test: `backend/tests/profile_api.rs`

**Interfaces:**
- Consumes: `profiles::repo::{ProfileRow, ProfileInput, find_by_user_id, interests_for, save}`, `assessment::repo::answered_count`, `assessment::service::TOTAL_QUESTIONS`, `auth::extractor::CurrentUser`
- Produces:
  - `cofounder_api::profiles::routes::router() -> Router<AppState>` mounting `GET /options`, `GET /me/profile`, `PUT /me/profile`
  - `cofounder_api::profiles::vocab::{Choice, ROLES, IDEA_STATUSES, STAGES, COMMITMENTS, INTERESTS, contains}`
  - `cofounder_api::profiles::service::{ProfileView, ProfileBody, view, update}`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/profile_api.rs`:

```rust
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use cofounder_api::assessment::questions::QUESTIONS;
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;

fn state_with(pool: PgPool, mailer: Arc<RecordingMailer>) -> AppState {
    AppState {
        db: pool,
        mailer,
        base_url: "http://localhost:3000".into(),
        secure_cookies: false,
        test_mailer: None,
    }
}

fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn sign_in(state: AppState, mailer: &RecordingMailer, email: &str) -> String {
    router(state.clone())
        .oneshot(post_json(
            "/auth/magic-link",
            serde_json::json!({ "email": email }),
        ))
        .await
        .unwrap();

    let link = mailer.sent().last().unwrap().1.clone();
    let token = link.split("token=").nth(1).unwrap().to_string();

    let response = router(state)
        .oneshot(post_json(
            "/auth/verify",
            serde_json::json!({ "token": token }),
        ))
        .await
        .unwrap();

    let cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();

    cookie.split(';').next().unwrap().to_string()
}

fn get(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

fn put_json(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn a_complete_profile() -> serde_json::Value {
    serde_json::json!({
        "display_name": "Ada Lovelace",
        "headline": "Building tools for analytical engines",
        "bio": "Twenty years in numerical computing.",
        "city": "London",
        "country": "United Kingdom",
        "timezone": "Europe/London",
        "linkedin_url": "https://linkedin.com/in/ada",
        "github_url": null,
        "website_url": null,
        "roles": ["engineering", "research"],
        "seeking_roles": ["gtm"],
        "idea_status": "committed_idea",
        "stage": "prototype",
        "commitment": "full_time_now",
        "interests": ["developer_tools", "ai_ml"]
    })
}

async fn answer_everything(state: AppState, cookie: &str) {
    let responses: Vec<serde_json::Value> = QUESTIONS
        .iter()
        .map(|q| serde_json::json!({ "question_id": q.id, "value": 3 }))
        .collect();

    router(state)
        .oneshot(put_json(
            "/me/responses",
            cookie,
            serde_json::json!({ "responses": responses }),
        ))
        .await
        .unwrap();
}

#[sqlx::test]
async fn options_lists_every_vocabulary(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state).oneshot(get("/options", &cookie)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["roles"].as_array().unwrap().len(), 6);
    assert_eq!(body["idea_statuses"].as_array().unwrap().len(), 3);
    assert_eq!(body["stages"].as_array().unwrap().len(), 4);
    assert_eq!(body["commitments"].as_array().unwrap().len(), 4);
    assert!(!body["interests"].as_array().unwrap().is_empty());
    assert!(body["roles"][0]["id"].is_string());
    assert!(body["roles"][0]["label"].is_string());
}

#[sqlx::test]
async fn a_new_user_gets_an_empty_profile_rather_than_a_404(pool: PgPool) {
    // The form needs something to render. 404 would make an ordinary first
    // visit look like an error.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(get("/me/profile", &cookie))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["profile"]["bio"], "");
    assert_eq!(body["profile"]["roles"].as_array().unwrap().len(), 0);
    assert_eq!(body["complete"], false);
}

#[sqlx::test]
async fn a_profile_is_saved_and_read_back(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let saved = router(state.clone())
        .oneshot(put_json("/me/profile", &cookie, a_complete_profile()))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);

    let response = router(state)
        .oneshot(get("/me/profile", &cookie))
        .await
        .unwrap();
    let body = json_body(response).await;

    assert_eq!(body["profile"]["display_name"], "Ada Lovelace");
    assert_eq!(body["profile"]["commitment"], "full_time_now");
    assert_eq!(body["profile"]["interests"].as_array().unwrap().len(), 2);
}

#[sqlx::test]
async fn a_profile_is_incomplete_until_the_assessment_is_answered(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, a_complete_profile()))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["complete"], false);
    assert_eq!(body["missing"], serde_json::json!(["responses"]));
}

#[sqlx::test]
async fn a_profile_becomes_complete_once_everything_is_filled(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    router(state.clone())
        .oneshot(put_json("/me/profile", &cookie, a_complete_profile()))
        .await
        .unwrap();
    answer_everything(state.clone(), &cookie).await;

    let response = router(state)
        .oneshot(get("/me/profile", &cookie))
        .await
        .unwrap();
    let body = json_body(response).await;

    assert_eq!(body["complete"], true);
    assert_eq!(body["missing"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn every_missing_requirement_is_named(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json(
            "/me/profile",
            &cookie,
            serde_json::json!({ "display_name": "Ada" }),
        ))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(
        body["missing"],
        serde_json::json!(["bio", "roles", "seeking_roles", "commitment", "responses"])
    );
}

#[sqlx::test]
async fn an_unknown_role_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["roles"] = serde_json::json!(["astrology"]);

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "roles");
}

#[sqlx::test]
async fn an_unknown_interest_tag_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["interests"] = serde_json::json!(["underwater_basket_weaving"]);

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "interests");
}

#[sqlx::test]
async fn an_unknown_commitment_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["commitment"] = serde_json::json!("whenever");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "commitment");
}

#[sqlx::test]
async fn an_overlong_bio_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["bio"] = serde_json::json!("x".repeat(2001));

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "bio");
}

#[sqlx::test]
async fn a_link_that_is_not_a_url_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["website_url"] = serde_json::json!("javascript:alert(1)");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "website_url");
}

#[sqlx::test]
async fn an_empty_link_is_stored_as_null_rather_than_rejected(pool: PgPool) {
    // The form submits "" for a link the user left blank.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["website_url"] = serde_json::json!("");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body["profile"]["website_url"].is_null());
}

#[sqlx::test]
async fn whitespace_around_text_is_trimmed(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["display_name"] = serde_json::json!("  Ada Lovelace  ");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["profile"]["display_name"], "Ada Lovelace");
}

#[sqlx::test]
async fn a_bio_of_only_whitespace_does_not_count_as_filled_in(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["bio"] = serde_json::json!("   ");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert!(body["missing"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("bio")));
}

#[sqlx::test]
async fn one_users_profile_is_invisible_to_another(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());

    let ada = sign_in(state.clone(), &mailer, "ada@example.com").await;
    router(state.clone())
        .oneshot(put_json("/me/profile", &ada, a_complete_profile()))
        .await
        .unwrap();

    let grace = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let response = router(state)
        .oneshot(get("/me/profile", &grace))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["profile"]["display_name"], "");
}

#[sqlx::test]
async fn the_profile_endpoints_require_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer);

    let read = router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/me/profile")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::UNAUTHORIZED);

    let write = router(state)
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/me/profile")
                .header("content-type", "application/json")
                .body(Body::from(a_complete_profile().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(write.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test profile_api`
Expected: FAIL — `could not find routes in profiles`.

- [ ] **Step 3: Write the vocabularies**

Create `backend/src/profiles/vocab.rs`:

```rust
/// One selectable value. Served to the frontend so the form's labels and the
/// database's CHECK constraints can never drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Choice {
    pub id: &'static str,
    pub label: &'static str,
}

pub const ROLES: [Choice; 6] = [
    Choice { id: "engineering", label: "Engineering" },
    Choice { id: "design", label: "Design" },
    Choice { id: "product", label: "Product" },
    Choice { id: "gtm", label: "GTM / Sales" },
    Choice { id: "ops_finance", label: "Ops / Finance" },
    Choice { id: "research", label: "Research / Science" },
];

pub const IDEA_STATUSES: [Choice; 3] = [
    Choice { id: "committed_idea", label: "I have an idea I'm committed to" },
    Choice { id: "flexible_idea", label: "I have an idea but I'm flexible" },
    Choice { id: "looking_to_join", label: "I'm looking to join someone else's" },
];

pub const STAGES: [Choice; 4] = [
    Choice { id: "idea", label: "Idea" },
    Choice { id: "prototype", label: "Prototype" },
    Choice { id: "users", label: "Users" },
    Choice { id: "revenue", label: "Revenue" },
];

pub const COMMITMENTS: [Choice; 4] = [
    Choice { id: "full_time_now", label: "Full-time now" },
    Choice { id: "full_time_when_funded", label: "Full-time once funded" },
    Choice { id: "part_time", label: "Part-time" },
    Choice { id: "exploring", label: "Exploring" },
];

pub const INTERESTS: [Choice; 18] = [
    Choice { id: "ai_ml", label: "AI / ML" },
    Choice { id: "agritech", label: "Agriculture" },
    Choice { id: "biotech", label: "Biotech" },
    Choice { id: "climate", label: "Climate" },
    Choice { id: "consumer_social", label: "Consumer / Social" },
    Choice { id: "developer_tools", label: "Developer tools" },
    Choice { id: "ecommerce", label: "E-commerce" },
    Choice { id: "edtech", label: "Education" },
    Choice { id: "fintech", label: "Fintech" },
    Choice { id: "gaming", label: "Gaming" },
    Choice { id: "healthtech", label: "Health" },
    Choice { id: "logistics", label: "Logistics" },
    Choice { id: "marketplace", label: "Marketplaces" },
    Choice { id: "media", label: "Media" },
    Choice { id: "real_estate", label: "Real estate" },
    Choice { id: "robotics", label: "Robotics" },
    Choice { id: "saas", label: "SaaS" },
    Choice { id: "security", label: "Security" },
];

pub fn contains(choices: &[Choice], id: &str) -> bool {
    choices.iter().any(|choice| choice.id == id)
}
```

- [ ] **Step 4: Write the service**

Create `backend/src/profiles/service.rs`:

```rust
use std::collections::HashSet;

use uuid::Uuid;

use crate::app::AppState;
use crate::assessment::repo as assessment_repo;
use crate::assessment::service::TOTAL_QUESTIONS;
use crate::error::{ApiError, ApiResult, FieldError};
use crate::profiles::repo::{self, ProfileInput, ProfileRow};
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
fn normalize_link(
    errors: &mut Vec<FieldError>,
    field: &str,
    value: &mut Option<String>,
) {
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

    check_length(&mut errors, "display_name", &input.display_name, MAX_DISPLAY_NAME);
    check_length(&mut errors, "headline", &input.headline, MAX_HEADLINE);
    check_length(&mut errors, "bio", &input.bio, MAX_BIO);
    check_length(&mut errors, "city", &input.city, MAX_PLACE);
    check_length(&mut errors, "country", &input.country, MAX_PLACE);
    check_length(&mut errors, "timezone", &input.timezone, MAX_TIMEZONE);

    check_tags(&mut errors, "roles", &input.roles, &vocab::ROLES, vocab::ROLES.len());
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

    check_choice(&mut errors, "idea_status", &input.idea_status, &vocab::IDEA_STATUSES);
    check_choice(&mut errors, "stage", &input.stage, &vocab::STAGES);
    check_choice(&mut errors, "commitment", &input.commitment, &vocab::COMMITMENTS);

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
```

- [ ] **Step 5: Write the routes**

Create `backend/src/profiles/routes.rs`:

```rust
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
```

Modify `backend/src/profiles/mod.rs`:

```rust
pub mod repo;
pub mod routes;
pub mod service;
pub mod vocab;
```

- [ ] **Step 6: Mount the router**

Modify `backend/src/app.rs` — extend the merge chain again:

```rust
pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/health", get(health))
        .merge(crate::auth::routes::router())
        .merge(crate::assessment::routes::router())
        .merge(crate::profiles::routes::router());

    if state.test_mailer.is_some() {
        app = app.merge(test_router());
    }

    app.with_state(state)
}
```

- [ ] **Step 7: Run the tests and verify they pass**

Run: `cd backend && cargo test --test profile_api`
Expected: PASS — 16 tests.

Run: `cd backend && cargo test`
Expected: PASS — the whole suite.

- [ ] **Step 8: Commit**

```bash
git add backend/src backend/tests/profile_api.rs
git commit -m "feat: profile endpoints, vocabularies, and completeness"
```

---

### Task 7: Profile page

**Files:**
- Create: `frontend/lib/profile.ts`, `frontend/app/(app)/profile/page.tsx`, `frontend/app/(app)/profile/profile-form.tsx`
- Test: covered end-to-end in Task 9

**Interfaces:**
- Consumes: `GET /api/options`, `GET /api/me/profile`, `PUT /api/me/profile`, `apiFetch` and `ApiError` from `frontend/lib/api.ts`
- Produces: `frontend/lib/profile.ts` exporting `Choice`, `Options`, `ProfileBody`, `ProfileView`, `EMPTY_PROFILE`, `MISSING_LABELS`

- [ ] **Step 1: Read the framework docs**

`frontend/AGENTS.md` requires this before any frontend code. Read at minimum:

```bash
cd frontend
cat node_modules/next/dist/docs/01-app/01-getting-started/05-server-and-client-components.md
cat node_modules/next/dist/docs/01-app/01-getting-started/03-layouts-and-pages.md
```

Heed any deprecation notices. Nothing below should contradict them; if it does, the docs win — note the divergence in the commit message.

- [ ] **Step 2: Write the shared types**

Create `frontend/lib/profile.ts`:

```ts
export interface Choice {
  id: string;
  label: string;
}

export interface Options {
  roles: Choice[];
  idea_statuses: Choice[];
  stages: Choice[];
  commitments: Choice[];
  interests: Choice[];
}

export interface ProfileBody {
  display_name: string;
  headline: string;
  bio: string;
  city: string;
  country: string;
  timezone: string;
  linkedin_url: string | null;
  github_url: string | null;
  website_url: string | null;
  roles: string[];
  seeking_roles: string[];
  idea_status: string | null;
  stage: string | null;
  commitment: string | null;
  interests: string[];
}

export interface ProfileView {
  profile: ProfileBody;
  complete: boolean;
  missing: string[];
}

export const EMPTY_PROFILE: ProfileBody = {
  display_name: "",
  headline: "",
  bio: "",
  city: "",
  country: "",
  timezone: "",
  linkedin_url: "",
  github_url: "",
  website_url: "",
  roles: [],
  seeking_roles: [],
  idea_status: null,
  stage: null,
  commitment: null,
  interests: [],
};

/// The API names what is missing; this turns those names into prose.
export const MISSING_LABELS: Record<string, string> = {
  bio: "Write a short bio",
  roles: "Pick what you bring",
  seeking_roles: "Pick what you're looking for",
  commitment: "Say how committed you are",
  responses: "Answer the 18 work-style questions",
};
```

- [ ] **Step 3: Write the page shell**

Create `frontend/app/(app)/profile/page.tsx`:

```tsx
import ProfileForm from "./profile-form";

export default function ProfilePage() {
  return <ProfileForm />;
}
```

- [ ] **Step 4: Write the form**

Create `frontend/app/(app)/profile/profile-form.tsx`:

```tsx
"use client";

import { useEffect, useState } from "react";
import { ApiError, apiFetch } from "@/lib/api";
import {
  Choice,
  EMPTY_PROFILE,
  Options,
  ProfileBody,
  ProfileView,
} from "@/lib/profile";

type Status = "loading" | "ready" | "saving" | "saved" | "failed";

export default function ProfileForm() {
  const [options, setOptions] = useState<Options | null>(null);
  const [profile, setProfile] = useState<ProfileBody>(EMPTY_PROFILE);
  const [missing, setMissing] = useState<string[]>([]);
  const [status, setStatus] = useState<Status>("loading");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      apiFetch<Options>("/options"),
      apiFetch<ProfileView>("/me/profile"),
    ])
      .then(([loadedOptions, view]) => {
        setOptions(loadedOptions);
        // The API returns null for unset links; the inputs need strings.
        setProfile({
          ...view.profile,
          linkedin_url: view.profile.linkedin_url ?? "",
          github_url: view.profile.github_url ?? "",
          website_url: view.profile.website_url ?? "",
        });
        setMissing(view.missing);
        setStatus("ready");
      })
      .catch(() => {
        setStatus("failed");
        setMessage("Could not load your profile. Reload to try again.");
      });
  }, []);

  function set<K extends keyof ProfileBody>(key: K, value: ProfileBody[K]) {
    setProfile((current) => ({ ...current, [key]: value }));
    setStatus("ready");
  }

  function toggle(key: "roles" | "seeking_roles" | "interests", id: string) {
    setProfile((current) => {
      const selected = current[key];
      return {
        ...current,
        [key]: selected.includes(id)
          ? selected.filter((value) => value !== id)
          : [...selected, id],
      };
    });
    setStatus("ready");
  }

  async function onSubmit(event: React.FormEvent) {
    event.preventDefault();
    setStatus("saving");
    setErrors({});
    setMessage(null);

    try {
      const view = await apiFetch<ProfileView>("/me/profile", {
        method: "PUT",
        body: JSON.stringify(profile),
      });
      setMissing(view.missing);
      setStatus("saved");
      setMessage("Profile saved");
    } catch (err) {
      setStatus("failed");
      if (err instanceof ApiError) {
        const fields: Record<string, string> = {};
        for (const problem of err.problem.errors ?? []) {
          fields[problem.field] = problem.message;
        }
        setErrors(fields);
        setMessage(
          err.problem.errors?.length
            ? "Some fields need attention"
            : err.problem.title,
        );
      } else {
        setMessage("Could not reach the server. Try again.");
      }
    }
  }

  if (status === "loading") {
    return <p className="text-neutral-600">Loading your profile…</p>;
  }

  if (!options) {
    return (
      <p role="alert" className="text-red-600">
        {message}
      </p>
    );
  }

  return (
    <form onSubmit={onSubmit} className="flex max-w-2xl flex-col gap-8">
      <div>
        <h1 className="text-2xl font-semibold">Your profile</h1>
        <p className="mt-1 text-neutral-600">
          {missing.length === 0
            ? "Your profile is complete."
            : `${missing.length} thing${missing.length === 1 ? "" : "s"} left before you appear in decks.`}
        </p>
      </div>

      <Section title="Identity">
        <Field label="Display name" error={errors.display_name}>
          <input
            id="display_name"
            value={profile.display_name}
            onChange={(e) => set("display_name", e.target.value)}
            className={inputClass}
          />
        </Field>
        <Field label="Headline" error={errors.headline}>
          <input
            id="headline"
            value={profile.headline}
            onChange={(e) => set("headline", e.target.value)}
            placeholder="One line on what you're building"
            className={inputClass}
          />
        </Field>
        <Field label="Bio" error={errors.bio}>
          <textarea
            id="bio"
            rows={5}
            value={profile.bio}
            onChange={(e) => set("bio", e.target.value)}
            className={inputClass}
          />
        </Field>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="City" error={errors.city}>
            <input
              id="city"
              value={profile.city}
              onChange={(e) => set("city", e.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label="Country" error={errors.country}>
            <input
              id="country"
              value={profile.country}
              onChange={(e) => set("country", e.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label="Timezone" error={errors.timezone}>
            <input
              id="timezone"
              value={profile.timezone}
              onChange={(e) => set("timezone", e.target.value)}
              placeholder="Europe/London"
              className={inputClass}
            />
          </Field>
        </div>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="LinkedIn" error={errors.linkedin_url}>
            <input
              id="linkedin_url"
              value={profile.linkedin_url ?? ""}
              onChange={(e) => set("linkedin_url", e.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label="GitHub" error={errors.github_url}>
            <input
              id="github_url"
              value={profile.github_url ?? ""}
              onChange={(e) => set("github_url", e.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label="Website" error={errors.website_url}>
            <input
              id="website_url"
              value={profile.website_url ?? ""}
              onChange={(e) => set("website_url", e.target.value)}
              className={inputClass}
            />
          </Field>
        </div>
      </Section>

      <Section title="What you bring">
        <ChoiceGroup
          legend="Your strengths"
          choices={options.roles}
          selected={profile.roles}
          onToggle={(id) => toggle("roles", id)}
          error={errors.roles}
        />
      </Section>

      <Section title="What you're looking for">
        <ChoiceGroup
          legend="Cofounder strengths"
          choices={options.roles}
          selected={profile.seeking_roles}
          onToggle={(id) => toggle("seeking_roles", id)}
          error={errors.seeking_roles}
        />
      </Section>

      <Section title="Where you are">
        <Field label="Idea status" error={errors.idea_status}>
          <select
            id="idea_status"
            value={profile.idea_status ?? ""}
            onChange={(e) => set("idea_status", e.target.value || null)}
            className={inputClass}
          >
            <option value="">Not set</option>
            {options.idea_statuses.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Stage" error={errors.stage}>
          <select
            id="stage"
            value={profile.stage ?? ""}
            onChange={(e) => set("stage", e.target.value || null)}
            className={inputClass}
          >
            <option value="">Not set</option>
            {options.stages.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Commitment" error={errors.commitment}>
          <select
            id="commitment"
            value={profile.commitment ?? ""}
            onChange={(e) => set("commitment", e.target.value || null)}
            className={inputClass}
          >
            <option value="">Not set</option>
            {options.commitments.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.label}
              </option>
            ))}
          </select>
        </Field>
      </Section>

      <Section title="Interests">
        <ChoiceGroup
          legend="Industries"
          choices={options.interests}
          selected={profile.interests}
          onToggle={(id) => toggle("interests", id)}
          error={errors.interests}
        />
      </Section>

      <div className="flex items-center gap-3">
        <button
          type="submit"
          disabled={status === "saving"}
          className="rounded-lg bg-neutral-900 px-4 py-2 text-white disabled:opacity-50"
        >
          {status === "saving" ? "Saving…" : "Save profile"}
        </button>
        {message && (
          <p
            id="profile-status"
            role="status"
            className={
              status === "failed" ? "text-sm text-red-600" : "text-sm text-green-700"
            }
          >
            {message}
          </p>
        )}
      </div>
    </form>
  );
}

const inputClass = "w-full rounded-lg border border-neutral-300 px-3 py-2";

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-lg font-medium">{title}</h2>
      {children}
    </section>
  );
}

function Field({
  label,
  error,
  children,
}: {
  label: string;
  error?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-sm font-medium text-neutral-700">{label}</span>
      {children}
      {error && <p className="text-sm text-red-600">{error}</p>}
    </div>
  );
}

function ChoiceGroup({
  legend,
  choices,
  selected,
  onToggle,
  error,
}: {
  legend: string;
  choices: Choice[];
  selected: string[];
  onToggle: (id: string) => void;
  error?: string;
}) {
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="text-sm font-medium text-neutral-700">{legend}</legend>
      <div className="flex flex-wrap gap-2">
        {choices.map((choice) => {
          const active = selected.includes(choice.id);
          return (
            <button
              key={choice.id}
              type="button"
              aria-pressed={active}
              onClick={() => onToggle(choice.id)}
              className={`rounded-full border px-3 py-1 text-sm ${
                active
                  ? "border-neutral-900 bg-neutral-900 text-white"
                  : "border-neutral-300 text-neutral-700"
              }`}
            >
              {choice.label}
            </button>
          );
        })}
      </div>
      {error && <p className="text-sm text-red-600">{error}</p>}
    </fieldset>
  );
}
```

- [ ] **Step 5: Check it compiles and lints**

Run: `cd frontend && npx tsc --noEmit && npm run lint`
Expected: PASS, no errors.

- [ ] **Step 6: Commit**

```bash
git add frontend/lib/profile.ts "frontend/app/(app)/profile"
git commit -m "feat: profile editing page"
```

---

### Task 8: Assessment page

**Files:**
- Create: `frontend/app/(app)/assessment/page.tsx`, `frontend/app/(app)/assessment/assessment-client.tsx`
- Test: covered end-to-end in Task 9

**Interfaces:**
- Consumes: `GET /api/questions`, `GET /api/me/responses`, `PUT /api/me/responses`, `apiFetch` from `frontend/lib/api.ts`
- Produces: nothing other tasks import

- [ ] **Step 1: Write the page shell**

Create `frontend/app/(app)/assessment/page.tsx`:

```tsx
import AssessmentClient from "./assessment-client";

export default function AssessmentPage() {
  return <AssessmentClient />;
}
```

- [ ] **Step 2: Write the autosaving client**

Create `frontend/app/(app)/assessment/assessment-client.tsx`:

```tsx
"use client";

import { useEffect, useState } from "react";
import { apiFetch } from "@/lib/api";

interface Question {
  id: string;
  text: string;
  axis: string;
}

interface ScalePoint {
  value: number;
  label: string;
}

interface Questionnaire {
  questions: Question[];
  scale: ScalePoint[];
}

interface Response {
  question_id: string;
  value: number;
}

interface ResponsesView {
  responses: Response[];
  answered: number;
  total: number;
  complete: boolean;
}

export default function AssessmentClient() {
  const [questionnaire, setQuestionnaire] = useState<Questionnaire | null>(null);
  const [answers, setAnswers] = useState<Record<string, number>>({});
  const [view, setView] = useState<ResponsesView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      apiFetch<Questionnaire>("/questions"),
      apiFetch<ResponsesView>("/me/responses"),
    ])
      .then(([loaded, current]) => {
        setQuestionnaire(loaded);
        setAnswers(
          Object.fromEntries(
            current.responses.map((r) => [r.question_id, r.value]),
          ),
        );
        setView(current);
        setLoading(false);
      })
      .catch(() => {
        setError("Could not load the questions. Reload to try again.");
        setLoading(false);
      });
  }, []);

  async function answer(questionId: string, value: number) {
    // Optimistic: the radio should light up on click, not after a roundtrip.
    const previous = answers[questionId];
    setAnswers((current) => ({ ...current, [questionId]: value }));
    setError(null);

    try {
      const updated = await apiFetch<ResponsesView>("/me/responses", {
        method: "PUT",
        body: JSON.stringify({
          responses: [{ question_id: questionId, value }],
        }),
      });
      setView(updated);
    } catch {
      setAnswers((current) => {
        const rolledBack = { ...current };
        if (previous === undefined) {
          delete rolledBack[questionId];
        } else {
          rolledBack[questionId] = previous;
        }
        return rolledBack;
      });
      setError("That answer didn't save. Try again.");
    }
  }

  if (loading) {
    return <p className="text-neutral-600">Loading the questions…</p>;
  }

  if (!questionnaire) {
    return (
      <p role="alert" className="text-red-600">
        {error}
      </p>
    );
  }

  const answered = view?.answered ?? 0;
  const total = view?.total ?? questionnaire.questions.length;

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <div>
        <h1 className="text-2xl font-semibold">Work style</h1>
        <p className="mt-1 text-neutral-600">
          Eighteen statements. There are no right answers — answer as you
          actually work, not as you would like to. Each answer saves as you go.
        </p>
      </div>

      <p id="assessment-progress" role="status" className="text-sm font-medium">
        {answered === total
          ? "All 18 answered"
          : `${answered} of ${total} answered`}
      </p>

      {error && (
        <p role="alert" className="text-sm text-red-600">
          {error}
        </p>
      )}

      <ol className="flex flex-col gap-6">
        {questionnaire.questions.map((question, index) => (
          <li key={question.id} className="flex flex-col gap-2">
            <fieldset>
              <legend className="text-neutral-900">
                {index + 1}. {question.text}
              </legend>
              <div className="mt-2 flex flex-wrap gap-2">
                {questionnaire.scale.map((point) => {
                  const selected = answers[question.id] === point.value;
                  return (
                    <label
                      key={point.value}
                      className={`cursor-pointer rounded-lg border px-3 py-1 text-sm ${
                        selected
                          ? "border-neutral-900 bg-neutral-900 text-white"
                          : "border-neutral-300 text-neutral-700"
                      }`}
                    >
                      <input
                        type="radio"
                        name={question.id}
                        value={point.value}
                        checked={selected}
                        onChange={() => answer(question.id, point.value)}
                        className="sr-only"
                      />
                      {point.label}
                    </label>
                  );
                })}
              </div>
            </fieldset>
          </li>
        ))}
      </ol>
    </div>
  );
}
```

- [ ] **Step 3: Check it compiles and lints**

Run: `cd frontend && npx tsc --noEmit && npm run lint`
Expected: PASS, no errors.

- [ ] **Step 4: Commit**

```bash
git add "frontend/app/(app)/assessment"
git commit -m "feat: autosaving work-style assessment"
```

---

### Task 9: Navigation, home checklist, and the end-to-end journey

**Files:**
- Modify: `frontend/app/(app)/layout.tsx`, `frontend/app/(app)/home/page.tsx`
- Create: `frontend/e2e/helpers.ts`, `frontend/e2e/profile.spec.ts`

**Interfaces:**
- Consumes: `getCurrentUser` from `frontend/lib/session.ts`, `MISSING_LABELS` and `ProfileView` from `frontend/lib/profile.ts`
- Produces: `frontend/e2e/helpers.ts` exporting `signIn(page, request): Promise<string>` — signs a fresh user in and returns the email used

- [ ] **Step 1: Write the failing end-to-end test**

Create `frontend/e2e/helpers.ts`:

```ts
import { APIRequestContext, Page, expect } from "@playwright/test";

const BACKEND = process.env.BACKEND_URL ?? "http://localhost:8080";

/**
 * Signs a brand-new user in through the real magic-link flow and returns the
 * address used. The link is read from the test-only endpoint the backend
 * mounts when APP_ENV=test.
 */
export async function signIn(
  page: Page,
  request: APIRequestContext,
  prefix = "user",
): Promise<string> {
  const email = `${prefix}+${Date.now()}@example.com`;

  await page.goto("/login");
  await page.getByPlaceholder("you@example.com").fill(email);
  await page.getByRole("button", { name: "Send sign-in link" }).click();
  await expect(page.getByText("Check your email")).toBeVisible();

  const { link } = await (
    await request.get(`${BACKEND}/test/last-login-link`)
  ).json();

  await page.goto(link);
  await expect(page).toHaveURL(/\/home$/);

  return email;
}
```

Create `frontend/e2e/profile.spec.ts`:

```ts
import { expect, test } from "@playwright/test";
import { signIn } from "./helpers";

test("a founder fills in a profile and completes the assessment", async ({
  page,
  request,
}) => {
  await signIn(page, request, "ada");

  // A fresh account starts with nothing done.
  await expect(page.getByText("Answer the 18 work-style questions")).toBeVisible();

  await page.getByRole("link", { name: "Profile" }).click();
  await expect(page.getByRole("heading", { name: "Your profile" })).toBeVisible();

  await page.locator("#display_name").fill("Ada Lovelace");
  await page.locator("#headline").fill("Building tools for analytical engines");
  await page.locator("#bio").fill("Twenty years in numerical computing.");
  await page.locator("#city").fill("London");
  await page.locator("#country").fill("United Kingdom");
  await page.locator("#timezone").fill("Europe/London");
  await page.locator("#commitment").selectOption("full_time_now");

  await page
    .getByRole("group", { name: "Your strengths" })
    .getByRole("button", { name: "Engineering" })
    .click();
  await page
    .getByRole("group", { name: "Cofounder strengths" })
    .getByRole("button", { name: "GTM / Sales" })
    .click();
  await page
    .getByRole("group", { name: "Industries" })
    .getByRole("button", { name: "Developer tools" })
    .click();

  await page.getByRole("button", { name: "Save profile" }).click();
  await expect(page.locator("#profile-status")).toHaveText("Profile saved");

  // The profile is filled in but the assessment is not, so it is still incomplete.
  await expect(page.getByText("1 thing left before you appear in decks.")).toBeVisible();

  await page.getByRole("link", { name: "Assessment" }).click();
  await expect(page.locator("#assessment-progress")).toHaveText("0 of 18 answered");

  const questions = page.getByRole("listitem");
  const count = await questions.count();
  expect(count).toBe(18);

  for (let index = 0; index < count; index++) {
    await questions.nth(index).getByText("Neutral").click();
    await expect(page.locator("#assessment-progress")).toHaveText(
      index + 1 === 18 ? "All 18 answered" : `${index + 1} of 18 answered`,
    );
  }

  await page.getByRole("link", { name: "Home" }).click();
  await expect(page.getByText("Your profile is complete")).toBeVisible();
});

test("an answer survives a reload", async ({ page, request }) => {
  await signIn(page, request, "grace");

  await page.goto("/assessment");
  const questions = page.getByRole("listitem");
  await questions.first().getByText("Strongly agree").click();
  await expect(page.locator("#assessment-progress")).toHaveText("1 of 18 answered");

  await page.reload();
  await expect(page.locator("#assessment-progress")).toHaveText("1 of 18 answered");
});

test("the profile form surfaces a server-side error inline", async ({
  page,
  request,
}) => {
  await signIn(page, request, "hopper");

  await page.goto("/profile");
  await page.locator("#website_url").fill("javascript:alert(1)");
  await page.getByRole("button", { name: "Save profile" }).click();

  await expect(page.getByText("must start with http:// or https://")).toBeVisible();
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd frontend && npm run test:e2e -- profile.spec.ts`
Expected: FAIL — no `Profile` navigation link exists yet.

- [ ] **Step 3: Add navigation to the authenticated shell**

Modify `frontend/app/(app)/layout.tsx` to its full new contents:

```tsx
import Link from "next/link";
import { redirect } from "next/navigation";
import { getCurrentUser } from "@/lib/session";

export default async function AppLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const user = await getCurrentUser();
  if (!user) redirect("/login");

  return (
    <div className="min-h-screen">
      <header className="flex items-center justify-between border-b border-neutral-200 px-6 py-3">
        <nav className="flex items-center gap-4">
          <span className="font-semibold">Cofounder</span>
          <Link href="/home" className="text-sm text-neutral-700 hover:underline">
            Home
          </Link>
          <Link href="/profile" className="text-sm text-neutral-700 hover:underline">
            Profile
          </Link>
          <Link
            href="/assessment"
            className="text-sm text-neutral-700 hover:underline"
          >
            Assessment
          </Link>
        </nav>
        <span className="text-sm text-neutral-600">{user.email}</span>
      </header>
      <main className="p-6">{children}</main>
    </div>
  );
}
```

- [ ] **Step 4: Turn the home page into a completeness checklist**

Modify `frontend/app/(app)/home/page.tsx` to its full new contents:

```tsx
import Link from "next/link";
import { cookies } from "next/headers";
import { MISSING_LABELS, ProfileView } from "@/lib/profile";

const BACKEND_URL = process.env.BACKEND_URL ?? "http://localhost:8080";

/**
 * A server component cannot use the /api rewrite, so it calls the backend
 * directly and forwards the incoming cookie header — the same approach
 * lib/session.ts takes for /me.
 */
async function getProfileView(): Promise<ProfileView | null> {
  const cookieHeader = (await cookies()).toString();

  const response = await fetch(`${BACKEND_URL}/me/profile`, {
    headers: { cookie: cookieHeader },
    cache: "no-store",
  });

  if (!response.ok) return null;
  return (await response.json()) as ProfileView;
}

export default async function HomePage() {
  const view = await getProfileView();

  if (!view) {
    return (
      <p role="alert" className="text-red-600">
        Could not load your profile. Reload to try again.
      </p>
    );
  }

  if (view.complete) {
    return (
      <div className="flex flex-col gap-2">
        <h1 className="text-2xl font-semibold">Your profile is complete</h1>
        <p className="text-neutral-600">
          The swipe deck arrives in the next slice. Until then you can keep
          your profile and answers up to date.
        </p>
      </div>
    );
  }

  return (
    <div className="flex max-w-xl flex-col gap-4">
      <div>
        <h1 className="text-2xl font-semibold">Finish your profile</h1>
        <p className="mt-1 text-neutral-600">
          You will not appear in anyone&apos;s deck until all of this is done.
        </p>
      </div>

      <ul className="flex flex-col gap-2">
        {view.missing.map((item) => (
          <li key={item} className="flex items-center gap-2">
            <span aria-hidden className="text-neutral-400">
              ○
            </span>
            <Link
              href={item === "responses" ? "/assessment" : "/profile"}
              className="underline"
            >
              {MISSING_LABELS[item] ?? item}
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
```

- [ ] **Step 5: Run the end-to-end suite and verify it passes**

Run: `cd frontend && npm run test:e2e`
Expected: PASS — the slice-1 auth specs and the three new profile specs.

If Playwright cannot start the backend, confirm Postgres is running and
`backend/.env` has a `DATABASE_URL`; the config is unchanged from slice 1.

- [ ] **Step 6: Run every test in the repository**

Run: `cd backend && cargo test`
Expected: PASS.

Run: `cd frontend && npx tsc --noEmit && npm run lint`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add "frontend/app/(app)" frontend/e2e
git commit -m "feat: profile navigation, completeness checklist, and e2e coverage"
```

---

## Self-Review

Checked against `docs/superpowers/specs/2026-07-25-cofounder-matching-design.md`:

**Covered by this plan.** Data model rows `profiles`, `profile_interests`,
`question_responses`, `trait_scores` (Tasks 3 and 5). Profile identity, role,
situation, commitment, and interest fields (Task 5). The completeness rule
verbatim from the spec — bio, ≥1 role, ≥1 sought role, commitment, 18 answers
(Task 6). Eighteen Likert questions, three per axis, each axis carrying a
reverse item, defined in a single Rust constant (Task 1). Axis mean after
reversal mapped onto 0–100 (Task 2). `trait_scores` stored rather than
recomputed, and recalculated whenever an answer changes (Task 4). API surface
`GET /questions`, `GET`/`PUT /me/responses`, `GET`/`PUT /me/profile` (Tasks 4
and 6). RFC-7807 error handling and inline field errors reuse slice 1's
`ApiError` unchanged (Tasks 6 and 7). Assessment scoring tests including
reverse handling and boundaries, and repository tests against a throwaway
Postgres (Tasks 2, 3, 5).

**Deliberately deferred, with the reason.**
- *Profile photo* — the spec lists one under Identity. Excluded by decision
  recorded in Global Constraints: it needs object storage, which is a
  sub-project. No column is created, so adding it later is an additive
  migration.
- *Scoring, deck, swiping, messaging, moderation* — slices 3 and 4.
- *Playwright coverage of "swipe, match, message"* — the spec's end-to-end
  path. Only the parts that exist after this slice (sign up, complete
  questionnaire) are covered in Task 9; the rest follows its own slice.

**Deliberate addition beyond the spec.** `GET /options` is not in the spec's
API table. Without it the five fixed vocabularies would be duplicated in
TypeScript and drift from the Postgres `CHECK` constraints. Rationale is
recorded in a comment on the handler itself.

**Type consistency.** `TraitScores`'s six fields match the six `Axis::slug()`
values and the six `trait_scores` columns. `Response { question_id, value }` is
the same struct across repo, service, routes, and the JSON body. `ProfileRow`'s
fields match `ProfileInput`'s (minus `interests`), the `profiles` columns, and
`ProfileBody` in `frontend/lib/profile.ts`. The `missing` strings emitted by
`missing_requirements` — `bio`, `roles`, `seeking_roles`, `commitment`,
`responses` — are exactly the keys of `MISSING_LABELS`. `TOTAL_QUESTIONS` is
defined once in `assessment::service` and consumed by `profiles::service`.
