//! werk-deploy unit tests — pure helpers (no subprocess, no system mutation).
use werk_deploy::{
    branch_name, card_from_subject, cdhash_divergences, chorus_bin_install_cmd,
    crate_binaries_in, crate_binaries_with_service_in, demo_cdhashes, extract_running_cdhash,
    landed_commit_ok, live_main_flag, mcp_init_ready, parse_build_summary, parse_target, partition_lib_only, require_approval,
    running_verdict, RunVerdict,
    resolve_trace, service_for_crate, spine_args, target_class_in, TargetClass,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// #3517 inc1 — the one-sha invariant gate (PURE). landedCommit (threaded from werk-merge via
// werk.yml) must == the deployed origin/main HEAD, else RED: drift (main advanced between trigger
// and build) or empty (capture failed → "unknown"=RED, never silent-pass). Proven at the verify,
// not via a checkout (#2706 / detached-HEAD class).
#[test]
fn landed_commit_ok_true_only_on_exact_match() {
    let sha = "abc123def456abc123def456abc123def4560789";
    assert!(landed_commit_ok(sha, sha));
    assert!(landed_commit_ok(&format!("{sha}\n"), sha), "trims surrounding whitespace");
}

#[test]
fn landed_commit_ok_red_on_drift() {
    let a = "aaaa123def456abc123def456abc123def456078a";
    let b = "bbbb123def456abc123def456abc123def456078b";
    assert!(!landed_commit_ok(a, b), "main advanced between trigger and build = drift = RED");
}

#[test]
fn landed_commit_ok_red_on_empty_or_blank() {
    let sha = "abc123def456abc123def456abc123def4560789";
    assert!(!landed_commit_ok(sha, ""), "empty landedCommit = capture failed = unknown = RED");
    assert!(!landed_commit_ok("", sha), "empty deployed-commit = unresolvable = RED");
    assert!(!landed_commit_ok("   ", "   "), "whitespace-only both = RED, not a false match");
}

// #3517 inc2 — running_verdict (the 06-04 stale-daemon catcher). The 5 branches, on Option<bool>
// restarted (the start_epoch>=install_epoch math lives in the thin shell, not the pure core).
#[test]
fn running_verdict_cli_verb_is_ok_no_live_process() {
    // !daemon → Ok (one-shot CLI; inc1 install-verify is its proof). installed/restarted irrelevant.
    assert_eq!(running_verdict(false, "abc", "abc", None), RunVerdict::Ok);
    assert_eq!(running_verdict(false, "abc", "zzz", Some(false)), RunVerdict::Ok);
}
#[test]
fn running_verdict_daemon_mismatch_is_broken_install() {
    // installed != built → broken install, regardless of restart (subsumes restarted-but-wrong-file).
    assert_eq!(running_verdict(true, "abc", "zzz", Some(true)), RunVerdict::Mismatch);
}
#[test]
fn running_verdict_daemon_unresolvable_pid_is_unknown() {
    assert_eq!(running_verdict(true, "abc", "abc", None), RunVerdict::Unknown);
}
#[test]
fn running_verdict_daemon_restarted_onto_built_is_ok() {
    assert_eq!(running_verdict(true, "abc", "abc", Some(true)), RunVerdict::Ok);
}
#[test]
fn running_verdict_daemon_not_restarted_is_stale() {
    // started before the install = old inode = the 06-04 stale daemon → needs reload.
    assert_eq!(running_verdict(true, "abc", "abc", Some(false)), RunVerdict::Stale);
}

// #3317 — structural binary discovery (port of bash chorus-deploy's crate_binaries, the
// #3179/#3250 union rule): enumerate EVERY binary a crate emits = explicit [[bin]] names ∪
// src/bin/*.rs autobins, with the package name as the single-binary fallback. Kills the
// allowlist class — a crate that gains a binary nobody hand-listed shipped stale before.
#[test]
fn crate_binaries_in_unions_explicit_bins_and_src_bin_autobins() {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    // the #3250 drift: declares one [[bin]] but carries extra src/bin/*.rs — must return ALL 3
    let dir = std::env::temp_dir().join(format!("wd-cb-{}", n));
    fs::create_dir_all(dir.join("src/bin")).unwrap();
    fs::write(dir.join("Cargo.toml"),
        "[package]\nname = \"werk-accept\"\n\n[[bin]]\nname = \"werk-accept\"\npath = \"src/main.rs\"\n").unwrap();
    fs::write(dir.join("src/main.rs"), "fn main(){}").unwrap();
    fs::write(dir.join("src/bin/werk-do-more.rs"), "fn main(){}").unwrap();
    fs::write(dir.join("src/bin/werk-finalize.rs"), "fn main(){}").unwrap();
    let mut got = crate_binaries_in(&dir);
    got.sort();
    assert_eq!(got, vec!["werk-accept".to_string(), "werk-do-more".to_string(), "werk-finalize".to_string()]);
    fs::remove_dir_all(&dir).ok();

    // simple single-binary crate (no [[bin]], no src/bin, src/main.rs present)
    // → falls back to package name
    let d2 = std::env::temp_dir().join(format!("wd-cb2-{}", n));
    fs::create_dir_all(d2.join("src")).unwrap();
    fs::write(d2.join("Cargo.toml"), "[package]\nname = \"werk-merge\"\nversion = \"0.1.0\"\n").unwrap();
    fs::write(d2.join("src/main.rs"), "fn main(){}").unwrap();
    assert_eq!(crate_binaries_in(&d2), vec!["werk-merge".to_string()]);
    fs::remove_dir_all(&d2).ok();

    // #3431 — LIB-ONLY crate (src/lib.rs, no main.rs/[[bin]]/src/bin) emits NO
    // binaries: deploy must not hunt for a phantom target/release binary.
    let d4 = std::env::temp_dir().join(format!("wd-cb4-{}", n));
    fs::create_dir_all(d4.join("src")).unwrap();
    fs::write(d4.join("Cargo.toml"), "[package]\nname = \"werk-teardown\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"werk_teardown\"\npath = \"src/lib.rs\"\n").unwrap();
    fs::write(d4.join("src/lib.rs"), "pub fn x(){}").unwrap();
    assert_eq!(crate_binaries_in(&d4), Vec::<String>::new());
    fs::remove_dir_all(&d4).ok();

    // no Cargo.toml → basename fallback
    let d3 = std::env::temp_dir().join(format!("wd-cb3-{}", n));
    fs::create_dir_all(&d3).unwrap();
    assert_eq!(crate_binaries_in(&d3), vec![format!("wd-cb3-{}", n)]);
    fs::remove_dir_all(&d3).ok();
}

// #3316 — "what you demo is what ships", PROVEN not assumed. Prod builds from merged main
// (no copy); because build is a pure function of source, the prod cdhash must equal the
// demo'd cdhash. A divergence = source moved between demo and land (the integration trap).
#[test]
fn cdhash_divergences_flags_only_changed_crates_with_a_demo_baseline() {
    let demo = vec![("werk-deploy".to_string(), "aaa".to_string()), ("chorus-mcp".to_string(), "bbb".to_string())];
    // prod: matches werk-deploy, DIFFERS on chorus-mcp, and adds a crate with no demo baseline
    let prod = vec![
        ("werk-deploy".to_string(), "aaa".to_string()),
        ("chorus-mcp".to_string(), "XXX".to_string()),
        ("new-crate".to_string(), "ccc".to_string()),
    ];
    let d = cdhash_divergences(&demo, &prod);
    assert_eq!(d.len(), 1, "only chorus-mcp diverged; new-crate has no demo baseline to compare");
    assert_eq!(d[0], ("chorus-mcp".to_string(), "bbb".to_string(), "XXX".to_string())); // (crate, demo, prod)
    assert!(cdhash_divergences(&demo, &demo).is_empty(), "identical sets → no divergence");
}

#[test]
fn demo_cdhashes_reads_the_last_rebuilt_event_for_the_card() {
    let jsonl = "{\"event\":\"deploy.started\",\"card_id\":3316}\n\
                 {\"event\":\"rebuilt\",\"card_id\":3316,\"built\":\"werk-deploy=aaa,chorus-mcp=bbb\"}\n\
                 {\"event\":\"rebuilt\",\"card_id\":9999,\"built\":\"other=zzz\"}\n\
                 {\"event\":\"rebuilt\",\"card_id\":3316,\"built\":\"werk-deploy=ccc\"}";
    assert_eq!(demo_cdhashes(jsonl, 3316), vec![("werk-deploy".to_string(), "ccc".to_string())]); // last 3316
    assert!(demo_cdhashes(jsonl, 1234).is_empty(), "no demo for card → empty → never a false divergence");
}

// #3315 — ADR-037 deploy approval gate (mirrors werk-merge #3297 require_approval),
// adapted for deploy's --target axis: the gate fires ONLY on standalone --atomic to PROD
// (canonical). In-flow (no --atomic) NEVER blocks — the werk-land GO already
// authorized — which is the load-bearing safety: no double-gate deadlock of the pipeline.
#[test]
fn require_approval_gates_standalone_prod_only() {
    // explicit accepter authorizes either door (records who)
    assert_eq!(require_approval(true, "canonical", Some("jeff".to_string())).unwrap(), "jeff");
    assert_eq!(require_approval(false, "canonical", Some("jeff".to_string())).unwrap(), "jeff");
    // standalone --atomic to canonical (prod), no accepter → REFUSE (the gate)
    assert!(require_approval(true, "canonical", None).is_err());
    assert!(require_approval(true, "canonical", Some("  ".to_string())).is_err()); // empty == none
    // --atomic to the werk slot (local, reversible) → no gate
    assert_eq!(require_approval(true, "werk", None).unwrap(), "flow");
    // in-flow (no --atomic) → flow; never blocks, with or without ACCEPTER
    assert_eq!(require_approval(false, "canonical", None).unwrap(), "flow");
    assert_eq!(require_approval(false, "werk", None).unwrap(), "flow");
}

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
    // chorus-hooks mirrors the real crate: TWO [[bin]] entries (daemon + shim) so the
    // structural service-binary selection (#3317) is exercised, not just the fallback.
    w("platform/services/chorus-hooks/Cargo.toml",
        "[package]\nname=\"chorus-hooks\"\n\n[[bin]]\nname = \"chorus-hooks\"\npath = \"src/main.rs\"\n\n[[bin]]\nname = \"chorus-hook-shim\"\npath = \"src/shim.rs\"\n");
    // single-binary crates carry src/main.rs like their real counterparts — the
    // package-name fallback is gated on it (#3431 lib-only crates emit no bins).
    w("platform/services/chorus-inject/Cargo.toml", "[package]\nname=\"chorus-inject\"\n");
    w("platform/services/chorus-inject/src/main.rs", "fn main(){}");
    w("platform/services/werk-deploy/Cargo.toml", "[package]\nname=\"werk-deploy\"\n");
    w("platform/services/werk-deploy/src/main.rs", "fn main(){}");
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

// --- #3179/#3317: a crate can emit MORE THAN ONE binary; werk-deploy must deploy them
// all and verify the DAEMON binary — now discovered STRUCTURALLY, no hardcoded map. ---

#[test]
fn crate_binaries_with_service_flags_the_daemon_structurally() {
    // chorus-hooks emits TWO: the daemon (chorus-hooks/main.rs → com.chorus.hooks) and
    // the PreToolUse shim (chorus-hook-shim). Installing only one left the daemon on
    // stale code behind a green deploy (#3179 false-green). is_service=true marks the
    // binary the launchd service runs — the one named like the crate (cargo's package
    // binary), read from the crate's OWN Cargo.toml, not a crate→bin map.
    let root = werk_fixture("cbsvc");
    assert_eq!(
        crate_binaries_with_service_in(&root.join("platform/services/chorus-hooks"), "chorus-hooks"),
        vec![
            ("chorus-hooks".to_string(), true),
            ("chorus-hook-shim".to_string(), false),
        ]
    );
    // single-binary crates unchanged: just themselves, as the service binary (AC5 no-regression)
    assert_eq!(
        crate_binaries_with_service_in(&root.join("platform/services/chorus-inject"), "chorus-inject"),
        vec![("chorus-inject".to_string(), true)]
    );
    // no binary named like the crate → the FIRST discovered binary is the service
    // (a crate whose only binaries are renamed [[bin]] entries still gets a verify target).
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("wd-cbs-{}", n));
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("Cargo.toml"),
        "[package]\nname = \"renamed\"\n\n[[bin]]\nname = \"other-bin\"\npath = \"src/main.rs\"\n").unwrap();
    assert_eq!(
        crate_binaries_with_service_in(&d, "renamed"),
        vec![("other-bin".to_string(), true)]
    );
    fs::remove_dir_all(&d).ok();
}

