//! End-to-end tests against a real PostgreSQL database.
//!
//! Skipped unless `DATABASE_URL` is set, e.g.:
//! `DATABASE_URL=postgresql://localhost:5432/better_auth_poem_example cargo test -p sqlx-custom-entities`
//!
//! These exercise the custom SaaS entities through the full stack
//! (vercel-poem/Poem → better-auth-poem → SqlxAdapter → Postgres): `SaasUser`
//! via `/api/me`, and `SaasOrganization` / `SaasMember` / `SaasInvitation` via
//! the organization endpoints.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use poem::endpoint::BoxEndpoint;
use poem::http::StatusCode;
use poem::test::TestClient;
use poem::{Endpoint, Response};
use serde_json::json;
use sqlx_custom_entities::{build_app, build_auth, connect_and_migrate};

type App = BoxEndpoint<'static, Response>;

// ---------------------------------------------------------------------------
// Shared setup — migrate exactly once per test binary via a std::sync::OnceLock
// so that concurrent #[tokio::test]s don't race on CREATE TABLE.
// Each test then gets its own BetterAuth instance (and its own pool connection)
// so there's no shared mutable state between parallel tests.
// ---------------------------------------------------------------------------

static DB_URL: OnceLock<String> = OnceLock::new();

/// Ensure the schema is migrated and return the DATABASE_URL, or `None` if
/// the var is unset (skip signal).
async fn ensure_migrated() -> Option<String> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return None;
    };
    // Run migrations exactly once; all other callers skip straight through.
    if DB_URL.get().is_none() {
        // Only the first caller actually migrates; the rest see the OnceLock set.
        let pool = connect_and_migrate(&database_url)
            .await
            .expect("connect + migrate");
        drop(pool); // migration done; each test opens its own pool
        let _ = DB_URL.set(database_url.clone());
    }
    Some(database_url)
}

