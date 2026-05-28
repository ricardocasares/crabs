# better-auth-poem

[Poem](https://github.com/poem-web/poem) framework integration for [better-auth-rs](https://github.com/better-auth-rs/better-auth-rs).

Mirrors the upstream axum integration: exposes a `PoemIntegration` trait that mounts all better-auth endpoints onto a Poem `Route`, plus `CurrentSession` / `OptionalSession` extractors usable in both plain Poem handlers and `poem-openapi` `#[OpenApi]` handlers.

## Usage

```rust
use better_auth::adapters::MemoryDatabaseAdapter;
use better_auth::plugins::{EmailPasswordPlugin, SessionManagementPlugin};
use better_auth::{AuthBuilder, AuthConfig, AuthUser};
use better_auth_poem::{CurrentSession, PoemIntegration};
use poem::middleware::AddData;
use poem::web::Json;
use poem::{get, handler, EndpointExt, Route};
use std::sync::Arc;

type Db = MemoryDatabaseAdapter;

#[handler]
async fn me(session: CurrentSession<Db>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "id": session.user.id(),
        "email": session.user.email(),
    }))
}

#[tokio::main]
async fn main() {
    let auth = Arc::new(
        AuthBuilder::new(AuthConfig::new("your-secret-key-at-least-32-chars"))
            .database(MemoryDatabaseAdapter::new())
            .plugin(EmailPasswordPlugin::new().enable_signup(true))
            .plugin(SessionManagementPlugin::new())
            .build()
            .await
            .unwrap(),
    );

    let app = Route::new()
        .at("/me", get(me))
        .nest("/auth", auth.clone().poem_route())
        .with(AddData::new(auth));
}
```

### With poem-openapi

`CurrentSession` and `OptionalSession` implement Poem's `FromRequest` trait, which means they work unchanged inside `#[OpenApi]` handlers via the blanket `ApiExtractor` impl:

```rust
use poem_openapi::{payload::Json, Object, OpenApi};

#[derive(Object)]
struct Profile { id: String, email: Option<String> }

struct Api;

#[OpenApi]
impl Api {
    #[oai(path = "/me", method = "get")]
    async fn me(&self, session: CurrentSession<Db>) -> Json<Profile> {
        Json(Profile {
            id: session.user.id().to_string(),
            email: session.user.email().map(str::to_string),
        })
    }
}
```

## OpenAPI

better-auth-rs serves its own OpenAPI spec at `/auth/reference/openapi.json` (when mounted under `/auth`). `poem-openapi` serves the app's spec separately. The two coexist without merging.

## Database adapters

The crate is generic over `DB: DatabaseAdapter` — swap `MemoryDatabaseAdapter` for `SqlxAdapter` (or any custom adapter) without changing the integration code.

## License

MIT OR Apache-2.0
