//! Pure-helper unit tests (#3193 v2, RED first) — the CLI seam (three modes), the
//! objective-floor checks (AC checkboxes, src-without-test, removed pub symbols),
//! and the anti-ceremony verdict validation.

use werk_review::{
    parse_review_args, removed_pub_symbols, src_without_test, unchecked_ac, validate_verdict,
    Mode,
};

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parse_recognizes_floor_verdict_and_check() {
    let m = parse_review_args(&args(&["3193", "kade"]), None).unwrap();
    assert_eq!(m, Mode::Floor { card: 3193, role: "kade".into() });

    let m = parse_review_args(&args(&["verdict", "3193", "pass", "clean diff, AC 1-6 all covered"]), None).unwrap();
    assert_eq!(m, Mode::Verdict { card: 3193, pass: true, findings: "clean diff, AC 1-6 all covered".into() });

    let m = parse_review_args(&args(&["verdict", "3193", "fail", "AC item 3 not covered", "src/lib.rs:42 unwrap on user input"]), None).unwrap();
    assert_eq!(m, Mode::Verdict { card: 3193, pass: false, findings: "AC item 3 not covered src/lib.rs:42 unwrap on user input".into() });

    let m = parse_review_args(&args(&["check", "3193"]), None).unwrap();
    assert_eq!(m, Mode::Check { card: 3193 });

    // role falls back to DEPLOY_ROLE for the floor (verb convention).
    let m = parse_review_args(&args(&["3193"]), Some("kade".into())).unwrap();
    assert_eq!(m, Mode::Floor { card: 3193, role: "kade".into() });

    assert!(parse_review_args(&args(&[]), None).is_err());
    assert!(parse_review_args(&args(&["notanum", "kade"]), None).is_err());
    assert!(parse_review_args(&args(&["verdict", "3193", "maybe", "x"]), None).is_err(), "verdict is pass|fail only");
    assert!(parse_review_args(&args(&["3193"]), None).is_err(), "floor needs a role from arg or env");
}

#[test]
fn unchecked_ac_finds_open_boxes_in_the_cards_human_view() {
    let view = "## AC\n- [x] done thing\n- [ ] open thing one\n- [X] also done\n- [ ] open thing two\nnot a checkbox line";
    let open = unchecked_ac(view);
    assert_eq!(open, vec!["open thing one", "open thing two"]);
    assert!(unchecked_ac("- [x] a\n- [X] b").is_empty(), "all checked → no findings");
    assert!(unchecked_ac("").is_empty());
}

#[test]
fn src_without_test_fires_only_when_source_changed_and_no_test_did() {
    // src changed, no test changed → finding (the missing-tests hunt's objective floor).
    let names = "platform/services/werk-review/src/lib.rs\nREADME.md\n";
    assert!(src_without_test(names), "src without any test change fires");
    // src + a Rust test file → quiet.
    let names = "platform/services/x/src/lib.rs\nplatform/services/x/tests/e2e.rs\n";
    assert!(!src_without_test(names));
    // ts src + .test.ts → quiet.
    let names = "platform/api/src/handlers/foo.ts\nplatform/api/tests/handlers/foo.test.ts\n";
    assert!(!src_without_test(names));
    // docs-only diff → quiet (nothing to test).
    assert!(!src_without_test("designing/docs/x.html\nREADME.md\n"));
    // inline #[cfg(test)] convention: a src-only diff that touches a *_test.rs or
    // tests/ path counts; bare src stays a finding even if small.
    let names = "platform/services/x/src/lib.rs\n";
    assert!(src_without_test(names));
}

#[test]
fn removed_pub_symbols_reads_the_unified_diff() {
    let diff = "\
--- a/src/lib.rs
+++ b/src/lib.rs
-pub fn gone_function(x: u64) -> String {
-pub struct GoneStruct {
+pub fn renamed_function(x: u64) -> String {
 pub fn untouched(x: u64) {}
--- a/src/server.ts
+++ b/src/server.ts
-export function goneTsHelper(s: string): boolean {
+const unrelated = 1;
-  let not_a_symbol = 2;
";
    let gone = removed_pub_symbols(diff);
    assert!(gone.contains(&"gone_function".to_string()), "{gone:?}");
    assert!(gone.contains(&"GoneStruct".to_string()), "{gone:?}");
    assert!(gone.contains(&"goneTsHelper".to_string()), "{gone:?}");
    assert!(!gone.contains(&"untouched".to_string()), "context lines are not removals");
    assert!(!gone.contains(&"renamed_function".to_string()), "additions are not removals");
    assert_eq!(gone.len(), 3, "no junk symbols: {gone:?}");
}

#[test]
fn verdict_validation_is_the_anti_ceremony_guard() {
    // fail with empty findings = a non-review → rejected.
    assert!(validate_verdict(false, "", true).is_err(), "fail demands specific findings");
    assert!(validate_verdict(false, "   ", true).is_err());
    // any verdict without the floor having run = ceremony → rejected.
    let e = validate_verdict(true, "looks clean", false).unwrap_err();
    assert!(e.contains("floor"), "names the missing floor run: {e}");
    // pass with substance + floor-run → ok; fail with findings + floor-run → ok.
    assert!(validate_verdict(true, "AC 1-6 covered, no scope creep", true).is_ok());
    assert!(validate_verdict(false, "AC item 3 not covered", true).is_ok());
}
