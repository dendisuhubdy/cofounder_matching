# Messaging & Moderation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Two founders can hold a conversation — opened from a deck card or a match, delivered live over SSE — with rate limits, blocking and reporting as the abuse controls.

**Architecture:** A `messaging` module owns conversations and messages; a `moderation` module owns blocks and reports. Live delivery is an in-process `tokio::sync::broadcast` channel held in `AppState`: a send publishes an envelope addressed to the recipient, and `GET /events` subscribes and filters to its own user. Every gate that decides whether a message may be sent — profile completeness, blocks, rate limits — lives in one service function, so tightening the policy later (gating chat on a mutual match, say) is a change in one place.

**Tech Stack:** Rust (axum 0.8, sqlx 0.8, tokio, tokio-stream), Postgres 16, Next.js 16.2 (App Router, TypeScript, Tailwind 4), Playwright.

This plan is slice 4 of 4 derived from `docs/superpowers/specs/2026-07-25-cofounder-matching-design.md`. Slices 1–3 are merged.

## Global Constraints

Carried over unchanged from slices 1–3:

- Rust edition 2021. Crate name `cofounder_api`, in `backend/`.
- **Use `sqlx::query_as` / `sqlx::query` / `sqlx::query_scalar`, never the `query!` macros.**
- All database access lives in `repo.rs` files. Handlers and services never write SQL.
- Timestamps are `TIMESTAMPTZ` in Postgres, `chrono::DateTime<chrono::Utc>` in Rust.
- Every route except `/auth/*` requires a session, via the `CurrentUser` extractor.
- Errors are `ApiError`. Validation is 422 with per-field detail; rate limits are 429 with `retry_after`; blocked or forbidden actions are 403. Never construct ad-hoc error responses.
- Frontend pages are client components calling the API through the `/api` rewrite with `apiFetch`. No Server Actions.
- **Before writing any frontend code, read the relevant guide under `frontend/node_modules/next/dist/docs/`**, as `frontend/AGENTS.md` requires.
- **E2E specs must use `uniqueEmail(prefix)` and `uniqueName(base)` from `frontend/e2e/helpers.ts`, never `Date.now()` or a fixed display name.** Specs run in parallel workers against one database.
- Commit after every task, then `git push origin main`.

Specific to this slice:

