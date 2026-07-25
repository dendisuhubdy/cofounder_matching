# Cofounder Matching — Design

**Date:** 2026-07-25
**Status:** Approved, ready for implementation planning

## Summary

A public website where founders find cofounders. Users sign up with a magic link, build a profile, and answer an 18-question work-style assessment. A rule-based scorer ranks every other user against them, and they review candidates one at a time in a swipe deck. Swiping right on someone who already swiped right creates a match. Messaging is open — any user with a complete profile may start a conversation with any other, no match required — with rate limits, blocking, and reporting as the abuse controls.

Frontend is Next.js. Backend is Rust (Axum) over Postgres. The two communicate over HTTP.

## Goals

- A founder can go from signup to a first conversation in one sitting.
- Recommendations are explainable: every card states why it was surfaced.
- The scoring logic is a pure function, exhaustively unit-testable, and replaceable without touching anything else.
- The system is useful with a small user base — no cold-start dependency on volume or training data.

## Non-goals

Video, group formation, events, NDA or equity tooling, paid tiers, native mobile apps, and any LLM-based ranking. The scorer's interface is designed so an LLM re-rank can be added later, but none is built now.

## Architecture

Three deployable pieces: a Next.js frontend, an Axum backend, and a Postgres database.

The Rust service is the only thing that touches the database and the only place business logic lives. It owns magic-link auth, session issuance, profiles, trait scoring, deck generation, swipes, matches, messaging, and moderation. Next.js renders UI and calls the API with a session cookie; it holds no domain logic and no database credentials.

This split was chosen over putting auth in Next.js (via Auth.js) because splitting identity across two services means the Rust side must blindly trust a header from the frontend — coupling that gets worse the moment a second client exists. Writing the magic-link flow in Rust is roughly 150 lines: a token table, an email send, and a verification endpoint.

### Backend stack

- **axum** — HTTP routing, extractors, SSE
- **sqlx** — compile-time-checked queries against Postgres, migrations
- **tokio** — async runtime
- **sha2** — hashing magic-link and session tokens at rest. SHA-256, not a password KDF: these are 256-bit random tokens, so there is no low-entropy secret to slow an attacker down against, and argon2 would add latency to every request for no security gain.
- Session cookies: `HttpOnly`, `Secure`, `SameSite=Lax`, opaque random ID backed by a `sessions` row

### Frontend stack

- Next.js App Router, TypeScript, Tailwind
- Server components for profile and settings pages; client components for the swipe deck and chat
- API base URL from env; all requests forward the session cookie

## Data model

| Table | Purpose |
|---|---|
| `users` | email, created_at, last_active_at, status (active/suspended) |
| `sessions` | opaque session id, user_id, expires_at |
| `magic_link_tokens` | hashed token, user_id, expires_at, consumed_at |
| `profiles` | one per user — display fields, role, situation, commitment, geography |
| `profile_interests` | user_id → industry tag (many-to-many) |
| `question_responses` | user_id, question_id, likert value 1–5 |
| `trait_scores` | user_id, one 0–100 column per axis, derived from responses |
| `swipes` | swiper_id, target_id, direction, created_at — unique on (swiper, target) |
| `matches` | user_a_id, user_b_id (ordered), created_at — created on mutual right-swipe |
| `conversations` | user_a_id, user_b_id (ordered), created_at, last_message_at |
| `messages` | conversation_id, sender_id, body, created_at, read_at |
| `blocks` | blocker_id, blocked_id |
| `reports` | reporter_id, reported_id, reason, body, status |

`trait_scores` is stored rather than recomputed on every deck request, and recalculated whenever a user changes a questionnaire answer.

## Profile

**Identity** — display name, photo, headline, bio, location (city + country), timezone, optional links (LinkedIn, GitHub, personal site).

**Role** — what they bring and what they are looking for, each a multi-select over: engineering, design, product, GTM/sales, ops/finance, research/science.

