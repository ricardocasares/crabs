//! # openapi-hello-world
//!
//! A minimal, well-documented REST API built with [Poem](https://github.com/poem-web/poem)
//! and [`poem_openapi`], served on the Vercel Rust runtime via [`vercel_poem`].
//!
//! The generated OpenAPI 3.0 spec is available at `/spec.json` and the
//! [Scalar](https://scalar.com) interactive UI at `/docs`.
//!
//! ## Endpoints
//!
//! | Method | Path          | Description             |
//! |--------|---------------|-------------------------|
//! | `GET`  | `/hello`      | Return a greeting       |
//! | `GET`  | `/health`     | Health check            |
//! | `GET`  | `/docs`       | Interactive Scalar UI   |
//! | `GET`  | `/spec.json`  | OpenAPI 3.0 JSON spec   |

use poem::Route;
use poem_openapi::{
    ApiResponse, Object, OpenApi, OpenApiService, Tags, param::Query, payload::Json,
};

// ── Tags ──────────────────────────────────────────────────────────────────────

/// Groups endpoints in the generated OpenAPI spec by category.
#[derive(Tags)]
enum ApiTags {
    /// Greeting operations — say hello to the world.
    Greetings,
    /// System-level operations such as health checks and diagnostics.
    System,
}

// ── Schemas ───────────────────────────────────────────────────────────────────

/// A greeting produced by `GET /hello`.
#[derive(Object)]
struct Greeting {
    /// The full greeting message, e.g. `"Hello, Alice!"`.
    message: String,
    /// The name that appears in `message`.
    name: String,
}

/// A health-check response produced by `GET /health`.
#[derive(Object)]
struct Health {
    /// Service status. Always `"ok"` when the endpoint is reachable.
    status: String,
}

// ── Response enums ────────────────────────────────────────────────────────────

/// Responses for `GET /hello`.
#[derive(ApiResponse)]
enum HelloResponse {
    /// The greeting was produced successfully.
    #[oai(status = 200)]
    Ok(Json<Greeting>),
}

/// Responses for `GET /health`.
#[derive(ApiResponse)]
enum HealthResponse {
    /// The service is healthy and reachable.
    #[oai(status = 200)]
    Ok(Json<Health>),
}

// ── API ───────────────────────────────────────────────────────────────────────

struct Api;

#[OpenApi]
impl Api {
    /// Say hello
    ///
    /// Returns a personalised greeting. Pass `name` to address someone
    /// specific; omit it to greet the whole world.
    #[oai(path = "/hello", method = "get", tag = "ApiTags::Greetings")]
    async fn hello(
        &self,
        /// Name of the person (or thing) to greet. Defaults to `"World"` when omitted.
        name: Query<Option<String>>,
    ) -> HelloResponse {
        let name = name.0.unwrap_or_else(|| "World".to_string());
        HelloResponse::Ok(Json(Greeting {
            message: format!("Hello, {name}!"),
            name,
        }))
    }

    /// Health check
    ///
    /// Returns `{ "status": "ok" }` whenever the function is running.
    /// Useful as a deployment smoke test after a Vercel deploy.
    #[oai(path = "/health", method = "get", tag = "ApiTags::System")]
    async fn health(&self) -> HealthResponse {
        HealthResponse::Ok(Json(Health {
            status: "ok".to_string(),
        }))
    }
}

// ── App builder ───────────────────────────────────────────────────────────────

/// Build the Poem application: OpenAPI service, Scalar UI, and spec endpoint.
pub fn build_app() -> impl poem::Endpoint {
    let service = OpenApiService::new(Api, "Hello World API", "1.0.0")
        .description(
            "A minimal Poem + poem-openapi application running on the Vercel Rust runtime.",
        )
        .server("http://localhost:3000");

    let ui = service.scalar();
    let spec = service.spec_endpoint();

    Route::new()
        .nest("/docs", ui)
        .at("/spec.json", spec)
        .nest("/", service)
}
