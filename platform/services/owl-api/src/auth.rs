//! #3356 — the verifier moved to the shared chorus-oidc crate (one implementation
//! for owl-api's door AND the chorus-model DAL; principle: no-competing-implementations).
//! This module is now a re-export shim so owl-api's `auth::*` call sites are unchanged.
pub use chorus_oidc::auth::*;
