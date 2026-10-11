//! #4499 — the edges werk-build, werk-test and werk-deploy scope by come from
//! the same code-unit generator the crawler writes to the graph, read from the
//! build files git tracks in the real tree (no folder walk).
use werk_build::declared_edges;

fn repo() -> std::path::PathBuf {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::path::Path::new(&dir).join("../../..").canonicalize().unwrap()
}

#[test]
fn the_real_tree_states_its_crate_package_and_include_edges() {
    let e = declared_edges(&repo());
    let has = |p: &str, d: &str| e.iter().any(|(a, b)| a == p && b == d);
    assert!(has("werk-teardown", "werk-accept"), "cargo path dep");
    assert!(has("platform/services/shared/crate_dirs.rs", "werk-build"), "shared include, per file");
    assert!(has("chorus-oidc", "pulse-api") || e.iter().any(|(a, b)| a == "chorus-oidc" && b.contains("pulse")), "a crate outside platform/services (#4491): {:?}",
        e.iter().filter(|(a, _)| a == "chorus-oidc").collect::<Vec<_>>());
}

#[test]
fn negative_proof_a_tree_git_cannot_list_gives_no_edges_and_says_so() {
    let d = std::env::temp_dir().join(format!("wb-4499-nogit-{}", std::process::id()));
    std::fs::create_dir_all(d.join("platform/services/a")).unwrap();
    std::fs::write(d.join("platform/services/a/Cargo.toml"), "[dependencies]\nb = { path = \"../b\" }\n").unwrap();
    assert!(declared_edges(&d).is_empty());
}
