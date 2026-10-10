// @domain: domains
//! athena-gen — #4490, athena-make v2 step 1.
//!
//! Reads the model the repo authors and writes one Rust API crate per product.
//! Shares no source with athena-make (v1): the freeze rule in #4478 keeps v1 to
//! break fixes, and v2 grows here.

pub mod model;
pub mod emit;
