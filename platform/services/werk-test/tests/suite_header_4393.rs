//! #4393 — card runs pick the right tests. Jeff 2026-09-27: "we want the
//! goldilocks spot on tests, not too many and not too little". On #4335 two
//! suites that send live alerts paged Silas and Jeff from card runs, and
//! assert-only edits in 94 suites pulled every case, needs-stack ones too.
use werk_test::{assert_only_change, header_says_sends_live_alerts};

fn suite(name: &str) -> String {
    // run time, not env!(): the werk-test 4030 guard refuses a compile-time path
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/../../tests/{name}")).expect("suite")
}

// #4515 — the two suites that paged (3968, 4027) were deleted by Jeff: they
// skipped in every run, card and nightly. The guard stays for any later live
// suite, so its header is written here.
const LIVE: &str = "#!/usr/bin/env bats\n# @test-type: integration, sends-live-alerts — posts real probes to the live bridge\n# @domain: alerts\n";

#[test]
fn a_suite_that_sends_live_alerts_says_so_in_its_header() {
    assert!(header_says_sends_live_alerts(LIVE));
}

#[test]
fn negative_proof_the_header_before_the_tag_runs_and_a_hermetic_stub_never_reads_as_live() {
    let before = LIVE.replacen("integration, sends-live-alerts —", "integration —", 1);
    assert!(!header_says_sends_live_alerts(&before), "before the tag the suite would still run in a card run");
    // 4039 drives a caged ops-nudge: it names the alert path but sends nothing
    assert!(!header_says_sends_live_alerts(&suite("4039-nudge-cage.bats")));
}

#[test]
fn an_assert_only_edit_is_seen_as_one() {
    let diff = "--- a/x.bats\n+++ b/x.bats\n@@ -10 +10 @@\n-  [ \"$status\" -eq 0 ]\n+  [ \"$status\" -eq 1 ]\n@@ -20 +20 @@\n-  [[ \"$output\" == *a* ]]\n+  [[ \"$output\" == *b* ]]\n";
    assert!(assert_only_change(diff));
}

#[test]
fn negative_proof_any_other_edit_runs_the_whole_suite() {
    let diff = "--- a/x.bats\n+++ b/x.bats\n@@ -9 +9 @@\n-  run curl -s localhost:3340/health\n+  run curl -s localhost:3341/health\n";
    assert!(!assert_only_change(diff));
    assert!(!assert_only_change(""), "no change is not an assert-only change");
    let setup = "+setup() { start_stack; }\n+  [ 1 ]\n";
    assert!(!assert_only_change(setup));
}
