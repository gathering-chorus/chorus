//! werk-deploy unit tests — pure helpers (no subprocess, no system mutation).
use werk_deploy::{
    branch_name, crate_binary, extract_running_cdhash, parse_build_summary, parse_target,
    resolve_trace, service_for_crate, target_class, TargetClass,
};

#[test]
fn target_class_resolves_rust_services_with_svc_and_bin() {
    // #3092 — chorus-hooks/inject/mcp are the original Rust-service slice.
    // The svc is the launchd unit name; the bin may differ from the crate name
    // (chorus-hooks → chorus-hook-shim mirrors build-signed.sh's resolve_crate).
    match target_class("chorus-hooks").unwrap() {
        TargetClass::RustService { svc, bin } => {
            assert_eq!(svc, "com.chorus.hooks");
            assert_eq!(bin, "chorus-hook-shim");
        }
        other => panic!("expected RustService, got {:?}", other),
    }
    match target_class("chorus-inject").unwrap() {
        TargetClass::RustService { svc, bin } => {
            assert_eq!(svc, "com.chorus.inject");
            assert_eq!(bin, "chorus-inject");
        }
        other => panic!("expected RustService, got {:?}", other),
    }
    match target_class("chorus-mcp").unwrap() {
        TargetClass::RustService { svc, .. } => assert_eq!(svc, "com.chorus.mcp"),
        other => panic!("expected RustService, got {:?}", other),
    }
}

#[test]
fn target_class_resolves_chorus_api_as_ts_service() {
    // #3092 — chorus-api is the net-new TS-service path. Returns dist dir + smoke URL
    // so the deploy step doesn't have to hard-code them.
    match target_class("chorus-api").unwrap() {
        TargetClass::TsService { svc, dist_dir_rel, smoke_url } => {
            assert_eq!(svc, "com.chorus.api");
            assert_eq!(dist_dir_rel, "platform/api/dist");
            assert!(
                smoke_url.contains("/api/chorus/health"),
                "smoke URL should hit chorus-api's health endpoint, got {}",
                smoke_url
            );
        }
        other => panic!("expected TsService, got {:?}", other),
    }
}

#[test]
fn target_class_resolves_werk_cli_verbs_to_cli_verb() {
    // #3092 — werk-* binaries are CLI verbs (no LaunchAgent, no kickstart).
    for verb in &[
        "werk-pull", "werk-commit", "werk-push", "werk-build", "werk-deploy", "werk-accept", "werk-demo",
    ] {
        match target_class(verb).unwrap() {
            TargetClass::CliVerb { bin } => assert_eq!(&bin, verb),
            other => panic!("expected CliVerb for {}, got {:?}", verb, other),
        }
    }
}

#[test]
fn target_class_resolves_chorus_sdk_as_shared_lib() {
    // #3126 — chorus-sdk is the shared-library path: deploy its dist to canonical,
    // then cascade-redeploy + verify graph-discovered consumers. Carries the lib's
    // dist dir so the deploy step doesn't hardcode it.
    match target_class("chorus-sdk").unwrap() {
        TargetClass::SharedLib { name, lib_dist_rel } => {
            assert_eq!(name, "chorus-sdk");
            assert_eq!(lib_dist_rel, "platform/chorus-sdk/dist");
        }
        other => panic!("expected SharedLib, got {:?}", other),
    }
}

#[test]
fn target_class_refuses_unknown_names_with_actionable_message() {
    // Unknown names surface; don't silent-mis-deploy. The error names the three
    // possible kinds so the next-card path is obvious.
    let err = target_class("chorus-future-thing").unwrap_err();
    assert!(err.contains("unknown name"), "error must name 'unknown'; got {}", err);
    assert!(
        err.contains("Rust service") && err.contains("TS service")
            && err.contains("CLI verb") && err.contains("shared lib"),
        "error must enumerate the kinds; got {}",
        err
    );
}

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
