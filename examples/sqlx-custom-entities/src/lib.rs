//! # SQLx custom entities — better-auth-poem on the Vercel runtime
//!
//! A reproduction of better-auth-rs's `sqlx-custom-entities` example, wired
//! through [`better_auth_poem`] (Poem) instead of axum and served via
//! [`vercel_poem`]. Each entity (`SaasUser`, `SaasSession`, ...) carries extra
//! application columns (billing plan, Stripe id, ...) persisted in PostgreSQL.
//!
//! `GET /api/me` is protected by [`better_auth_poem::CurrentSession`] — the
//! extractor yields the fully-hydrated custom `SaasUser` (including the SaaS
//! columns) straight from the database.
//!
//! ## Run it
//!
//! ```bash
//! createdb better_auth_poem_example
//! export DATABASE_URL="postgresql://localhost:5432/better_auth_poem_example"
//! cargo run -p sqlx-custom-entities        # serves on http://127.0.0.1:3000
//! ```
//!
//! Migrations are applied automatically on startup.
//!
//! ## End-to-end test
//!
//! ```bash
//! export DATABASE_URL="postgresql://localhost:5432/better_auth_poem_example"
//! cargo test -p sqlx-custom-entities       # skipped when DATABASE_URL is unset
//! ```

mod entities;

pub use entities::*;

use std::sync::Arc;

use better_auth::plugins::{
    EmailPasswordPlugin, OrganizationPlugin, PasswordManagementPlugin, SessionManagementPlugin,
};
use better_auth::{AuthBuilder, AuthConfig, BetterAuth};
use better_auth_poem::{CurrentSession, PoemIntegration};
use poem::endpoint::BoxEndpoint;
use poem::middleware::AddData;
use poem::web::Json;
use poem::{EndpointExt, Response, Route, get, handler};
use serde::Serialize;
use sqlx::postgres::PgPool;

/// Boxed, thread-safe error used across the example's fallible setup steps.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Connect to PostgreSQL and apply the schema migration (idempotent).
pub async fn connect_and_migrate(database_url: &str) -> Result<PgPool, BoxError> {
    let pool = PgPool::connect(database_url).await?;
    sqlx::raw_sql(include_str!("../migrations/001_init.sql"))
        .execute(&pool)
        .await?;
    Ok(pool)
}

/// Build a `BetterAuth` instance over the custom-entity SQLx adapter.
pub async fn build_auth(pool: PgPool) -> Result<Arc<BetterAuth<SaasAdapter>>, BoxError> {
    let adapter = SaasAdapter::from_pool(pool);

    let config = AuthConfig::new("your-very-secure-secret-key-at-least-32-chars-long")
        .base_url("http://localhost:3000")
        .password_min_length(8);

    let auth = AuthBuilder::new(config)
        .database(adapter)
        .plugin(EmailPasswordPlugin::new().enable_signup(true))
        .plugin(SessionManagementPlugin::new())
        .plugin(PasswordManagementPlugin::new())
        .plugin(OrganizationPlugin::new())
        .build()
        .await?;

    Ok(Arc::new(auth))
}

/// Compose the Poem application: better-auth routes under `/auth`, plus a
/// protected `/api/me`. `AddData` makes the auth instance available to the
/// `CurrentSession` extractor.
pub fn build_app(auth: Arc<BetterAuth<SaasAdapter>>) -> BoxEndpoint<'static, Response> {
    Route::new()
        .at("/api/me", get(get_me))
        .nest("/auth", auth.clone().poem_route())
        .with(AddData::new(auth))
        .boxed()
}

/// Response for `GET /api/me`, including the SaaS-specific user columns.
#[derive(Serialize)]
struct MeResponse {
    id: String,
    email: Option<String>,
    name: Option<String>,
    plan: String,
    stripe_customer_id: Option<String>,
    phone: Option<String>,
}

/// `GET /api/me` — the `CurrentSession` extractor validates the session and
/// loads the full custom `SaasUser` (SaaS columns included) from PostgreSQL.
#[handler]
async fn get_me(session: CurrentSession<SaasAdapter>) -> Json<MeResponse> {
    let user = &session.user;
    Json(MeResponse {
        id: user.id.clone(),
        email: user.email.clone(),
        name: user.display_name.clone(),
        plan: user.plan.clone(),
        stripe_customer_id: user.stripe_customer_id.clone(),
        phone: user.phone.clone(),
    })
}
