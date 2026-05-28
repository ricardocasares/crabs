# vercel-poem

Run [Poem](https://github.com/poem-web/poem) applications on the [Vercel Rust runtime](https://github.com/vercel/vercel/tree/main/packages/rust).

## Usage

```rust
use poem::{get, handler, Route};

#[handler]
fn hello() -> &'static str {
    "Hello from Poem on Vercel!"
}

#[tokio::main]
async fn main() -> Result<(), vercel_poem::Error> {
    let app = Route::new().at("/", get(hello));
    vercel_poem::run(app).await
}
```

Place this in `api/index.rs`. Vercel picks it up as a serverless function.

## How it works

Poem's `Endpoint` trait is not a `tower::Service`, so the adapter bridges through `vercel_runtime::service_fn`:

1. `to_poem_request` converts the incoming `hyper::Request<Incoming>` into a `poem::Request`, forwarding the body as a lazy stream.
2. The Poem endpoint handles the request via `get_response`.
3. `to_vercel_response` converts the `poem::Response` back:
   - **Buffered** (default): body collected to bytes → exact `Content-Length` header.
   - **Streaming** (`Content-Type: text/event-stream`): body forwarded lazily via `SyncStream` for SSE.

Both conversion functions are public so you can use them outside `run()` if you need finer control.

## Generic body support

`to_poem_request` is generic over the body type (`B: HttpBody + Send`), so it works with the runtime's `hyper::body::Incoming` in production and with `http_body_util::Full<Bytes>` in tests.

## License

MIT OR Apache-2.0
