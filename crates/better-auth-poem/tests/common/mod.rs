#![allow(dead_code)]

use std::sync::Arc;

use better_auth::adapters::MemoryDatabaseAdapter;
use better_auth::plugins::{EmailPasswordPlugin, SessionManagementPlugin};
use better_auth::{AuthBuilder, AuthConfig, BetterAuth};
use poem::Endpoint;
use poem::http::header;
use poem::test::{TestClient, TestResponse};
use serde_json::json;

pub type Db = MemoryDatabaseAdapter;

/// Default better-auth session cookie name (see `SessionConfig::default`).
pub const COOKIE_NAME: &str = "better-auth.session-token";

/// Build a `BetterAuth` instance backed by the in-memory adapter with the
/// email/password + session-management plugins enabled.
pub async fn build_auth() -> Arc<BetterAuth<Db>> {
    let config = AuthConfig::new("test-secret-key-that-is-at-least-32-characters-long")
        .base_url("http://localhost:3000")
        .password_min_length(8);

    Arc::new(
        AuthBuilder::new(config)
            .database(MemoryDatabaseAdapter::new())
            .plugin(EmailPasswordPlugin::new().enable_signup(true))
            .plugin(SessionManagementPlugin::new())
            .build()
            .await
            .expect("failed to build BetterAuth"),
    )
}

/// Sign up a user, asserting success, and return the session token from the
/// response body.
pub async fn sign_up_token<E>(client: &TestClient<E>, email: &str, password: &str) -> String
where
    E: Endpoint,
{
    let resp = client
        .post("/api/auth/sign-up/email")
        .body_json(&json!({ "email": email, "password": password, "name": "Test User" }))
        .send()
        .await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value().object().get("token").string().to_string()
}

/// Sign up a user and return both the session cookie (`name=value`) and the
/// raw session token from the response body.
pub async fn sign_up<E>(client: &TestClient<E>, email: &str, password: &str) -> (String, String)
where
    E: Endpoint,
{
    let resp = client
        .post("/api/auth/sign-up/email")
        .body_json(&json!({ "email": email, "password": password, "name": "Test User" }))
        .send()
        .await;
    resp.assert_status_is_ok();
    let cookie = session_cookie(&resp);
    let token = resp
        .json()
        .await
        .value()
        .object()
        .get("token")
        .string()
        .to_string();
    (cookie, token)
}

/// Extract the `name=value` portion of the session Set-Cookie header.
pub fn session_cookie(resp: &TestResponse) -> String {
    for value in resp.0.headers().get_all(header::SET_COOKIE).iter() {
        if let Ok(s) = value.to_str()
            && s.starts_with(COOKIE_NAME)
        {
            return s.split(';').next().unwrap_or(s).to_string();
        }
    }
    panic!("no `{COOKIE_NAME}` Set-Cookie header in response");
}
