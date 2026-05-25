//! werk-deploy unit tests — pure helpers (no subprocess, no system mutation).
use werk_deploy::{
    branch_name, crate_binary, extract_running_cdhash, parse_build_summary, parse_target,
    resolve_trace, service_for_crate,
};

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("silas", 3062), "silas/3062");
}

#[test]
fn parse_target_defaults_canonical_and_accepts_both_slots() {
    assert_eq!(parse_target(&["3062".into(), "silas".into()]).unwrap(), "canonical");
    assert_eq!(parse_target(&["3062".into(), "silas".into(), "--target".into(), "werk".into()]).unwrap(), "werk");
    assert_eq!(parse_target(&["--target".into(), "canonical".into()]).unwrap(), "canonical");
    assert!(parse_target(&["--target".into(), "prod".into()]).is_err(), "bad target rejected");
    assert!(parse_target(&["--target".into()]).is_err(), "missing target value rejected");
}

#[test]
fn parse_build_summary_reads_crate_cdhash_pairs() {
    assert_eq!(
        parse_build_summary("chorus-hooks=abc123,chorus-inject=def456"),
        vec![("chorus-hooks".to_string(), "abc123".to_string()), ("chorus-inject".to_string(), "def456".to_string())]
    );
    assert_eq!(parse_build_summary("werk-build=ff00\n"), vec![("werk-build".to_string(), "ff00".to_string())]);
    assert_eq!(parse_build_summary("no pairs here"), Vec::<(String, String)>::new());
}

#[test]
fn extract_running_cdhash_parses_codesign() {
    let out = "Executable=/x\nIdentifier=com.chorus.hook-shim\nCDHash=deadbeef1234\nFlags=0x0\n";
    assert_eq!(extract_running_cdhash(out), Some("deadbeef1234".to_string()));
    assert_eq!(extract_running_cdhash("no cdhash"), None);
}

#[test]
fn crate_binary_maps_hooks_shim_else_identity() {
    assert_eq!(crate_binary("chorus-hooks"), "chorus-hook-shim");
    assert_eq!(crate_binary("chorus-inject"), "chorus-inject");
    assert_eq!(crate_binary("werk-build"), "werk-build");
}

#[test]
fn service_for_crate_strips_chorus_prefix() {
    assert_eq!(service_for_crate("chorus-hooks"), "com.chorus.hooks");
    assert_eq!(service_for_crate("chorus-api"), "com.chorus.api");
    assert_eq!(service_for_crate("chorus-inject"), "com.chorus.inject");
}

#[test]
fn resolve_trace_prefers_env_then_persists() {
    std::env::set_var("CHORUS_TRACE_ID", "dep-trace-1");
    assert_eq!(resolve_trace(990001), "dep-trace-1");
    std::env::remove_var("CHORUS_TRACE_ID");
    let card = 990002u64;
    let p = format!("/tmp/{}-trace", card);
    let _ = std::fs::remove_file(&p);
    let t1 = resolve_trace(card);
    assert!(!t1.is_empty());
    assert_eq!(t1, resolve_trace(card), "trace persists so the chain threads");
    let _ = std::fs::remove_file(&p);
}
