# Scoring & Deck Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A founder with a complete profile opens a deck of scored candidates, sees why each was surfaced, and swipes through them — with a mutual right swipe creating a match.

**Architecture:** A pure `scoring` module turns two `ScoredProfile` values into a `MatchScore` with a total out of 100 and human-readable reasons. It has no database access, no I/O, and no clock, so the whole point table is exhaustively unit-testable. A `deck` module does the I/O around it: one SQL query filters the candidate pool, the scorer ranks what comes back, two cheap swipe-history adjustments nudge the order, and the top 20 are returned. A `swipes` module records swipes and creates matches.

**Tech Stack:** Rust (axum 0.8, sqlx 0.8, tokio, chrono-tz), Postgres 16, Next.js 16.2 (App Router, TypeScript, Tailwind 4), Playwright.

This plan is slice 3 of 4 derived from `docs/superpowers/specs/2026-07-25-cofounder-matching-design.md`. Slices 1 (auth) and 2 (profile & assessment) are merged. Slice 4 adds messaging and moderation.

## Global Constraints

Carried over unchanged from slices 1 and 2:

- Rust edition 2021. Crate name `cofounder_api`, in `backend/`.
- **Use `sqlx::query_as` / `sqlx::query` / `sqlx::query_scalar`, never the `query!` macros.** They need a live database at compile time; the runtime-checked functions do not.
- All database access lives in `repo.rs` files. Handlers and services never write SQL.
- Timestamps are `TIMESTAMPTZ` in Postgres, `chrono::DateTime<chrono::Utc>` in Rust.
- Every route except `/auth/*` requires a session, obtained by taking `CurrentUser` as a handler argument.
- Errors are `ApiError`; validation failures are `ApiError::Validation(Vec<FieldError>)`, rendering 422 with per-field detail. Never construct ad-hoc error responses.
- Frontend is a client component calling the Rust API through the `/api` rewrite with `apiFetch`. No Server Actions — Next holds no domain logic.
- **Before writing any frontend code, read the relevant guide under `frontend/node_modules/next/dist/docs/`**, as `frontend/AGENTS.md` requires.
- **E2E tests must use `uniqueEmail(prefix)` from `frontend/e2e/helpers.ts`, never `Date.now()`.** Specs run in parallel workers; timestamped addresses collide and two tests then share one account.
- Commit after every task. Conventional-commit prefixes (`feat:`, `test:`, `chore:`).

Specific to this slice:

- **The scorer is pure.** `score()` takes two `ScoredProfile` values and returns a `MatchScore`. No `PgPool`, no `now()`, no `Tz` lookup. Anything needing a clock or the database belongs in `deck/`.
- **Timezone offsets are resolved at save time, not at score time.** `profiles.utc_offset_minutes` is derived from the IANA name when the profile is written. An IANA zone's offset moves with DST, so resolving it inside the scorer would make the same pair score differently in March than in July.
- **The `blocks` table is created in this slice**, and the deck's candidate query excludes blocked pairs in both directions from the start. `POST /blocks` arrives in slice 4; until then the table is simply empty. The deck's most safety-critical filter is not something to bolt on later.
- **The point budget is exactly 100**: roles 30, traits 25, situation 20, interests 15, geography 10. A test asserts the components sum to 100 for a perfect pair.
- **Deck interaction is buttons plus keyboard arrows, not drag gestures.** Drag is a large amount of frontend work that Playwright cannot exercise meaningfully. The card renders one candidate at a time with Pass / Interested controls.
- `trait_scores` presence is the completeness signal the deck query joins on — a row exists iff all 18 answers do. Do not re-derive completeness by counting `question_responses`.

## File Structure

```
backend/
  Cargo.toml                      + chrono-tz
  migrations/
    0008_profile_utc_offset.sql   derived offset column
    0009_swipes.sql               swiper, target, direction
    0010_matches.sql              ordered pair, unique
    0011_blocks.sql               table only; endpoint is slice 4
  src/
    lib.rs                        + pub mod deck; pub mod scoring; pub mod swipes;
    app.rs                        + merge the two new routers
    error.rs                      + ApiError::Conflict
    profiles/
      service.rs                  + timezone validation and offset derivation
      timezone.rs                 IANA name -> fixed UTC offset
    scoring/
      mod.rs
      profile.rs                  ScoredProfile, MatchScore, Reason, Component
      roles.rs                    role complementarity (30)
      traits.rs                   trait fit (25) and the per-axis ideal table
      situation.rs                commitment and stage (20)
      interests.rs                interest overlap (15)
      geography.rs                geography (10)
      score.rs                    assembly, total, top-three reasons
    deck/
      mod.rs
      repo.rs                     candidate query, swipe-history queries
      service.rs                  load, score, adjust, take 20
      routes.rs                   GET /deck
    swipes/
      mod.rs
      repo.rs                     record swipe, create match, list matches
      routes.rs                   POST /swipes, GET /matches
  tests/
    scoring_roles.rs              role component
    scoring_traits.rs             the similarity-vs-complementarity table
    scoring_situation.rs          commitment and stage
    scoring_interests.rs          Jaccard
    scoring_geography.rs          metro, timezone bands
    scoring_total.rs              assembly, budget, reasons
    profile_timezone.rs           offset derivation and validation
    deck_repo.rs                  candidate filtering
    deck_api.rs                   GET /deck
    swipes_api.rs                 POST /swipes, GET /matches

frontend/
  lib/deck.ts                     shared types
  app/(app)/deck/page.tsx         server shell
  app/(app)/deck/deck-client.tsx  card, controls, match moment
  app/(app)/matches/page.tsx      server shell
  app/(app)/matches/matches-client.tsx
  app/(app)/layout.tsx            + Deck and Matches links
  e2e/deck.spec.ts                two founders, a mutual swipe, a match
```

One file per scoring component keeps each one small enough to hold in context alongside its test file, and the point budget stays visible in `score.rs` rather than being spread through a single large function.

---

### Task 1: Scoring types and role complementarity

**Files:**
- Create: `backend/src/scoring/mod.rs`, `backend/src/scoring/profile.rs`, `backend/src/scoring/roles.rs`
- Modify: `backend/src/lib.rs`, `backend/src/profiles/vocab.rs`
- Test: `backend/tests/scoring_roles.rs`

**Interfaces:**
- Consumes: `assessment::scoring::TraitScores`, `profiles::vocab::{Choice, ROLES}`
- Produces:
  - `cofounder_api::scoring::profile::{ScoredProfile, MatchScore, Reason, Component, ComponentScore}`
  - `cofounder_api::scoring::roles::score_roles(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore`
  - `cofounder_api::profiles::vocab::label(choices: &[Choice], id: &str) -> Option<&'static str>`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/scoring_roles.rs`:

```rust
use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::profile::{Component, ScoredProfile};
use cofounder_api::scoring::roles::score_roles;
use uuid::Uuid;

pub fn neutral_traits() -> TraitScores {
    TraitScores {
        risk_tolerance: 50,
        pace_vs_rigor: 50,
        conflict_style: 50,
        decision_basis: 50,
        work_mode: 50,
        orientation: 50,
    }
}

fn profile(roles: &[&str], seeking: &[&str]) -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: roles.iter().map(|r| r.to_string()).collect(),
        seeking_roles: seeking.iter().map(|r| r.to_string()).collect(),
        interests: Vec::new(),
        idea_status: None,
        stage: None,
        commitment: None,
        city: String::new(),
        country: String::new(),
        utc_offset_minutes: None,
        traits: neutral_traits(),
    }
}

#[test]
fn a_mutual_fit_scores_the_whole_budget() {
    // The archetypal good result: a builder who wants a seller, matched with
    // a seller who wants a builder.
    let viewer = profile(&["engineering"], &["gtm"]);
    let candidate = profile(&["gtm"], &["engineering"]);

    let result = score_roles(&viewer, &candidate);

    assert_eq!(result.component, Component::Roles);
    assert!((result.points - 30.0).abs() < 0.001, "got {}", result.points);
}

#[test]
fn a_one_sided_fit_scores_half() {
    // They have what the viewer wants, but do not want what the viewer has.
    let viewer = profile(&["engineering"], &["gtm"]);
    let candidate = profile(&["gtm"], &["design"]);

    let result = score_roles(&viewer, &candidate);

    assert!((result.points - 15.0).abs() < 0.001, "got {}", result.points);
}

#[test]
fn no_overlap_scores_nothing() {
    let viewer = profile(&["engineering"], &["gtm"]);
    let candidate = profile(&["research"], &["design"]);

    let result = score_roles(&viewer, &candidate);

    assert_eq!(result.points, 0.0);
    assert!(result.reason.is_none());
}

#[test]
fn covering_more_of_what_is_wanted_scores_higher() {
    let viewer = profile(&["engineering"], &["gtm", "design"]);
    let covers_one = profile(&["gtm"], &[]);
    let covers_both = profile(&["gtm", "design"], &[]);

    let partial = score_roles(&viewer, &covers_one).points;
    let full = score_roles(&viewer, &covers_both).points;

    assert!((partial - 7.5).abs() < 0.001, "got {partial}");
    assert!((full - 15.0).abs() < 0.001, "got {full}");
}

#[test]
fn seeking_nothing_earns_nothing_from_that_direction(
) {
    // An empty wishlist cannot be satisfied; it must not divide by zero.
    let viewer = profile(&["engineering"], &[]);
    let candidate = profile(&["gtm"], &["engineering"]);

    let result = score_roles(&viewer, &candidate);

    assert!((result.points - 15.0).abs() < 0.001, "got {}", result.points);
}

#[test]
fn a_mutual_fit_explains_both_directions() {
    let viewer = profile(&["engineering"], &["gtm"]);
    let candidate = profile(&["gtm"], &["engineering"]);

    let reason = score_roles(&viewer, &candidate).reason.expect("a reason");

    assert!(reason.contains("GTM / Sales"), "got {reason}");
    assert!(reason.contains("Engineering"), "got {reason}");
}

#[test]
fn a_one_sided_fit_explains_only_that_direction() {
    let viewer = profile(&["engineering"], &["gtm"]);
    let candidate = profile(&["gtm"], &["design"]);

    let reason = score_roles(&viewer, &candidate).reason.expect("a reason");

    assert!(reason.contains("GTM / Sales"), "got {reason}");
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test scoring_roles`
Expected: FAIL — `could not find scoring in cofounder_api`.

- [ ] **Step 3: Add the label lookup to the vocabulary**

Modify `backend/src/profiles/vocab.rs` — append below the existing `contains` function:

```rust
pub fn label(choices: &[Choice], id: &str) -> Option<&'static str> {
    choices
        .iter()
        .find(|choice| choice.id == id)
        .map(|choice| choice.label)
}
```

- [ ] **Step 4: Define the scoring types**

Create `backend/src/scoring/mod.rs`:

```rust
pub mod profile;
pub mod roles;
```

Create `backend/src/scoring/profile.rs`:

```rust
use uuid::Uuid;

use crate::assessment::scoring::TraitScores;

/// Everything the scorer is allowed to see. Assembled by `deck::repo` from
/// `profiles`, `profile_interests` and `trait_scores`; the scorer itself
/// never touches the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredProfile {
    pub user_id: Uuid,
    pub display_name: String,
    pub roles: Vec<String>,
    pub seeking_roles: Vec<String>,
    pub interests: Vec<String>,
    pub idea_status: Option<String>,
    pub stage: Option<String>,
    pub commitment: Option<String>,
    pub city: String,
    pub country: String,
    /// Derived from the IANA name when the profile is saved. `None` for a
    /// profile written before the column existed, or one with no timezone.
    pub utc_offset_minutes: Option<i16>,
    pub traits: TraitScores,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    Roles,
    Traits,
    Situation,
    Interests,
    Geography,
}

/// One component's contribution, with the sentence that explains it. The
/// reason is produced here rather than reconstructed later, so what a card
/// says can never drift from what actually scored.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentScore {
    pub component: Component,
    pub points: f64,
    pub reason: Option<String>,
}

impl ComponentScore {
    pub fn new(component: Component, points: f64, reason: Option<String>) -> Self {
        Self {
            component,
            points,
            reason,
        }
    }

