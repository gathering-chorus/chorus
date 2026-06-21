//! werk-test unit tests (#3190) — the pure decision core, no subprocess, no fs.
//! Each test maps to an AC: affected-unit detection on the diff, the bootstrap
//! escape, and the advisory→blocking gate decision.
use werk_test::{
    affected_units, check_plan, gate_outcome, is_self_modifying, spine_args, CheckKind,
    GateOutcome, PlannedCheck, TestUnit,
};

fn plan_kinds(units: &[TestUnit]) -> Vec<CheckKind> {
    check_plan(units).into_iter().map(|c| c.kind).collect()
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

// --- affected_units: classify the card's diff into test units ---

#[test]
fn affected_units_detects_a_rust_crate() {
    let changed = s(&["platform/services/werk-merge/src/lib.rs"]);
    assert_eq!(affected_units(&changed), vec![TestUnit::RustCrate("werk-merge".into())]);
}

#[test]
fn affected_units_detects_a_ts_package() {
    let changed = s(&["platform/api/src/server.ts"]);
    assert_eq!(affected_units(&changed), vec![TestUnit::TsPackage("platform/api".into())]);
}

#[test]
fn affected_units_dedupes_a_crate_touched_in_many_files() {
    let changed = s(&[
        "platform/services/werk-demo/src/lib.rs",
        "platform/services/werk-demo/src/main.rs",
        "platform/services/werk-demo/tests/e2e.rs",
    ]);
    assert_eq!(affected_units(&changed), vec![TestUnit::RustCrate("werk-demo".into())]);
}

#[test]
fn affected_units_is_deterministic_crates_sorted_then_packages() {
    let changed = s(&[
        "platform/services/werk-push/src/lib.rs",
        "platform/api/src/x.ts",
        "platform/services/werk-build/src/lib.rs",
    ]);
    // crates sorted alpha first, then TS packages in TS_PACKAGES order
    assert_eq!(
        affected_units(&changed),
        vec![
            TestUnit::RustCrate("werk-build".into()),
            TestUnit::RustCrate("werk-push".into()),
            TestUnit::TsPackage("platform/api".into()),
        ]
    );
}

#[test]
fn affected_units_empty_when_diff_touches_no_test_unit() {
    let changed = s(&["docs/readme.md", "activity.md", ".github/workflows/quality.yml"]);
    assert!(affected_units(&changed).is_empty());
}

#[test]
fn affected_units_includes_werk_test_itself_as_a_crate() {
    // a card editing this very crate still gets its tests run (advisory — see
    // is_self_modifying); the unit must still be detected.
    let changed = s(&["platform/services/werk-test/src/lib.rs"]);
    assert_eq!(affected_units(&changed), vec![TestUnit::RustCrate("werk-test".into())]);
}

// --- is_self_modifying: the bootstrap escape ---

#[test]
fn self_modifying_true_when_diff_touches_canonical_werkyml() {
    assert!(is_self_modifying(&s(&[".github/workflows/werk.yml"])));
}

#[test]
fn self_modifying_true_when_diff_touches_the_werk_test_crate() {
    assert!(is_self_modifying(&s(&["platform/services/werk-test/src/lib.rs"])));
}

#[test]
fn self_modifying_false_for_an_ordinary_card() {
    assert!(!is_self_modifying(&s(&[
        "platform/services/werk-merge/src/lib.rs",
        "platform/api/src/server.ts",
    ])));
}

#[test]
fn self_modifying_does_not_match_a_lookalike_sibling_crate() {
    // "werk-tester" must not trip the "werk-test/" prefix guard.
    assert!(!is_self_modifying(&s(&["platform/services/werk-tester/src/lib.rs"])));
}

// --- gate_outcome: advisory → blocking, with the bootstrap escape ---

#[test]
fn gate_no_units_passes() {
    assert_eq!(gate_outcome(0, false, false), GateOutcome::NoUnits);
    assert_eq!(GateOutcome::NoUnits.exit_code(), 0);
}

#[test]
fn gate_all_green_passes() {
    assert_eq!(gate_outcome(2, false, false), GateOutcome::Pass);
    assert_eq!(GateOutcome::Pass.exit_code(), 0);
}

#[test]
fn gate_red_floor_blocks_the_land() {
    let o = gate_outcome(2, true, false);
    assert_eq!(o, GateOutcome::Block);
    assert_eq!(o.exit_code(), 1, "a red floor MUST stop the land");
}

#[test]
fn gate_red_but_self_modifying_is_advisory_not_blocking() {
    // the #3397 deadlock escape: a card fixing the gate can't be hard-gated by
    // the canonical werk.yml it's fixing — honest-red, but exit 0.
    let o = gate_outcome(1, true, true);
    assert_eq!(o, GateOutcome::AdvisoryFail);
    assert_eq!(o.exit_code(), 0, "a self-modifying card must not deadlock");
}

// --- check_plan: the 3 checks #3397 didn't wire (tsc/clippy/doc) join cargo+jest ---

#[test]
fn plan_rust_crate_runs_cargo_then_workspace_clippy_and_doc() {
    let units = vec![TestUnit::RustCrate("werk-merge".into())];
    assert_eq!(
        plan_kinds(&units),
        vec![CheckKind::CargoTest, CheckKind::ClippyRatchet, CheckKind::DocCoherence]
    );
}

#[test]
fn plan_ts_package_runs_tsc_and_jest_then_doc_but_no_clippy() {
    let units = vec![TestUnit::TsPackage("platform/api".into())];
    // no Rust changed → no clippy-ratchet; doc-coherence still runs.
    assert_eq!(
        plan_kinds(&units),
        vec![CheckKind::Tsc, CheckKind::Jest, CheckKind::DocCoherence]
    );
}

#[test]
fn plan_mixed_runs_all_five_check_kinds() {
    let units = vec![
        TestUnit::RustCrate("werk-build".into()),
        TestUnit::TsPackage("platform/pulse".into()),
    ];
    assert_eq!(
        plan_kinds(&units),
        vec![
            CheckKind::CargoTest,
            CheckKind::Tsc,
            CheckKind::Jest,
            CheckKind::ClippyRatchet,
            CheckKind::DocCoherence,
        ]
    );
}

#[test]
fn plan_empty_when_nothing_affected() {
    assert!(check_plan(&[]).is_empty());
}

#[test]
fn plan_clippy_and_doc_are_workspace_level_no_unit() {
    let plan = check_plan(&[TestUnit::RustCrate("werk-push".into())]);
    let ws: Vec<&PlannedCheck> = plan.iter().filter(|c| c.unit.is_none()).collect();
    assert_eq!(ws.len(), 2);
    assert!(ws.iter().all(|c| matches!(c.kind, CheckKind::ClippyRatchet | CheckKind::DocCoherence)));
}

// --- spine_args: typed failure emission shape (#3162 inherited-trace pattern) ---

#[test]
fn spine_args_builds_event_role_card_trace_and_extras() {
    let got = spine_args(
        "test.failed",
        "kade",
        "3190",
        "abc-123",
        &[("check", "cargo-test"), ("unit", "werk-merge")],
    );
    assert_eq!(
        got,
        vec![
            "test.failed".to_string(),
            "kade".to_string(),
            "card=3190".to_string(),
            "trace=abc-123".to_string(),
            "check=cargo-test".to_string(),
            "unit=werk-merge".to_string(),
        ]
    );
}
