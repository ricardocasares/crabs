mod common;

use better_auth::AuthUser;
use better_auth_poem::{CurrentSession, PoemIntegration};
use common::{Db, build_auth, sign_up};
use poem::http::StatusCode;
use poem::middleware::AddData;
use poem::test::TestClient;
use poem::{EndpointExt, Route};
use poem_openapi::{Object, OpenApi, OpenApiService, payload::Json};

#[derive(Object)]
struct MeResponse {
    id: String,
    email: Option<String>,
}

struct Api;

#[OpenApi]
impl Api {
    /// Protected via the better-auth-poem `CurrentSession` extractor — this is
    /// the whole point: a Poem `FromRequest` used inside an `#[OpenApi]` method.
    #[oai(path = "/me", method = "get")]
    async fn me(&self, session: CurrentSession<Db>) -> Json<MeResponse> {
        Json(MeResponse {
            id: session.user.id().to_string(),
            email: session.user.email().map(|s| s.to_string()),
        })
    }
}

async fn app() -> TestClient<impl poem::Endpoint> {
    let auth = build_auth().await;
    let service = OpenApiService::new(Api, "rustelix-test", "0.1.0");
    let spec = service.spec_endpoint();
    let app = Route::new()
        .nest("/api/auth", auth.clone().poem_route())
        .nest("/app", service)
        .at("/spec.json", spec)
        .with(AddData::new(auth));
    TestClient::new(app)
}

#[tokio::test]
async fn openapi_handler_requires_session() {
    let client = app().await;
    let resp = client.get("/app/me").send().await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn openapi_handler_with_session_succeeds() {
    let client = app().await;
    let (cookie, _token) = sign_up(&client, "oai@example.com", "password123").await;

    let resp = client.get("/app/me").header("cookie", cookie).send().await;
    resp.assert_status_is_ok();
    resp.json()
        .await
        .value()
        .object()
        .get("email")
        .assert_string("oai@example.com");
}

#[tokio::test]
async fn openapi_spec_serializes_and_contains_route() {
    let client = app().await;
    let resp = client.get("/spec.json").send().await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value()
        .object()
        .get("paths")
        .object()
        .get("/me")
        .assert_not_null();
}

// Probe: does poem allow an overlapping `/api` catch-all nest to coexist with a
// more-specific `/api/auth` nest? Determines the smoke-test app's mount layout.
#[tokio::test]
async fn overlapping_api_and_api_auth_nests_coexist() {
    let auth = build_auth().await;
    let service = OpenApiService::new(Api, "rustelix-test", "0.1.0");
    let app = Route::new()
        .nest("/api/auth", auth.clone().poem_route())
        .nest("/api", service)
        .with(AddData::new(auth));
    let client = TestClient::new(app);

    // app route reachable under /api
    let (cookie, _t) = sign_up(&client, "overlap@example.com", "password123").await;
    client
        .get("/api/me")
        .header("cookie", cookie)
        .send()
        .await
        .assert_status_is_ok();

    // auth route still reachable under the more-specific prefix
    client
        .get("/api/auth/reference/openapi.json")
        .send()
        .await
        .assert_status_is_ok();
}
