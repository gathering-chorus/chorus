// #4393 — what a test suite's own header says about running it in a card run.
//
// Jeff 2026-09-27: "we want the goldilocks spot on tests, not too many and not
// too little". On #4335 two suites (3968 real bridge probes, 4027 live
// chorus-health) paged Silas and Jeff from inside card runs, three runs over.
// A suite that sends live alerts says so on its `@test-type:` line, and
// werk-test never runs it in a card run. The suite's own header is the source
// (the crawler reads the same line); the row's hermeticity stays needs-stack,
// because the nightly's integration tier is chosen by that value. std only.

/// The marker a suite's `@test-type:` line carries when running it sends a
/// live alert, nudge or page.
pub const SENDS_LIVE_ALERTS: &str = "sends-live-alerts";

/// Does the suite's header (its first 40 lines) say it sends live alerts?
pub fn header_says_sends_live_alerts(content: &str) -> bool {
    content
        .lines()
        .take(40)
        .any(|l| l.contains("@test-type:") && l.contains(SENDS_LIVE_ALERTS))
}

/// #4393 rule 1 — does a suite's diff change only assertion lines? `diff` is
/// `git diff -U0` output for that one file. An assert-only edit cannot need the
/// stack the suite's other cases stand up, so the card runs the suite's
/// hermetic cases only (bats `--filter-tags '!needs-stack'`).
pub fn assert_only_change(diff: &str) -> bool {
    let changed: Vec<&str> = diff
        .lines()
        .filter(|l| (l.starts_with('+') || l.starts_with('-')) && !l.starts_with("+++") && !l.starts_with("---"))
        .map(|l| l[1..].trim())
        .filter(|l| !l.is_empty())
        .collect();
    !changed.is_empty()
        && changed.iter().all(|l| {
            l.starts_with("[ ")
                || l.starts_with("[[ ")
                || l.starts_with("! [")
                || l.starts_with("assert")
                || l.starts_with("refute")
                || l.starts_with('#')
        })
}
