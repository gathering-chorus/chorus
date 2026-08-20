//! werk-test unit tests (#3190) — the pure decision core, no subprocess, no fs.
//! Each test maps to an AC: affected-unit detection on the diff, the bootstrap
//! escape, and the advisory→blocking gate decision.
use werk_test::{
    affected_units, check_plan, expired_cases, gate_outcome, is_self_modifying,
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
fn plan_ts_package_runs_tsc_jest_lint_ratchet_then_doc_but_no_clippy() {
    let units = vec![TestUnit::TsPackage("platform/api".into())];
    // no Rust changed → no clippy-ratchet; lint-ratchet fires on any TS change
    // (#3787 — drift refuses at land, never an anonymous 03:55 red); doc still runs.
    assert_eq!(
        plan_kinds(&units),
        vec![CheckKind::Tsc, CheckKind::Jest, CheckKind::LintRatchet, CheckKind::DocCoherence]
    );
}

#[test]
fn plan_rust_only_card_skips_lint_ratchet() {
    // no TS package affected → the ESLint ratchet has nothing to measure (#3787).
    let units = vec![TestUnit::RustCrate("werk-merge".into())];
    assert!(!plan_kinds(&units).contains(&CheckKind::LintRatchet));
}

#[test]
fn lint_ratchet_is_workspace_level_and_runs_once() {
    assert_eq!(CheckKind::LintRatchet.label(), "lint-ratchet");
    let plan = check_plan(&[
        TestUnit::TsPackage("platform/pulse".into()),
        TestUnit::TsPackage("platform/api".into()),
    ]);
    let lint: Vec<&PlannedCheck> =
        plan.iter().filter(|c| c.kind == CheckKind::LintRatchet).collect();
    assert_eq!(lint.len(), 1, "lint-ratchet runs once, workspace-level");
    assert!(lint[0].unit.is_none());
}

#[test]
fn self_modifying_true_when_diff_touches_the_lint_ratchet_surface() {
    // #3787 AC3 — a card fixing the ratchet itself (script, config, baseline)
    // must run advisory, not deadlock on its own gate (#3197 bootstrap lesson).
    assert!(is_self_modifying(&s(&["platform/scripts/lint-ratchet.js"])));
    assert!(is_self_modifying(&s(&["eslint.config.js"])));
    assert!(is_self_modifying(&s(&[".eslint-baseline.json"])));
}

#[test]
fn plan_mixed_runs_all_six_check_kinds() {
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
            CheckKind::LintRatchet,
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
    // #3592 — payload is EXACTLY the deployed TestSuiteRunShape (cardId/ts/result);
    // off-model props got every prior post 422-refused by the DAL. Run context
    // lives in the test.started/test.completed spine events.
    for needle in ["\"cardId\":\"3634\"", "\"result\":\"blocked\"", "\"ts\":", "testsuiterun-3634-"] {
        assert!(p.contains(needle), "payload missing {}: {}", needle, p);
    }
    for banned in ["\"card\":", "\"role\":", "\"traceId\":", "\"planSource\":",
                   "\"checksPlanned\":", "\"durationMs\":", "\"verdict\":"] {
        assert!(!p.contains(banned), "off-model prop must not be sent {}: {}", banned, p);
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
    // #3592 — scoped tokens (#3573) demand the target graph named per request;
    // without this header every scoped write 403s out-of-scope.
    assert!(joined.contains("x-target-graph: urn:chorus:domains:tests"), "{}", joined);
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
    // (#3592: trace no longer travels in the payload — shape-exact cardId/ts/result)
    assert!(p.contains(r#"36\"34"#), "card escaped: {}", p);
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

// ═══ #3661 — the runner runs what the tests domain declares ═══

// ── AC1/AC2 groundwork: rows carry the model's pyramidLayer (3-col TSV) ──
#[test]
fn parse_test_rows_reads_the_three_column_layer_form() {
    use werk_test::parse_test_rows;
    let rows = parse_test_rows("platform/api/tests/a.test.ts\tsenses\tunit\n");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pyramid_layer, "unit");
}

#[test]
fn parse_test_rows_two_column_rows_still_parse_with_empty_layer() {
    use werk_test::parse_test_rows;
    // back-compat: a 2-col row (pre-#3661 jq) parses; layer is empty, never dropped
    let rows = parse_test_rows("platform/api/tests/a.test.ts\tsenses\n");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pyramid_layer, "");
}

// ── AC2: --domain / --type scope the declared set ──
fn row(path: &str, covers: &str, layer: &str) -> werk_test::TestRow {
    werk_test::TestRow {
        file_path: path.to_string(),
        covers: covers.to_string(),
        pyramid_layer: layer.to_string(),
    }
}

#[test]
fn scope_rows_by_domain_keeps_only_covering_tests() {
    use werk_test::scope_rows;
    let rows = vec![
        row("platform/api/tests/a.test.ts", "senses", "unit"),
        row("platform/api/tests/b.test.ts", "cicd", "unit"),
    ];
    let scoped = scope_rows(&rows, Some("senses"), None);
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].file_path, "platform/api/tests/a.test.ts");
}

#[test]
fn scope_rows_by_type_keeps_only_that_layer() {
    use werk_test::scope_rows;
    let rows = vec![
        row("platform/api/tests/a.test.ts", "senses", "unit"),
        row("platform/api/tests/c.test.ts", "senses", "integration"),
    ];
    let scoped = scope_rows(&rows, None, Some("integration"));
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].file_path, "platform/api/tests/c.test.ts");
}