    pub fn empty(component: Component) -> Self {
        Self {
            component,
            points: 0.0,
            reason: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Reason {
    pub component: Component,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MatchScore {
    pub total: u16,
    pub reasons: Vec<Reason>,
}
```

Modify `backend/src/lib.rs` — add `pub mod scoring;` so the list reads:

```rust
pub mod app;
pub mod assessment;
pub mod auth;
pub mod config;
pub mod db;
pub mod email;
pub mod error;
pub mod profiles;
pub mod scoring;
pub mod users;
```

- [ ] **Step 5: Implement role complementarity**

Create `backend/src/scoring/roles.rs`:

```rust
use std::collections::HashSet;

use crate::profiles::vocab::ROLES;
use crate::scoring::profile::{Component, ComponentScore, ScoredProfile};

pub const MAX_POINTS: f64 = 30.0;
const PER_DIRECTION: f64 = MAX_POINTS / 2.0;

/// What fraction of `wanted` is covered by `offered`, and the first covered
/// role in the vocabulary's own order so the explanation is deterministic.
fn coverage(offered: &[String], wanted: &[String]) -> (f64, Option<&'static str>) {
    if wanted.is_empty() {
        return (0.0, None);
    }

    let offered: HashSet<&str> = offered.iter().map(String::as_str).collect();
    let matched: Vec<&str> = wanted
        .iter()
        .map(String::as_str)
        .filter(|role| offered.contains(role))
        .collect();

    if matched.is_empty() {
        return (0.0, None);
    }

    let fraction = matched.len() as f64 / wanted.len() as f64;

    // Ordered by the vocabulary rather than by the user's array so that the
    // same pair always produces the same sentence.
    let first = ROLES
        .iter()
        .find(|choice| matched.contains(&choice.id))
        .map(|choice| choice.label);

    (fraction.min(1.0), first)
}

/// The strongest single signal in the model: a technical founder seeking GTM
/// matched with a GTM founder seeking technical.
pub fn score_roles(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore {
    let (they_offer, they_bring) = coverage(&candidate.roles, &viewer.seeking_roles);
    let (you_offer, you_bring) = coverage(&viewer.roles, &candidate.seeking_roles);

    let points = they_offer * PER_DIRECTION + you_offer * PER_DIRECTION;

    let reason = match (they_bring, you_bring) {
        (Some(theirs), Some(yours)) => {
            Some(format!("They bring {theirs}, you bring {yours}"))
        }
        (Some(theirs), None) => Some(format!("Brings {theirs}, which you're after")),
        (None, Some(yours)) => Some(format!("Looking for {yours}, which you bring")),
        (None, None) => None,
    };

    ComponentScore::new(Component::Roles, points, reason)
}
```

Note the import line is `use crate::profiles::vocab::ROLES;` — this module reads the vocabulary's order but not the `label` helper, which Task 3's situation component uses.

- [ ] **Step 6: Run the tests and verify they pass**

Run: `cd backend && cargo test --test scoring_roles`
Expected: PASS — 7 tests.

- [ ] **Step 7: Commit**

```bash
git add backend/src/scoring backend/src/lib.rs backend/src/profiles/vocab.rs backend/tests/scoring_roles.rs
git commit -m "feat: scoring types and role complementarity"
git push origin main
```

---

### Task 2: Trait fit and the similarity-versus-complementarity table

This is the subtlest logic in the product: a table that is easy to invert and that stays silently wrong if it is. It gets its own task and its own test file.

**Files:**
- Create: `backend/src/scoring/traits.rs`
- Modify: `backend/src/scoring/mod.rs`, `backend/src/assessment/questions.rs`
- Test: `backend/tests/scoring_traits.rs`

**Interfaces:**
- Consumes: `scoring::profile::{ScoredProfile, ComponentScore, Component}`, `assessment::questions::Axis`, `assessment::scoring::TraitScores`
- Produces:
  - `cofounder_api::scoring::traits::{score_traits, Ideal, ideal_for, MAX_POINTS}`
  - `Ideal` — enum `Similar`, `MildDifference`, `Complementary`; `Ideal::target_distance(self) -> f64`
  - `ideal_for(axis: Axis) -> Ideal`
  - `cofounder_api::assessment::questions::Axis::{low_label, high_label}`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/scoring_traits.rs`:

```rust
use cofounder_api::assessment::questions::Axis;
use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::profile::ScoredProfile;
use cofounder_api::scoring::traits::{ideal_for, score_traits, Ideal, MAX_POINTS};
use uuid::Uuid;

fn with_traits(traits: TraitScores) -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: Vec::new(),
        seeking_roles: Vec::new(),
        interests: Vec::new(),
        idea_status: None,
        stage: None,
        commitment: None,
        city: String::new(),
        country: String::new(),
        utc_offset_minutes: None,
        traits,
    }
}

fn uniform(value: i16) -> TraitScores {
    TraitScores {
        risk_tolerance: value,
        pace_vs_rigor: value,
        conflict_style: value,
        decision_basis: value,
        work_mode: value,
        orientation: value,
    }
}

// The table straight out of the design document. If an axis is ever flipped,
// this is the test that says so.
#[test]
fn each_axis_wants_what_the_design_says_it_wants() {
    assert_eq!(ideal_for(Axis::RiskTolerance), Ideal::Similar);
    assert_eq!(ideal_for(Axis::PaceVsRigor), Ideal::Similar);
    assert_eq!(ideal_for(Axis::ConflictStyle), Ideal::Similar);
    assert_eq!(ideal_for(Axis::DecisionBasis), Ideal::MildDifference);
    assert_eq!(ideal_for(Axis::WorkMode), Ideal::Complementary);
    assert_eq!(ideal_for(Axis::Orientation), Ideal::Complementary);
}

#[test]
fn the_ideal_distances_are_ordered() {
    assert!(Ideal::Similar.target_distance() < Ideal::MildDifference.target_distance());
    assert!(
        Ideal::MildDifference.target_distance() < Ideal::Complementary.target_distance()
    );
}

#[test]
fn a_pair_sitting_exactly_on_every_ideal_scores_the_whole_budget() {
    // Similar axes identical; decision_basis 25 apart; the two complementary
    // axes 60 apart.
    let viewer = with_traits(TraitScores {
        risk_tolerance: 50,
        pace_vs_rigor: 50,
        conflict_style: 50,
        decision_basis: 40,
        work_mode: 20,
        orientation: 20,
    });
    let candidate = with_traits(TraitScores {
        risk_tolerance: 50,
        pace_vs_rigor: 50,
        conflict_style: 50,
        decision_basis: 65,
        work_mode: 80,
        orientation: 80,
    });

    let result = score_traits(&viewer, &candidate);

    assert!(
        (result.points - MAX_POINTS).abs() < 0.001,
        "got {}",
        result.points
    );
}

#[test]
fn identical_people_do_not_score_full_marks() {
    // Two identical profiles are perfect on the similar axes and poor on the
    // complementary ones. If this ever reaches the maximum, the
    // complementary axes have been treated as similarity axes.
    let viewer = with_traits(uniform(50));
    let candidate = with_traits(uniform(50));

    let result = score_traits(&viewer, &candidate);

    assert!(result.points < MAX_POINTS, "got {}", result.points);
    assert!(result.points > 0.0, "got {}", result.points);
}

#[test]
fn opposites_beat_twins_on_a_complementary_axis() {
    let twin_a = with_traits(TraitScores {
        work_mode: 50,
        ..uniform(50)
    });
    let twin_b = with_traits(TraitScores {
        work_mode: 50,
        ..uniform(50)
    });
    let deep = with_traits(TraitScores {
        work_mode: 20,
        ..uniform(50)
    });
    let social = with_traits(TraitScores {
        work_mode: 80,
        ..uniform(50)
    });

    let twins = score_traits(&twin_a, &twin_b).points;
    let opposites = score_traits(&deep, &social).points;

    assert!(
        opposites > twins,
        "complementary axis: opposites {opposites} should beat twins {twins}"
    );
}

#[test]
fn twins_beat_opposites_on_a_similarity_axis() {
    let twin_a = with_traits(uniform(50));
    let twin_b = with_traits(uniform(50));
    let cautious = with_traits(TraitScores {
        risk_tolerance: 10,
        ..uniform(50)
    });
    let bold = with_traits(TraitScores {
        risk_tolerance: 90,
        ..uniform(50)
    });

    let twins = score_traits(&twin_a, &twin_b).points;
    let opposites = score_traits(&cautious, &bold).points;

    assert!(
        twins > opposites,
        "similarity axis: twins {twins} should beat opposites {opposites}"
    );
}

#[test]
fn scoring_is_symmetric() {
    let a = with_traits(TraitScores {
        risk_tolerance: 30,
        pace_vs_rigor: 70,
        conflict_style: 10,
        decision_basis: 90,
        work_mode: 25,
        orientation: 80,
    });
    let b = with_traits(TraitScores {
        risk_tolerance: 45,
        pace_vs_rigor: 55,
        conflict_style: 60,
        decision_basis: 20,
        work_mode: 75,
        orientation: 15,
    });

    let forwards = score_traits(&a, &b).points;
    let backwards = score_traits(&b, &a).points;

    assert!((forwards - backwards).abs() < 0.001);
}

#[test]
fn the_worst_possible_pair_scores_nothing_on_the_similar_axes() {
    let cautious = with_traits(uniform(0));
    let bold = with_traits(uniform(100));

    let result = score_traits(&cautious, &bold);

    // Every similar axis is 100 apart, so those three contribute zero; the
    // complementary axes are overshot but not by the full range.
    assert!(result.points < MAX_POINTS / 2.0, "got {}", result.points);
}

#[test]
fn a_similar_axis_is_explained_by_what_the_pair_shares() {
    let ship_fast_a = with_traits(TraitScores {
        pace_vs_rigor: 90,
        ..uniform(50)
    });
    let ship_fast_b = with_traits(TraitScores {
        pace_vs_rigor: 88,
        ..uniform(50)
    });

    let reason = score_traits(&ship_fast_a, &ship_fast_b)
        .reason
        .expect("a reason");

    assert!(reason.starts_with("Both "), "got {reason}");
    assert!(reason.contains("ship it now"), "got {reason}");
}

#[test]
fn a_complementary_axis_is_explained_as_a_pairing() {
    let deep = with_traits(TraitScores {
        work_mode: 15,
        ..uniform(50)
    });
    let social = with_traits(TraitScores {
        work_mode: 85,
        ..uniform(50)
    });

    let reason = score_traits(&deep, &social).reason.expect("a reason");

    assert!(reason.contains("deep solo work"), "got {reason}");
    assert!(reason.contains("constant collaboration"), "got {reason}");
}

#[test]
fn every_axis_has_labels_for_both_ends() {
    for axis in Axis::ALL {
        assert!(!axis.low_label().is_empty(), "{}", axis.slug());
        assert!(!axis.high_label().is_empty(), "{}", axis.slug());
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test scoring_traits`
Expected: FAIL — `could not find traits in scoring`.

- [ ] **Step 3: Add the axis end labels**

Modify `backend/src/assessment/questions.rs` — add these two methods inside the existing `impl Axis` block, below `slug`:

```rust
    /// The wording used on cards. Taken from the design document's axis
    /// table so an explanation cannot describe an axis differently from the
    /// way it is scored.
    pub fn low_label(self) -> &'static str {
        match self {
            Axis::RiskTolerance => "de-risk before committing",
            Axis::PaceVsRigor => "build it right",
            Axis::ConflictStyle => "seek harmony",
            Axis::DecisionBasis => "trust intuition",
            Axis::WorkMode => "deep solo work",
            Axis::Orientation => "near-term execution",
        }
    }

    pub fn high_label(self) -> &'static str {
        match self {
            Axis::RiskTolerance => "bet big early",
            Axis::PaceVsRigor => "ship it now",
            Axis::ConflictStyle => "address directly",
            Axis::DecisionBasis => "require data",
            Axis::WorkMode => "constant collaboration",
            Axis::Orientation => "long-range vision",
        }
    }
```

- [ ] **Step 4: Implement trait fit**

Create `backend/src/scoring/traits.rs`:

```rust
use crate::assessment::questions::Axis;
use crate::assessment::scoring::TraitScores;
use crate::scoring::profile::{Component, ComponentScore, ScoredProfile};

pub const MAX_POINTS: f64 = 25.0;

/// What a healthy pair looks like on one axis. Distances are on the same
/// 0–100 scale as the axis scores themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ideal {
    Similar,
    MildDifference,
    Complementary,
}

impl Ideal {
    pub fn target_distance(self) -> f64 {
        match self {
            Ideal::Similar => 0.0,
            Ideal::MildDifference => 25.0,
            Ideal::Complementary => 60.0,
        }
    }
}

/// The design document's table, verbatim:
///
/// | axis           | ideal           | why                                       |
/// |----------------|-----------------|-------------------------------------------|
/// | risk_tolerance | similar         | divergent risk appetite breaks cofounders |
/// | pace_vs_rigor  | similar         | ship-fast and build-right partners fight  |
/// | conflict_style | similar         | mismatched styles breed resentment        |
/// | decision_basis | mild difference | instinct against evidence is productive   |
/// | work_mode      | complementary   | one goes deep, one runs relationships     |
/// | orientation    | complementary   | vision and execution is the classic pair  |
pub fn ideal_for(axis: Axis) -> Ideal {
    match axis {
        Axis::RiskTolerance => Ideal::Similar,
        Axis::PaceVsRigor => Ideal::Similar,
        Axis::ConflictStyle => Ideal::Similar,
        Axis::DecisionBasis => Ideal::MildDifference,
        Axis::WorkMode => Ideal::Complementary,
        Axis::Orientation => Ideal::Complementary,
    }
}

fn value_of(traits: &TraitScores, axis: Axis) -> f64 {
    let raw = match axis {
        Axis::RiskTolerance => traits.risk_tolerance,
        Axis::PaceVsRigor => traits.pace_vs_rigor,
        Axis::ConflictStyle => traits.conflict_style,
        Axis::DecisionBasis => traits.decision_basis,
        Axis::WorkMode => traits.work_mode,
        Axis::Orientation => traits.orientation,
    };
    f64::from(raw)
}

/// How well one axis sits against its ideal: 1.0 exactly on target, falling
/// linearly to 0.0 when it is the full range away from it.
fn axis_fraction(viewer: &TraitScores, candidate: &TraitScores, axis: Axis) -> f64 {
    let distance = (value_of(viewer, axis) - value_of(candidate, axis)).abs();
    let miss = (distance - ideal_for(axis).target_distance()).abs();
    (1.0 - miss / 100.0).clamp(0.0, 1.0)
}

fn explain(viewer: &TraitScores, candidate: &TraitScores, axis: Axis) -> String {
    let a = value_of(viewer, axis);
    let b = value_of(candidate, axis);

    match ideal_for(axis) {
        Ideal::Complementary => {
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            let _ = (low, high);
            format!("{} meets {}", axis.low_label(), axis.high_label())
        }
        Ideal::Similar | Ideal::MildDifference => {
            let midpoint = (a + b) / 2.0;
            if midpoint >= 50.0 {
                format!("Both {}", axis.high_label())
            } else {
                format!("Both {}", axis.low_label())
            }
        }
    }
}

/// Axes are equally weighted within the budget. Each contributes in
/// proportion to how close the pair's actual distance sits to that axis's
/// ideal distance — which is not the same thing as being alike.
pub fn score_traits(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore {
    let per_axis = MAX_POINTS / Axis::ALL.len() as f64;

    let mut points = 0.0;
    let mut best: Option<(f64, Axis)> = None;

    for axis in Axis::ALL {
        let fraction = axis_fraction(&viewer.traits, &candidate.traits, axis);
        points += fraction * per_axis;

        if best.is_none_or(|(top, _)| fraction > top) {
            best = Some((fraction, axis));
        }
    }

    // Only explain an axis that actually sits well; a card should not boast
    // about the least bad of six poor fits.
    let reason = best
        .filter(|(fraction, _)| *fraction >= 0.75)
        .map(|(_, axis)| explain(&viewer.traits, &candidate.traits, axis));

    ComponentScore::new(Component::Traits, points, reason)
}
```

Modify `backend/src/scoring/mod.rs`:

```rust
pub mod profile;
pub mod roles;
pub mod traits;
```

- [ ] **Step 5: Run the tests and verify they pass**

Run: `cd backend && cargo test --test scoring_traits`
Expected: PASS — 11 tests.

If `is_none_or` is rejected by the toolchain, replace `best.is_none_or(|(top, _)| fraction > top)` with `best.map_or(true, |(top, _)| fraction > top)`.

- [ ] **Step 6: Commit**

```bash
git add backend/src/scoring backend/src/assessment/questions.rs backend/tests/scoring_traits.rs
git commit -m "feat: trait fit with the per-axis ideal-distance table"
git push origin main
```

---

### Task 3: Situation, interests, and geography

Three small components, each a handful of lines, each with its own test file.

**Files:**
- Create: `backend/src/scoring/situation.rs`, `backend/src/scoring/interests.rs`, `backend/src/scoring/geography.rs`
- Modify: `backend/src/scoring/mod.rs`
- Test: `backend/tests/scoring_situation.rs`, `backend/tests/scoring_interests.rs`, `backend/tests/scoring_geography.rs`

**Interfaces:**
- Consumes: `scoring::profile::{ScoredProfile, ComponentScore, Component}`, `profiles::vocab::{self, COMMITMENTS, STAGES, INTERESTS, label}`
- Produces:
  - `cofounder_api::scoring::situation::{score_situation, MAX_POINTS}`
  - `cofounder_api::scoring::interests::{score_interests, MAX_POINTS}`
  - `cofounder_api::scoring::geography::{score_geography, MAX_POINTS, MAX_PARTIAL_OFFSET_MINUTES}`

- [ ] **Step 1: Write the failing tests**

Create `backend/tests/scoring_situation.rs`:

```rust
use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::profile::ScoredProfile;
use cofounder_api::scoring::situation::{score_situation, MAX_POINTS};
use uuid::Uuid;

fn profile(commitment: Option<&str>, stage: Option<&str>) -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: Vec::new(),
        seeking_roles: Vec::new(),
        interests: Vec::new(),
        idea_status: None,
        stage: stage.map(str::to_string),
        commitment: commitment.map(str::to_string),
        city: String::new(),
        country: String::new(),
        utc_offset_minutes: None,
        traits: TraitScores {
            risk_tolerance: 50,
            pace_vs_rigor: 50,
            conflict_style: 50,
            decision_basis: 50,
            work_mode: 50,
            orientation: 50,
        },
    }
}

#[test]
fn an_identical_situation_scores_the_whole_budget() {
    let viewer = profile(Some("full_time_now"), Some("prototype"));
    let candidate = profile(Some("full_time_now"), Some("prototype"));

    let result = score_situation(&viewer, &candidate);

    assert!(
        (result.points - MAX_POINTS).abs() < 0.001,
        "got {}",
        result.points
    );
}

#[test]
fn adjacent_commitment_levels_score_most_of_it() {
    let now = profile(Some("full_time_now"), Some("prototype"));
    let when_funded = profile(Some("full_time_when_funded"), Some("prototype"));

    let adjacent = score_situation(&now, &when_funded).points;
    let identical = score_situation(&now, &now).points;

    assert!(adjacent < identical, "{adjacent} should be under {identical}");
    assert!(adjacent > identical * 0.7, "{adjacent} is too harsh");
}

#[test]
fn distant_commitment_levels_score_near_zero_on_that_part() {
    // Full-time now against exploring is the classic doomed pairing.
    let now = profile(Some("full_time_now"), Some("prototype"));
    let exploring = profile(Some("exploring"), Some("prototype"));

    let result = score_situation(&now, &exploring);

    // The stage half still scores; the commitment half does not.
    assert!(result.points < MAX_POINTS / 2.0, "got {}", result.points);
    assert!(result.points > 0.0, "got {}", result.points);
}

#[test]
fn a_mismatch_is_penalised_never_filtered() {
    // Even the worst situation returns a score rather than an absence, so a
    // strong fit elsewhere can still outweigh it.
    let now = profile(Some("full_time_now"), Some("idea"));
    let exploring = profile(Some("exploring"), Some("revenue"));

    let result = score_situation(&now, &exploring);

    assert!(result.points >= 0.0);
    assert!(result.points < MAX_POINTS);
}

#[test]
fn commitment_counts_for_more_than_stage() {
    let base = profile(Some("full_time_now"), Some("idea"));
    let stage_differs = profile(Some("full_time_now"), Some("revenue"));
    let commitment_differs = profile(Some("exploring"), Some("idea"));

    let losing_stage = score_situation(&base, &stage_differs).points;
    let losing_commitment = score_situation(&base, &commitment_differs).points;

    assert!(
        losing_commitment < losing_stage,
        "commitment {losing_commitment} should cost more than stage {losing_stage}"
    );
}

#[test]
fn an_unset_commitment_scores_nothing_for_that_half() {
    let viewer = profile(None, Some("prototype"));
    let candidate = profile(Some("full_time_now"), Some("prototype"));

    let result = score_situation(&viewer, &candidate);

    assert!(result.points < MAX_POINTS, "got {}", result.points);
}

#[test]
fn an_identical_commitment_is_explained() {
    let viewer = profile(Some("full_time_now"), Some("prototype"));
    let candidate = profile(Some("full_time_now"), Some("prototype"));

    let reason = score_situation(&viewer, &candidate).reason.expect("a reason");

    assert!(reason.contains("Full-time now"), "got {reason}");
}

#[test]
fn a_distant_pair_is_not_explained() {
    let viewer = profile(Some("full_time_now"), Some("idea"));
    let candidate = profile(Some("exploring"), Some("revenue"));

    assert!(score_situation(&viewer, &candidate).reason.is_none());
}
```

Create `backend/tests/scoring_interests.rs`:

```rust
use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::interests::{score_interests, MAX_POINTS};
use cofounder_api::scoring::profile::ScoredProfile;
use uuid::Uuid;

fn profile(interests: &[&str]) -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: Vec::new(),
        seeking_roles: Vec::new(),
        interests: interests.iter().map(|i| i.to_string()).collect(),
        idea_status: None,
        stage: None,
        commitment: None,
        city: String::new(),
        country: String::new(),
        utc_offset_minutes: None,
        traits: TraitScores {
            risk_tolerance: 50,
            pace_vs_rigor: 50,
            conflict_style: 50,
            decision_basis: 50,
            work_mode: 50,
            orientation: 50,
        },
    }
}

#[test]
fn identical_interests_score_the_whole_budget() {
    let viewer = profile(&["ai_ml", "saas"]);
    let candidate = profile(&["ai_ml", "saas"]);

    let result = score_interests(&viewer, &candidate);

    assert!(
        (result.points - MAX_POINTS).abs() < 0.001,
        "got {}",
        result.points
    );
}

#[test]
fn jaccard_similarity_is_used() {
    // One shared out of two distinct: 1/2 of the budget.
    let viewer = profile(&["ai_ml", "saas"]);
    let candidate = profile(&["ai_ml"]);

    let result = score_interests(&viewer, &candidate);

    assert!(
        (result.points - MAX_POINTS / 2.0).abs() < 0.001,
        "got {}",
        result.points
    );
}

#[test]
fn disjoint_interests_score_nothing() {
    let viewer = profile(&["ai_ml"]);
    let candidate = profile(&["climate"]);

    let result = score_interests(&viewer, &candidate);

    assert_eq!(result.points, 0.0);
    assert!(result.reason.is_none());
}

#[test]
fn two_people_with_no_interests_score_nothing_rather_than_everything() {
    // An empty union must not be read as a perfect overlap.
    let viewer = profile(&[]);
    let candidate = profile(&[]);

    let result = score_interests(&viewer, &candidate);

    assert_eq!(result.points, 0.0);
    assert!(result.reason.is_none());
}

#[test]
fn scoring_is_symmetric() {
    let a = profile(&["ai_ml", "saas", "fintech"]);
    let b = profile(&["saas"]);

    let forwards = score_interests(&a, &b).points;
    let backwards = score_interests(&b, &a).points;

    assert!((forwards - backwards).abs() < 0.001);
}

#[test]
fn a_shared_interest_is_named_by_its_label() {
    let viewer = profile(&["ai_ml"]);
    let candidate = profile(&["ai_ml"]);

    let reason = score_interests(&viewer, &candidate).reason.expect("a reason");

    assert!(reason.contains("AI / ML"), "got {reason}");
}
```

Create `backend/tests/scoring_geography.rs`:

```rust
use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::geography::{
    score_geography, MAX_PARTIAL_OFFSET_MINUTES, MAX_POINTS,
};
use cofounder_api::scoring::profile::ScoredProfile;
use uuid::Uuid;

fn profile(city: &str, country: &str, offset: Option<i16>) -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: Vec::new(),
        seeking_roles: Vec::new(),
        interests: Vec::new(),
        idea_status: None,
        stage: None,
        commitment: None,
        city: city.into(),
        country: country.into(),
        utc_offset_minutes: offset,
        traits: TraitScores {
            risk_tolerance: 50,
            pace_vs_rigor: 50,
            conflict_style: 50,
            decision_basis: 50,
            work_mode: 50,
            orientation: 50,
        },
    }
}

#[test]
fn the_same_metro_scores_the_whole_budget() {
    let viewer = profile("Jakarta", "Indonesia", Some(420));
    let candidate = profile("Jakarta", "Indonesia", Some(420));

    let result = score_geography(&viewer, &candidate);

    assert!(
        (result.points - MAX_POINTS).abs() < 0.001,
        "got {}",
        result.points
    );
}

#[test]
fn the_same_metro_is_matched_case_insensitively() {
    let viewer = profile("Jakarta", "Indonesia", Some(420));
    let candidate = profile("  jakarta ", "INDONESIA", Some(420));

    let result = score_geography(&viewer, &candidate);

    assert!((result.points - MAX_POINTS).abs() < 0.001);
}

#[test]
fn the_same_city_name_in_another_country_is_not_the_same_metro() {
    let viewer = profile("Cambridge", "United Kingdom", Some(0));
    let candidate = profile("Cambridge", "United States", Some(-300));

    let result = score_geography(&viewer, &candidate);

    assert!(result.points < MAX_POINTS, "got {}", result.points);
}

#[test]
fn nearby_timezones_score_partially() {
    let viewer = profile("Jakarta", "Indonesia", Some(420));
    let candidate = profile("Singapore", "Singapore", Some(480));

    let result = score_geography(&viewer, &candidate);

    assert!(result.points > 0.0, "got {}", result.points);
    assert!(result.points < MAX_POINTS, "got {}", result.points);
}

#[test]
fn the_edge_of_the_band_still_counts() {
    let viewer = profile("London", "United Kingdom", Some(0));
    let candidate = profile("Dubai", "UAE", Some(MAX_PARTIAL_OFFSET_MINUTES));

    assert!(score_geography(&viewer, &candidate).points > 0.0);
}

#[test]
fn beyond_the_band_scores_nothing() {
    let viewer = profile("London", "United Kingdom", Some(0));
    let candidate = profile(
        "San Francisco",
        "United States",
        Some(MAX_PARTIAL_OFFSET_MINUTES + 1),
    );

    let result = score_geography(&viewer, &candidate);

    assert_eq!(result.points, 0.0);
    assert!(result.reason.is_none());
}

#[test]
fn an_unknown_offset_scores_nothing_rather_than_guessing() {
    let viewer = profile("London", "United Kingdom", None);
    let candidate = profile("Paris", "France", Some(60));

    assert_eq!(score_geography(&viewer, &candidate).points, 0.0);
}

#[test]
fn two_blank_locations_are_not_the_same_metro() {
    // Every profile that skipped the location fields would otherwise be in
    // the same city as every other.
    let viewer = profile("", "", Some(0));
    let candidate = profile("", "", Some(0));

    let result = score_geography(&viewer, &candidate);

    assert!(result.points < MAX_POINTS, "got {}", result.points);
}

#[test]
fn the_shared_city_is_named() {
    let viewer = profile("Jakarta", "Indonesia", Some(420));
    let candidate = profile("Jakarta", "Indonesia", Some(420));

    let reason = score_geography(&viewer, &candidate).reason.expect("a reason");

    assert!(reason.contains("Jakarta"), "got {reason}");
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `cd backend && cargo test --test scoring_situation --test scoring_interests --test scoring_geography`
Expected: FAIL — `could not find situation in scoring` and the same for the other two.

- [ ] **Step 3: Implement situation**

Create `backend/src/scoring/situation.rs`:

```rust
use crate::profiles::vocab::{self, COMMITMENTS, STAGES};
use crate::scoring::profile::{Component, ComponentScore, ScoredProfile};

pub const MAX_POINTS: f64 = 20.0;

/// Commitment carries most of the weight; stage proximity contributes a
/// smaller share, as the design document specifies.
const COMMITMENT_POINTS: f64 = 14.0;
const STAGE_POINTS: f64 = 6.0;

/// Both vocabularies are ladders, so a position in the list is a meaningful
/// distance rather than an arbitrary index.
fn rung(choices: &[vocab::Choice], id: &Option<String>) -> Option<usize> {
    let id = id.as_deref()?;
    choices.iter().position(|choice| choice.id == id)
}

fn ladder_fraction(steps: usize, falloff: &[f64]) -> f64 {
    falloff.get(steps).copied().unwrap_or(0.0)
}

pub fn score_situation(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore {
    // Identical scores full, adjacent scores most, distant scores near zero.
    const COMMITMENT_FALLOFF: [f64; 4] = [1.0, 0.7, 0.3, 0.0];
    const STAGE_FALLOFF: [f64; 4] = [1.0, 0.6, 0.3, 0.0];

    let mut points = 0.0;
    let mut reason = None;

    if let (Some(a), Some(b)) = (
        rung(&COMMITMENTS, &viewer.commitment),
        rung(&COMMITMENTS, &candidate.commitment),
    ) {
        let steps = a.abs_diff(b);
        points += ladder_fraction(steps, &COMMITMENT_FALLOFF) * COMMITMENT_POINTS;

        if steps == 0 {
            if let Some(label) = vocab::label(&COMMITMENTS, COMMITMENTS[a].id) {
                reason = Some(format!("Both {label}"));
            }
        }
    }

    if let (Some(a), Some(b)) = (
        rung(&STAGES, &viewer.stage),
        rung(&STAGES, &candidate.stage),
    ) {
        points += ladder_fraction(a.abs_diff(b), &STAGE_FALLOFF) * STAGE_POINTS;
    }

    ComponentScore::new(Component::Situation, points, reason)
}
```

The vocabulary label is used unchanged, so an identical commitment reads "Both Full-time now". That is what the test asserts.

- [ ] **Step 4: Implement interests**

Create `backend/src/scoring/interests.rs`:

```rust
use std::collections::HashSet;

use crate::profiles::vocab::INTERESTS;
use crate::scoring::profile::{Component, ComponentScore, ScoredProfile};

pub const MAX_POINTS: f64 = 15.0;

pub fn score_interests(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore {
    let mine: HashSet<&str> = viewer.interests.iter().map(String::as_str).collect();
    let theirs: HashSet<&str> = candidate.interests.iter().map(String::as_str).collect();

    let union = mine.union(&theirs).count();
    if union == 0 {
        // Two empty sets are not a perfect overlap; they are no information.
        return ComponentScore::empty(Component::Interests);
    }

    let shared: Vec<&str> = mine.intersection(&theirs).copied().collect();
    let jaccard = shared.len() as f64 / union as f64;

    // Named in vocabulary order rather than set order, which is arbitrary,
    // so the same pair always produces the same sentence.
    let reason = INTERESTS
        .iter()
        .find(|choice| shared.contains(&choice.id))
        .map(|choice| {
            if shared.len() > 1 {
                format!(
                    "Share {} interests including {}",
                    shared.len(),
                    choice.label
                )
            } else {
                format!("Both into {}", choice.label)
            }
        });

    ComponentScore::new(Component::Interests, jaccard * MAX_POINTS, reason)
}
```

- [ ] **Step 5: Implement geography**

Create `backend/src/scoring/geography.rs`:

```rust
use crate::scoring::profile::{Component, ComponentScore, ScoredProfile};

pub const MAX_POINTS: f64 = 10.0;

/// Three hours either side, in minutes. Beyond this a working day does not
/// meaningfully overlap.
pub const MAX_PARTIAL_OFFSET_MINUTES: i16 = 180;

const PARTIAL_POINTS: f64 = 5.0;

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

/// Blank counts as unknown, never as a match: otherwise every profile that
/// skipped the location fields shares a city with every other one.
fn same_metro(viewer: &ScoredProfile, candidate: &ScoredProfile) -> bool {
    let city = normalize(&viewer.city);
    let country = normalize(&viewer.country);

    !city.is_empty()
        && !country.is_empty()
        && city == normalize(&candidate.city)
        && country == normalize(&candidate.country)
}

pub fn score_geography(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore {
    if same_metro(viewer, candidate) {
        let city = viewer.city.trim().to_string();
        return ComponentScore::new(
            Component::Geography,
            MAX_POINTS,
            Some(format!("Both in {city}")),
        );
    }

    let (Some(mine), Some(theirs)) = (viewer.utc_offset_minutes, candidate.utc_offset_minutes)
    else {
        return ComponentScore::empty(Component::Geography);
    };

    let apart = (i32::from(mine) - i32::from(theirs)).abs();
    if apart > i32::from(MAX_PARTIAL_OFFSET_MINUTES) {
        return ComponentScore::empty(Component::Geography);
    }

    let hours = (apart as f64 / 60.0).round() as i32;
    let reason = if hours == 0 {
        Some("In the same timezone as you".to_string())
    } else {
        Some(format!(
            "Within {hours} hour{} of you",
            if hours == 1 { "" } else { "s" }
        ))
    };

    ComponentScore::new(Component::Geography, PARTIAL_POINTS, reason)
}
```

Modify `backend/src/scoring/mod.rs`:

```rust
pub mod geography;
pub mod interests;
pub mod profile;
pub mod roles;
pub mod situation;
pub mod traits;
```

- [ ] **Step 6: Run the tests and verify they pass**

Run: `cd backend && cargo test --test scoring_situation --test scoring_interests --test scoring_geography`
Expected: PASS — 8, 6 and 9 tests respectively.

- [ ] **Step 7: Commit**

```bash
git add backend/src/scoring backend/tests/scoring_situation.rs backend/tests/scoring_interests.rs backend/tests/scoring_geography.rs
git commit -m "feat: situation, interest, and geography scoring"
git push origin main
```

---

### Task 4: Assembling the score

**Files:**
- Create: `backend/src/scoring/score.rs`
- Modify: `backend/src/scoring/mod.rs`
- Test: `backend/tests/scoring_total.rs`

**Interfaces:**
- Consumes: every `score_*` function from Tasks 1–3
- Produces: `cofounder_api::scoring::score::{score, MAX_TOTAL, MAX_REASONS}`
  - `score(viewer: &ScoredProfile, candidate: &ScoredProfile) -> MatchScore`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/scoring_total.rs`:

```rust
use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::profile::{Component, ScoredProfile};
use cofounder_api::scoring::score::{score, MAX_REASONS, MAX_TOTAL};
use uuid::Uuid;

fn blank() -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: Vec::new(),
        seeking_roles: Vec::new(),
        interests: Vec::new(),
        idea_status: None,
        stage: None,
        commitment: None,
        city: String::new(),
        country: String::new(),
        utc_offset_minutes: None,
        traits: TraitScores {
            risk_tolerance: 50,
            pace_vs_rigor: 50,
            conflict_style: 50,
            decision_basis: 50,
            work_mode: 50,
            orientation: 50,
        },
    }
}

/// A pair sitting on the ideal of every single component.
fn perfect_pair() -> (ScoredProfile, ScoredProfile) {
    let viewer = ScoredProfile {
        roles: vec!["engineering".into()],
        seeking_roles: vec!["gtm".into()],
        interests: vec!["ai_ml".into()],
        stage: Some("prototype".into()),
        commitment: Some("full_time_now".into()),
        city: "Jakarta".into(),
        country: "Indonesia".into(),
        utc_offset_minutes: Some(420),
        traits: TraitScores {
            risk_tolerance: 50,
            pace_vs_rigor: 50,
            conflict_style: 50,
            decision_basis: 40,
            work_mode: 20,
            orientation: 20,
        },
        ..blank()
    };

    let candidate = ScoredProfile {
        roles: vec!["gtm".into()],
        seeking_roles: vec!["engineering".into()],
        interests: vec!["ai_ml".into()],
        stage: Some("prototype".into()),
        commitment: Some("full_time_now".into()),
        city: "Jakarta".into(),
        country: "Indonesia".into(),
        utc_offset_minutes: Some(420),
        traits: TraitScores {
            risk_tolerance: 50,
            pace_vs_rigor: 50,
            conflict_style: 50,
            decision_basis: 65,
            work_mode: 80,
            orientation: 80,
        },
        ..blank()
    };

    (viewer, candidate)
}

#[test]
fn the_components_add_up_to_exactly_one_hundred() {
    let (viewer, candidate) = perfect_pair();

    let result = score(&viewer, &candidate);

    assert_eq!(result.total, MAX_TOTAL);
    assert_eq!(MAX_TOTAL, 100);
}

#[test]
fn two_strangers_with_nothing_in_common_score_low() {
    let viewer = blank();
    let candidate = blank();

    let result = score(&viewer, &candidate);

    assert!(result.total < 30, "got {}", result.total);
}

#[test]
fn the_total_never_exceeds_the_budget() {
    let (viewer, candidate) = perfect_pair();

    assert!(score(&viewer, &candidate).total <= MAX_TOTAL);
}

#[test]
fn at_most_three_reasons_are_surfaced() {
    let (viewer, candidate) = perfect_pair();

    let result = score(&viewer, &candidate);

    assert!(
        result.reasons.len() <= MAX_REASONS,
        "got {}",
        result.reasons.len()
    );
    assert_eq!(MAX_REASONS, 3);
}

#[test]
fn reasons_come_from_the_highest_contributing_components() {
    // Roles (30), traits (25) and situation (20) outrank interests (15) and
    // geography (10), so those three are what a card should say.
    let (viewer, candidate) = perfect_pair();

    let components: Vec<Component> = score(&viewer, &candidate)
        .reasons
        .iter()
        .map(|reason| reason.component)
        .collect();

    assert_eq!(
        components,
        vec![Component::Roles, Component::Traits, Component::Situation]
    );
}

#[test]
fn a_component_that_scored_nothing_never_explains_itself() {
    // Nothing in common except a shared city: geography is the only
    // component with anything to say.
    let viewer = ScoredProfile {
        city: "Jakarta".into(),
        country: "Indonesia".into(),
        ..blank()
    };
    let candidate = ScoredProfile {
        city: "Jakarta".into(),
        country: "Indonesia".into(),
        ..blank()
    };

    let result = score(&viewer, &candidate);

    assert!(result
        .reasons
        .iter()
        .all(|reason| reason.component != Component::Interests));
    assert!(result
        .reasons
        .iter()
        .any(|reason| reason.component == Component::Geography));
}

#[test]
fn every_surfaced_reason_has_text() {
    let (viewer, candidate) = perfect_pair();

    for reason in score(&viewer, &candidate).reasons {
        assert!(!reason.text.trim().is_empty(), "{reason:?}");
    }
}

#[test]
fn scoring_is_deterministic() {
    let (viewer, candidate) = perfect_pair();

    assert_eq!(score(&viewer, &candidate), score(&viewer, &candidate));
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test scoring_total`
Expected: FAIL — `could not find score in scoring`.

- [ ] **Step 3: Implement the assembly**

Create `backend/src/scoring/score.rs`:

```rust
use crate::scoring::profile::{MatchScore, Reason, ScoredProfile};
use crate::scoring::{geography, interests, roles, situation, traits};

pub const MAX_TOTAL: u16 = 100;
pub const MAX_REASONS: usize = 3;

/// A pure function: no database, no clock, no I/O. Swipe-history adjustments
/// live in `deck::service` precisely so that this stays true and the whole
/// point table can be tested exhaustively.
pub fn score(viewer: &ScoredProfile, candidate: &ScoredProfile) -> MatchScore {
    let components = [
        roles::score_roles(viewer, candidate),
        traits::score_traits(viewer, candidate),
        situation::score_situation(viewer, candidate),
        interests::score_interests(viewer, candidate),
        geography::score_geography(viewer, candidate),
    ];

    let raw: f64 = components.iter().map(|component| component.points).sum();
    let total = raw.round().clamp(0.0, f64::from(MAX_TOTAL)) as u16;

    let mut ranked: Vec<_> = components
        .iter()
        .filter(|component| component.points > 0.0 && component.reason.is_some())
        .collect();

    // Descending by contribution. `total_cmp` rather than `partial_cmp`
    // because these are plain finite floats and this avoids an unwrap.
    ranked.sort_by(|a, b| b.points.total_cmp(&a.points));

    let reasons = ranked
        .into_iter()
        .take(MAX_REASONS)
        .map(|component| Reason {
            component: component.component,
            text: component.reason.clone().unwrap_or_default(),
        })
        .collect();

    MatchScore { total, reasons }
}
```

Modify `backend/src/scoring/mod.rs`:

```rust
pub mod geography;
pub mod interests;
pub mod profile;
pub mod roles;
pub mod score;
pub mod situation;
pub mod traits;
```

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cd backend && cargo test --test scoring_total`
Expected: PASS — 8 tests.

- [ ] **Step 5: Run the whole backend suite**

Run: `cd backend && cargo test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/scoring backend/tests/scoring_total.rs
git commit -m "feat: assemble the match score and its top three reasons"
git push origin main
```

---

### Task 5: Timezone offsets resolved at save time

**Files:**
- Create: `backend/src/profiles/timezone.rs`, `backend/migrations/0008_profile_utc_offset.sql`
- Modify: `backend/Cargo.toml`, `backend/src/profiles/mod.rs`, `backend/src/profiles/repo.rs`, `backend/src/profiles/service.rs`
- Test: `backend/tests/profile_timezone.rs`, `backend/tests/profile_api.rs`

**Interfaces:**
- Consumes: `profiles::repo::{ProfileRow, ProfileInput}`
- Produces:
  - `cofounder_api::profiles::timezone::{utc_offset_minutes, REFERENCE_YEAR}`
  - `utc_offset_minutes(name: &str) -> Option<i16>`
  - `ProfileRow.utc_offset_minutes: Option<i16>` and `ProfileInput.utc_offset_minutes: Option<i16>` (the latter `#[serde(skip)]` — derived, never client-supplied)

- [ ] **Step 1: Add the dependency**

```bash
cd backend && cargo add chrono-tz@0.10
```

- [ ] **Step 2: Write the failing test**

Create `backend/tests/profile_timezone.rs`:

```rust
use cofounder_api::profiles::timezone::utc_offset_minutes;

#[test]
fn a_zone_east_of_utc_has_a_positive_offset() {
    assert_eq!(utc_offset_minutes("Asia/Jakarta"), Some(420));
}

#[test]
fn a_zone_west_of_utc_has_a_negative_offset() {
    assert_eq!(utc_offset_minutes("America/New_York"), Some(-300));
}

#[test]
fn utc_itself_is_zero() {
    assert_eq!(utc_offset_minutes("UTC"), Some(0));
}

#[test]
fn a_half_hour_zone_is_handled() {
    // India is UTC+5:30. An hours-only implementation loses the thirty.
    assert_eq!(utc_offset_minutes("Asia/Kolkata"), Some(330));
}

#[test]
fn resolution_is_against_a_fixed_instant_not_today() {
    // London is UTC+0 in January and UTC+1 in July. Resolving against a
    // fixed reference is what keeps the scorer's output stable year-round.
    assert_eq!(utc_offset_minutes("Europe/London"), Some(0));
}

#[test]
fn surrounding_whitespace_is_tolerated() {
    assert_eq!(utc_offset_minutes("  Asia/Jakarta  "), Some(420));
}

#[test]
fn an_unknown_zone_has_no_offset() {
    assert_eq!(utc_offset_minutes("Mars/Olympus_Mons"), None);
}

#[test]
fn a_blank_zone_has_no_offset() {
    assert_eq!(utc_offset_minutes(""), None);
    assert_eq!(utc_offset_minutes("   "), None);
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cd backend && cargo test --test profile_timezone`
Expected: FAIL — `could not find timezone in profiles`.

- [ ] **Step 4: Implement the resolver**

Create `backend/src/profiles/timezone.rs`:

```rust
use chrono::{Offset, TimeZone, Utc};
use chrono_tz::Tz;

/// Offsets are resolved against the first of January rather than against
/// today. An IANA zone's offset moves with daylight saving, so resolving at
/// score time would make the same pair score differently in March than in
/// July. The cost is that southern-hemisphere zones are recorded at their
/// summer-time offset; that is a consistent hour, not a drifting one.
pub const REFERENCE_YEAR: i32 = 2024;

pub fn utc_offset_minutes(name: &str) -> Option<i16> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }

    let zone: Tz = trimmed.parse().ok()?;
    let reference = Utc
        .with_ymd_and_hms(REFERENCE_YEAR, 1, 1, 0, 0, 0)
        .single()?;

    let seconds = zone
        .offset_from_utc_datetime(&reference.naive_utc())
        .fix()
        .local_minus_utc();

    i16::try_from(seconds / 60).ok()
}
```

Modify `backend/src/profiles/mod.rs`:

```rust
pub mod repo;
pub mod routes;
pub mod service;
pub mod timezone;
pub mod vocab;
```

- [ ] **Step 5: Run the resolver tests and verify they pass**

Run: `cd backend && cargo test --test profile_timezone`
Expected: PASS — 8 tests.

- [ ] **Step 6: Add the column**

Create `backend/migrations/0008_profile_utc_offset.sql`:

```sql
-- Derived from profiles.timezone when the profile is saved. The scorer reads
-- this integer instead of an IANA name so it needs no timezone database and
-- no clock, and so the same pair scores identically all year round.
-- Null for a profile saved before this column existed; it is filled in the
-- next time that profile is written.
ALTER TABLE profiles ADD COLUMN utc_offset_minutes SMALLINT;
```

- [ ] **Step 7: Carry the column through the repository**

Modify `backend/src/profiles/repo.rs`:

Add the field to `ProfileRow`, after `timezone`:

```rust
    pub timezone: String,
    pub utc_offset_minutes: Option<i16>,
```

Add the field to `ProfileInput`, after `timezone`:

```rust
    #[serde(default)]
    pub timezone: String,
    /// Derived from `timezone` by the service layer, never accepted from the
    /// client — a caller must not be able to claim an offset that disagrees
    /// with the zone they named.
    #[serde(skip)]
    pub utc_offset_minutes: Option<i16>,
```

Extend `COLUMNS`:

```rust
const COLUMNS: &str = "display_name, headline, bio, city, country, timezone, \
     utc_offset_minutes, linkedin_url, github_url, website_url, roles, seeking_roles, \
     idea_status, stage, commitment";
```

In `save`, add the column to the insert list after `timezone`, add `$16` to the `VALUES` list, add `utc_offset_minutes = EXCLUDED.utc_offset_minutes` to the `DO UPDATE SET` list, and bind the value. The full statement becomes:

```rust
    let row = sqlx::query_as::<_, ProfileRow>(&format!(
        r#"
        INSERT INTO profiles (
            user_id, display_name, headline, bio, city, country, timezone,
            utc_offset_minutes, linkedin_url, github_url, website_url,
            roles, seeking_roles, idea_status, stage, commitment, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, now())
        ON CONFLICT (user_id) DO UPDATE SET
            display_name       = EXCLUDED.display_name,
            headline           = EXCLUDED.headline,
            bio                = EXCLUDED.bio,
            city               = EXCLUDED.city,
            country            = EXCLUDED.country,
            timezone           = EXCLUDED.timezone,
            utc_offset_minutes = EXCLUDED.utc_offset_minutes,
            linkedin_url       = EXCLUDED.linkedin_url,
            github_url         = EXCLUDED.github_url,
            website_url        = EXCLUDED.website_url,
            roles              = EXCLUDED.roles,
            seeking_roles      = EXCLUDED.seeking_roles,
            idea_status        = EXCLUDED.idea_status,
            stage              = EXCLUDED.stage,
            commitment         = EXCLUDED.commitment,
            updated_at         = now()
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
    .bind(input.utc_offset_minutes)
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
```

- [ ] **Step 8: Derive and validate in the service**

Modify `backend/src/profiles/service.rs`.

Add the import:

```rust
use crate::profiles::timezone;
```

Add the field to `empty_row()`, after `timezone`:

```rust
        timezone: String::new(),
        utc_offset_minutes: None,
```

In `normalize_and_validate`, after the `check_length(&mut errors, "timezone", ...)` call, add:

```rust
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
```

- [ ] **Step 9: Add the API-level tests**

Modify `backend/tests/profile_api.rs` — append these three tests:

```rust
#[sqlx::test]
async fn saving_a_profile_derives_the_timezone_offset(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["timezone"] = serde_json::json!("Asia/Jakarta");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let offset: Option<i16> = sqlx::query_scalar("SELECT utc_offset_minutes FROM profiles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(offset, Some(420));
}

#[sqlx::test]
async fn an_unknown_timezone_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["timezone"] = serde_json::json!("Mars/Olympus_Mons");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "timezone");
}

#[sqlx::test]
async fn a_client_cannot_supply_its_own_offset(pool: PgPool) {
    // The offset is derived. Accepting it from the body would let a caller
    // claim to be in a timezone their named zone contradicts.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["timezone"] = serde_json::json!("Asia/Jakarta");
    payload["utc_offset_minutes"] = serde_json::json!(-600);

    router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    let offset: Option<i16> = sqlx::query_scalar("SELECT utc_offset_minutes FROM profiles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(offset, Some(420));
}
```

- [ ] **Step 10: Fix the repository test fixture**

Modify `backend/tests/profiles_repo.rs` — add the field to `an_input()`, after `timezone`:

```rust
        timezone: "Europe/London".into(),
        utc_offset_minutes: Some(0),
```

- [ ] **Step 11: Run the tests and verify they pass**

Run: `cd backend && cargo test`
Expected: PASS — including the three new profile API tests and the eight resolver tests.

- [ ] **Step 12: Commit**

```bash
git add backend/Cargo.toml backend/Cargo.lock backend/migrations backend/src/profiles backend/tests
git commit -m "feat: derive a UTC offset from the profile timezone at save time"
git push origin main
```

---

### Task 6: Swipes, matches, and blocks

**Files:**
- Create: `backend/migrations/0009_swipes.sql`, `backend/migrations/0010_matches.sql`, `backend/migrations/0011_blocks.sql`, `backend/src/swipes/mod.rs`, `backend/src/swipes/repo.rs`
- Modify: `backend/src/lib.rs`
- Test: `backend/tests/swipes_repo.rs`

**Interfaces:**
- Consumes: `users::repo::find_or_create_by_email`
- Produces: `cofounder_api::swipes::repo::{Direction, SwipeOutcome, MatchedUser, record_swipe, matches_for, recent_left_swipe_targets}`
  - `Direction` — `Left` / `Right`, `Direction::as_str(self) -> &'static str`
  - `record_swipe(&PgPool, swiper: Uuid, target: Uuid, Direction) -> sqlx::Result<Option<SwipeOutcome>>` — `None` when a swipe on that pair already exists
  - `SwipeOutcome { matched: bool }`
  - `MatchedUser { user_id: Uuid, display_name: String, headline: String, matched_at: DateTime<Utc> }`
  - `matches_for(&PgPool, Uuid) -> sqlx::Result<Vec<MatchedUser>>`
  - `recent_left_swipe_targets(&PgPool, Uuid, limit: i64) -> sqlx::Result<Vec<Uuid>>`

- [ ] **Step 1: Write the migrations**

Create `backend/migrations/0009_swipes.sql`:

```sql
-- Both directions are permanent and both exclude the target from future
-- decks, so the pair is the primary key rather than a surrogate id.
CREATE TABLE swipes (
    swiper_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    direction  TEXT NOT NULL CHECK (direction IN ('left', 'right')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (swiper_id, target_id),
    CONSTRAINT no_self_swipe CHECK (swiper_id <> target_id)
);

-- Serves the popularity adjustment, which reads recent swipes by target.
CREATE INDEX swipes_target_recent_idx ON swipes (target_id, created_at DESC);

-- Serves the pass-suppression adjustment, which reads a viewer's recent
-- left swipes.
CREATE INDEX swipes_swiper_recent_idx ON swipes (swiper_id, created_at DESC);
```

Create `backend/migrations/0010_matches.sql`:

```sql
-- The pair is stored in a fixed order so that a match between two people is
-- one row rather than two, and so the primary key catches duplicates.
CREATE TABLE matches (
    user_a_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_a_id, user_b_id),
    CONSTRAINT ordered_pair CHECK (user_a_id < user_b_id)
);

CREATE INDEX matches_user_b_idx ON matches (user_b_id);
```

Create `backend/migrations/0011_blocks.sql`:

```sql
-- Created in slice 3 although POST /blocks arrives in slice 4: the deck's
-- candidate query excludes blocked pairs from the start, so the most
-- safety-critical filter is never bolted on afterwards. Until slice 4 the
-- table is simply empty.
CREATE TABLE blocks (
    blocker_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (blocker_id, blocked_id),
    CONSTRAINT no_self_block CHECK (blocker_id <> blocked_id)
);

-- The deck filters on being blocked as well as on blocking.
CREATE INDEX blocks_blocked_idx ON blocks (blocked_id);
```

- [ ] **Step 2: Write the failing test**

Create `backend/tests/swipes_repo.rs`:

```rust
use cofounder_api::swipes::repo::{self, Direction};
use cofounder_api::users;
use sqlx::PgPool;
use uuid::Uuid;

async fn a_user(pool: &PgPool, email: &str) -> Uuid {
    users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id
}

/// The deck requires a complete profile, and `matches_for` reads the display
/// name, so matched users need a profile row.
async fn with_profile(pool: &PgPool, email: &str, name: &str) -> Uuid {
    let id = a_user(pool, email).await;

    sqlx::query(
        "INSERT INTO profiles (user_id, display_name, headline, bio, roles, seeking_roles, commitment)
         VALUES ($1, $2, 'Building things', 'A bio.', ARRAY['engineering'], ARRAY['gtm'], 'full_time_now')",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    id
}

#[sqlx::test]
async fn a_left_swipe_is_recorded_without_matching(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    let outcome = repo::record_swipe(&pool, ada, grace, Direction::Left)
        .await
        .unwrap()
        .expect("a fresh swipe");

    assert!(!outcome.matched);
}

#[sqlx::test]
async fn a_one_sided_right_swipe_does_not_match(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    let outcome = repo::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap()
        .expect("a fresh swipe");

    assert!(!outcome.matched);

    let matches: i64 = sqlx::query_scalar("SELECT count(*) FROM matches")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(matches, 0);
}

#[sqlx::test]
async fn a_mutual_right_swipe_creates_exactly_one_match(pool: PgPool) {
    let ada = with_profile(&pool, "ada@example.com", "Ada").await;
    let grace = with_profile(&pool, "grace@example.com", "Grace").await;

    repo::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap();
    let second = repo::record_swipe(&pool, grace, ada, Direction::Right)
        .await
        .unwrap()
        .expect("a fresh swipe");

    assert!(second.matched);

    let matches: i64 = sqlx::query_scalar("SELECT count(*) FROM matches")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(matches, 1, "one row, not one per direction");
}

#[sqlx::test]
async fn a_right_swipe_onto_a_left_swipe_does_not_match(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    repo::record_swipe(&pool, ada, grace, Direction::Left)
        .await
        .unwrap();
    let outcome = repo::record_swipe(&pool, grace, ada, Direction::Right)
        .await
        .unwrap()
        .expect("a fresh swipe");

    assert!(!outcome.matched);
}

#[sqlx::test]
async fn swiping_the_same_person_twice_is_refused(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    repo::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap()
        .expect("a fresh swipe");

    let repeat = repo::record_swipe(&pool, ada, grace, Direction::Left)
        .await
        .unwrap();

    assert!(repeat.is_none(), "a swipe is permanent");

    let direction: String =
        sqlx::query_scalar("SELECT direction FROM swipes WHERE swiper_id = $1")
            .bind(ada)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(direction, "right", "the original swipe stands");
}

#[sqlx::test]
async fn the_match_pair_is_stored_in_a_fixed_order(pool: PgPool) {
    let ada = with_profile(&pool, "ada@example.com", "Ada").await;
    let grace = with_profile(&pool, "grace@example.com", "Grace").await;

    repo::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap();
    repo::record_swipe(&pool, grace, ada, Direction::Right)
        .await
        .unwrap();

    let (a, b): (Uuid, Uuid) =
        sqlx::query_as("SELECT user_a_id, user_b_id FROM matches")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(a < b);
}

#[sqlx::test]
async fn both_sides_see_the_match(pool: PgPool) {
    let ada = with_profile(&pool, "ada@example.com", "Ada").await;
    let grace = with_profile(&pool, "grace@example.com", "Grace").await;

    repo::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap();
    repo::record_swipe(&pool, grace, ada, Direction::Right)
        .await
        .unwrap();

    let for_ada = repo::matches_for(&pool, ada).await.unwrap();
    let for_grace = repo::matches_for(&pool, grace).await.unwrap();

    assert_eq!(for_ada.len(), 1);
    assert_eq!(for_ada[0].display_name, "Grace");
    assert_eq!(for_grace.len(), 1);
    assert_eq!(for_grace[0].display_name, "Ada");
}

#[sqlx::test]
async fn someone_with_no_matches_sees_none(pool: PgPool) {
    let ada = with_profile(&pool, "ada@example.com", "Ada").await;

    assert!(repo::matches_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn recent_left_swipes_are_returned_newest_first_and_capped(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;

    let mut passed = Vec::new();
    for index in 0..5 {
        let target = a_user(&pool, &format!("passed{index}@example.com")).await;
        repo::record_swipe(&pool, ada, target, Direction::Left)
            .await
            .unwrap();
        passed.push(target);
    }

    let liked = a_user(&pool, "liked@example.com").await;
    repo::record_swipe(&pool, ada, liked, Direction::Right)
        .await
        .unwrap();

    let recent = repo::recent_left_swipe_targets(&pool, ada, 3).await.unwrap();

    assert_eq!(recent.len(), 3, "the limit is respected");
    assert!(!recent.contains(&liked), "right swipes are not passes");
    for target in &recent {
        assert!(passed.contains(target));
    }
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cd backend && cargo test --test swipes_repo`
Expected: FAIL — `could not find swipes in cofounder_api`.

- [ ] **Step 4: Implement the repository**

Create `backend/src/swipes/mod.rs`:

```rust
pub mod repo;
```

Modify `backend/src/lib.rs` — add `pub mod swipes;` after `pub mod scoring;`.

Create `backend/src/swipes/repo.rs`:

```rust
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Left,
    Right,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Left => "left",
            Direction::Right => "right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SwipeOutcome {
    pub matched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct MatchedUser {
    pub user_id: Uuid,
    pub display_name: String,
    pub headline: String,
    pub matched_at: DateTime<Utc>,
}

/// A match is one row for the pair, so the two ids are stored in a fixed
/// order. Uuid ordering is arbitrary but stable, which is all that is needed.
fn ordered(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Records a swipe and, when it completes a mutual right swipe, the match it
/// creates — in one transaction, so a match can never exist without the
/// swipe that caused it. Returns `None` if this pair was already swiped:
/// swipes are permanent, so a second one is a conflict rather than an update.
pub async fn record_swipe(
    pool: &PgPool,
    swiper_id: Uuid,
    target_id: Uuid,
    direction: Direction,
) -> sqlx::Result<Option<SwipeOutcome>> {
    let mut tx = pool.begin().await?;

    let inserted: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO swipes (swiper_id, target_id, direction)
        VALUES ($1, $2, $3)
        ON CONFLICT (swiper_id, target_id) DO NOTHING
        RETURNING swiper_id
        "#,
    )
    .bind(swiper_id)
    .bind(target_id)
    .bind(direction.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    if inserted.is_none() {
        tx.rollback().await?;
        return Ok(None);
    }

    let mut matched = false;

    if direction == Direction::Right {
        let reciprocated: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT swiper_id FROM swipes
            WHERE swiper_id = $1 AND target_id = $2 AND direction = 'right'
            "#,
        )
        .bind(target_id)
        .bind(swiper_id)
        .fetch_optional(&mut *tx)
        .await?;

        if reciprocated.is_some() {
            let (a, b) = ordered(swiper_id, target_id);

            sqlx::query(
                r#"
                INSERT INTO matches (user_a_id, user_b_id)
                VALUES ($1, $2)
                ON CONFLICT (user_a_id, user_b_id) DO NOTHING
                "#,
            )
            .bind(a)
            .bind(b)
            .execute(&mut *tx)
            .await?;

            matched = true;
        }
    }

    tx.commit().await?;

    Ok(Some(SwipeOutcome { matched }))
}

/// Both sides of a match see the other person, so the query looks at each
/// column in turn and selects whichever id is not the caller's.
pub async fn matches_for(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<MatchedUser>> {
    sqlx::query_as::<_, MatchedUser>(
        r#"
        SELECT
            other.id            AS user_id,
            p.display_name      AS display_name,
            p.headline          AS headline,
            m.created_at        AS matched_at
        FROM matches m
        JOIN users other
          ON other.id = CASE WHEN m.user_a_id = $1 THEN m.user_b_id ELSE m.user_a_id END
        JOIN profiles p ON p.user_id = other.id
        WHERE $1 IN (m.user_a_id, m.user_b_id)
        ORDER BY m.created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn recent_left_swipe_targets(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        r#"
        SELECT target_id FROM swipes
        WHERE swiper_id = $1 AND direction = 'left'
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}
```

- [ ] **Step 5: Run the tests and verify they pass**

Run: `cd backend && cargo test --test swipes_repo`
Expected: PASS — 9 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/migrations backend/src/swipes backend/src/lib.rs backend/tests/swipes_repo.rs
git commit -m "feat: swipe, match, and block persistence"
git push origin main
```

---

### Task 7: The candidate query

**Files:**
- Create: `backend/src/deck/mod.rs`, `backend/src/deck/repo.rs`
- Modify: `backend/src/lib.rs`
- Test: `backend/tests/deck_repo.rs`

**Interfaces:**
- Consumes: `scoring::profile::ScoredProfile`, `assessment::scoring::TraitScores`
- Produces: `cofounder_api::deck::repo::{Candidate, load_profile, candidates_for, recent_pass_tags, right_swipe_rates}`
  - `Candidate { profile: ScoredProfile, headline: String, bio: String }`
  - `load_profile(&PgPool, Uuid) -> sqlx::Result<Option<Candidate>>` — the viewer, with no exclusions applied
  - `candidates_for(&PgPool, viewer: Uuid) -> sqlx::Result<Vec<Candidate>>`
  - `recent_pass_tags(&PgPool, viewer: Uuid, limit: i64) -> sqlx::Result<Vec<String>>`
  - `right_swipe_rates(&PgPool, days: i32) -> sqlx::Result<Vec<(Uuid, f64)>>`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/deck_repo.rs`:

```rust
use cofounder_api::deck::repo;
use cofounder_api::swipes::repo as swipes;
use cofounder_api::swipes::repo::Direction;
use cofounder_api::users;
use sqlx::PgPool;
use uuid::Uuid;

/// A user who satisfies every deck filter: active, complete profile, and a
/// trait_scores row (which exists only when all eighteen answers do).
async fn complete_user(pool: &PgPool, email: &str, name: &str) -> Uuid {
    let id = users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id;

    sqlx::query(
        "INSERT INTO profiles (user_id, display_name, headline, bio, city, country,
                               timezone, utc_offset_minutes, roles, seeking_roles,
                               idea_status, stage, commitment)
         VALUES ($1, $2, 'Building things', 'A real bio.', 'Jakarta', 'Indonesia',
                 'Asia/Jakarta', 420, ARRAY['engineering'], ARRAY['gtm'],
                 'committed_idea', 'prototype', 'full_time_now')",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO trait_scores (user_id, risk_tolerance, pace_vs_rigor, conflict_style,
                                   decision_basis, work_mode, orientation)
         VALUES ($1, 50, 50, 50, 50, 50, 50)",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO profile_interests (user_id, tag) VALUES ($1, 'ai_ml')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();

    id
}

#[sqlx::test]
async fn a_complete_stranger_is_a_candidate(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    complete_user(&pool, "grace@example.com", "Grace").await;

    let candidates = repo::candidates_for(&pool, ada).await.unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].profile.display_name, "Grace");
    assert_eq!(candidates[0].bio, "A real bio.");
    assert_eq!(candidates[0].profile.roles, vec!["engineering"]);
    assert_eq!(candidates[0].profile.interests, vec!["ai_ml"]);
    assert_eq!(candidates[0].profile.utc_offset_minutes, Some(420));
    assert_eq!(candidates[0].profile.traits.risk_tolerance, 50);
}

#[sqlx::test]
async fn you_are_never_your_own_candidate(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn someone_already_swiped_on_is_excluded(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    swipes::record_swipe(&pool, ada, grace, Direction::Left)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_right_swipe_also_removes_them_from_the_deck(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    swipes::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_profile_without_trait_scores_is_incomplete(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("DELETE FROM trait_scores WHERE user_id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_profile_without_a_bio_is_incomplete(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("UPDATE profiles SET bio = '' WHERE user_id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_profile_with_no_roles_is_incomplete(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("UPDATE profiles SET roles = '{}' WHERE user_id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_profile_with_no_commitment_is_incomplete(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("UPDATE profiles SET commitment = NULL WHERE user_id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_suspended_account_is_excluded(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("UPDATE users SET status = 'suspended' WHERE id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn someone_the_viewer_blocked_is_excluded(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(ada)
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn someone_who_blocked_the_viewer_is_excluded(pool: PgPool) {
    // The block has to bite in both directions, or blocking someone merely
    // hides you from them while they stay visible to you.
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(grace)
        .bind(ada)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_candidate_appears_once_however_many_interests_they_have(pool: PgPool) {
    // A naive join against profile_interests returns one row per tag.
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("INSERT INTO profile_interests (user_id, tag) VALUES ($1, 'saas'), ($1, 'fintech')")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    let candidates = repo::candidates_for(&pool, ada).await.unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].profile.interests.len(), 3);
}

#[sqlx::test]
async fn a_candidate_with_no_interests_still_appears(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("DELETE FROM profile_interests WHERE user_id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    let candidates = repo::candidates_for(&pool, ada).await.unwrap();

    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].profile.interests.is_empty());
}

#[sqlx::test]
async fn the_viewer_can_be_loaded_directly(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;

    let viewer = repo::load_profile(&pool, ada).await.unwrap().expect("loaded");

    assert_eq!(viewer.profile.user_id, ada);
    assert_eq!(viewer.profile.display_name, "Ada");
}

#[sqlx::test]
async fn loading_an_incomplete_viewer_returns_nothing(pool: PgPool) {
    let ada = users::repo::find_or_create_by_email(&pool, "ada@example.com")
        .await
        .unwrap()
        .id;

    assert!(repo::load_profile(&pool, ada).await.unwrap().is_none());
}

#[sqlx::test]
async fn recent_pass_tags_gather_roles_and_interests(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let passed = complete_user(&pool, "passed@example.com", "Passed").await;

    swipes::record_swipe(&pool, ada, passed, Direction::Left)
        .await
        .unwrap();

    let tags = repo::recent_pass_tags(&pool, ada, 20).await.unwrap();

    assert!(tags.contains(&"engineering".to_string()));
    assert!(tags.contains(&"ai_ml".to_string()));
}

#[sqlx::test]
async fn right_swipes_do_not_become_pass_tags(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let liked = complete_user(&pool, "liked@example.com", "Liked").await;

    swipes::record_swipe(&pool, ada, liked, Direction::Right)
        .await
        .unwrap();

    assert!(repo::recent_pass_tags(&pool, ada, 20).await.unwrap().is_empty());
}

#[sqlx::test]
async fn right_swipe_rates_are_reported_per_target(pool: PgPool) {
    let popular = complete_user(&pool, "popular@example.com", "Popular").await;
    let ignored = complete_user(&pool, "ignored@example.com", "Ignored").await;

    for index in 0..3 {
        let admirer = complete_user(&pool, &format!("fan{index}@example.com"), "Fan").await;
        swipes::record_swipe(&pool, admirer, popular, Direction::Right)
            .await
            .unwrap();
        swipes::record_swipe(&pool, admirer, ignored, Direction::Left)
            .await
            .unwrap();
    }

    let rates = repo::right_swipe_rates(&pool, 30).await.unwrap();

    let popular_rate = rates.iter().find(|(id, _)| *id == popular).unwrap().1;
    let ignored_rate = rates.iter().find(|(id, _)| *id == ignored).unwrap().1;

    assert!((popular_rate - 1.0).abs() < 0.001, "got {popular_rate}");
    assert!((ignored_rate - 0.0).abs() < 0.001, "got {ignored_rate}");
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test deck_repo`
Expected: FAIL — `could not find deck in cofounder_api`.

- [ ] **Step 3: Implement the repository**

Create `backend/src/deck/mod.rs`:

```rust
pub mod repo;
```

Modify `backend/src/lib.rs` — add `pub mod deck;` after `pub mod db;`.

Create `backend/src/deck/repo.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

use crate::assessment::scoring::TraitScores;
use crate::scoring::profile::ScoredProfile;

/// A candidate as the deck needs them: everything the scorer reads, plus the
/// display-only fields a card shows. Kept separate from `ScoredProfile` so
/// that prose never leaks into the scoring type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub profile: ScoredProfile,
    pub headline: String,
    pub bio: String,
}

#[derive(sqlx::FromRow)]
struct CandidateRow {
    user_id: Uuid,
    display_name: String,
    headline: String,
    bio: String,
    city: String,
    country: String,
    utc_offset_minutes: Option<i16>,
    roles: Vec<String>,
    seeking_roles: Vec<String>,
    interests: Vec<String>,
    idea_status: Option<String>,
    stage: Option<String>,
    commitment: Option<String>,
    risk_tolerance: i16,
    pace_vs_rigor: i16,
    conflict_style: i16,
    decision_basis: i16,
    work_mode: i16,
    orientation: i16,
}

impl From<CandidateRow> for Candidate {
    fn from(row: CandidateRow) -> Self {
        Candidate {
            headline: row.headline,
            bio: row.bio,
            profile: ScoredProfile {
                user_id: row.user_id,
                display_name: row.display_name,
                roles: row.roles,
                seeking_roles: row.seeking_roles,
                interests: row.interests,
                idea_status: row.idea_status,
                stage: row.stage,
                commitment: row.commitment,
                city: row.city,
                country: row.country,
                utc_offset_minutes: row.utc_offset_minutes,
                traits: TraitScores {
                    risk_tolerance: row.risk_tolerance,
                    pace_vs_rigor: row.pace_vs_rigor,
                    conflict_style: row.conflict_style,
                    decision_basis: row.decision_basis,
                    work_mode: row.work_mode,
                    orientation: row.orientation,
                },
            },
        }
    }
}

/// Selected columns and the joins that define completeness. The join onto
/// `trait_scores` is the completeness check for the assessment: that row
/// exists only when all eighteen answers do, so there is no need to count
/// responses here.
const SELECT: &str = r#"
    SELECT
        u.id                 AS user_id,
        p.display_name       AS display_name,
        p.headline           AS headline,
        p.bio                AS bio,
        p.city               AS city,
        p.country            AS country,
        p.utc_offset_minutes AS utc_offset_minutes,
        p.roles              AS roles,
        p.seeking_roles      AS seeking_roles,
        COALESCE(
            ARRAY_AGG(pi.tag ORDER BY pi.tag) FILTER (WHERE pi.tag IS NOT NULL),
            '{}'
        )                    AS interests,
        p.idea_status        AS idea_status,
        p.stage              AS stage,
        p.commitment         AS commitment,
        t.risk_tolerance     AS risk_tolerance,
        t.pace_vs_rigor      AS pace_vs_rigor,
        t.conflict_style     AS conflict_style,
        t.decision_basis     AS decision_basis,
        t.work_mode          AS work_mode,
        t.orientation        AS orientation
    FROM users u
    JOIN profiles p     ON p.user_id = u.id
    JOIN trait_scores t ON t.user_id = u.id
    LEFT JOIN profile_interests pi ON pi.user_id = u.id
"#;

/// The completeness rule, in SQL. Mirrors `profiles::service::missing_requirements`.
const COMPLETE: &str = r#"
    u.status = 'active'
    AND btrim(p.bio) <> ''
    AND cardinality(p.roles) > 0
    AND cardinality(p.seeking_roles) > 0
    AND p.commitment IS NOT NULL
"#;

const GROUP_BY: &str = " GROUP BY u.id, p.user_id, t.user_id ";

pub async fn load_profile(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Option<Candidate>> {
    let sql = format!("{SELECT} WHERE u.id = $1 AND {COMPLETE} {GROUP_BY}");

    let row = sqlx::query_as::<_, CandidateRow>(&sql)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(Candidate::from))
}

/// The candidate pool: everyone except the viewer, anyone already swiped on,
/// anyone blocked in either direction, suspended accounts, and incomplete
/// profiles.
pub async fn candidates_for(pool: &PgPool, viewer_id: Uuid) -> sqlx::Result<Vec<Candidate>> {
    let sql = format!(
        r#"
        {SELECT}
        WHERE u.id <> $1
          AND {COMPLETE}
          AND NOT EXISTS (
              SELECT 1 FROM swipes s
              WHERE s.swiper_id = $1 AND s.target_id = u.id
          )
          AND NOT EXISTS (
              SELECT 1 FROM blocks b
              WHERE (b.blocker_id = $1 AND b.blocked_id = u.id)
                 OR (b.blocker_id = u.id AND b.blocked_id = $1)
          )
        {GROUP_BY}
        "#
    );

    let rows = sqlx::query_as::<_, CandidateRow>(&sql)
        .bind(viewer_id)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(Candidate::from).collect())
}

/// The role and interest tags of the people this viewer most recently passed
/// on. Feeds pass suppression.
pub async fn recent_pass_tags(
    pool: &PgPool,
    viewer_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        r#"
        WITH recent AS (
            SELECT target_id FROM swipes
            WHERE swiper_id = $1 AND direction = 'left'
            ORDER BY created_at DESC
            LIMIT $2
        )
        SELECT DISTINCT tag FROM (
            SELECT UNNEST(p.roles) AS tag
            FROM profiles p JOIN recent r ON r.target_id = p.user_id
            UNION ALL
            SELECT pi.tag
            FROM profile_interests pi JOIN recent r ON r.target_id = pi.user_id
        ) tags
        "#,
    )
    .bind(viewer_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Each target's right-swipe rate over a trailing window. Feeds the
/// popularity boost. This reads the clock, which is exactly why it lives
/// here and not in the scorer.
pub async fn right_swipe_rates(pool: &PgPool, days: i32) -> sqlx::Result<Vec<(Uuid, f64)>> {
    sqlx::query_as(
        r#"
        SELECT
            target_id,
            count(*) FILTER (WHERE direction = 'right')::float8 / count(*)::float8 AS rate
        FROM swipes
        WHERE created_at > now() - make_interval(days => $1)
        GROUP BY target_id
        "#,
    )
    .bind(days)
    .fetch_all(pool)
    .await
}
```

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cd backend && cargo test --test deck_repo`
Expected: PASS — 18 tests.

- [ ] **Step 5: Commit**

```bash
git add backend/src/deck backend/src/lib.rs backend/tests/deck_repo.rs
git commit -m "feat: deck candidate query with completeness and block filtering"
git push origin main
```

---

### Task 8: GET /deck

**Files:**
- Create: `backend/src/deck/service.rs`, `backend/src/deck/routes.rs`
- Modify: `backend/src/deck/mod.rs`, `backend/src/app.rs`
- Test: `backend/tests/deck_api.rs`

**Interfaces:**
- Consumes: `deck::repo::{Candidate, load_profile, candidates_for, recent_pass_tags, right_swipe_rates}`, `scoring::score::score`
- Produces:
  - `cofounder_api::deck::routes::router() -> Router<AppState>` mounting `GET /deck`
  - `cofounder_api::deck::service::{DeckCard, DeckView, build, DECK_SIZE, MAX_PASS_PENALTY, MAX_POPULARITY_BOOST, POPULARITY_WINDOW_DAYS, RECENT_PASSES}`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/deck_api.rs`. It reuses the sign-in helpers from the earlier API tests; copy them verbatim rather than importing, since Rust integration tests do not share modules by default:

```rust
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

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

    response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

fn get(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Inserts a complete profile directly. `roles`/`seeking_roles` are passed so
/// a test can make one candidate a better fit than another.
async fn complete_profile(
    pool: &PgPool,
    email: &str,
    name: &str,
    roles: &str,
    seeking: &str,
) -> Uuid {
    let id = cofounder_api::users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id;

    sqlx::query(&format!(
        "INSERT INTO profiles (user_id, display_name, headline, bio, city, country,
                               timezone, utc_offset_minutes, roles, seeking_roles,
                               idea_status, stage, commitment)
         VALUES ($1, $2, 'Building things', 'A real bio.', 'Jakarta', 'Indonesia',
                 'Asia/Jakarta', 420, ARRAY['{roles}'], ARRAY['{seeking}'],
                 'committed_idea', 'prototype', 'full_time_now')
         ON CONFLICT (user_id) DO UPDATE SET
             display_name = EXCLUDED.display_name,
             roles = EXCLUDED.roles,
             seeking_roles = EXCLUDED.seeking_roles"
    ))
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO trait_scores (user_id, risk_tolerance, pace_vs_rigor, conflict_style,
                                   decision_basis, work_mode, orientation)
         VALUES ($1, 50, 50, 50, 50, 50, 50)
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();

    id
}

#[sqlx::test]
async fn the_deck_requires_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(Request::builder().uri("/deck").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn an_incomplete_viewer_gets_an_empty_deck_and_is_told_why(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["profile_complete"], false);
    assert_eq!(body["cards"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn a_complete_viewer_sees_a_scored_card(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    complete_profile(&pool, "ada@example.com", "Ada", "engineering", "gtm").await;
    complete_profile(&pool, "grace@example.com", "Grace", "gtm", "engineering").await;

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();
    let body = json_body(response).await;

    assert_eq!(body["profile_complete"], true);
    let cards = body["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["display_name"], "Grace");
    assert!(cards[0]["score"].as_u64().unwrap() > 0);
    assert!(!cards[0]["reasons"].as_array().unwrap().is_empty());
    assert!(cards[0]["reasons"][0]["text"].is_string());
}

#[sqlx::test]
async fn a_better_fit_is_ranked_first(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    complete_profile(&pool, "ada@example.com", "Ada", "engineering", "gtm").await;
    // Wants what Ada has, has what Ada wants.
    complete_profile(&pool, "great@example.com", "Great", "gtm", "engineering").await;
    // Neither.
    complete_profile(&pool, "poor@example.com", "Poor", "research", "design").await;

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();
    let body = json_body(response).await;
    let cards = body["cards"].as_array().unwrap();

    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0]["display_name"], "Great");
    assert!(
        cards[0]["score"].as_u64().unwrap() > cards[1]["score"].as_u64().unwrap(),
        "{cards:?}"
    );
}

#[sqlx::test]
async fn the_deck_never_contains_the_viewer(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    complete_profile(&pool, "ada@example.com", "Ada", "engineering", "gtm").await;

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();
    let body = json_body(response).await;

    assert_eq!(body["cards"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn the_deck_is_capped(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    complete_profile(&pool, "ada@example.com", "Ada", "engineering", "gtm").await;
    for index in 0..25 {
        complete_profile(
            &pool,
            &format!("other{index}@example.com"),
            "Other",
            "gtm",
            "engineering",
        )
        .await;
    }

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();
    let body = json_body(response).await;

    assert_eq!(
        body["cards"].as_array().unwrap().len(),
        cofounder_api::deck::service::DECK_SIZE
    );
}

#[sqlx::test]
async fn a_score_never_leaves_the_zero_to_one_hundred_range(pool: PgPool) {
    // The popularity boost is added after scoring, so a perfect pair must
    // still not be able to exceed the budget.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    complete_profile(&pool, "ada@example.com", "Ada", "engineering", "gtm").await;
    let grace =
        complete_profile(&pool, "grace@example.com", "Grace", "gtm", "engineering").await;

    for index in 0..5 {
        let fan = complete_profile(
            &pool,
            &format!("fan{index}@example.com"),
            "Fan",
            "gtm",
            "engineering",
        )
        .await;
        cofounder_api::swipes::repo::record_swipe(
            &pool,
            fan,
            grace,
            cofounder_api::swipes::repo::Direction::Right,
        )
        .await
        .unwrap();
    }

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();
    let body = json_body(response).await;

    for card in body["cards"].as_array().unwrap() {
        let score = card["score"].as_u64().unwrap();
        assert!(score <= 100, "got {score}");
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test deck_api`
Expected: FAIL — `could not find service in deck`.

- [ ] **Step 3: Implement the service**

Create `backend/src/deck/service.rs`:

```rust
use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::app::AppState;
use crate::deck::repo::{self, Candidate};
use crate::error::ApiResult;
use crate::scoring::profile::Reason;
use crate::scoring::score;

pub const DECK_SIZE: usize = 20;
/// How far a candidate resembling the viewer's recent passes can be pushed
/// down. Deliberately small: it nudges, it does not filter.
pub const MAX_PASS_PENALTY: f64 = 5.0;
/// Capped well below the smallest scoring component so popularity can never
/// outweigh genuine fit.
pub const MAX_POPULARITY_BOOST: f64 = 3.0;
pub const POPULARITY_WINDOW_DAYS: i32 = 30;
pub const RECENT_PASSES: i64 = 20;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeckCard {
    pub user_id: Uuid,
    pub display_name: String,
    pub headline: String,
    pub bio: String,
    pub city: String,
    pub country: String,
    pub roles: Vec<String>,
    pub seeking_roles: Vec<String>,
    pub interests: Vec<String>,
    pub score: u16,
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeckView {
    pub cards: Vec<DeckCard>,
    /// False when the viewer's own profile is not complete. They get an
    /// empty deck and a reason rather than an error, because the frontend
    /// wants to show a prompt, not a failure.
    pub profile_complete: bool,
}

/// Candidates resembling the viewer's recent passes lose ground, in
/// proportion to how much of their own tag set the viewer has been rejecting.
fn pass_penalty(candidate: &Candidate, passed: &HashSet<String>) -> f64 {
    if passed.is_empty() {
        return 0.0;
    }

    let own: Vec<&String> = candidate
        .profile
        .roles
        .iter()
        .chain(candidate.profile.interests.iter())
        .collect();

    if own.is_empty() {
        return 0.0;
    }

    let shared = own.iter().filter(|tag| passed.contains(**tag)).count();

    (shared as f64 / own.len() as f64) * MAX_PASS_PENALTY
}

pub async fn build(state: &AppState, viewer_id: Uuid) -> ApiResult<DeckView> {
    let Some(viewer) = repo::load_profile(&state.db, viewer_id).await? else {
        return Ok(DeckView {
            cards: Vec::new(),
            profile_complete: false,
        });
    };

    let candidates = repo::candidates_for(&state.db, viewer_id).await?;

    let passed: HashSet<String> = repo::recent_pass_tags(&state.db, viewer_id, RECENT_PASSES)
        .await?
        .into_iter()
        .collect();

    let rates: HashMap<Uuid, f64> =
        repo::right_swipe_rates(&state.db, POPULARITY_WINDOW_DAYS)
            .await?
            .into_iter()
            .collect();

    let mut cards: Vec<DeckCard> = candidates
        .into_iter()
        .map(|candidate| {
            let scored = score::score(&viewer.profile, &candidate.profile);

            let penalty = pass_penalty(&candidate, &passed);
            let boost = rates
                .get(&candidate.profile.user_id)
                .copied()
                .unwrap_or(0.0)
                * MAX_POPULARITY_BOOST;

            let adjusted = (f64::from(scored.total) - penalty + boost)
                .clamp(0.0, f64::from(score::MAX_TOTAL));

            DeckCard {
                user_id: candidate.profile.user_id,
                display_name: candidate.profile.display_name,
                headline: candidate.headline,
                bio: candidate.bio,
                city: candidate.profile.city,
                country: candidate.profile.country,
                roles: candidate.profile.roles,
                seeking_roles: candidate.profile.seeking_roles,
                interests: candidate.profile.interests,
                score: adjusted.round() as u16,
                reasons: scored.reasons,
            }
        })
        .collect();

    // Descending by score, then by id so that equal scores keep a stable
    // order across requests rather than shuffling between page loads.
    cards.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.user_id.cmp(&b.user_id))
    });
    cards.truncate(DECK_SIZE);

    Ok(DeckView {
        cards,
        profile_complete: true,
    })
}
```

- [ ] **Step 4: Implement the route**

Create `backend/src/deck/routes.rs`:

```rust
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::app::AppState;
use crate::auth::extractor::CurrentUser;
use crate::deck::service::{self, DeckView};
use crate::error::ApiResult;

pub fn router() -> Router<AppState> {
    Router::new().route("/deck", get(deck))
}

/// Computed on demand. There is no precomputed match table: that is a cache
/// for a load problem this product does not have yet.
async fn deck(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<DeckView>> {
    Ok(Json(service::build(&state, user.id).await?))
}
```

Modify `backend/src/deck/mod.rs`:

```rust
pub mod repo;
pub mod routes;
pub mod service;
```

Modify `backend/src/app.rs` — extend the merge chain:

```rust
        .merge(crate::assessment::routes::router())
        .merge(crate::profiles::routes::router())
        .merge(crate::deck::routes::router());
```

- [ ] **Step 5: Run the tests and verify they pass**

Run: `cd backend && cargo test --test deck_api`
Expected: PASS — 7 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/src/deck backend/src/app.rs backend/tests/deck_api.rs
git commit -m "feat: the deck endpoint with pass suppression and popularity"
git push origin main
```

---

### Task 9: POST /swipes and GET /matches

**Files:**
- Create: `backend/src/swipes/service.rs`, `backend/src/swipes/routes.rs`
- Modify: `backend/src/swipes/mod.rs`, `backend/src/app.rs`, `backend/src/error.rs`
- Test: `backend/tests/swipes_api.rs`

**Interfaces:**
- Consumes: `swipes::repo::{Direction, SwipeOutcome, MatchedUser, record_swipe, matches_for}`, `users::repo::find_by_id`
- Produces:
  - `cofounder_api::swipes::routes::router() -> Router<AppState>` mounting `POST /swipes`, `GET /matches`
  - `cofounder_api::error::ApiError::Conflict` — renders 409 with type slug `conflict`
  - `cofounder_api::swipes::service::{record, list_matches}`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/swipes_api.rs`. Copy the `state_with`, `post_json`, `sign_in`, `get` and `json_body` helpers verbatim from `backend/tests/deck_api.rs` — integration tests do not share modules — then add:

```rust
/// A complete profile, inserted directly so the test does not have to drive
/// eighteen questionnaire answers through the API.
async fn complete_profile(pool: &PgPool, email: &str, name: &str) -> Uuid {
    let id = cofounder_api::users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id;

    sqlx::query(
        "INSERT INTO profiles (user_id, display_name, headline, bio, roles, seeking_roles, commitment)
         VALUES ($1, $2, 'Building things', 'A real bio.', ARRAY['engineering'], ARRAY['gtm'], 'full_time_now')
         ON CONFLICT (user_id) DO UPDATE SET display_name = EXCLUDED.display_name",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO trait_scores (user_id, risk_tolerance, pace_vs_rigor, conflict_style,
                                   decision_basis, work_mode, orientation)
         VALUES ($1, 50, 50, 50, 50, 50, 50)
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();

    id
}

fn swipe(cookie: &str, target: Uuid, direction: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/swipes")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "target_id": target, "direction": direction }).to_string(),
        ))
        .unwrap()
}

#[sqlx::test]
async fn swiping_requires_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/swipes")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "target_id": Uuid::new_v4(),
                        "direction": "right"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn a_swipe_is_recorded(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    let response = router(state)
        .oneshot(swipe(&cookie, grace, "right"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["matched"], false);
}

#[sqlx::test]
async fn a_mutual_right_swipe_reports_a_match(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let ada_cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace_cookie = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    let first = router(state.clone())
        .oneshot(swipe(&ada_cookie, grace, "right"))
        .await
        .unwrap();
    assert_eq!(json_body(first).await["matched"], false);

    let second = router(state)
        .oneshot(swipe(&grace_cookie, ada, "right"))
        .await
        .unwrap();
    assert_eq!(json_body(second).await["matched"], true);
}

#[sqlx::test]
async fn swiping_the_same_person_twice_is_a_conflict(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    router(state.clone())
        .oneshot(swipe(&cookie, grace, "right"))
        .await
        .unwrap();

    let repeat = router(state)
        .oneshot(swipe(&cookie, grace, "left"))
        .await
        .unwrap();

    assert_eq!(repeat.status(), StatusCode::CONFLICT);
    let body = json_body(repeat).await;
    assert_eq!(body["type"], "conflict");
}

#[sqlx::test]
async fn swiping_on_yourself_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;

    let response = router(state)
        .oneshot(swipe(&cookie, ada, "right"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn swiping_on_a_stranger_who_does_not_exist_is_a_404(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(swipe(&cookie, Uuid::new_v4(), "right"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn an_unknown_direction_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    let response = router(state)
        .oneshot(swipe(&cookie, grace, "sideways"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn matches_are_listed_for_both_sides(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let ada_cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace_cookie = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    router(state.clone())
        .oneshot(swipe(&ada_cookie, grace, "right"))
        .await
        .unwrap();
    router(state.clone())
        .oneshot(swipe(&grace_cookie, ada, "right"))
        .await
        .unwrap();

    let for_ada = json_body(
        router(state.clone())
            .oneshot(get("/matches", &ada_cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(for_ada["matches"][0]["display_name"], "Grace");

    let for_grace = json_body(
        router(state).oneshot(get("/matches", &grace_cookie)).await.unwrap(),
    )
    .await;
    assert_eq!(for_grace["matches"][0]["display_name"], "Ada");
}

#[sqlx::test]
async fn matches_start_empty(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let body = json_body(router(state).oneshot(get("/matches", &cookie)).await.unwrap()).await;

    assert_eq!(body["matches"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn matches_require_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(Request::builder().uri("/matches").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test swipes_api`
Expected: FAIL — the routes are not mounted, so the assertions on 201 and 409 fail.

- [ ] **Step 3: Add the conflict error**

Modify `backend/src/error.rs`.

Add the variant, after `NotFound`:

```rust
    /// The request contradicts something already recorded. Used for a repeat
    /// swipe: swipes are permanent, so a second one is neither an update nor
    /// a success.
    #[error("already done")]
    Conflict,
```

Add to `status()`:

```rust
            ApiError::Conflict => StatusCode::CONFLICT,
```

Add to `type_slug()`:

```rust
            ApiError::Conflict => "conflict",
```

- [ ] **Step 4: Implement the service**

Create `backend/src/swipes/service.rs`:

```rust
use uuid::Uuid;

use crate::app::AppState;
use crate::error::{ApiError, ApiResult, FieldError};
use crate::swipes::repo::{self, Direction, MatchedUser, SwipeOutcome};
use crate::users;

pub async fn record(
    state: &AppState,
    swiper_id: Uuid,
    target_id: Uuid,
    direction: Direction,
) -> ApiResult<SwipeOutcome> {
    if swiper_id == target_id {
        return Err(ApiError::Validation(vec![FieldError {
            field: "target_id".into(),
            message: "you cannot swipe on yourself".into(),
        }]));
    }

    if users::repo::find_by_id(&state.db, target_id).await?.is_none() {
        return Err(ApiError::NotFound);
    }

    repo::record_swipe(&state.db, swiper_id, target_id, direction)
        .await?
        .ok_or(ApiError::Conflict)
}

pub async fn list_matches(state: &AppState, user_id: Uuid) -> ApiResult<Vec<MatchedUser>> {
    Ok(repo::matches_for(&state.db, user_id).await?)
}
```

- [ ] **Step 5: Implement the routes**

Create `backend/src/swipes/routes.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::extractor::CurrentUser;
use crate::error::ApiResult;
use crate::swipes::repo::{Direction, MatchedUser, SwipeOutcome};
use crate::swipes::service;

/// An unknown direction fails deserialization, which axum renders as 422 —
/// the same shape a validation failure produces.
#[derive(serde::Deserialize)]
pub struct SwipeRequest {
    pub target_id: Uuid,
    pub direction: Direction,
}

#[derive(serde::Serialize)]
pub struct MatchesView {
    pub matches: Vec<MatchedUser>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/swipes", post(create_swipe))
        .route("/matches", get(list_matches))
}

async fn create_swipe(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(payload): Json<SwipeRequest>,
) -> ApiResult<(StatusCode, Json<SwipeOutcome>)> {
    let outcome =
        service::record(&state, user.id, payload.target_id, payload.direction).await?;

    Ok((StatusCode::CREATED, Json(outcome)))
}

async fn list_matches(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<MatchesView>> {
    Ok(Json(MatchesView {
        matches: service::list_matches(&state, user.id).await?,
    }))
}
```

Modify `backend/src/swipes/mod.rs`:

```rust
pub mod repo;
pub mod routes;
pub mod service;
```

Modify `backend/src/app.rs` — extend the merge chain:

```rust
        .merge(crate::deck::routes::router())
        .merge(crate::swipes::routes::router());
```

- [ ] **Step 6: Run the tests and verify they pass**

Run: `cd backend && cargo test --test swipes_api`
Expected: PASS — 10 tests.

If `an_unknown_direction_is_rejected` sees 400 rather than 422, axum's JSON rejection is being rendered rather than `ApiError`. Add an explicit check instead: change `SwipeRequest.direction` to `String` and map it in the handler with

```rust
let direction = match payload.direction.as_str() {
    "left" => Direction::Left,
    "right" => Direction::Right,
    _ => {
        return Err(ApiError::Validation(vec![FieldError {
            field: "direction".into(),
            message: "must be left or right".into(),
        }]))
    }
};
```

- [ ] **Step 7: Run the whole backend suite and commit**

Run: `cd backend && cargo test`
Expected: PASS.

```bash
git add backend/src backend/tests/swipes_api.rs
git commit -m "feat: swipe endpoint, match creation, and the match list"
git push origin main
```

---

### Task 10: The deck page

**Files:**
- Create: `frontend/lib/deck.ts`, `frontend/app/(app)/deck/page.tsx`, `frontend/app/(app)/deck/deck-client.tsx`
- Test: covered end-to-end in Task 11

**Interfaces:**
- Consumes: `GET /api/deck`, `POST /api/swipes`, `apiFetch` and `ApiError` from `frontend/lib/api.ts`
- Produces: `frontend/lib/deck.ts` exporting `DeckCard`, `DeckView`, `SwipeOutcome`, `MatchSummary`, `MatchesView`

- [ ] **Step 1: Read the framework docs**

`frontend/AGENTS.md` requires this before any frontend code:

```bash
cd frontend
cat node_modules/next/dist/docs/01-app/01-getting-started/05-server-and-client-components.md
```

- [ ] **Step 2: Write the shared types**

Create `frontend/lib/deck.ts`:

```ts
export interface Reason {
  component: string;
  text: string;
}

export interface DeckCard {
  user_id: string;
  display_name: string;
  headline: string;
  bio: string;
  city: string;
  country: string;
  roles: string[];
  seeking_roles: string[];
  interests: string[];
  score: number;
  reasons: Reason[];
}

export interface DeckView {
  cards: DeckCard[];
  profile_complete: boolean;
}

export interface SwipeOutcome {
  matched: boolean;
}

export interface MatchSummary {
  user_id: string;
  display_name: string;
  headline: string;
  matched_at: string;
}

export interface MatchesView {
  matches: MatchSummary[];
}
```

- [ ] **Step 3: Write the page shell**

Create `frontend/app/(app)/deck/page.tsx`:

```tsx
import DeckClient from "./deck-client";

export default function DeckPage() {
  return <DeckClient />;
}
```

- [ ] **Step 4: Write the deck client**

Create `frontend/app/(app)/deck/deck-client.tsx`:

```tsx
"use client";

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { apiFetch } from "@/lib/api";
import { DeckCard, DeckView, SwipeOutcome } from "@/lib/deck";
import { Choice, Options } from "@/lib/profile";

export default function DeckClient() {
  const [cards, setCards] = useState<DeckCard[]>([]);
  const [options, setOptions] = useState<Options | null>(null);
  const [complete, setComplete] = useState(true);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [matched, setMatched] = useState<DeckCard | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([apiFetch<DeckView>("/deck"), apiFetch<Options>("/options")])
      .then(([deck, loadedOptions]) => {
        setCards(deck.cards);
        setComplete(deck.profile_complete);
        setOptions(loadedOptions);
        setLoading(false);
      })
      .catch(() => {
        setError("Could not load your deck. Reload to try again.");
        setLoading(false);
      });
  }, []);

  const current = cards[0];

  const swipe = useCallback(
    async (direction: "left" | "right") => {
      if (!current || busy) return;

      setBusy(true);
      setError(null);

      try {
        const outcome = await apiFetch<SwipeOutcome>("/swipes", {
          method: "POST",
          body: JSON.stringify({
            target_id: current.user_id,
            direction,
          }),
        });

        if (outcome.matched) setMatched(current);
        // Permanent either way, so the card never returns to the deck.
        setCards((rest) => rest.slice(1));
      } catch {
        setError("That didn't save. Try again.");
      } finally {
        setBusy(false);
      }
    },
    [current, busy],
  );

  // Arrow keys are the keyboard equivalent of the two buttons. A modal is
  // open when a match happens, so keys are ignored until it is dismissed.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (matched) return;
      if (event.key === "ArrowLeft") swipe("left");
      if (event.key === "ArrowRight") swipe("right");
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [swipe, matched]);

  function labelFor(choices: Choice[] | undefined, id: string): string {
    return choices?.find((choice) => choice.id === id)?.label ?? id;
  }

  if (loading) return <p className="text-neutral-600">Loading your deck…</p>;

  if (!complete) {
    return (
      <div className="flex max-w-xl flex-col gap-3">
        <h1 className="text-2xl font-semibold">Finish your profile first</h1>
        <p className="text-neutral-600">
          The deck opens once your profile is complete — that is what keeps it
          free of empty accounts.
        </p>
        <Link href="/home" className="underline">
          See what&apos;s left
        </Link>
      </div>
    );
  }

  return (
    <div className="flex max-w-xl flex-col gap-4">
      <h1 className="text-2xl font-semibold">Your deck</h1>

      {error && (
        <p role="alert" className="text-sm text-red-600">
          {error}
        </p>
      )}

      {matched && (
        <div
          role="dialog"
          aria-modal="true"
          aria-labelledby="match-heading"
          className="rounded-xl border border-neutral-900 bg-neutral-50 p-4"
        >
          <h2 id="match-heading" className="text-lg font-semibold">
            It&apos;s a match
          </h2>
          <p className="mt-1 text-neutral-700">
            You and {matched.display_name} both swiped right.
          </p>
          <div className="mt-3 flex gap-3">
            <Link href="/matches" className="underline">
              See your matches
            </Link>
            <button
              type="button"
              onClick={() => setMatched(null)}
              className="underline"
            >
              Keep swiping
            </button>
          </div>
        </div>
      )}

      {!current ? (
        <p id="deck-empty" className="text-neutral-600">
          That&apos;s everyone for now. Check back as more founders join.
        </p>
      ) : (
        <article className="flex flex-col gap-3 rounded-xl border border-neutral-200 p-5">
          <div className="flex items-baseline justify-between gap-3">
            <h2 className="text-xl font-semibold">{current.display_name}</h2>
            <span
              aria-label={`Match score ${current.score} out of 100`}
              className="text-sm text-neutral-600"
            >
              {current.score}
            </span>
          </div>

          {current.headline && (
            <p className="text-neutral-700">{current.headline}</p>
          )}

          {current.reasons.length > 0 && (
            <ul className="flex flex-wrap gap-2">
              {current.reasons.map((reason) => (
                <li
                  key={reason.component}
                  className="rounded-full bg-neutral-100 px-3 py-1 text-sm text-neutral-800"
                >
                  {reason.text}
                </li>
              ))}
            </ul>
          )}

          <p className="whitespace-pre-line text-neutral-700">{current.bio}</p>

          <dl className="flex flex-col gap-1 text-sm text-neutral-600">
            <div className="flex gap-2">
              <dt className="font-medium">Brings</dt>
              <dd>
                {current.roles
                  .map((role) => labelFor(options?.roles, role))
                  .join(", ")}
              </dd>
            </div>
            <div className="flex gap-2">
              <dt className="font-medium">Looking for</dt>
              <dd>
                {current.seeking_roles
                  .map((role) => labelFor(options?.roles, role))
                  .join(", ")}
              </dd>
            </div>
            {current.city && (
              <div className="flex gap-2">
                <dt className="font-medium">Based in</dt>
                <dd>
                  {current.city}
                  {current.country ? `, ${current.country}` : ""}
                </dd>
              </div>
            )}
          </dl>

          <div className="mt-2 flex gap-3">
            <button
              type="button"
              disabled={busy}
              onClick={() => swipe("left")}
              className="rounded-lg border border-neutral-300 px-4 py-2 disabled:opacity-50"
            >
              Pass
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => swipe("right")}
              className="rounded-lg bg-neutral-900 px-4 py-2 text-white disabled:opacity-50"
            >
              Interested
            </button>
          </div>
        </article>
      )}
    </div>
  );
}
```

- [ ] **Step 5: Check it compiles and lints**

Run: `cd frontend && npx tsc --noEmit && npm run lint`
Expected: PASS, no errors.

- [ ] **Step 6: Commit**

```bash
git add frontend/lib/deck.ts "frontend/app/(app)/deck"
git commit -m "feat: the swipe deck"
git push origin main
```

---

### Task 11: Matches page, navigation, and the end-to-end journey

**Files:**
- Create: `frontend/app/(app)/matches/page.tsx`, `frontend/app/(app)/matches/matches-client.tsx`, `frontend/e2e/deck.spec.ts`
- Modify: `frontend/app/(app)/layout.tsx`, `frontend/app/(app)/home/page.tsx`, `frontend/e2e/helpers.ts`

**Interfaces:**
- Consumes: `GET /api/matches`, `signIn` and `uniqueEmail` from `frontend/e2e/helpers.ts`
- Produces: `frontend/e2e/helpers.ts` exporting `completeOnboarding(page): Promise<void>` — fills a profile and all 18 answers through the API using the browser context's own cookie

- [ ] **Step 1: Write the failing test**

Modify `frontend/e2e/helpers.ts` — append:

```ts
/**
 * Brings the signed-in user to a complete profile through the API rather
 * than the UI. Driving eighteen questionnaire answers through the browser
 * for every deck test is slow and tests nothing the assessment specs do not
 * already cover. `page.request` shares the page's cookies, so this runs as
 * the signed-in user.
 */
export async function completeOnboarding(
  page: Page,
  overrides: Record<string, unknown> = {},
): Promise<void> {
  const profile = {
    display_name: "Test Founder",
    headline: "Building something",
    bio: "A real bio, long enough to count.",
    city: "Jakarta",
    country: "Indonesia",
    timezone: "Asia/Jakarta",
    roles: ["engineering"],
    seeking_roles: ["gtm"],
    idea_status: "committed_idea",
    stage: "prototype",
    commitment: "full_time_now",
    interests: ["ai_ml"],
    ...overrides,
  };

  const saved = await page.request.put("/api/me/profile", { data: profile });
  if (!saved.ok()) {
    throw new Error(`profile save failed: ${saved.status()} ${await saved.text()}`);
  }

  const questions = await (await page.request.get("/api/questions")).json();
  const responses = questions.questions.map((question: { id: string }) => ({
    question_id: question.id,
    value: 3,
  }));

  const answered = await page.request.put("/api/me/responses", {
    data: { responses },
  });
  if (!answered.ok()) {
    throw new Error(`answers failed: ${answered.status()} ${await answered.text()}`);
  }
}
```

Create `frontend/e2e/deck.spec.ts`:

```ts
import { expect, test } from "@playwright/test";
import { completeOnboarding, signIn } from "./helpers";

test("a founder sees a scored candidate and can pass on them", async ({
  page,
  request,
}) => {
  // A candidate for the deck to contain.
  const other = await page.context().browser()!.newContext();
  const otherPage = await other.newPage();
  await signIn(otherPage, otherPage.request, "candidate");
  await completeOnboarding(otherPage, {
    display_name: "Grace Hopper",
    roles: ["gtm"],
    seeking_roles: ["engineering"],
  });
  await otherPage.close();
  await other.close();

  await signIn(page, request, "viewer");
  await completeOnboarding(page);

  await page.getByRole("link", { name: "Deck" }).click();
  await expect(page.getByRole("heading", { name: "Your deck" })).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Grace Hopper" }),
  ).toBeVisible();

  // The card explains itself.
  await expect(page.getByText("They bring GTM / Sales")).toBeVisible();

  await page.getByRole("button", { name: "Pass" }).click();
  await expect(page.locator("#deck-empty")).toBeVisible();

  // A pass is permanent, so a reload does not bring them back.
  await page.reload();
  await expect(page.locator("#deck-empty")).toBeVisible();
});

test("an incomplete profile is told to finish before swiping", async ({
  page,
  request,
}) => {
  await signIn(page, request, "empty");

  await page.goto("/deck");

  await expect(
    page.getByRole("heading", { name: "Finish your profile first" }),
  ).toBeVisible();
});

test("a mutual right swipe creates a match both founders can see", async ({
  page,
  browser,
  request,
}) => {
  const viewerEmail = await signIn(page, request, "first");
  await completeOnboarding(page, { display_name: "Ada Lovelace" });

  const secondContext = await browser.newContext();
  const secondPage = await secondContext.newPage();
  await signIn(secondPage, secondPage.request, "second");
  await completeOnboarding(secondPage, {
    display_name: "Grace Hopper",
    roles: ["gtm"],
    seeking_roles: ["engineering"],
  });

  // Ada swipes right first; no match yet.
  await page.goto("/deck");
  await expect(
    page.getByRole("heading", { name: "Grace Hopper" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Interested" }).click();
  await expect(page.getByRole("dialog")).toBeHidden();

  // Grace swipes back, which completes the match.
  await secondPage.goto("/deck");
  await expect(
    secondPage.getByRole("heading", { name: "Ada Lovelace" }),
  ).toBeVisible();
  await secondPage.getByRole("button", { name: "Interested" }).click();

  await expect(secondPage.getByRole("dialog")).toBeVisible();
  await expect(secondPage.getByText("It's a match")).toBeVisible();

  // Both sides see it on the matches page.
  await secondPage.getByRole("link", { name: "See your matches" }).click();
  await expect(secondPage.getByText("Ada Lovelace")).toBeVisible();

  await page.goto("/matches");
  await expect(page.getByText("Grace Hopper")).toBeVisible();

  expect(viewerEmail).toContain("first+");

  await secondPage.close();
  await secondContext.close();
});

test("the deck can be driven from the keyboard", async ({
  page,
  browser,
  request,
}) => {
  const otherContext = await browser.newContext();
  const otherPage = await otherContext.newPage();
  await signIn(otherPage, otherPage.request, "keyboardtarget");
  await completeOnboarding(otherPage, { display_name: "Alan Turing" });
  await otherPage.close();
  await otherContext.close();

  await signIn(page, request, "keyboard");
  await completeOnboarding(page);

  await page.goto("/deck");
  await expect(page.getByRole("heading", { name: "Alan Turing" })).toBeVisible();

  await page.keyboard.press("ArrowLeft");

  await expect(page.locator("#deck-empty")).toBeVisible();
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd frontend && npm run test:e2e -- deck.spec.ts`
Expected: FAIL — there is no `Deck` navigation link and no `/matches` page.

- [ ] **Step 3: Write the matches page**

Create `frontend/app/(app)/matches/page.tsx`:

```tsx
import MatchesClient from "./matches-client";

export default function MatchesPage() {
  return <MatchesClient />;
}
```

Create `frontend/app/(app)/matches/matches-client.tsx`:

```tsx
"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { apiFetch } from "@/lib/api";
import { MatchSummary, MatchesView } from "@/lib/deck";

export default function MatchesClient() {
  const [matches, setMatches] = useState<MatchSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    apiFetch<MatchesView>("/matches")
      .then((view) => {
        setMatches(view.matches);
        setLoading(false);
      })
      .catch(() => {
        setError("Could not load your matches. Reload to try again.");
        setLoading(false);
      });
  }, []);

  if (loading) return <p className="text-neutral-600">Loading your matches…</p>;

  if (error) {
    return (
      <p role="alert" className="text-red-600">
        {error}
      </p>
    );
  }

  if (matches.length === 0) {
    return (
      <div className="flex max-w-xl flex-col gap-3">
        <h1 className="text-2xl font-semibold">No matches yet</h1>
        <p className="text-neutral-600">
          A match happens when you and another founder both swipe right.
        </p>
        <Link href="/deck" className="underline">
          Open your deck
        </Link>
      </div>
    );
  }

  return (
    <div className="flex max-w-xl flex-col gap-4">
      <h1 className="text-2xl font-semibold">Your matches</h1>
      <ul className="flex flex-col gap-3">
        {matches.map((match) => (
          <li
            key={match.user_id}
            className="rounded-xl border border-neutral-200 p-4"
          >
            <p className="font-medium">{match.display_name}</p>
            {match.headline && (
              <p className="text-sm text-neutral-600">{match.headline}</p>
            )}
          </li>
        ))}
      </ul>
      <p className="text-sm text-neutral-600">
        Messaging arrives in the next slice.
      </p>
    </div>
  );
}
```

- [ ] **Step 4: Add the navigation links**

Modify `frontend/app/(app)/layout.tsx` — add two links inside the `<nav>`, after the Assessment link:

```tsx
          <Link href="/deck" className="text-sm text-neutral-700 hover:underline">
            Deck
          </Link>
          <Link
            href="/matches"
            className="text-sm text-neutral-700 hover:underline"
          >
            Matches
          </Link>
```

- [ ] **Step 5: Point a complete profile at the deck**

Modify `frontend/app/(app)/home/page.tsx` — in the `view.complete` branch, replace the paragraph with a link onward:

```tsx
  if (view.complete) {
    return (
      <div className="flex flex-col gap-2">
        <h1 className="text-2xl font-semibold">Your profile is complete</h1>
        <p className="text-neutral-600">
          You&apos;re in the deck, and other founders can see you.
        </p>
        <Link href="/deck" className="underline">
          Open your deck
        </Link>
      </div>
    );
  }
```

- [ ] **Step 6: Run the end-to-end suite and verify it passes**

Run: `cd frontend && npm run test:e2e`
Expected: PASS — slice 1's auth specs, slice 2's profile specs, and the four new deck specs.

- [ ] **Step 7: Run every test in the repository**

Run: `cd backend && cargo test`
Expected: PASS.

Run: `cd frontend && npx tsc --noEmit && npm run lint`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add "frontend/app/(app)" frontend/e2e
git commit -m "feat: matches page, deck navigation, and end-to-end match coverage"
git push origin main
```

---

## Self-Review

Checked against `docs/superpowers/specs/2026-07-25-cofounder-matching-design.md`.

**Covered.** The scorer's signature and purity (Tasks 1–4). All five components at their specified budgets — roles 30, traits 25, commitment and stage 20, interests 15, geography 10 — with a test asserting they sum to exactly 100 (Task 4). The per-axis similarity-versus-complementarity table, with a test naming each axis's ideal explicitly and two tests proving twins beat opposites on a similarity axis and lose on a complementary one (Task 2). Jaccard for interests (Task 3). Same metro full, three timezones partial, otherwise zero (Task 3). Reasons produced by the scorer alongside the number, top three, drawn from the highest-contributing components (Task 4). Deck generation's four steps: SQL filter, score in Rust, adjust, return the top 20 (Tasks 7–8). Every exclusion the spec lists — self, prior swipes, blocks in either direction, suspended, incomplete (Task 7). Pass suppression capped at 5 and popularity capped at 3 over a trailing 30 days, neither requiring training or batch jobs (Task 8). Right and left swipes both permanent and both excluding from future decks (Tasks 6–7). Mutual right swipe creating a match and returning a match indicator the frontend renders as a match moment (Tasks 9–10). `matches` sorted newest first (Task 6). `GET /deck`, `POST /swipes`, `GET /matches` (Tasks 8–9). Scorer unit tests as the largest body of tests; repository tests against a throwaway Postgres covering deck filtering and match creation; request-level API tests including permission boundaries (throughout).

**Deliberate decisions recorded in the plan rather than the spec.**
- The spec says matching does not gate messaging; nothing here gates anything on a match, and `matches` is stored purely as the quality signal the spec describes.
- Ideal distances (0, 25, 60) and falloff ladders are numbers the spec describes qualitatively ("similar", "mild difference", "complementary"; "identical full, adjacent most, distant near zero"). They are named constants with tests asserting the ordering rather than the exact values, so tuning them does not rewrite the suite.
- `blocks` is created here though its endpoint is slice 4, so the deck's exclusion is never retrofitted.
- `utc_offset_minutes` is derived at save time to keep the scorer pure.
- Deck interaction is buttons plus arrow keys, not drag gestures.

**Deferred to slice 4.** Conversations, messages, SSE, the new-conversation and per-minute rate limits, `POST /blocks`, and `POST /reports`. The spec's full end-to-end path ends at "message"; Task 11 covers it as far as the match.

**Type consistency.** `ScoredProfile` is constructed identically in every scoring test and by `deck::repo::Candidate::from`. `ComponentScore` is returned by all five `score_*` functions and consumed only by `score::score`. `MAX_POINTS` is defined per component module and referenced by that module's own tests. `Direction` is one enum shared by `swipes::repo`, `swipes::routes`, and the deck tests. `DeckCard` in `deck::service` matches `DeckCard` in `frontend/lib/deck.ts` field for field, and `MatchedUser` matches `MatchSummary`. `Reason { component, text }` is identical in Rust and TypeScript.
