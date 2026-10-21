// @test-type: unit — hermetic: parses convergence-land sync output into a validate row; no Grafana, no store.
// @domain: heralds
// @card: #4091
// @owner: kade
//! #4091 AC4 — the grafana sync reading rides the sync sensor: one validate
//! row (alerts / Grafana rule) with missing and stale named.
use chorus_crawl::grafana_rule_row;

#[test]
fn a_measured_sync_names_missing_and_stale() {
    let r = grafana_rule_row("measured tree=3 graph=2\nmissing host-pressure\nstale disk-full (for)\n");
    assert!(r.measured);
    assert_eq!((r.tree, r.graph), (3, 2));
    assert_eq!(r.missing, vec!["host-pressure"]);
    assert_eq!(r.stale, vec!["disk-full (for)"]);
    assert_eq!((r.domain.as_str(), r.class.as_str()), ("alerts", "Grafana rule"));
}

#[test]
fn a_clean_sync_is_a_measured_zero() {
    let r = grafana_rule_row("measured tree=0 graph=0\n");
    assert!(r.measured && r.missing.is_empty() && r.stale.is_empty());
}

// NEGATIVE PROOF: no credential, no binary, garbage or a half line is unmeasured, never a clean zero.
#[test]
fn anything_but_a_measured_head_is_unmeasured() {
    for out in ["unmeasured refused: no Grafana credential", "", "cannot run convergence-land", "measured tree=3", "measured tree=x graph=0"] {
        assert!(!grafana_rule_row(out).measured, "{out:?}");
    }
}
