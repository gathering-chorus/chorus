// @domain: code — placed by chorus-crawl's domain rules (#4538)
//! #4925 — the 11-22 07:24 nightly reported 268 registered tests with no result; 200 of them
//! live in three crates outside platform/services (designing/services/athena-gen,
//! directing/products/pulse/api, shaping/domains/principles/api). The nightly plan mapped a
//! test file to a Rust crate only under platform/services/, so those crates never ran.
use std::path::PathBuf;
use werk_test::{crate_name_in_tree, nightly_cargo_crates, set_repo_root, TestRow};

fn repo() -> PathBuf {
    // run-time, never env!() — the #4030 guard
    let m = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    m.ancestors().find(|d| d.join(".git").exists()).unwrap().to_path_buf()
}

fn row(path: &str) -> TestRow {
    TestRow {
        file_path: path.into(),
        covers: String::new(),
        pyramid_layer: String::new(),
        hermeticity: String::new(),
        test_concern: String::new(),
        needs: String::new(),
    }
}

#[test]
fn the_nightly_plans_the_three_crates_outside_platform_services() {
    set_repo_root(&repo());
    let rows = vec![
        row("designing/services/athena-gen/tests/emit_4490.rs"),
        row("directing/products/pulse/api/src/door_tests.rs"),
        row("shaping/domains/principles/api/src/door_tests.rs"),
        row("platform/services/werk-test/tests/units.rs"),
    ];
    let crates = nightly_cargo_crates(&rows);
    for want in ["athena-gen", "pulse-api", "principles-api", "werk-test"] {
        assert!(crates.iter().any(|c| c == want), "{want} not planned: {crates:?}");
    }
}

#[test]
fn the_package_name_comes_from_the_manifest_not_the_directory() {
    // pulse/api's directory ends in "api"; the crate is "pulse-api"
    assert_eq!(crate_name_in_tree(&repo(), "directing/products/pulse/api/src/door_tests.rs").as_deref(), Some("pulse-api"));
}

#[test]
fn negative_proof_a_path_in_no_crate_is_no_crate() {
    // a home whose directory holds no Cargo.toml is never a crate, and neither is a doc
    let t = std::env::temp_dir().join(format!("wt-4925-{}", std::process::id()));
    std::fs::create_dir_all(t.join("designing/services/ghost/tests")).unwrap();
    assert_eq!(crate_name_in_tree(&t, "designing/services/ghost/tests/a.rs"), None);
    assert_eq!(crate_name_in_tree(&repo(), "designing/docs/x.md"), None);
    // a manifest makes it one, by its package name
    std::fs::write(t.join("designing/services/ghost/Cargo.toml"), "[package]\nname = \"ghost-crate\"\n").unwrap();
    assert_eq!(crate_name_in_tree(&t, "designing/services/ghost/tests/a.rs").as_deref(), Some("ghost-crate"));
    let _ = std::fs::remove_dir_all(&t);
}

/// #4925 round 2 — suite rows and typed binaries name a crate by its real home.
/// NEGATIVE: the old prefix would have named athena-gen under platform/services.
#[test]
fn a_crate_is_named_by_its_real_home() {
    set_repo_root(&repo());
    assert_eq!(werk_test::crate_home("athena-gen"), "designing/services/athena-gen");
    assert_eq!(werk_test::crate_home("pulse-api"), "directing/products/pulse/api");
    assert_eq!(werk_test::crate_home("werk-test"), "platform/services/werk-test");
    assert_ne!(werk_test::crate_home("athena-gen"), "platform/services/athena-gen");
}
