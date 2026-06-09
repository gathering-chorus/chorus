//! werk-build unit tests — pure helpers (no subprocess, no fs side effects).
use werk_build::{
    branch_name, discover_build_units_in_tree, ensure_node_modules, extract_cdhash, extract_file_deps,
    has_build_script, jsonl_line, lib_source_changed, parse_atomic, parse_target, pkg_name, resolve_trace, spine_args, BuildUnit,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Unique temp dir for a fixture (no tempfile dep; std-only, per ADR-032 §1).
fn fixture(tag: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("wb-tree-{}-{}-{}", tag, std::process::id(), nanos));
    fs::create_dir_all(&p).unwrap();
    p
}

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, body).unwrap();
}

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("silas", 3061), "silas/3061");
}

// #3308 — werk-build --atomic: the seven-verb free-group contract flag. Detected
// anywhere in argv (mirrors werk-commit's `--atomic`); absence = false. The verb is
// already standalone-explicit (card/role/--target/--only), so --atomic is the uniform
// contract marker, not a new build path — but it MUST be parsed so it doesn't fall
// through to positional and break the card-id parse.
// #3309 — harden the --target seam (shipped untested, the #3306 lesson): werk vs
// canonical, default werk, bad value refused, position-agnostic.
#[test]
fn parse_target_selects_werk_or_canonical_defaults_werk() {
    assert_eq!(parse_target(&[]).unwrap(), "werk");
    assert_eq!(parse_target(&["3309".to_string(), "silas".to_string()]).unwrap(), "werk");
    assert_eq!(parse_target(&["--target".to_string(), "canonical".to_string()]).unwrap(), "canonical");
    assert_eq!(parse_target(&["3309".to_string(), "silas".to_string(), "--target".to_string(), "werk".to_string()]).unwrap(), "werk");
    assert!(parse_target(&["--target".to_string(), "bogus".to_string()]).is_err());
}

#[test]
fn parse_atomic_detects_flag_anywhere_in_argv() {
    assert!(parse_atomic(&["3308".to_string(), "silas".to_string(), "--atomic".to_string()]));
    assert!(parse_atomic(&["--atomic".to_string(), "3308".to_string()]));
    assert!(!parse_atomic(&["3308".to_string(), "silas".to_string()]));
    assert!(!parse_atomic(&["3308".to_string(), "silas".to_string(), "--target".to_string(), "werk".to_string()]));
}

#[test]
fn build_unit_name_covers_shared_lib() {
    assert_eq!(BuildUnit::SharedLib("chorus-sdk".to_string()).name(), "chorus-sdk");
}

#[test]
fn extract_file_deps_pulls_name_and_file_target() {
    // #3126 — consumer discovery is graph-driven: a consumer declares the lib as a
    // `file:` dependency. extract_file_deps yields (dep_name, file_target) pairs so
    // the bundler set is DISCOVERED, never a hardcoded list that rots.
    let pkg = r#"{
        "name": "cards",
        "dependencies": { "chorus-sdk": "file:../../../platform/chorus-sdk", "express": "^4.0.0" }
    }"#;
    let deps = extract_file_deps(pkg);
    assert_eq!(deps, vec![("chorus-sdk".to_string(), "../../../platform/chorus-sdk".to_string())]);
}

#[test]
fn extract_file_deps_empty_when_no_file_deps() {
    let pkg = r#"{ "name": "x", "dependencies": { "express": "^4.0.0" } }"#;
    assert!(extract_file_deps(pkg).is_empty());
}

