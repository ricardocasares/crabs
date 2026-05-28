//! Poem framework integration for [`better-auth-rs`].
//!
//! Mirrors the upstream axum integration: exposes a [`PoemIntegration`] trait
//! that turns a configured `Arc<BetterAuth<DB>>` into a Poem [`Route`], plus
//! [`CurrentSession`] / [`OptionalSession`] extractors usable in both plain
//! Poem handlers and `poem-openapi` `#[OpenApi]` handlers.
//!
//! [`better-auth-rs`]: https://docs.rs/better-auth
//! [`Route`]: poem::Route

mod convert;
mod extractor;
mod route;

pub use convert::{auth_error_to_poem, auth_response_to_poem, poem_request_to_auth_request};
pub use extractor::{CurrentSession, OptionalSession};
pub use route::PoemIntegration;
