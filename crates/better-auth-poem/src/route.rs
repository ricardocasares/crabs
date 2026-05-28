use std::sync::Arc;

use better_auth::BetterAuth;
use better_auth_core::DatabaseAdapter;
use poem::{Endpoint, Request, Response, Result, Route};

use crate::convert;

/// Extension trait that turns a configured `Arc<BetterAuth<DB>>` into a Poem
/// [`Route`] hosting every plugin-registered auth endpoint.
///
/// Mirrors the upstream `AxumIntegration::axum_router` shape. Typical usage:
///
/// ```rust,ignore
/// use better_auth_poem::PoemIntegration;
/// use poem::{middleware::AddData, Route};
///
/// let auth = std::sync::Arc::new(auth);
/// let app = Route::new()
///     .nest("/api/auth", auth.clone().poem_route())
///     .with(AddData::new(auth));
/// ```
///
/// [`AddData`](poem::middleware::AddData) is required so [`CurrentSession`] /
/// [`OptionalSession`] extractors can locate the auth instance.
///
/// [`CurrentSession`]: crate::CurrentSession
/// [`OptionalSession`]: crate::OptionalSession
pub trait PoemIntegration<DB: DatabaseAdapter> {
    fn poem_route(self) -> Route;
}

impl<DB: DatabaseAdapter + 'static> PoemIntegration<DB> for Arc<BetterAuth<DB>> {
    fn poem_route(self) -> Route {
        // Better-auth-core matches `(method, path)` internally and honours
        // `disabled_paths` at request-dispatch time, so a single catch-all
        // endpoint avoids duplicating the route registry and any path-syntax
        // translation between axum/Poem placeholders.
        Route::new().nest_no_strip("/", AuthEndpoint { auth: self })
    }
}

struct AuthEndpoint<DB: DatabaseAdapter> {
    auth: Arc<BetterAuth<DB>>,
}

impl<DB: DatabaseAdapter + 'static> Endpoint for AuthEndpoint<DB> {
    type Output = Response;

    async fn call(&self, req: Request) -> Result<Self::Output> {
        let limit_cfg = self.auth.body_limit_config();
        let max_bytes = if limit_cfg.enabled {
            limit_cfg.max_bytes
        } else {
            usize::MAX
        };

        let (req, mut body) = req.split();
        let auth_req = match convert::poem_request_to_auth_request(&req, &mut body, max_bytes).await
        {
            Ok(req) => req,
            Err(err) => return Ok(convert::auth_error_to_poem(err)),
        };

        match self.auth.handle_request(auth_req).await {
            Ok(resp) => Ok(convert::auth_response_to_poem(resp)),
            Err(err) => Ok(convert::auth_error_to_poem(err)),
        }
    }
}
