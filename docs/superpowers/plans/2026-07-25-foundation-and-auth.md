# Foundation & Authentication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A running Rust API and Next.js frontend where a person can enter their email, receive a login link, click it, and land on an authenticated page.

**Architecture:** An Axum service owns Postgres, all business logic, and identity. Next.js renders UI and proxies `/api/*` to the Axum service through a Next rewrite, so the session cookie is first-party and there is no CORS to configure. Login is passwordless: a 256-bit random token is emailed, its SHA-256 hash is stored, and consuming it issues an opaque session token stored the same way.

**Tech Stack:** Rust (axum 0.8, sqlx 0.8, tokio), Postgres 16, Next.js (App Router, TypeScript, Tailwind), Playwright.

This plan is slice 1 of 4 derived from `docs/superpowers/specs/2026-07-25-cofounder-matching-design.md`. Later slices add profiles and the assessment, the scorer and deck, and messaging.

## Global Constraints

- Rust edition 2021. Crate name `cofounder_api`, living in `backend/`.
- **Use `sqlx::query_as` / `sqlx::query`, never the `query!` macros.** The macros require a live database at compile time; the runtime-checked functions do not. This keeps `cargo build` working without a database.
- All database access lives in `repo.rs` files. Handlers and services never write SQL.
- Emails are normalized with `.trim().to_lowercase()` before any database read or write, without exception.
- Tokens are 32 random bytes, base64url-encoded without padding. Only the SHA-256 hash is ever persisted. This applies to both magic-link tokens and session tokens.
- Session cookie is named `session`, and is always `HttpOnly`, `SameSite=Lax`, `Path=/`. `Secure` is set whenever `APP_ENV != "development"`.
- Auth endpoints must not disclose whether an email is registered. `POST /auth/magic-link` returns `202` for every syntactically valid email. Expired, consumed, and unknown tokens all return the identical error.
- Timestamps are `TIMESTAMPTZ` in Postgres and `chrono::DateTime<chrono::Utc>` in Rust.
- Frontend lives in `frontend/`. It never holds database credentials and never calls the backend on any path other than `/api/*`.
- Commit after every task. Conventional-commit prefixes (`feat:`, `test:`, `chore:`).

## File Structure

```
backend/
  Cargo.toml
  .env.example
  migrations/
    0001_users.sql              users table
    0002_magic_link_tokens.sql  login token table
    0003_sessions.sql           session table
  src/
    lib.rs                      module declarations
    main.rs                     binary entrypoint: config, pool, mailer, serve
    app.rs                      AppState and router assembly
    config.rs                   environment parsing
    db.rs                       pool construction
    error.rs                    ApiError and problem+json rendering
    email/
      mod.rs                    Mailer trait
      console.rs                ConsoleMailer (dev) and RecordingMailer (tests)
    users/
      mod.rs
      repo.rs                   User struct, find_or_create_by_email, find_by_id
    auth/
      mod.rs
      tokens.rs                 generate_token, hash_token (pure)
      repo.rs                   magic-link token and session persistence
      service.rs                request_login_link, verify_token, logout
      routes.rs                 HTTP handlers
      extractor.rs              CurrentUser extractor
  tests/
    health.rs
    auth_flow.rs

frontend/
  package.json
  next.config.ts                /api/* rewrite to the backend
  app/layout.tsx
  app/page.tsx                  redirects by auth state
  app/login/page.tsx            email entry form
  app/verify/page.tsx           consumes ?token=, sets session
  app/(app)/layout.tsx          authenticated shell, redirects out if signed out
  app/(app)/home/page.tsx       placeholder landing for signed-in users
  lib/api.ts                    fetch wrapper for /api
  lib/session.ts                server-side getCurrentUser
  e2e/auth.spec.ts              Playwright login journey
  playwright.config.ts
```

Each `repo.rs` owns persistence for one concept, each `service.rs` owns a workflow, and `routes.rs` owns only HTTP shape. A reviewer should be able to read any one of these without opening the others.

---

### Task 1: Rust service skeleton with health check