#[test]
fn pkg_name_reads_name_field() {
    assert_eq!(pkg_name(r#"{ "name": "cards", "version": "1.0.0" }"#), Some("cards".to_string()));
    assert_eq!(pkg_name(r#"{ "version": "1.0.0" }"#), None);
}

#[test]
fn build_unit_name_returns_inner_string_regardless_of_kind() {
    assert_eq!(BuildUnit::RustCrate("chorus-hooks".to_string()).name(), "chorus-hooks");
    assert_eq!(BuildUnit::TsService("chorus-api".to_string()).name(), "chorus-api");
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

// --- #3132: structural enumeration (build everything by structure, no allowlist) ---

#[test]
fn has_build_script_detects_build_in_scripts() {
    assert!(has_build_script(r#"{"name":"x","scripts":{"build":"tsc","test":"jest"}}"#));
    assert!(has_build_script(r#"{"name":"x","scripts":{"start":"node .","build":"tsc"}}"#));
    // no build script -> false (a runtime-only or lib package without a build step).
    assert!(!has_build_script(r#"{"name":"x","scripts":{"start":"node ."}}"#));
    assert!(!has_build_script(r#"{"name":"x","dependencies":{"express":"^4"}}"#));
}

#[test]
fn discover_in_tree_finds_pulse_and_every_service_not_just_an_allowlist() {
    // THE #3132 CRUX: platform/pulse matched no hardcoded rule, so the old
    // diff-discovery built ZERO units for a pulse change -> #3130 merged green and
    // ran stale. Structural enumeration finds it because it IS a build-script
    // package — no rule to add, no list to forget.
    let root = fixture("pulse");
    write(&root, "platform/services/werk-build/Cargo.toml", "[package]\nname=\"werk-build\"\n");
    write(&root, "platform/services/chorus-hooks/Cargo.toml", "[package]\nname=\"chorus-hooks\"\n");
    write(&root, "platform/api/package.json", r#"{"name":"chorus-api","scripts":{"build":"tsc"}}"#);
    write(&root, "platform/pulse/package.json", r#"{"name":"chorus-messaging","scripts":{"build":"tsc"}}"#);
    write(&root, "platform/mcp-server/package.json", r#"{"name":"chorus-mcp","scripts":{"build":"tsc"}}"#);
    // a docs dir + a no-build package are NOT build units.
    write(&root, "designing/docs/x.html", "<html>");
    write(&root, "platform/configonly/package.json", r#"{"name":"configonly","scripts":{"start":"node ."}}"#);

    let units = discover_build_units_in_tree(&root);
    assert!(units.contains(&BuildUnit::TsService("chorus-messaging".to_string())),
        "pulse (chorus-messaging) MUST be discovered structurally — this is the #3130/#3132 stale gap");
    assert!(units.contains(&BuildUnit::TsService("chorus-api".to_string())));
    assert!(units.contains(&BuildUnit::TsService("chorus-mcp".to_string())));
    assert!(units.contains(&BuildUnit::RustCrate("werk-build".to_string())));
    assert!(units.contains(&BuildUnit::RustCrate("chorus-hooks".to_string())));
    // no-build package and docs are absent.
    assert!(!units.iter().any(|u| u.name() == "configonly"));
    assert_eq!(units.len(), 5, "exactly the 2 crates + 3 build-script services");
}

#[test]
fn discover_in_tree_classifies_shared_lib_by_file_dep_target() {
    // SharedLib vs TsService is STRUCTURAL: a package that is the target of a `file:`
    // dependency is a library (its consumers cascade-rebuild on deploy); one that
    // isn't is a service. No hardcoded "chorus-sdk is special" rule.
    let root = fixture("sharedlib");
    write(&root, "platform/chorus-sdk/package.json", r#"{"name":"chorus-sdk","scripts":{"build":"tsc"}}"#);
    write(&root, "platform/pulse/package.json", r#"{"name":"chorus-messaging","scripts":{"build":"tsc"}}"#);
    // a consumer (the cards CLI) declares chorus-sdk as a file: dep -> sdk is a lib.
    write(&root, "directing/products/cards/package.json",
        r#"{"name":"cards","scripts":{"build":"tsc"},"dependencies":{"chorus-sdk":"file:../../../platform/chorus-sdk"}}"#);

    let units = discover_build_units_in_tree(&root);
    assert!(units.contains(&BuildUnit::SharedLib("chorus-sdk".to_string())),
        "chorus-sdk is a file:-dep target -> SharedLib (cascade), discovered not hardcoded");
    assert!(units.contains(&BuildUnit::TsService("chorus-messaging".to_string())),
        "pulse is no one's file: dep -> a service, not a lib");
    assert!(units.contains(&BuildUnit::TsService("cards".to_string())),
        "the consumer itself is also a build unit (built directly, not only via cascade)");
}

#[test]
fn discover_in_tree_skips_node_modules() {
    // A build-script package vendored under node_modules must NOT become a build unit.
    let root = fixture("nm");
    write(&root, "platform/pulse/package.json", r#"{"name":"chorus-messaging","scripts":{"build":"tsc"}}"#);
    write(&root, "platform/pulse/node_modules/dep/package.json", r#"{"name":"dep","scripts":{"build":"tsc"}}"#);
    let units = discover_build_units_in_tree(&root);
    assert!(units.contains(&BuildUnit::TsService("chorus-messaging".to_string())));
    assert!(!units.iter().any(|u| u.name() == "dep"), "node_modules packages are not build units");
}

// --- #3168: gate the consumer cascade on the lib's SOURCE actually changing ---

#[test]
fn cascade_skipped_when_lib_untouched() {
    // Wren #3147 (the bite): a chorus-api-only card. chorus-sdk source is untouched,
    // so the consumer cascade must NOT fire. Before #3168 it fired unconditionally and
    // the cards-consumer rebuild died on hoisted-tsc -> false "breaking change, refusing"
    // that blocked every unrelated card team-wide.
    let changed = vec![
        "platform/api/src/lib/fts-query.ts".to_string(),
        "designing/docs/chorus-search-tobe.svg".to_string(),
    ];
    assert!(!lib_source_changed(&changed, "platform/chorus-sdk"));
}

#[test]
fn cascade_runs_when_lib_source_changed() {
    // A real chorus-sdk change DOES need the cascade — consumers must rebuild to catch
    // a genuine breaking change.
    let changed = vec![
        "platform/chorus-sdk/src/emit.ts".to_string(),
        "platform/api/src/server.ts".to_string(),
    ];
    assert!(lib_source_changed(&changed, "platform/chorus-sdk"));
}

#[test]
fn cascade_skipped_when_only_lib_dist_changed() {
    // dist/ is generated output, not source — a dist-only diff is not a source change,
    // so it must not trigger the cascade (defends against a future where dist is tracked).
    let changed = vec!["platform/chorus-sdk/dist/emit.js".to_string()];
    assert!(!lib_source_changed(&changed, "platform/chorus-sdk"));
}

// --- #3166: build.failed reaches the spine (not just the jsonl witness) ---

#[test]
fn spine_args_builds_chorus_log_argv_for_build_failed() {
    // build.failed must be Loki-queryable + counted in the #3165 rollup, so it goes
    // to the ONE spine via chorus-log. spine_args is the pure arg-builder emit_spine
    // shells to chorus-log with — mirrors werk-pull #3161.
    let a = spine_args("build.failed", "silas", 3166, "tr1", &[("disposition", "fail"), ("kind", "rust"), ("name", "chorus-hooks")]);
    assert_eq!(a[0], "build.failed");
    assert_eq!(a[1], "silas");
    assert!(a.contains(&"card=3166".to_string()), "carries card");
    assert!(a.contains(&"trace=tr1".to_string()), "carries trace");
    assert!(a.contains(&"disposition=fail".to_string()), "rollup keys on disposition (#3165)");
    assert!(a.contains(&"kind=rust".to_string()));
    assert!(a.contains(&"name=chorus-hooks".to_string()));
}

// --- #3169: build preamble ensures werk node_modules mirror canonical (symlink-replace partials) ---

#[test]
fn ensure_node_modules_replaces_partial_with_canonical_symlink() {
    // The bug: a werk package's node_modules is a stale PARTIAL real dir (missing
    // @types/node), so in-werk tsc fails. ensure must replace it with a symlink to
    // canonical's COMPLETE tree, so @types resolves — the keystone for #3174/#3171
    // (build mcp-server/chorus-hooks in-werk instead of hand-deploying).
    let root = fixture("ensure-nm");
    let home = root.join("chorus");
    let werk = root.join("chorus-werk/silas-3169");
    // canonical: complete node_modules (has @types/node)
    write(&home, "platform/api/package.json", r#"{"name":"chorus-api","scripts":{"build":"tsc"}}"#);
    fs::create_dir_all(home.join("platform/api/node_modules/@types/node")).unwrap();
    // werk: same package, but a PARTIAL real node_modules (no @types/node)
    write(&werk, "platform/api/package.json", r#"{"name":"chorus-api","scripts":{"build":"tsc"}}"#);
    fs::create_dir_all(werk.join("platform/api/node_modules")).unwrap();

    let n = ensure_node_modules(werk.to_str().unwrap(), &home);
    let werk_nm = werk.join("platform/api/node_modules");
    assert_eq!(n, 1, "relinked exactly the one package");
    assert!(werk_nm.is_symlink(), "werk node_modules is now a symlink, not the partial real dir");
    assert!(werk_nm.join("@types/node").exists(), "resolves canonical's @types/node through the symlink");

    // idempotent: a second run is a no-op (already the right symlink)
    assert_eq!(ensure_node_modules(werk.to_str().unwrap(), &home), 0, "idempotent — no relink on re-run");
}

#[test]
fn jsonl_line_is_well_formed() {
    let l = jsonl_line(123, "build.completed", "silas", 3061, "tr1", ",\"built\":\"werk-build=ab\"");
    assert!(l.contains("\"event\":\"build.completed\""));
    assert!(l.contains("\"card_id\":3061"));
    assert!(l.contains("\"trace_id\":\"tr1\""));
    assert!(l.ends_with("}\n"));
}