#[test]
fn scope_rows_domain_and_type_intersect() {
    use werk_test::scope_rows;
    let rows = vec![
        row("platform/api/tests/a.test.ts", "senses", "unit"),
        row("platform/api/tests/c.test.ts", "senses", "integration"),
        row("platform/api/tests/b.test.ts", "cicd", "integration"),
    ];
    let scoped = scope_rows(&rows, Some("senses"), Some("integration"));
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].file_path, "platform/api/tests/c.test.ts");
}

#[test]
fn scope_rows_unscoped_is_identity() {
    use werk_test::scope_rows;
    let rows = vec![row("platform/api/tests/a.test.ts", "senses", "unit")];
    assert_eq!(scope_rows(&rows, None, None), rows);
}

// ── AC1: a scoped plan derives its units FROM the declared rows ──
#[test]
fn plan_units_from_rows_maps_declared_tests_to_their_units() {
    use werk_test::plan_units_from_rows;
    let rows = vec![
        row("platform/api/tests/a.test.ts", "senses", "unit"),
        row("platform/services/werk-demo/src/lib.rs", "build-domain", "unit"),
    ];
    let units = plan_units_from_rows(&rows);
    assert_eq!(
        units,
        vec![
            TestUnit::RustCrate("werk-demo".to_string()),
            TestUnit::TsPackage("platform/api".to_string()),
        ],
        "crates sorted before packages, derived from the declared rows only"
    );
}

#[test]
fn plan_units_from_rows_dedupes_many_tests_in_one_unit() {
    use werk_test::plan_units_from_rows;
    let rows = vec![
        row("platform/api/tests/a.test.ts", "senses", "unit"),
        row("platform/api/tests/b.test.ts", "cicd", "integration"),
    ];
    assert_eq!(plan_units_from_rows(&rows).len(), 1);
}

#[test]
fn plan_units_from_rows_ignores_paths_outside_known_units() {
    use werk_test::plan_units_from_rows;
    let rows = vec![row("designing/docs/some-doc-test.md", "athena", "unit")];
    assert!(plan_units_from_rows(&rows).is_empty());
}

// ── AC2 guard: a scoped run REQUIRES the model — no silent legacy fallback ──
#[test]
fn scoped_run_requires_model_plan_source() {
    use werk_test::scoped_requires_model;
    // scoped + fallback = refuse (the scope IS the model; legacy can't honor it)
    assert!(scoped_requires_model(true, "fallback"));
    // scoped + model = fine; unscoped never refuses on source
    assert!(!scoped_requires_model(true, "model"));
    assert!(!scoped_requires_model(false, "fallback"));
    assert!(!scoped_requires_model(false, "model"));
}

