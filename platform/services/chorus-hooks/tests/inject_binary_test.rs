//! #3949 — RETIRED as cargo tests, replaced by the runtime lane.
//!
//! These asserted a RELEASE binary exists at chorus-inject/target/release —
//! ambient build-state, not code under test (#3528: a test brings its own
//! world; a fresh werk has no target/ and can never satisfy this, so the
//! blocking gate went red on every chorus-hooks card). The deploy-state
//! question they asked is already owned by chorus-health's
//! deployed-equals-running check against ~/.chorus/bin (#2734's actual
//! deploy location — target/release was never it).
//!
//! Kept as a file (not deleted) so the retirement is legible; the module
//! intentionally contains no #[test].