#[test]
fn target_class_resolves_rust_service_when_a_plist_exists() {
    // #3132 — RustService iff a committed `com.chorus.<svc>` plist exists in the repo.
    // The bin may differ from the crate name (chorus-hooks → chorus-hook-shim).
    let root = werk_fixture("rust");
    match target_class_in("chorus-hooks", &root).unwrap() {
        TargetClass::RustService { svc, bin } => {
            assert_eq!(svc, "com.chorus.hooks");
            // #3179 — the service binary is the DAEMON (chorus-hooks/main.rs), the
            // binary com.chorus.hooks actually runs. Was wrongly "chorus-hook-shim"
            // (the PreToolUse shim), which made the deploy verify the wrong binary
            // and leave the daemon on stale code (merged≠live false-green).
            assert_eq!(bin, "chorus-hooks");
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

// #3317 — pure halves of the native canonical engine (bash chorus-deploy absorbed).

#[test]
fn card_from_subject_takes_the_first_card_number() {
    // bash parity: `git log -1 --pretty=%s | grep -oE '#[0-9]+' | head -1`
    assert_eq!(card_from_subject("silas: #3317 (#534)"), 3317);
    assert_eq!(card_from_subject("kade: acp #3294"), 3294);
    assert_eq!(card_from_subject("hand-edited commit, no card"), 0);
    assert_eq!(card_from_subject("trailing hash # then #42"), 42);
}

#[test]
fn live_main_flag_true_only_when_neither_behind_nor_ahead() {
    // #3270 — deploy.completed states the merged≠live truth: live_main=true only when
    // the deployed HEAD IS origin/main.
    assert_eq!(live_main_flag(0, 0), "true");
    assert_eq!(live_main_flag(1, 0), "false");
    assert_eq!(live_main_flag(0, 1), "false");
}

#[test]
fn mcp_init_ready_requires_protocol_version_inside_a_result() {
    // #2997 port — success = the SSE body carries protocolVersion in a result field:
    // the unambiguous signal the MCP middleware answered as a valid server.
    assert!(mcp_init_ready("data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":\"2024-11-05\"}}\n"));
    // an error wrapper is NOT ready
    assert!(!mcp_init_ready("data: {\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32000}}\n"));
    // a generic 200 from the wrong handler is NOT ready
    assert!(!mcp_init_ready("<html>ok</html>"));
    // protocolVersion outside a data: line is NOT ready (not an SSE answer)
    assert!(!mcp_init_ready("{\"result\":{\"protocolVersion\":\"x\"}}"));
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

/// #3192 — chorus-bin-install is a script in platform/scripts, NOT a deployed binary,
/// so it is absent from the werk PATH (~/.chorus/bin + role bin-slots). Spawning it by
/// bare name ENOENTs and breaks every canonical deploy. The resolver must return an
/// ABSOLUTE path, sourced from canonical (main) first so a card's branch can't redefine
/// how installs happen.
fn cbi_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("wd-cbi-{}-{}-{}", tag, std::process::id(), nanos))
}

#[test]
fn chorus_bin_install_resolves_absolute_canonical_first() {
    std::env::remove_var("CHORUS_BIN_INSTALL");
    let home = cbi_dir("home");
    let werk = cbi_dir("werk");
    // Both roots carry the script; canonical (home) must win.
    let home_script = home.join("platform/scripts/chorus-bin-install");
    let werk_script = werk.join("platform/scripts/chorus-bin-install");
    fs::create_dir_all(home_script.parent().unwrap()).unwrap();
    fs::create_dir_all(werk_script.parent().unwrap()).unwrap();
    fs::write(&home_script, "#!/bin/sh\n").unwrap();
    fs::write(&werk_script, "#!/bin/sh\n").unwrap();

    let got = chorus_bin_install_cmd(&home, werk.to_str().unwrap());
    assert_eq!(got, home_script.to_string_lossy(), "canonical platform/scripts wins, absolute");
}

#[test]
fn chorus_bin_install_falls_back_to_bare_when_absent() {
    std::env::remove_var("CHORUS_BIN_INSTALL");
    // Neither root has the script → bare name, so PATH-shimmed e2e tests still resolve.
    let home = cbi_dir("h-absent");
    let werk = cbi_dir("w-absent");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&werk).unwrap();
    let got = chorus_bin_install_cmd(&home, werk.to_str().unwrap());
    assert_eq!(got, "chorus-bin-install", "bare-name fallback preserves the test shim path");
}

// --- #3215: the demo-env lifecycle event contract -----------------------------
// AC1 of #3215 wants env.up/env.down on the SPINE as STRUCTURED events carrying
// the RIGHT ELEMENTS (role, card, trace + per-service svc/port/result). spine_args
// is the pure shape under emit_spine, so it pins the structured-shape + right-
// elements guarantees here; the "reliably written every transition" guarantee is
// the live round-trip (AC5). This guards against silently dropping a field.

#[test]
fn spine_args_lifecycle_carries_role_card_trace_in_order() {
    let a = spine_args("env.up.started", "silas", 3215, "tr-abc", &[]);
    assert_eq!(a[0], "env.up.started", "event name first");
    assert_eq!(a[1], "silas", "role second");
    assert_eq!(a[2], "card=3215", "card= third");
    assert_eq!(a[3], "trace=tr-abc", "trace= fourth");
    assert_eq!(a.len(), 4, "no extras → exactly the four core elements");
}

#[test]
fn spine_args_smoked_carries_svc_port_result_elements() {
    // env.up.smoked is the per-service truth Borg pairs against env.down.stopped.
    let a = spine_args(
        "env.up.smoked", "silas", 3215, "tr-abc",
        &[("svc", "chorus-api"), ("port", "3343"), ("result", "ok")],
    );
    assert_eq!(a[0], "env.up.smoked");
    assert!(a.contains(&"svc=chorus-api".to_string()), "svc element present");
    assert!(a.contains(&"port=3343".to_string()), "port element present");
    assert!(a.contains(&"result=ok".to_string()), "smoke result present");
    // a leak is a fail result, not an absent event — the element must survive.
    let f = spine_args("env.up.smoked", "silas", 3215, "tr", &[("result", "fail")]);
    assert!(f.contains(&"result=fail".to_string()), "fail result is emitted, not dropped");
}

// #3239 — env-up must target the card under test, not the first/stale werk.
#[test]
fn werk_root_for_uses_the_card_werk_and_refuses_without_a_card() {
    // With a card → that exact card's werk.
    let r = werk_deploy::demo_env::werk_root_for("silas", Some(3239), "/tmp/wb").unwrap();
    assert_eq!(r, "/tmp/wb/silas-3239");
    // Without a card → REFUSE (the old code picked the first <role>-* dir, standing up an
    // arbitrary/stale werk — proven live: kade/3236 env-up ran in kade-3224).
    let e = werk_deploy::demo_env::werk_root_for("silas", None, "/tmp/wb").unwrap_err();
    assert!(
        e.contains("card_id") && e.to_lowercase().contains("refus"),
        "no-card env-up must refuse naming card_id, got: {e}"
    );
}

// #3243 — deploy_canonical must also deploy the TS services (chorus-mcp at
// platform/mcp-server, chorus-api at platform/api), which live OUTSIDE platform/services/
// so changed_service_crates misses them (the merged≠live hole that bit #3239/#3241).
#[test]
fn changed_ts_services_detects_mcp_and_api_distinct_from_rust_crates() {
    let diff = "platform/mcp-server/src/server.ts\n\
                platform/api/src/server.ts\n\
                platform/services/werk-deploy/src/lib.rs\n\
                README.md\n";
    let ts = werk_deploy::changed_ts_services(diff);
    assert!(ts.contains(&"chorus-mcp".to_string()), "platform/mcp-server → chorus-mcp, got {ts:?}");
    assert!(ts.contains(&"chorus-api".to_string()), "platform/api → chorus-api, got {ts:?}");
    assert_eq!(ts.len(), 2, "only the two TS services, deduped: {ts:?}");
    // the rust-crate detector is unchanged and disjoint — it sees only the platform/services/ crate
    assert_eq!(werk_deploy::changed_service_crates(diff), vec!["werk-deploy".to_string()]);
    // no false positive on a docs-only diff
    assert!(werk_deploy::changed_ts_services("README.md\ndesigning/docs/x.html\n").is_empty());
    // dedup: many touched files in one TS service collapse to one deploy target
    let multi = "platform/mcp-server/src/a.ts\nplatform/mcp-server/src/b.ts\nplatform/mcp-server/package.json\n";
    assert_eq!(werk_deploy::changed_ts_services(multi), vec!["chorus-mcp".to_string()]);
}

// #3320 — deploying chorus-mcp FROM chorus-mcp must detach: the inline kickstart
// kills the invoking daemon, so the caller's MCP response drops (the transport-drop
// class — bit chorus_werk_land live 2026-06-10). Detection is a pure half.
#[test]
fn self_deploy_detach_fires_only_for_mcp_invoked_by_mcp() {
    use werk_deploy::self_deploy_detach_needed as need;
    assert!(need("chorus-mcp", Some("chorus-mcp"), false), "mcp-from-mcp detaches");
    assert!(!need("chorus-api", Some("chorus-mcp"), false), "non-self unit deploys inline");
    assert!(!need("chorus-mcp", None, false), "CLI / agent-state invoker deploys inline");
    assert!(!need("chorus-mcp", Some("chorus-api"), false), "other invokers deploy inline");
    assert!(!need("werk-deploy", Some("chorus-mcp"), false), "rust crates never detach");
    assert!(
        !need("chorus-mcp", Some("chorus-mcp"), true),
        "the detached continuation must NOT re-detach (no respawn loop)"
    );
}

// #3320 — the detached continuation is a crate-mode redeploy of the ONE unit,
// rollback flag preserved. Same surface as `werk-deploy crate <name> [--rollback]`.
#[test]
fn detach_argv_is_a_crate_mode_redeploy_of_the_one_unit() {
    assert_eq!(werk_deploy::detach_argv("chorus-mcp", false), vec!["crate", "chorus-mcp"]);
    assert_eq!(
        werk_deploy::detach_argv("chorus-mcp", true),
        vec!["crate", "chorus-mcp", "--rollback"]
    );
}

// #3320 — a detach ack must never be mistaken for a completed deploy: callers skip
// their deploy.completed emission for an ack (the child emits the real one).
#[test]
fn detached_ack_is_distinguishable_from_completed_deploys() {
    assert!(werk_deploy::is_detached_ack(
        "chorus-mcp deploy detached pid=123 — survives its own kickstart; poll spine deploy.completed trace=t"
    ));
    assert!(!werk_deploy::is_detached_ack("chorus-mcp deployed"));
    assert!(!werk_deploy::is_detached_ack("chorus-mcp deployed target=canonical"));
    assert!(!werk_deploy::is_detached_ack("chorus-mcp rolled back"));
}

// #3352 — pulse + clearing join TS-service deploy discovery (the merged-but-stale
// class: #3357's land said deploy-success while running pulse stayed stale).
#[test]
fn changed_ts_services_detects_pulse_and_clearing() {
    let diff = "platform/pulse/src/session-registry.ts\ndirecting/clearing/src/server.ts\nplatform/api/src/server.ts\n";
    let ts = werk_deploy::changed_ts_services(diff);
    assert_eq!(ts, vec!["pulse".to_string(), "clearing".to_string(), "chorus-api".to_string()]); // diff-line order
}

#[test]
fn changed_ts_services_dedupes_pulse() {
    let diff = "platform/pulse/src/a.ts\nplatform/pulse/src/b.ts\n";
    assert_eq!(werk_deploy::changed_ts_services(diff), vec!["pulse".to_string()]);
}

// #3375 — the TS smoke false-negative: wait_for_api_healthy required the literal
// body "status":"healthy" (chorus-api's shape). #3352 added pulse + clearing to
// the same smoke, but both answer {"status":"ok",...} — never matched, so every
// deploy spun the full 30s and refused "health timeout" against a service
// answering in 0.17s (blocked #3366's land 3x on 2026-06-12). The predicate is
// now pure + shape-tolerant: healthy|ok on an anchored "status" key, nothing else.
#[test]
fn health_body_accepts_clearing_ok_shape() {
    // clearing's literal /health body, 2026-06-12
    assert!(werk_deploy::health_body_ok(r#"{"status":"ok","port":3470}"#));
}

#[test]
fn health_body_accepts_pulse_ok_shape() {
    assert!(werk_deploy::health_body_ok(
        r#"{"status":"ok","port":3475,"total":17015,"pending":17013}"#
    ));
}

#[test]
fn health_body_accepts_chorus_api_healthy_shape() {
    assert!(werk_deploy::health_body_ok(r#"{"status":"healthy","checks":[]}"#));
}

#[test]
fn health_body_rejects_unhealthy_and_noise() {
    // degraded/down must NOT pass
    assert!(!werk_deploy::health_body_ok(r#"{"status":"degraded","failures":6}"#));
    assert!(!werk_deploy::health_body_ok(r#"{"status":"down"}"#));
    // an error page or empty body must not pass
    assert!(!werk_deploy::health_body_ok("502 Bad Gateway"));
    assert!(!werk_deploy::health_body_ok(""));
    // "ok"/"healthy" appearing OUTSIDE the status key must not pass (substring trap,
    // the 6.401ms class from #3369)
    assert!(!werk_deploy::health_body_ok(r#"{"status":"error","note":"last ok 2h ago"}"#));
}

#[test]
fn health_timeout_err_names_url_and_last_body() {
    // #3375 AC3 — a real timeout refusal must say WHAT it polled and what it last
    // saw, never a bare "30s timeout".
    let e = werk_deploy::health_timeout_err(
        "http://localhost:3470/health",
        std::time::Duration::from_secs(30),
        Some(r#"{"status":"degraded","failures":6}"#),
        Some(0),
    );
    assert!(e.contains("http://localhost:3470/health"), "names the polled URL: {}", e);
    assert!(e.contains("degraded"), "carries the last observed body: {}", e);
    assert!(e.contains("curl exit: 0"), "carries the last curl exit: {}", e);
    // connection refused: curl exit 7, no body — distinguishable from empty-200
    let e2 = werk_deploy::health_timeout_err("http://x/health", std::time::Duration::from_secs(30), None, Some(7));
    assert!(e2.contains("no response"), "no-answer case named: {}", e2);
    assert!(e2.contains("curl exit: 7"), "connection-refused exit visible: {}", e2);
}


// #3376 — stale role-slot verb binaries. The #3101 wrapper routes CHORUS_ROLE to
// $WERK_<ROLE>_BIN/<verb> FIRST; nothing refreshed those slots on canonical
// deploy. Tests drive slots_to_refresh_with an injected is_variant so fixtures
// model the REAL property (every werk is a full checkout — presence of a crate
// dir discriminates nothing; only the branch DIFF does). The first cut's
// crate-dir-exists guard was a production no-op caught by cold-eyes — these
// fixtures exist so that false-green cannot recur.

fn full_checkout_werk(base: &std::path::Path, name: &str) {
    // models reality: EVERY werk contains EVERY crate dir
    for c in ["werk-demo", "werk-merge", "werk-deploy", "owl-api"] {
        std::fs::create_dir_all(base.join(name).join("platform/services").join(c)).unwrap();
    }
}

#[test]
fn slots_to_refresh_includes_stale_slot_when_no_werk_modifies_the_crate() {
    let base = std::env::temp_dir().join(format!("s3376-a-{}", std::process::id()));
    std::fs::create_dir_all(base.join("kade-bin")).unwrap();
    std::fs::write(base.join("kade-bin/werk-demo"), b"old").unwrap();
    full_checkout_werk(&base, "kade-9999"); // live werk, full checkout, MODIFIES NOTHING here
    let r = werk_deploy::slots_to_refresh_with(
        base.to_str().unwrap(), "werk-demo", "werk-demo",
        &|_werk, _crate| false, // branch diff touches nothing
    );
    assert_eq!(r, vec!["kade".to_string()],
        "full checkout present but crate NOT modified → slot refreshes (the no-op guard regression)");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn slots_to_refresh_skips_role_whose_live_werk_modifies_the_crate() {
    let base = std::env::temp_dir().join(format!("s3376-b-{}", std::process::id()));
    std::fs::create_dir_all(base.join("wren-bin")).unwrap();
    std::fs::write(base.join("wren-bin/werk-demo"), b"variant").unwrap();
    full_checkout_werk(&base, "wren-9999");
    let r = werk_deploy::slots_to_refresh_with(
        base.to_str().unwrap(), "werk-demo", "werk-demo",
        &|werk, krate| werk.ends_with("wren-9999") && krate == "werk-demo",
    );
    assert!(r.is_empty(), "live werk MODIFYING the crate owns its slot; got {:?}", r);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn slots_to_refresh_skips_roles_without_slot_binary() {
    let base = std::env::temp_dir().join(format!("s3376-c-{}", std::process::id()));
    std::fs::create_dir_all(base.join("silas-bin")).unwrap();
    let r = werk_deploy::slots_to_refresh_with(
        base.to_str().unwrap(), "werk-demo", "werk-demo", &|_, _| false);
    assert!(r.is_empty(), "no slot binary → nothing to refresh; got {:?}", r);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn slots_to_refresh_werk_modifying_other_crate_does_not_protect() {
    let base = std::env::temp_dir().join(format!("s3376-d-{}", std::process::id()));
    std::fs::create_dir_all(base.join("silas-bin")).unwrap();
    std::fs::write(base.join("silas-bin/werk-demo"), b"old").unwrap();
    full_checkout_werk(&base, "silas-8888");
    let r = werk_deploy::slots_to_refresh_with(
        base.to_str().unwrap(), "werk-demo", "werk-demo",
        &|werk, krate| werk.ends_with("silas-8888") && krate == "werk-merge", // modifies a DIFFERENT crate
    );
    assert_eq!(r, vec!["silas".to_string()]);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn werk_diff_touches_crate_detects_modification_via_real_git() {
    // the production discriminator: branch diff vs origin/main touches the crate.
    let base = std::env::temp_dir().join(format!("s3376-g-{}", std::process::id()));
    let werk = base.join("repo");
    std::fs::create_dir_all(werk.join("platform/services/werk-demo")).unwrap();
    let git = |args: &[&str]| {
        assert!(std::process::Command::new("git").arg("-C").arg(&werk)
            .envs([("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),
                   ("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")])
            .args(args).status().unwrap().success());
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(werk.join("platform/services/werk-demo/f.rs"), b"a").unwrap();
    git(&["add", "."]); git(&["commit", "-q", "-m", "base"]);
    // simulate origin/main at base
    git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    assert!(!werk_deploy::werk_diff_touches_crate(werk.to_str().unwrap(), "werk-demo"),
        "no modification → not a variant");
    std::fs::write(werk.join("platform/services/werk-demo/f.rs"), b"changed").unwrap();
    git(&["add", "."]); git(&["commit", "-q", "-m", "variant"]);
    assert!(werk_deploy::werk_diff_touches_crate(werk.to_str().unwrap(), "werk-demo"),
        "branch modifies the crate → variant");
    assert!(!werk_deploy::werk_diff_touches_crate(werk.to_str().unwrap(), "werk-merge"),
        "other crates not protected by this werk");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn refresh_executor_makes_slot_content_equal_canonical() {
    // #3376 AC3 — post-refresh, the role-resolved binary content == canonical.
    let base = std::env::temp_dir().join(format!("s3376-e-{}", std::process::id()));
    let home = base.join("home");
    std::fs::create_dir_all(home.join("ops/logs")).unwrap();
    std::fs::create_dir_all(base.join("kade-bin")).unwrap();
    std::fs::write(base.join("kade-bin/werk-x"), b"OLD").unwrap();
    let canonical = base.join("werk-x-bin");
    std::fs::write(&canonical, b"NEW-CANONICAL").unwrap();
    std::env::set_var("CHORUS_WERK_BASE", base.to_str().unwrap());
    werk_deploy::refresh_role_slots_for_test(&home, "kade", 9376, "t", "werk-x", "werk-x", canonical.to_str().unwrap());
    std::env::remove_var("CHORUS_WERK_BASE");
    let got = std::fs::read(base.join("kade-bin/werk-x")).unwrap();
    assert_eq!(got, b"NEW-CANONICAL", "slot content must equal canonical after refresh");
    let w = std::fs::read_to_string(home.join("ops/logs/werk-deploy.jsonl")).unwrap_or_default();
    assert!(w.contains("slot.refreshed"), "refresh witnessed; got: {}", w);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn refresh_failure_retires_stale_slot_aside_so_canonical_serves() {
    // #3376 cold-eyes probe 3 — a FAILED refresh must not leave the stale slot
    // executable (wrapper would keep routing to last week's semantics). The slot
    // is renamed aside (.stale) so fall-through serves canonical; witnessed.
    let base = std::env::temp_dir().join(format!("s3376-f-{}", std::process::id()));
    let home = base.join("home");
    std::fs::create_dir_all(home.join("ops/logs")).unwrap();
    std::fs::create_dir_all(base.join("kade-bin")).unwrap();
    std::fs::write(base.join("kade-bin/werk-x"), b"STALE").unwrap();
    std::env::set_var("CHORUS_WERK_BASE", base.to_str().unwrap());
    // canonical path does NOT exist → fs::copy fails → retire-aside branch
    let missing = base.join("no-such-canonical");
    werk_deploy::refresh_role_slots_for_test(&home, "kade", 9376, "t", "werk-x", "werk-x", missing.to_str().unwrap());
    std::env::remove_var("CHORUS_WERK_BASE");
    assert!(!base.join("kade-bin/werk-x").exists(), "stale slot must be GONE from the exec name");
    assert_eq!(std::fs::read(base.join("kade-bin/werk-x.stale")).unwrap(), b"STALE", "retired aside, not deleted");
    let w = std::fs::read_to_string(home.join("ops/logs/werk-deploy.jsonl")).unwrap_or_default();
    assert!(w.contains("slot.retired_stale"), "retirement witnessed; got: {}", w);
    let _ = std::fs::remove_dir_all(&base);
}

// #3638 — a LIB-ONLY changed crate must be PARTITIONED OUT of the canonical deploy
// list, not sent to deploy_crate_canonical where the werk-* CliVerb short-circuit
// demands a binary that cannot exist (the exact #3638 land failure: werk-teardown
// changed → deploy-canonical died with "binary missing for werk-teardown").
#[test]
fn partition_lib_only_splits_teardown_from_deployables() {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("wd-plo-{}", n));
    // lib-only crate (the werk-teardown shape)
    let lib = root.join("platform/services/werk-teardown");
    fs::create_dir_all(lib.join("src")).unwrap();
    fs::write(lib.join("Cargo.toml"),
        "[package]\nname = \"werk-teardown\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"werk_teardown\"\npath = \"src/lib.rs\"\n").unwrap();
    fs::write(lib.join("src/lib.rs"), "").unwrap();
    // normal single-binary crate
    let bin = root.join("platform/services/werk-commit");
    fs::create_dir_all(bin.join("src")).unwrap();
    fs::write(bin.join("Cargo.toml"), "[package]\nname = \"werk-commit\"\nversion = \"0.1.0\"\n").unwrap();
    fs::write(bin.join("src/main.rs"), "fn main(){}").unwrap();

    let crates = vec!["werk-teardown".to_string(), "werk-commit".to_string()];
    let (deployable, lib_only) = partition_lib_only(&root, &crates);
    assert_eq!(deployable, vec!["werk-commit".to_string()]);
    assert_eq!(lib_only, vec!["werk-teardown".to_string()]);

    // a crate dir that doesn't exist stays deployable — deploy's own error names it
    // honestly rather than a silent skip hiding a real miss
    let (d2, l2) = partition_lib_only(&root, &vec!["no-such-crate".to_string()]);
    assert_eq!(d2, vec!["no-such-crate".to_string()]);
    assert!(l2.is_empty());
    fs::remove_dir_all(&root).ok();
}
