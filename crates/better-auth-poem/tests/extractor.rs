mod common;

use better_auth::AuthUser;
use better_auth_poem::{CurrentSession, OptionalSession, PoemIntegration};
use common::{Db, build_auth, sign_up};
use poem::http::StatusCode;
use poem::middleware::AddData;
use poem::test::TestClient;
use poem::web::Json;
use poem::{EndpointExt, Route, get, handler};
use serde_json::{Value, json};

#[handler]
async fn protected(session: CurrentSession<Db>) -> Json<Value> {
    Json(json!({
        "id": session.user.id(),
        "email": session.user.email(),
    }))
}

#[handler]
async fn maybe(session: OptionalSession<Db>) -> Json<Value> {
    match session.0 {
        Some(s) => Json(json!({ "id": s.user.id() })),
        None => Json(json!({ "id": Value::Null })),
    }
}

async fn app() -> TestClient<impl poem::Endpoint> {
    let auth = build_auth().await;
    let app = Route::new()
        .nest("/api/auth", auth.clone().poem_route())
        .at("/protected", get(protected))
        .at("/maybe", get(maybe))
        .with(AddData::new(auth));
    TestClient::new(app)
}

#[tokio::test]
async fn protected_without_session_is_unauthorized() {
    let client = app().await;
    let resp = client.get("/protected").send().await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_with_cookie_succeeds() {
    let client = app().await;
    let (cookie, _token) = sign_up(&client, "cookie@example.com", "password123").await;

    let resp = client
        .get("/protected")
        .header("cookie", cookie)
        .send()
        .await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value()
        .object()
        .get("email")
        .assert_string("cookie@example.com");
}

#[tokio::test]
async fn protected_with_bearer_token_succeeds() {
    let client = app().await;
    let (_cookie, token) = sign_up(&client, "bearer@example.com", "password123").await;

    let resp = client
        .get("/protected")
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value()
        .object()
        .get("email")
        .assert_string("bearer@example.com");
}

#[tokio::test]
async fn optional_without_session_returns_null() {
    let client = app().await;
    let resp = client.get("/maybe").send().await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value().object().get("id").assert_null();
}

#[tokio::test]
async fn optional_with_session_returns_user() {
    let client = app().await;
    let (cookie, _token) = sign_up(&client, "optional@example.com", "password123").await;

    let resp = client.get("/maybe").header("cookie", cookie).send().await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value().object().get("id").assert_not_null();
}

/// Well-formed (`session_` prefix, sufficient length) but non-existent token.
const UNKNOWN_TOKEN: &str = "session_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[tokio::test]
async fn missing_auth_data_is_internal_error() {
    // The protected route is mounted WITHOUT `AddData`, so the extractor can't
    // find the BetterAuth instance — this must surface as a 500, not a panic.
    let client = TestClient::new(Route::new().at("/protected", get(protected)));
    let resp = client.get("/protected").send().await;
    resp.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn invalid_token_current_session_is_unauthorized() {
    let client = app().await;
    let resp = client
        .get("/protected")
        .header("authorization", format!("Bearer {UNKNOWN_TOKEN}"))
        .send()
        .await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invalid_token_optional_session_is_none() {
    let client = app().await;
    let resp = client
        .get("/maybe")
        .header("authorization", format!("Bearer {UNKNOWN_TOKEN}"))
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.json().await.value().object().get("id").assert_null();
}

#[tokio::test]
async fn sign_out_invalidates_session() {
    let client = app().await;
    let (_cookie, token) = sign_up(&client, "signout@example.com", "password123").await;

    // The session is valid before signing out.
    client
        .get("/protected")
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .assert_status_is_ok();

    // Sign out deletes the session server-side.
    client
        .post("/api/auth/sign-out")
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .assert_status_is_ok();

    // The same token is now rejected.
    client
        .get("/protected")
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}
