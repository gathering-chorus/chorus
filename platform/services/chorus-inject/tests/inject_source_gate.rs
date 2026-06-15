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


fn chorus_root() -> String {
    std::env::var("CHORUS_ROOT").ok().filter(|s| !s.is_empty())
        .expect("CHORUS_ROOT must be set and non-empty")
}

#[test]
fn inject_source_uses_keystroke_not_do_script() {
    let source = std::fs::read_to_string(
        format!("{}/platform/services/chorus-inject/src/lib.rs", chorus_root())
    ).expect("can't read lib.rs");

    assert!(
        source.contains("key code 36"),
        "inject must use 'key code 36' (Return) for auto-submit"
    );
    assert!(
        source.contains("keystroke"),
        "inject must use 'keystroke' for text delivery"
    );
    // #2029: a BARE `do script` breaks auto-submit. #3352 (Jeff 2026-06-11) found
    // the exception: the by-tty path writes `do script "<text>" in t` then submits
    // with a follow-up `do script "" in t` (the real newline that Claude treats as
    // submit). So `do script` is allowed ONLY when paired with that empty-do-script
    // submit; a lone do-script with no newline submit is still the #2029 bug.
    let code_lines: Vec<&str> = source.lines()
        .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("//!"))
        .collect();
    let code_only = code_lines.join("\n");
    if code_only.contains("do script") {
        assert!(
            code_only.contains(r#"do script "" in t"#),
            "#3352: `do script` is only allowed in the by-tty path paired with the empty-do-script newline submit; a bare do-script is the #2029 auto-submit bug"
        );
    }
}