/// Build a fresh, independent Poem app for each test.  The schema is already
/// in place from `ensure_migrated()`, so `connect_and_migrate` is idempotent
/// (all CREATE TABLE IF NOT EXISTS are no-ops on the second call).
async fn setup() -> Option<TestClient<App>> {
    let database_url = ensure_migrated().await?;
    let pool = connect_and_migrate(&database_url)
        .await
        .expect("connect pool");
    let auth = build_auth(pool).await.expect("build auth");
    Some(TestClient::new(build_app(auth)))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Process-unique suffix so parallel runs against the same database never
/// collide on unique columns (email, org slug).
fn unique(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{nanos}-{n}")
}

fn unique_email() -> String {
    format!("{}@example.com", unique("e2e"))
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// Sign up a fresh user and return its session token.
async fn sign_up<E: Endpoint>(client: &TestClient<E>, email: &str) -> String {
    let resp = client
        .post("/auth/sign-up/email")
        .body_json(&json!({ "email": email, "password": "secure123", "name": "Test User" }))
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.json()
        .await
        .value()
        .object()
        .get("token")
        .string()
        .to_string()
}

/// Create an organization owned by `token`'s user; return the new org id.
async fn create_org<E: Endpoint>(client: &TestClient<E>, token: &str, slug: &str) -> String {
    let resp = client
        .post("/auth/organization/create")
        .header("authorization", bearer(token))
        .body_json(&json!({ "name": "Acme Inc", "slug": slug }))
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.json()
        .await
        .value()
        .object()
        .get("id")
        .string()
        .to_string()
}

/// Invite `email` to the org as a "member"; return the new invitation id.
async fn invite<E: Endpoint>(
    client: &TestClient<E>,
    token: &str,
    org_id: &str,
    email: &str,
) -> String {
    let resp = client
        .post("/auth/organization/invite-member")
        .header("authorization", bearer(token))
        .body_json(&json!({ "email": email, "role": "member", "organizationId": org_id }))
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.json()
        .await
        .value()
        .object()
        .get("id")
        .string()
        .to_string()
}

/// Accept an invitation as `token`'s user.
async fn accept<E: Endpoint>(client: &TestClient<E>, token: &str, invitation_id: &str) {
    client
        .post("/auth/organization/accept-invitation")
        .header("authorization", bearer(token))
        .body_json(&json!({ "invitationId": invitation_id }))
        .send()
        .await
        .assert_status_is_ok();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_signup_and_me_with_custom_entities() {
    let Some(client) = setup().await else {
        eprintln!("DATABASE_URL not set — skipping SQLx E2E test");
        return;
    };

    let email = unique_email();
    let token = sign_up(&client, &email).await;

    // Protected route — CurrentSession loads the full custom SaasUser from Postgres.
    let resp = client
        .get("/api/me")
        .header("authorization", bearer(&token))
        .send()
        .await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value().object().get("email").assert_string(&email);
    body.value().object().get("name").assert_string("Test User");
    body.value().object().get("plan").assert_string("free");

    // Unauthenticated access is rejected.
    client
        .get("/api/me")
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn e2e_create_organization_persists_custom_org_and_owner_member() {
    let Some(client) = setup().await else {
        return;
    };

    let token = sign_up(&client, &unique_email()).await;
    let slug = unique("acme");

    let resp = client
        .post("/auth/organization/create")
        .header("authorization", bearer(&token))
        .body_json(&json!({ "name": "Acme Inc", "slug": slug }))
        .send()
        .await;
    resp.assert_status_is_ok();
    let body = resp.json().await;

    body.value().object().get("slug").assert_string(&slug);
    body.value().object().get("plan").assert_string("free");
    body.value().object().get("id").assert_not_null();

    let members = body.value().object().get("members").array();
    members.assert_len(1);
    members.get(0).object().get("role").assert_string("owner");
    members.get(0).object().get("userId").assert_not_null();

    let resp = client
        .get("/auth/organization/list")
        .header("authorization", bearer(&token))
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.json()
        .await
        .value()
        .array()
        .assert_contains(|org| org.object().get("slug").string() == slug.as_str());
}

#[tokio::test]
async fn e2e_list_members_returns_owner() {
    let Some(client) = setup().await else {
        return;
    };

    let token = sign_up(&client, &unique_email()).await;
    let org_id = create_org(&client, &token, &unique("acme")).await;

    let resp = client
        .get("/auth/organization/list-members")
        .query("organizationId", &org_id)
        .header("authorization", bearer(&token))
        .send()
        .await;
    resp.assert_status_is_ok();
    let body = resp.json().await;

    assert!(
        body.value().object().get("total").i64() >= 1,
        "expected at least one member"
    );
    body.value()
        .object()
        .get("members")
        .array()
        .assert_contains(|m| m.object().get("role").string() == "owner");
}

#[tokio::test]
async fn e2e_invite_member_and_list_invitations() {
    let Some(client) = setup().await else {
        return;
    };

    let token = sign_up(&client, &unique_email()).await;
    let org_id = create_org(&client, &token, &unique("acme")).await;
    let invitee = unique_email();

    let resp = client
        .post("/auth/organization/invite-member")
        .header("authorization", bearer(&token))
        .body_json(&json!({ "email": invitee, "role": "member", "organizationId": org_id }))
        .send()
        .await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value().object().get("email").assert_string(&invitee);
    body.value().object().get("role").assert_string("member");
    body.value().object().get("status").assert_string("pending");

    let resp = client
        .get("/auth/organization/list-invitations")
        .query("organizationId", &org_id)
        .header("authorization", bearer(&token))
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.json()
        .await
        .value()
        .array()
        .assert_contains(|inv| inv.object().get("email").string() == invitee.as_str());
}

#[tokio::test]
async fn e2e_accept_invitation_creates_member() {
    let Some(client) = setup().await else {
        return;
    };

    let owner_token = sign_up(&client, &unique_email()).await;
    let org_id = create_org(&client, &owner_token, &unique("acme")).await;
    let invitee_email = unique_email();
    let invitation_id = invite(&client, &owner_token, &org_id, &invitee_email).await;

    let invitee_token = sign_up(&client, &invitee_email).await;
    let resp = client
        .post("/auth/organization/accept-invitation")
        .header("authorization", bearer(&invitee_token))
        .body_json(&json!({ "invitationId": invitation_id }))
        .send()
        .await;
    resp.assert_status_is_ok();
    let body = resp.json().await;

    body.value()
        .object()
        .get("invitation")
        .object()
        .get("status")
        .assert_string("accepted");
    body.value()
        .object()
        .get("member")
        .object()
        .get("role")
        .assert_string("member");
    body.value()
        .object()
        .get("member")
        .object()
        .get("userId")
        .assert_not_null();

    let resp = client
        .get("/auth/organization/list-members")
        .query("organizationId", &org_id)
        .header("authorization", bearer(&owner_token))
        .send()
        .await;
    resp.assert_status_is_ok();
    assert!(
        resp.json().await.value().object().get("total").i64() >= 2,
        "expected owner + invitee as members"
    );

    let resp = client
        .get("/auth/organization/list-invitations")
        .query("organizationId", &org_id)
        .header("authorization", bearer(&owner_token))
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.json().await.value().array().assert_contains(|inv| {
        inv.object().get("email").string() == invitee_email.as_str()
            && inv.object().get("status").string() == "accepted"
    });
}

#[tokio::test]
async fn e2e_accepted_member_sees_organization() {
    let Some(client) = setup().await else {
        return;
    };

    let owner_token = sign_up(&client, &unique_email()).await;
    let slug = unique("acme");
    let org_id = create_org(&client, &owner_token, &slug).await;

    let invitee_email = unique_email();
    let invitation_id = invite(&client, &owner_token, &org_id, &invitee_email).await;
    let invitee_token = sign_up(&client, &invitee_email).await;

    let resp = client
        .get("/auth/organization/list")
        .header("authorization", bearer(&invitee_token))
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.json().await.value().array().assert_is_empty();

    accept(&client, &invitee_token, &invitation_id).await;

    let resp = client
        .get("/auth/organization/list")
        .header("authorization", bearer(&invitee_token))
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.json().await.value().array().assert_contains(|org| {
        org.object().get("id").string() == org_id.as_str()
            && org.object().get("slug").string() == slug.as_str()
            && org.object().get("plan").string() == "free"
    });
}

#[tokio::test]
async fn e2e_accept_invitation_with_wrong_email_is_forbidden() {
    let Some(client) = setup().await else {
        return;
    };

    let owner_token = sign_up(&client, &unique_email()).await;
    let org_id = create_org(&client, &owner_token, &unique("acme")).await;
    let invitation_id = invite(&client, &owner_token, &org_id, &unique_email()).await;

    let intruder_token = sign_up(&client, &unique_email()).await;
    let resp = client
        .post("/auth/organization/accept-invitation")
        .header("authorization", bearer(&intruder_token))
        .body_json(&json!({ "invitationId": invitation_id }))
        .send()
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn e2e_non_owner_member_cannot_invite() {
    let Some(client) = setup().await else {
        return;
    };

    let owner_token = sign_up(&client, &unique_email()).await;
    let org_id = create_org(&client, &owner_token, &unique("acme")).await;
    let member_email = unique_email();
    let invitation_id = invite(&client, &owner_token, &org_id, &member_email).await;
    let member_token = sign_up(&client, &member_email).await;
    accept(&client, &member_token, &invitation_id).await;

    let resp = client
        .post("/auth/organization/invite-member")
        .header("authorization", bearer(&member_token))
        .body_json(&json!({ "email": unique_email(), "role": "member", "organizationId": org_id }))
        .send()
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);
}