// ── AC3: on-disk-but-undeclared tests surface as a NAMED gap ──
#[test]
fn undeclared_gaps_names_disk_files_absent_from_the_domain() {
    use werk_test::undeclared_gaps;
    let on_disk = s(&[
        "platform/api/tests/a.test.ts",
        "platform/api/tests/new-unregistered.test.ts",
    ]);
    let declared = vec![row("platform/api/tests/a.test.ts", "senses", "unit")];
    let gaps = undeclared_gaps(&on_disk, &declared);
    assert_eq!(gaps, s(&["platform/api/tests/new-unregistered.test.ts"]));
}

#[test]
fn undeclared_gaps_empty_when_domain_declares_everything() {
    use werk_test::undeclared_gaps;
    let on_disk = s(&["platform/api/tests/a.test.ts"]);
    let declared = vec![row("platform/api/tests/a.test.ts", "senses", "unit")];
    assert!(undeclared_gaps(&on_disk, &declared).is_empty());
}

#[test]
fn undeclared_gaps_output_is_sorted_and_deduped() {
    use werk_test::undeclared_gaps;
    let on_disk = s(&[
        "platform/api/tests/z.test.ts",
        "platform/api/tests/b.test.ts",
        "platform/api/tests/b.test.ts",
    ]);
    let gaps = undeclared_gaps(&on_disk, &[]);
    assert_eq!(gaps, s(&["platform/api/tests/b.test.ts", "platform/api/tests/z.test.ts"]));
}

#[test]
fn gap_report_names_every_gap_and_the_none_case_is_explicit() {
    use werk_test::gap_report;
    // visible, never silent — mirrors quarantine_report's explicit-none style
    assert_eq!(gap_report(&[]), "undeclared: none");
    let r = gap_report(&s(&["platform/api/tests/x.test.ts"]));
    assert!(r.contains("undeclared (1"), "count named: {}", r);
    assert!(r.contains("platform/api/tests/x.test.ts"), "file named: {}", r);
}

// ---- #3592 — per-test result capture + reconcile shapes ----

#[test]
fn parse_case_tsv_maps_jest_statuses_and_drops_incomplete_rows() {
    use werk_test::parse_case_tsv;
    let tsv = "platform/api/tests/a.test.ts\tdoes x\tpassed\n\
               platform/api/tests/a.test.ts\tdoes y\tfailed\n\
               platform/api/tests/b.test.ts\tskipped one\tpending\n\
               only-two-cols\tnope\n\
               \n";
    let cases = parse_case_tsv(tsv);
    assert_eq!(cases.len(), 3);
    assert_eq!(cases[0].result, "pass");
    assert_eq!(cases[1].result, "fail");
    assert_eq!(cases[2].result, "skip");
    assert_eq!(cases[0].file_path, "platform/api/tests/a.test.ts");
    assert_eq!(cases[0].test_name, "does x");
}

#[test]
fn match_cargo_case_requires_unique_match_within_crate() {
    use werk_test::match_cargo_case;
    let rows = vec![
        row("platform/services/werk-test/src/lib.rs", "cicd", "unit"),
        row("platform/services/werk-test/src/main.rs", "cicd", "unit"),
        row("platform/services/other/src/lib.rs", "cicd", "unit"),
    ];
    // testName lives on the row via a parallel arg — identity = (filePath within crate, bare name)
    let names = vec!["alpha".to_string(), "alpha".to_string(), "beta".to_string()];
    // beta: unique within other crate
    assert_eq!(
        match_cargo_case("beta", "platform/services/other", &rows, &names),
        Some("platform/services/other/src/lib.rs".to_string())
    );
    // alpha within werk-test: ambiguous (two files) -> None
    assert_eq!(match_cargo_case("alpha", "platform/services/werk-test", &rows, &names), None);
    // missing entirely -> None
    assert_eq!(match_cargo_case("gamma", "platform/services/other", &rows, &names), None);
}

#[test]
fn rel_path_strips_werk_root_only() {
    use werk_test::rel_path;
    assert_eq!(
        rel_path("/w/kade-1/platform/api/tests/a.test.ts", "/w/kade-1"),
        "platform/api/tests/a.test.ts"
    );
    assert_eq!(rel_path("platform/api/tests/a.test.ts", "/w/kade-1"), "platform/api/tests/a.test.ts");
}

