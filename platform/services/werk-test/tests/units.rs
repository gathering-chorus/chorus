//! werk-test unit tests (#3190) — the pure decision core, no subprocess, no fs.
//! Each test maps to an AC: affected-unit detection on the diff, the bootstrap
//! escape, and the advisory→blocking gate decision.
use werk_test::{
    affected_units, cargo_skip_args, check_plan, expired_cases, gate_outcome, is_self_modifying,
    parse_quarantine_rows, quarantine_report, spine_args, CheckKind, GateOutcome, PlannedCheck,
    Quarantined, TestUnit,
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

// --- quarantine: skip flaky cases at the gate, report them visibly (#2530 absorbed) ---

#[test]
fn cargo_skip_args_empty_when_nothing_quarantined() {
    // no quarantine → the normal `cargo test` invocation is unchanged (no trailing `--`).
    assert!(cargo_skip_args(&[]).is_empty());
}

#[test]
fn cargo_skip_args_builds_one_skip_per_quarantined_case() {
    let got = cargo_skip_args(&["acquire_lock_reclaims_stale", "flaky_net_timeout"]);
    assert_eq!(
        got,
        s(&["--", "--skip", "acquire_lock_reclaims_stale", "--skip", "flaky_net_timeout"])
    );
}

#[test]
fn quarantine_report_none_is_explicit_not_silent() {
    // a skip must be VISIBLE even when empty — never a silent absence (#3443 bar).
    assert_eq!(quarantine_report(&[]), "quarantined: none");
}

#[test]
fn quarantine_report_lists_case_reason_and_expiry() {
    let q = vec![
        Quarantined {
            case: "flaky_net_timeout".into(),
            reason: "intermittent net".into(),
            until: "2026-07-01".into(),
        },
    ];
    let line = quarantine_report(&q);
    assert!(line.contains("flaky_net_timeout"), "names the case");
    assert!(line.contains("intermittent net"), "names the reason");
    assert!(line.contains("2026-07-01"), "names the expiry");
    assert!(line.contains('1'), "counts how many were skipped");
}

// --- parse_quarantine_rows: the curl|jq TSV → Quarantined (read-wiring, testable) ---

#[test]
fn parse_quarantine_rows_empty_input_is_empty() {
    assert!(parse_quarantine_rows("").is_empty());
    assert!(parse_quarantine_rows("\n  \n").is_empty());
}

#[test]
fn parse_quarantine_rows_reads_case_reason_until() {
    let tsv = "flaky_net_timeout\tintermittent net\t2026-07-01\n";
    assert_eq!(
        parse_quarantine_rows(tsv),
        vec![Quarantined {
            case: "flaky_net_timeout".into(),
            reason: "intermittent net".into(),
            until: "2026-07-01".into(),
        }]
    );
}

#[test]
fn parse_quarantine_rows_skips_lines_with_no_case() {
    // a row whose testName column is blank can't be skipped by name — drop it.
    let tsv = "\tsome reason\t2026-07-01\nreal_case\twhy\t2026-08-01\n";
    let got = parse_quarantine_rows(tsv);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].case, "real_case");
}

// --- expired_cases: quarantineUntil < today → auto-file-card candidates (#2530) ---

#[test]
fn expired_cases_flags_past_expiry_only() {
    let q = vec![
        Quarantined { case: "a".into(), reason: "r".into(), until: "2026-06-01".into() }, // past
        Quarantined { case: "b".into(), reason: "r".into(), until: "2026-12-31".into() }, // future
    ];
    let expired = expired_cases(&q, "2026-06-23");
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].case, "a");
}

#[test]
fn expired_cases_today_is_not_yet_expired() {
    // until == today: the hold runs through end of day, not expired yet.
    let q = vec![Quarantined { case: "a".into(), reason: "r".into(), until: "2026-06-23".into() }];
    assert!(expired_cases(&q, "2026-06-23").is_empty());
}

