//! #3468/#3471 — the ops gate JUDGES a chorus-health report it cannot fetch
//! (the headless gate runs `claude -p` with `--disallowedTools Bash`, so the
//! SKILL's `bash chorus-health` call fails → the "no shell access" gate error).
//! The gate-runner (Rust, which CAN shell) pre-runs chorus-health and folds the
//! output into the ops gate's context via `ops_ctx`. Red-first (DEC-1674).

use werk_demo::ops_ctx;

#[test]
fn ops_ctx_folds_health_for_ops_only() {
    let base = "CARD #1\n=== BRANCH DIFF ===\n...";
    // non-ops gates pass through unchanged — the health fold is ops-only
    for g in ["product", "code", "quality", "arch"] {
        assert_eq!(ops_ctx(base, g, "HEALTH"), base, "non-ops gate {} must pass through", g);
    }
    // ops gate: the base context is preserved AND the health report is folded in,
    // with the instruction to judge (not run) it.
    let o = ops_ctx(base, "ops", "chorus-health: PASS\nexit 0");
    assert!(o.starts_with(base), "ops ctx keeps the base context");
    assert!(o.contains("chorus-health: PASS"), "folds the health report in");
    assert!(o.contains("JUDGE this output"), "tells the gate to judge, not run");
    assert!(o.contains("do NOT run chorus-health"), "tells the gate it has no shell");
}
