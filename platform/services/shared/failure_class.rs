// #3513 — the ONE failure classifier, shared by every werk verb.
//
// NOT a crate, NOT a verb. A single source file under `shared/` (deliberately
// outside the werk-* verb family — werk-common would have read as a phantom verb,
// which is the exact drift this card kills). Each verb does:
//     include!("../../shared/failure_class.rs");
// so there is one classifier source, compiled into every verb — no per-verb copies
// to drift. Lifted from werk-merge's private `failure_class` (#3495).
//
// `failureClass` splits a failure into CHANGE (the work under test is bad — it
// BELONGS in DORA change-failure-rate) vs TOOLING (pipeline mechanics hiccupped —
// the change is fine, MUST be excluded). Read side = #3497. Conservative default:
// tooling, so a new/unknown reason never inflates CFR.

/// Classify a failure/refusal `reason` as `"change"` or `"tooling"`.
pub fn failure_class(reason: &str) -> &'static str {
    match reason {
        // CHANGE — the code/work under test is what failed.
        "test-fail" | "gate-fail" | "build-fail" | "compile-fail" | "tsc-fail"
        | "npm-build-fail" | "clippy-fail" | "review-floor-fail" | "dirty-floor-inputs" => "change",
        // TOOLING (default) — pipeline mechanics, and anything not yet seen.
        _ => "tooling",
    }
}

/// The witness payload for a failure/refusal event: `,"reason":..,"failureClass":..`
/// appended to the jsonl/spine `extra`. Every verb uses this so the discriminator
/// is uniform (was `refused_extra`, private to werk-merge).
pub fn fail_extra(reason: &str) -> String {
    format!(",\"reason\":\"{}\",\"failureClass\":\"{}\"", reason, failure_class(reason))
}

#[cfg(test)]
mod failure_class_tests {
    use super::*;

    #[test]
    fn change_reasons_are_change() {
        for r in [
            "test-fail", "gate-fail", "build-fail", "compile-fail", "tsc-fail",
            "clippy-fail", "review-floor-fail", "dirty-floor-inputs",
        ] {
            assert_eq!(failure_class(r), "change", "{r} must classify as change");
        }
    }

    #[test]
    fn tooling_and_unknown_default_to_tooling() {
        for r in [
            "no-werk", "branch-mismatch", "merge-conflict", "pr-create-fail",
            "not-mergeable", "no-open-pr", "env-up-fail", "env-down-fail",
            "push-rejected", "nudge-fail", "cdhash-divergence", "card-not-found",
            "wrong-status", "nothing-to-commit", "some-brand-new-reason",
        ] {
            assert_eq!(failure_class(r), "tooling", "{r} must default to tooling");
        }
    }

    #[test]
    fn fail_extra_is_a_valid_json_tail() {
        assert_eq!(fail_extra("test-fail"), ",\"reason\":\"test-fail\",\"failureClass\":\"change\"");
        assert_eq!(fail_extra("merge-conflict"), ",\"reason\":\"merge-conflict\",\"failureClass\":\"tooling\"");
    }
}
