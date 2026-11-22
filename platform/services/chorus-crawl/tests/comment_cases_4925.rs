// @domain: code — placed by chorus-crawl's domain rules (#4538)
//! #4925 — child-cards-4669.test.ts registered a case named "patient 0" that no runner
//! emits: the words `it ("patient 0", …)` sit in its header comment, and the TS case
//! scanner read comments as code. "no result" every night.
use chorus_crawl::cases::{blank_js_comments, jest_case_names};

const SRC: &str = "/**\n * A follow-on becomes a child of\n * it (\"patient 0\", Jeff 2026-10-20) instead of a reopen.\n */\n\
// it('a line comment', () => {})\n\
const url = 'http://x/it(\"not a comment\")';\n\
it('links the child', () => { expect(1).toBe(1); });\n\
test(\"shows children\", () => {}); // it('trailing comment')\n";

#[test]
fn negative_proof_a_case_named_in_a_comment_is_not_registered() {
    let names = jest_case_names(SRC);
    assert!(!names.iter().any(|n| n == "patient 0"), "block comment read as a case: {names:?}");
    assert!(!names.iter().any(|n| n == "a line comment" || n == "trailing comment"), "line comment read as a case: {names:?}");
    for want in ["links the child", "shows children"] {
        assert!(names.iter().any(|n| n == want), "control: the real case {want} is missing: {names:?}");
    }
}

#[test]
fn blanking_keeps_offsets_lines_and_strings() {
    let out = blank_js_comments(SRC);
    assert_eq!(out.len(), SRC.len(), "byte offsets moved");
    assert_eq!(out.lines().count(), SRC.lines().count(), "line numbers moved");
    assert!(out.contains("'http://x/it(\"not a comment\")'"), "a // inside a string was blanked: {out}");
}
