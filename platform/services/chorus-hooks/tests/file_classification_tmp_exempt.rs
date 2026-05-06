//! `/tmp` and `/var/folders` are ephemeral sketch surfaces, not production code.
//!
//! The TDD gate uses `is_source_code` / `is_production_code` to decide whether
//! an Edit/Write tool call requires a prior test edit. HTML/Rust files written
//! into ephemeral OS-temp paths are drafts (e.g. design sketches passed to
//! Jeff's browser), not behavior that ships. Treating them as production
//! blocks legitimate sketch work and pushes drafts into other locations that
//! drift away from the canonical chorus tree.
//!
//! This test pins the exemption so the classifier stays narrow.

use chorus_hooks::shared::file_classification::{is_production_code, is_source_code, is_test_file};

#[test]
fn tmp_html_not_production() {
    assert!(!is_source_code("/tmp/chorus-werk-structure.html"));
    assert!(!is_production_code("/tmp/chorus-werk-structure.html"));
}

#[test]
fn tmp_rust_not_production() {
    assert!(!is_source_code("/tmp/sketch.rs"));
    assert!(!is_production_code("/tmp/sketch.rs"));
}

#[test]
fn macos_tmpdir_not_production() {
    // macOS `$TMPDIR` resolves under /var/folders/<hash>/T/...
    assert!(!is_source_code("/var/folders/xy/abc123/T/sketch.html"));
    assert!(!is_production_code("/var/folders/xy/abc123/T/sketch.rs"));
}

#[test]
fn real_paths_still_classify_as_source() {
    // Sanity: the exemption must not over-reach. Real source paths still classify.
    assert!(is_source_code(
        "/Users/jeff/CascadeProjects/chorus/platform/services/chorus-hooks/src/main.rs"
    ));
    assert!(is_source_code(
        "/Users/jeff/CascadeProjects/chorus/roles/kade/harvest-pipeline.html"
    ));
    assert!(is_production_code("src/handlers/foo.handler.ts"));
}

// #2740 — `test-foo.sh` (hyphen prefix) is a test-naming convention used
// across platform/scripts/ — it should classify as a test file like
// `test_foo.sh` (underscore prefix) already does. Without this, the TDD
// gate refuses edits to test-chorus-werk.sh and friends as if they were
// production code.

#[test]
fn test_hyphen_prefix_is_test_file() {
    assert!(is_test_file("platform/scripts/test-chorus-werk.sh"));
    assert!(is_test_file("platform/scripts/test-chorus-werk-sync.sh"));
    assert!(is_test_file("platform/scripts/test-chorus-bin-install.sh"));
    assert!(is_test_file("platform/scripts/test-hardcoded-bin-paths.sh"));
}

#[test]
fn test_hyphen_prefix_is_not_production() {
    assert!(!is_production_code(
        "platform/scripts/test-chorus-werk.sh"
    ));
    assert!(!is_production_code("test-something-else.sh"));
}

#[test]
fn underscore_prefix_still_works() {
    // Don't regress the existing `test_` / `_test.` matching.
    assert!(is_test_file("tests/foo_test.rs"));
    assert!(is_test_file("src/foo_test.ts"));
    assert!(is_test_file("test_helper.sh"));
}

#[test]
fn similar_but_not_test_files_unaffected() {
    // `testimony.md`, `latest.json`, etc. should NOT trip the test-file rule.
    // Match must be on `test-` or `test_` as a meaningful prefix, not a
    // substring match.
    assert!(is_source_code("src/latest_release.rs"));
    assert!(!is_test_file("src/contest.rs"));
    assert!(!is_test_file("src/protest_handler.ts"));
}
