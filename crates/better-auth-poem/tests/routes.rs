mod common;

use better_auth_poem::PoemIntegration;
use common::{COOKIE_NAME, build_auth, session_cookie};
use poem::middleware::AddData;
use poem::test::TestClient;
use poem::{EndpointExt, Route};
use serde_json::json;

async fn app() -> TestClient<impl poem::Endpoint> {
    let auth = build_auth().await;
    let app = Route::new()
        .nest("/api/auth", auth.clone().poem_route())
        .with(AddData::new(auth));
    TestClient::new(app)
}

#[tokio::test]
async fn sign_up_sets_session_cookie() {
    let client = app().await;
    let resp = client
        .post("/api/auth/sign-up/email")
        .body_json(&json!({
            "email": "alice@example.com",
            "password": "password123",
            "name": "Alice",
        }))
        .send()
        .await;
    resp.assert_status_is_ok();
    let cookie = session_cookie(&resp);
    assert!(cookie.starts_with(COOKIE_NAME), "cookie was: {cookie}");
}

#[tokio::test]
async fn sign_up_with_malformed_json_is_rejected() {
    let client = app().await;
    let resp = client
        .post("/api/auth/sign-up/email")
        .content_type("application/json")
        .body("{ this is not valid json ")
        .send()
        .await;
    let status = resp.0.status().as_u16();
    assert!(
        (400..500).contains(&status),
        "expected 4xx for malformed JSON, got {status}"
    );
}

#[tokio::test]
async fn sign_in_with_bad_credentials_is_unauthorized() {
    let client = app().await;
    let resp = client
        .post("/api/auth/sign-in/email")
        .body_json(&json!({
            "email": "nobody@example.com",
            "password": "wrongpassword",
        }))
        .send()
        .await;
    resp.assert_status(poem::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sign_up_then_sign_in_succeeds() {
    let client = app().await;
    client
        .post("/api/auth/sign-up/email")
        .body_json(&json!({
            "email": "bob@example.com",
            "password": "password123",
            "name": "Bob",
        }))
        .send()
        .await
        .assert_status_is_ok();

    let resp = client
        .post("/api/auth/sign-in/email")
        .body_json(&json!({
            "email": "bob@example.com",
            "password": "password123",
        }))
        .send()
        .await;
    resp.assert_status_is_ok();
    let cookie = session_cookie(&resp);
    assert!(cookie.starts_with(COOKIE_NAME));
}

#[tokio::test]
async fn better_auth_serves_its_own_openapi_spec() {
    let client = app().await;
    let resp = client.get("/api/auth/reference/openapi.json").send().await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    // Must be a valid OpenAPI document with a `paths` object.
    body.value().object().get("paths").assert_not_null();
}