#[test]
fn test_result_payload_carries_identity_and_escapes() {
    use werk_test::test_result_payload;
    let p = test_result_payload(
        "platform/api/tests/a.test.ts", "asserts \"quoted\" thing", "fail",
        "test-a-asserts-quoted-thing", "3592", "kade", "trace-1", 1700000000123, 7,
    );
    assert!(p.contains("\"filePath\":\"platform/api/tests/a.test.ts\""), "{}", p);
    assert!(p.contains("\"testName\":\"asserts \\\"quoted\\\" thing\""), "{}", p);
    assert!(p.contains("\"result\":\"fail\""), "{}", p);
    assert!(p.contains("\"name\":\"testresult-3592-1700000000123-7\""), "{}", p);
    assert!(p.contains("\"ofTest\":\"test-a-asserts-quoted-thing\""), "{}", p);
    // #3925 INVERTED: runTs/cardId are ON-model now (shape bump, minCount 0
    // this cycle). The old assertion pinned the smuggling era — these fields
    // MUST travel as real properties, run-time-clocked, never post-time.
    assert!(p.contains("\"runTs\":"), "runTs is a real field now: {}", p);
    assert!(p.contains("\"cardId\":\"3592\""), "cardId is a real field now: {}", p);
    // the bare `card` key stays forbidden — cardId is the modeled name.
    assert!(!p.contains("\"card\":"), "bare 'card' was never the modeled name: {}", p);
}

#[test]
fn reconcile_gap_is_registered_minus_executed() {
    use werk_test::reconcile_gap;
    let registered = vec![
        ("a.ts".to_string(), "one".to_string()),
        ("a.ts".to_string(), "two".to_string()),
        ("b.rs".to_string(), "three".to_string()),
    ];
    let executed = vec![("a.ts".to_string(), "one".to_string())];
    let gap = reconcile_gap(&registered, &executed);
    assert_eq!(gap.len(), 2);
    assert!(gap.contains(&("a.ts".to_string(), "two".to_string())));
    assert!(gap.contains(&("b.rs".to_string(), "three".to_string())));
    // executed superset -> empty
    assert!(reconcile_gap(&registered, &registered).is_empty());
}

#[test]
fn reconcile_report_names_counts_and_explicit_none() {
    use werk_test::reconcile_report;
    assert_eq!(reconcile_report(3, &[]), "reconcile: registered 3, never-run: none");
    let gap = vec![("b.rs".to_string(), "three".to_string())];
    let r = reconcile_report(3, &gap);
    assert!(r.contains("never-run (1"), "{}", r);
    assert!(r.contains("b.rs :: three"), "{}", r);
}

#[test]
fn parse_rows_and_names_stays_aligned_and_backcompat() {
    use werk_test::parse_rows_and_names;
    let tsv = "a.ts\tsenses\tunit\tdoes x\ttest-a-does-x\n\
               b.rs\tcicd\tunit\n\
               \tbad\tunit\tnope\n";
    let (rows, names, entities) = parse_rows_and_names(tsv);
    assert_eq!(rows.len(), 2);
    assert_eq!(names.len(), 2);
    assert_eq!(entities.len(), 2);
    assert_eq!(names[0], "does x");
    assert_eq!(entities[0], "test-a-does-x");
    assert_eq!((names[1].as_str(), entities[1].as_str()), ("", ""));
}

#[test]
fn join_cases_maps_identity_to_entity_and_counts_unregistered() {
    use werk_test::{join_cases, CaseResult};
    let rows = vec![row("a.ts", "senses", "unit")];
    let names = vec!["does x".to_string()];
    let entities = vec!["test-a-does-x".to_string()];
    let cases = vec![
        CaseResult { file_path: "a.ts".into(), test_name: "does x".into(), result: "pass".into() },
        CaseResult { file_path: "a.ts".into(), test_name: "brand new".into(), result: "pass".into() },
    ];
    let (joined, unregistered) = join_cases(&cases, &rows, &names, &entities);
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].1, "test-a-does-x");
    assert_eq!(unregistered, 1);
}

