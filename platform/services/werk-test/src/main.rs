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
            let (ok, summary) = run_ui_flows(&werk, &ui_set);
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
    let extras = werk_test::completed_extras(
        &outcome,
        units.len(),
        plan.len(),
        failed_count,
        started_at.elapsed().as_millis(),
        self_mod,
    );
    let extra_refs: Vec<(&str, &str)> = extras.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    emit_spine("test.completed", &role, &card, &trace, &extra_refs);
    // #3634 write side — the run becomes a TestSuiteRun instance in the graph.
    // Best-effort and WITNESSED either way: the gate's verdict never depends on
    // the write, but a skipped post is a spine event, not a silence.
    post_suite_run(&role, &card, &trace, plan_source, plan.len(), failed_count,
        started_at.elapsed().as_millis(), outcome.label());
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
    post_test_results(&role, &card, &trace, &joined, run_epoch_ms);
    println!("werk-test: {} (exit {})", outcome.label(), outcome.exit_code());
    Ok(outcome.exit_code())
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
    let mut all_cases: Vec<CaseResult> = Vec::new();
    let mut unmatched_cargo = 0usize;
    for c in &crates {
        let crate_prefix = format!("platform/services/{}/tests/", c);
        let ns_bins: Vec<String> = if stack_down.is_some() {
            ns_all.iter()
                .filter_map(|f| f.strip_prefix(&crate_prefix))
                .filter_map(|rest| rest.strip_suffix(".rs"))
                .filter(|stem| !stem.contains('/'))
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        };
        let ns_refs: Vec<&str> = ns_bins.iter().map(|s| s.as_str()).collect();
        let (ok, cases) = run_cargo(&root, c, &q_names, &ns_refs);
        let passed = cases.iter().filter(|(_, r)| r == "pass").count();
        let case_failed = cases.iter().filter(|(_, r)| r != "pass").count();
        println!("{}", werk_test::nightly_unit_line(c, ok, passed, case_failed, ns_refs.len()));
        let crate_dir = format!("platform/services/{}", c);
        for (bare, result) in cases {
            match match_cargo_case(&bare, &crate_dir, &rows, &row_names) {
                Some(fp) => all_cases.push(CaseResult { file_path: fp, test_name: bare, result }),
                None => unmatched_cargo += 1,
            }
        }
        if !ok {
            any_failed = true;
            failed_count += 1;
            emit_spine("test.failed", &role, &card, &trace,
                &[("check", "cargo"), ("unit", c)]);
        }
    }

    // #3974 — npm lane: every registered TS/node package, full selection.
    // jest packages run jest; non-jest packages run their own `npm test`
    // (never a vacuous green). needs-stack files leave the run typed when
    // the stack is down, same vocabulary as the cargo lane.
    let ts_pkgs: Vec<String> = werk_test::nightly_ts_packages(&rows)
        .into_iter()
        .filter(|p| only.as_deref().map(|o| o == p).unwrap_or(true))
        .collect();
    // #3559/#3974 — platform/api's INTEGRATION jest project is only
    // constructed under RUN_INTEGRATION=true; the nightly sets it from the
    // live stack probe so integration tests run with the stack and are
    // typed-absent without it — the wrapper's old per-package env is retired.
    if stack_down.is_none() {
        std::env::set_var("RUN_INTEGRATION", "true");
    }
    for p in &ts_pkgs {
        let pkg_ns: Vec<String> = if stack_down.is_some() {
            ns_all.iter().filter(|f| f.starts_with(&format!("{}/", p))).cloned().collect()
        } else { Vec::new() };
        let (ok, cases) = run_jest(&root, p);
        let passed = cases.iter().filter(|c| c.result == "pass").count();
        let case_failed = cases.iter().filter(|c| c.result != "pass").count();
        let npm_kind = if werk_test::security_units(&rows).contains(p) { "security" } else { "npm" };
        println!("{}", werk_test::nightly_lane_line(npm_kind, p, ok, passed, case_failed, pkg_ns.len()));
        all_cases.extend(cases);
        if !ok {
            any_failed = true;
            failed_count += 1;
            emit_spine("test.failed", &role, &card, &trace, &[("check", "npm"), ("unit", p)]);
        }
    }

    // #3974 — bats lane: registered suites from the registry, per-case TAP
    // results (boolean-only bats is over).
    let bats_suites: Vec<String> = werk_test::nightly_bats_suites(&rows)
        .into_iter()
        .filter(|b| only.as_deref().map(|o| o == b).unwrap_or(true))
        .collect();
    // #3922 — security-declared units fold under their own lane label so the
    // report and owner routing see ONE security lane on its own cadence.
    let sec_units = werk_test::security_units(&rows);
    for b in &bats_suites {
        let kind = if sec_units.contains(b) { "security" }
            else if b.ends_with(".sh") { "shell" } else { "bats" };
        let (ok, cases, text) = run_bats_cases(&root, b);
        let (passed, case_failed) = if cases.is_empty() && kind == "shell" {
            // shell suites report summary counts, not TAP cases
            werk_test::parse_shell_counts(&text)
                .unwrap_or(if ok { (1, 0) } else { (0, 1) })
        } else {
            (cases.iter().filter(|(_, r)| r == "pass").count(),
             cases.iter().filter(|(_, r)| r != "pass").count())
        };
        println!("{}", werk_test::nightly_lane_line(kind, b, ok, passed, case_failed, 0));
        for (name, result) in cases {
            all_cases.push(CaseResult { file_path: b.clone(), test_name: name, result });
        }
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
    let extras = werk_test::completed_extras(
        &outcome, total_units, total_units, failed_count,
        started_at.elapsed().as_millis(), false);
    let extra_refs: Vec<(&str, &str)> = extras.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    emit_spine("test.completed", &role, &card, &trace, &extra_refs);
    // #3975 — the graph writes authenticate as the NIGHTLY machine principal
    // (least-privilege scope: the tests graph only). Spine events keep role
    // "system" — who acted vs which credential wrote are different facts.
    let mint_role = std::env::var("WERK_NIGHTLY_MINT_ROLE").unwrap_or_else(|_| "nightly".to_string());
    post_suite_run(&mint_role, &card, &trace, plan_source, crates.len(), failed_count,
        started_at.elapsed().as_millis(), outcome.label());
    if unmatched_cargo > 0 {
        emit_spine("testresult.unmatched", &role, &card, &trace,
            &[("count", &unmatched_cargo.to_string()), ("kind", "cargo-ambiguous-or-unregistered")]);
    }
    let (joined, unregistered) = werk_test::join_cases(&all_cases, &rows, &row_names, &row_entities);
    if unregistered > 0 {
        emit_spine("testresult.unregistered", &role, &card, &trace,
            &[("count", &unregistered.to_string())]);
    }
    post_test_results(&mint_role, &card, &trace, &joined, run_epoch_ms);
    println!("werk-test: {} (exit {})", outcome.label(), outcome.exit_code());
    Ok(outcome.exit_code())
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
    match cmd.output() {
        Ok(o) => {
            let text = format!("{}{}",
                String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            let ok = o.status.success();
            if !ok {
                let tail: Vec<&str> = text.lines().rev().take(20).collect();
                eprintln!("{}", tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
            }
            let cases = werk_test::parse_bats_cases(&text);
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
    status_ok(
        Command::new("bats")
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
    let mut args: Vec<String> = werk_test::nextest_run_args(quarantined, ns_bins);
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
    apply_suite_world(&mut cmd, werk);
    match cmd.output() {
        Ok(o) => {
            let ok = o.status.success();
            if !ok {
                eprintln!("{}", String::from_utf8_lossy(&o.stderr));
            }
            (ok, jest_cases_via_jq(&o.stdout, werk))
        }
        Err(_) => (false, Vec::new()),
    }
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
    let mut cmd = Command::new("npm");
    cmd.args(["test", "--silent"]).current_dir(&pkg_dir);
    cmd.env("CHORUS_CONTEXT", ""); // #3918 — test child stays refusable
    apply_suite_world(&mut cmd, werk);
    match cmd.output() {
        Ok(o) => {
            let text = format!("{}{}",
                String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            let ok = o.status.success();
            if !ok {
                let tail: Vec<&str> = text.lines().rev().take(20).collect();
                eprintln!("{}", tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
            }
            let cases = werk_test::parse_bats_cases(&text)
                .into_iter()
                .map(|(name, result)| CaseResult {
                    file_path: format!("{}/", pkg),
                    test_name: name,
                    result,
                })
                .collect();
            (ok, cases)
        }
        Err(_) => (false, Vec::new()),
    }
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
    match cmd.output() {
        Ok(o) => {
            let ok = o.status.success();
            if !ok {
                eprintln!("{}", String::from_utf8_lossy(&o.stderr));
            }
            (ok, jest_cases_via_jq(&o.stdout, werk))
        }
        Err(_) => (false, Vec::new()),
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
/// silence. Bounded at 2000 posts per run (no silent caps — dropped count named).
fn post_test_results(
    role: &str,
    card: &str,
    trace: &str,
    joined: &[(CaseResult, String)],
    run_epoch_ms: u128,
) {
    if joined.is_empty() {
        return;
    }
    let endpoint = std::env::var("OWL_API_TESTRESULTS")
        .unwrap_or_else(|_| "http://localhost:3360/testresults".to_string());
    let Some(token) = write_token(role) else {
        emit_spine("testresult.post.skipped", role, card, trace,
            &[("reason", "no-write-token"), ("count", &joined.len().to_string())]);
        return;
    };
    const MAX_POSTS: usize = 2000;
    // #3925 — the RUN's clock, threaded from run start; post time is not run time.
    let ts = run_epoch_ms;
    let payloads: Vec<String> = joined
        .iter()
        .take(MAX_POSTS)
        .enumerate()
        .map(|(i, (c, of_test))| {
            test_result_payload(
                &c.file_path, &c.test_name, &c.result, of_test, card, role, trace, ts, i)
        })
        .collect();
    // #3808 — the loop yields per-case HTTP codes (retiring #3725's second
    // diagnostic request) and recovers a token that expires mid-stream: the
    // first 401 re-mints ONCE and resumes from the same case. Measured cause
    // (#3802): one token minted up front, chorus-identity-token's ~600s cache,
    // 594 posts landed then 534 died 401. A 401 that survives the re-mint is a
    // real refusal and fails loudly below — never an endless retry.
    let stats = werk_test::post_results_loop(&endpoint, &token, &payloads, &|| mint_token(role));
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
    let mut extras: Vec<(String, String)> = vec![
        ("count".into(), stats.posted.to_string()),
        ("failed_posts".into(), stats.failed.to_string()),
        // #3808 AC3 — expiry frequency is observable from the spine.
        ("remint_attempts".into(), stats.remint_attempts.to_string()),
    ];
    if let Some(c) = &stats.first_fail_code {
        extras.push(("first_fail_http".into(), c.clone()));
    }
    if joined.len() > MAX_POSTS {
        extras.push(("truncated_dropped".into(), (joined.len() - MAX_POSTS).to_string()));
    }
    let refs: Vec<(&str, &str)> = extras.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    emit_spine("testresult.posted", role, card, trace, &refs);
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
    let jq = r#".data[] | [.filePath, .testName] | @tsv"#;
    let pipe = format!("curl -sf --max-time 15 '{}' | jq -r '{}'", endpoint, jq);
    let out = Command::new("bash")
        .args(["-c", &pipe])
        .output()
        .map_err(|e| format!("testresults fetch failed: {}", e))?;
    let mut executed: Vec<(String, String)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            match (it.next(), it.next()) {
                (Some(f), Some(n)) if !f.is_empty() => Some((f.to_string(), n.to_string())),
                _ => None,
            }
        })
        .collect();
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
fn run_ui_flows(werk: &str, files: &std::collections::BTreeSet<String>) -> (bool, String) {
    let mut cmd = Command::new("npx");
    cmd.arg("playwright").arg("test");
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
                Some((p, f)) => (o.status.success() && f == 0, format!(" ({} passed, {} failed)", p, f)),
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
