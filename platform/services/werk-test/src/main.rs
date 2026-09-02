//! werk-test binary (#3190) — thin shell over the pure core in lib.rs.
//!
//! Promotes #3397's inline werk.yml test step to a verb, flips it BLOCKING, and
//! adds the bootstrap escape + the three checks #3397 didn't wire (tsc,
//! clippy-ratchet, doc-coherence). Typed failures emit to the ONE spine on the
//! inherited trace (#3162) so a red gate is queryable, not just an exit code.
//!
//! Remaining (AC): wire the verb INTO werk.yml (replace the inline advisory step)
//! + deploy — the integration that flips it live, demo-gated.
use std::path::Path;
use std::process::Command;
use werk_test::{
    affected_units, check_plan, gap_report, gate_outcome, is_self_modifying,
    match_cargo_case, model_units, parse_case_tsv, parse_quarantine_rows,
    jest_plan, parse_rows_and_names, plan_source_label, plan_units_from_rows, quarantine_report,
    JestPlan,
    reconcile_gap, reconcile_report, rel_path, scope_rows, scoped_requires_model, spine_args,
    scope_declared_edges, scoped_test_units, suite_run_payload, test_result_payload,
    undeclared_gaps, CaseResult, CheckKind, Quarantined, ScopeUnit, TestRow, TestUnit,
    TS_PACKAGES,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("werk-test: {}", e);
            std::process::exit(1);
        }
    }
}

/// Parse `card` and `role`, find the card's werk, detect affected units on the
/// diff, run the planned checks, emit typed failures to the spine, and gate.
fn run(args: &[String]) -> Result<i32, String> {
    // #3592 AC3 — `werk-test --reconcile`: registered ∖ executed, visible +
    // alertable (tests.reconcile spine event), never blocking.
    if args.iter().any(|a| a == "--reconcile") {
        return run_reconcile();
    }
    // #3920 fold — `werk-test --nightly`: the 03:00 cargo lane runs through THIS
    // verb, so nextest (#3929), the needs-stack typed skips (#3919), and the
    // per-case TestResult posts (#3592) apply at 03:00 identically to the gate.
    if args.iter().any(|a| a == "--nightly") {
        return run_nightly(args);
    }
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let card = positional
        .first()
        .map(|s| s.to_string())
        .ok_or("usage: werk-test <card_id> <role> [--domain=<d>] [--type=<unit|integration|bdd|e2e>]")?;
    let role = positional
        .get(1)
        .map(|s| s.to_string())
        .or_else(|| std::env::var("ROLE").ok())
        .ok_or("missing role (argv[2] or $ROLE)")?;

    let werk_base =
        std::env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE unset".to_string())?;
    let werk = format!("{}/{}-{}", werk_base, role, card);
    if !Path::new(&werk).is_dir() {
        return Err(format!("werk not found: {}", werk));
    }
    let trace = std::env::var("CHORUS_TRACE_ID").unwrap_or_default();

    let changed = git_changed_files(&werk)?;
    let legacy_units = affected_units(&changed);
    // #3634 — stage 2: derive the plan from the tests domain. Model rows (filePath,
    // covers) widen the legacy path-derived units to every unit holding tests that
    // cover a touched domain — UNION, never smaller (the superset AC). A failed
    // fetch degrades to the legacy plan, loudly (test.plan.degraded), never silently.
    let (rows, row_names, row_entities, plan_source) = fetch_test_rows();
    // #3661 AC2 — --domain/--type scope the DECLARED set; the scope is a model
    // predicate, so a scoped run refuses (loudly) when the domain is unreachable
    // instead of running an unscopable legacy plan. Unscoped keeps the degrade path.
    let scope_domain = flag_value(args, "--domain");
    let scope_type = flag_value(args, "--type");
    let scoped = scope_domain.is_some() || scope_type.is_some();
    if scoped_requires_model(scoped, plan_source) {
        emit_spine("test.scope.refused", &role, &card, &trace,
            &[("reason", "tests-domain-unreachable"),
              ("scope_domain", scope_domain.as_deref().unwrap_or("")),
              ("scope_type", scope_type.as_deref().unwrap_or(""))]);
        return Err("scoped run (--domain/--type) requires the tests domain; fetch failed or empty — refusing, not degrading to legacy lanes".into());
    }
    let units = if scoped {
        // #3661 AC1 — the scoped plan derives from the declared rows, nothing else.
        let scoped_rows = scope_rows(&rows, scope_domain.as_deref(), scope_type.as_deref());
        println!(
            "scope: domain={} type={} → {} declared test(s)",
            scope_domain.as_deref().unwrap_or("*"),
            scope_type.as_deref().unwrap_or("*"),
            scoped_rows.len()
        );
        plan_units_from_rows(&scoped_rows)
    } else {
        // #3821 — diff-scoped plan via the ONE shared core werk-build uses:
        // a diff runs the tests of the units it can affect (touched + declared
        // dependents), the FULL widened plan only on a loud fallback. The
        // 24-minute lesson (#3810): an HTML page pulled four Rust crates'
        // 1,782 cases through the covers-union; nothing it touched could
        // reach them.
        let full_units = model_units(&rows, &legacy_units);
        match diff_scoped_units(&werk, &changed) {
            Some(scoped_units) => {
                emit_spine("test.scoped", &role, &card, &trace,
                    &[("changed", &changed.len().to_string()),
                      ("scoped", &scoped_units.len().to_string()),
                      ("of", &full_units.len().to_string())]);
                println!("scope(diff): {} unit(s) of {} (shared scope core, #3821)",
                    scoped_units.len(), full_units.len());
                scoped_units
            }
            None => {
                emit_spine("test.scope.full", &role, &card, &trace,
                    &[("reason", "unmapped-or-forced"), ("units", &full_units.len().to_string())]);
                println!("scope(diff): FULL fallback — unmapped path or WERK_TEST_FULL (loud, never silent)");
                full_units
            }
        }
    };
    // #3917 — bats suites and the shell scripts they cover were invisible to every
    // selection lane above: neither is a Rust crate nor a TS package, so a diff made
    // entirely of them selected ZERO units and the blocking gate exited 0. Union the
    // implicated suites in here, after scoping, so they are added on every lane
    // (scoped, diff-scoped, and full fallback alike).
    let mut units = units;
    let cov_index = build_suite_coverage(&werk);
    for suite in werk_test::affected_bats_suites(&changed, &cov_index) {
        let u = werk_test::TestUnit::BatsSuite(suite);
        if !units.contains(&u) {
            units.push(u);
        }
    }
    // AC2 — a changed script no suite exercises is a NAMED gap. Silence here is how
    // "nothing ran" became indistinguishable from "everything passed".
    for s in werk_test::uncovered_scripts(&changed, &cov_index) {
        println!("   gap: {} — no bats suite references this script (uncovered, #3917)", s);
        emit_spine("test.script.uncovered", &role, &card, &trace, &[("script", &s)]);
    }
    let units = units;

    // #3917 AC5 — `--explain` answers "what would this diff measure?" without
    // running anything. The question had no answer before: the only way to learn
    // the gate had selected nothing was to read a green summary and disbelieve it.
    if args.iter().any(|a| a == "--explain") {
        println!("changed: {} file(s)", changed.len());
        for u in &units {
            println!("  unit: {}", unit_name(u));
        }
        if units.is_empty() {
            println!("  {} — this diff would be waved through", gate_outcome(0, false, false).label());
        }
        return Ok(0);
    }

    let self_mod = is_self_modifying(&changed);
    let plan = check_plan(&units);

    // #3912 phase 1 — the jest leg pulls REGISTERED tests: per affected TS
    // package, jest's import graph names the related test files for the diff
    // and the registry (unit layer) is the authority on what runs. Registry
    // unreachable → FULL per-package fallback, loudly labeled.
    let mut related_all: Vec<String> = Vec::new();
    for u in &units {
        if let werk_test::TestUnit::TsPackage(p) = u {
            let in_pkg: Vec<String> = changed
                .iter()
                .filter(|f| f.starts_with(&format!("{}/", p)))
                .cloned()
                .collect();
            related_all.extend(jest_related_files(&werk, p, &in_pkg));
        }
    }
    let jplan = jest_plan(plan_source == "model", &rows, &related_all);
    if let JestPlan::FullFallback { ref reason } = jplan {
        println!("jest-select: {}", reason);
    } else if let JestPlan::Selected(ref sels) = jplan {
        let n: usize = sels.iter().map(|s| s.test_files.len()).sum();
        println!("jest-select: {} registered unit test file(s) cover the diff (registry-answered)", n);
    }
    // #3931 — the selection is EVIDENCE, not a count: name every selected file
    // with its reason (or the fallback's reason) on stdout and the spine, so an
    // under-selection is inspectable from the record alone.
    let sel_details = werk_test::selection_details(&jplan);
    for d in &sel_details {
        println!("jest-select:   {} ({})", d.file, d.reason);
    }
    if !sel_details.is_empty() {
        let sample = sel_details.iter().take(8).map(|d| d.file.as_str())
            .collect::<Vec<_>>().join(";");
        let reason0 = sel_details[0].reason.clone();
        emit_spine("test.selection", &role, &card, &trace,
            &[("count", &sel_details.len().to_string()), ("files", &sample), ("reason", &reason0)]);
    }

    // #3661 AC3 — the on-disk-but-undeclared surface: test files in the planned
    // units that the tests domain does not declare are NAMED (stdout + spine),
    // never silently run or skipped. Only meaningful when the model answered.
    if plan_source == "model" {
        let on_disk = on_disk_test_files(&werk, &units);
        let gaps = undeclared_gaps(&on_disk, &rows);
        println!("{}", gap_report(&gaps));
        if !gaps.is_empty() {
            let sample = gaps.iter().take(5).cloned().collect::<Vec<_>>().join(";");
            emit_spine("test.gap.undeclared", &role, &card, &trace,
                &[("count", &gaps.len().to_string()), ("files", &sample)]);
        }
    }

    // Quarantined cases (flaky holds) the gate must SKIP — fetched from the tests
    // domain (#2530). A skip is always VISIBLE, never silent (#3443).
    let quarantined = quarantined_cases();
    let q_names: Vec<&str> = quarantined.iter().map(|q| q.case.as_str()).collect();
    println!("{}", quarantine_report(&quarantined));

    println!(
        "-- werk-test #{} ({}) — {} unit(s), {} check(s){} --",
        card,
        role,
        units.len(),
        plan.len(),
        if self_mod { ", self-modifying → advisory" } else { "" }
    );

    // #3621 — canonical run evidence: started at plan time, completed ALWAYS.
    let started_at = std::time::Instant::now();
    // #3925 — runTs must be when the TEST RAN, not when the result was posted
    // (Wren's catch: #3941's land posted 20min after the suite finished, so
    // created-time already lies). Capture the wall clock ONCE at run start.
    let run_epoch_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if plan_source == "fallback" {
        emit_spine("test.plan.degraded", &role, &card, &trace,
            &[("reason", "tests-domain-unreachable"), ("plan", "legacy-lanes")]);
    }
    emit_spine(
        "test.started",
        &role,
        &card,
        &trace,
        &[
            ("units", &units.len().to_string()),
            ("checks_planned", &plan.len().to_string()),
            ("plan_source", plan_source),
        ],
    );
    // #3919 — the integration tier. Registered needs-stack tests inside the
    // selected units run ONLY with the live stack; without it they are a TYPED,
    // counted SKIPPED state — never green-by-default, never silence.
    let ns_all = werk_test::needs_stack_files(&rows);
    let in_units = |f: &str| units.iter().any(|u| match u {
        TestUnit::RustCrate(c) => f.starts_with(&format!("platform/services/{}/", c)),
        TestUnit::TsPackage(p) => f.starts_with(&format!("{}/", p)),
        TestUnit::BatsSuite(_) => false,
    });
    let selected_ns: Vec<String> = ns_all.iter().filter(|f| in_units(f)).cloned().collect();
    // count REGISTERED TESTS (rows), not files — the report must count what it names
    let selected_ns_tests = rows.iter()
        .filter(|r| r.hermeticity == "needs-stack" && in_units(&r.file_path))
        .count();
    let stack_down: Option<String> = if selected_ns.is_empty() {
        None
    } else {
        match werk_test::stack_verdict(&probe_stack().iter().map(|(n, o)| (n.as_str(), *o)).collect::<Vec<_>>()) {
            Ok(()) => None,
            Err(down) => Some(down),
        }
    };
    let ns_excluded: Vec<String> = if stack_down.is_some() { selected_ns.clone() } else { Vec::new() };
    println!("{}", werk_test::integration_report(selected_ns_tests, stack_down.as_deref()));
    if let Some(down) = &stack_down {
        emit_spine("test.integration.skipped", &role, &card, &trace,
            &[("count", &selected_ns_tests.to_string()), ("stack_down", down)]);
    }
    // #3920 — the browser lane: registered testConcern=ui files run as ONE
    // workspace check when the diff touches a ui surface. Stack-gated like any
    // needs-stack tier; zero registered = explicit absence, never vacuous.
    let ui_set = werk_test::ui_files(&rows);
    let ui_fired = werk_test::ui_lane_fires(&changed)
        || std::env::var("WERK_TEST_FULL").map(|v| v == "1").unwrap_or(false);
    let ui_check = werk_test::ui_plan(ui_fired, ui_set.len());
    let mut any_failed = false;
    let mut failed_count: usize = 0;
    // #3592 — every executed case, keyed to the registered identity, plus the
    // loud counter for cargo cases that can't be joined unambiguously.
    let mut all_cases: Vec<CaseResult> = Vec::new();
    let mut unmatched_cargo: usize = 0;
    let phase_started = std::time::Instant::now();
    let mut unit_costs: Vec<(String, f64)> = Vec::new();
    for check in &plan {
        let target = check.unit.as_ref().map(unit_name).unwrap_or("workspace");
        let check_started = std::time::Instant::now();
        let ok = match (&check.kind, &check.unit) {
            (CheckKind::CargoTest, Some(TestUnit::RustCrate(c))) => {
                // stack-down: this crate's needs-stack integration binaries
                // (tests/<stem>.rs → binary <stem>) drop out of the run, typed.
                let crate_prefix = format!("platform/services/{}/tests/", c);
                let ns_bins: Vec<String> = ns_excluded.iter()
                    .filter_map(|f| f.strip_prefix(&crate_prefix))
                    .filter_map(|rest| rest.strip_suffix(".rs"))
                    .filter(|stem| !stem.contains('/'))
                    .map(|s| s.to_string())
                    .collect();
                let ns_refs: Vec<&str> = ns_bins.iter().map(|s| s.as_str()).collect();
                let (ok, cases) = run_cargo(&werk, c, &q_names, &ns_refs);
                let crate_dir = format!("platform/services/{}", c);
                for (bare, result) in cases {
                    match match_cargo_case(&bare, &crate_dir, &rows, &row_names) {
                        Some(fp) => all_cases.push(CaseResult {
                            file_path: fp,
                            test_name: bare,
                            result,
                        }),
                        None => unmatched_cargo += 1,
                    }
                }
                ok
            }
            (CheckKind::Tsc, Some(TestUnit::TsPackage(p))) => run_tsc(&werk, p),
            (CheckKind::Jest, Some(TestUnit::TsPackage(p))) => {
                let (ok, cases) = match &jplan {
                    JestPlan::Selected(sels) => {
                        match sels.iter().find(|s| s.package == *p) {
                            Some(sel) => {
                                // #3919 — stack-down: needs-stack files leave the
                                // selection; the typed SKIPPED line above owns them.
                                let files: Vec<String> = sel.test_files.iter()
                                    .filter(|f| !ns_excluded.contains(f))
                                    .cloned().collect();
                                if files.is_empty() {
                                    println!("   jest:{} — selection all needs-stack, skipped typed (stack down)", p);
                                    (true, Vec::new())
                                } else {
                                    run_jest_selected(&werk, p, &files)
                                }
                            }
                            None => {
                                // A valid registry answer: nothing registered
                                // covers this diff in this package. Visible,
                                // never silent (#3443) — and the undeclared-gap
                                // channel above names unregistered files.
                                println!("   jest:{} — 0 registered unit tests cover the diff (selection empty)", p);
                                (true, Vec::new())
                            }
                        }
                    }
                    JestPlan::FullFallback { .. } => run_jest(&werk, p),
                };
                all_cases.extend(cases);
                ok
            }
            (CheckKind::Bats, Some(TestUnit::BatsSuite(s))) => run_bats(&werk, s),
            (CheckKind::ClippyRatchet, None) => run_clippy_ratchet(&werk),
            (CheckKind::LintRatchet, None) => werk_test::run_lint_ratchet(&werk),
            (CheckKind::DocCoherence, None) => run_doc_coherence(&werk),
            _ => true, // unreachable given check_plan's construction
        };
        unit_costs.push((format!("{}:{}", check.kind.label(), target),
            check_started.elapsed().as_secs_f64()));
        println!("   {}:{} … {}", check.kind.label(), target, if ok { "ok" } else { "FAIL" });
        if !ok {
            any_failed = true;
            failed_count += 1;
            emit_spine(
                "test.failed",
                &role,
                &card,
                &trace,
                &[("check", check.kind.label()), ("unit", target)],
            );
        }
    }

    if let Some(kind) = ui_check {
        if let Some(down) = &stack_down_ui(&selected_ns, &ns_all) {
            println!("   {}: SKIPPED (stack-down: {}) — {} registered ui file(s) not run; typed skip", kind.label(), down, ui_set.len());
            emit_spine("test.integration.skipped", &role, &card, &trace,
                &[("count", &ui_set.len().to_string()), ("stack_down", down), ("lane", "ui")]);
        } else {
            let (ok, summary) = run_ui_flows(&werk, &ui_set, &quarantined);
            println!("   {}:workspace … {}{}", kind.label(), if ok { "ok" } else { "FAIL" }, summary);
            if !ok {
                any_failed = true;
                failed_count += 1;
                emit_spine("test.failed", &role, &card, &trace,
                    &[("check", kind.label()), ("unit", "workspace")]);
            }
        }
    } else if ui_fired {
        println!("   ui-flows: none registered testConcern=ui — explicit absence (#3443)");
    }
    // #3953/#3955 — the test phase reports what it selects cost: elapsed vs the
    // table's in-run target (col3). Over-target WARNS LOUDLY (spine event + the
    // per-unit culprit table) and NEVER blocks a green run — Jeff's ruling after
    // run 66 failed 1642 green tests at 701s.
    let phase_elapsed = phase_started.elapsed().as_secs_f64();
    let budgets_path = format!("{}/platform/config/werk-phase-budgets.tsv",
        std::env::var("CHORUS_HOME").unwrap_or_default());
    let target = std::fs::read_to_string(&budgets_path).ok()
        .and_then(|t| werk_test::phase_target(&t, "test"));
    if let Some(budget) = target {
        if let Some(warning) = werk_test::budget_verdict(phase_elapsed, budget) {
            eprintln!("{}", warning);
            print!("{}", werk_test::unit_cost_report(&unit_costs));
            emit_spine("test.budget.blown", &role, &card, &trace,
                &[("elapsed_s", &format!("{:.0}", phase_elapsed)),
                  ("target_s", &format!("{:.0}", budget)),
                  ("largest", unit_costs.iter()
                      .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                      .map(|(n, _)| n.as_str()).unwrap_or(""))]);
        }
    }
    let outcome = gate_outcome(units.len(), any_failed, self_mod);
    let execution_duration_ms = started_at.elapsed().as_millis();
    let execution_extras = werk_test::completed_extras(
        &outcome,
        units.len(),
        plan.len(),
        failed_count,
        execution_duration_ms,
        self_mod,
    );
    let execution_refs: Vec<(&str, &str)> = execution_extras.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    emit_spine("test.execution.completed", &role, &card, &trace, &execution_refs);
    let writeback_started = std::time::Instant::now();
    // #3634 write side — the run becomes a TestSuiteRun instance in the graph.
    // Best-effort and WITNESSED either way: the gate's verdict never depends on
    // the write, but a skipped post is a spine event, not a silence.
    post_suite_run(&role, &card, &trace, plan_source, plan.len(), failed_count,
        execution_duration_ms, outcome.label());
    // #3592 — per-case wire-back. Unjoinable cargo cases are NAMED (spine),
    // never silently dropped.
    if unmatched_cargo > 0 {
        emit_spine("testresult.unmatched", &role, &card, &trace,
            &[("count", &unmatched_cargo.to_string()), ("kind", "cargo-ambiguous-or-unregistered")]);
    }
    // #3592 — a TestResult's ofTest edge is mandatory, so only cases that JOIN
    // to a registered Test are posted; executed-but-unregistered is its own
    // loud surface (the mirror of the reconcile gap), never a fabricated identity.
    let (joined, unregistered) = werk_test::join_cases(&all_cases, &rows, &row_names, &row_entities);
    if unregistered > 0 {
        emit_spine("testresult.unregistered", &role, &card, &trace,
            &[("count", &unregistered.to_string())]);
        println!("executed-but-unregistered: {} case(s) (no registered Test identity — not posted)", unregistered);
    }
    // #4015 — same rule on the card path as on the nightly: a run whose evidence
    // did not survive has not proven anything, so it must not exit clean.
    let stored = post_test_results(&role, &card, &trace, &joined, run_epoch_ms, 0);
    let lost = werk_test::results_lost(joined.len(), stored);
    if lost > 0 {
        println!(
            "!! werk-test: {} of {} results were NOT stored — this run cannot report on itself",
            lost, joined.len()
        );
        emit_spine("testresult.lost", &role, &card, &trace,
            &[("lost", &lost.to_string()), ("expected", &joined.len().to_string()),
              ("stored", &stored.to_string())]);
    }
    let writeback_duration_ms = writeback_started.elapsed().as_millis();
    let total_duration_ms = started_at.elapsed().as_millis();
    let mut completed = werk_test::completed_extras(
        &outcome,
        units.len(),
        plan.len(),
        failed_count,
        total_duration_ms,
        self_mod,
    );
    completed.push(("execution_duration_ms".into(), execution_duration_ms.to_string()));
    completed.push(("writeback_duration_ms".into(), writeback_duration_ms.to_string()));
    let completed_refs: Vec<(&str, &str)> = completed.iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    emit_spine("test.completed", &role, &card, &trace, &completed_refs);
    let exit = werk_test::run_exit_code(outcome.exit_code(), joined.len(), stored);
    if lost > 0 {
        println!("werk-test: RESULTS LOST — {} of {} (exit {})", lost, joined.len(), exit);
        return Ok(exit);
    }
    println!("werk-test: {} (exit {})", outcome.label(), exit);
    Ok(exit)
}

