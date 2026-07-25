//! #3356 — chorus-oidc: the shared CSS-token verifier (ES256/JWKS via CSS /.oidc/jwks,
//! HS256 legacy) extracted verbatim from owl-api so owl-api's door AND the chorus-model
//! DAL verify identity through ONE implementation (principle: no-competing-implementations).
//! Behavior-preserving: auth.rs + oidc.rs are the owl-api originals, unedited; query.rs
//! holds the pure helpers they call (owl-api re-exports all of it).
pub mod auth;
pub mod oidc;
pub mod query;
pub use query::{role_from_webid, select_v};
