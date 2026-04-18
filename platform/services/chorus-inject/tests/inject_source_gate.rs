//! chorus-inject source gate: build-time lint, NOT behavior.
//!
//! **This is not a behavior test.** It reads `src/lib.rs` and asserts
//! literal strings are present (`keystroke`, `key code 36`) and absent
//! (`do script`). It fails on rename-without-semantic-change and passes
//! on semantic-change-without-rename. Treat as clippy-with-more-ceremony.
//!
//! Extracted 2026-04-18 per #2155 — separating source-grep lints from
//! behavior tests so `tests/` files are either one or the other, never
//! mixed. Every `tests/` file should be honestly labeled. See also
//! loom-principles:quality-at-source.
//!
//! Related: #2029 — `do script` breaks auto-submit (keystroke + return
//! key code are required). #2167 — shipped AppleScript moved from main.rs
//! to lib.rs; this lint follows.

#[test]
fn inject_source_uses_keystroke_not_do_script() {
    let source = std::fs::read_to_string(
        "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-inject/src/lib.rs"
    ).expect("can't read lib.rs");

    assert!(
        source.contains("key code 36"),
        "inject must use 'key code 36' (Return) for auto-submit"
    );
    assert!(
        source.contains("keystroke"),
        "inject must use 'keystroke' for text delivery"
    );
    // do script breaks auto-submit (#2029). Only comments should reference it.
    let code_lines: Vec<&str> = source.lines()
        .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("//!"))
        .collect();
    let code_only = code_lines.join("\n");
    assert!(
        !code_only.contains("do script"),
        "inject must NOT use 'do script' — breaks auto-submit (#2029)"
    );
}