// --- #3808: wire-back re-mint decision — token expiry mid-stream is recoverable,
// a real 401 (re-mint didn't help) fails loudly after ONE re-mint attempt ---

#[test]
fn remint_decision_accepts_2xx_and_fails_non_401_without_reminting() {
    use werk_test::{remint_decision, PostStep};
    assert_eq!(remint_decision("200", false), PostStep::Accept);
    assert_eq!(remint_decision("201", true), PostStep::Accept);
    // a 502 is the server refusing content — reminting can't help, count it failed
    assert_eq!(remint_decision("502", false), PostStep::Fail);
    assert_eq!(remint_decision("000", false), PostStep::Fail); // curl couldn't connect
}

#[test]
fn remint_decision_reminting_exactly_once_on_401() {
    use werk_test::{remint_decision, PostStep};
    // first 401 of the run → re-mint and retry (expired-token recovery)
    assert_eq!(remint_decision("401", false), PostStep::Remint);
    // 401 AFTER a re-mint → the identity is genuinely refused; fail loudly,
    // never retry forever (#3808 AC2)
    assert_eq!(remint_decision("401", true), PostStep::Fail);
}

#[test]
fn post_args_with_code_returns_status_and_never_uses_sf() {
    // the per-case post must yield the HTTP code (the 401 trigger) — `-sf`
    // swallows it and forced a second diagnostic request per failure (#3725)
    let args = werk_test::post_args_with_code("http://x/testresults", "tok", "{}");
    assert!(args.contains(&"%{http_code}".to_string()), "{:?}", args);
    assert!(!args.contains(&"-sf".to_string()), "{:?}", args);
    assert!(args.contains(&"Authorization: Bearer tok".to_string()));
    assert!(args.contains(&"x-target-graph: urn:chorus:domains:tests".to_string()));
}

// ── #3821 — the test leg scopes to the diff via the SAME shared core builds
// use (shared/scope_units.rs). Full suite is the loud fallback, never the
// default; an asset-only diff runs nothing.

fn sg(v: &str) -> String { v.to_string() }

fn su(name: &str, dir: &str) -> werk_test::ScopeUnit {
    werk_test::ScopeUnit { name: name.into(), dir: dir.into() }
}

fn test_world() -> (Vec<werk_test::ScopeUnit>, Vec<(String, String)>) {
    let units = vec![
        su("werk-teardown", "platform/services/werk-teardown"),
        su("werk-accept", "platform/services/werk-accept"),
        su("owl-api", "platform/services/owl-api"),
        su("platform/api", "platform/api"),
        su("platform/pulse", "platform/pulse"),
    ];
    let edges = vec![
        ("werk-teardown".to_string(), "werk-accept".to_string()),
        ("chorus-sdk".to_string(), "platform/api".to_string()),
    ];
    (units, edges)
}

#[test]
fn asset_only_diff_scopes_to_no_test_units() {
    // #3810's exact shape: an HTML page + a knowledge doc → zero units, the
    // 24-minute Rust sweep never starts.
    let (units, edges) = test_world();
    let v = werk_test::scoped_test_units(
        &[sg("platform/api/public/index.html"), sg("knowledge/doc-coherence.md")],
        &units, &edges);
    assert_eq!(v, Some(vec![]));
}

#[test]
fn negative_proof_3821_downstream_dependent_tests_still_run() {
    // #3734 — the fixture where scoping too narrowly ships a break: a change
    // in werk-teardown (lib-only provider) must include werk-accept's tests,
    // because A's change can break B's suite. Scope-to-A-alone fails here.
    let (units, edges) = test_world();
    let v = werk_test::scoped_test_units(
        &[sg("platform/services/werk-teardown/src/lib.rs")], &units, &edges);
    let scoped = v.expect("must scope");
    assert!(scoped.contains(&su("werk-teardown", "platform/services/werk-teardown")), "{:?}", scoped);
    assert!(scoped.contains(&su("werk-accept", "platform/services/werk-accept")),
        "downstream dependent's tests MUST run — under-scoping ships breaks: {:?}", scoped);
}

