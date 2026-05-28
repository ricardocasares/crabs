use bytes::Bytes;
use http_body::Body as HttpBody;
use http_body_util::{BodyExt, Full};
use poem::http::{HeaderMap, StatusCode};
use poem::{Endpoint, Route, get, handler, post};
use vercel_poem::{to_poem_request, to_vercel_response};

const BIG_LEN: usize = 100_000;

#[handler]
fn hello() -> &'static str {
    "hello world"
}

#[handler]
async fn echo(body: String) -> String {
    body
}

#[handler]
fn teapot() -> poem::Response {
    poem::Response::builder()
        .status(StatusCode::IM_A_TEAPOT)
        .header("x-custom", "yes")
        .body("short and stout")
}

#[handler]
fn big() -> String {
    "x".repeat(BIG_LEN)
}

#[handler]
fn echo_query(req: &poem::Request) -> String {
    req.uri().query().unwrap_or_default().to_string()
}

#[handler]
fn multi_cookie() -> poem::Response {
    poem::Response::builder()
        .header("set-cookie", "a=1")
        .header("set-cookie", "b=2")
        .body(())
}

fn app() -> Route {
    Route::new()
        .at("/", get(hello))
        .at("/echo", post(echo))
        .at("/teapot", get(teapot))
        .at("/big", get(big))
        .at("/query", get(echo_query))
        .at("/cookies", get(multi_cookie))
}

fn req(method: &str, uri: &str, body: impl Into<Bytes>) -> hyper::Request<Full<Bytes>> {
    hyper::Request::builder()
        .method(method)
        .uri(uri)
        .body(Full::new(body.into()))
        .unwrap()
}

/// Drive a request through both conversions and a real Poem endpoint, exactly
/// as the runtime would: hyper request -> poem request -> endpoint -> poem
/// response -> hyper/Vercel response.
async fn roundtrip<E: Endpoint>(
    app: &E,
    req: hyper::Request<Full<Bytes>>,
) -> (StatusCode, HeaderMap, Bytes) {
    let poem_req = to_poem_request(req);
    let poem_resp = app.get_response(poem_req).await;
    let vercel_resp = to_vercel_response(poem_resp)
        .await
        .expect("response conversion");
    let (parts, body) = vercel_resp.into_parts();
    let bytes = body.collect().await.expect("collect body").to_bytes();
    (parts.status, parts.headers, bytes)
}

#[tokio::test]
async fn buffered_get_roundtrips() {
    let req = hyper::Request::builder()
        .method("GET")
        .uri("/")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let (status, _headers, body) = roundtrip(&app(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from("hello world"));
}

#[tokio::test]
async fn request_body_is_delivered_to_handler() {
    let req = hyper::Request::builder()
        .method("POST")
        .uri("/echo")
        .body(Full::new(Bytes::from("ping-pong")))
        .unwrap();

    let (status, _headers, body) = roundtrip(&app(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from("ping-pong"));
}

#[tokio::test]
async fn status_and_headers_are_preserved() {
    let req = hyper::Request::builder()
        .method("GET")
        .uri("/teapot")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let (status, headers, body) = roundtrip(&app(), req).await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
    assert_eq!(headers.get("x-custom").unwrap(), "yes");
    assert_eq!(body, Bytes::from("short and stout"));
}

#[tokio::test]
async fn unknown_route_is_not_found() {
    let req = hyper::Request::builder()
        .method("GET")
        .uri("/nope")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let (status, _headers, _body) = roundtrip(&app(), req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn query_string_is_preserved() {
    let req = hyper::Request::builder()
        .method("GET")
        .uri("/query?a=1&b=two")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let (status, _headers, body) = roundtrip(&app(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from("a=1&b=two"));
}

#[tokio::test]
async fn duplicate_response_headers_are_preserved() {
    let req = hyper::Request::builder()
        .method("GET")
        .uri("/cookies")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let (status, headers, _body) = roundtrip(&app(), req).await;
    assert_eq!(status, StatusCode::OK);

    let cookies: Vec<&str> = headers
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert!(cookies.contains(&"a=1"), "got: {cookies:?}");
    assert!(cookies.contains(&"b=2"), "got: {cookies:?}");
}

#[tokio::test]
async fn streaming_response_preserves_chunks_in_order() {
    let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
        Ok(Bytes::from("a")),
        Ok(Bytes::from("b")),
        Ok(Bytes::from("c")),
    ];
    let poem_resp = poem::Response::builder()
        .header("content-type", "text/event-stream")
        .body(poem::Body::from_bytes_stream(tokio_stream::iter(chunks)));

    let vercel_resp = to_vercel_response(poem_resp)
        .await
        .expect("response conversion");

    // A streamed body has no exact length, so hyper will use chunked encoding
    // (no Content-Length) — which is correct for SSE.
    assert_eq!(
        HttpBody::size_hint(vercel_resp.body()).exact(),
        None,
        "streaming response must not advertise an exact length"
    );

    let mut body = vercel_resp.into_body();
    let mut frames = 0usize;
    let mut data = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.expect("frame");
        if let Ok(d) = frame.into_data() {
            frames += 1;
            data.extend_from_slice(&d);
        }
    }

    assert_eq!(data, b"abc");
    assert!(
        frames >= 2,
        "expected the body to stream as multiple frames, got {frames}"
    );
}

#[tokio::test]
async fn buffered_response_advertises_exact_content_length() {
    let poem_req = to_poem_request(req("GET", "/", Bytes::new()));
    let poem_resp = app().get_response(poem_req).await;
    let vercel_resp = to_vercel_response(poem_resp)
        .await
        .expect("response conversion");

    // Exact size_hint is what makes hyper emit a Content-Length header.
    assert_eq!(
        HttpBody::size_hint(vercel_resp.body()).exact(),
        Some("hello world".len() as u64),
    );
}

#[tokio::test]
async fn large_buffered_body_roundtrips_intact() {
    let poem_req = to_poem_request(req("GET", "/big", Bytes::new()));
    let poem_resp = app().get_response(poem_req).await;
    let vercel_resp = to_vercel_response(poem_resp)
        .await
        .expect("response conversion");

    assert_eq!(
        HttpBody::size_hint(vercel_resp.body()).exact(),
        Some(BIG_LEN as u64),
    );

    let bytes = vercel_resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    assert_eq!(bytes.len(), BIG_LEN);
    assert!(bytes.iter().all(|&b| b == b'x'));
}
