//! #3854 — the cap is injected into every turn's context so drafts start
//! short; the Stop gate stays as backstop. Pure-line test first (TDD red).

use chorus_hooks::word_cap::cap_context_line;

// Wiring contract (context_inject appends the line to the manifest block):
// the line must be self-contained prose safe to append after any block.
#[test]
fn cap_line_is_single_line_and_appendable() {
    let line = cap_context_line(100);
    assert!(!line.contains('\n'), "one line — appended below the manifest envelope");
    assert!(line.starts_with("Reply cap:"), "scannable label for the model");
}

#[test]
fn cap_line_names_the_number_and_the_rules() {
    let line = cap_context_line(100);
    assert!(line.contains("100 words"), "carries the resolved cap");
    assert!(line.contains("don't count") || line.contains("do not count"),
        "names what is excluded so receipts get fenced");
    assert!(line.to_lowercase().contains("first time"),
        "says write short FIRST — the gate cannot unprint a draft");
}

#[test]
fn cap_line_carries_a_nondefault_cap() {
    assert!(cap_context_line(40).contains("40 words"), "property value flows through");
}
