// @test-type: unit — hermetic: path resolution and reason filtering over given inputs.
// @domain: alerts
// @card: #4714
// @owner: kade
//! #4714 AC4 — the cause, measured 10-23 21:41Z from the new reason field:
//! "reconcile grafana rules: UNMEASURED — cannot run convergence-land: No such
//! file or directory". src/lib.rs `convergence_land_bin` finds the deployed shim
//! when PATH has no ~/.chorus/bin; src/readings.rs `why_lines` (used by
//! src/main.rs run_sync) keeps only the lines for the classes that failed.
use chorus_crawl::convergence_land_bin;
use chorus_crawl::readings::why_lines;

#[test]
fn the_deployed_shim_is_found_without_it_on_path() {
    let b = convergence_land_bin(None, "/Users/x", |p| p == "/Users/x/.chorus/bin/convergence-land");
    assert_eq!(b, "/Users/x/.chorus/bin/convergence-land");
    assert_eq!(convergence_land_bin(Some("/opt/cl".into()), "/Users/x", |_| true), "/opt/cl", "the env var wins");
}

// negative proof: with no shim installed the old bare name is still the fallback,
// so a box that has it on PATH keeps working; the 03:30 box has the shim.
#[test]
fn negative_proof_no_shim_falls_back_to_the_bare_name() {
    assert_eq!(convergence_land_bin(None, "/Users/x", |_| false), "convergence-land");
}

const OUT: &str = "chorus-crawl: reconcile declared domains: UNMEASURED — no runner for a.test.js (#4537)
chorus-crawl: reconcile declared domains: UNMEASURED — no runner for b.test.js (#4537)
chorus-crawl: reconcile grafana rules: UNMEASURED — cannot run convergence-land: No such file or directory (os error 2)
VALIDATE|tests|Test (declared domain)|tree=1|graph=1|missing=0|stale=0|measured";

#[test]
fn only_the_failed_class_line_is_the_reason() {
    let w = why_lines(OUT, "not measured: Grafana rule");
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("cannot run convergence-land"), "{w:?}");
}

// negative proof: the 21:41Z reason carried every UNMEASURED line and the cap cut the cause
#[test]
fn negative_proof_unrelated_unmeasured_lines_are_left_out() {
    let w = why_lines(OUT, "not measured: Grafana rule");
    assert!(!w.iter().any(|l| l.contains("declared domains")), "{w:?}");
    assert!(why_lines(OUT, "the check printed no VALIDATE rows").is_empty(), "no class named, no lines");
}