**Files:**
- Create: `backend/Cargo.toml`, `backend/src/lib.rs`, `backend/src/main.rs`, `backend/src/app.rs`
- Test: `backend/tests/health.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `cofounder_api::app::router() -> axum::Router`

- [ ] **Step 1: Create the crate and add dependencies**

```bash
cd backend 2>/dev/null || (mkdir backend && cd backend)
cargo init --name cofounder_api
cargo add axum@0.8
cargo add tokio@1 --features full
cargo add tower --features util
cargo add tracing tracing-subscriber
cargo add anyhow
cargo add --dev tower --features util
```

Add a library target by creating `src/lib.rs` (next step). Cargo picks it up automatically.

- [ ] **Step 2: Write the failing test**

Create `backend/tests/health.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn health_returns_ok() {
    let app = cofounder_api::app::router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cargo test --test health`
Expected: FAIL — `unresolved import` / `could not find app in cofounder_api`.

- [ ] **Step 4: Implement the router**

Create `backend/src/lib.rs`:

```rust
pub mod app;
```

Create `backend/src/app.rs`:

```rust
use axum::{routing::get, Router};

pub fn router() -> Router {
    Router::new().route("/health", get(health))
}

async fn health() -> &'static str {
    "ok"
}
```

Replace `backend/src/main.rs`:

```rust
use cofounder_api::app;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app::router()).await?;

    Ok(())
}
```

- [ ] **Step 5: Run the test and verify it passes**

Run: `cargo test --test health`
Expected: PASS — 1 passed.

- [ ] **Step 6: Commit**

```bash
git add backend
git commit -m "feat: axum service skeleton with health check"
```

---

### Task 2: Database pool, users table, and user repository

**Files:**
- Create: `backend/migrations/0001_users.sql`, `backend/src/config.rs`, `backend/src/db.rs`, `backend/src/users/mod.rs`, `backend/src/users/repo.rs`, `backend/.env.example`
- Modify: `backend/src/lib.rs`, `backend/src/app.rs`, `backend/src/main.rs`

**Interfaces:**
- Consumes: `app::router()` from Task 1
- Produces:
  - `config::Config { database_url: String, base_url: String, app_env: String, bind_addr: String }`, `Config::from_env() -> anyhow::Result<Config>`
  - `db::connect(database_url: &str) -> anyhow::Result<sqlx::PgPool>`
  - `users::repo::User { id: Uuid, email: String, status: String, created_at: DateTime<Utc>, last_active_at: DateTime<Utc> }`
  - `users::repo::find_or_create_by_email(pool: &PgPool, email: &str) -> sqlx::Result<User>`
  - `users::repo::find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<User>>`
  - `app::AppState { db: PgPool }` and `app::router(state: AppState) -> Router`

- [ ] **Step 1: Add dependencies and start Postgres**

```bash
cd backend
cargo add sqlx@0.8 --no-default-features --features runtime-tokio,tls-rustls,postgres,uuid,chrono,macros,migrate
cargo add uuid@1 --features v4,serde
cargo add chrono@0.4 --features serde
cargo add serde@1 --features derive
cargo add dotenvy
cargo install sqlx-cli --no-default-features --features rustls,postgres
docker run -d --name cofounder-pg -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:16
```

Create `backend/.env.example`:

```
DATABASE_URL=postgres://postgres:postgres@localhost:5432/cofounder
BASE_URL=http://localhost:3000
APP_ENV=development
BIND_ADDR=0.0.0.0:8080
```

```bash
cp .env.example .env
sqlx database create
```

- [ ] **Step 2: Write the migration**

Create `backend/migrations/0001_users.sql`:

```sql
CREATE TABLE users (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email          TEXT NOT NULL UNIQUE,
    status         TEXT NOT NULL DEFAULT 'active'
                   CHECK (status IN ('active', 'suspended')),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_active_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

- [ ] **Step 3: Write the failing test**

Create `backend/tests/users_repo.rs`:

```rust
use cofounder_api::users::repo;
use sqlx::PgPool;

#[sqlx::test]
async fn creates_user_on_first_lookup(pool: PgPool) {
    let user = repo::find_or_create_by_email(&pool, "ada@example.com")
        .await
        .unwrap();

    assert_eq!(user.email, "ada@example.com");
    assert_eq!(user.status, "active");
}

#[sqlx::test]
async fn returns_same_user_on_second_lookup(pool: PgPool) {
    let first = repo::find_or_create_by_email(&pool, "ada@example.com")
        .await
        .unwrap();
    let second = repo::find_or_create_by_email(&pool, "ada@example.com")
        .await
        .unwrap();

    assert_eq!(first.id, second.id);
}

#[sqlx::test]
async fn normalizes_case_and_whitespace(pool: PgPool) {
    let first = repo::find_or_create_by_email(&pool, "Ada@Example.com").await.unwrap();
    let second = repo::find_or_create_by_email(&pool, "  ada@example.com  ").await.unwrap();

    assert_eq!(first.id, second.id);
    assert_eq!(first.email, "ada@example.com");
}

#[sqlx::test]
async fn find_by_id_returns_none_for_unknown_id(pool: PgPool) {
    let result = repo::find_by_id(&pool, uuid::Uuid::new_v4()).await.unwrap();
    assert!(result.is_none());
}
```

- [ ] **Step 4: Run the test and verify it fails**

Run: `cargo test --test users_repo`
Expected: FAIL — `could not find users in cofounder_api`.

- [ ] **Step 5: Implement config, pool, and repository**

Create `backend/src/config.rs`:

```rust
use anyhow::Context;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub base_url: String,
    pub app_env: String,
    pub bind_addr: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            base_url: std::env::var("BASE_URL").context("BASE_URL is required")?,
            app_env: std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string()),
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
        })
    }

    pub fn is_development(&self) -> bool {
        self.app_env == "development"
    }
}
```

Create `backend/src/db.rs`:

```rust
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}
```

Create `backend/src/users/mod.rs`:

```rust
pub mod repo;
```

Create `backend/src/users/repo.rs`:

```rust
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

fn normalize(email: &str) -> String {
    email.trim().to_lowercase()
}

pub async fn find_or_create_by_email(pool: &PgPool, email: &str) -> sqlx::Result<User> {
    // ON CONFLICT DO UPDATE (rather than DO NOTHING) so RETURNING yields a row
    // whether the user already existed or not.
    sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (email)
        VALUES ($1)
        ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email
        RETURNING id, email, status, created_at, last_active_at
        "#,
    )
    .bind(normalize(email))
    .fetch_one(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT id, email, status, created_at, last_active_at
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}
```

Update `backend/src/lib.rs`:

```rust
pub mod app;
pub mod config;
pub mod db;
pub mod users;
```

Update `backend/src/app.rs` to carry state:

```rust
use axum::{routing::get, Router};
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}
```

Update `backend/src/main.rs`:

```rust
use cofounder_api::{app, config::Config, db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;
    let state = app::AppState { db: pool };

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app::router(state)).await?;

    Ok(())
}
```

- [ ] **Step 6: Fix the health test for the new signature**

Update `backend/tests/health.rs` to build state. Because the health route needs no database work but the router now requires a pool, use `#[sqlx::test]`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use sqlx::PgPool;
use tower::ServiceExt;

#[sqlx::test]
async fn health_returns_ok(pool: PgPool) {
    let app = router(AppState { db: pool });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 7: Run the tests and verify they pass**

Run: `cargo test`
Expected: PASS — 5 passed across both test files.

`#[sqlx::test]` creates and drops an isolated database per test and applies `./migrations` automatically. It reads `DATABASE_URL` from `.env`.

- [ ] **Step 8: Commit**

```bash
git add backend
git commit -m "feat: postgres pool, users table, and user repository"
```

---

### Task 3: API error type with problem+json rendering

**Files:**
- Create: `backend/src/error.rs`
- Modify: `backend/src/lib.rs`
- Test: `backend/tests/error_rendering.rs`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `error::FieldError { field: String, message: String }`
  - `error::ApiError` with variants `Unauthorized`, `Forbidden`, `NotFound`, `InvalidToken`, `RateLimited { retry_after_seconds: u64 }`, `Validation(Vec<FieldError>)`, `Internal(anyhow::Error)`
  - `impl IntoResponse for ApiError`, `impl From<sqlx::Error> for ApiError`
  - `error::ApiResult<T> = Result<T, ApiError>`

- [ ] **Step 1: Add dependencies**

```bash
cd backend
cargo add thiserror@2
cargo add serde_json@1
```

- [ ] **Step 2: Write the failing test**

Create `backend/tests/error_rendering.rs`:

```rust
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
    let response = ApiError::Internal(anyhow::anyhow!("connection string was postgres://secret"))
        .into_response();

    assert_eq!(response.status(), 500);

    let body = body_json(response).await;
    let serialized = body.to_string();
    assert!(!serialized.contains("secret"));
    assert_eq!(body["type"], "internal_error");
}
```

```bash
cargo add --dev http-body-util
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cargo test --test error_rendering`
Expected: FAIL — `could not find error in cofounder_api`.

- [ ] **Step 4: Implement the error type**

Create `backend/src/error.rs`:

```rust
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
```

Update `backend/src/lib.rs`:

```rust
pub mod app;
pub mod config;
pub mod db;
pub mod error;
pub mod users;
```

- [ ] **Step 5: Run the test and verify it passes**

Run: `cargo test --test error_rendering`
Expected: PASS — 4 passed.

- [ ] **Step 6: Commit**

```bash
git add backend
git commit -m "feat: api error type with problem+json rendering"
```

---

### Task 4: Token generation and hashing

**Files:**
- Create: `backend/src/auth/mod.rs`, `backend/src/auth/tokens.rs`
- Modify: `backend/src/lib.rs`
- Test: `backend/tests/tokens.rs`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `auth::tokens::generate_token() -> String` — 32 random bytes, base64url no padding (43 chars)
  - `auth::tokens::hash_token(token: &str) -> Vec<u8>` — 32-byte SHA-256 digest

- [ ] **Step 1: Add dependencies**

```bash
cd backend
cargo add rand@0.8
cargo add sha2@0.10
cargo add base64@0.22
```

Pin `rand` to 0.8 explicitly: 0.9 renamed `thread_rng` to `rng`, and the code below uses the 0.8 API.

- [ ] **Step 2: Write the failing test**

Create `backend/tests/tokens.rs`:

```rust
use cofounder_api::auth::tokens::{generate_token, hash_token};

#[test]
fn generated_tokens_are_url_safe_and_full_length() {
    let token = generate_token();

    // 32 bytes base64url-encoded without padding is 43 characters.
    assert_eq!(token.len(), 43);
    assert!(token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
}

#[test]
fn generated_tokens_are_unique() {
    let tokens: std::collections::HashSet<String> =
        (0..1000).map(|_| generate_token()).collect();

    assert_eq!(tokens.len(), 1000);
}

#[test]
fn hashing_is_deterministic() {
    assert_eq!(hash_token("abc"), hash_token("abc"));
}

#[test]
fn different_tokens_hash_differently() {
    assert_ne!(hash_token("abc"), hash_token("abd"));
}

#[test]
fn hash_is_32_bytes_and_not_the_token_itself() {
    let token = generate_token();
    let hash = hash_token(&token);

    assert_eq!(hash.len(), 32);
    assert_ne!(hash, token.as_bytes().to_vec());
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cargo test --test tokens`
Expected: FAIL — `could not find auth in cofounder_api`.

- [ ] **Step 4: Implement token functions**

Create `backend/src/auth/mod.rs`:

```rust
pub mod tokens;
```

Create `backend/src/auth/tokens.rs`:

```rust
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// A 256-bit cryptographically random token, safe to place in a URL.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// SHA-256 of the token. Only this is persisted, so a database leak does not
/// hand over usable login links or sessions.
///
/// A password KDF would be the wrong tool here: the input is already 256 bits
/// of entropy, so there is nothing to brute-force and nothing for key
/// stretching to protect.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
```

Update `backend/src/lib.rs`:

```rust
pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod users;
```

- [ ] **Step 5: Run the test and verify it passes**

Run: `cargo test --test tokens`
Expected: PASS — 5 passed.

- [ ] **Step 6: Commit**

```bash
git add backend
git commit -m "feat: cryptographic token generation and hashing"
```

---

### Task 5: Mailer trait with console and recording implementations

**Files:**
- Create: `backend/src/email/mod.rs`, `backend/src/email/console.rs`
- Modify: `backend/src/lib.rs`, `backend/src/app.rs`, `backend/src/main.rs`
- Test: `backend/tests/mailer.rs`

**Interfaces:**
- Consumes: `app::AppState` from Task 2
- Produces:
  - `email::Mailer` trait with `async fn send_login_link(&self, to: &str, link: &str) -> anyhow::Result<()>`
  - `email::console::ConsoleMailer` — logs the link, used in development
  - `email::console::RecordingMailer` with `sent(&self) -> Vec<(String, String)>` — used in tests
  - `app::AppState { db: PgPool, mailer: Arc<dyn Mailer>, base_url: String, secure_cookies: bool }`

- [ ] **Step 1: Add dependency**

```bash
cd backend
cargo add async-trait
```

- [ ] **Step 2: Write the failing test**

Create `backend/tests/mailer.rs`:

```rust
use cofounder_api::email::console::RecordingMailer;
use cofounder_api::email::Mailer;

#[tokio::test]
async fn recording_mailer_captures_recipient_and_link() {
    let mailer = RecordingMailer::default();

    mailer
        .send_login_link("ada@example.com", "http://localhost:3000/verify?token=xyz")
        .await
        .unwrap();

    let sent = mailer.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "ada@example.com");
    assert_eq!(sent[0].1, "http://localhost:3000/verify?token=xyz");
}

#[tokio::test]
async fn recording_mailer_accumulates_across_sends() {
    let mailer = RecordingMailer::default();

    mailer.send_login_link("a@example.com", "link-a").await.unwrap();
    mailer.send_login_link("b@example.com", "link-b").await.unwrap();

    assert_eq!(mailer.sent().len(), 2);
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cargo test --test mailer`
Expected: FAIL — `could not find email in cofounder_api`.

- [ ] **Step 4: Implement the mailer**

Create `backend/src/email/mod.rs`:

```rust
pub mod console;

#[async_trait::async_trait]
pub trait Mailer: Send + Sync {
    async fn send_login_link(&self, to: &str, link: &str) -> anyhow::Result<()>;
}
```

Create `backend/src/email/console.rs`:

```rust
use std::sync::Mutex;

use super::Mailer;

/// Development mailer: writes the login link to the log instead of sending it.
pub struct ConsoleMailer;

#[async_trait::async_trait]
impl Mailer for ConsoleMailer {
    async fn send_login_link(&self, to: &str, link: &str) -> anyhow::Result<()> {
        tracing::info!(recipient = %to, login_link = %link, "login link (not actually emailed)");
        Ok(())
    }
}

/// Test mailer: keeps every message in memory for assertions.
#[derive(Default)]
pub struct RecordingMailer {
    messages: Mutex<Vec<(String, String)>>,
}

impl RecordingMailer {
    pub fn sent(&self) -> Vec<(String, String)> {
        self.messages.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Mailer for RecordingMailer {
    async fn send_login_link(&self, to: &str, link: &str) -> anyhow::Result<()> {
        self.messages
            .lock()
            .unwrap()
            .push((to.to_string(), link.to_string()));
        Ok(())
    }
}
```

Update `backend/src/lib.rs` to add `pub mod email;`.

Update `backend/src/app.rs`:

```rust
use std::sync::Arc;

use axum::{routing::get, Router};
use sqlx::PgPool;

use crate::email::Mailer;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub mailer: Arc<dyn Mailer>,
    pub base_url: String,
    pub secure_cookies: bool,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}
```

Update `backend/src/main.rs`:

```rust
use std::sync::Arc;

use cofounder_api::{app, config::Config, db, email::console::ConsoleMailer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;

    let state = app::AppState {
        db: pool,
        mailer: Arc::new(ConsoleMailer),
        base_url: config.base_url.clone(),
        secure_cookies: !config.is_development(),
    };

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app::router(state)).await?;

    Ok(())
}
```

- [ ] **Step 5: Fix the health test for the new state shape**

Update `backend/tests/health.rs`:

```rust
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;

#[sqlx::test]
async fn health_returns_ok(pool: PgPool) {
    let app = router(AppState {
        db: pool,
        mailer: Arc::new(RecordingMailer::default()),
        base_url: "http://localhost:3000".into(),
        secure_cookies: false,
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 6: Run the tests and verify they pass**

Run: `cargo test`
Expected: PASS — all tests green.

- [ ] **Step 7: Commit**

```bash
git add backend
git commit -m "feat: mailer trait with console and recording implementations"
```

---

### Task 6: Magic-link issuance endpoint

**Files:**
- Create: `backend/migrations/0002_magic_link_tokens.sql`, `backend/src/auth/repo.rs`, `backend/src/auth/service.rs`, `backend/src/auth/routes.rs`
- Modify: `backend/src/auth/mod.rs`, `backend/src/app.rs`
- Test: `backend/tests/auth_flow.rs`

**Interfaces:**
- Consumes: `users::repo::find_or_create_by_email`, `auth::tokens::{generate_token, hash_token}`, `email::Mailer`, `error::ApiError`, `app::AppState`
- Produces:
  - `auth::repo::issue_magic_link(pool, user_id: Uuid, token_hash: &[u8], ttl_minutes: i64) -> sqlx::Result<()>`
  - `auth::repo::count_recent_magic_links(pool, user_id: Uuid, within_minutes: i64) -> sqlx::Result<i64>`
  - `auth::service::request_login_link(state: &AppState, email: &str) -> ApiResult<()>`
  - `auth::routes::router() -> Router<AppState>` mounting `POST /auth/magic-link`
  - Request body `{ "email": String }`; response `202` with empty body

- [ ] **Step 1: Write the migration**

Create `backend/migrations/0002_magic_link_tokens.sql`:

```sql
CREATE TABLE magic_link_tokens (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  BYTEA NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX magic_link_tokens_user_created_idx
    ON magic_link_tokens (user_id, created_at DESC);
```

- [ ] **Step 2: Write the failing test**

Create `backend/tests/auth_flow.rs`:

```rust
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;

pub fn state_with(pool: PgPool, mailer: Arc<RecordingMailer>) -> AppState {
    AppState {
        db: pool,
        mailer,
        base_url: "http://localhost:3000".into(),
        secure_cookies: false,
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

#[sqlx::test]
async fn magic_link_request_returns_202_and_sends_a_link(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer.clone()));

    let response = app
        .oneshot(post_json(
            "/auth/magic-link",
            serde_json::json!({ "email": "ada@example.com" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let sent = mailer.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "ada@example.com");
    assert!(sent[0].1.starts_with("http://localhost:3000/verify?token="));
}

#[sqlx::test]
async fn magic_link_request_creates_the_user(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool.clone(), mailer));

    app.oneshot(post_json(
        "/auth/magic-link",
        serde_json::json!({ "email": "ada@example.com" }),
    ))
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = $1")
        .bind("ada@example.com")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count, 1);
}

#[sqlx::test]
async fn magic_link_stores_only_the_hash_never_the_token(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool.clone(), mailer.clone()));

    app.oneshot(post_json(
        "/auth/magic-link",
        serde_json::json!({ "email": "ada@example.com" }),
    ))
    .await
    .unwrap();

    let link = mailer.sent()[0].1.clone();
    let token = link.split("token=").nth(1).unwrap().to_string();

    let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM magic_link_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_ne!(stored, token.as_bytes().to_vec());
    assert_eq!(stored, cofounder_api::auth::tokens::hash_token(&token));
}

#[sqlx::test]
async fn magic_link_rejects_a_malformed_email(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer.clone()));

    let response = app
        .oneshot(post_json(
            "/auth/magic-link",
            serde_json::json!({ "email": "not-an-email" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(mailer.sent().is_empty());
}
```

```bash
cargo add --dev serde_json
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cargo test --test auth_flow`
Expected: FAIL — 404 on `/auth/magic-link`, and compile errors for missing modules.

- [ ] **Step 4: Implement the repository**

Create `backend/src/auth/repo.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

pub async fn issue_magic_link(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &[u8],
    ttl_minutes: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO magic_link_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, now() + make_interval(mins => $3))
        "#,
    )
    .bind(user_id)
    .bind(token_hash)
    .bind(ttl_minutes as i32)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn count_recent_magic_links(
    pool: &PgPool,
    user_id: Uuid,
    within_minutes: i64,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM magic_link_tokens
        WHERE user_id = $1
          AND created_at > now() - make_interval(mins => $2)
        "#,
    )
    .bind(user_id)
    .bind(within_minutes as i32)
    .fetch_one(pool)
    .await
}
```

- [ ] **Step 5: Implement the service**

Create `backend/src/auth/service.rs`:

```rust
use crate::app::AppState;
use crate::auth::{repo, tokens};
use crate::error::{ApiError, ApiResult, FieldError};
use crate::users;

pub const MAGIC_LINK_TTL_MINUTES: i64 = 15;

/// Deliberately minimal: a full RFC 5322 parser would reject valid addresses
/// and accept unusable ones. Deliverability is proven by the link itself.
fn is_plausible_email(email: &str) -> bool {
    let trimmed = email.trim();
    match trimmed.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !trimmed.contains(char::is_whitespace)
        }
        None => false,
    }
}

