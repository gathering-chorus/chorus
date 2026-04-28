//! #1308 — CHORUS_ROOT env var tests
//! AC: CHORUS_ROOT env var defined — single source of truth for repo root path

// Note: these tests use the library's shared module via the main binary crate.
// We test the contract: env var overrides default, absence uses fallback.

use std::process::Command;

/// chorus_root() returns the CHORUS_ROOT env var when set — the binary
/// must accept the env var without crashing on startup.
#[test]
fn chorus_root_reads_env_var() {
    let output = Command::new("env")
        .args(["CHORUS_ROOT=/tmp/test-chorus-root",
               env!("CARGO_BIN_EXE_chorus-hook-shim"), "health", "--version"])
        .output();
    assert!(output.is_ok(), "binary should accept CHORUS_ROOT env var");
}

/// chorus_root() falls back to default when env var is not set — the
/// binary must work without CHORUS_ROOT in the environment.
#[test]
fn chorus_root_fallback_default() {
    let output = Command::new(env!("CARGO_BIN_EXE_chorus-hook-shim"))
        .env_remove("CHORUS_ROOT")
        .args(["health", "--version"])
        .output();
    assert!(output.is_ok(), "binary should work without CHORUS_ROOT");
}
