//! #3871 — a Clearing change must plan a BROWSER FLOW check.
//!
//! Jeff, 2026-08-14: "our user facing test automation for clearing is 100% not
//! helping — i doubt we even run it on changes."
//!
//! Worse than not running on changes: `proving/flows` appears in NEITHER
//! `.github/workflows/werk.yml` NOR `nightly-suites.sh`. Every browser flow
//! written this week — the folding canary, the room-key proof, the
//! leaked-markup check, the url topology gate — has only ever run when a role
//! typed the command by hand. They are documentation.
//!
//! `werk-test` knows three check kinds (#3190, #3397): cargo-test, tsc, jest.
//! All unit-layer. Nothing in the land path opens a page, so every Clearing
//! defect Jeff hit this week was invisible to the pipeline that approved it.
//!
//! NEGATIVE PROOF (#3734): `clearing_change_plans_a_flow_check` fails against
//! today's code — `CheckKind::BrowserFlow` does not exist, so a Clearing change
//! plans only tsc+jest. `a_non_user_facing_change_plans_no_flow` is the other
//! half: booting a browser for a Rust-crate change is a tax with no signal, and
//! a tax gets switched off the first slow week.

use werk_test::{check_plan, CheckKind, TestUnit};

fn kinds(units: &[TestUnit]) -> Vec<CheckKind> {
    check_plan(units).into_iter().map(|c| c.kind).collect()
}

#[test]
fn clearing_change_plans_a_flow_check() {
    let plan = kinds(&[TestUnit::TsPackage("directing/clearing".into())]);
    assert!(
        plan.contains(&CheckKind::BrowserFlow),
        "a Clearing change must plan a browser flow; planned {:?}",
        plan
    );
}

#[test]
fn a_clearing_change_still_plans_its_unit_checks() {
    // The flow is ADDITIVE. Displacing tsc or jest would trade a fast signal
    // for a slow one and call it progress.
    let plan = kinds(&[TestUnit::TsPackage("directing/clearing".into())]);
    assert!(plan.contains(&CheckKind::Tsc), "tsc still planned: {:?}", plan);
    assert!(plan.contains(&CheckKind::Jest), "jest still planned: {:?}", plan);
}

#[test]
fn a_non_user_facing_change_plans_no_flow() {
    let plan = kinds(&[TestUnit::RustCrate("werk-test".into())]);
    assert!(
        !plan.contains(&CheckKind::BrowserFlow),
        "a Rust-only change must not plan a browser flow; planned {:?}",
        plan
    );
}

/// Silas, 2026-08-14, on the `_ => true` removal: "the whole #3734 disease in
/// three characters — an unwired check kind indistinguishable from a passing
/// one. Make its absence a fixture so it can't grow back."
///
/// `has_runner` is an exhaustive match, so a NEW variant fails to compile until
/// someone answers the question. This test is the second lock: it fails if a
/// variant is ever answered `false` and left planned.
#[test]
fn all_kinds_are_wired() {
    for kind in werk_test::ALL_KINDS {
        assert!(
            kind.has_runner(),
            "{} is planned but has no runner — it would report without running",
            kind.label()
        );
    }
}


#[test]
fn the_flow_check_is_labelled_for_the_spine() {
    // The label lands in test.failed events and on the demo surface. "check
    // failed" with no name is how a red goes unread for a week.
    assert_eq!(CheckKind::BrowserFlow.label(), "browser-flow");
}
