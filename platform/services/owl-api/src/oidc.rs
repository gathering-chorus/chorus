//! #3356 — the CSS ES256/JWKS verifier moved to the shared chorus-oidc crate.
//! Re-export shim: owl-api's `oidc::*` call sites (verify_any, OidcVerifier, …) unchanged.
pub use chorus_oidc::oidc::*;