#[test]
fn expired_cases_blank_until_is_never_expired() {
    // a malformed/blank expiry must not silently auto-file — be conservative.
    let q = vec![Quarantined { case: "a".into(), reason: "r".into(), until: "".into() }];
    assert!(expired_cases(&q, "2026-06-23").is_empty());
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

// --- #3621: the canonical wide test.completed — emitted ALWAYS (green included).
// werk-test used to emit only test.failed, so a passing gate left ZERO spine
// evidence — "all tests passed" and "the step never ran" were indistinguishable
// in a trace (Jeff's #3609 question). completed_extras is the pure field
// builder: verdict + counts + duration on every run, failureClass on red.

#[test]
fn completed_extras_carries_verdict_counts_and_duration_on_pass() {
    let got = werk_test::completed_extras(&GateOutcome::Pass, 2, 5, 0, 12345, false);
    assert!(got.contains(&("verdict".to_string(), "pass".to_string())));
    assert!(got.contains(&("units".to_string(), "2".to_string())));
    assert!(got.contains(&("checks_run".to_string(), "5".to_string())));
    assert!(got.contains(&("checks_failed".to_string(), "0".to_string())));
    assert!(got.contains(&("duration_ms".to_string(), "12345".to_string())));
    // green carries NO failureClass — absence is meaningful, not accidental
    assert!(!got.iter().any(|(k, _)| k == "failureClass"));
}

#[test]
fn completed_extras_names_the_failure_class_on_block() {
    let got = werk_test::completed_extras(&GateOutcome::Block, 1, 3, 2, 900, false);
    assert!(got.contains(&("verdict".to_string(), "BLOCK".to_string())));
    assert!(got.contains(&("checks_failed".to_string(), "2".to_string())));
    assert!(got.contains(&("failureClass".to_string(), "change".to_string()))); // closed {change,tooling} DORA enum
}

#[test]
fn completed_extras_marks_advisory_for_self_modifying_cards() {
    let got = werk_test::completed_extras(&GateOutcome::AdvisoryFail, 1, 2, 1, 500, true);
    assert!(got.contains(&("advisory".to_string(), "true".to_string())));
    assert!(got.contains(&("verdict".to_string(), "advisory-fail (self-modifying)".to_string())));
}

// ── #3634 — model-driven plan derivation from the tests domain ─────────────
// The run plan comes from /tests rows (filePath → unit, covers → domain),
// fetched via curl|jq at the boundary (the quarantine pattern) and parsed as
// TSV here: changed units name the touched domains (the covers of tests living
// in those units), and every unit holding tests covering a touched domain joins
// the plan. UNION with the legacy path-derived units — the model can only ADD
// coverage in v1, never subtract (the superset AC, proven by construction).

#[test]
fn model_plan_unions_covers_matched_units_with_legacy() {
    use werk_test::{model_units, parse_test_rows, TestUnit};
    // three tests: one in platform/api covering "senses", one in a rust crate
    // covering "senses" (cross-unit blast radius!), one covering "borg" only.
    let tsv = "platform/api/tests/a.test.ts\tsenses\n\
               platform/services/pulse-gather/tests/b.rs\tsenses\n\
               platform/pulse/tests/c.test.ts\tborg\n";
    let rows = parse_test_rows(tsv);
    assert_eq!(rows.len(), 3);
    // the card changed platform/api only → legacy picks TsPackage(platform/api);
    // its tests cover "senses" → pulse-gather (also covering senses) joins.
    let legacy = vec![TestUnit::TsPackage("platform/api".to_string())];
    let units = model_units(&rows, &legacy);
    assert!(units.contains(&TestUnit::TsPackage("platform/api".to_string())), "legacy retained");
    assert!(units.contains(&TestUnit::RustCrate("pulse-gather".to_string())),
        "cross-unit covers match joins the plan: {:?}", units);
    assert!(!units.contains(&TestUnit::TsPackage("platform/pulse".to_string())),
        "unrelated domain (borg) stays out: {:?}", units);
}

#[test]
fn model_plan_is_superset_of_legacy_by_construction() {
    use werk_test::{model_units, parse_test_rows, TestUnit};
    // empty model data → model_units == legacy exactly (never smaller).
    let rows = parse_test_rows("");
    let legacy = vec![TestUnit::RustCrate("werk-commit".to_string()), TestUnit::TsPackage("platform/api".to_string())];
    let units = model_units(&rows, &legacy);
    for l in &legacy {
        assert!(units.contains(l), "legacy unit {:?} must survive", l);
    }
}

#[test]
fn parse_test_rows_tolerates_garbage_and_missing_fields() {
    use werk_test::parse_test_rows;
    assert!(parse_test_rows("not a tsv row").is_empty());
    // rows missing either field are dropped, not panicked on.
    assert!(parse_test_rows("only-one-field\n\tcovers-no-path\n").is_empty());
}

// ── #3634 — TestSuiteRun write-back payload ────────────────────────────────
#[test]
fn suite_run_payload_carries_the_run_facts() {
    use werk_test::suite_run_payload;
    let p = suite_run_payload("3634", "kade", "trace-x", "model", 5, 1, 1234, "blocked");
    for needle in ["\"card\":\"3634\"", "\"role\":\"kade\"", "\"traceId\":\"trace-x\"",
                   "\"planSource\":\"model\"", "\"checksPlanned\":5", "\"checksFailed\":1",
                   "\"durationMs\":1234", "\"verdict\":\"blocked\"", "testsuiterun-3634-"] {
        assert!(p.contains(needle), "payload missing {}: {}", needle, p);
    }
}

#[test]
fn suite_run_post_args_pin_the_curl_contract() {
    use werk_test::suite_run_post_args;
    let a = suite_run_post_args("http://x/testsuiteruns", "tok", "{\"k\":1}");
    let joined = a.join(" ");
    assert!(joined.starts_with("-sf --max-time 10 -X POST"), "fail-fast + bounded: {}", joined);
    assert!(joined.contains("Authorization: Bearer tok"), "{}", joined);
    assert!(joined.contains("Content-Type: application/json"), "{}", joined);
    assert!(joined.ends_with("http://x/testsuiteruns"), "endpoint last: {}", joined);
}

// ── #3634 gather feedback (silas): JSON payload must survive hostile strings ─
// (zero-dep crate: validated with the lib's own escaper, not serde)
#[test]
fn json_escape_neutralizes_quotes_backslashes_and_control_chars() {
    use werk_test::json_escape;
    assert_eq!(json_escape("plain"), "plain");
    assert_eq!(json_escape(r#"qu"ote"#), r#"qu\"ote"#);
    assert_eq!(json_escape(r"back\slash"), r"back\\slash");
    assert_eq!(json_escape("new\nline"), r"new\nline");
}

#[test]
fn suite_run_payload_escapes_every_string_field() {
    use werk_test::suite_run_payload;
    let p = suite_run_payload(r#"36"34"#, "kade", r#"tr"ace"#, "model", 1, 0, 2, r#"block"ed"#);
    // no RAW interior quotes may survive: every quote inside a value must be escaped
    assert!(p.contains(r#"36\"34"#), "card escaped: {}", p);
    assert!(p.contains(r#"tr\"ace"#), "trace escaped: {}", p);
    assert!(p.contains(r#"block\"ed"#), "verdict escaped: {}", p);
    // structural sanity: after dropping escaped quotes, the raw quotes pair up
    let unescaped = p.replace(r#"\""#, "");
    assert_eq!(unescaped.matches('"').count() % 2, 0, "quotes balanced: {}", p);
}

// ── #3634 gather feedback (wren): fallback labeling is a pure, pinned decision ─
#[test]
fn plan_source_label_is_model_only_on_successful_nonempty_fetch() {
    use werk_test::plan_source_label;
    assert_eq!(plan_source_label(true, 10), "model");
    assert_eq!(plan_source_label(true, 0), "fallback", "empty result = fallback");
    assert_eq!(plan_source_label(false, 0), "fallback", "failed fetch = fallback");
}

// ── #3634 gather feedback (silas): multi-valued covers fans out, never drops ─
#[test]
fn parse_test_rows_accepts_fanned_multi_covers_rows() {
    use werk_test::parse_test_rows;
    // the jq filter emits one row per covers value — both rows parse
    let rows = parse_test_rows("platform/api/tests/a.test.ts\tsenses\nplatform/api/tests/a.test.ts\tborg\n");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].covers, "senses");
    assert_eq!(rows[1].covers, "borg");
}
