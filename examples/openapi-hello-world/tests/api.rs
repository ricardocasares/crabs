use openapi_hello_world::build_app;
use poem::test::TestClient;

fn client() -> TestClient<impl poem::Endpoint> {
    TestClient::new(build_app())
}

#[tokio::test]
async fn hello_defaults_to_world() {
    let resp = client().get("/hello").send().await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value()
        .object()
        .get("message")
        .assert_string("Hello, World!");
    body.value().object().get("name").assert_string("World");
}

#[tokio::test]
async fn hello_greets_provided_name() {
    let resp = client().get("/hello").query("name", &"Alice").send().await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value()
        .object()
        .get("message")
        .assert_string("Hello, Alice!");
    body.value().object().get("name").assert_string("Alice");
}

#[tokio::test]
async fn health_returns_ok() {
    let resp = client().get("/health").send().await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    body.value().object().get("status").assert_string("ok");
}

#[tokio::test]
async fn spec_endpoint_is_valid_and_contains_routes() {
    let resp = client().get("/spec.json").send().await;
    resp.assert_status_is_ok();
    let body = resp.json().await;
    // Both endpoints are declared in the spec.
    body.value()
        .object()
        .get("paths")
        .object()
        .get("/hello")
        .assert_not_null();
    body.value()
        .object()
        .get("paths")
        .object()
        .get("/health")
        .assert_not_null();
}

#[tokio::test]
async fn docs_endpoint_returns_html() {
    let resp = client().get("/docs").send().await;
    resp.assert_status_is_ok();
}