/// #3920 fold — the nightly cargo lane, through the ONE runner. Full selection
/// from the registry (every registered Rust crate), nextest execution, the
/// #3919 needs-stack typed skips, quarantine holds, and per-case TestResult
/// posts — identical mechanics to the gate, run against canonical with cardId
/// OMITTED (typed absence). `--crate=<name>` narrows to one crate so a red has
/// a one-command reproduction (`nightly-suites.sh --run-one cargo <dir>`).
/// Emits one machine line per crate (`nightly-unit|cargo|…`) that
/// nightly-suites.sh folds into its SUITE report — same verdict vocabulary,
/// no second walker.
fn run_nightly(args: &[String]) -> Result<i32, String> {
    let root = std::env::var("CHORUS_ROOT")
        .or_else(|_| std::env::var("CHORUS_HOME"))
        .map_err(|_| "nightly mode needs CHORUS_ROOT or CHORUS_HOME".to_string())?;
    if !Path::new(&root).is_dir() {
        return Err(format!("nightly root not found: {}", root));
    }
    let role = "system".to_string();
    let card = String::new(); // typed absence — a nightly run has no card
    let trace = std::env::var("CHORUS_TRACE_ID").unwrap_or_default();
    let only = flag_value(args, "--crate");

    let (rows, row_names, row_entities, plan_source) = fetch_test_rows();
    if werk_test::nightly_requires_model(plan_source) {
        // One selection engine: no glob fallback here — a degrade WOULD be the
        // second walker this mode retires. Refuse loudly; the shell wrapper
        // renders this as a red SUITE line the morning read can see.
        emit_spine("test.nightly.refused", &role, &card, &trace,
            &[("reason", "tests-domain-unreachable")]);
        return Err("nightly run requires the tests domain (one selection engine, no glob fallback) — fetch failed or empty; refusing loudly".into());
    }
    let crates: Vec<String> = werk_test::nightly_cargo_crates(&rows)
        .into_iter()
        .filter(|c| only.as_deref().map(|o| o == c).unwrap_or(true))
        .collect();
    if let Some(o) = &only {
        if crates.is_empty() {
            return Err(format!("--crate={} holds no registered tests", o));
        }
    }

    let quarantined = quarantined_cases();
    let q_names: Vec<&str> = quarantined.iter().map(|q| q.case.as_str()).collect();
    println!("{}", quarantine_report(&quarantined));

    // #3919 — the same typed integration tier as the gate: probe once, and a
    // down stack turns registered needs-stack tests into a counted SKIPPED
    // state per crate, never a fail and never silence.
    let ns_all = werk_test::needs_stack_files(&rows);
    let ns_total = rows.iter().filter(|r| r.hermeticity == "needs-stack").count();
    let stack_down: Option<String> = if ns_all.is_empty() {
        None
    } else {
        match werk_test::stack_verdict(&probe_stack().iter().map(|(n, o)| (n.as_str(), *o)).collect::<Vec<_>>()) {
            Ok(()) => None,
            Err(down) => Some(down),
        }
    };
    println!("{}", werk_test::integration_report(ns_total, stack_down.as_deref()));

    println!("-- werk-test --nightly — {} registered crate(s), full selection --", crates.len());
    let started_at = std::time::Instant::now();
    let run_epoch_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    emit_spine("test.started", &role, &card, &trace,
        &[("units", &crates.len().to_string()),
          ("checks_planned", &crates.len().to_string()),
          ("plan_source", plan_source),
          ("lane", "nightly-cargo")]);

    let mut any_failed = false;
    let mut failed_count = 0usize;
    // #4030 AC3 — results are stored PER UNIT, the moment a unit finishes, not
    // in one batch after the last lane. On 2026-08-30 03:00 the run executed
    // 1,127 cargo cases, then hung in the npm lane and was killed at the lane
    // cap: every verdict it had computed died with it ("stored 0"). Now a
    // killed run keeps everything it finished. The totals are atomics because
    // the pools post from their worker threads.
    use std::sync::atomic::{AtomicUsize, Ordering};
    let expected_total = AtomicUsize::new(0);
    let stored_total = AtomicUsize::new(0);
    let unregistered_total = AtomicUsize::new(0);
    let unmatched_cargo = AtomicUsize::new(0);
    // #3975 — the graph writes authenticate as the NIGHTLY machine principal
    // (least-privilege scope: the tests graph only). Spine events keep role
    // "system" — who acted vs which credential wrote are different facts.
    let mint_role = std::env::var("WERK_NIGHTLY_MINT_ROLE").unwrap_or_else(|_| "nightly".to_string());
    let store_unit = |unit: &str, cases: &[CaseResult]| {
        if cases.is_empty() {
            return;
        }
        let (joined, unregistered) = werk_test::join_cases(cases, &rows, &row_names, &row_entities);
        // #4033 — claim this unit's slice of the run's index space first, so
        // concurrent units never mint the same name (fetch_add is the claim).
        let idx_base = werk_test::claim_index_base(&expected_total, joined.len());
        let stored = post_test_results(&mint_role, &card, &trace, &joined, run_epoch_ms, idx_base);
        stored_total.fetch_add(stored, Ordering::SeqCst);
        unregistered_total.fetch_add(unregistered, Ordering::SeqCst);
        println!("nightly-stored|{}|{} of {}", unit, stored, joined.len());
    };
    // #3974 — npm lane: every registered TS/node package, full selection.
    // jest packages run jest; non-jest packages run their own `npm test`
    // (never a vacuous green). needs-stack files leave the run typed when
    // the stack is down, same vocabulary as the cargo lane.
    let ts_pkgs: Vec<String> = werk_test::nightly_ts_packages(&rows)
        .into_iter()
        .filter(|p| only.as_deref().map(|o| o == p).unwrap_or(true))
        .collect();
    // #3974 — bats lane: registered suites from the registry, per-case TAP
    // results (boolean-only bats is over).
    let bats_suites: Vec<String> = werk_test::nightly_bats_suites(&rows)
        .into_iter()
        .filter(|b| only.as_deref().map(|o| o == b).unwrap_or(true))
        .collect();
    // #3922 — security-declared units fold under their own lane label so the
    // report and owner routing see ONE security lane on its own cadence.
    let sec_units = werk_test::security_units(&rows);
    let bats_kind = |b: &str| -> &'static str {
        if sec_units.contains(b) { "security" } else if b.ends_with(".sh") { "shell" } else { "bats" }
    };
    // #4030 AC4 — the PLAN, printed before any lane runs. A planned unit that
    // never produces its `nightly-unit|` line is folded by nightly-suites.sh
    // into a red NEVER RAN row (`never_ran_units`): a run killed at a cap can
    // no longer report only the units it got to and read as "3 red".
    for c in &crates {
        println!("{}", werk_test::nightly_plan_line("cargo", c));
    }
    for p in &ts_pkgs {
        let k = if sec_units.contains(p) { "security" } else { "npm" };
        println!("{}", werk_test::nightly_plan_line(k, p));
    }
    for b in &bats_suites {
        println!("{}", werk_test::nightly_plan_line(bats_kind(b), b));
    }
    // #4022 — the cargo lane was 24 serial `cargo nextest` invocations against
    // an already-warm shared target dir; pool them. cargo's own flock still
    // serializes any cold BUILD, so contention degrades to the old timing,
    // never to corruption. Worker count is deliberately smaller than the bats
    // pool — nextest is internally parallel, so crates multiply CPU.
    // #4022 — load-aware widths: each lane gets a share of the box, and no pool
    // takes a new unit while the 1-minute load is over cap (2× cores). Env
    // overrides keep the old knobs; the defaults are the box's.
    let budget = werk_test::cpu_budget();
    let (cw_default, nextest_threads, nw_default, jest_workers, bats_default) = werk_test::lane_widths(budget);
    let cap: f64 = std::env::var("NIGHTLY_LOAD_CAP").ok().and_then(|v| v.parse().ok())
        .unwrap_or_else(|| werk_test::load_cap(budget));
    let gate_wait = std::time::Duration::from_secs(
        std::env::var("NIGHTLY_GATE_MAX_WAIT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(180));
    let gate_tick = std::time::Duration::from_secs(5);
    println!("-- #4022 load-aware: budget {} cores, cap load {:.0}, cargo {}×{} threads, npm {}×{} jest workers, bats {} --",
        budget, cap, cw_default, nextest_threads, nw_default, jest_workers, bats_default);
    let cargo_workers: usize = std::env::var("NIGHTLY_CARGO_WORKERS").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(cw_default);
    if std::env::var("NIGHTLY_NEXTEST_THREADS").is_err() {
        std::env::set_var("NIGHTLY_NEXTEST_THREADS", nextest_threads.to_string());
    }
    let ns_bins_for = |c: &str| -> Vec<String> {
        if stack_down.is_some() {
            let crate_prefix = format!("platform/services/{}/tests/", c);
            ns_all.iter()
                .filter_map(|f| f.strip_prefix(&crate_prefix))
                .filter_map(|rest| rest.strip_suffix(".rs"))
                .filter(|stem| !stem.contains('/'))
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        }
    };
    let cargo_root = root.clone();
    let (cargo_results, cargo_waits) = werk_test::run_pool_gated(&crates, cargo_workers, cap, read_loadavg, gate_wait, gate_tick, |c| {
        let ns_bins = ns_bins_for(c);
        let ns_refs: Vec<&str> = ns_bins.iter().map(|s| s.as_str()).collect();
        let (ok, cases) = run_cargo(&cargo_root, c, &q_names, &ns_refs);
        // #4030 AC3 — join + store THIS crate's cases now, in the worker
        let crate_dir = format!("platform/services/{}", c);
        let mut matched: Vec<CaseResult> = Vec::new();
        for (bare, result) in &cases {
            match match_cargo_case(bare, &crate_dir, &rows, &row_names) {
                Some(fp) => matched.push(CaseResult { file_path: fp, test_name: bare.clone(), result: result.clone() }),
                None => { unmatched_cargo.fetch_add(1, Ordering::SeqCst); }
            }
        }
        store_unit(c, &matched);
        ((ok, cases), ns_bins.len())
    });
    for (c, ((ok, cases), ns_len)) in cargo_results {
        let c = &c;
        let passed = cases.iter().filter(|(_, r)| r == "pass").count();
        let case_failed = cases.iter().filter(|(_, r)| r != "pass").count();
        println!("{}", werk_test::nightly_unit_line(c, ok, passed, case_failed, ns_len));
        if !ok {
            any_failed = true;
            failed_count += 1;
            emit_spine("test.failed", &role, &card, &trace,
                &[("check", "cargo"), ("unit", c)]);
        }
    }

    // #3559/#3974 — platform/api's INTEGRATION jest project is only
    // constructed under RUN_INTEGRATION=true; the nightly sets it from the
    // live stack probe so integration tests run with the stack and are
    // typed-absent without it — the wrapper's old per-package env is retired.
    if stack_down.is_none() {
        std::env::set_var("RUN_INTEGRATION", "true");
    }
    // #4022 — npm lane pooled at 2: jest is internally parallel, so two
    // concurrent packages already saturate; more just multiplies load (the
    // 6-worker take pegged the box to 194).
    let npm_workers: usize = std::env::var("NIGHTLY_NPM_WORKERS").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(nw_default);
    let npm_root = root.clone();
    let (npm_results, npm_waits) = werk_test::run_pool_gated(&ts_pkgs, npm_workers, cap, read_loadavg, gate_wait, gate_tick,
        |p| {
            let (ok, cases) = run_jest_with(&npm_root, p, Some(jest_workers));
            // #4030 AC3 — stored the moment the package finishes (foreign-file
            // cases included: they join by their own file path)
            store_unit(p, &cases);
            (ok, cases)
        });
    for (p, (ok, cases)) in npm_results {
        let p = &p;
        let pkg_ns: Vec<String> = if stack_down.is_some() {
            ns_all.iter().filter(|f| f.starts_with(&format!("{}/", p))).cloned().collect()
        } else { Vec::new() };
        // #4004 — attribute each case to the package its FILE lives in, not to
        // the package the runner happened to invoke. Kade read "cards 7 fail /
        // clearing 1 fail" and found the failing cases were
        // platform/api/tests/*.integration.test.ts: jest's rootDir can reach
        // past the package dir, so another package's results land on this row
        // (the nightly claimed 609 tests for cards; cards alone runs 529 green).
        // A count under the wrong name sends the wrong owner hunting through a
        // suite that is not red.
        let (mine, foreign): (Vec<_>, Vec<_>) = cases
            .into_iter()
            .partition(|c| werk_test::package_owns_case(p, &c.file_path));
        if !foreign.is_empty() {
            let mut owners: Vec<&str> = foreign
                .iter()
                .map(|c| c.file_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("?"))
                .collect();
            owners.sort_unstable();
            owners.dedup();
            println!(
                "!! jest:{} ran {} case(s) whose files live OUTSIDE it ({}) — not counted on this row",
                p, foreign.len(), owners.join(", ")
            );
        }
        if mine.is_empty() && !foreign.is_empty() {
            // never let a package read as simply empty when its row ran nothing of its own
            println!("!! jest:{} produced NO cases of its own — every result came from elsewhere", p);
        }
        let cases = mine;
        let passed = cases.iter().filter(|c| c.result == "pass").count();
        let case_failed = cases.iter().filter(|c| c.result != "pass").count();
        let npm_kind = if werk_test::security_units(&rows).contains(p) { "security" } else { "npm" };
        // #4063 — name every failed case before the fold line, so a red row
        // points at a test, not at a count.
        for l in werk_test::failed_case_lines(&format!("jest:{}", p), &cases) {
            println!("{}", l);
        }
        println!("{}", werk_test::nightly_lane_line(npm_kind, p, ok, passed, case_failed, pkg_ns.len()));
        if !ok {
            any_failed = true;
            failed_count += 1;
            emit_spine("test.failed", &role, &card, &trace, &[("check", "npm"), ("unit", p)]);
        }
    }

    // #4030 AC3 — one bats runner for the three pools: run, then store now.
    let run_bats_stored = |werk: &str, b: &str| -> (bool, Vec<(String, String)>, String) {
        let r = run_bats_cases(werk, b);
        let cases: Vec<CaseResult> = r.1.iter()
            .map(|(n, res)| CaseResult { file_path: b.to_string(), test_name: n.clone(), result: res.clone() })
            .collect();
        store_unit(b, &cases);
        r
    };
    // #4022 — the lane's suites are independent subprocesses; fan them out.
    // A suite is serialized when a registered file of its is needs-stack or it
    // is named in the isolation conf (a suite that mutates the shared stack
    // must never overlap anything). Report order stays the plan's order —
    // run_pool returns input order regardless of completion order.
    let iso_conf = std::fs::read_to_string(
        Path::new(&root).join("platform/scripts/nightly-isolation.conf"))
        .unwrap_or_default();
    let explicit_iso: Vec<String> = iso_conf.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let plan = werk_test::plan_parallel_units(&bats_suites,
        &|u| werk_test::unit_is_isolated(u, &rows, &explicit_iso));
    let workers: usize = std::env::var("NIGHTLY_SUITE_WORKERS").ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(bats_default);
    // #4022 second cut — the serialized tail was 50 suites at width 1. Only
    // conf-listed MUTATORS truly need to run alone; needs-stack READERS
    // (probes, health checks) overlap each other safely at width 2.
    let (stack_readers, mutators) = werk_test::split_serialized(&plan.serialized, &explicit_iso);
    println!("-- #4022 parallel plan: {} suites fan out across {} workers, {} stack-readers at 2, {} mutators alone --",
        plan.parallel.len(), workers, stack_readers.len(), mutators.len());
    let pool_root = root.clone();
    let (mut lane_results, bats_waits): (Vec<(String, (bool, Vec<(String, String)>, String))>, usize) =
        werk_test::run_pool_gated(&plan.parallel, workers, cap, read_loadavg, gate_wait, gate_tick, |b| run_bats_stored(&pool_root, b));
    let reader_root = root.clone();
    let (reader_results, reader_waits) =
        werk_test::run_pool_gated(&stack_readers, 2, cap, read_loadavg, gate_wait, gate_tick, |b| run_bats_stored(&reader_root, b));
    lane_results.extend(reader_results);
    println!("-- #4022 load gate: held {} time(s) at load > {:.0} (cargo {}, npm {}, bats {}) --",
        cargo_waits + npm_waits + bats_waits + reader_waits, cap, cargo_waits, npm_waits, bats_waits + reader_waits);
    for b in &mutators {
        lane_results.push((b.clone(), run_bats_stored(&root, b)));
    }
    for (b, (ok, cases, text)) in lane_results {
        let b = &b;
        let kind = bats_kind(b);
        // #4065 — a suite that DECLINED to run (rc=3, e.g. test-product-membrane
        // refusing to boot out live agents unattended, #4004) is neither pass
        // nor fail. Its one synthetic "skip" case used to count as a FAIL here,
        // so the row read "pass | 0 pass, 1 fail" — the reporter contradiction
        // #3753 flagged every night. It is now its own verdict: skip.
        if werk_test::is_self_refused(&cases) {
            println!("{}", werk_test::nightly_lane_line_refused(kind, b));
            continue;
        }
        let (passed, case_failed) = if cases.is_empty() && kind == "shell" {
            // shell suites report summary counts, not TAP cases
            werk_test::parse_shell_counts(&text)
                .unwrap_or(if ok { (1, 0) } else { (0, 1) })
        } else {
            (cases.iter().filter(|(_, r)| r == "pass").count(),
             cases.iter().filter(|(_, r)| r != "pass" && r != "skip").count())
        };
        println!("{}", werk_test::nightly_lane_line(kind, b, ok, passed, case_failed, 0));
        if !ok {
            any_failed = true;
            failed_count += 1;
            emit_spine("test.failed", &role, &card, &trace, &[("check", "bats"), ("unit", b)]);
        }
    }
    if werk_test::security_rows(&rows).is_empty() {
        println!("security-lane: none registered testConcern=security — explicit absence (#3443/#3922)");
    }
    let total_units = crates.len() + ts_pkgs.len() + bats_suites.len();

    let outcome = gate_outcome(total_units, any_failed, false);
    let execution_duration_ms = started_at.elapsed().as_millis();
    let execution_extras = werk_test::completed_extras(
        &outcome,
        total_units,
        total_units,
        failed_count,
        execution_duration_ms,
        false,
    );
    let execution_refs: Vec<(&str, &str)> = execution_extras.iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    emit_spine("test.execution.completed", &role, &card, &trace, &execution_refs);
    let writeback_started = std::time::Instant::now();
    post_suite_run(&mint_role, &card, &trace, plan_source, crates.len(), failed_count,
        execution_duration_ms, outcome.label());
    let unmatched_cargo = unmatched_cargo.load(Ordering::SeqCst);
    if unmatched_cargo > 0 {
        emit_spine("testresult.unmatched", &role, &card, &trace,
            &[("count", &unmatched_cargo.to_string()), ("kind", "cargo-ambiguous-or-unregistered")]);
    }
    let unregistered = unregistered_total.load(Ordering::SeqCst);
    if unregistered > 0 {
        emit_spine("testresult.unregistered", &role, &card, &trace,
            &[("count", &unregistered.to_string())]);
    }
    // #4015 — the run's verdict depends on its evidence surviving: on
    // 2026-08-27 the nightly executed 7,411 tests, stored NONE, and exited 0.
    // #4030 — the posts already happened per unit; this is the ledger of them.
    let expected = expected_total.load(Ordering::SeqCst);
    let stored = stored_total.load(Ordering::SeqCst);
    let lost = werk_test::results_lost(expected, stored);
    if lost > 0 {
        println!(
            "!! werk-test: {} of {} results were NOT stored — this run cannot report on itself",
            lost, expected
        );
        emit_spine("testresult.lost", &role, &card, &trace,
            &[("lost", &lost.to_string()), ("expected", &expected.to_string()),
              ("stored", &stored.to_string())]);
    }
    println!("nightly-stored|run|{} of {}", stored, expected);
    let writeback_duration_ms = writeback_started.elapsed().as_millis();
    let total_duration_ms = started_at.elapsed().as_millis();
    let mut completed = werk_test::completed_extras(
        &outcome,
        total_units,
        total_units,
        failed_count,
        total_duration_ms,
        false,
    );
    completed.push(("execution_duration_ms".into(), execution_duration_ms.to_string()));
    completed.push(("writeback_duration_ms".into(), writeback_duration_ms.to_string()));
    let completed_refs: Vec<(&str, &str)> = completed.iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    emit_spine("test.completed", &role, &card, &trace, &completed_refs);
    // #4022 AC3/AC4 — elapsed against Jeff's bar (default 15m), on the spine so
    // drift is visible, and a breach is loud in the run's own output.
    let bar_secs: u64 = std::env::var("NIGHTLY_ELAPSED_BAR_SECS").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(900);
    let elapsed_secs = (total_duration_ms / 1000) as u64;
    emit_spine("test.nightly.elapsed", &role, &card, &trace,
        &[("elapsed_secs", &elapsed_secs.to_string()), ("bar_secs", &bar_secs.to_string()),
          ("over_bar", if elapsed_secs > bar_secs { "true" } else { "false" })]);
    if let Some(breach) = werk_test::elapsed_breach(elapsed_secs, bar_secs) {
        println!("!! {}", breach);
        emit_spine("test.nightly.over_bar", &role, &card, &trace,
            &[("elapsed_secs", &elapsed_secs.to_string()), ("bar_secs", &bar_secs.to_string())]);
    }
    let exit = werk_test::run_exit_code(outcome.exit_code(), expected, stored);
    if lost > 0 {
        println!("werk-test: RESULTS LOST — {} of {} (exit {})", lost, expected, exit);
        return Ok(exit);
    }
    println!("werk-test: {} (exit {})", outcome.label(), exit);
    Ok(exit)
}

fn unit_name(u: &TestUnit) -> &str {
    match u {
        TestUnit::RustCrate(n) => n,
        TestUnit::TsPackage(p) => p,
        TestUnit::BatsSuite(s) => s,
    }
}

/// #3917 — build the script→suite coverage index by reading each bats suite and
/// recording the repo-relative script paths its body names. Deliberately textual:
/// a suite that runs a script names it, and the tests-domain graph (stage 2) is
/// where this becomes a declared `covers` edge rather than a read.
fn build_suite_coverage(werk: &str) -> Vec<werk_test::SuiteCoverage> {
    let dir = std::path::Path::new(werk).join("platform/tests");
    let mut rows = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return rows,
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "bats").unwrap_or(false))
        .collect();
    paths.sort();
    for path in paths {
        let body = match std::fs::read_to_string(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let suite = format!(
            "platform/tests/{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        let mut covers: Vec<String> = Vec::new();
        for tok in body.split(|c: char| !(c.is_alphanumeric() || "._/-".contains(c))) {
            // #3934 — governed surfaces (workflow yml, hooks) count as coverage
            // targets too: a suite that greps werk.yml is ABOUT werk.yml.
            if tok.contains(".github/workflows/") || tok.contains("platform/hooks/") {
                let rel = match tok.find(".github/workflows/").or_else(|| tok.find("platform/hooks/")) {
                    Some(i) => tok[i..].to_string(),
                    None => continue,
                };
                if !covers.contains(&rel) {
                    covers.push(rel);
                }
                continue;
            }
            if tok.ends_with(".sh") {
                let rel = match tok.find("platform/") {
                    Some(i) => tok[i..].to_string(),
                    None => continue,
                };
                if !covers.contains(&rel) {
                    covers.push(rel);
                }
            }
        }
        rows.push(werk_test::SuiteCoverage { suite, covers });
    }
    rows
}

/// #3974 — bats with per-case capture: same suite-world as run_bats, but the
/// TAP output becomes per-case results for the wire-back.
fn run_bats_cases(werk: &str, suite: &str) -> (bool, Vec<(String, String)>, String) {
    let tmp = std::env::temp_dir().join(format!("werk-test-bats-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    // #3974 — one script-suite variant, two runners: .bats via bats, .sh via
    // bash (the shell tier's suites). Shell output is summary-grain (counted
    // in the lane line via parse_shell_counts); TAP suites get per-case rows.
    let mut cmd = if suite.ends_with(".sh") {
        let mut c = Command::new("bash");
        c.arg(suite);
        c
    } else {
        let mut c = Command::new("bats");
        c.arg(suite);
        c
    };
    cmd.current_dir(werk).env("CHORUS_CONTEXT", "");
    apply_suite_world(&mut cmd, werk);
    // #4022 / TD-028 — suite output goes to a FILE and the runner waits on
    // CHILD EXIT with a deadline, never on pipe-EOF. Twice today a finished
    // suite's leaked server (test-share-path-prefix's http.server, the crawler
    // bats wedge) inherited the output pipe and hung the run for as long as
    // anyone let it; a file leaves nothing to hold, and a suite that outlives
    // its budget is killed and scored failed, loudly.
    let suite_slug: String = suite.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let out_path = tmp.join(format!("suite-{}.out", suite_slug));
    // #4035 — the UNIT cap, never the wrapper's LANE vocabulary. nightly-suites.sh
    // env-prefixes NIGHTLY_SUITE_TIMEOUT=<lane cap, 7200s> onto the werk-test
    // invocation for its own _run_capped, and the export reaches this process:
    // reading it here gave EVERY suite a 2-hour deadline. 2026-08-30: trivy hung,
    // burned hours inside the 2-wide pool, and three daytime runs went ~2h.
    let timeout_secs: u64 = werk_test::unit_timeout().as_secs();
    // #4030 — the ONE deadline primitive (process-group kill, so a suite's
    // forked children die with it); jest and npm-test units share it.
    let outcome = std::fs::File::create(&out_path)
        .map_err(|e| e.to_string())
        .and_then(|f| {
            let ferr = f.try_clone().map_err(|e| e.to_string())?;
            cmd.stdout(f).stderr(ferr);
            werk_test::run_with_deadline(&mut cmd, std::time::Duration::from_secs(timeout_secs))
        })
        .map(|fin| (fin.code, fin.success, fin.timed_out));
    match outcome {
        Ok((code, success, timed_out)) => {
            let mut text = std::fs::read_to_string(&out_path).unwrap_or_default();
            let _ = std::fs::remove_file(&out_path);
            if timed_out {
                text.push_str(&format!(
                    "\nSUITE TIMED OUT after {}s — killed by the runner (deadline is child-exit, not pipe-EOF)\n",
                    timeout_secs));
                eprintln!("!! {} timed out after {}s — killed", suite, timeout_secs);
                return (false, werk_test::parse_bats_cases(&text), text);
            }
            // #4016 — rc=3 is a suite's SELF-REFUSAL ("I must not run here"),
            // not a failure. nightly-suites.sh learned this in #4004; this
            // runner never did, and the nightly uses THIS one — so a correctly
            // refusing suite (test-product-membrane, which bootouts every agent
            // and needs explicit authority) kept reporting "0 pass, 1 fail".
            // Two scorers, one taught. Both must agree or the fix is invisible.
            let refused = code == Some(3);
            let ok = success || refused;
            if !ok {
                // #4065 — the failing suite's own last lines go to STDOUT, each
                // prefixed with the suite path, so nightly-suites.sh's per-unit
                // fail log (which greps the lane output for lines naming the
                // unit) carries the CAUSE, not just the verdict. On stderr they
                // were lost: test-role-state-spine.sh was red at 03:00 and green
                // by hand for days with a fail log that said only "0 pass, 2 fail".
                let tail: Vec<&str> = text.lines().rev().take(40).collect();
                for line in tail.into_iter().rev() {
                    println!("{} | {}", suite, line);
                }
            }
            let mut cases = werk_test::parse_bats_cases(&text);
            if refused && cases.is_empty() {
                cases.push((
                    format!("SELF-REFUSED rc=3 — {} declined to run here", suite),
                    "skip".to_string(),
                ));
            }
            (ok, cases, text)
        }
        Err(_) => (false, Vec::new(), String::new()),
    }
}

/// Run one bats suite. The suite gets its own world (#3528/#3615): CHORUS_LOG_FILE
/// points into a per-run tempdir so a suite that emits to the spine cannot write
/// the production log from a build context.
fn run_bats(werk: &str, suite: &str) -> bool {
    let tmp = std::env::temp_dir().join(format!("werk-test-bats-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    // #4004 — a .sh suite is EXECUTED by bash, never handed to bats. run_bats_cases
    // (the nightly lane) already branches this way; this path — the diff-selected
    // lane every werk run takes — did not, so shell suites went through `bats
    // file.sh`. bats discovers tests by SOURCING the file (bats-gather-tests),
    // which runs the whole suite inside bats' own errexit shell: the first
    // recorded pass aborted it, and the red was reported against a synthetic
    // test named "bats-gather-tests". That is why 28 suites read red in the werk
    // pipeline while every one of them passes when run directly.
    let runner = if suite.ends_with(".sh") { "bash" } else { "bats" };
    status_ok(
        Command::new(runner)
            .arg(suite)
            .current_dir(werk)
            // #3918 — the child is a TEST: clear the runner's prod declaration so
            // the membrane classifies it from its own ambient markers and still
            // refuses it the production spine.
            .env("CHORUS_CONTEXT", "")
            .env("CHORUS_ROOT", werk)
            .env("CHORUS_LOG_FILE", tmp.join("spine.log")),
    )
}

/// #3661 — `--flag=value` extraction (the verb's positional parse filters all
/// `--` args, so flags carry their value inline; a bare `--flag` is ignored).
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let prefix = format!("{}=", flag);
    args.iter()
        .find_map(|a| a.strip_prefix(&prefix))
        .map(|v| v.to_string())
        .filter(|v| !v.is_empty())
}

/// #3661 AC3 — the on-disk test files of the planned units, repo-relative, by
/// the same conventions the registration crawl uses: `tests/**/*.rs` for a
/// crate, `tests/**/*.test.ts` for a TS package. node_modules never entered.
fn on_disk_test_files(werk: &str, units: &[TestUnit]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for unit in units {
        let (dir, suffix): (String, &str) = match unit {
            TestUnit::RustCrate(c) => (format!("platform/services/{}/tests", c), ".rs"),
            TestUnit::TsPackage(p) => (format!("{}/tests", p), ".test.ts"),
            // A bats suite IS its own file — there is no directory to crawl, and
            // the file is already the unit. Skip rather than invent a convention.
            TestUnit::BatsSuite(s) => {
                if !found.contains(s) {
                    found.push(s.clone());
                }
                continue;
            }
        };
        collect_files(werk, &dir, suffix, &mut found);
    }
    found
}

fn collect_files(werk: &str, rel_dir: &str, suffix: &str, out: &mut Vec<String>) {
    collect_files_depth(werk, rel_dir, suffix, out, 0);
}

/// Depth-capped, symlink-blind walk (gather hardening, silas): a symlinked
/// tests/ subdir can't loop the walker, and 8 levels is far beyond any real
/// test tree — hitting the cap just stops descending, never errors the gate.
const MAX_WALK_DEPTH: u32 = 8;

fn collect_files_depth(werk: &str, rel_dir: &str, suffix: &str, out: &mut Vec<String>, depth: u32) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let abs = format!("{}/{}", werk, rel_dir);
    let entries = match std::fs::read_dir(&abs) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "node_modules" || name.starts_with('.') {
            continue;
        }
        let rel = format!("{}/{}", rel_dir, name);
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            collect_files_depth(werk, &rel, suffix, out, depth + 1);
        } else if name.ends_with(suffix) {
            out.push(rel);
        }
    }
}

/// Changed files on the card's diff: `git diff --name-only <merge-base> HEAD`,
/// merge-base against origin/main (falls back to HEAD~1, like #3397).
fn git_changed_files(werk: &str) -> Result<Vec<String>, String> {
    let base = Command::new("git")
        .args(["-C", werk, "merge-base", "origin/main", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "HEAD~1".to_string());
    let out = Command::new("git")
        .args(["-C", werk, "diff", "--name-only", &format!("{}..HEAD", base)])
        .output()
        .map_err(|e| format!("git diff failed: {}", e))?;
    if !out.status.success() {
        return Err("git diff returned non-zero".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// `cargo test --lib --bins` in the crate dir, iff it has a Cargo.toml (a path
/// match without a manifest is skipped = pass; nothing to run). Quarantined case
/// names are appended as `-- --skip <case>` (#2530) so a flaky hold can't block the
/// #3892 — spawn suites with a self-contained world (tempdir overrides for
/// every overridable membrane surface) so a lazy subprocess test never panics
/// MEMBRANE REFUSED into the werk log. Explicit env wins: a var the caller
/// already set is left alone, so fixtures/integration setups keep control.
fn apply_suite_world(cmd: &mut Command, werk: &str) {
    // OUTSIDE the werk tree: an untracked dir inside it would trip the
    // teardown's refuse-if-dirty at accept (#3431).
    let slot = Path::new(werk).file_name().and_then(|s| s.to_str()).unwrap_or("werk");
    let tmp = std::env::temp_dir().join(format!("werk-suite-world-{slot}")).to_string_lossy().into_owned();
    let _ = std::fs::create_dir_all(&tmp);
    for (k, v) in werk_test::suite_world_env(&tmp) {
        if std::env::var(&k).is_err() {
            cmd.env(k, v);
        }
    }
}

/// #3929 — probe `cargo nextest --version` ONCE per process against the pin in
/// `<werk>/.config/nextest.toml`. Absence, staleness, or a missing pin all
/// refuse the whole cargo lane loudly; there is no fallback to `cargo test`.
/// #3919 — probe the live stack the needs-stack tier depends on. Overridable
/// via WERK_STACK_PROBES="name=url,name=url" so tests bring their own world;
/// defaults to the two services the registered integration tests actually hit.
fn probe_stack() -> Vec<(String, bool)> {
    let spec = std::env::var("WERK_STACK_PROBES").unwrap_or_else(|_|
        "chorus-api=http://localhost:3340/api/chorus/context/health,athena-make=http://localhost:3360/".to_string());
    spec.split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, url)| {
            let ok = Command::new("curl")
                .args(["-sf", "--max-time", "3", "-o", "/dev/null", url])
                .status().map(|s| s.success()).unwrap_or(false);
            (name.trim().to_string(), ok)
        })
        .collect()
}

fn nextest_gate(werk: &str) -> &'static Result<(), String> {
    static GATE: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    GATE.get_or_init(|| {
        let pin_path = format!("{}/.config/nextest.toml", werk);
        let pin = std::fs::read_to_string(&pin_path).ok()
            .and_then(|t| werk_test::parse_nextest_pin(&t))
            .ok_or_else(|| format!("nextest-pin-missing: no nextest-version in {}", pin_path))?;
        let mut cmd = Command::new("cargo");
        cmd.args(["nextest", "--version"]).current_dir(werk);
        match cmd.output() {
            Ok(o) => {
                let text = format!("{}{}",
                    String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
                werk_test::classify_nextest_probe(o.status.success(), &text, pin)
            }
            Err(e) => Err(format!("nextest-probe-spawn-failed: {}", e)),
        }
    })
}

/// gate; an empty quarantine set leaves the invocation byte-identical.
fn run_cargo(werk: &str, name: &str, quarantined: &[&str], ns_bins: &[&str]) -> (bool, Vec<(String, String)>) {
    let dir = format!("{}/platform/services/{}", werk, name);
    if !Path::new(&format!("{}/Cargo.toml", dir)).is_file() {
        return (true, Vec::new());
    }
    if let Err(reason) = nextest_gate(werk) {
        eprintln!("REFUSED cargo lane for {}: {}", name, reason);
        return (false, Vec::new());
    }
    // #4022 — each crate gets its share of the CPU budget (see lane_widths);
    // NIGHTLY_NEXTEST_THREADS is set by the nightly lane, absent for card runs.
    let threads = std::env::var("NIGHTLY_NEXTEST_THREADS").ok().and_then(|v| v.parse().ok());
    let mut args: Vec<String> = werk_test::nextest_run_args_threads(quarantined, ns_bins, threads);
    // #3955 — the ONE nextest config (pin + serial-e2e groups) lives at the werk
    // root; per-crate runs resolve config from the CRATE dir, so pass it
    // explicitly or the serial-e2e grouping silently never applies.
    let cfg = format!("{}/.config/nextest.toml", werk);
    if Path::new(&cfg).is_file() {
        args.insert(2, "--config-file".to_string());
        args.insert(3, cfg);
    }
    // #3592 — capture instead of inherit: per-case lines feed TestResult emit.
    // Failure output is still shown (tail), honest-red stays visible.
    let mut cmd = Command::new("cargo");
    // #3918 — test child: cleared, so the membrane still refuses it (see child_context).
    cmd.env("CHORUS_CONTEXT", "");
    cmd.args(&args).current_dir(&dir);
    apply_suite_world(&mut cmd, werk);
    match cmd.output() {
        Ok(o) => {
            let ok = o.status.success();
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            if !ok {
                let lines: Vec<&str> = text.lines().collect();
                let start = lines.len().saturating_sub(60);
                eprintln!("{}", lines[start..].join("\n"));
            }
            (ok, werk_test::parse_nextest_cases(&text))
        }
        Err(_) => (false, Vec::new()),
    }
}

/// Fetch the quarantined test cases from the tests domain (athena-make `/tests`), via a
/// curl|jq subprocess so the verb stays zero-dep/std-only (ADR-032 §6, same pattern
/// as `emit_spine`). Best-effort: any failure (endpoint down, jq absent) yields an
/// EMPTY set — quarantine never blocks the gate from running, it only relaxes it.
/// Each row is `testName\treason\tuntil`. (Server-side `?quarantined=true` filtering
/// is a follow-on; today we pull and filter client-side.)
fn quarantined_cases() -> Vec<Quarantined> {
    let endpoint = std::env::var("OWL_API_TESTS")
        .unwrap_or_else(|_| "http://localhost:3360/tests?limit=10000".to_string());
    // #3766 — athena-make serves quarantined as the STRING "true" (SHACL string field),
    // so a boolean-only compare NEVER matched: the quarantine gate was vacuous from
    // the day it shipped ("quarantined: none" = type mismatch, not an empty set).
    // Found live 2026-08-06 by writing a quarantine row and running this exact jq.
    let jq = r#".data[] | select(.quarantined==true or .quarantined=="true") | [.testName,.quarantineReason,.quarantineUntil] | @tsv"#;
    let pipe = format!("curl -s '{}' | jq -r '{}'", endpoint, jq);
    let out = match Command::new("bash").args(["-c", &pipe]).output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    parse_quarantine_rows(&String::from_utf8_lossy(&out))
}

/// `tsc --noEmit` per TS package. Shares the dep-availability guard with jest:
/// if a CHANGED package's deps can't be provided, FAIL LOUD (the #3190 false-green
/// anti-pattern: honest-red beats lying-green).
fn run_tsc(werk: &str, pkg: &str) -> bool {
    let pkg_dir = format!("{}/{}", werk, pkg);
    if !ensure_ts_deps(werk, pkg) {
        eprintln!("!! tsc:{} CHANGED but deps unavailable — FAIL LOUD", pkg);
        return false;
    }
    let tsc = format!("{}/node_modules/.bin/tsc", pkg_dir);
    if !Path::new(&tsc).exists() {
        return true; // package has no local tsc → nothing to typecheck here
    }
    status_ok(
        Command::new(&tsc)
            .arg("--noEmit")
            .current_dir(&pkg_dir)
            // #3918 — test child: cleared (see child_context).
            .env("CHORUS_CONTEXT", ""),
    )
}

/// `jest --ci` per TS package, deps guarded the same way (#3397).
/// #3592 — `--json` capture: stdout is the machine result (per-case identity →
/// TestResult emit), progress/failures stay on stderr and are echoed on red.
fn run_jest(werk: &str, pkg: &str) -> (bool, Vec<CaseResult>) {
    run_jest_with(werk, pkg, None)
}

/// #4022 — jest with its share of the CPU budget (`--maxWorkers N`); None keeps
/// jest's default (a worker per core), which is what pegged the box.
fn run_jest_with(werk: &str, pkg: &str, max_workers: Option<usize>) -> (bool, Vec<CaseResult>) {
    let pkg_dir = format!("{}/{}", werk, pkg);
    if !ensure_ts_deps(werk, pkg) {
        eprintln!("!! jest:{} CHANGED but deps unavailable — FAIL LOUD", pkg);
        return (false, Vec::new());
    }
    let jest = format!("{}/node_modules/.bin/jest", pkg_dir);
    if !Path::new(&jest).exists() {
        // #3974 — a package without jest runs its OWN runner (mcp-server:
        // node:test via npm test). The old `return true` here was a silent
        // vacuous green for every non-jest package.
        return run_npm_test(werk, pkg);
    }
    let mut cmd = Command::new(&jest);
    // #3918 — test child: cleared (see child_context).
    cmd.env("CHORUS_CONTEXT", "");
    cmd.args(["--ci", "--forceExit", "--passWithNoTests", "--json"])
        .current_dir(&pkg_dir);
    if let Some(n) = max_workers {
        cmd.arg(format!("--maxWorkers={}", n.max(1)));
    }
    apply_suite_world(&mut cmd, werk);
    // #4030 — a per-unit wall cap. `cmd.output()` had no deadline: on
    // 2026-08-30 03:00 platform/api's jest sat two hours (a test waiting on a
    // blocked box) until the 7200s LANE cap killed the whole run — five
    // packages and every bats suite never ran. Now the unit dies at its own
    // cap, scored failed and named, and the lane goes on.
    match run_capped_unit(&mut cmd, &format!("jest:{}", pkg)) {
        Some((ok, stdout, stderr)) => {
            if !ok {
                eprintln!("{}", stderr);
            }
            (ok, jest_cases_via_jq(stdout.as_bytes(), werk))
        }
        None => (false, Vec::new()),
    }
}

/// #4030 — run one npm-side unit under `unit_timeout()`, output captured to
/// files (wait on child exit, never pipe-EOF). Returns (ok, stdout, stderr);
/// None when the child could not be spawned. A capped unit is `ok=false` with
/// the cap named in stderr, so the lane line and the failure log both say why.
fn run_capped_unit(cmd: &mut Command, label: &str) -> Option<(bool, String, String)> {
    let tmp = std::env::temp_dir().join(format!("werk-test-unit-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let slug: String = label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let out_path = tmp.join(format!("{}.out", slug));
    let err_path = tmp.join(format!("{}.err", slug));
    let timeout = werk_test::unit_timeout();
    let fin = std::fs::File::create(&out_path).ok().and_then(|f| {
        let e = std::fs::File::create(&err_path).ok()?;
        cmd.stdout(f).stderr(e);
        werk_test::run_with_deadline(cmd, timeout).ok()
    })?;
    let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
    let mut stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    if fin.timed_out {
        let note = format!("!! {} killed after {}s — per-unit wall cap (NIGHTLY_UNIT_TIMEOUT, #4030)",
            label, timeout.as_secs());
        eprintln!("{}", note);
        stderr.push('\n');
        stderr.push_str(&note);
        return Some((false, stdout, stderr));
    }
    Some((fin.success, stdout, stderr))
}

/// #3974 — the non-jest package runner: `npm test` (mcp-server = node:test).
/// TAP lines become per-case results; a missing test script is FAIL LOUD,
/// never a vacuous green.
fn run_npm_test(werk: &str, pkg: &str) -> (bool, Vec<CaseResult>) {
    let pkg_dir = format!("{}/{}", werk, pkg);
    let has_script = std::fs::read_to_string(format!("{}/package.json", pkg_dir))
        .map(|j| j.contains("\"test\""))
        .unwrap_or(false);
    if !has_script {
        eprintln!("!! npm:{} has neither jest nor a test script — FAIL LOUD (no silent green)", pkg);
        return (false, Vec::new());
    }
    // File attribution, and why this runs one file at a time.
    //
    // `npm test` here is `tsx --test tests/*.test.ts`, and node:test FLATTENS:
    // its TAP carries case names and no filename, even when handed many files.
    // Every case was therefore stored under the PACKAGE directory:
    //
    //   registered  platform/mcp-server/tests/word-cap.test.ts :: <case>
    //   stored      platform/mcp-server/                       :: <case>
    //
    // The reconcile census joins on (file, name), so nothing matched and 245
    // passing mcp-server tests were counted as "never ran" every night — about
    // half the whole never-ran gap, and none of it real.
    //
    // Running per file is the only way to know which file a case came from.
    // It costs one process start per test file and buys a ledger that
    // cross-foots.
    let files = npm_test_files(&pkg_dir);
    if files.is_empty() {
        eprintln!("!! npm:{} has a test script but no test files found — FAIL LOUD", pkg);
        return (false, Vec::new());
    }
    // The runner is invoked DIRECTLY, not through `npm test -- <file>`: the
    // script globs its own files, so an appended path is additive — it runs the
    // whole suite again and attributes all 245 cases to whichever file was
    // named. Checked before shipping; it would have been silently wrong 16×.
    let Some(runner) = npm_test_runner(&pkg_dir) else {
        eprintln!("!! npm:{} test script is not a node:test runner — cannot attribute \
cases to files; refusing to store package-level rows that can never cross-foot", pkg);
        return (false, Vec::new());
    };
    let mut all_ok = true;
    let mut cases: Vec<CaseResult> = Vec::new();
    for rel in &files {
        let mut cmd = Command::new(&runner.0);
        cmd.args(&runner.1).arg(rel).current_dir(&pkg_dir);
        cmd.env("CHORUS_CONTEXT", ""); // #3918 — test child stays refusable
        apply_suite_world(&mut cmd, werk);
        match run_capped_unit(&mut cmd, &format!("npm:{}:{}", pkg, rel)) {
            Some((ok, stdout, stderr)) => {
                let text = format!("{}{}", stdout, stderr);
                if !ok {
                    all_ok = false;
                    let tail: Vec<&str> = text.lines().rev().take(20).collect();
                    eprintln!("{}", tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
                }
                cases.extend(werk_test::parse_bats_cases(&text).into_iter().map(
                    |(name, result)| CaseResult {
                        file_path: format!("{}/{}", pkg, rel),
                        test_name: name,
                        result,
                    },
                ));
            }
            None => all_ok = false,
        }
    }
    (all_ok, cases)
}

/// The node:test runner behind a package's `test` script, as (program, args)
/// with the file glob stripped — so one file can be appended. Recognises the
/// two shapes we run: `tsx --test <glob>` and `node --test <glob>`.
fn npm_test_runner(pkg_dir: &str) -> Option<(String, Vec<String>)> {
    let json = std::fs::read_to_string(format!("{}/package.json", pkg_dir)).ok()?;
    let script = json
        .lines()
        .find(|l| l.contains("\"test\":"))?
        .split_once(':')?
        .1
        .trim()
        .trim_end_matches(',')
        .trim()
        .trim_matches('"')
        .to_string();
    if !script.contains("--test") {
        return None;
    }
    let mut toks = script.split_whitespace().map(str::to_string);
    let prog = toks.next()?;
    // keep flags, drop the glob (anything that is not a flag)
    let args: Vec<String> = toks.filter(|t| t.starts_with('-')).collect();
    if !args.iter().any(|a| a == "--test") {
        return None;
    }
    let prog = if prog == "tsx" { "npx".to_string() } else { prog };
    let args = if prog == "npx" {
        let mut v = vec!["--no-install".to_string(), "tsx".to_string()];
        v.extend(args);
        v
    } else {
        args
    };
    Some((prog, args))
}

/// The package's test files, package-relative, sorted. Mirrors what the test
/// script globs; used so each file can be run on its own for attribution.
fn npm_test_files(pkg_dir: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for sub in ["tests", "test"] {
        let dir = format!("{}/{}", pkg_dir, sub);
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".test.ts") || name.ends_with(".test.js") {
                out.push(format!("{}/{}", sub, name));
            }
        }
    }
    out.sort();
    out
}

/// #3912 — run jest on an explicit registered-file selection. Paths arrive
/// repo-relative; jest wants them package-relative.
fn run_jest_selected(werk: &str, pkg: &str, files: &[String]) -> (bool, Vec<CaseResult>) {
    let pkg_dir = format!("{}/{}", werk, pkg);
    if !ensure_ts_deps(werk, pkg) {
        eprintln!("!! jest:{} CHANGED but deps unavailable — FAIL LOUD", pkg);
        return (false, Vec::new());
    }
    let jest = format!("{}/node_modules/.bin/jest", pkg_dir);
    if !Path::new(&jest).exists() {
        return (true, Vec::new());
    }
    let rel: Vec<String> = files
        .iter()
        .map(|f| f.strip_prefix(&format!("{}/", pkg)).unwrap_or(f).to_string())
        .collect();
    let mut cmd = Command::new(&jest);
    // #3918 — test child: cleared (see child_context).
    cmd.env("CHORUS_CONTEXT", "");
    cmd.args(["--ci", "--forceExit", "--passWithNoTests", "--json", "--runTestsByPath"])
        .args(&rel)
        .current_dir(&pkg_dir);
    apply_suite_world(&mut cmd, werk);
    // #4030 — a per-unit wall cap. `cmd.output()` had no deadline: on
    // 2026-08-30 03:00 platform/api's jest sat two hours (a test waiting on a
    // blocked box) until the 7200s LANE cap killed the whole run — five
    // packages and every bats suite never ran. Now the unit dies at its own
    // cap, scored failed and named, and the lane goes on.
    match run_capped_unit(&mut cmd, &format!("jest:{}", pkg)) {
        Some((ok, stdout, stderr)) => {
            if !ok {
                eprintln!("{}", stderr);
            }
            (ok, jest_cases_via_jq(stdout.as_bytes(), werk))
        }
        None => (false, Vec::new()),
    }
}

/// #3912 — jest's own import graph: which test FILES relate to the changed
/// sources. `--listTests --findRelatedTests` prints absolute paths; normalize
/// to repo-relative. Any failure yields EMPTY (selection then runs nothing
/// for the package — visible; the full-fallback lane is only for a dead
/// registry, not a jest hiccup, which would fail the real run anyway).
fn jest_related_files(werk: &str, pkg: &str, changed_in_pkg: &[String]) -> Vec<String> {
    if changed_in_pkg.is_empty() {
        return Vec::new();
    }
    let pkg_dir = format!("{}/{}", werk, pkg);
    let jest = format!("{}/node_modules/.bin/jest", pkg_dir);
    if !Path::new(&jest).exists() {
        return Vec::new();
    }
    let rel: Vec<String> = changed_in_pkg
        .iter()
        .map(|f| f.strip_prefix(&format!("{}/", pkg)).unwrap_or(f).to_string())
        .collect();
    let mut cmd = Command::new(&jest);
    // #3918 — test child: cleared (see child_context).
    cmd.env("CHORUS_CONTEXT", "");
    cmd.args(["--listTests", "--findRelatedTests"]).args(&rel).current_dir(&pkg_dir);
    match cmd.output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                let idx = l.find(&format!("{}/", pkg))?;
                Some(l[idx..].to_string())
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// jq-extract per-case rows from jest's --json report (curl|jq zero-dep
/// pattern, ADR-032 §6). Any jq failure yields an EMPTY set — emit is
/// best-effort, the gate verdict never depends on it.
fn jest_cases_via_jq(json: &[u8], werk: &str) -> Vec<CaseResult> {
    let jq_filter =
        r#".testResults[] | .name as $f | .assertionResults[] | [$f, .fullName, .status] | @tsv"#;
    let mut jq = match Command::new("jq")
        .args(["-r", jq_filter])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    if let Some(mut stdin) = jq.stdin.take() {
        use std::io::Write;
        if stdin.write_all(json).is_err() {
            return Vec::new();
        }
    }
    let out = match jq.wait_with_output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    parse_case_tsv(&String::from_utf8_lossy(&out))
        .into_iter()
        .map(|c| CaseResult { file_path: rel_path(&c.file_path, werk), ..c })
        .collect()
}

/// `clippy-ratchet.sh` — workspace-wide per-lint ratchet (counts only decrease).
fn run_clippy_ratchet(werk: &str) -> bool {
    let script = format!("{}/platform/scripts/clippy-ratchet.sh", werk);
    if !Path::new(&script).is_file() {
        return true;
    }
    // #3701 — pin CHORUS_ROOT to the werk: clippy-ratchet.py prefers $CHORUS_ROOT,
    // which the session env points at canonical, so the ratchet measured main
    // instead of this card's diff. Same pin run_doc_coherence carries (CHORUS_REPO).
    status_ok(Command::new("bash").arg(&script).current_dir(werk).env("CHORUS_ROOT", werk))
}

/// `doc-coherence-ratchet.test.sh` — the repo-wide doc-inventory floor, run with
/// CHORUS_REPO pinned to the werk so it checks THIS card's docs (#2994).
fn run_doc_coherence(werk: &str) -> bool {
    let script = format!("{}/platform/tests/doc-coherence-ratchet.test.sh", werk);
    if !Path::new(&script).is_file() {
        return true;
    }
    status_ok(Command::new("bash").arg(&script).current_dir(werk).env("CHORUS_REPO", werk))
}

/// Provide a TS package's node_modules by symlinking canonical's ONLY when the
/// lockfiles match (no dep drift — #3397). Returns true if deps are present after.
fn ensure_ts_deps(werk: &str, pkg: &str) -> bool {
    if !TS_PACKAGES.contains(&pkg) {
        return false;
    }
    let pkg_dir = format!("{}/{}", werk, pkg);
    if Path::new(&format!("{}/node_modules/.bin", pkg_dir)).is_dir() {
        return true;
    }
    if let Ok(home) = std::env::var("CHORUS_HOME") {
        let canon_nm = format!("{}/{}/node_modules", home, pkg);
        let werk_lock = format!("{}/package-lock.json", pkg_dir);
        let canon_lock = format!("{}/{}/package-lock.json", home, pkg);
        if Path::new(&canon_nm).is_dir() && lockfiles_match(&werk_lock, &canon_lock) {
            let _ = std::os::unix::fs::symlink(&canon_nm, format!("{}/node_modules", pkg_dir));
        }
    }
    Path::new(&format!("{}/node_modules/.bin", pkg_dir)).is_dir()
}

fn lockfiles_match(a: &str, b: &str) -> bool {
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn status_ok(cmd: &mut Command) -> bool {
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

/// Emit a typed test event to the ONE spine via chorus-log (subprocess, so the
/// verb stays zero-dep per ADR-032 §6). Best-effort: never affects the gate.
/// #3621 — takes the event name: test.started / test.completed are emitted on
/// EVERY run (green included), test.failed per failing check.
fn emit_spine(event: &str, role: &str, card: &str, trace: &str, extras: &[(&str, &str)]) {
    let home = match std::env::var("CHORUS_HOME") {
        Ok(h) => h,
        Err(_) => return,
    };
    let log = format!("{}/platform/scripts/chorus-log", home);
    if !Path::new(&log).is_file() {
        return;
    }
    let args = spine_args(event, role, card, trace, extras);
    let mut argv: Vec<&str> = vec![&log];
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    argv.extend(refs);
    let _ = Command::new("bash").args(&argv).status();
}

/// #3634 read side — fetch (filePath, covers) rows from the tests domain via
/// curl|jq (the quarantine pattern; zero-dep per ADR-032 §6). Returns the rows
/// plus the plan-source label: "model" on success, "fallback" on any failure —
/// the caller witnesses the degradation, the gate still runs on legacy lanes.
fn fetch_test_rows() -> (Vec<TestRow>, Vec<String>, Vec<String>, &'static str) {
    let endpoint = std::env::var("OWL_API_TESTS")
        .unwrap_or_else(|_| "http://localhost:3360/tests?limit=10000".to_string());
    // #3634 gather hardening (silas): NO shell interpolation — curl and jq run as
    // argv-exec'd subprocesses (a hostile char in the endpoint can't become shell).
    // The jq filter emits one TSV row PER covers value, so a multi-valued covers
    // (array in a future TestShape) fans out instead of being dropped silently.
    let jq_filter = r#".data[] | .filePath as $f | .pyramidLayer as $l | .testName as $n | .name as $e | .hermeticity as $h | .testConcern as $tc | (.covers | if type=="array" then .[] else . end) as $c | [$f,$c,($l // ""),($n // ""),($e // ""),($h // ""),($tc // "")] | @tsv"#;
    let curl = Command::new("curl")
        .args(["-sf", "--max-time", "10", &endpoint])
        .output();
    let body = match curl {
        Ok(o) if o.status.success() => o.stdout,
        _ => return (Vec::new(), Vec::new(), Vec::new(), plan_source_label(false, 0)),
    };
    let mut jq = match Command::new("jq")
        .args(["-r", jq_filter])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return (Vec::new(), Vec::new(), Vec::new(), plan_source_label(false, 0)),
    };
    if let Some(mut stdin) = jq.stdin.take() {
        use std::io::Write;
        if stdin.write_all(&body).is_err() {
            return (Vec::new(), Vec::new(), Vec::new(), plan_source_label(false, 0));
        }
    }
    let out = match jq.wait_with_output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return (Vec::new(), Vec::new(), Vec::new(), plan_source_label(false, 0)),
    };
    let (rows, names, entities) = parse_rows_and_names(&String::from_utf8_lossy(&out));
    let label = plan_source_label(true, rows.len());
    (rows, names, entities, label)
}

/// #3592 — shared token acquisition: $CHORUS_WRITE_TOKEN, else mint. Pulled out
/// of post_suite_run so TestResult posts reuse ONE token per run.
fn write_token(role: &str) -> Option<String> {
    std::env::var("CHORUS_WRITE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| mint_token(role))
}

/// #3592 emit side — every executed case lands as a chorus:TestResult keyed to
/// the registered identity (filePath+testName). Best-effort + WITNESSED: the
/// gate verdict never depends on it; skip/truncation is a spine event, not a
/// silence. Bounded at 2000 entities per run and packed into byte-bounded atomic
/// requests (no silent caps — every dropped/failed entity is accounted).
fn post_test_results(
    role: &str,
    card: &str,
    trace: &str,
    joined: &[(CaseResult, String)],
    run_epoch_ms: u128,
    idx_base: usize,
) -> usize {
    let writeback_started = std::time::Instant::now();
    let endpoint = std::env::var("OWL_API_TESTRESULTS_BATCH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            let collection = std::env::var("OWL_API_TESTRESULTS")
                .unwrap_or_else(|_| "http://localhost:3360/testresults".to_string());
            werk_test::testresult_batch_endpoint(&collection)
        });
    // #4022 — was 2000, set when a card-scoped run posted ~200 rows. The first
    // full parallel nightly joined 6,712 cases and the cap silently outranked
    // the storage promise: 4,712 computed verdicts dropped, caught only because
    // #4015's results-lost gate now fails the run. 10k clears the current
    // battery (~6.7k) with headroom; the chunker already byte-bounds requests,
    // so a bigger cap costs more chunks, not bigger ones.
    const MAX_POSTS: usize = 10_000;
    // #3925 — the RUN's clock, threaded from run start; post time is not run time.
    let ts = run_epoch_ms;
    let payloads: Vec<String> = joined
        .iter()
        .take(MAX_POSTS)
        .enumerate()
        .map(|(i, (c, of_test))| {
            // #4033 — names are testresult-<card>-<ts>-<idx>; with the per-unit
            // store (#4030) every unit restarted i at 0 under the run's shared
            // ts, so unit two's names were unit one's and the store answered
            // 409 for the whole chunk. idx_base makes idx run-unique.
            test_result_payload(
                &c.file_path, &c.test_name, &c.result, of_test, card, role, trace, ts, idx_base + i)
        })
        .collect();
    let packed = werk_test::chunk_json_payloads(&payloads, werk_test::TESTRESULT_BATCH_MAX_BYTES);
    let mut stats = if packed.chunks.is_empty() {
        werk_test::PostStats::default()
    } else if let Some(token) = write_token(role) {
        // The first 401 re-mints ONCE and retries the same atomic chunk. A 401
        // that survives the re-mint is a real refusal; no per-entity fallback.
        werk_test::post_results_loop(&endpoint, &token, &packed.chunks, &|| mint_token(role))
    } else {
        emit_spine("testresult.post.skipped", role, card, trace,
            &[("reason", "no-write-token"), ("count", &payloads.len().to_string())]);
        werk_test::PostStats {
            failed: packed.chunks.iter().map(|c| c.entities).sum(),
            chunks_failed: packed.chunks.len(),
            first_fail_code: Some("no-write-token".into()),
            ..Default::default()
        }
    };
    let truncated_dropped = joined.len().saturating_sub(MAX_POSTS);
    werk_test::account_unsent_results(&mut stats, packed.oversized, truncated_dropped);
    // #3725 AC4 — say it out loud. A run that posts ZERO results must never
    // look identical to one that posted all of them.
    if stats.failed > 0 {
        println!(
            "!! testresult wire-back: {} of {} case(s) FAILED to POST to the model{}              — the tests domain did not receive this run's results",
            stats.failed,
            stats.posted + stats.failed,
            match &stats.first_fail_code {
                Some(c) => format!(" (first failure HTTP {})", c),
                None => String::new(),
            }
        );
    }
    let posted = stats.posted.to_string();
    let failed = stats.failed.to_string();
    let chunks_attempted = stats.chunks_attempted.to_string();
    let chunks_succeeded = stats.chunks_succeeded.to_string();
    let chunks_failed = stats.chunks_failed.to_string();
    let remint_attempts = stats.remint_attempts.to_string();
    let mut extras: Vec<(String, String)> = vec![
        ("count".into(), posted.clone()),
        ("failed_posts".into(), failed.clone()),
        ("chunks_attempted".into(), chunks_attempted.clone()),
        ("chunks_succeeded".into(), chunks_succeeded.clone()),
        ("chunks_failed".into(), chunks_failed.clone()),
        // #3808 AC3 — expiry frequency is observable from the spine.
        ("remint_attempts".into(), remint_attempts.clone()),
    ];
    if let Some(c) = &stats.first_fail_code {
        extras.push(("first_fail_http".into(), c.clone()));
    }
    if joined.len() > MAX_POSTS {
        extras.push(("truncated_dropped".into(), truncated_dropped.to_string()));
    }
    let refs: Vec<(&str, &str)> = extras.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    emit_spine("testresult.posted", role, card, trace, &refs);
    let duration_ms = writeback_started.elapsed().as_millis().to_string();
    let truncated = truncated_dropped.to_string();
    let completed = [
        ("duration_ms", duration_ms.as_str()),
        ("chunks_attempted", chunks_attempted.as_str()),
        ("chunks_succeeded", chunks_succeeded.as_str()),
        ("chunks_failed", chunks_failed.as_str()),
        ("entities_posted", posted.as_str()),
        ("entities_failed", failed.as_str()),
        ("truncated_dropped", truncated.as_str()),
        ("remint_attempts", remint_attempts.as_str()),
    ];
    emit_spine("testresult.writeback.completed", role, card, trace, &completed);
    stats.posted
}

/// #3592 AC3 — registered ∖ executed. Reads BOTH generated collections, prints
/// the explicit-none report, emits tests.reconcile with the counts. Exit 0 —
/// the count is alertable (spine/monitors), the command itself never blocks.
fn run_reconcile() -> Result<i32, String> {
    let role = std::env::var("ROLE").unwrap_or_else(|_| "kade".to_string());
    let trace = std::env::var("CHORUS_TRACE_ID").unwrap_or_default();
    let (rows, names, _entities, source) = fetch_test_rows();
    if source != "model" {
        return Err("reconcile requires the tests domain; fetch failed or empty".into());
    }
    let mut registered: Vec<(String, String)> = rows
        .iter()
        .zip(names.iter())
        .map(|(r, n)| (r.file_path.clone(), n.clone()))
        .collect();
    registered.sort();
    registered.dedup(); // covers fan-out duplicates one row per covers value
    let endpoint = std::env::var("OWL_API_TESTRESULTS")
        .unwrap_or_else(|_| "http://localhost:3360/testresults?limit=100000".to_string());
    // #4022 — `pipefail`, and a failed fetch is an ERROR, not an empty ledger.
    // On 2026-08-28 /testresults 502'd, `curl -sf` printed nothing, and this
    // function reported "7,794 registered tests never ran" — every test the
    // nightly had just executed and stored (7,269 posted, 49/49 chunks). A
    // census that cannot reach the ledger must say so, never "nothing ran".
    // The ledger is larger than one page (229k rows, 100k page cap), so the
    // walk follows `links.next` until the collection is exhausted.
    let jq = r#"(.data[] | [.filePath, .testName] | @tsv), ("__NEXT__\t" + (.links.next // ""))"#;
    let mut executed: Vec<(String, String)> = Vec::new();
    let mut page = endpoint.clone();
    let mut pages = 0usize;
    loop {
        pages += 1;
        let pipe = format!("set -o pipefail; curl -sf --max-time 120 '{}' | jq -r '{}'", page, jq);
        let out = Command::new("bash")
            .args(["-c", &pipe])
            .output()
            .map_err(|e| format!("testresults fetch failed: {}", e))?;
        if !out.status.success() {
            return Err(format!(
                "testresults fetch failed: {} unreachable or refused (rc={}, page {}) — census not taken",
                page,
                out.status.code().unwrap_or(-1),
                pages
            ));
        }
        let mut next = String::new();
        for l in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(n) = l.strip_prefix("__NEXT__\t") {
                next = n.to_string();
                continue;
            }
            let mut it = l.split('\t');
            if let (Some(f), Some(n)) = (it.next(), it.next()) {
                if !f.is_empty() {
                    executed.push((f.to_string(), n.to_string()));
                }
            }
        }
        match werk_test::next_page_url(&page, &next) {
            Some(n) if pages < 50 => page = n,
            _ => break,
        }
    }
    executed.sort();
    executed.dedup();
    let gap = reconcile_gap(&registered, &executed);
    println!("{}", reconcile_report(registered.len(), &gap));
    emit_spine("tests.reconcile", &role, "-", &trace,
        &[("registered", &registered.len().to_string()),
          ("executed", &executed.len().to_string()),
          ("never_run", &gap.len().to_string())]);
    Ok(0)
}

/// #3634 write side — POST the run's TestSuiteRun through the generated write
/// surface with a #3619-scoped token. Token: $CHORUS_WRITE_TOKEN if the runner
/// provides it, else minted via chorus-identity-token (ES256 identity from the
/// realm env inside the script — never echoed here). Every outcome is witnessed:
/// testsuiterun.posted / testsuiterun.post.skipped with the reason.
#[allow(clippy::too_many_arguments)]
fn post_suite_run(
    role: &str,
    card: &str,
    trace: &str,
    plan_source: &str,
    checks_planned: usize,
    checks_failed: usize,
    duration_ms: u128,
    verdict: &str,
) {
    let endpoint = std::env::var("OWL_API_TESTSUITERUNS")
        .unwrap_or_else(|_| "http://localhost:3360/testsuiteruns".to_string());
    let Some(token) = write_token(role) else {
        emit_spine("testsuiterun.post.skipped", role, card, trace,
            &[("reason", "no-write-token")]);
        return;
    };
    let payload = suite_run_payload(card, role, trace, plan_source, checks_planned,
        checks_failed, duration_ms, verdict);
    let args = werk_test::suite_run_post_args(&endpoint, &token, &payload);
    let ok = Command::new("curl")
        .args(&args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok {
        emit_spine("testsuiterun.posted", role, card, trace, &[("verdict", verdict), ("plan_source", plan_source)]);
    } else {
        emit_spine("testsuiterun.post.skipped", role, card, trace, &[("reason", "post-failed")]);
    }
}

/// Mint a write token — #3689: ES256 CSS IDENTITY, no scope in the token.
/// Scope is model data now (chorus:hasScope on the Principal, resolved at the
/// athena-make door per TTL). The HS256 mint script this replaced (#3689/#3719)
/// carried a SELF-DECLARED scope claim — the caller authorized itself, which
/// is the class #3689 retires. Best-effort, same contract as before.
fn mint_token(role: &str) -> Option<String> {
    let home = std::env::var("CHORUS_HOME").ok()?;
    let script = format!("{}/platform/scripts/chorus-identity-token", home);
    if !Path::new(&script).is_file() {
        return None;
    }
    let out = Command::new(&script)
        .arg(role)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

/// #3821 — candidate units + declared edges from the werk tree, then the shared
/// scoping core. Candidates are STRUCTURAL: every platform/services crate with a
/// Cargo.toml (lib-only crates have tests too) + the known TS packages. TS edges
/// come back keyed by package NAME; test units key TS by DIR, so names translate
/// through each package.json before scoping.
fn diff_scoped_units(werk: &str, changed: &[String]) -> Option<Vec<TestUnit>> {
    let root = Path::new(werk);
    let mut units: Vec<ScopeUnit> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root.join("platform/services")) {
        for e in entries.flatten() {
            if e.path().join("Cargo.toml").is_file() {
                if let Some(n) = e.file_name().to_str() {
                    units.push(ScopeUnit {
                        name: n.to_string(),
                        dir: format!("platform/services/{}", n),
                    });
                }
            }
        }
    }
    let mut ts_name_to_dir: Vec<(String, String)> = Vec::new();
    for pkg in TS_PACKAGES {
        units.push(ScopeUnit { name: (*pkg).to_string(), dir: (*pkg).to_string() });
        if let Ok(content) = std::fs::read_to_string(root.join(pkg).join("package.json")) {
            if let Some(i) = content.find("\"name\"") {
                let rest = &content[i + 6..];
                if let Some(c) = rest.find(':') {
                    let rest = rest[c + 1..].trim_start();
                    if let Some(rest) = rest.strip_prefix('"') {
                        if let Some(e) = rest.find('"') {
                            ts_name_to_dir.push((rest[..e].to_string(), (*pkg).to_string()));
                        }
                    }
                }
            }
        }
    }
    let edges: Vec<(String, String)> = scope_declared_edges(root)
        .into_iter()
        .map(|(p, d)| {
            let p2 = ts_name_to_dir.iter().find(|(n, _)| *n == p).map(|(_, d2)| d2.clone()).unwrap_or(p);
            let d2 = ts_name_to_dir.iter().find(|(n, _)| *n == d).map(|(_, dd)| dd.clone()).unwrap_or(d);
            (p2, d2)
        })
        .collect();
    let scoped = scoped_test_units(changed, &units, &edges)?;
    Some(
        scoped
            .into_iter()
            .map(|u| {
                if u.dir.starts_with("platform/services/") {
                    TestUnit::RustCrate(u.name)
                } else {
                    TestUnit::TsPackage(u.dir)
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod suite_coverage_3917 {
    use super::*;

    /// #3934 — key the fixture dir on the CALLER, not on files.len(): two tests
    /// with one file each collided in the same tmpdir and saw each other's
    /// suites. The tests caught it; the naming scheme was the bug.
    fn world_named(tag: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir()
            .join(format!("werk-test-cov-{}-{}", std::process::id(), tag));
        let tests = root.join("platform/tests");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&tests).unwrap();
        for (name, body) in files {
            std::fs::write(tests.join(name), body).unwrap();
        }
        root
    }

    #[test]
    fn index_records_the_scripts_a_suite_names() {
        let root = world_named("scripts", &[(
            "a.bats",
            "run bash \"${CHORUS_ROOT}/platform/scripts/gate-spine-vikunja-bridge.sh\" code 1\n",
        )]);
        let rows = build_suite_coverage(root.to_str().unwrap());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].suite, "platform/tests/a.bats");
        assert_eq!(rows[0].covers, vec!["platform/scripts/gate-spine-vikunja-bridge.sh"]);
    }

    /// NEGATIVE PROOF (#3734): a suite that names NO script must yield no
    /// coverage. If this ever returns rows, the index is matching noise and
    /// every script would look covered — the exact hollow-gate shape #3917 fixes.
    #[test]
    fn a_suite_naming_no_script_covers_nothing() {
        let root = world_named("nogovern", &[("b.bats", "@test \"nothing\" { true; }\n"), ("c.bats", "# no scripts here\n")]);
        let rows = build_suite_coverage(root.to_str().unwrap());
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.covers.is_empty()), "got {:?}", rows);
    }

    /// A missing tests dir is empty coverage, not a panic — the index must not
    /// take down the gate in a tree that has no bats suites.
    /// #3934 — the index must harvest GOVERNED SURFACES, not just *.sh. This is
    /// the edge that let #3918's werk.yml change run no tests.
    #[test]
    fn index_records_a_governed_surface_a_suite_greps() {
        let root = world_named("governed", &[(
            "guard.bats",
            "grep -qE 'CHORUS_CONTEXT' \"$CHORUS_ROOT/.github/workflows/werk.yml\"\n",
        )]);
        let rows = build_suite_coverage(root.to_str().unwrap());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].covers, vec![".github/workflows/werk.yml"]);
    }

    /// NEGATIVE PROOF (#3734): a suite naming no governed surface covers none —
    /// otherwise every yml change would sweep every suite.
    #[test]
    fn a_suite_naming_no_governed_surface_covers_none() {
        let root = world_named("noscript", &[("plain.bats", "@test \"x\" { true; }\n")]);
        let rows = build_suite_coverage(root.to_str().unwrap());
        assert!(rows[0].covers.is_empty(), "got {:?}", rows[0].covers);
    }

    #[test]
    fn missing_tests_dir_is_empty_not_fatal() {
        assert!(build_suite_coverage("/nonexistent/werk/root").is_empty());
    }
}

/// #3920 — the ui lane's stack verdict. The lane is needs-stack by nature
/// (browser against live pages); reuse the SAME probe machinery so up/down has
/// one definition. Probes only when the lane actually fires.
fn stack_down_ui(_selected_ns: &[String], _ns_all: &std::collections::BTreeSet<String>) -> Option<String> {
    match werk_test::stack_verdict(&probe_stack().iter().map(|(n, o)| (n.as_str(), *o)).collect::<Vec<_>>()) {
        Ok(()) => None,
        Err(down) => Some(down),
    }
}

/// #3920 — run the registered ui specs via playwright, from the werk (variant
/// URLs injectable via env; local defaults inside the specs). One invocation,
/// all files — playwright parallelizes internally.
/// #4004 — a ui flow that brings its own service needs that service BUILT. The
/// tiles spec spawns directing/clearing/dist/server.js; a werk has no dist until
/// the package is compiled, so the spawn died instantly and the only symptom was
/// a 30s wait ending in "own Clearing did not answer on :3487" — the port blamed
/// for a missing build, and two rounds lost to it. Build it here, where the lane
/// that depends on it runs, rather than hoping an earlier phase happened to.
fn ensure_ui_service_built(werk: &str, pkg: &str, artifact: &str) {
    if Path::new(&format!("{}/{}/{}", werk, pkg, artifact)).exists() {
        return;
    }
    if !ensure_ts_deps(werk, pkg) {
        eprintln!("!! ui-flows: {} deps unavailable — its flows will fail loud", pkg);
        return;
    }
    let out = Command::new("npm")
        .args(["run", "build", "--silent"])
        .current_dir(format!("{}/{}", werk, pkg))
        .output();
    match out {
        Ok(o) if o.status.success() => println!("   ui-flows: built {} for its own-service flows", pkg),
        _ => eprintln!("!! ui-flows: {} build FAILED — flows needing it will name that", pkg),
    }
}

fn run_ui_flows(werk: &str, files: &std::collections::BTreeSet<String>, quarantined: &[werk_test::Quarantined]) -> (bool, String) {
    ensure_ui_service_built(werk, "directing/clearing", "dist/server.js");
    let mut cmd = Command::new("npx");
    cmd.arg("playwright").arg("test");
    // #4045 — honour the quarantine here too, not only in run_cargo. Visible: the
    // pattern is printed, so a skipped spec is never a silent absence (#3443).
    let mut excluded = String::new();
    if let Some(pat) = werk_test::playwright_grep_invert(quarantined) {
        let names: Vec<&str> = quarantined.iter().map(|q| q.case.as_str()).collect();
        println!("   ui-flows: quarantined specs excluded via --grep-invert: {}", names.join(", "));
        excluded = format!(" [quarantined, excluded: {}]", names.join(", "));
        cmd.arg("--grep-invert").arg(pat);
    }
    for f in files {
        cmd.arg(f);
    }
    cmd.current_dir(werk);
    cmd.env("CHORUS_CONTEXT", ""); // #3918 — test child stays refusable
    match cmd.output() {
        Ok(o) => {
            let text = format!("{}{}",
                String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            match werk_test::parse_playwright_summary(&text) {
                Some((p, f)) => {
                    // #4004 — a red that will not NAME itself is unactionable. When
                    // the summary parsed we printed only "(60 passed, 1 failed)" and
                    // swallowed the output, so the same ui-flows red survived two
                    // rounds with nobody able to say which flow it was — and it does
                    // not reproduce locally, so the log was the only witness.
                    for line in werk_test::playwright_failure_lines(&text) {
                        eprintln!("{}", line);
                    }
                    // #4045 — a skip is typed and visible: counted here, named in the summary.
                    let skipped = werk_test::parse_playwright_skipped(&text);
                    let skip_note = if skipped > 0 {
                        let why = if std::env::var("CLEARING_URL").map(|v| v.is_empty()).unwrap_or(true) {
                            " (clearing specs: no CLEARING_URL — the leg covers none of Clearing until a variant room exists)"
                        } else { "" };
                        println!("   ui-flows: {} skipped{}", skipped, why);
                        format!(", {} skipped{}", skipped, why)
                    } else { String::new() };
                    (o.status.success() && f == 0, format!(" ({} passed, {} failed{}){}", p, f, skip_note, excluded))
                }
                None => {
                    let tail: Vec<&str> = text.lines().rev().take(15).collect();
                    eprintln!("{}", tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
                    (false, " (no playwright summary — crashed before running, fail loud)".to_string())
                }
            }
        }
        Err(e) => (false, format!(" (spawn failed: {})", e)),
    }
}

#[cfg(test)]
mod suite_deadline_tests {
    use super::*;

    fn world(name: &str, body: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("wt-deadline-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), body).unwrap();
        (dir, name.to_string())
    }

    #[test]
    fn a_leaked_pipe_holding_child_cannot_outlive_the_suite() {
        // TD-028 live shape: the suite FINISHES but leaves `sleep &` holding
        // stdout. Under cmd.output() this call blocked until the leak died
        // (24 minutes on 2026-08-27); under child-exit waiting it returns now.
        let (dir, s) = world("leaker.sh",
            "( sleep 300 & )\necho '=== Results: 1 passed, 0 failed ==='\nexit 0\n");
        let t0 = std::time::Instant::now();
        let (ok, _, text) = run_bats_cases(dir.to_str().unwrap(), &s);
        assert!(t0.elapsed().as_secs() < 30, "runner waited on the leaked pipe");
        assert!(ok);
        assert!(text.contains("1 passed"));
    }

    #[test]
    fn negative_proof_unit_cap_fires_and_the_lane_cap_never_reaches_a_suite() {
        // #4035 two-caps-separate proof (#3734), one fixture, two legs.
        // Leg 1 — the LEAKED lane cap alone (1s) must NOT kill a 3s suite:
        // under the pre-#4035 read this leg dies at 1s and the test goes red.
        std::env::set_var("NIGHTLY_SUITE_TIMEOUT", "1");
        std::env::remove_var("NIGHTLY_UNIT_TIMEOUT");
        let (dir, s) = world("slowish.sh",
            "sleep 3\necho '=== Results: 1 passed, 0 failed ==='\n");
        let (ok, _, text) = run_bats_cases(dir.to_str().unwrap(), &s);
        assert!(ok, "a suite inside the UNIT cap must survive a lane-cap leak: {}", text);
        // Leg 2 — the UNIT cap (2s) kills a hung suite, loud (#4022 AC4's half):
        // a suite that never exits breaches its budget, dies, reads as a fail.
        std::env::set_var("NIGHTLY_UNIT_TIMEOUT", "2");
        let (dir, s) = world("hung.sh", "echo started\nsleep 300\n");
        let t0 = std::time::Instant::now();
        let (ok, _, text) = run_bats_cases(dir.to_str().unwrap(), &s);
        std::env::remove_var("NIGHTLY_UNIT_TIMEOUT");
        std::env::remove_var("NIGHTLY_SUITE_TIMEOUT");
        assert!(!ok);
        assert!(t0.elapsed().as_secs() < 30);
        assert!(text.contains("SUITE TIMED OUT"));
    }
}


/// #4022 — the box's 1-minute load, via `sysctl -n vm.loadavg` (macOS) with an
/// `uptime` fallback. None when neither answers: no data must never gate.
fn read_loadavg() -> Option<f64> {
    let try_cmd = |c: &str, a: &[&str]| -> Option<String> {
        let o = Command::new(c).args(a).output().ok()?;
        if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).to_string()) } else { None }
    };
    try_cmd("sysctl", &["-n", "vm.loadavg"]).and_then(|t| werk_test::parse_loadavg(&t))
        .or_else(|| try_cmd("uptime", &[]).and_then(|t| {
            let tail = t.rsplit("load average").next().unwrap_or("");
            werk_test::parse_loadavg(tail.trim_start_matches('s').trim_start_matches(':'))
        }))
}
