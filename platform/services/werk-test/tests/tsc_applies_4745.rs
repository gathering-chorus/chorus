// @test-type: unit
// @domain: tests
// #4745 — #4745 run 1 read UNMEASURED at tsc:platform/apps/sexuality-player, a
// JavaScript package (no tsconfig.json, no typescript) whose lockfile the card
// bumped. Covers platform/services/werk-test/src/lib.rs (tsc_applies) and
// platform/services/werk-test/src/main.rs (run_tsc).
use std::fs;
use werk_test::tsc_applies;

fn pkg(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("tsc-applies-4745-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("package.json"), r#"{"scripts":{"test":"node --test"}}"#).unwrap();
    d
}

#[test]
fn a_javascript_package_has_nothing_to_typecheck() {
    assert!(!tsc_applies(&pkg("js")));
}

/// NEGATIVE PROOF: a package with a tsconfig.json still gets its tsc lane.
#[test]
fn negative_proof_a_typescript_package_is_still_typechecked() {
    let d = pkg("ts");
    fs::write(d.join("tsconfig.json"), "{}").unwrap();
    assert!(tsc_applies(&d));
}

#[test]
fn the_real_javascript_package_reads_as_javascript() {
    let root = std::env::current_dir().unwrap().join("../../.."); // cargo runs tests from the crate
    assert!(!tsc_applies(&root.join("platform/apps/sexuality-player")));
    assert!(tsc_applies(&root.join("platform/pulse")), "pulse is TypeScript");
}
