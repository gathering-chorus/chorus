//! werk-build unit tests — pure helpers (no subprocess, no fs side effects).
use werk_build::{
    branch_name, crate_for_path, discover_build_units, extract_cdhash, extract_file_deps,
    jsonl_line, pkg_name, resolve_trace, shared_lib_for_path, ts_service_for_path, BuildUnit,
};

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("silas", 3061), "silas/3061");
}

#[test]
fn crate_for_path_maps_services_paths_only() {
    assert_eq!(crate_for_path("platform/services/werk-build/src/lib.rs"), Some("werk-build".to_string()));
    assert_eq!(crate_for_path("platform/services/chorus-hooks/Cargo.toml"), Some("chorus-hooks".to_string()));
    // not a services crate -> None (TS paths handled by ts_service_for_path; docs etc. ignored)
    assert_eq!(crate_for_path("platform/api/src/server.ts"), None);
    assert_eq!(crate_for_path("roles/silas/adr/ADR-032.md"), None);
    assert_eq!(crate_for_path("platform/services/"), None);
}

#[test]
fn ts_service_for_path_maps_api_paths_to_chorus_api() {
    // #3092 — platform/api/* is chorus-api (the TS service); other paths are None.
    assert_eq!(ts_service_for_path("platform/api/src/server.ts"), Some("chorus-api".to_string()));
    assert_eq!(ts_service_for_path("platform/api/src/handlers/chorus-crawl.ts"), Some("chorus-api".to_string()));
    assert_eq!(ts_service_for_path("platform/api/package.json"), Some("chorus-api".to_string()));
    // a Rust crate path is NOT a TS service.
    assert_eq!(ts_service_for_path("platform/services/werk-build/src/lib.rs"), None);
    // docs/state are not TS services.
    assert_eq!(ts_service_for_path("roles/silas/adr/ADR-032.md"), None);
    assert_eq!(ts_service_for_path("platform/api"), None); // needs trailing slash to be inside the dir
}

#[test]
fn shared_lib_for_path_maps_chorus_sdk_paths() {
    // #3126 — platform/chorus-sdk/* is the shared library chorus-sdk. A change to
    // it MUST yield a build unit (the silent-stale-prod gap #3092 left: it matched
    // no class → zero units → acp shipped nothing → consumers ran stale).
    assert_eq!(shared_lib_for_path("platform/chorus-sdk/src/emit.ts"), Some("chorus-sdk".to_string()));
    assert_eq!(shared_lib_for_path("platform/chorus-sdk/package.json"), Some("chorus-sdk".to_string()));
    // a service / api / docs path is NOT the shared lib.
    assert_eq!(shared_lib_for_path("platform/services/werk-build/src/lib.rs"), None);
    assert_eq!(shared_lib_for_path("platform/api/src/server.ts"), None);
    assert_eq!(shared_lib_for_path("platform/chorus-sdk"), None); // needs trailing slash to be inside
}

#[test]
fn discover_build_units_recognizes_shared_lib_never_zero() {
    // The crux of #3126: a chorus-sdk-only diff used to yield ZERO units. It must
    // now yield the SharedLib unit so the cascade fires and prod can't run stale.
    let diff = ["platform/chorus-sdk/src/emit.ts", "platform/chorus-sdk/src/index.ts"];
    let units = discover_build_units(diff.iter());
    assert_eq!(units, vec![BuildUnit::SharedLib("chorus-sdk".to_string())]);
    assert!(!units.is_empty(), "a shared-lib change must NEVER yield zero build units (#3126)");
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
fn discover_build_units_finds_rust_and_ts_in_one_diff() {
    // A mixed diff produces both kinds, deduplicated, ordered.
    let diff = [
        "platform/services/werk-build/src/lib.rs",
        "platform/services/werk-build/tests/units.rs", // same crate -> dedup
        "platform/api/src/server.ts",
        "platform/api/src/handlers/chorus-crawl.ts", // same service -> dedup
        "roles/silas/adr/ADR-032.md",                 // not buildable
    ];
    let units = discover_build_units(diff.iter());
    // BTreeSet over (kind, name) sorts: RustCrate < TsService by enum-variant order.
    assert_eq!(
        units,
        vec![
            BuildUnit::RustCrate("werk-build".to_string()),
            BuildUnit::TsService("chorus-api".to_string()),
        ]
    );
}

#[test]
fn discover_build_units_empty_on_no_buildable_paths() {
    let diff = ["roles/silas/adr/ADR-032.md", "designing/docs/x.html"];
    let units = discover_build_units(diff.iter());
    assert!(units.is_empty(), "non-buildable paths must yield empty unit list");
}

#[test]
fn discover_build_units_handles_empty_lines_and_whitespace() {
    // git diff output may have trailing whitespace; the function trims per-line.
    let diff = ["", "  platform/services/chorus-hooks/src/lib.rs  ", "\n"];
    let units = discover_build_units(diff.iter());
    assert_eq!(units, vec![BuildUnit::RustCrate("chorus-hooks".to_string())]);
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

#[test]
fn jsonl_line_is_well_formed() {
    let l = jsonl_line(123, "build.completed", "silas", 3061, "tr1", ",\"built\":\"werk-build=ab\"");
    assert!(l.contains("\"event\":\"build.completed\""));
    assert!(l.contains("\"card_id\":3061"));
    assert!(l.contains("\"trace_id\":\"tr1\""));
    assert!(l.ends_with("}\n"));
}
