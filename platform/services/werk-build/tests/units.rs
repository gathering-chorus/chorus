//! werk-build unit tests — pure helpers (no subprocess, no fs side effects).
use werk_build::{branch_name, crate_for_path, extract_cdhash, jsonl_line, resolve_trace};

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("silas", 3061), "silas/3061");
}

#[test]
fn crate_for_path_maps_services_paths_only() {
    assert_eq!(crate_for_path("platform/services/werk-build/src/lib.rs"), Some("werk-build".to_string()));
    assert_eq!(crate_for_path("platform/services/chorus-hooks/Cargo.toml"), Some("chorus-hooks".to_string()));
    // not a services crate -> None (chorus-api is TS, no cdhash; docs etc. ignored)
    assert_eq!(crate_for_path("platform/api/src/server.ts"), None);
    assert_eq!(crate_for_path("roles/silas/adr/ADR-032.md"), None);
    assert_eq!(crate_for_path("platform/services/"), None);
}

#[test]
fn extract_cdhash_parses_build_signed_line() {
    let out = "build-signed: cargo build --release in /x\nbuild-signed: cdhash=abc123def\nbuild-signed: done\n";
    assert_eq!(extract_cdhash(out), Some("abc123def".to_string()));
    assert_eq!(extract_cdhash("no hash here"), None);
}

#[test]
fn resolve_trace_prefers_env_over_file() {
    std::env::set_var("CHORUS_TRACE_ID", "env-trace-xyz");
    assert_eq!(resolve_trace(999001), "env-trace-xyz");
    std::env::remove_var("CHORUS_TRACE_ID");
}

#[test]
fn resolve_trace_mints_and_persists_when_absent() {
    std::env::remove_var("CHORUS_TRACE_ID");
    let card = 999002u64;
    let p = format!("/tmp/{}-trace", card);
    let _ = std::fs::remove_file(&p);
    let t1 = resolve_trace(card);
    assert!(!t1.is_empty());
    // persisted -> a second resolve reads the same value back from the file.
    let t2 = resolve_trace(card);
    assert_eq!(t1, t2, "trace must persist to /tmp/<card>-trace so the chain threads");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn jsonl_line_is_well_formed() {
    let l = jsonl_line(123, "build.completed", "silas", 3061, "tr1", ",\"built\":\"werk-build=ab\"");
    assert!(l.contains("\"event\":\"build.completed\""));
    assert!(l.contains("\"card_id\":3061"));
    assert!(l.contains("\"trace_id\":\"tr1\""));
    assert!(l.ends_with("}\n"));
}
