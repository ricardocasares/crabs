#![forbid(unsafe_code)]

//! Run [Poem](https://github.com/poem-web/poem) applications on the Vercel Rust
//! runtime ([`vercel_runtime`]).
//!
//! This is the Poem counterpart to `vercel_runtime`'s built-in axum/actix
//! adapters. A Poem [`Endpoint`] isn't a `tower::Service`, so instead of the
//! layer-stacking approach the axum adapter uses, this crate bridges through
//! [`vercel_runtime::service_fn`]: it converts the incoming hyper request into
//! a [`poem::Request`], drives the endpoint via [`Endpoint::get_response`], and
//! converts the [`poem::Response`] back out.
//!
//! Buffered responses are sent with an exact `Content-Length`; responses whose
//! `Content-Type` is `text/event-stream` are forwarded lazily as a stream (so
//! SSE works) without being collected into memory.
//!
//! ```rust,no_run
//! use poem::{get, handler, Route};
//!
//! #[handler]
//! fn hello() -> &'static str { "Hello from Poem on Vercel!" }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), vercel_poem::Error> {
//!     let app = Route::new().at("/", get(hello));
//!     vercel_poem::run(app).await
//! }
//! ```

use std::sync::Arc;

use bytes::Bytes;
use http_body::Body as HttpBody;
use http_body_util::{BodyStream, StreamBody};
use hyper::body::Frame;
use hyper::header::CONTENT_TYPE;
use poem::{Endpoint, IntoEndpoint};
use sync_wrapper::SyncStream;
use tokio_stream::StreamExt;
use vercel_runtime::{Response, ResponseBody};

#[doc(no_inline)]
pub use vercel_runtime::{Error, Request};

/// Convert an incoming hyper request (the type the Vercel runtime hands to a
/// service) into a [`poem::Request`].
///
/// Generic over the body type so it works with the runtime's
/// `hyper::body::Incoming` in production and with any [`http_body::Body`] (e.g.
/// `http_body_util::Full`) in tests. The request body is forwarded lazily as a
/// stream — it is not buffered here.
pub fn to_poem_request<B>(req: hyper::Request<B>) -> poem::Request
where
    B: HttpBody + Send + 'static,
    B::Data: Into<Bytes>,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let (parts, body) = req.into_parts();

    // http_body::Body -> stream of data Bytes (trailers dropped).
    let data_stream = BodyStream::new(body).filter_map(|frame| match frame {
        Ok(f) => f
            .into_data()
            .ok()
            .map(|data| Ok::<Bytes, std::io::Error>(data.into())),
        Err(e) => Some(Err(std::io::Error::other(e))),
    });

    let mut poem_req = poem::Request::builder()
        .method(parts.method)
        .uri(parts.uri)
        .version(parts.version)
        .body(poem::Body::from_bytes_stream(data_stream));
    *poem_req.headers_mut() = parts.headers;
    *poem_req.extensions_mut() = parts.extensions;
    poem_req
}

/// Convert a [`poem::Response`] into the Vercel runtime's
/// `hyper::Response<ResponseBody>`.
///
/// Buffered responses are collected to bytes so the resulting body has an exact
/// size and hyper emits a `Content-Length`. Responses whose `Content-Type` is
/// `text/event-stream` are forwarded lazily (no buffering) so server-sent
/// events stream as they are produced. In neither case is a background task or
/// channel involved — the body is polled directly by hyper.
pub async fn to_vercel_response(resp: poem::Response) -> Result<Response<ResponseBody>, Error> {
    let (parts, body) = resp.into_parts();

    let response_body = if is_event_stream(&parts.headers) {
        // `poem::Body`'s byte stream is `Send` but not `Sync`; `SyncStream`
        // supplies the `Sync` that `ResponseBody: From<StreamBody<_>>` requires
        // (the same wrapper poem uses internally), so hyper can poll it lazily.
        let frames = body
            .into_bytes_stream()
            .map(|chunk| chunk.map(Frame::data).map_err(|e| Box::new(e) as Error));
        ResponseBody::from(StreamBody::new(SyncStream::new(frames)))
    } else {
        let bytes = body.into_bytes().await.map_err(|e| Box::new(e) as Error)?;
        ResponseBody::from(bytes)
    };

    let mut builder = Response::builder()
        .status(parts.status)
        .version(parts.version);
    if let Some(headers) = builder.headers_mut() {
        *headers = parts.headers;
    }
    builder
        .body(response_body)
        .map_err(|e| Box::new(e) as Error)
}

/// Whether the response should be streamed rather than buffered, keyed on
/// `Content-Type: text/event-stream` (matching `vercel_runtime`'s own adapters).
fn is_event_stream(headers: &hyper::HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_start().starts_with("text/event-stream"))
        .unwrap_or(false)
}

/// Serve a Poem application on the Vercel Rust runtime.
///
/// Accepts anything convertible into a Poem [`Endpoint`] (a `Route`, a single
/// handler, an endpoint wrapped in middleware, etc.) and runs it for the
/// lifetime of the function invocation.
pub async fn run<E>(endpoint: E) -> Result<(), Error>
where
    E: IntoEndpoint,
    E::Endpoint: 'static,
{
    let endpoint = Arc::new(endpoint.into_endpoint());

    vercel_runtime::run(vercel_runtime::service_fn(
        move |req: vercel_runtime::Request| {
            let endpoint = endpoint.clone();
            async move {
                let poem_req = to_poem_request(req);
                let poem_resp = endpoint.get_response(poem_req).await;
                to_vercel_response(poem_resp).await
            }
        },
    ))
    .await
}