#[test]
fn unmapped_file_falls_back_to_full_suite() {
    let (units, edges) = test_world();
    let v = werk_test::scoped_test_units(&[sg("tsconfig.base.json")], &units, &edges);
    assert_eq!(v, None, "unknown means run everything, never means skip");
}

// ---- #3929 — nextest is the ONLY cargo lane; absence is a loud red ----------

#[test]
fn nextest_args_base_selects_lib_and_bins() {
    let args = werk_test::nextest_run_args(&[]);
    assert_eq!(args, vec!["nextest", "run", "--no-tests=fail"]);
    // (c): no --lib/--bins — tests/ suites are IN the gate now, and zero-tests is red
    assert!(!args.iter().any(|a| a == "--lib" || a == "--bins"));
}

#[test]
fn nextest_args_translate_quarantine_to_exact_filterset() {
    let args = werk_test::nextest_run_args(&["flaky_a", "flaky_b"]);
    let e = args.iter().position(|a| a == "-E").expect("filterset flag");
    assert_eq!(args[e + 1], "not test(=flaky_a) and not test(=flaky_b)");
    // never the cargo-test style trailing `-- --skip` form
    assert!(!args.iter().any(|a| a == "--skip"), "cargo-test skip leaked into nextest args");
}

#[test]
fn nextest_case_lines_parse_pass_fail_skip() {
    let out = "\
    Starting 3 tests across 1 binary\n\
        PASS [   0.005s] werk-test units::alpha_ok\n\
        FAIL [   0.102s] werk-test units::beta_broken\n\
        SKIP [         ] werk-test units::gamma_quarantined\n\
     Summary [   0.110s] 3 tests run: 1 passed, 1 failed, 1 skipped\n";
    let cases = werk_test::parse_nextest_cases(out);
    assert_eq!(
        cases,
        vec![
            ("alpha_ok".to_string(), "pass".to_string()),
            ("beta_broken".to_string(), "fail".to_string()),
            ("gamma_quarantined".to_string(), "skip".to_string()),
        ]
    );
}

/// NEGATIVE PROOF (#3734): the fixture below is the VERBATIM output this
/// machine produced with nextest absent (captured 2026-08-20, pre-install).
/// The guarded condition — no nextest — must classify as a refusal, never Ok.
#[test]
fn negative_proof_absent_nextest_refuses_loudly() {
    let real_absence = "error: no such command: `nextest`\n\n\
help: a command with a similar name exists: `test`\n";
    let v = werk_test::classify_nextest_probe(false, real_absence, (0, 9, 143));
    let err = v.expect_err("absent nextest classified Ok — vacuous whole-crate green is back");
    assert!(err.contains("nextest-missing"), "refusal must be named, got: {}", err);
}

#[test]
fn present_nextest_at_or_above_pin_is_ok() {
    assert!(werk_test::classify_nextest_probe(true, "cargo-nextest 0.9.143\n", (0, 9, 143)).is_ok());
    assert!(werk_test::classify_nextest_probe(true, "cargo-nextest 0.10.0\n", (0, 9, 143)).is_ok());
}

#[test]
fn present_but_older_than_pin_refuses() {
    let err = werk_test::classify_nextest_probe(true, "cargo-nextest 0.9.72\n", (0, 9, 143))
        .expect_err("stale nextest passed the pin");
    assert!(err.contains("nextest-older-than-pin"), "got: {}", err);
}

#[test]
fn failed_probe_with_other_error_is_still_a_refusal_not_a_pass() {
    // e.g. broken rustup shim — anything non-ok refuses; there is NO fallback lane
    assert!(werk_test::classify_nextest_probe(false, "error: toolchain hosed", (0, 9, 143)).is_err());
}

#[test]
fn pin_parses_from_config_and_absence_of_pin_is_none() {
    let toml = "# comment\nnextest-version = \"0.9.143\"\n";
    assert_eq!(werk_test::parse_nextest_pin(toml), Some((0, 9, 143)));
    assert_eq!(werk_test::parse_nextest_pin("store.dir = \"x\""), None);
}