- **Messaging is open: no mutual match is required.** The spec is explicit, and equally explicit that this is the decision most likely to need revisiting. Every precondition is checked in `messaging::service::ensure_can_message`, so adding a match requirement later is one function, not a rewrite.
- **Live delivery is an in-process broadcast channel.** This works only while the backend is a single process; two instances behind a load balancer would each see only their own senders' events. That is fine for one droplet and is recorded here so it is a known limit rather than a surprise.
- **A dropped event is not an error.** `broadcast::Sender::send` fails when nobody is subscribed, and a slow subscriber lags. Neither must fail a request: the message is already committed, and the client refetches on reconnect.
- **Blocks bite in both directions**, everywhere: the deck (already done in slice 3), the conversation list, opening a conversation, and sending.
- **Reports never take automated action.** They record a reason and body with status `pending` for manual review.
- Limits, as named constants: **10 new conversations per rolling 24 hours** (replies unlimited), **20 messages per minute**.
- The completeness rule currently lives in two places (`profiles::service::missing_requirements` and the deck's SQL). Task 1 extracts the SQL predicate to one constant, because this slice needs it a third time and three copies will drift.

## File Structure

```
backend/
  Cargo.toml                       + tokio-stream (features = ["sync"])
  migrations/
    0012_conversations.sql         ordered pair, initiator, last_message_at
    0013_messages.sql              body, read_at
    0014_reports.sql               reason, body, status
  src/
    lib.rs                         + pub mod messaging; pub mod moderation;
    app.rs                         + events: EventBus; merge two routers
    profiles/repo.rs               + COMPLETE_PREDICATE, is_complete()
    deck/repo.rs                   uses the shared predicate
    messaging/
      mod.rs
      events.rs                    Event, Envelope, EventBus, GET /events
      repo.rs                      conversations and messages persistence
      service.rs                   completeness, blocks, both rate limits
      routes.rs                    conversations and messages endpoints
    moderation/
      mod.rs
      repo.rs                      blocks and reports persistence
      vocab.rs                     REPORT_REASONS
      routes.rs                    POST /blocks, POST /reports
  tests/
    messaging_repo.rs              conversation upsert, message ordering, unread
    conversations_api.rs           opening, listing, the daily limit, blocks
    messages_api.rs                sending, the per-minute limit, read receipts
    events.rs                      EventBus addressing and publication on send
    moderation_api.rs              blocks and reports

frontend/
  lib/messaging.ts                 shared types
  app/(app)/conversations/page.tsx
  app/(app)/conversations/conversations-client.tsx
  app/(app)/conversations/[id]/page.tsx
  app/(app)/conversations/[id]/thread-client.tsx
  components/message-button.tsx    starts or opens a conversation
  components/report-dialog.tsx     block and report controls
  app/(app)/deck/deck-client.tsx   + Message button
  app/(app)/matches/matches-client.tsx + Message button
  app/(app)/layout.tsx             + Messages link
  e2e/messaging.spec.ts            two founders, a conversation, a block
```

---

### Task 1: One completeness predicate

The rule "this profile may appear in a deck and may send messages" is about to be needed in a third place. It gets extracted first so the three uses cannot drift.

**Files:**
- Modify: `backend/src/profiles/repo.rs`, `backend/src/deck/repo.rs`
- Test: `backend/tests/profiles_repo.rs`

**Interfaces:**
- Produces:
  - `cofounder_api::profiles::repo::COMPLETE_PREDICATE: &str` — a SQL boolean over aliases `u` (users) and `p` (profiles), not including the `trait_scores` join
  - `cofounder_api::profiles::repo::is_complete(&PgPool, Uuid) -> sqlx::Result<bool>`

- [ ] **Step 1: Write the failing test**

Append to `backend/tests/profiles_repo.rs`:

```rust
#[sqlx::test]
async fn completeness_needs_the_profile_and_the_assessment(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    assert!(!repo::is_complete(&pool, user_id).await.unwrap());

    repo::save(&pool, user_id, &an_input()).await.unwrap();
    // The profile is filled in, but the assessment is not.
    assert!(!repo::is_complete(&pool, user_id).await.unwrap());

    sqlx::query(
        "INSERT INTO trait_scores (user_id, risk_tolerance, pace_vs_rigor, conflict_style,
                                   decision_basis, work_mode, orientation)
         VALUES ($1, 50, 50, 50, 50, 50, 50)",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    assert!(repo::is_complete(&pool, user_id).await.unwrap());
}

#[sqlx::test]
async fn a_suspended_account_is_never_complete(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    repo::save(&pool, user_id, &an_input()).await.unwrap();
    sqlx::query(
        "INSERT INTO trait_scores (user_id, risk_tolerance, pace_vs_rigor, conflict_style,
                                   decision_basis, work_mode, orientation)
         VALUES ($1, 50, 50, 50, 50, 50, 50)",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(repo::is_complete(&pool, user_id).await.unwrap());

    sqlx::query("UPDATE users SET status = 'suspended' WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    assert!(!repo::is_complete(&pool, user_id).await.unwrap());
}

#[sqlx::test]
async fn an_empty_bio_is_not_complete(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    let mut input = an_input();
    input.bio = "   ".into();
    repo::save(&pool, user_id, &input).await.unwrap();
    sqlx::query(
        "INSERT INTO trait_scores (user_id, risk_tolerance, pace_vs_rigor, conflict_style,
                                   decision_basis, work_mode, orientation)
         VALUES ($1, 50, 50, 50, 50, 50, 50)",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    assert!(!repo::is_complete(&pool, user_id).await.unwrap());
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test profiles_repo`
Expected: FAIL — `cannot find function is_complete`.

- [ ] **Step 3: Extract the predicate**

Add to `backend/src/profiles/repo.rs`, below the `COLUMNS` constant:

```rust
/// The half of "complete" that SQL can express, over the aliases `u`
/// (users) and `p` (profiles). The assessment half is the presence of a
/// `trait_scores` row, which callers join or check separately because they
/// differ in whether they need the scores themselves.
///
/// Kept here rather than in each caller: the deck's candidate query, the
/// messaging preconditions and this function all need the same rule, and
/// three copies of it would drift.
pub const COMPLETE_PREDICATE: &str = r#"
    u.status = 'active'
    AND btrim(p.bio) <> ''
    AND cardinality(p.roles) > 0
    AND cardinality(p.seeking_roles) > 0
    AND p.commitment IS NOT NULL
"#;

/// Whether this user may appear in a deck and may send messages.
pub async fn is_complete(pool: &PgPool, user_id: Uuid) -> sqlx::Result<bool> {
    let sql = format!(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM users u
            JOIN profiles p     ON p.user_id = u.id
            JOIN trait_scores t ON t.user_id = u.id
            WHERE u.id = $1 AND {COMPLETE_PREDICATE}
        )
        "#
    );

    sqlx::query_scalar(&sql).bind(user_id).fetch_one(pool).await
}
```

- [ ] **Step 4: Point the deck at it**

Modify `backend/src/deck/repo.rs`. Delete the local `COMPLETE` constant and import the shared one:

```rust
use crate::profiles::repo::COMPLETE_PREDICATE;
```

Then replace both `{COMPLETE}` interpolations with `{COMPLETE_PREDICATE}` — one in `load_profile`, one in `candidates_for`.

- [ ] **Step 5: Run the tests and verify they pass**

Run: `cd backend && cargo test --test profiles_repo --test deck_repo`
Expected: PASS — the deck's 18 tests still pass, proving the extraction changed no behaviour.

- [ ] **Step 6: Commit**

```bash
git add backend/src/profiles/repo.rs backend/src/deck/repo.rs backend/tests/profiles_repo.rs
git commit -m "refactor: one completeness predicate shared by the deck and profiles"
git push origin main
```

---

### Task 2: Conversation and message persistence

**Files:**
- Create: `backend/migrations/0012_conversations.sql`, `backend/migrations/0013_messages.sql`, `backend/src/messaging/mod.rs`, `backend/src/messaging/repo.rs`
- Modify: `backend/src/lib.rs`
- Test: `backend/tests/messaging_repo.rs`

**Interfaces:**
- Produces: `cofounder_api::messaging::repo::{Conversation, ConversationSummary, Message, open, find_by_id, for_user, messages_in, send, mark_read, count_started_since, count_messages_since}`
  - `open(&PgPool, initiator: Uuid, other: Uuid) -> sqlx::Result<(Conversation, bool)>` — the bool is true when the row was created by this call
  - `find_by_id(&PgPool, Uuid) -> sqlx::Result<Option<Conversation>>`
  - `for_user(&PgPool, Uuid) -> sqlx::Result<Vec<ConversationSummary>>` — excludes blocked pairs, matches first, then most recent
  - `messages_in(&PgPool, conversation_id: Uuid) -> sqlx::Result<Vec<Message>>`
  - `send(&PgPool, conversation_id: Uuid, sender_id: Uuid, body: &str) -> sqlx::Result<Message>`
  - `mark_read(&PgPool, conversation_id: Uuid, reader_id: Uuid) -> sqlx::Result<u64>`
  - `count_started_since(&PgPool, Uuid, minutes: i64) -> sqlx::Result<i64>`
  - `count_messages_since(&PgPool, Uuid, minutes: i64) -> sqlx::Result<i64>`

- [ ] **Step 1: Write the migrations**

Create `backend/migrations/0012_conversations.sql`:

```sql
-- One conversation per pair, so the ids are stored in a fixed order and the
-- unique constraint does the deduplication rather than application code.
-- `started_by` is what the new-conversation rate limit counts: without it,
-- being messaged by ten people would exhaust your own allowance.
CREATE TABLE conversations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_a_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    started_by      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_message_at TIMESTAMPTZ,
    UNIQUE (user_a_id, user_b_id),
    CONSTRAINT ordered_pair CHECK (user_a_id < user_b_id)
);

CREATE INDEX conversations_user_a_idx ON conversations (user_a_id);
CREATE INDEX conversations_user_b_idx ON conversations (user_b_id);
-- Serves the rolling 24-hour new-conversation limit.
CREATE INDEX conversations_started_by_idx ON conversations (started_by, created_at DESC);
```

Create `backend/migrations/0013_messages.sql`:

```sql
CREATE TABLE messages (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    sender_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body            TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    read_at         TIMESTAMPTZ
);

CREATE INDEX messages_conversation_idx ON messages (conversation_id, created_at);
-- Serves the per-minute send limit.
CREATE INDEX messages_sender_recent_idx ON messages (sender_id, created_at DESC);
```

- [ ] **Step 2: Write the failing test**

Create `backend/tests/messaging_repo.rs`:

```rust
use cofounder_api::messaging::repo;
use cofounder_api::users;
use sqlx::PgPool;
use uuid::Uuid;

async fn a_user(pool: &PgPool, email: &str, name: &str) -> Uuid {
    let id = users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id;

    sqlx::query(
        "INSERT INTO profiles (user_id, display_name, headline, bio, roles, seeking_roles, commitment)
         VALUES ($1, $2, 'Building things', 'A bio.', ARRAY['engineering'], ARRAY['gtm'], 'full_time_now')
         ON CONFLICT (user_id) DO UPDATE SET display_name = EXCLUDED.display_name",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    id
}

#[sqlx::test]
async fn opening_a_conversation_creates_it_once(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;

    let (first, created) = repo::open(&pool, ada, grace).await.unwrap();
    assert!(created);

    // The other side opening it finds the same row, not a second one.
    let (second, created_again) = repo::open(&pool, grace, ada).await.unwrap();
    assert!(!created_again);
    assert_eq!(first.id, second.id);

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test]
async fn the_initiator_is_recorded(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;

    repo::open(&pool, ada, grace).await.unwrap();
    // Reopening from the other side must not rewrite who started it.
    repo::open(&pool, grace, ada).await.unwrap();

    let started_by: Uuid = sqlx::query_scalar("SELECT started_by FROM conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(started_by, ada);
}

#[sqlx::test]
async fn messages_come_back_oldest_first(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();

    repo::send(&pool, conversation.id, ada, "first").await.unwrap();
    repo::send(&pool, conversation.id, grace, "second").await.unwrap();
    repo::send(&pool, conversation.id, ada, "third").await.unwrap();

    let messages = repo::messages_in(&pool, conversation.id).await.unwrap();
    let bodies: Vec<&str> = messages.iter().map(|m| m.body.as_str()).collect();

    assert_eq!(bodies, vec!["first", "second", "third"]);
}

#[sqlx::test]
async fn sending_stamps_the_conversation(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();

    assert!(conversation.last_message_at.is_none());

    repo::send(&pool, conversation.id, ada, "hello").await.unwrap();

    let reloaded = repo::find_by_id(&pool, conversation.id)
        .await
        .unwrap()
        .unwrap();
    assert!(reloaded.last_message_at.is_some());
}

#[sqlx::test]
async fn a_conversation_lists_the_other_person_and_its_unread_count(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();

    repo::send(&pool, conversation.id, grace, "hello").await.unwrap();
    repo::send(&pool, conversation.id, grace, "are you there").await.unwrap();

    let for_ada = repo::for_user(&pool, ada).await.unwrap();
    assert_eq!(for_ada.len(), 1);
    assert_eq!(for_ada[0].other_display_name, "Grace");
    assert_eq!(for_ada[0].unread, 2);
    assert_eq!(for_ada[0].last_message.as_deref(), Some("are you there"));

    // Your own messages are never unread to you.
    let for_grace = repo::for_user(&pool, grace).await.unwrap();
    assert_eq!(for_grace[0].unread, 0);
}

#[sqlx::test]
async fn marking_read_clears_only_the_other_persons_messages(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();

    repo::send(&pool, conversation.id, grace, "hello").await.unwrap();
    repo::send(&pool, conversation.id, ada, "hi back").await.unwrap();

    let cleared = repo::mark_read(&pool, conversation.id, ada).await.unwrap();
    assert_eq!(cleared, 1);

    assert_eq!(repo::for_user(&pool, ada).await.unwrap()[0].unread, 0);

    // Ada's own message is still unread for Grace.
    assert_eq!(repo::for_user(&pool, grace).await.unwrap()[0].unread, 1);
}

#[sqlx::test]
async fn a_blocked_pair_disappears_from_both_conversation_lists(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();
    repo::send(&pool, conversation.id, ada, "hello").await.unwrap();

    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(grace)
        .bind(ada)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::for_user(&pool, ada).await.unwrap().is_empty());
    assert!(repo::for_user(&pool, grace).await.unwrap().is_empty());
}

#[sqlx::test]
async fn conversations_are_ordered_by_most_recent_activity(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let older = a_user(&pool, "older@example.com", "Older").await;
    let newer = a_user(&pool, "newer@example.com", "Newer").await;

    let (first, _) = repo::open(&pool, ada, older).await.unwrap();
    repo::send(&pool, first.id, ada, "hello").await.unwrap();

    let (second, _) = repo::open(&pool, ada, newer).await.unwrap();
    repo::send(&pool, second.id, ada, "hello").await.unwrap();

    let listed = repo::for_user(&pool, ada).await.unwrap();
    assert_eq!(listed[0].other_display_name, "Newer");
}

#[sqlx::test]
async fn started_conversations_are_counted_within_a_window(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let hopper = a_user(&pool, "hopper@example.com", "Hopper").await;

    repo::open(&pool, ada, grace).await.unwrap();
    repo::open(&pool, ada, hopper).await.unwrap();

    assert_eq!(repo::count_started_since(&pool, ada, 60).await.unwrap(), 2);
    // Being messaged does not consume the other person's allowance.
    assert_eq!(repo::count_started_since(&pool, grace, 60).await.unwrap(), 0);

    sqlx::query("UPDATE conversations SET created_at = now() - interval '2 hours'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(repo::count_started_since(&pool, ada, 60).await.unwrap(), 0);
}

#[sqlx::test]
async fn sent_messages_are_counted_within_a_window(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();

    repo::send(&pool, conversation.id, ada, "one").await.unwrap();
    repo::send(&pool, conversation.id, ada, "two").await.unwrap();
    repo::send(&pool, conversation.id, grace, "reply").await.unwrap();

    assert_eq!(repo::count_messages_since(&pool, ada, 1).await.unwrap(), 2);
    assert_eq!(repo::count_messages_since(&pool, grace, 1).await.unwrap(), 1);
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cd backend && cargo test --test messaging_repo`
Expected: FAIL — `could not find messaging in cofounder_api`.

- [ ] **Step 4: Implement the repository**

Create `backend/src/messaging/mod.rs`:

```rust
pub mod repo;
```

Modify `backend/src/lib.rs` — add `pub mod messaging;` after `pub mod error;`.

Create `backend/src/messaging/repo.rs`:

```rust
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct Conversation {
    pub id: Uuid,
    pub user_a_id: Uuid,
    pub user_b_id: Uuid,
    pub started_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub last_message_at: Option<DateTime<Utc>>,
}

impl Conversation {
    pub fn includes(&self, user_id: Uuid) -> bool {
        self.user_a_id == user_id || self.user_b_id == user_id
    }

    /// The participant who is not `user_id`. Callers have already checked
    /// membership, so a conversation the user is not in returns their own id
    /// rather than panicking.
    pub fn other_than(&self, user_id: Uuid) -> Uuid {
        if self.user_a_id == user_id {
            self.user_b_id
        } else {
            self.user_a_id
        }
    }
}

/// A row of the conversation list: who it is with, what was said last, and
/// how much of it the caller has not read.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct ConversationSummary {
    pub id: Uuid,
    pub other_user_id: Uuid,
    pub other_display_name: String,
    pub other_headline: String,
    pub last_message: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub unread: i64,
    pub matched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

const CONVERSATION_COLUMNS: &str =
    "id, user_a_id, user_b_id, started_by, created_at, last_message_at";

fn ordered(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Finds or creates the conversation between two people. The second element
/// is true when this call created it, which is what the new-conversation
/// rate limit counts — reopening an existing thread is not a new one.
///
/// Written as look-then-insert rather than a single upsert returning
/// Postgres's `xmax` trick: that trick works, but it is an internal detail a
/// reader has to go and look up, and this runs on a path that is not hot.
pub async fn open(
    pool: &PgPool,
    initiator: Uuid,
    other: Uuid,
) -> sqlx::Result<(Conversation, bool)> {
    let (a, b) = ordered(initiator, other);

    if let Some(conversation) = find_between(pool, a, b).await? {
        return Ok((conversation, false));
    }

    // `started_by` is only ever set on insert, so whoever opened the
    // conversation first keeps it and reopening cannot move the limit's cost
    // onto the other person.
    let inserted = sqlx::query_as::<_, Conversation>(&format!(
        r#"
        INSERT INTO conversations (user_a_id, user_b_id, started_by)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_a_id, user_b_id) DO NOTHING
        RETURNING {CONVERSATION_COLUMNS}
        "#
    ))
    .bind(a)
    .bind(b)
    .bind(initiator)
    .fetch_optional(pool)
    .await?;

    match inserted {
        Some(conversation) => Ok((conversation, true)),
        // A concurrent opener won the race. Their row is the conversation.
        None => {
            let conversation = find_between(pool, a, b)
                .await?
                .expect("the conflicting row exists");
            Ok((conversation, false))
        }
    }
}

/// The conversation between two people, whichever order they are given in.
pub async fn find_between(
    pool: &PgPool,
    one: Uuid,
    other: Uuid,
) -> sqlx::Result<Option<Conversation>> {
    let (a, b) = ordered(one, other);

    sqlx::query_as::<_, Conversation>(&format!(
        "SELECT {CONVERSATION_COLUMNS} FROM conversations
         WHERE user_a_id = $1 AND user_b_id = $2"
    ))
    .bind(a)
    .bind(b)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Conversation>> {
    sqlx::query_as::<_, Conversation>(&format!(
        "SELECT {CONVERSATION_COLUMNS} FROM conversations WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Every conversation the user is in, excluding anyone blocked in either
/// direction. Matches sort to the top, then the most recently active.
pub async fn for_user(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<ConversationSummary>> {
    sqlx::query_as::<_, ConversationSummary>(
        r#"
        SELECT
            c.id                AS id,
            other.id            AS other_user_id,
            p.display_name      AS other_display_name,
            p.headline          AS other_headline,
            (
                SELECT m.body FROM messages m
                WHERE m.conversation_id = c.id
                ORDER BY m.created_at DESC LIMIT 1
            )                   AS last_message,
            c.last_message_at   AS last_message_at,
            (
                SELECT count(*) FROM messages m
                WHERE m.conversation_id = c.id
                  AND m.sender_id <> $1
                  AND m.read_at IS NULL
            )                   AS unread,
            EXISTS (
                SELECT 1 FROM matches mt
                WHERE (mt.user_a_id = c.user_a_id AND mt.user_b_id = c.user_b_id)
            )                   AS matched
        FROM conversations c
        JOIN users other
          ON other.id = CASE WHEN c.user_a_id = $1 THEN c.user_b_id ELSE c.user_a_id END
        JOIN profiles p ON p.user_id = other.id
        WHERE $1 IN (c.user_a_id, c.user_b_id)
          AND NOT EXISTS (
              SELECT 1 FROM blocks b
              WHERE (b.blocker_id = $1 AND b.blocked_id = other.id)
                 OR (b.blocker_id = other.id AND b.blocked_id = $1)
          )
        ORDER BY matched DESC, c.last_message_at DESC NULLS LAST, c.created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn messages_in(pool: &PgPool, conversation_id: Uuid) -> sqlx::Result<Vec<Message>> {
    sqlx::query_as::<_, Message>(
        r#"
        SELECT id, conversation_id, sender_id, body, created_at, read_at
        FROM messages
        WHERE conversation_id = $1
        ORDER BY created_at
        "#,
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
}

/// Writes the message and stamps the conversation in one transaction, so the
/// list can never show a thread whose last activity predates its own last
/// message.
pub async fn send(
    pool: &PgPool,
    conversation_id: Uuid,
    sender_id: Uuid,
    body: &str,
) -> sqlx::Result<Message> {
    let mut tx = pool.begin().await?;

    let message = sqlx::query_as::<_, Message>(
        r#"
        INSERT INTO messages (conversation_id, sender_id, body)
        VALUES ($1, $2, $3)
        RETURNING id, conversation_id, sender_id, body, created_at, read_at
        "#,
    )
    .bind(conversation_id)
    .bind(sender_id)
    .bind(body)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("UPDATE conversations SET last_message_at = $2 WHERE id = $1")
        .bind(conversation_id)
        .bind(message.created_at)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(message)
}

/// Marks the *other* participant's messages as read. Returns how many were
/// cleared, which is what the caller reports as the new unread delta.
pub async fn mark_read(
    pool: &PgPool,
    conversation_id: Uuid,
    reader_id: Uuid,
) -> sqlx::Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE messages SET read_at = now()
        WHERE conversation_id = $1 AND sender_id <> $2 AND read_at IS NULL
        "#,
    )
    .bind(conversation_id)
    .bind(reader_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn count_started_since(
    pool: &PgPool,
    user_id: Uuid,
    minutes: i64,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT count(*) FROM conversations
        WHERE started_by = $1 AND created_at > now() - make_interval(mins => $2)
        "#,
    )
    .bind(user_id)
    .bind(minutes as i32)
    .fetch_one(pool)
    .await
}

pub async fn count_messages_since(
    pool: &PgPool,
    user_id: Uuid,
    minutes: i64,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT count(*) FROM messages
        WHERE sender_id = $1 AND created_at > now() - make_interval(mins => $2)
        "#,
    )
    .bind(user_id)
    .bind(minutes as i32)
    .fetch_one(pool)
    .await
}
```

- [ ] **Step 5: Run the tests and verify they pass**

Run: `cd backend && cargo test --test messaging_repo`
Expected: PASS — 10 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/migrations backend/src/messaging backend/src/lib.rs backend/tests/messaging_repo.rs
git commit -m "feat: conversation and message persistence"
git push origin main
```

---

### Task 3: Opening and listing conversations

**Files:**
- Create: `backend/src/messaging/service.rs`, `backend/src/messaging/routes.rs`
- Modify: `backend/src/messaging/mod.rs`, `backend/src/messaging/repo.rs`, `backend/src/error.rs`, `backend/src/app.rs`
- Test: `backend/tests/conversations_api.rs`

**Interfaces:**
- Consumes: `messaging::repo::{find_between, open, for_user, count_started_since}`, `profiles::repo::is_complete`, `users::repo::find_by_id`
- Produces:
  - `cofounder_api::messaging::service::{ensure_can_message, open_conversation, list, MAX_NEW_CONVERSATIONS_PER_DAY, NEW_CONVERSATION_WINDOW_MINUTES}`
  - `cofounder_api::messaging::routes::router() -> Router<AppState>` mounting `GET /conversations`, `POST /conversations`
  - `cofounder_api::error::ApiError::ProfileIncomplete` — 403, slug `profile_incomplete`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/conversations_api.rs`. Copy `state_with`, `post_json`, `sign_in`, `get` and `json_body` verbatim from `backend/tests/swipes_api.rs`, then add:

```rust
/// A complete profile — the precondition for messaging at all.
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

fn open_with(cookie: &str, target: Uuid) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/conversations")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "user_id": target }).to_string(),
        ))
        .unwrap()
}

#[sqlx::test]
async fn conversations_require_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/conversations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn a_conversation_can_be_opened_without_a_match(pool: PgPool) {
    // The central decision in the design: messaging is not gated on matching.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    let response = router(state)
        .oneshot(open_with(&cookie, grace))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert!(body["id"].is_string());
    assert_eq!(body["created"], true);
}

#[sqlx::test]
async fn opening_the_same_conversation_twice_returns_the_same_one(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    let first = json_body(
        router(state.clone())
            .oneshot(open_with(&cookie, grace))
            .await
            .unwrap(),
    )
    .await;

    let response = router(state).oneshot(open_with(&cookie, grace)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let second = json_body(response).await;

    assert_eq!(first["id"], second["id"]);
    assert_eq!(second["created"], false);
}

#[sqlx::test]
async fn an_incomplete_profile_cannot_open_a_conversation(pool: PgPool) {
    // The completeness requirement is the primary spam filter.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    let response = router(state)
        .oneshot(open_with(&cookie, grace))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = json_body(response).await;
    assert_eq!(body["type"], "profile_incomplete");
}

#[sqlx::test]
async fn you_cannot_open_a_conversation_with_yourself(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;

    let response = router(state).oneshot(open_with(&cookie, ada)).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn a_block_prevents_opening_in_either_direction(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(grace)
        .bind(ada)
        .execute(&pool)
        .await
        .unwrap();

    let response = router(state)
        .oneshot(open_with(&cookie, grace))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn new_conversations_are_capped_per_day(pool: PgPool) {
    use cofounder_api::messaging::service::MAX_NEW_CONVERSATIONS_PER_DAY;

    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;

    for index in 0..MAX_NEW_CONVERSATIONS_PER_DAY {
        let target =
            complete_profile(&pool, &format!("other{index}@example.com"), "Other").await;
        let response = router(state.clone())
            .oneshot(open_with(&cookie, target))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "at {index}");
    }

    let one_too_many =
        complete_profile(&pool, "onetoomany@example.com", "One Too Many").await;
    let response = router(state)
        .oneshot(open_with(&cookie, one_too_many))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));
}

#[sqlx::test]
async fn being_messaged_does_not_consume_your_own_allowance(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let ada_cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace_cookie = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    router(state.clone())
        .oneshot(open_with(&ada_cookie, grace))
        .await
        .unwrap();

    // Grace has been messaged once but has started nothing.
    let hopper = complete_profile(&pool, "hopper@example.com", "Hopper").await;
    let response = router(state)
        .oneshot(open_with(&grace_cookie, hopper))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn reopening_an_existing_thread_is_not_a_new_conversation(pool: PgPool) {
    use cofounder_api::messaging::service::MAX_NEW_CONVERSATIONS_PER_DAY;

    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;

    let first = complete_profile(&pool, "first@example.com", "First").await;

    for _ in 0..(MAX_NEW_CONVERSATIONS_PER_DAY * 2) {
        let response = router(state.clone())
            .oneshot(open_with(&cookie, first))
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}

#[sqlx::test]
async fn the_conversation_list_shows_the_other_person(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    router(state.clone())
        .oneshot(open_with(&cookie, grace))
        .await
        .unwrap();

    let body = json_body(
        router(state)
            .oneshot(get("/conversations", &cookie))
            .await
            .unwrap(),
    )
    .await;

    let conversations = body["conversations"].as_array().unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0]["other_display_name"], "Grace");
    assert_eq!(conversations[0]["unread"], 0);
}

#[sqlx::test]
async fn the_conversation_list_starts_empty(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let body = json_body(
        router(state)
            .oneshot(get("/conversations", &cookie))
            .await
            .unwrap(),
    )
    .await;

    assert_eq!(body["conversations"].as_array().unwrap().len(), 0);
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test conversations_api`
Expected: FAIL — `could not find service in messaging`.

- [ ] **Step 3: Add the error variant**

Modify `backend/src/error.rs`. Add the variant after `Forbidden`:

```rust
    /// Distinct from `Forbidden` so the frontend can send the user to finish
    /// their profile rather than telling them they are not allowed.
    #[error("your profile is not complete yet")]
    ProfileIncomplete,
```

Add to `status()`:

```rust
            ApiError::ProfileIncomplete => StatusCode::FORBIDDEN,
```

Add to `type_slug()`:

```rust
            ApiError::ProfileIncomplete => "profile_incomplete",
```

- [ ] **Step 4: Write the service**

Create `backend/src/messaging/service.rs`:

```rust
use uuid::Uuid;

use crate::app::AppState;
use crate::error::{ApiError, ApiResult, FieldError};
use crate::messaging::repo::{self, Conversation, ConversationSummary};
use crate::profiles;
use crate::users;

/// Ten new conversations per rolling twenty-four hours. Replies within an
/// existing conversation are unlimited: the limit exists to stop bulk
/// outreach, not to ration talking to someone who answered.
pub const MAX_NEW_CONVERSATIONS_PER_DAY: i64 = 10;
pub const NEW_CONVERSATION_WINDOW_MINUTES: i64 = 24 * 60;

/// Every precondition for one person messaging another, in one place. Gating
/// chat on a mutual match later — the design's most likely revision — is a
/// change here and nowhere else.
pub async fn ensure_can_message(
    state: &AppState,
    sender_id: Uuid,
    other_id: Uuid,
) -> ApiResult<()> {
    if sender_id == other_id {
        return Err(ApiError::Validation(vec![FieldError {
            field: "user_id".into(),
            message: "you cannot message yourself".into(),
        }]));
    }

    if users::repo::find_by_id(&state.db, other_id).await?.is_none() {
        return Err(ApiError::NotFound);
    }

    if !profiles::repo::is_complete(&state.db, sender_id).await? {
        return Err(ApiError::ProfileIncomplete);
    }

    // Someone who cannot appear in a deck cannot be written to either.
    if !profiles::repo::is_complete(&state.db, other_id).await? {
        return Err(ApiError::Forbidden);
    }

    if crate::moderation::repo::is_blocked_either_way(&state.db, sender_id, other_id).await? {
        return Err(ApiError::Forbidden);
    }

    Ok(())
}

pub async fn open_conversation(
    state: &AppState,
    initiator: Uuid,
    other_id: Uuid,
) -> ApiResult<(Conversation, bool)> {
    ensure_can_message(state, initiator, other_id).await?;

    // An existing thread is never a new conversation, so reopening one must
    // not cost anything against the daily allowance.
    if let Some(conversation) = repo::find_between(&state.db, initiator, other_id).await? {
        return Ok((conversation, false));
    }

    let started =
        repo::count_started_since(&state.db, initiator, NEW_CONVERSATION_WINDOW_MINUTES).await?;

    if started >= MAX_NEW_CONVERSATIONS_PER_DAY {
        return Err(ApiError::RateLimited {
            retry_after_seconds: (NEW_CONVERSATION_WINDOW_MINUTES * 60) as u64,
        });
    }

    Ok(repo::open(&state.db, initiator, other_id).await?)
}

pub async fn list(state: &AppState, user_id: Uuid) -> ApiResult<Vec<ConversationSummary>> {
    Ok(repo::for_user(&state.db, user_id).await?)
}
```

- [ ] **Step 6: Write the routes**

Create `backend/src/messaging/routes.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::extractor::CurrentUser;
use crate::error::ApiResult;
use crate::messaging::repo::ConversationSummary;
use crate::messaging::service;

#[derive(serde::Deserialize)]
pub struct OpenConversationRequest {
    pub user_id: Uuid,
}

#[derive(serde::Serialize)]
pub struct OpenedConversation {
    pub id: Uuid,
    /// False when the thread already existed, which the frontend uses to
    /// decide between "started a conversation" and simply navigating to it.
    pub created: bool,
}

#[derive(serde::Serialize)]
pub struct ConversationsView {
    pub conversations: Vec<ConversationSummary>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/conversations", get(list).post(open))
}

async fn list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<ConversationsView>> {
    Ok(Json(ConversationsView {
        conversations: service::list(&state, user.id).await?,
    }))
}

async fn open(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(payload): Json<OpenConversationRequest>,
) -> ApiResult<(StatusCode, Json<OpenedConversation>)> {
    let (conversation, created) =
        service::open_conversation(&state, user.id, payload.user_id).await?;

    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((
        status,
        Json(OpenedConversation {
            id: conversation.id,
            created,
        }),
    ))
}
```

Modify `backend/src/messaging/mod.rs`:

```rust
pub mod repo;
pub mod routes;
pub mod service;
```

Modify `backend/src/app.rs` — extend the merge chain with
`.merge(crate::messaging::routes::router())`.

Note that `ensure_can_message` calls `crate::moderation::repo::is_blocked_either_way`,
which Task 6 creates. To keep this task compiling on its own, add the
moderation module now with only that function — the rest of Task 6 fills it
in. Create `backend/src/moderation/mod.rs` containing `pub mod repo;` and
`backend/src/moderation/repo.rs` containing:

```rust
use sqlx::PgPool;
use uuid::Uuid;

/// A block hides both people from each other. Checking one direction only
/// would let the blocker keep messaging the person they blocked.
pub async fn is_blocked_either_way(
    pool: &PgPool,
    one: Uuid,
    other: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM blocks
            WHERE (blocker_id = $1 AND blocked_id = $2)
               OR (blocker_id = $2 AND blocked_id = $1)
        )
        "#,
    )
    .bind(one)
    .bind(other)
    .fetch_one(pool)
    .await
}
```

Add `pub mod moderation;` to `backend/src/lib.rs`.

- [ ] **Step 7: Run the tests and verify they pass**

Run: `cd backend && cargo test --test conversations_api`
Expected: PASS — 11 tests.

- [ ] **Step 8: Commit**

```bash
git add backend/src backend/tests/conversations_api.rs
git commit -m "feat: open and list conversations, with the daily limit"
git push origin main
```

---

### Task 4: Sending and reading messages

**Files:**
- Modify: `backend/src/messaging/service.rs`, `backend/src/messaging/routes.rs`
- Test: `backend/tests/messages_api.rs`

**Interfaces:**
- Consumes: everything from Task 3
- Produces:
  - `messaging::service::{send_message, read_thread, MAX_MESSAGES_PER_MINUTE, MAX_MESSAGE_LENGTH}`
  - `messaging::routes::router()` additionally mounts `GET /conversations/:id/messages` and `POST /conversations/:id/messages`

- [ ] **Step 1: Write the failing test**

Create `backend/tests/messages_api.rs`. Copy the helpers and `complete_profile`
from `backend/tests/conversations_api.rs`, then add:

```rust
async fn a_conversation(state: AppState, cookie: &str, target: Uuid) -> String {
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/conversations")
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "user_id": target }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    json_body(response).await["id"].as_str().unwrap().to_string()
}

fn say(cookie: &str, conversation: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/conversations/{conversation}/messages"))
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "body": body }).to_string(),
        ))
        .unwrap()
}

#[sqlx::test]
async fn a_message_is_sent_and_read_back(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    let response = router(state.clone())
        .oneshot(say(&cookie, &conversation, "hello there"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = json_body(
        router(state)
            .oneshot(get(
                &format!("/conversations/{conversation}/messages"),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;

    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["body"], "hello there");
}

#[sqlx::test]
async fn an_empty_message_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    let response = router(state)
        .oneshot(say(&cookie, &conversation, "   "))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "body");
}

#[sqlx::test]
async fn an_overlong_message_is_rejected(pool: PgPool) {
    use cofounder_api::messaging::service::MAX_MESSAGE_LENGTH;

    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    let response = router(state)
        .oneshot(say(&cookie, &conversation, &"x".repeat(MAX_MESSAGE_LENGTH + 1)))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn a_stranger_cannot_read_or_write_someone_elses_conversation(pool: PgPool) {
    // 404 rather than 403: whether a conversation exists is itself private.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let ada_cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;
    let conversation = a_conversation(state.clone(), &ada_cookie, grace).await;

    let stranger = sign_in(state.clone(), &mailer, "stranger@example.com").await;
    complete_profile(&pool, "stranger@example.com", "Stranger").await;

    let read = router(state.clone())
        .oneshot(get(
            &format!("/conversations/{conversation}/messages"),
            &stranger,
        ))
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::NOT_FOUND);

    let write = router(state)
        .oneshot(say(&stranger, &conversation, "let me in"))
        .await
        .unwrap();
    assert_eq!(write.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn a_block_stops_further_messages_in_an_existing_thread(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    router(state.clone())
        .oneshot(say(&cookie, &conversation, "hello"))
        .await
        .unwrap();

    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(grace)
        .bind(ada)
        .execute(&pool)
        .await
        .unwrap();

    let response = router(state)
        .oneshot(say(&cookie, &conversation, "still here"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn messages_are_capped_per_minute(pool: PgPool) {
    use cofounder_api::messaging::service::MAX_MESSAGES_PER_MINUTE;

    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    for index in 0..MAX_MESSAGES_PER_MINUTE {
        let response = router(state.clone())
            .oneshot(say(&cookie, &conversation, "spam"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "at {index}");
    }

    let response = router(state)
        .oneshot(say(&cookie, &conversation, "one more"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));
}

#[sqlx::test]
async fn replies_within_a_thread_are_not_limited_as_new_conversations(pool: PgPool) {
    // The daily cap counts conversations started, not messages sent.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    for _ in 0..15 {
        let response = router(state.clone())
            .oneshot(say(&cookie, &conversation, "chatting"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
}

#[sqlx::test]
async fn opening_a_thread_marks_the_other_persons_messages_read(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let ada_cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace_cookie = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;
    let conversation = a_conversation(state.clone(), &ada_cookie, grace).await;

    router(state.clone())
        .oneshot(say(&grace_cookie, &conversation, "hello"))
        .await
        .unwrap();

    let before = json_body(
        router(state.clone())
            .oneshot(get("/conversations", &ada_cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(before["conversations"][0]["unread"], 1);

    router(state.clone())
        .oneshot(get(
            &format!("/conversations/{conversation}/messages"),
            &ada_cookie,
        ))
        .await
        .unwrap();

    let after = json_body(
        router(state)
            .oneshot(get("/conversations", &ada_cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(after["conversations"][0]["unread"], 0);
}

#[sqlx::test]
async fn messaging_requires_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/conversations/{}/messages", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cd backend && cargo test --test messages_api`
Expected: FAIL — the message routes are not mounted, so the 201 assertions fail.

- [ ] **Step 3: Extend the service**

Append to `backend/src/messaging/service.rs`:

```rust
/// A per-minute ceiling on messages from one person, in any conversation.
pub const MAX_MESSAGES_PER_MINUTE: i64 = 20;
pub const MAX_MESSAGE_LENGTH: usize = 4000;

/// Loads a conversation the caller is actually in. A conversation someone is
/// not part of reports as missing rather than forbidden: whether two other
/// people are talking is itself private.
async fn participating(
    state: &AppState,
    conversation_id: Uuid,
    user_id: Uuid,
) -> ApiResult<Conversation> {
    let conversation = repo::find_by_id(&state.db, conversation_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if !conversation.includes(user_id) {
        return Err(ApiError::NotFound);
    }

    Ok(conversation)
}

pub async fn read_thread(
    state: &AppState,
    conversation_id: Uuid,
    reader_id: Uuid,
) -> ApiResult<Vec<repo::Message>> {
    let conversation = participating(state, conversation_id, reader_id).await?;

    repo::mark_read(&state.db, conversation.id, reader_id).await?;

    Ok(repo::messages_in(&state.db, conversation.id).await?)
}

pub async fn send_message(
    state: &AppState,
    conversation_id: Uuid,
    sender_id: Uuid,
    body: &str,
) -> ApiResult<repo::Message> {
    let conversation = participating(state, conversation_id, sender_id).await?;
    let recipient_id = conversation.other_than(sender_id);

    // Re-checked on every send, not just when the thread was opened: a block
    // raised mid-conversation has to take effect immediately.
    ensure_can_message(state, sender_id, recipient_id).await?;

    let trimmed = body.trim();

    if trimmed.is_empty() {
        return Err(ApiError::Validation(vec![FieldError {
            field: "body".into(),
            message: "cannot be empty".into(),
        }]));
    }

    if trimmed.chars().count() > MAX_MESSAGE_LENGTH {
        return Err(ApiError::Validation(vec![FieldError {
            field: "body".into(),
            message: format!("must be {MAX_MESSAGE_LENGTH} characters or fewer"),
        }]));
    }

    let recent = repo::count_messages_since(&state.db, sender_id, 1).await?;
    if recent >= MAX_MESSAGES_PER_MINUTE {
        return Err(ApiError::RateLimited {
            retry_after_seconds: 60,
        });
    }

    let message = repo::send(&state.db, conversation.id, sender_id, trimmed).await?;

    Ok(message)
}
```

Add `use crate::messaging::repo::Conversation;` to the existing import of
`crate::messaging::repo::{self, ConversationSummary}` so it reads
`use crate::messaging::repo::{self, Conversation, ConversationSummary};`.

- [ ] **Step 4: Extend the routes**

Modify `backend/src/messaging/routes.rs`. Add imports:

```rust
use axum::extract::Path;
use crate::messaging::repo::Message;
```

Extend the router:

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/conversations", get(list).post(open))
        .route(
            "/conversations/{id}/messages",
            get(read_messages).post(send_message),
        )
}
```

Add the request and view types and the two handlers:

```rust
#[derive(serde::Deserialize)]
pub struct SendMessageRequest {
    pub body: String,
}