pub async fn request_login_link(state: &AppState, email: &str) -> ApiResult<()> {
    if !is_plausible_email(email) {
        return Err(ApiError::Validation(vec![FieldError {
            field: "email".into(),
            message: "must be a valid email address".into(),
        }]));
    }

    let user = users::repo::find_or_create_by_email(&state.db, email).await?;

    let token = tokens::generate_token();
    let hash = tokens::hash_token(&token);
    repo::issue_magic_link(&state.db, user.id, &hash, MAGIC_LINK_TTL_MINUTES).await?;

    let link = format!("{}/verify?token={}", state.base_url, token);
    state
        .mailer
        .send_login_link(&user.email, &link)
        .await
        .map_err(ApiError::Internal)?;

    Ok(())
}
```

- [ ] **Step 6: Implement the routes**

Create `backend/src/auth/routes.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};

use crate::app::AppState;
use crate::auth::service;
use crate::error::ApiResult;

#[derive(serde::Deserialize)]
pub struct MagicLinkRequest {
    pub email: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/auth/magic-link", post(request_magic_link))
}

async fn request_magic_link(
    State(state): State<AppState>,
    Json(payload): Json<MagicLinkRequest>,
) -> ApiResult<StatusCode> {
    service::request_login_link(&state, &payload.email).await?;

    // 202 regardless of whether the address was already registered, so the
    // response cannot be used to enumerate accounts.
    Ok(StatusCode::ACCEPTED)
}
```

Update `backend/src/auth/mod.rs`:

```rust
pub mod repo;
pub mod routes;
pub mod service;
pub mod tokens;
```

Update `backend/src/app.rs` to merge the auth router:

```rust
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .merge(crate::auth::routes::router())
        .with_state(state)
}
```

- [ ] **Step 7: Run the tests and verify they pass**

Run: `cargo test --test auth_flow`
Expected: PASS — 4 passed.

- [ ] **Step 8: Commit**

```bash
git add backend
git commit -m "feat: magic-link issuance endpoint"
```

---

### Task 7: Token verification and session creation

**Files:**
- Create: `backend/migrations/0003_sessions.sql`
- Modify: `backend/src/auth/repo.rs`, `backend/src/auth/service.rs`, `backend/src/auth/routes.rs`
- Test: `backend/tests/auth_flow.rs` (add cases)

**Interfaces:**
- Consumes: everything from Task 6
- Produces:
  - `auth::repo::consume_magic_link(pool, token_hash: &[u8]) -> sqlx::Result<Option<Uuid>>` — atomically marks consumed and returns the user id, or `None` if expired/consumed/unknown
  - `auth::repo::create_session(pool, user_id: Uuid, token_hash: &[u8], ttl_days: i64) -> sqlx::Result<()>`
  - `auth::service::verify_login_token(state: &AppState, token: &str) -> ApiResult<(User, String)>` — returns the user and the raw session token
  - `POST /auth/verify` with body `{ "token": String }`, responding `200` with the user JSON and a `Set-Cookie: session=...`

- [ ] **Step 1: Write the migration**

Create `backend/migrations/0003_sessions.sql`:

```sql
CREATE TABLE sessions (
    token_hash BYTEA PRIMARY KEY,
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX sessions_user_idx ON sessions (user_id);
```

- [ ] **Step 2: Write the failing test**

Append to `backend/tests/auth_flow.rs`:

```rust
async fn request_link_and_extract_token(app: axum::Router, mailer: &RecordingMailer, email: &str) -> String {
    app.oneshot(post_json(
        "/auth/magic-link",
        serde_json::json!({ "email": email }),
    ))
    .await
    .unwrap();

    let link = mailer.sent().last().unwrap().1.clone();
    link.split("token=").nth(1).unwrap().to_string()
}

#[sqlx::test]
async fn verifying_a_token_sets_a_session_cookie(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());

    let token = request_link_and_extract_token(router(state.clone()), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(post_json(
            "/auth/verify",
            serde_json::json!({ "token": token }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();

    assert!(cookie.starts_with("session="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
}

#[sqlx::test]
async fn a_token_cannot_be_used_twice(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());

    let token = request_link_and_extract_token(router(state.clone()), &mailer, "ada@example.com").await;

    let first = router(state.clone())
        .oneshot(post_json("/auth/verify", serde_json::json!({ "token": token.clone() })))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = router(state)
        .oneshot(post_json("/auth/verify", serde_json::json!({ "token": token })))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn an_unknown_token_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(post_json(
            "/auth/verify",
            serde_json::json!({ "token": "not-a-real-token" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn an_expired_token_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let token = request_link_and_extract_token(router(state.clone()), &mailer, "ada@example.com").await;

    sqlx::query("UPDATE magic_link_tokens SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();

    let response = router(state)
        .oneshot(post_json("/auth/verify", serde_json::json!({ "token": token })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cargo test --test auth_flow`
Expected: FAIL — 404 on `/auth/verify`.

- [ ] **Step 4: Add repository functions**

Append to `backend/src/auth/repo.rs`:

```rust
/// Marks the token consumed and returns its user, in a single statement so
/// two concurrent verifications cannot both succeed. Returns `None` when the
/// token is unknown, already consumed, or expired — the caller must not be
/// able to distinguish these cases.
pub async fn consume_magic_link(pool: &PgPool, token_hash: &[u8]) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        r#"
        UPDATE magic_link_tokens
        SET consumed_at = now()
        WHERE token_hash = $1
          AND consumed_at IS NULL
          AND expires_at > now()
        RETURNING user_id
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &[u8],
    ttl_days: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO sessions (token_hash, user_id, expires_at)
        VALUES ($1, $2, now() + make_interval(days => $3))
        "#,
    )
    .bind(token_hash)
    .bind(user_id)
    .bind(ttl_days as i32)
    .execute(pool)
    .await?;

    Ok(())
}
```

- [ ] **Step 5: Add the service function**

Append to `backend/src/auth/service.rs`:

```rust
use crate::users::repo::User;

pub const SESSION_TTL_DAYS: i64 = 30;

pub async fn verify_login_token(state: &AppState, token: &str) -> ApiResult<(User, String)> {
    let token_hash = tokens::hash_token(token);

    let user_id = repo::consume_magic_link(&state.db, &token_hash)
        .await?
        .ok_or(ApiError::InvalidToken)?;

    let user = users::repo::find_by_id(&state.db, user_id)
        .await?
        .ok_or(ApiError::InvalidToken)?;

    if user.status != "active" {
        return Err(ApiError::Forbidden);
    }

    let session_token = tokens::generate_token();
    let session_hash = tokens::hash_token(&session_token);
    repo::create_session(&state.db, user.id, &session_hash, SESSION_TTL_DAYS).await?;

    Ok((user, session_token))
}
```

- [ ] **Step 6: Add the route and cookie handling**

```bash
cd backend
cargo add axum-extra@0.10 --features cookie
```

Append to `backend/src/auth/routes.rs`:

```rust
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};

pub const SESSION_COOKIE: &str = "session";

#[derive(serde::Deserialize)]
pub struct VerifyRequest {
    pub token: String,
}

pub fn session_cookie(token: String, secure: bool, max_age_days: i64) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .max_age(time::Duration::days(max_age_days))
        .build()
}

async fn verify(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<VerifyRequest>,
) -> ApiResult<(CookieJar, Json<crate::users::repo::User>)> {
    let (user, session_token) = service::verify_login_token(&state, &payload.token).await?;

    let jar = jar.add(session_cookie(
        session_token,
        state.secure_cookies,
        service::SESSION_TTL_DAYS,
    ));

    Ok((jar, Json(user)))
}
```

Add the route to the same file's `router()`:

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/magic-link", post(request_magic_link))
        .route("/auth/verify", post(verify))
}
```

```bash
cargo add time@0.3
```

- [ ] **Step 7: Run the tests and verify they pass**

Run: `cargo test --test auth_flow`
Expected: PASS — 8 passed.

- [ ] **Step 8: Commit**

```bash
git add backend
git commit -m "feat: magic-link verification and session creation"
```

---

### Task 8: CurrentUser extractor, GET /me, and logout

**Files:**
- Create: `backend/src/auth/extractor.rs`
- Modify: `backend/src/auth/mod.rs`, `backend/src/auth/repo.rs`, `backend/src/auth/routes.rs`
- Test: `backend/tests/auth_flow.rs` (add cases)

**Interfaces:**
- Consumes: everything from Task 7
- Produces:
  - `auth::extractor::CurrentUser(pub User)` implementing `FromRequestParts<AppState>` with `Rejection = ApiError`
  - `auth::repo::find_user_by_session(pool, token_hash: &[u8]) -> sqlx::Result<Option<User>>`
  - `auth::repo::delete_session(pool, token_hash: &[u8]) -> sqlx::Result<()>`
  - `GET /me` returning the current user JSON, `401` when signed out
  - `POST /auth/logout` returning `204` and clearing the cookie

- [ ] **Step 1: Write the failing test**

Append to `backend/tests/auth_flow.rs`:

```rust
async fn sign_in(state: AppState, mailer: &RecordingMailer, email: &str) -> String {
    let token = request_link_and_extract_token(router(state.clone()), mailer, email).await;

    let response = router(state)
        .oneshot(post_json("/auth/verify", serde_json::json!({ "token": token })))
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

#[sqlx::test]
async fn me_returns_the_signed_in_user(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(body["email"], "ada@example.com");
}

#[sqlx::test]
async fn me_returns_401_without_a_cookie(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(Request::builder().uri("/me").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn me_returns_401_with_a_forged_cookie(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("cookie", "session=totally-made-up")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn logout_invalidates_the_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let logout = router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header("cookie", cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);

    let after = router(state)
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
}
```

```bash
cargo add --dev http-body-util
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cargo test --test auth_flow`
Expected: FAIL — 404 on `/me`.

- [ ] **Step 3: Add repository functions**

Append to `backend/src/auth/repo.rs`:

```rust
use crate::users::repo::User;

pub async fn find_user_by_session(pool: &PgPool, token_hash: &[u8]) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT u.id, u.email, u.status, u.created_at, u.last_active_at
        FROM sessions s
        JOIN users u ON u.id = s.user_id
        WHERE s.token_hash = $1
          AND s.expires_at > now()
          AND u.status = 'active'
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub async fn delete_session(pool: &PgPool, token_hash: &[u8]) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(pool)
        .await?;

    Ok(())
}
```

- [ ] **Step 4: Implement the extractor**

Create `backend/src/auth/extractor.rs`:

```rust
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;

use crate::app::AppState;
use crate::auth::routes::SESSION_COOKIE;
use crate::auth::{repo, tokens};
use crate::error::ApiError;
use crate::users::repo::User;

/// Extracting this on a handler makes the route require authentication.
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);

        let token = jar
            .get(SESSION_COOKIE)
            .map(|cookie| cookie.value().to_string())
            .ok_or(ApiError::Unauthorized)?;

        let user = repo::find_user_by_session(&state.db, &tokens::hash_token(&token))
            .await?
            .ok_or(ApiError::Unauthorized)?;

        Ok(CurrentUser(user))
    }
}
```

Update `backend/src/auth/mod.rs` to add `pub mod extractor;`.

- [ ] **Step 5: Add the routes**

Append to `backend/src/auth/routes.rs`:

```rust
use axum::routing::get;

use crate::auth::extractor::CurrentUser;
use crate::auth::tokens;

async fn me(CurrentUser(user): CurrentUser) -> Json<crate::users::repo::User> {
    Json(user)
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<(CookieJar, StatusCode)> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        crate::auth::repo::delete_session(&state.db, &tokens::hash_token(cookie.value())).await?;
    }

    // The removal cookie must carry the same Path as the one that was set,
    // or the browser treats it as a different cookie and keeps the original.
    let removal = Cookie::build((SESSION_COOKIE, "")).path("/").build();
    let jar = jar.remove(removal);

    Ok((jar, StatusCode::NO_CONTENT))
}
```

Update the same file's `router()`:

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/magic-link", post(request_magic_link))
        .route("/auth/verify", post(verify))
        .route("/auth/logout", post(logout))
        .route("/me", get(me))
}
```

- [ ] **Step 6: Run the tests and verify they pass**

Run: `cargo test`
Expected: PASS — all green, 12 in `auth_flow`.

- [ ] **Step 7: Commit**

```bash
git add backend
git commit -m "feat: session extractor, GET /me, and logout"
```

---

### Task 9: Rate limit magic-link requests

**Files:**
- Modify: `backend/src/auth/service.rs`
- Test: `backend/tests/auth_flow.rs` (add cases)

**Interfaces:**
- Consumes: `auth::repo::count_recent_magic_links` from Task 6
- Produces: `auth::service::MAX_LINKS_PER_HOUR: i64 = 5`; `request_login_link` returns `ApiError::RateLimited` past the cap

Without this, anyone can use the endpoint to send unlimited mail to an address they do not own.

- [ ] **Step 1: Write the failing test**

Append to `backend/tests/auth_flow.rs`:

```rust
#[sqlx::test]
async fn magic_link_requests_are_capped_per_hour(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());

    for _ in 0..5 {
        let response = router(state.clone())
            .oneshot(post_json(
                "/auth/magic-link",
                serde_json::json!({ "email": "ada@example.com" }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    let sixth = router(state)
        .oneshot(post_json(
            "/auth/magic-link",
            serde_json::json!({ "email": "ada@example.com" }),
        ))
        .await
        .unwrap();

    assert_eq!(sixth.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(sixth.headers().contains_key("retry-after"));
    assert_eq!(mailer.sent().len(), 5);
}

#[sqlx::test]
async fn the_cap_is_per_address(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());

    for _ in 0..5 {
        router(state.clone())
            .oneshot(post_json(
                "/auth/magic-link",
                serde_json::json!({ "email": "ada@example.com" }),
            ))
            .await
            .unwrap();
    }

    let other = router(state)
        .oneshot(post_json(
            "/auth/magic-link",
            serde_json::json!({ "email": "grace@example.com" }),
        ))
        .await
        .unwrap();

    assert_eq!(other.status(), StatusCode::ACCEPTED);
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cargo test --test auth_flow magic_link_requests_are_capped`
Expected: FAIL — got 202, expected 429.

- [ ] **Step 3: Add the check to the service**

In `backend/src/auth/service.rs`, add the constant near `MAGIC_LINK_TTL_MINUTES`:

```rust
pub const MAX_LINKS_PER_HOUR: i64 = 5;
```

Then insert this block in `request_login_link`, immediately after the `find_or_create_by_email` call and before the token is generated:

```rust
    let recent = repo::count_recent_magic_links(&state.db, user.id, 60).await?;
    if recent >= MAX_LINKS_PER_HOUR {
        return Err(ApiError::RateLimited {
            retry_after_seconds: 3600,
        });
    }
```

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cargo test`
Expected: PASS — all green, 14 in `auth_flow`.

- [ ] **Step 5: Commit**

```bash
git add backend
git commit -m "feat: cap magic-link requests at five per hour per address"
```

---

### Task 10: Next.js scaffold, API proxy, and login page

**Files:**
- Create: `frontend/` (via `create-next-app`), `frontend/next.config.ts`, `frontend/lib/api.ts`, `frontend/app/login/page.tsx`
- Modify: `frontend/app/page.tsx`

**Interfaces:**
- Consumes: `POST /api/auth/magic-link` from Task 6
- Produces:
  - `lib/api.ts` exporting `apiFetch<T>(path: string, init?: RequestInit): Promise<T>` and `ApiProblem { type: string; title: string; status: number; errors?: { field: string; message: string }[] }`
  - `/login` page

- [ ] **Step 1: Scaffold the frontend**

```bash
cd /Users/dendisuhubdy/Github/cofounder_matching
npx create-next-app@latest frontend --typescript --tailwind --app --eslint --src-dir=false --import-alias="@/*" --no-turbopack
```

- [ ] **Step 2: Configure the API proxy**

Replace `frontend/next.config.ts`:

```ts
import type { NextConfig } from "next";

const backendUrl = process.env.BACKEND_URL ?? "http://localhost:8080";

const nextConfig: NextConfig = {
  async rewrites() {
    // Proxying keeps the session cookie first-party, which removes any need
    // for CORS or SameSite=None.
    return [{ source: "/api/:path*", destination: `${backendUrl}/:path*` }];
  },
};

export default nextConfig;
```

- [ ] **Step 3: Write the API client**

Create `frontend/lib/api.ts`:

```ts
export interface ApiProblem {
  type: string;
  title: string;
  status: number;
  errors?: { field: string; message: string }[];
  retry_after?: number;
}

export class ApiError extends Error {
  constructor(public problem: ApiProblem) {
    super(problem.title);
    this.name = "ApiError";
  }

  fieldError(field: string): string | undefined {
    return this.problem.errors?.find((e) => e.field === field)?.message;
  }
}

export async function apiFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    credentials: "include",
    headers: { "content-type": "application/json", ...(init.headers ?? {}) },
  });

  if (!response.ok) {
    const problem = (await response.json().catch(() => ({
      type: "internal_error",
      title: "something went wrong",
      status: response.status,
    }))) as ApiProblem;
    throw new ApiError(problem);
  }

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}
```

- [ ] **Step 4: Build the login page**

Create `frontend/app/login/page.tsx`:

```tsx
"use client";

import { useState } from "react";
import { ApiError, apiFetch } from "@/lib/api";

export default function LoginPage() {
  const [email, setEmail] = useState("");
  const [status, setStatus] = useState<"idle" | "sending" | "sent">("idle");
  const [error, setError] = useState<string | null>(null);

  async function onSubmit(event: React.FormEvent) {
    event.preventDefault();
    setStatus("sending");
    setError(null);

    try {
      await apiFetch("/auth/magic-link", {
        method: "POST",
        body: JSON.stringify({ email }),
      });
      setStatus("sent");
    } catch (err) {
      setStatus("idle");
      if (err instanceof ApiError) {
        setError(err.fieldError("email") ?? err.problem.title);
      } else {
        setError("Could not reach the server. Try again.");
      }
    }
  }

  if (status === "sent") {
    return (
      <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center gap-3 p-6">
        <h1 className="text-2xl font-semibold">Check your email</h1>
        <p className="text-neutral-600">
          If {email} has an account, a sign-in link is on its way. It expires in
          15 minutes.
        </p>
      </main>
    );
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center gap-6 p-6">
      <div>
        <h1 className="text-2xl font-semibold">Find a cofounder</h1>
        <p className="mt-1 text-neutral-600">
          Enter your email and we&apos;ll send you a sign-in link.
        </p>
      </div>

      <form onSubmit={onSubmit} className="flex flex-col gap-3">
        <label htmlFor="email" className="sr-only">
          Email
        </label>
        <input
          id="email"
          type="email"
          required
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="you@example.com"
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? "email-error" : undefined}
          className="rounded-lg border border-neutral-300 px-3 py-2"
        />

        {error && (
          <p id="email-error" role="alert" className="text-sm text-red-600">
            {error}
          </p>
        )}

        <button
          type="submit"
          disabled={status === "sending"}
          className="rounded-lg bg-neutral-900 px-3 py-2 text-white disabled:opacity-50"
        >
          {status === "sending" ? "Sending…" : "Send sign-in link"}
        </button>
      </form>
    </main>
  );
}
```

- [ ] **Step 5: Verify by hand**

Run the backend in one terminal and the frontend in another:

```bash
cd backend && cargo run
cd frontend && npm run dev
```

Open `http://localhost:3000/login`, submit `ada@example.com`, and confirm the "Check your email" screen appears and the backend log shows a `login_link` line. Submit `nope` and confirm the inline validation message appears.

- [ ] **Step 6: Commit**

```bash
git add frontend
git commit -m "feat: next.js scaffold with api proxy and login page"
```

---

### Task 11: Verify page, session helper, and authenticated shell

**Files:**
- Create: `frontend/app/verify/page.tsx`, `frontend/lib/session.ts`, `frontend/app/(app)/layout.tsx`, `frontend/app/(app)/home/page.tsx`
- Modify: `frontend/app/page.tsx`

**Interfaces:**
- Consumes: `POST /api/auth/verify`, `GET /api/me`, `POST /api/auth/logout`
- Produces:
  - `lib/session.ts` exporting `User { id: string; email: string; status: string }` and `getCurrentUser(): Promise<User | null>` (server-side)
  - `/verify` page, `(app)` route group requiring auth, `/home` placeholder

- [ ] **Step 1: Write the server-side session helper**

Create `frontend/lib/session.ts`:

```ts
import { cookies } from "next/headers";

export interface User {
  id: string;
  email: string;
  status: string;
}

const BACKEND_URL = process.env.BACKEND_URL ?? "http://localhost:8080";

/// Server components cannot use the /api rewrite, so they call the backend
/// directly and forward the incoming cookie header themselves.
export async function getCurrentUser(): Promise<User | null> {
  const cookieHeader = (await cookies()).toString();
  if (!cookieHeader) return null;

  const response = await fetch(`${BACKEND_URL}/me`, {
    headers: { cookie: cookieHeader },
    cache: "no-store",
  });

  if (!response.ok) return null;
  return (await response.json()) as User;
}
```

- [ ] **Step 2: Build the verify page**

Create `frontend/app/verify/page.tsx`:

```tsx
"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { apiFetch } from "@/lib/api";

export default function VerifyPage() {
  const router = useRouter();
  const params = useSearchParams();
  const [failed, setFailed] = useState(false);
  const attempted = useRef(false);

  useEffect(() => {
    // The token is single-use, so React's development double-render must not
    // consume it twice.
    if (attempted.current) return;
    attempted.current = true;

    const token = params.get("token");
    if (!token) {
      setFailed(true);
      return;
    }

    apiFetch("/auth/verify", {
      method: "POST",
      body: JSON.stringify({ token }),
    })
      .then(() => router.replace("/home"))
      .catch(() => setFailed(true));
  }, [params, router]);

  return (
    <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center gap-3 p-6">
      {failed ? (
        <>
          <h1 className="text-2xl font-semibold">This link has expired</h1>
          <p className="text-neutral-600">
            Sign-in links last 15 minutes and work once.
          </p>
          <a href="/login" className="text-neutral-900 underline">
            Request a new one
          </a>
        </>
      ) : (
        <p className="text-neutral-600">Signing you in…</p>
      )}
    </main>
  );
}
```

- [ ] **Step 3: Build the authenticated shell**

Create `frontend/app/(app)/layout.tsx`:

```tsx
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
        <span className="font-semibold">Cofounder</span>
        <span className="text-sm text-neutral-600">{user.email}</span>
      </header>
      <main className="p-6">{children}</main>
    </div>
  );
}
```

Create `frontend/app/(app)/home/page.tsx`:

```tsx
export default function HomePage() {
  return (
    <div className="flex flex-col gap-2">
      <h1 className="text-2xl font-semibold">You&apos;re signed in</h1>
      <p className="text-neutral-600">
        Your profile and the swipe deck arrive in the next slice.
      </p>
    </div>
  );
}
```

Replace `frontend/app/page.tsx`:

```tsx
import { redirect } from "next/navigation";
import { getCurrentUser } from "@/lib/session";

export default async function RootPage() {
  const user = await getCurrentUser();
  redirect(user ? "/home" : "/login");
}
```

- [ ] **Step 4: Verify by hand**

With both servers running, go to `http://localhost:3000`, get redirected to `/login`, submit an email, copy the `login_link` from the backend log, paste it into the browser, and confirm you land on `/home` with your email in the header. Reload `/home` and confirm you stay signed in. Paste the same link a second time and confirm the expired-link screen appears.

- [ ] **Step 5: Commit**

```bash
git add frontend
git commit -m "feat: verify page, session helper, and authenticated shell"
```

---

### Task 12: End-to-end test of the login journey

**Files:**
- Create: `frontend/playwright.config.ts`, `frontend/e2e/auth.spec.ts`
- Modify: `backend/src/app.rs`, `backend/src/email/mod.rs`, `backend/src/email/console.rs`, `backend/src/main.rs`

**Interfaces:**
- Consumes: everything above
- Produces: `GET /test/last-login-link` — mounted **only** when `APP_ENV=test`; returns `{ "link": String }` for the most recently issued link

The e2e test needs to read the emailed link. Rather than run a mail server, the test build exposes the last link through an endpoint that does not exist in any other environment.

- [ ] **Step 1: Add the test-only mailer and route**

Append to `backend/src/email/console.rs`:

```rust
/// Development-and-test mailer that also retains the most recent link so the
/// e2e suite can follow it. Only constructed when APP_ENV=test.
#[derive(Default)]
pub struct LastLinkMailer {
    last: Mutex<Option<String>>,
}

impl LastLinkMailer {
    pub fn last(&self) -> Option<String> {
        self.last.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Mailer for LastLinkMailer {
    async fn send_login_link(&self, to: &str, link: &str) -> anyhow::Result<()> {
        tracing::info!(recipient = %to, login_link = %link, "login link (test mailer)");
        *self.last.lock().unwrap() = Some(link.to_string());
        Ok(())
    }
}
```

Add to `backend/src/app.rs`:

```rust
use axum::Json;
use serde_json::json;

use crate::email::console::LastLinkMailer;

/// Mounted only in the test environment. Exposing this anywhere else would
/// hand every account to anyone who could reach the endpoint.
pub fn test_router() -> Router<AppState> {
    Router::new().route("/test/last-login-link", get(last_login_link))
}

async fn last_login_link(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<serde_json::Value> {
    let link = state
        .test_mailer
        .as_ref()
        .and_then(|mailer| mailer.last());

    Json(json!({ "link": link }))
}
```

Add the field to `AppState`:

```rust
pub test_mailer: Option<Arc<LastLinkMailer>>,
```

Mount it conditionally in `router`:

```rust
pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/health", get(health))
        .merge(crate::auth::routes::router());

    if state.test_mailer.is_some() {
        app = app.merge(test_router());
    }

    app.with_state(state)
}
```

Update `backend/src/main.rs` to wire it:

```rust
    let test_mailer = if config.app_env == "test" {
        Some(Arc::new(cofounder_api::email::console::LastLinkMailer::default()))
    } else {
        None
    };

    let mailer: Arc<dyn cofounder_api::email::Mailer> = match &test_mailer {
        Some(m) => m.clone(),
        None => Arc::new(ConsoleMailer),
    };

    let state = app::AppState {
        db: pool,
        mailer,
        base_url: config.base_url.clone(),
        secure_cookies: !config.is_development(),
        test_mailer,
    };
```

Add `test_mailer: None` to every `AppState` construction in `backend/tests/*.rs`.

- [ ] **Step 2: Run the existing tests and verify they still pass**

Run: `cargo test`
Expected: PASS — all green, unchanged count.

- [ ] **Step 3: Install and configure Playwright**

```bash
cd frontend
npm install --save-dev @playwright/test
npx playwright install chromium
```

Create `frontend/playwright.config.ts`:

```ts
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  use: { baseURL: "http://localhost:3000" },
  webServer: {
    command: "npm run dev",
    url: "http://localhost:3000",
    reuseExistingServer: true,
  },
});
```

- [ ] **Step 4: Write the failing e2e test**

Create `frontend/e2e/auth.spec.ts`:

```ts
import { expect, test } from "@playwright/test";

const BACKEND = process.env.BACKEND_URL ?? "http://localhost:8080";

test("a new user signs in with a magic link", async ({ page, request }) => {
  const email = `ada+${Date.now()}@example.com`;

  await page.goto("/login");
  await page.getByPlaceholder("you@example.com").fill(email);
  await page.getByRole("button", { name: "Send sign-in link" }).click();

  await expect(page.getByText("Check your email")).toBeVisible();

  const response = await request.get(`${BACKEND}/test/last-login-link`);
  const { link } = await response.json();
  expect(link).toContain("/verify?token=");

  await page.goto(link);

  await expect(page).toHaveURL(/\/home$/);
  await expect(page.getByText(email)).toBeVisible();
});

test("a used link is rejected the second time", async ({ page, request }) => {
  const email = `grace+${Date.now()}@example.com`;

  await page.goto("/login");
  await page.getByPlaceholder("you@example.com").fill(email);
  await page.getByRole("button", { name: "Send sign-in link" }).click();
  await expect(page.getByText("Check your email")).toBeVisible();

  const { link } = await (await request.get(`${BACKEND}/test/last-login-link`)).json();

  await page.goto(link);
  await expect(page).toHaveURL(/\/home$/);

  await page.context().clearCookies();
  await page.goto(link);
  await expect(page.getByText("This link has expired")).toBeVisible();
});

test("an unauthenticated visitor is sent to login", async ({ page }) => {
  await page.goto("/home");
  await expect(page).toHaveURL(/\/login$/);
});
```

- [ ] **Step 5: Run the e2e suite**

Start the backend with the test environment, then run Playwright:

```bash
cd backend && APP_ENV=test cargo run &
cd frontend && npx playwright test
```

Expected: PASS — 3 passed.

If the first run fails because the backend was still starting, rerun. If it fails because `/test/last-login-link` returns `{"link": null}`, confirm the backend was started with `APP_ENV=test`.

- [ ] **Step 6: Add a script and commit**

Add to `frontend/package.json` scripts:

```json
"test:e2e": "playwright test"
```

```bash
git add frontend backend
git commit -m "test: end-to-end coverage of the magic-link login journey"
```

---

## Definition of Done

- `cargo test` in `backend/` passes: health, user repository, error rendering, token, mailer, and 14 auth-flow cases.
- `npx playwright test` in `frontend/` passes all three journeys.
- A person can visit `localhost:3000`, enter an email, follow the logged link, and reach `/home` signed in, with the session surviving a reload.
- No token or session value is stored in plaintext anywhere in the database.
- `/test/last-login-link` is absent unless `APP_ENV=test`.

## What This Slice Deliberately Omits

Profiles, the assessment, scoring, the deck, swipes, and messaging — all covered by slices 2 through 4. Also omitted: a real SMTP mailer (`ConsoleMailer` covers development; a `lettre`-based implementation of the same `Mailer` trait is a drop-in addition before launch), session refresh on activity, and expired-row cleanup jobs.