**Situation** — idea status (has a committed idea / has an idea but flexible / looking to join someone else's) and stage (idea / prototype / users / revenue).

**Commitment** — full-time now / full-time once funded / part-time / exploring.

**Interests** — industry tags, free selection from a fixed list.

A profile is **complete** when it has a bio, at least one role, at least one sought role, a commitment level, and all 18 questionnaire answers. Incomplete profiles never appear in any deck and cannot send messages. This is the primary spam filter.

## Work-style assessment

18 Likert questions (1–5, strongly disagree → strongly agree), three per axis. Each axis includes at least one reverse-scored item so that answering uniformly down the page does not produce a coherent profile.

Six axes, each normalized to 0–100:

| Axis | Low | High |
|---|---|---|
| `risk_tolerance` | de-risk before committing | bet big early |
| `pace_vs_rigor` | build it right | ship it now |
| `conflict_style` | seek harmony | address directly |
| `decision_basis` | trust intuition | require data |
| `work_mode` | deep solo work | constant collaboration |
| `orientation` | near-term execution | long-range vision |

Axis score = mean of its three responses (after reversing flagged items), mapped from 1–5 onto 0–100.

Questions are defined in a single Rust source constant — id, text, axis, reverse flag — so the set is versioned with the code and testable.

## Scoring

Signature: `score(viewer: &ScoredProfile, candidate: &ScoredProfile) -> MatchScore`

A pure function. No database access, no I/O, no clock. `MatchScore` carries a total out of 100 and a list of contributing reasons, each with a component label and human-readable text.

### Components

**Role complementarity — 30 points.** Award up to 15 for candidate roles that intersect the viewer's sought roles, and up to 15 for the reverse. The strongest single signal: a technical founder seeking GTM matched with a GTM founder seeking technical is the archetypal good result.

**Trait fit — 25 points.** Per axis, compute the absolute distance between the two scores, then compare it against that axis's ideal distance. **Not all axes want the same thing:**

| Axis | Ideal | Rationale |
|---|---|---|
| `risk_tolerance` | similar | Divergent risk appetite is a recurring source of cofounder breakup. |
| `pace_vs_rigor` | similar | Ship-fast and build-right partners fight over every release. |
| `conflict_style` | similar | Mismatched conflict styles turn small disagreements into resentment. |
| `decision_basis` | mild difference | Some tension between instinct and evidence is productive. |
| `work_mode` | complementary | One person going deep while another runs external relationships works well. |
| `orientation` | complementary | Vision and execution are the classic pairing. |

Each axis contributes proportionally to how close the pair's actual distance sits to that axis's ideal distance. Axes are equally weighted within the 25 points.

**Commitment and stage — 20 points.** Identical commitment scores full. Adjacent levels (full-time now vs. full-time once funded) score most. Distant levels (full-time now vs. exploring) score near zero. Stage proximity contributes a smaller share. Mismatches are penalized, never filtered — a strong match elsewhere can still outweigh them.

**Interest overlap — 15 points.** Jaccard similarity over industry tags, scaled to the point budget.

**Geography — 10 points.** Same metro area scores full; within three timezones scores partial; otherwise zero.

### Reasons

Each card surfaces the top three reasons, drawn from the highest-contributing components — for example "They sell, you build", "Both ship-fast", "Both in Jakarta". Reasons are generated by the scorer alongside the number, not reconstructed afterward, so displayed explanations cannot drift from actual scoring.

## Deck generation

`GET /deck` computes on demand. There is no precomputed match table; that is a cache for a load problem that does not exist yet.

1. **SQL filter** the candidate pool: exclude self, users already swiped on, blocked in either direction, suspended accounts, and incomplete profiles.
2. **Score** each remaining candidate in Rust.
3. **Adjust** for swipe history — see below.
4. **Return** the top 20 with profile payload, score, and reasons.

### Swipe feedback

Two adjustments, both cheap and both recomputed per request:

- **Pass suppression** — candidates resembling the viewer's 20 most recent left-swipes lose up to 5 points. Resemblance is measured on role and interest tags.
- **Popularity boost** — a candidate's right-swipe rate over the trailing 30 days adds up to 3 points, capped so it cannot outweigh genuine fit.

Neither requires training, batch jobs, or stored model state.

## Swiping and matching

Right swipe records interest; left swipe records a pass. Both are permanent and both exclude the target from future decks.

A right swipe on someone who has already right-swiped the viewer creates a `matches` row and returns a match indicator in the swipe response, which the frontend renders as a match moment. Matches sort to the top of the conversation list.

Matching does **not** gate messaging. It is retained because mutual interest is the strongest available quality signal and is worth surfacing and storing.

## Messaging

Any user with a complete profile may open a conversation with any other user who has a complete profile and has not blocked them. No prior swipe or match is required. In practice reach is still bounded by discovery: the deck is the only way to find another user — there is no browsable directory or search — so people message those they have been shown. Conversations are unique per user pair. Messages are plain text.

Delivery is over SSE from Axum: a client subscribes to `GET /events` and receives new-message and unread-count events. SSE avoids the latency and wasted requests of inbox polling and is well supported by Axum's response types.

### Abuse controls

- **New-conversation limit** — 10 per user per rolling 24 hours. Replies within existing conversations are unlimited.
- **Message rate limit** — per-minute cap per user.
- **Block** — hides both users from each other's decks, removes them from each other's conversation lists, and prevents further messages in both directions.
- **Report** — records a reason and free-text body, queued for manual review. No automated action.
- **Completeness requirement** — incomplete profiles cannot message at all.

Open messaging on an open-signup site is the design decision most likely to need revisiting. The controls above are structured so that gating chat on mutual match later is a policy change at the conversation-creation endpoint, not a rewrite.

## API surface

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/auth/magic-link` | Request a login link |
| `POST` | `/auth/verify` | Consume token, issue session |
| `POST` | `/auth/logout` | Destroy session |
| `GET` | `/me` | Current user identity — id, email, status |
| `GET`/`PUT` | `/me/profile` | Read and update own profile |
| `GET` | `/questions` | Questionnaire definition |
| `GET`/`PUT` | `/me/responses` | Read and submit questionnaire answers |
| `GET` | `/deck` | Scored candidate cards |
| `POST` | `/swipes` | Record a swipe, return match status |
| `GET` | `/matches` | Mutual matches |
| `GET`/`POST` | `/conversations` | List conversations, open a new one |
| `GET`/`POST` | `/conversations/:id/messages` | Read and send messages |
| `GET` | `/events` | SSE stream |
| `POST` | `/blocks` | Block a user |
| `POST` | `/reports` | Report a user |

All routes except `/auth/*` require a valid session.

## Error handling

The API returns RFC-7807-style JSON problem responses with a stable machine-readable `type`, so the frontend can branch on error kind rather than string-matching messages.

- Validation failures return 422 with per-field detail.
- Rate limits return 429 with a `retry_after` value.
- Auth failures return 401; blocked or forbidden actions return 403.
- Magic-link verification failures are deliberately uniform across expired, consumed, and unknown tokens, to avoid disclosing which emails are registered.
- Unexpected errors return 500 with an opaque body and a logged correlation id.

The frontend surfaces field-level errors inline on forms, and a retry affordance for transient failures. SSE disconnects reconnect with exponential backoff and refetch unread state on resume.

## Testing

**Scorer unit tests.** The largest and most important body of tests. Each component is tested independently, plus the per-axis similarity-versus-complementarity table — that table is exactly the kind of logic that is subtly inverted and silently stays wrong. Includes tests that reasons match the components that actually scored highest.

**Assessment scoring tests.** Reverse-item handling and axis normalization, including boundary responses.

**Repository tests.** `sqlx` integration tests against a throwaway Postgres, covering deck filtering (blocks, prior swipes, incomplete profiles), match creation on mutual swipe, and rate-limit accounting.

**API tests.** Request-level tests over the Axum router: auth flow, permission boundaries, error shapes.

**End-to-end.** One Playwright pass on the critical path — sign up, complete questionnaire, swipe, match, message.

## Open questions

None. All design decisions above are settled.
