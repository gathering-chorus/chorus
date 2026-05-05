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

use chorus_hooks::shared::file_classification::{is_production_code, is_source_code};

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
