//! werk-deploy unit tests — pure helpers (no subprocess, no system mutation).
use werk_deploy::{
    branch_name, crate_binary, extract_running_cdhash, parse_build_summary, parse_target,
    resolve_trace, service_for_crate, target_class_in, TargetClass,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// #3132 — a self-contained werk fixture mirroring the real repo layout, so
/// `target_class_in` can be exercised structurally (no env, no real repo).
fn werk_fixture(tag: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("wd-tc-{}-{}-{}", tag, std::process::id(), nanos));
    let w = |rel: &str, body: &str| {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, body).unwrap();
    };
    // Rust crates: one with a committed plist (service), werk-* are CLI verbs.
    w("platform/services/chorus-hooks/Cargo.toml", "[package]\nname=\"chorus-hooks\"\n");
    w("platform/services/chorus-inject/Cargo.toml", "[package]\nname=\"chorus-inject\"\n");
    w("platform/services/werk-deploy/Cargo.toml", "[package]\nname=\"werk-deploy\"\n");
    // committed launchd plists (the structural "is it a service?" signal).
    w("config/launchagents/com.chorus.hooks.plist", "<plist><string>chorus-hook-shim</string></plist>");
    w("config/launchagents/com.chorus.inject.plist", "<plist><string>chorus-inject</string></plist>");
    w("config/launchagents/com.chorus.api.plist",
        "<plist><array><string>node</string><string>platform/api/dist/server.js</string></array></plist>");
    w("config/launchagents/com.gathering.messaging.plist",
        "<plist><array><string>node</string><string>platform/pulse/dist/service.js</string></array></plist>");
    w("config/launchagents/com.chorus.mcp.plist",
        "<plist><string>WorkingDirectory</string><string>platform/mcp-server</string></plist>");
    // TS packages with build scripts. chorus-api declares its own health URL.
    w("platform/api/package.json",
        r#"{"name":"chorus-api","scripts":{"build":"tsc"},"chorus":{"health":"http://localhost:3340/api/chorus/health"}}"#);
    w("platform/pulse/package.json", r#"{"name":"chorus-messaging","scripts":{"build":"tsc"}}"#);
    w("platform/mcp-server/package.json", r#"{"name":"chorus-mcp","scripts":{"build":"tsc"}}"#);
    // shared lib + a consumer that declares it as a file: dep.
    w("platform/chorus-sdk/package.json", r#"{"name":"chorus-sdk","scripts":{"build":"tsc"}}"#);
    w("directing/products/cards/package.json",
        r#"{"name":"cards","scripts":{"build":"tsc"},"dependencies":{"chorus-sdk":"file:../../../platform/chorus-sdk"}}"#);
    // a plain CLI package: build script, no plist, not a file: dep target.
    w("directing/clearing/package.json", r#"{"name":"clearing","scripts":{"build":"tsc"}}"#);
    root
}

#[test]
fn target_class_resolves_rust_service_when_a_plist_exists() {
    // #3132 — RustService iff a committed `com.chorus.<svc>` plist exists in the repo.
    // The bin may differ from the crate name (chorus-hooks → chorus-hook-shim).
    let root = werk_fixture("rust");
    match target_class_in("chorus-hooks", &root).unwrap() {
        TargetClass::RustService { svc, bin } => {
            assert_eq!(svc, "com.chorus.hooks");
            assert_eq!(bin, "chorus-hook-shim");
        }
        other => panic!("expected RustService, got {:?}", other),
    }
    match target_class_in("chorus-inject", &root).unwrap() {
        TargetClass::RustService { svc, bin } => {
            assert_eq!(svc, "com.chorus.inject");
            assert_eq!(bin, "chorus-inject");
        }
        other => panic!("expected RustService, got {:?}", other),
    }
}

#[test]
fn target_class_resolves_chorus_api_as_ts_service_with_declared_health() {
    // #3132 — chorus-api is found structurally: the committed plist references
    // platform/api → svc = com.chorus.api; dist = platform/api/dist; the health URL
    // comes from the package's OWN manifest, not a hardcoded map.
    let root = werk_fixture("api");
    match target_class_in("chorus-api", &root).unwrap() {
        TargetClass::TsService { svc, dist_dir_rel, smoke_url } => {
            assert_eq!(svc, "com.chorus.api");
            assert_eq!(dist_dir_rel, "platform/api/dist");
            assert!(smoke_url.contains("/api/chorus/health"), "self-declared health URL, got {}", smoke_url);
        }
        other => panic!("expected TsService, got {:?}", other),
    }
}

#[test]
fn target_class_resolves_pulse_as_ts_service_liveness_floor() {
    // THE #3132 CRUX on the deploy side: pulse (chorus-messaging) used to Err in
    // target_class ("unknown name") → it could never deploy through the verb → #3130
    // ran stale. Now it's found structurally (com.gathering.messaging references
    // platform/pulse), and with NO declared health it falls back to the liveness
    // floor (empty smoke_url) — which must NEVER block the deploy.
    let root = werk_fixture("pulse");
    match target_class_in("chorus-messaging", &root).unwrap() {
        TargetClass::TsService { svc, dist_dir_rel, smoke_url } => {
            assert_eq!(svc, "com.gathering.messaging");
            assert_eq!(dist_dir_rel, "platform/pulse/dist");
            assert!(smoke_url.is_empty(), "pulse declares no health → liveness floor (empty), got {}", smoke_url);
        }
        other => panic!("expected TsService for pulse, got {:?}", other),
    }
}

#[test]
fn target_class_resolves_werk_cli_verbs_to_cli_verb() {
    // werk-* binaries are CLI verbs (no LaunchAgent, no kickstart). Short-circuit.
    let root = werk_fixture("cli");
    for verb in &["werk-pull", "werk-commit", "werk-push", "werk-build", "werk-deploy", "werk-accept", "werk-demo"] {
        match target_class_in(verb, &root).unwrap() {
            TargetClass::CliVerb { bin } => assert_eq!(&bin, verb),
            other => panic!("expected CliVerb for {}, got {:?}", verb, other),
        }
    }
}

#[test]
fn target_class_resolves_chorus_sdk_as_shared_lib_and_cards_as_ts_package() {
    // #3132 — chorus-sdk is a file:-dep target → SharedLib (cascade). The consumer
    // `cards` has a build script but no plist → TsPackage (copy dist, no restart):
    // it closes the stale gap for a CLI that changed on its own. `clearing` likewise.
    let root = werk_fixture("sdk");
    match target_class_in("chorus-sdk", &root).unwrap() {
        TargetClass::SharedLib { name, lib_dist_rel } => {
            assert_eq!(name, "chorus-sdk");
            assert_eq!(lib_dist_rel, "platform/chorus-sdk/dist");
        }
        other => panic!("expected SharedLib, got {:?}", other),
    }
    match target_class_in("cards", &root).unwrap() {
        TargetClass::TsPackage { name, dist_dir_rel } => {
            assert_eq!(name, "cards");
            assert_eq!(dist_dir_rel, "directing/products/cards/dist");
        }
        other => panic!("expected TsPackage for cards, got {:?}", other),
    }
    match target_class_in("clearing", &root).unwrap() {
        TargetClass::TsPackage { name, .. } => assert_eq!(name, "clearing"),
        other => panic!("expected TsPackage for clearing, got {:?}", other),
    }
}

#[test]
fn target_class_errs_only_on_a_real_contract_break_not_an_allowlist_miss() {
    // #3132 — there is no allowlist to miss. Err happens ONLY when a name in the build
    // summary can't be located in the werk at all — a genuine build/deploy contract
    // break, not "I don't recognize this service."
    let root = werk_fixture("err");
    let err = target_class_in("not-a-real-unit", &root).unwrap_err();
    assert!(err.contains("contract break"), "should name a contract break, got {}", err);
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