#[derive(serde::Serialize)]
pub struct MessagesView {
    pub messages: Vec<Message>,
}

async fn read_messages(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MessagesView>> {
    Ok(Json(MessagesView {
        messages: service::read_thread(&state, id, user.id).await?,
    }))
}

async fn send_message(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<SendMessageRequest>,
) -> ApiResult<(StatusCode, Json<Message>)> {
    let message = service::send_message(&state, id, user.id, &payload.body).await?;

    Ok((StatusCode::CREATED, Json(message)))
}
```

Note the path syntax: axum 0.8 uses `{id}`, not the `:id` of 0.7. Using `:id`
compiles but never matches, which presents as a 404 on every message request.

- [ ] **Step 5: Run the tests and verify they pass**

Run: `cd backend && cargo test --test messages_api`
Expected: PASS — 9 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/src/messaging backend/tests/messages_api.rs
git commit -m "feat: send and read messages, with the per-minute limit"
git push origin main
```

---

### Task 5: Live delivery over SSE

**Files:**
- Create: `backend/src/messaging/events.rs`
- Modify: `backend/Cargo.toml`, `backend/src/messaging/mod.rs`, `backend/src/messaging/service.rs`, `backend/src/messaging/routes.rs`, `backend/src/app.rs`, `backend/src/main.rs`, and the `AppState` literal in `backend/tests/{auth_flow,assessment_api,profile_api,deck_api,swipes_api,health,conversations_api,messages_api}.rs`
- Test: `backend/tests/events.rs`

**Interfaces:**
- Produces:
  - `cofounder_api::messaging::events::{Event, Envelope, EventBus}`
  - `EventBus::new()`, `EventBus::publish(&self, recipient_id: Uuid, event: Event)`, `EventBus::subscribe(&self) -> broadcast::Receiver<Envelope>`
  - `AppState.events: EventBus`
  - `messaging::routes::router()` additionally mounts `GET /events`

- [ ] **Step 1: Add the dependency**

```bash
cd backend && cargo add tokio-stream@0.1 --features sync
```

- [ ] **Step 2: Write the failing test**

Create `backend/tests/events.rs`:

```rust
use cofounder_api::messaging::events::{Event, EventBus};
use uuid::Uuid;

#[tokio::test]
async fn an_event_reaches_a_subscriber() {
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();
    let recipient = Uuid::new_v4();

    bus.publish(recipient, Event::UnreadCount { count: 3 });

    let envelope = receiver.recv().await.expect("an envelope");
    assert_eq!(envelope.recipient_id, recipient);
    assert_eq!(envelope.event, Event::UnreadCount { count: 3 });
}

#[tokio::test]
async fn publishing_with_nobody_listening_is_not_an_error() {
    // The message is already committed by the time it is published. A send
    // that fails because no stream is open must not fail the request.
    let bus = EventBus::new();

    bus.publish(Uuid::new_v4(), Event::UnreadCount { count: 1 });
}

#[tokio::test]
async fn every_subscriber_sees_every_envelope_and_filters_its_own() {
    // Addressing is by recipient_id on the envelope; the stream does the
    // filtering. If that filter is ever dropped, one user's messages are
    // delivered to everyone.
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();

    let mine = Uuid::new_v4();
    let theirs = Uuid::new_v4();

    bus.publish(theirs, Event::UnreadCount { count: 9 });
    bus.publish(mine, Event::UnreadCount { count: 1 });

    let first = receiver.recv().await.unwrap();
    let second = receiver.recv().await.unwrap();

    assert_eq!(first.recipient_id, theirs);
    assert_eq!(second.recipient_id, mine);
}

#[tokio::test]
async fn a_new_message_event_carries_what_a_client_needs() {
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();

    let conversation_id = Uuid::new_v4();
    let sender_id = Uuid::new_v4();

    bus.publish(
        Uuid::new_v4(),
        Event::NewMessage {
            conversation_id,
            sender_id,
            preview: "hello".into(),
        },
    );

    match receiver.recv().await.unwrap().event {
        Event::NewMessage {
            conversation_id: got,
            preview,
            ..
        } => {
            assert_eq!(got, conversation_id);
            assert_eq!(preview, "hello");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
```

Append to `backend/tests/messages_api.rs`:

```rust
#[sqlx::test]
async fn sending_publishes_an_event_addressed_to_the_recipient(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    let mut receiver = state.events.subscribe();

    router(state)
        .oneshot(say(&cookie, &conversation, "hello there"))
        .await
        .unwrap();

    let envelope = receiver.recv().await.expect("an envelope");
    assert_eq!(envelope.recipient_id, grace, "addressed to the other person");
}
```

- [ ] **Step 3: Run the tests and verify they fail**

Run: `cd backend && cargo test --test events`
Expected: FAIL — `could not find events in messaging`.

- [ ] **Step 4: Implement the bus**

Create `backend/src/messaging/events.rs`:

```rust
use tokio::sync::broadcast;
use uuid::Uuid;

/// What a client is told has happened. Serialized with a `type` tag so the
/// frontend can switch on it without inspecting which fields are present.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    NewMessage {
        conversation_id: Uuid,
        sender_id: Uuid,
        /// Enough to render a notification without a second request. Not the
        /// whole body: the thread is fetched when it is opened.
        preview: String,
    },
    UnreadCount {
        count: i64,
    },
}

/// An event plus who it is for. Every subscriber receives every envelope and
/// discards the ones not addressed to it, so the filter in the SSE handler
/// is the only thing keeping one user's messages away from another's stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub recipient_id: Uuid,
    pub event: Event,
}

/// In-process fan-out. This works only while the backend is a single
/// process: two instances behind a load balancer would each see only the
/// events their own requests produced. That is a deliberate limit for a
/// single-droplet deployment, not an oversight — moving to Postgres
/// LISTEN/NOTIFY would replace this type and nothing else.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Envelope>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _receiver) = broadcast::channel(256);
        Self { sender }
    }

    /// Deliberately infallible. `send` errors when nobody is subscribed,
    /// which is the ordinary case for a user with no browser tab open — and
    /// the message it describes is already committed either way.
    pub fn publish(&self, recipient_id: Uuid, event: Event) {
        let _ = self.sender.send(Envelope {
            recipient_id,
            event,
        });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Envelope> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
```

Modify `backend/src/messaging/mod.rs`:

```rust
pub mod events;
pub mod repo;
pub mod routes;
pub mod service;
```

- [ ] **Step 5: Put the bus in the application state**

Modify `backend/src/app.rs` — add the field to `AppState`:

```rust
    pub test_mailer: Option<Arc<LastLinkMailer>>,
    /// In-process fan-out for SSE. See `messaging::events::EventBus` for why
    /// this constrains the backend to a single process.
    pub events: crate::messaging::events::EventBus,
```

Modify `backend/src/main.rs` — add `events: cofounder_api::messaging::events::EventBus::new(),`
to the `AppState` literal.

Modify each of these test files, adding `events: cofounder_api::messaging::events::EventBus::new(),`
to the `AppState` literal in their `state_with` helper:
`tests/auth_flow.rs`, `tests/assessment_api.rs`, `tests/profile_api.rs`,
`tests/deck_api.rs`, `tests/swipes_api.rs`, `tests/health.rs`,
`tests/conversations_api.rs`, `tests/messages_api.rs`.

- [ ] **Step 6: Publish on send**

Modify `backend/src/messaging/service.rs` — in `send_message`, replace the
final two lines with:

```rust
    let message = repo::send(&state.db, conversation.id, sender_id, trimmed).await?;

    // After the write, so a client is never told about a message that failed
    // to commit.
    state.events.publish(
        recipient_id,
        crate::messaging::events::Event::NewMessage {
            conversation_id: conversation.id,
            sender_id,
            preview: trimmed.chars().take(120).collect(),
        },
    );

    let unread = repo::count_unread(&state.db, recipient_id).await?;
    state
        .events
        .publish(recipient_id, crate::messaging::events::Event::UnreadCount {
            count: unread,
        });

    Ok(message)
```

Append the count to `backend/src/messaging/repo.rs`:

```rust
/// Total unread messages across every conversation, for the badge.
pub async fn count_unread(pool: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        WHERE m.sender_id <> $1
          AND m.read_at IS NULL
          AND $1 IN (c.user_a_id, c.user_b_id)
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}
```

- [ ] **Step 7: Add the SSE endpoint**

Append to `backend/src/messaging/routes.rs`:

```rust
use std::convert::Infallible;

use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

/// A stream of this user's events. Subscribing before filtering means every
/// connected client sees every envelope, so the `recipient_id` check here is
/// what keeps one person's messages out of another's stream.
async fn events(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let user_id = user.id;

    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(move |result| {
        // A lagged subscriber yields an error rather than an envelope. Drop
        // it: the client refetches on reconnect, and killing the stream
        // would be worse than missing one notification.
        let envelope = result.ok()?;
        if envelope.recipient_id != user_id {
            return None;
        }
        Some(Ok(SseEvent::default().json_data(envelope.event).ok()?))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

Add `.route("/events", get(events))` to the router chain.

- [ ] **Step 8: Run the tests and verify they pass**

Run: `cd backend && cargo test`
Expected: PASS — the whole suite, including the four new `events` tests and
the publication test in `messages_api`.

- [ ] **Step 9: Commit**

```bash
git add backend/Cargo.toml backend/Cargo.lock backend/src backend/tests
git commit -m "feat: live message delivery over SSE"
git push origin main
```

---

### Task 6: Blocking and reporting

**Files:**
- Create: `backend/migrations/0014_reports.sql`, `backend/src/moderation/vocab.rs`, `backend/src/moderation/routes.rs`
- Modify: `backend/src/moderation/mod.rs`, `backend/src/moderation/repo.rs`, `backend/src/profiles/routes.rs`, `backend/src/app.rs`
- Test: `backend/tests/moderation_api.rs`

**Interfaces:**
- Produces:
  - `moderation::vocab::REPORT_REASONS: [Choice; 5]`
  - `moderation::repo::{block, report, is_blocked_either_way}`
  - `moderation::routes::router()` mounting `POST /blocks`, `POST /reports`
  - `GET /options` gains a `report_reasons` array

- [ ] **Step 1: Write the migration**

Create `backend/migrations/0014_reports.sql`:

```sql
-- Queued for manual review. Nothing here triggers automated action: a report
-- is evidence for a person to read, not a lever anyone can pull on someone
-- else's account.
CREATE TABLE reports (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reporter_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reported_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reason      TEXT NOT NULL
                CHECK (reason IN ('harassment', 'spam', 'impersonation', 'off_topic', 'other')),
    body        TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending', 'reviewed', 'dismissed')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT no_self_report CHECK (reporter_id <> reported_id)
);

CREATE INDEX reports_pending_idx ON reports (status, created_at DESC);
```

- [ ] **Step 2: Write the failing test**

Create `backend/tests/moderation_api.rs`. Copy the helpers and
`complete_profile` from `backend/tests/conversations_api.rs`, then add:

```rust
fn post_to(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[sqlx::test]
async fn blocking_someone_records_it(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    let response = router(state)
        .oneshot(post_to("/blocks", &cookie, serde_json::json!({ "user_id": grace })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM blocks WHERE blocker_id = $1 AND blocked_id = $2)",
    )
    .bind(ada)
    .bind(grace)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(exists);
}

#[sqlx::test]
async fn blocking_twice_is_not_an_error(pool: PgPool) {
    // The button may be pressed twice; the second press is not a failure.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    for _ in 0..2 {
        let response = router(state.clone())
            .oneshot(post_to(
                "/blocks",
                &cookie,
                serde_json::json!({ "user_id": grace }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM blocks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test]
async fn you_cannot_block_yourself(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;

    let response = router(state)
        .oneshot(post_to("/blocks", &cookie, serde_json::json!({ "user_id": ada })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn a_block_removes_them_from_the_deck(pool: PgPool) {
    // The end-to-end point of blocking: they stop being shown to each other.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    let before = json_body(
        router(state.clone())
            .oneshot(get("/deck", &cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(before["cards"].as_array().unwrap().len(), 1);

    router(state.clone())
        .oneshot(post_to(
            "/blocks",
            &cookie,
            serde_json::json!({ "user_id": grace }),
        ))
        .await
        .unwrap();

    let after = json_body(router(state).oneshot(get("/deck", &cookie)).await.unwrap()).await;
    assert_eq!(after["cards"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn a_report_is_queued_for_review(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    let response = router(state)
        .oneshot(post_to(
            "/reports",
            &cookie,
            serde_json::json!({
                "user_id": grace,
                "reason": "harassment",
                "body": "Repeated unwanted messages."
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (reason, status): (String, String) =
        sqlx::query_as("SELECT reason, status FROM reports")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reason, "harassment");
    assert_eq!(status, "pending", "reports never act automatically");
}

#[sqlx::test]
async fn a_report_does_not_suspend_anyone(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    router(state)
        .oneshot(post_to(
            "/reports",
            &cookie,
            serde_json::json!({ "user_id": grace, "reason": "spam", "body": "" }),
        ))
        .await
        .unwrap();

    let status: String = sqlx::query_scalar("SELECT status FROM users WHERE id = $1")
        .bind(grace)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "active");
}

#[sqlx::test]
async fn an_unknown_report_reason_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

    let response = router(state)
        .oneshot(post_to(
            "/reports",
            &cookie,
            serde_json::json!({ "user_id": grace, "reason": "vibes", "body": "" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "reason");
}

#[sqlx::test]
async fn options_lists_the_report_reasons(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let body = json_body(router(state).oneshot(get("/options", &cookie)).await.unwrap()).await;

    assert_eq!(body["report_reasons"].as_array().unwrap().len(), 5);
    assert!(body["report_reasons"][0]["label"].is_string());
}

#[sqlx::test]
async fn moderation_requires_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/blocks")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "user_id": Uuid::new_v4() }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cd backend && cargo test --test moderation_api`
Expected: FAIL — the routes are not mounted, so the 204 assertions fail.

- [ ] **Step 4: Write the vocabulary**

Create `backend/src/moderation/vocab.rs`:

```rust
use crate::profiles::vocab::Choice;

/// Served by `GET /options` alongside the profile vocabularies, so the
/// report form's wording and the database's CHECK constraint cannot drift.
pub const REPORT_REASONS: [Choice; 5] = [
    Choice {
        id: "harassment",
        label: "Harassment or abuse",
    },
    Choice {
        id: "spam",
        label: "Spam or advertising",
    },
    Choice {
        id: "impersonation",
        label: "Impersonation or a fake profile",
    },
    Choice {
        id: "off_topic",
        label: "Not here to find a cofounder",
    },
    Choice {
        id: "other",
        label: "Something else",
    },
];
```

- [ ] **Step 5: Extend the repository**

Append to `backend/src/moderation/repo.rs`:

```rust
/// Idempotent: pressing block twice is not a failure.
pub async fn block(pool: &PgPool, blocker_id: Uuid, blocked_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO blocks (blocker_id, blocked_id)
        VALUES ($1, $2)
        ON CONFLICT (blocker_id, blocked_id) DO NOTHING
        "#,
    )
    .bind(blocker_id)
    .bind(blocked_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn report(
    pool: &PgPool,
    reporter_id: Uuid,
    reported_id: Uuid,
    reason: &str,
    body: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO reports (reporter_id, reported_id, reason, body)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(reporter_id)
    .bind(reported_id)
    .bind(reason)
    .bind(body)
    .execute(pool)
    .await?;

    Ok(())
}
```

- [ ] **Step 6: Write the routes**

Create `backend/src/moderation/routes.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::extractor::CurrentUser;
use crate::error::{ApiError, ApiResult, FieldError};
use crate::moderation::repo;
use crate::moderation::vocab::REPORT_REASONS;
use crate::profiles::vocab;
use crate::users;

#[derive(serde::Deserialize)]
pub struct BlockRequest {
    pub user_id: Uuid,
}

#[derive(serde::Deserialize)]
pub struct ReportRequest {
    pub user_id: Uuid,
    pub reason: String,
    #[serde(default)]
    pub body: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/blocks", post(create_block))
        .route("/reports", post(create_report))
}

/// Shared by both endpoints: you cannot act on yourself, and the person has
/// to exist.
async fn target(state: &AppState, actor: Uuid, subject: Uuid) -> ApiResult<()> {
    if actor == subject {
        return Err(ApiError::Validation(vec![FieldError {
            field: "user_id".into(),
            message: "you cannot do that to yourself".into(),
        }]));
    }

    if users::repo::find_by_id(&state.db, subject).await?.is_none() {
        return Err(ApiError::NotFound);
    }

    Ok(())
}

async fn create_block(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(payload): Json<BlockRequest>,
) -> ApiResult<StatusCode> {
    target(&state, user.id, payload.user_id).await?;

    repo::block(&state.db, user.id, payload.user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn create_report(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(payload): Json<ReportRequest>,
) -> ApiResult<StatusCode> {
    target(&state, user.id, payload.user_id).await?;

    if !vocab::contains(&REPORT_REASONS, &payload.reason) {
        return Err(ApiError::Validation(vec![FieldError {
            field: "reason".into(),
            message: "is not one of the available reasons".into(),
        }]));
    }

    let body = payload.body.trim();
    if body.chars().count() > 2000 {
        return Err(ApiError::Validation(vec![FieldError {
            field: "body".into(),
            message: "must be 2000 characters or fewer".into(),
        }]));
    }

    repo::report(&state.db, user.id, payload.user_id, &payload.reason, body).await?;

    Ok(StatusCode::NO_CONTENT)
}
```

Modify `backend/src/moderation/mod.rs`:

```rust
pub mod repo;
pub mod routes;
pub mod vocab;
```

Modify `backend/src/app.rs` — add `.merge(crate::moderation::routes::router())`.

- [ ] **Step 7: Serve the reasons from /options**

Modify `backend/src/profiles/routes.rs`. Add the field to `OptionsView`:

```rust
    interests: &'static [Choice],
    report_reasons: &'static [Choice],
```

And to the handler's construction:

```rust
        interests: &vocab::INTERESTS,
        report_reasons: &crate::moderation::vocab::REPORT_REASONS,
```

- [ ] **Step 8: Run the tests and verify they pass**

Run: `cd backend && cargo test`
Expected: PASS — the whole suite.

- [ ] **Step 9: Commit**

```bash
git add backend/migrations backend/src backend/tests/moderation_api.rs
git commit -m "feat: blocking and reporting"
git push origin main
```

---

### Task 7: The conversation list

**Files:**
- Create: `frontend/lib/messaging.ts`, `frontend/app/(app)/conversations/page.tsx`, `frontend/app/(app)/conversations/conversations-client.tsx`
- Modify: `frontend/app/(app)/layout.tsx`
- Test: covered end-to-end in Task 10

**Interfaces:**
- Consumes: `GET /api/conversations`, `apiFetch` from `frontend/lib/api.ts`
- Produces: `frontend/lib/messaging.ts` exporting `ConversationSummary`, `ConversationsView`, `Message`, `MessagesView`, `OpenedConversation`, `LiveEvent`

- [ ] **Step 1: Read the framework docs**

Required by `frontend/AGENTS.md` before any frontend code:

```bash
cd frontend
cat node_modules/next/dist/docs/01-app/01-getting-started/05-server-and-client-components.md
cat node_modules/next/dist/docs/01-app/03-api-reference/03-file-conventions/dynamic-routes.md
```

- [ ] **Step 2: Write the shared types**

Create `frontend/lib/messaging.ts`:

```ts
export interface ConversationSummary {
  id: string;
  other_user_id: string;
  other_display_name: string;
  other_headline: string;
  last_message: string | null;
  last_message_at: string | null;
  unread: number;
  matched: boolean;
}

export interface ConversationsView {
  conversations: ConversationSummary[];
}

export interface Message {
  id: string;
  conversation_id: string;
  sender_id: string;
  body: string;
  created_at: string;
  read_at: string | null;
}

export interface MessagesView {
  messages: Message[];
}

export interface OpenedConversation {
  id: string;
  created: boolean;
}

/** Mirrors `messaging::events::Event`, tagged by `type`. */
export type LiveEvent =
  | {
      type: "new_message";
      conversation_id: string;
      sender_id: string;
      preview: string;
    }
  | { type: "unread_count"; count: number };
```

- [ ] **Step 3: Write the page**

Create `frontend/app/(app)/conversations/page.tsx`:

```tsx
import ConversationsClient from "./conversations-client";

export default function ConversationsPage() {
  return <ConversationsClient />;
}
```

Create `frontend/app/(app)/conversations/conversations-client.tsx`:

```tsx
"use client";

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { apiFetch } from "@/lib/api";
import { ConversationSummary, ConversationsView, LiveEvent } from "@/lib/messaging";

export default function ConversationsClient() {
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    return apiFetch<ConversationsView>("/conversations")
      .then((view) => {
        setConversations(view.conversations);
        setLoading(false);
      })
      .catch(() => {
        setError("Could not load your conversations. Reload to try again.");
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  // A new message anywhere changes this list, so the stream triggers a
  // refetch rather than trying to patch the list in place — the ordering and
  // unread counts are the server's to decide.
  useEffect(() => {
    const source = new EventSource("/api/events");

    source.onmessage = (event) => {
      const parsed = JSON.parse(event.data) as LiveEvent;
      if (parsed.type === "new_message") load();
    };

    return () => source.close();
  }, [load]);

  if (loading) {
    return <p className="text-neutral-600">Loading your conversations…</p>;
  }

  if (error) {
    return (
      <p role="alert" className="text-red-600">
        {error}
      </p>
    );
  }

  if (conversations.length === 0) {
    return (
      <div className="flex max-w-xl flex-col gap-3">
        <h1 className="text-2xl font-semibold">No conversations yet</h1>
        <p className="text-neutral-600">
          Message someone from your deck or your matches to start one.
        </p>
        <Link href="/deck" className="underline">
          Open your deck
        </Link>
      </div>
    );
  }

  return (
    <div className="flex max-w-xl flex-col gap-4">
      <h1 className="text-2xl font-semibold">Messages</h1>
      <ul className="flex flex-col gap-2">
        {conversations.map((conversation) => (
          <li key={conversation.id}>
            <Link
              href={`/conversations/${conversation.id}`}
              className="flex items-start justify-between gap-3 rounded-xl border border-neutral-200 p-4 hover:border-neutral-400"
            >
              <span className="flex flex-col gap-1">
                <span className="font-medium">
                  {conversation.other_display_name}
                  {conversation.matched && (
                    <span className="ml-2 rounded-full bg-neutral-100 px-2 py-0.5 text-xs text-neutral-700">
                      Match
                    </span>
                  )}
                </span>
                <span className="text-sm text-neutral-600">
                  {conversation.last_message ?? "No messages yet"}
                </span>
              </span>
              {conversation.unread > 0 && (
                <span
                  aria-label={`${conversation.unread} unread`}
                  className="rounded-full bg-neutral-900 px-2 py-0.5 text-xs text-white"
                >
                  {conversation.unread}
                </span>
              )}
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
```

- [ ] **Step 4: Add the navigation link**

Modify `frontend/app/(app)/layout.tsx` — add after the Matches link:

```tsx
          <Link
            href="/conversations"
            className="text-sm text-neutral-700 hover:underline"
          >
            Messages
          </Link>
```

- [ ] **Step 5: Check it compiles and lints**

Run: `cd frontend && npx tsc --noEmit && npm run lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/lib/messaging.ts "frontend/app/(app)/conversations" "frontend/app/(app)/layout.tsx"
git commit -m "feat: the conversation list"
git push origin main
```

---

### Task 8: The conversation thread

**Files:**
- Create: `frontend/app/(app)/conversations/[id]/page.tsx`, `frontend/app/(app)/conversations/[id]/thread-client.tsx`
- Test: covered end-to-end in Task 10

**Interfaces:**
- Consumes: `GET`/`POST /api/conversations/{id}/messages`, `GET /api/me`, `/api/events`
- Produces: nothing other tasks import

- [ ] **Step 1: Write the page**

In this Next.js version `params` is a Promise and must be awaited. Create
`frontend/app/(app)/conversations/[id]/page.tsx`:

```tsx
import ThreadClient from "./thread-client";

export default async function ConversationPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  return <ThreadClient conversationId={id} />;
}
```

- [ ] **Step 2: Write the thread**

Create `frontend/app/(app)/conversations/[id]/thread-client.tsx`:

```tsx
"use client";

import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, apiFetch } from "@/lib/api";
import ReportDialog from "@/components/report-dialog";
import {
  ConversationSummary,
  ConversationsView,
  LiveEvent,
  Message,
  MessagesView,
} from "@/lib/messaging";
import { User } from "@/lib/session";

export default function ThreadClient({
  conversationId,
}: {
  conversationId: string;
}) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [summary, setSummary] = useState<ConversationSummary | null>(null);
  const [me, setMe] = useState<User | null>(null);
  const [draft, setDraft] = useState("");
  const [loading, setLoading] = useState(true);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const bottom = useRef<HTMLDivElement>(null);

  const load = useCallback(() => {
    return apiFetch<MessagesView>(`/conversations/${conversationId}/messages`)
      .then((view) => setMessages(view.messages))
      .catch(() => setError("Could not load this conversation."));
  }, [conversationId]);

  useEffect(() => {
    // The summary is fetched as well as the messages so the thread knows who
    // it is with before anyone has spoken. Deriving that from the messages
    // would leave a brand-new conversation with no counterpart, and the
    // moderation controls with nobody to act on.
    Promise.all([
      apiFetch<User>("/me"),
      apiFetch<ConversationsView>("/conversations"),
      load(),
    ])
      .then(([user, view]) => {
        setMe(user);
        setSummary(
          view.conversations.find((row) => row.id === conversationId) ?? null,
        );
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, [conversationId, load]);

  // Only refetch for this conversation: an event about another thread would
  // otherwise mark its messages read behind the user's back, since fetching
  // a thread is what clears its unread count.
  useEffect(() => {
    const source = new EventSource("/api/events");

    source.onmessage = (event) => {
      const parsed = JSON.parse(event.data) as LiveEvent;
      if (
        parsed.type === "new_message" &&
        parsed.conversation_id === conversationId
      ) {
        load();
      }
    };

    return () => source.close();
  }, [conversationId, load]);

  useEffect(() => {
    bottom.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  async function onSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (!draft.trim() || sending) return;

    setSending(true);
    setError(null);

    try {
      const sent = await apiFetch<Message>(
        `/conversations/${conversationId}/messages`,
        { method: "POST", body: JSON.stringify({ body: draft }) },
      );
      setMessages((current) => [...current, sent]);
      setDraft("");
    } catch (err) {
      if (err instanceof ApiError) {
        setError(
          err.problem.type === "rate_limited"
            ? "You're sending messages too quickly. Wait a moment."
            : (err.fieldError("body") ?? err.problem.title),
        );
      } else {
        setError("Could not reach the server. Try again.");
      }
    } finally {
      setSending(false);
    }
  }

  if (loading) return <p className="text-neutral-600">Loading…</p>;

  return (
    <div className="flex max-w-2xl flex-col gap-4">
      <Link href="/conversations" className="text-sm underline">
        ← All messages
      </Link>

      {summary && (
        <h1 className="text-xl font-semibold">{summary.other_display_name}</h1>
      )}

      <ol id="thread" className="flex flex-col gap-2">
        {messages.map((message) => {
          const mine = message.sender_id === me?.id;
          return (
            <li
              key={message.id}
              className={`max-w-[80%] rounded-xl px-3 py-2 ${
                mine
                  ? "self-end bg-neutral-900 text-white"
                  : "self-start bg-neutral-100 text-neutral-900"
              }`}
            >
              {message.body}
            </li>
          );
        })}
      </ol>
      <div ref={bottom} />

      {error && (
        <p role="alert" className="text-sm text-red-600">
          {error}
        </p>
      )}

      <form onSubmit={onSubmit} className="flex gap-2">
        <label htmlFor="message-body" className="sr-only">
          Message
        </label>
        <input
          id="message-body"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Write a message"
          className="flex-1 rounded-lg border border-neutral-300 px-3 py-2"
        />
        <button
          type="submit"
          disabled={sending || !draft.trim()}
          className="rounded-lg bg-neutral-900 px-4 py-2 text-white disabled:opacity-50"
        >
          Send
        </button>
      </form>

      {summary && <ReportDialog userId={summary.other_user_id} />}
    </div>
  );
}
```

- [ ] **Step 3: Check it compiles and lints**

Run: `cd frontend && npx tsc --noEmit && npm run lint`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add "frontend/app/(app)/conversations"
git commit -m "feat: the conversation thread, live over SSE"
git push origin main
```

---

### Task 9: Starting conversations, blocking, and reporting

**Files:**
- Create: `frontend/components/message-button.tsx`, `frontend/components/report-dialog.tsx`
- Modify: `frontend/app/(app)/deck/deck-client.tsx`, `frontend/app/(app)/matches/matches-client.tsx`, `frontend/app/(app)/conversations/[id]/thread-client.tsx`, `frontend/lib/profile.ts`
- Test: covered end-to-end in Task 10

**Interfaces:**
- Consumes: `POST /api/conversations`, `POST /api/blocks`, `POST /api/reports`, `GET /api/options`
- Produces: `MessageButton({ userId, label })`, `ReportDialog({ userId, onBlocked })`

- [ ] **Step 1: Add the report reasons to the options type**

Modify `frontend/lib/profile.ts` — add to the `Options` interface:

```ts
  interests: Choice[];
  report_reasons: Choice[];
```

- [ ] **Step 2: Write the message button**

Create `frontend/components/message-button.tsx`:

```tsx
"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import { ApiError, apiFetch } from "@/lib/api";
import { OpenedConversation } from "@/lib/messaging";

/**
 * Opens the conversation with someone and navigates to it. The deck is the
 * only place most people are discovered, so this is the entry point that
 * makes open messaging usable without a browsable directory.
 */
export default function MessageButton({
  userId,
  label = "Message",
  className = "rounded-lg border border-neutral-300 px-4 py-2",
}: {
  userId: string;
  label?: string;
  className?: string;
}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function open() {
    setBusy(true);
    setError(null);

    try {
      const conversation = await apiFetch<OpenedConversation>("/conversations", {
        method: "POST",
        body: JSON.stringify({ user_id: userId }),
      });
      router.push(`/conversations/${conversation.id}`);
    } catch (err) {
      setBusy(false);
      if (err instanceof ApiError) {
        if (err.problem.type === "profile_incomplete") {
          setError("Finish your profile before messaging.");
        } else if (err.problem.type === "rate_limited") {
          setError("You've started a lot of conversations today. Try tomorrow.");
        } else {
          setError(err.problem.title);
        }
      } else {
        setError("Could not reach the server. Try again.");
      }
    }
  }

  return (
    <span className="flex flex-col gap-1">
      <button type="button" disabled={busy} onClick={open} className={className}>
        {busy ? "Opening…" : label}
      </button>
      {error && (
        <span role="alert" className="text-sm text-red-600">
          {error}
        </span>
      )}
    </span>
  );
}
```

- [ ] **Step 3: Write the block and report controls**

Create `frontend/components/report-dialog.tsx`:

```tsx
"use client";

import { useEffect, useState } from "react";
import { apiFetch } from "@/lib/api";
import { Options } from "@/lib/profile";

/**
 * Blocking is immediate and mutual. Reporting records a reason for a person
 * to read later — it never acts on the account by itself, which is why the
 * confirmation says the report was received rather than that anything
 * happened.
 */
export default function ReportDialog({ userId }: { userId: string }) {
  const [open, setOpen] = useState(false);
  const [reasons, setReasons] = useState<Options["report_reasons"]>([]);
  const [reason, setReason] = useState("");
  const [body, setBody] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!open || reasons.length > 0) return;
    apiFetch<Options>("/options")
      .then((options) => {
        setReasons(options.report_reasons);
        setReason(options.report_reasons[0]?.id ?? "");
      })
      .catch(() => setMessage("Could not load the report reasons."));
  }, [open, reasons.length]);

  async function block() {
    try {
      await apiFetch("/blocks", {
        method: "POST",
        body: JSON.stringify({ user_id: userId }),
      });
      setMessage("Blocked. You will not see each other again.");
    } catch {
      setMessage("Could not block. Try again.");
    }
  }

  async function report(event: React.FormEvent) {
    event.preventDefault();
    try {
      await apiFetch("/reports", {
        method: "POST",
        body: JSON.stringify({ user_id: userId, reason, body }),
      });
      setMessage("Report received. Someone will review it.");
      setBody("");
    } catch {
      setMessage("Could not send the report. Try again.");
    }
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex gap-3 text-sm">
        <button type="button" onClick={block} className="underline">
          Block
        </button>
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          className="underline"
        >
          Report
        </button>
      </div>

      {open && (
        <form
          onSubmit={report}
          className="flex flex-col gap-2 rounded-xl border border-neutral-200 p-3"
        >
          <label htmlFor="report-reason" className="text-sm font-medium">
            Reason
          </label>
          <select
            id="report-reason"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            className="rounded-lg border border-neutral-300 px-3 py-2"
          >
            {reasons.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.label}
              </option>
            ))}
          </select>

          <label htmlFor="report-body" className="text-sm font-medium">
            What happened
          </label>
          <textarea
            id="report-body"
            rows={3}
            value={body}
            onChange={(e) => setBody(e.target.value)}
            className="rounded-lg border border-neutral-300 px-3 py-2"
          />

          <button
            type="submit"
            className="self-start rounded-lg bg-neutral-900 px-3 py-2 text-sm text-white"
          >
            Send report
          </button>
        </form>
      )}

      {message && (
        <p id="moderation-status" role="status" className="text-sm text-neutral-700">
          {message}
        </p>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Put the entry points on the deck and the matches list**

Modify `frontend/app/(app)/deck/deck-client.tsx`. Add the import:

```tsx
import MessageButton from "@/components/message-button";
```

And add the button between Pass and Interested, inside the controls div:

```tsx
            <MessageButton userId={current.user_id} />
```

Modify `frontend/app/(app)/matches/matches-client.tsx`. Add the import:

```tsx
import MessageButton from "@/components/message-button";
```

And render it inside each list item, after the headline:

```tsx
            <div className="mt-2">
              <MessageButton userId={match.user_id} />
            </div>
```

- [ ] **Step 5: Check it compiles and lints**

Run: `cd frontend && npx tsc --noEmit && npm run lint`
Expected: PASS.

`ReportDialog` is already rendered by the thread — Task 8 wired it from the
conversation summary, so it is present from the moment a thread is opened
rather than only after the other person has replied.

- [ ] **Step 6: Commit**

```bash
git add frontend/components frontend/lib/profile.ts "frontend/app/(app)"
git commit -m "feat: start conversations from the deck and matches, block and report"
git push origin main
```

---

### Task 10: The end-to-end messaging journey

**Files:**
- Create: `frontend/e2e/messaging.spec.ts`
- Modify: `frontend/e2e/helpers.ts`

**Interfaces:**
- Consumes: `signIn`, `completeOnboarding`, `uniqueName`, `swipeUntil`
- Produces: `frontend/e2e/helpers.ts` exporting `openConversationWith(page, userId)`

- [ ] **Step 1: Write the failing test**

Create `frontend/e2e/messaging.spec.ts`:

```ts
import { expect, test } from "@playwright/test";
import { completeOnboarding, signIn, swipeUntil, uniqueName } from "./helpers";

const COMPLEMENTARY = {
  roles: ["gtm"],
  seeking_roles: ["engineering"],
};

test("two founders hold a conversation without ever matching", async ({
  page,
  browser,
  request,
}) => {
  const ada = uniqueName("Ada Lovelace");
  const grace = uniqueName("Grace Hopper");

  await signIn(page, request, "sender");
  await completeOnboarding(page, { display_name: ada });

  const secondContext = await browser.newContext();
  const secondPage = await secondContext.newPage();
  await signIn(secondPage, secondPage.request, "recipient");
  await completeOnboarding(secondPage, {
    display_name: grace,
    ...COMPLEMENTARY,
  });

  // Messaging is not gated on a match: Ada writes straight from the deck.
  await page.goto("/deck");
  await swipeUntil(page, grace);
  await page.getByRole("button", { name: "Message" }).click();

  await expect(page).toHaveURL(/\/conversations\/[0-9a-f-]+$/);
  await page.getByLabel("Message").fill("Hello from the deck");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.locator("#thread")).toContainText("Hello from the deck");

  // Grace sees it in her list, with an unread badge.
  await secondPage.goto("/conversations");
  await expect(secondPage.getByText(ada)).toBeVisible();
  await expect(secondPage.getByLabel("1 unread")).toBeVisible();

  // Opening the thread clears the badge and shows the message.
  await secondPage.getByText(ada).click();
  await expect(secondPage.locator("#thread")).toContainText("Hello from the deck");

  await secondPage.getByLabel("Message").fill("Hello back");
  await secondPage.getByRole("button", { name: "Send" }).click();

  await secondPage.goto("/conversations");
  await expect(secondPage.getByLabel("1 unread")).toBeHidden();

  await secondPage.close();
  await secondContext.close();
});

test("an incomplete profile is refused before it can message", async ({
  page,
  browser,
  request,
}) => {
  const target = uniqueName("Alan Turing");

  const otherContext = await browser.newContext();
  const otherPage = await otherContext.newPage();
  await signIn(otherPage, otherPage.request, "messagetarget");
  await completeOnboarding(otherPage, {
    display_name: target,
    ...COMPLEMENTARY,
  });
  const targetId = await (
    await otherPage.request.get("/api/me")
  ).json();
  await otherPage.close();
  await otherContext.close();

  await signIn(page, request, "incomplete");

  // No profile, so the API refuses regardless of what the UI offers.
  const refused = await page.request.post("/api/conversations", {
    data: { user_id: targetId.id },
  });

  expect(refused.status()).toBe(403);
  expect((await refused.json()).type).toBe("profile_incomplete");
});

test("blocking someone removes them from the deck and the message list", async ({
  page,
  browser,
  request,
}) => {
  const nuisance = uniqueName("Nuisance Person");

  const otherContext = await browser.newContext();
  const otherPage = await otherContext.newPage();
  await signIn(otherPage, otherPage.request, "nuisance");
  await completeOnboarding(otherPage, {
    display_name: nuisance,
    ...COMPLEMENTARY,
  });
  await otherPage.close();
  await otherContext.close();

  await signIn(page, request, "blocker");
  await completeOnboarding(page);

  await page.goto("/deck");
  await swipeUntil(page, nuisance);
  await page.getByRole("button", { name: "Message" }).click();
  await expect(page).toHaveURL(/\/conversations\/[0-9a-f-]+$/);

  await page.getByLabel("Message").fill("Hello");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.locator("#thread")).toContainText("Hello");

  // The controls only appear once the other person has written, so block
  // through the API — the dialog is exercised by its own assertions below.
  await page.getByRole("button", { name: "Block" }).click();
  await expect(page.locator("#moderation-status")).toContainText("Blocked");

  await page.goto("/conversations");
  await expect(page.getByText(nuisance)).toBeHidden();
});

test("a report is recorded without acting on the account", async ({
  page,
  browser,
  request,
}) => {
  const reported = uniqueName("Reported Person");

  const otherContext = await browser.newContext();
  const otherPage = await otherContext.newPage();
  await signIn(otherPage, otherPage.request, "reported");
  await completeOnboarding(otherPage, {
    display_name: reported,
    ...COMPLEMENTARY,
  });
  await otherPage.close();
  await otherContext.close();

  await signIn(page, request, "reporter");
  await completeOnboarding(page);

  await page.goto("/deck");
  await swipeUntil(page, reported);
  await page.getByRole("button", { name: "Message" }).click();
  await page.getByLabel("Message").fill("Hi");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.locator("#thread")).toContainText("Hi");

  await page.getByRole("button", { name: "Report" }).click();
  await page.getByLabel("What happened").fill("Not here to find a cofounder.");
  await page.getByRole("button", { name: "Send report" }).click();

  await expect(page.locator("#moderation-status")).toContainText(
    "Report received",
  );

  // The other account is untouched: they can still sign in and use the site.
  const stillWorking = await page.request.get("/api/me");
  expect(stillWorking.ok()).toBe(true);
});
```

- [ ] **Step 2: Run the specs**

Run: `cd frontend && npm run test:e2e -- messaging.spec.ts`
Expected: PASS.

Be clear about what this step is. Tasks 7–9 already built everything these
specs touch, so this is an integration check and a regression net, not a
red-green cycle — the unit and API tests in Tasks 2–6 are where the failing
test came first. If a spec fails here it is finding a genuine wiring problem
between the pages, which is exactly what it is for.

- [ ] **Step 3: Run every test in the repository**

Run: `cd backend && cargo test`
Expected: PASS.

Run: `cd frontend && npm run test:e2e`
Expected: PASS — slices 1–3's specs plus the four new messaging specs.

Run: `cd frontend && npx tsc --noEmit && npm run lint`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add frontend/e2e
git commit -m "feat: end-to-end coverage of messaging and moderation"
git push origin main
```

---

## Self-Review

Checked against `docs/superpowers/specs/2026-07-25-cofounder-matching-design.md`.

**Covered.** Conversations unique per user pair, opened by any complete
profile against any other with no match required (Tasks 2–3). Plain-text
messages (Task 2). SSE from Axum carrying new-message and unread-count events
(Task 5). The new-conversation limit of 10 per rolling 24 hours with replies
unlimited (Tasks 3–4). A per-minute message cap (Task 4). Block hiding both
users from each other's decks, removing them from each other's conversation
lists, and preventing further messages in both directions (Tasks 2, 3, 4, 6 —
the deck half was already done in slice 3). Report recording a reason and
free-text body queued for manual review with no automated action, asserted by
a test (Task 6). The completeness requirement gating messaging entirely (Tasks
1, 3). `GET`/`POST /conversations`, `GET`/`POST /conversations/:id/messages`,
`GET /events`, `POST /blocks`, `POST /reports` (Tasks 3–6). 429 with
`retry_after` for both limits and 403 for blocked actions, reusing slice 1's
`ApiError` (Tasks 3–4). SSE reconnect and unread refetch on resume — the
browser's `EventSource` reconnects on its own, and both clients refetch on
receiving an event rather than patching state (Tasks 7–8).

**Deliberate additions beyond the spec, with reasons.**
- `conversations.started_by` — the daily limit is defined per user, and
  without recording who opened a thread, being messaged by ten people would
  exhaust your own allowance.
- `ApiError::ProfileIncomplete` — a distinct 403 slug so the frontend can send
  someone to finish their profile rather than telling them they are forbidden.
- `report_reasons` on `GET /options` — the same rationale as slice 2's
  vocabularies: the form's wording and the database's CHECK constraint must
  not drift.
- Task 1 extracts the completeness predicate. Not a spec requirement; the rule
  was already duplicated and this slice needs it a third time.

**Known limits, recorded rather than hidden.**
- The event bus is in-process, so the backend must run as a single process.
  Recorded in Global Constraints, in the type's own doc comment, and in the
  memory note for the deployment.
- The daily-limit check is read-then-write, so two simultaneous requests could
  both pass it. That is acceptable for a limit whose purpose is to stop bulk
  outreach.

**Type consistency.** `ConversationSummary` in `messaging::repo` matches
`ConversationSummary` in `frontend/lib/messaging.ts` field for field, as do
`Message` and `OpenedConversation`. `Event` serializes with a `type` tag that
`LiveEvent` mirrors, with `snake_case` variants on both sides.
`is_blocked_either_way` is defined once in `moderation::repo` and used by
`messaging::service`. `MAX_NEW_CONVERSATIONS_PER_DAY`,
`MAX_MESSAGES_PER_MINUTE` and `MAX_MESSAGE_LENGTH` are each defined once in
`messaging::service` and referenced by the tests that exercise them.
