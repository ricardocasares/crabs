use std::sync::Arc;

use better_auth::BetterAuth;
use better_auth_core::entity::AuthSession;
use better_auth_core::{AuthError, DatabaseAdapter};
use poem::http::StatusCode;
use poem::{Error, FromRequest, Request, RequestBody, Result};

use crate::convert::auth_error_to_poem;

/// Authenticated session extractor.
///
/// Validates the request's session (via `Authorization: Bearer <token>` first,
/// then the configured session cookie) and yields the current user + session.
/// Rejects with `401` when no valid session is present.
///
/// Requires the `Arc<BetterAuth<DB>>` to be attached as request data via
/// [`AddData`](poem::middleware::AddData). Because Poem's blanket
/// `FromRequest -> ApiExtractor` impl applies, this works unchanged inside
/// `poem-openapi` `#[OpenApi]` handlers.
pub struct CurrentSession<DB: DatabaseAdapter> {
    pub user: DB::User,
    pub session: DB::Session,
}

/// Like [`CurrentSession`] but yields `None` instead of rejecting when there's
/// no valid session — for routes that vary behaviour for anonymous users.
pub struct OptionalSession<DB: DatabaseAdapter>(pub Option<CurrentSession<DB>>);

impl<'a, DB: DatabaseAdapter + 'static> FromRequest<'a> for CurrentSession<DB> {
    async fn from_request(req: &'a Request, _body: &mut RequestBody) -> Result<Self> {
        let auth = req.data::<Arc<BetterAuth<DB>>>().ok_or_else(|| {
            Error::from_string(
                "BetterAuth instance missing from request data; mount `.with(AddData::new(auth))`",
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;

        let cookie_name = auth.config().session.cookie_name.clone();
        let token =
            extract_token(req, &cookie_name).ok_or_else(|| reject(AuthError::Unauthenticated))?;

        let session = auth
            .session_manager()
            .get_session(&token)
            .await
            .map_err(reject)?
            .ok_or_else(|| reject(AuthError::SessionNotFound))?;

        let user = auth
            .database()
            .get_user_by_id(session.user_id())
            .await
            .map_err(reject)?
            .ok_or_else(|| reject(AuthError::UserNotFound))?;

        Ok(CurrentSession { user, session })
    }
}

impl<'a, DB: DatabaseAdapter + 'static> FromRequest<'a> for OptionalSession<DB> {
    async fn from_request(req: &'a Request, body: &mut RequestBody) -> Result<Self> {
        match CurrentSession::<DB>::from_request(req, body).await {
            Ok(session) => Ok(OptionalSession(Some(session))),
            Err(_) => Ok(OptionalSession(None)),
        }
    }
}

fn reject(err: AuthError) -> Error {
    Error::from_response(auth_error_to_poem(err))
}

/// Extract a session token: `Authorization: Bearer <token>` first, then the
/// named session cookie. Mirrors the upstream axum extractor.
fn extract_token(req: &Request, cookie_name: &str) -> Option<String> {
    if let Some(auth_header) = req.headers().get("authorization")
        && let Ok(auth_str) = auth_header.to_str()
        && let Some(token) = auth_str.strip_prefix("Bearer ")
    {
        return Some(token.to_string());
    }

    if let Some(cookie_header) = req.headers().get("cookie")
        && let Ok(cookie_str) = cookie_header.to_str()
    {
        let prefix = format!("{}=", cookie_name);
        for part in cookie_str.split(';') {
            let part = part.trim();
            if let Some(value) = part.strip_prefix(&prefix)
                && !value.is_empty()
            {
                return Some(value.to_string());
            }
        }
    }

    None
}
