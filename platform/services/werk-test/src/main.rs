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
    model_units, parse_case_tsv, parse_quarantine_rows,
    jest_plan, parse_rows_and_names, plan_source_label, plan_units_from_rows, quarantine_report,
    JestPlan,
    rel_path, scope_rows, scoped_requires_model, spine_args,
    is_test_suite_path, scope_declared_edges, scoped_test_units, scoped_test_reason, suite_run_payload, test_result_payload,
    unmapped_path,
    undeclared_gaps, CaseResult, CheckKind, Quarantined, ScopeUnit, TestRow, TestUnit,
    discover_ts_packages, set_repo_root,
    is_node_test_file, node_test_plan, package_test_script,
};

mod nightly_all;

// #4446 — a scheduled job logs its own failure.
#[allow(dead_code)] // each crate uses part of the shared helper
mod service_lifecycle {
    include!("../../shared/service_lifecycle.rs");
}

fn main() {
    // #4446 — under launchd, a failed run is logged as service.failed (shared/service_lifecycle.rs).
    service_lifecycle::run_as_job();
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
    // #3920 fold — `werk-test --nightly`: the 03:00 cargo lane runs through THIS
    // verb, so nextest (#3929), the needs-stack typed skips (#3919), and the
    // per-case TestResult posts (#3592) apply at 03:00 identically to the gate.
    // #4145 — `--nightly --run-all`: the whole run (pre-checks, lanes, census,
    // record, nudges, readout) in the runner; launchd's 03:00 job.
    if args.iter().any(|a| a == "--nightly") {
        // read-only modes first (--last-run / --load-gate / --lock-probe): they
        // must never fall through to a real run (a test's --lock-probe did, 19:30)
        if let Some(r) = nightly_all::run_mode(args) {
            return r;
        }
        if args.iter().any(|a| a == "--run-all") {
            return nightly_all::run_all(args);
        }
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

    // #4419 — WERK_TEST_TREE points the run at another tree (canonical main, to
    // replay a landed commit named by WERK_TEST_REPLAY) instead of the card's werk.
    let werk = match std::env::var("WERK_TEST_TREE") {
        Ok(t) if !t.is_empty() => t,
        _ => {
            let werk_base =
                std::env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE unset".to_string())?;
            format!("{}/{}-{}", werk_base, role, card)
        }
    };
    if !Path::new(&werk).is_dir() {
        return Err(format!("werk not found: {}", werk));
    }
    // #4424 — TS packages are discovered in the tree under test.
    set_repo_root(Path::new(&werk));
    let trace = std::env::var("CHORUS_TRACE_ID").unwrap_or_default();
    std::env::set_var("WERK_TEST_TREE_ROOT", &werk);

    let changed_all = git_changed_files(&werk)?;
    // #4138 — a deleted test is retired, not red: a path gone from the tree is
    // reported here and never becomes a unit (bats on a missing file = "does not exist" = FAIL).
    let (changed, deleted) = werk_test::split_deleted(&changed_all, |f| Path::new(&werk).join(f).is_file());
    if !deleted.is_empty() {
        println!(
            "test.plan.deleted | {} path(s) on the diff no longer exist in the werk and are not units: {}",
            deleted.len(),
            deleted.join(", ")
        );
    }
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
                // #4169 — Jeff, 2026-09-13: "we must never fall back to the whole
                // tree that is always wrong for a card we fail immediately and fix
                // the data". An unmapped path is a DATA defect; widening hides it
                // behind an hour of other people's reds (#4166: 314 units, 61 min,
                // and not one of the nine failures was that card's change).
                let reason = diff_scope_reason(&werk, &changed);
                if let Some(path) = unmapped_path(&reason) {
                    emit_spine("test.scope.refused", &role, &card, &trace,
                        &[("reason", &reason), ("path", path)]);
                    eprintln!("scope(diff): REFUSED — {} is claimed by no unit.", path);
                    eprintln!("  Nothing ran. Map that path (or its directory) and re-run.");
                    eprintln!("  A card never runs the whole tree: that is the nightly's job.");
                    std::process::exit(2);
                }
                emit_spine("test.scope.full", &role, &card, &trace,
                    &[("reason", &reason), ("units", &full_units.len().to_string())]);
                println!("scope(diff): FULL — {} (deliberate, not a fallback)", reason);
                full_units
            }
        }
    };
    // #4440 — ONE selection rule (Jeff 2026-10-06: "we dont need 4 conflicting
    // set of rules"): the tests that exercise the changed files, below. The
    // units above only say which crates and packages to BUILD and check; the
    // bats-by-name lane (#3917) and jest --findRelatedTests (#3912) are gone —
    // the exercise rule names the same suites by the same path, and follows
    // imports transitively itself.
    let mut units = units;
    // #4419 — the registered tests (any layer) that exercise each changed file.
    // A changed file in a unit that no test exercises is refused and named.
    // Jeff, 2026-10-02: the card's domain tests run HERE, in werk-test and the
    // demo, before the land — never after it.
    let dsel = domain_select(&werk, &changed, &rows);
    match &dsel {
        None => println!("domain-select: UNMEASURED — no registered tests to read; import-graph selection only"),
        Some(d) => {
            println!(
                "domain-select: {} changed file(s) touch domain(s) [{}] → {} registered test file(s)",
                changed.len(),
                d.domains.iter().cloned().collect::<Vec<_>>().join(", "),
                d.tests.len()
            );
            if args.iter().any(|a| a == "--explain" || a == "--select-only") {
                for (f, why) in &d.tests {
                    println!("domain-select:   {f} ({why})");
                }
            }
            // #4419 reopened (Wren, Jeff 2026-10-06: "we already fixed that bug") — a
            // changed file in a package with no domain used to run its whole
            // package (#4438: 13 of 14 files → whole packages). #4169 and Jeff
            // 2026-09-13: never widen; fail and fix the data. Refuse, naming each.
            let refused = werk_test::untagged_in_a_unit(&d.untagged, &|f: &str| werk_test::ts_package_of(f).or_else(|| crate_of(f)));
            for f in d.untagged.iter().filter(|f| !refused.iter().any(|(r, _)| r == *f)) {
                println!("domain-select: {} — no registered test exercises it, and it is in no package", f);
            }
            // #4419 reopened — what would otherwise pass with nothing run
            let runnable = |f: &str| werk_test::runnable_test(f, werk_test::ts_package_of(f).is_some());
            let unrun: Vec<(String, &str)> = werk_test::unrun_changes(&changed, d, &runnable).into_iter()
                .filter(|(f, _)| !refused.iter().any(|(r, _)| r == f)).collect();
            if !unrun.is_empty() {
                for (f, why) in &unrun {
                    eprintln!("domain-select: REFUSED — {}: {}. Give it a test werk-test runs (bats, jest in its package, cargo tests/), and re-run.", f, why);
                }
                eprintln!("  Nothing ran. A change no runnable test covers never passes as green (#4420 run 8).");
                emit_spine("test.scope.refused", &role, &card, &trace,
                    &[("reason", "unrun"), ("path", unrun[0].0.as_str()), ("count", &unrun.len().to_string())]);
                std::process::exit(2);
            }
            if !refused.is_empty() {
                for (f, unit) in &refused {
                    eprintln!("domain-select: REFUSED — {} (in {}): no registered test exercises it. Add or register the test that does, and re-run.", f, unit);
                }
                eprintln!("  Nothing ran. A card never widens to a whole package for an unexercised file (#4169).");
                emit_spine("test.scope.refused", &role, &card, &trace,
                    &[("reason", "untagged"), ("path", refused[0].0.as_str()), ("count", &refused.len().to_string())]);
                std::process::exit(2);
            }
            for f in d.tests.keys() {
                if let Some(u) = unit_for_test(f) {
                    if !units.contains(&u) {
                        units.push(u);
                    }
                }
            }
            emit_spine("test.selection.domain", &role, &card, &trace,
                &[("domains", &d.domains.iter().cloned().collect::<Vec<_>>().join(",")),
                  ("tests", &d.tests.len().to_string()),
                  ("untagged", &d.untagged.len().to_string())]);
        }
    }
    let units = units;
    // #4419 — `--select-only`: print what this diff selects and why, run nothing.
    // Jeff 2026-10-06: "how do u figure out the tests to run for a file what r u querying".
    if args.iter().any(|a| a == "--select-only") {
        for u in &units {
            println!("select-only: {}", unit_name(u));
        }
        return Ok(0);
    }

    // #4392 — build, in the werk, what the selected suites run, before any of
    // them runs. A suite that runs an unbuilt or stale binary is red for the
    // test world, not the product (four #4335 runs on 2026-09-27).
    // Rule 3 (Jeff + Kade, 2026-09-27): a suite whose build failed reports
    // UNMEASURED, by name, instead of running against a missing or stale build.
    let unbuilt: std::collections::BTreeMap<String, String> = if args.iter().any(|a| a == "--explain") {
        Default::default()
    } else {
        let m = build_for_suites(&werk, &units);
        for (suite, why) in &m {
            println!("   build FAILED for {}: {} — the suite reports UNMEASURED", suite, why);
            emit_spine("test.build.failed", &role, &card, &trace, &[("message", &format!("{}: {}", suite, why))]);
        }
        m
    };

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

    // #3912 — registry unreachable → FULL per-package fallback, loudly labeled.
    // Otherwise jest runs exactly the exercise rule's files (#4440).
    let mut jplan = jest_plan(plan_source == "model", &rows, &[]);
    // #4419 — the domain lane's TS files join the selection, and an untagged
    // file's package runs every registered test it holds.
    let mut domain_reasons: std::collections::BTreeMap<String, String> = Default::default();
    if let (JestPlan::Selected(ref mut sels), Some(d)) = (&mut jplan, &dsel) {
        let mut add = |file: &str, why: &str| {
            let Some(p) = werk_test::ts_package_of(file) else { return };
            if !(file.ends_with(".ts") || file.ends_with(".js") || file.ends_with(".cjs")) {
                return;
            }
            domain_reasons.entry(file.to_string()).or_insert_with(|| why.to_string());
            match sels.iter_mut().find(|s| s.package == p) {
                Some(s) => {
                    if !s.test_files.iter().any(|f| f == file) {
                        s.test_files.push(file.to_string());
                    }
                }
                None => sels.push(werk_test::JestSelection { package: p, test_files: vec![file.to_string()] }),
            }
        };
        for (f, why) in &d.tests {
            add(f, why);
        }
        for s in sels.iter_mut() {
            s.test_files.sort();
            s.test_files.dedup();
        }
    }
    if let JestPlan::FullFallback { ref reason } = jplan {
        println!("jest-select: {}", reason);
    } else if let JestPlan::Selected(ref sels) = jplan {
        let n: usize = sels.iter().map(|s| s.test_files.len()).sum();
        println!("jest-select: {} registered unit test file(s) cover the diff (registry-answered)", n);
    }
    // #3931 — the selection is EVIDENCE, not a count: name every selected file
    // with its reason (or the fallback's reason) on stdout and the spine, so an
    // under-selection is inspectable from the record alone.
    let mut sel_details = werk_test::selection_details(&jplan);
    for d in sel_details.iter_mut() {
        if let Some(why) = domain_reasons.get(&d.file) {
            d.reason = why.clone();
        }
    }
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
    // #4236 — both numbers come from the SELECTION. An empty selection means
    // the lane did not narrow anything, so the unit's rows are the honest count.
    let chosen: std::collections::BTreeSet<String> =
        sel_details.iter().map(|d| d.file.clone()).collect();
    // #4238 — which packages the jest lane NARROWED. Every other unit (cargo
    // crate, bats suite) runs whole, so its needs-stack rows are all selected.
    let narrowed: Vec<String> = match &jplan {
        JestPlan::Selected(sels) => sels.iter().map(|s| s.package.clone()).collect(),
        JestPlan::FullFallback { .. } => Vec::new(),
    };
    let narrows = |f: &str| narrowed.iter().any(|p| f.starts_with(&format!("{p}/")));
    let selected_ns: Vec<String> = ns_all
        .iter()
        .filter(|f| in_units(f) && (!narrows(f) || chosen.contains(f.as_str())))
        .cloned()
        .collect();
    let registered_in_unit = rows.iter()
        .filter(|r| r.hermeticity == "needs-stack" && in_units(&r.file_path))
        .count();
    // count REGISTERED TESTS (rows), not files — the report must count what it names
    let selected_ns_tests =
        werk_test::needs_stack_in_selection(&rows, &narrowed, &chosen, &in_units);
    if selected_ns_tests == 0 && registered_in_unit > 0 {
        println!("{}", werk_test::integration_report_none_selected(registered_in_unit));
    }
    // #4336 — an empty needs-stack selection is only "nothing needs the stack"
    // when the registry answered. On the fallback plan the rows are missing, so
    // empty means UNKNOWN: probe the stack, or RUN_INTEGRATION goes true with no
    // stack behind it and bare jest lists the integration tier (the #4111 state).
    let stack = if selected_ns.is_empty() && plan_source != "fallback" {
        werk_test::StackState::Up
    } else {
        stack_state_now()
    };
    let stack_down: Option<String> = stack_down_of(&stack);
    // #4102 — the werk lane never set RUN_INTEGRATION, so every bats
    // integration case self-skipped and reported `ok` in a green run (the
    // three ACs of #4102 among them). The nightly lane sets it from the same
    // probe; the werk lane must too, or a pipeline cannot prove an AC that
    // needs the stack.
    if stack_down.is_none() {
        std::env::set_var("RUN_INTEGRATION", "true");
    }
    let ns_excluded: Vec<String> = if stack_down.is_some() { selected_ns.clone() } else { Vec::new() };
    if let werk_test::StackState::Unmeasurable(u) = &stack {
        println!("{}", werk_test::integration_report_unmeasurable(selected_ns_tests, u));
        emit_spine("test.integration.unmeasurable", &role, &card, &trace,
            &[("count", &selected_ns_tests.to_string()), ("saw", u)]);
    } else {
        // #4251 — when the stack is UP there is no plan line. It claimed a
        // count before any lane had decided what to run, and was wrong three
        // different ways in two days: 134 "ran" when one file was selected
        // (#4236), 498 when 75 ran (#4238), 532 when 109 ran (this card). The
        // measured line after the run is the only honest one. The typed SKIP
        // and UNMEASURABLE states below still print — they are real states the
        // run knows before it starts, not predictions of what will run.
        if stack_down.is_some() {
            println!("{}", werk_test::integration_report(selected_ns_tests, stack_down.as_deref()));
        } else if selected_ns_tests == 0 {
            println!("{}", werk_test::integration_report(0, None));
        }
        if let Some(down) = &stack_down {
            emit_spine("test.integration.skipped", &role, &card, &trace,
                &[("count", &selected_ns_tests.to_string()), ("stack_down", down)]);
        }
    }
    // #3920 — the browser lane: registered testConcern=ui files run as ONE
    // workspace check when the diff touches a ui surface. Stack-gated like any
    // needs-stack tier; zero registered = explicit absence, never vacuous.
    let ui_set = werk_test::ui_files(&rows);
    let ui_fired = werk_test::ui_lane_fires(&changed)
        || std::env::var("WERK_TEST_FULL").map(|v| v == "1").unwrap_or(false);
    let ui_check = werk_test::ui_plan(ui_fired, ui_set.len());
    let mut any_failed = false;
    // #4265 — suites that scored nothing, named in the summary so an
    // UNMEASURED run can never be read as a clean one.
    let mut unmeasured: Vec<String> = Vec::new();
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
        let cases_before = all_cases.len();
        // #4454 — the unit starts on the trace: a hung unit reads started, no end
        emit_spine("test.unit.started", &role, &card, &trace,
            &[("check", check.kind.label()), ("unit", target), ("message", &format!("{} {} started", check.kind.label(), target))]);
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
                // #4440 reopen — only test files changed: run just those binaries
                let only = werk_test::cargo_only_bins(c, &changed);
                let (ok, cases) = if only.is_empty() {
                    run_cargo(&werk, c, &q_names, &ns_refs)
                } else {
                    println!("cargo-select: {} → only {} (no src change; the tests that changed run)", c, only.join(", "));
                    let only_refs: Vec<&str> = only.iter().map(|s| s.as_str()).collect();
                    let (ok, cases, _) = run_cargo_sel(&werk, c, &q_names, &ns_refs, &only_refs);
                    (ok, cases)
                };
                let crate_dir = format!("platform/services/{}", c);
                for (path, result) in cases {
                    match werk_test::match_cargo_case_path(&path, &crate_dir, &rows, &row_names) {
                        Some(fp) => all_cases.push(CaseResult {
                            file_path: fp,
                            test_name: werk_test::nextest_bare_name(&path).to_string(),
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
            (CheckKind::Bats, Some(TestUnit::BatsSuite(s))) => {
                // #4265 — a bats suite can report UNMEASURED (exit 2): it ran,
                // found its subject unserved, and scored nothing. It must not
                // read as a pass — the line says UNMEASURED and the run is not
                // green on its account — and it must not read as a failure,
                // because nothing was shown broken.
                if unbuilt.contains_key(s) {
                    unmeasured.push(target.to_string());
                    true
                } else {
                    let (outcome, cases) = run_bats(&werk, s);
                    all_cases.extend(cases);
                    match outcome {
                        BatsOutcome::Pass => true,
                        BatsOutcome::Unmeasured => {
                            unmeasured.push(target.to_string());
                            true
                        }
                        BatsOutcome::Fail => false,
                    }
                }
            }
            (CheckKind::ClippyRatchet, None) => run_clippy_ratchet(&werk),
            (CheckKind::LintRatchet, None) => werk_test::run_lint_ratchet(&werk),
            (CheckKind::DocCoherence, None) => run_doc_coherence(&werk),
            _ => true, // unreachable given check_plan's construction
        };
        let secs = check_started.elapsed().as_secs_f64();
        // #4454 — this check's cases on the trace, as soon as it ends, and
        // whatever its fixtures said
        emit_case_events(target, &all_cases[cases_before..], &role, &card, &trace);
        forward_test_events(&werk);
        unit_costs.push((format!("{}:{}", check.kind.label(), target), secs));
        let verdict = if unmeasured.last().map(|u| u == target).unwrap_or(false) {
            "UNMEASURED"
        } else if ok { "ok" } else { "FAIL" };
        // #4436 — every unit's wall time on its own line, and with its result on
        // the spine, so "why did this run take an hour" has an answer per unit.
        println!("   {}:{} … {} {}", check.kind.label(), target, verdict, werk_test::fmt_secs(secs));
        emit_spine("test.unit.timed", &role, &card, &trace,
            &[("check", check.kind.label()), ("unit", target), ("verdict", verdict),
              ("seconds", &format!("{:.1}", secs))]);
        if !ok {
            any_failed = true;
            failed_count += 1;
            let msg = werk_test::unit_failure_message(check.kind.label(), target);
            emit_spine(
                "test.failed",
                &role,
                &card,
                &trace,
                &[
                    ("check", check.kind.label()),
                    ("unit", target),
                    ("message", msg.as_str()),
                ],
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
            let word = match ok { Some(true) => "ok", Some(false) => "FAIL", None => "UNMEASURED" };
            println!("   {}:workspace … {}{}", kind.label(), word, summary);
            match ok {
                Some(false) => {
                    any_failed = true;
                    failed_count += 1;
                    let msg = werk_test::unit_failure_message(kind.label(), "workspace");
                    emit_spine("test.failed", &role, &card, &trace,
                        &[("check", kind.label()), ("unit", "workspace"), ("message", msg.as_str())]);
                }
                // #4154 — nothing ran and nothing crashed: typed, visible, not red.
                None => emit_spine("test.unmeasured", &role, &card, &trace,
                    &[("check", kind.label()), ("unit", "workspace"), ("reason", "selected-no-spec")]),
                Some(true) => {}
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
    // #4436 — the slowest units and the step total, every run, not only when
    // the budget is blown: the hour on #4432 run 11 had no per-unit answer.
    print!("{}", werk_test::slowest_units_report(&unit_costs, 10, phase_elapsed));
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
    // #4265 — an unmeasured suite is reported before the verdict, so a run
    // that proved nothing about a suite cannot be read as having passed it.
    if !unmeasured.is_empty() {
        println!("   UNMEASURED: {} suite(s) scored nothing — {}",
                 unmeasured.len(), unmeasured.join(", "));
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
    // #4139 — the measured integration line: what ran, after it ran
    if stack_down.is_none() && selected_ns_tests > 0 {
        let executed_ns = all_cases.iter().filter(|c| selected_ns.iter().any(|f| f == &c.file_path)).count();
        println!("{}", werk_test::integration_measured_report(selected_ns_tests, executed_ns));
    }
    let (joined, unregistered) = werk_test::join_cases(&all_cases, &rows, &row_names, &row_entities);
    if unregistered > 0 {
        emit_spine("testresult.unregistered", &role, &card, &trace,
            &[("count", &unregistered.to_string())]);
        println!("executed-but-unregistered: {} case(s) (no registered Test identity — not posted)", unregistered);
    }
    // #4015 — same rule on the card path as on the nightly: a run whose evidence
    // did not survive has not proven anything, so it must not exit clean.
    let stored = post_test_results(&role, &card, &trace, &joined, run_epoch_ms, 0, &|_| String::new());
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
    // #4440 reopen — name the tree, so its TS packages are found from launchd's cwd `/`
    set_repo_root(Path::new(&root));
    let role = "system".to_string();
    let card = String::new(); // typed absence — a nightly run has no card
    // #4454 reopen — launchd starts the nightly with no CHORUS_TRACE_ID, so all
    // 9,910 case events of 10-09 03:00 carried trace="" and no one query could
    // pull the run. Mint one per run and hand it to every child (the test
    // helpers read CHORUS_TRACE_ID), so the run is one trace.
    let trace = werk_test::run_trace_id(
        std::env::var("CHORUS_TRACE_ID").ok().as_deref(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0),
        std::process::id(),
    );
    std::env::set_var("CHORUS_TRACE_ID", &trace);
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
    // #4131 — a crate is a directory with a Cargo.toml. The registry yields
    // names from test filePaths, and platform/services/shared/ is a SOURCE
    // directory other crates include; planning it produced a suite row that
    // could only ever read UNMEASURED ("no parseable output"). #4012 taught the
    // coverage denominator this; the plan learns it here.
    let crates: Vec<String> = werk_test::nightly_cargo_crates(&rows)
        .into_iter()
        .filter(|c| only.as_deref().map(|o| o == c).unwrap_or(true))
        .filter(|c| Path::new(&format!("{}/platform/services/{}/Cargo.toml", root, c)).is_file())
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
    let stack = if ns_all.is_empty() { werk_test::StackState::Up } else { stack_state_now() };
    let stack_down: Option<String> = stack_down_of(&stack);
    if let werk_test::StackState::Unmeasurable(u) = &stack {
        println!("{}", werk_test::integration_report_unmeasurable(ns_total, u));
        // the nightly folds this to an UNMEASURED row on the page — not green
        println!("nightly-unit|probe|platform/services/werk-test/stack-probe|unmeasured|0 pass, 0 fail (UNMEASURED — stack probe timed out: {}; {} needs-stack test(s) not run)", u, ns_total);
        emit_spine("test.integration.unmeasurable", &role, &card, &trace,
            &[("count", &ns_total.to_string()), ("saw", u)]);
    } else {
        println!("{}", werk_test::integration_report(ns_total, stack_down.as_deref()));
    }

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
    // #4063 — a werk-rooted nightly never writes prod's ledger (see
    // nightly_writeback_withheld). Withheld cases are counted and named once,
    // never silently, and never as "lost": nothing was promised to the store.
    let endpoint_overridden = ["OWL_API_TESTRESULTS", "OWL_API_TESTRESULTS_BATCH"]
        .iter()
        .any(|k| std::env::var(k).map(|v| !v.trim().is_empty()).unwrap_or(false));
    let withheld = werk_test::nightly_writeback_withheld(&root, endpoint_overridden);
    let withheld_total = AtomicUsize::new(0);
    // #4139 — needs-stack cases that actually produced a result this run
    let executed_ns = AtomicUsize::new(0);
    if withheld {
        println!("!! werk nightly: results will NOT be written to the prod test ledger — root {} is a werk; set OWL_API_TESTRESULTS to the werk's store to write (#4063)", root);
    }
    // #4155 — `reasons` are (file, case, text) as the runner read them; a
    // failed case takes its own, else its file's (a suite that died before
    // any case ran), else the unit's (an empty file and case name).
    // #4454 reopen — `runs` is (target, path) per case, in case order, for a
    // cargo unit; empty for runners with neither.
    let store_unit = |unit: &str, cases: &[CaseResult], runs: &[(String, String)], reasons: &[(String, String, String)], times: &std::collections::HashMap<String, u64>| {
        if cases.is_empty() {
            return;
        }
        let reason_of = |c: &CaseResult| -> String {
            if c.result != "fail" {
                return String::new();
            }
            reasons.iter()
                .find(|(f, n, _)| *f == c.file_path && *n == c.test_name)
                .or_else(|| reasons.iter().find(|(f, n, _)| *f == c.file_path && n.is_empty()))
                .or_else(|| reasons.iter().find(|(f, n, _)| f.is_empty() && n.is_empty()))
                .map(|(_, _, r)| r.clone())
                .unwrap_or_default()
        };
        executed_ns.fetch_add(cases.iter().filter(|c| ns_all.contains(&c.file_path)).count(), Ordering::SeqCst);
        // #4155 — the reason, as its own line, the moment the case fails,
        // for every failed case: a werk run that withholds its ledger writes
        // and a case the registry does not know still say why they failed.
        // The log keeps the line, Loki gets it on the event below, the graph
        // gets it on the result row.
        let whys: std::collections::HashMap<(String, String), (String, String)> = cases
            .iter()
            .filter(|c| c.result == "fail")
            .map(|c| {
                let why = werk_test::why::why_line(&c.file_path, &c.test_name, &reason_of(c));
                println!("{}", why);
                let (_, _, kind, reason) = werk_test::why::parse_why_line(&why).unwrap_or_default();
                ((c.file_path.clone(), c.test_name.clone()), (kind, reason))
            })
            .collect();
        // #4454 — every case is an event on the run's trace, passed, failed or
        // skipped, with its time and (failed) its reason: one Loki query on the
        // trace says which case, how long, and why. Before the withheld return,
        // so a werk-rooted run logs its cases too. One process for the unit.
        let events: Vec<String> = cases.iter().enumerate().map(|(i, c)| {
            let (kind, reason) = whys.get(&(c.file_path.clone(), c.test_name.clone())).cloned().unwrap_or_default();
            let (target, path) = runs.get(i).map(|(t, p)| (t.as_str(), p.as_str())).unwrap_or(("", ""));
            werk_test::batch_line(&werk_test::case_event_args_at(c, times.get(&c.test_name).copied(), unit,
                &kind, &reason, &mint_role, &card, &trace, target, path))
        }).collect();
        emit_spine_batch(&events);
        forward_test_events(&root);
        if withheld {
            withheld_total.fetch_add(cases.len(), Ordering::SeqCst);
            println!("nightly-withheld|{}|{} (werk root, prod ledger untouched)", unit, cases.len());
            return;
        }
        let (joined, unregistered) = werk_test::join_cases(cases, &rows, &row_names, &row_entities);
        // #4033 — claim this unit's slice of the run's index space first, so
        // concurrent units never mint the same name (fetch_add is the claim).
        let idx_base = werk_test::claim_index_base(&expected_total, joined.len());
        let stored = post_test_results(&mint_role, &card, &trace, &joined, run_epoch_ms, idx_base, &reason_of);
        stored_total.fetch_add(stored, Ordering::SeqCst);
        unregistered_total.fetch_add(unregistered, Ordering::SeqCst);
        // #4145 — the run's own record of what it posted, one line per case,
        // so the census is "registry minus these" and never a ledger walk.
        // #4247 — the result rides with the identity. Without it the log had a
        // result per SUITE and an identity per TEST, two units that cannot be
        // compared; the census below reads this same line.
        for (c, _, registered) in &joined {
            // #4271 — four fields now. The case's own name is its identity; the
            // registered row it answers for is a SUFFIX of that name, so the
            // line carries the suffix's byte LENGTH rather than a second copy
            // of the name. A second copy is a second record of one fact, and
            // two records that must agree is the defect this card exists to
            // kill. A name cannot be swallowed by the split because digits
            // cannot, and an old three-field line still parses.
            println!(
                "nightly-case|{}|{}|{}|{}",
                c.file_path,
                c.test_name,
                c.result,
                werk_test::nightly_run::registered_suffix_len(&c.test_name, registered)
            );
        }
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
    // #4440 reopen — a run that plans no TS package on a tree that has them
    // measured nothing, and must say so rather than read as a quiet night.
    if only.is_none() {
        if let Some(why) = werk_test::npm_lane_unmeasured(&discover_ts_packages(Path::new(&root)), &ts_pkgs) {
            // the report's own unit line, so the page shows it as unmeasured (#798's shape)
            println!("nightly-unit|npm|ts-packages|unmeasured|0 pass, 0 fail ({})", why);
        }
    }
    // #3974 — bats lane: registered suites from the registry, per-case TAP
    // results (boolean-only bats is over).
    // #4131 — a suite that can only ever self-refuse unattended (it boots out
    // every com.chorus.* agent and needs an explicit restore grant, #4004) has
    // no place in an unattended plan: it read "skipped" every night. The
    // wrapper's NIGHTLY_DESTRUCTIVE_SUITES names them; same list, same seam.
    // #4135 — test-product-membrane.sh was excluded here as destructive (it
    // boots every com.chorus.* agent). Since #4131 it is a freshness VERDICT
    // under the nightly and refuses attended runs without an explicit grant,
    // so it is safe to plan; excluding it left a registered row no lane could
    // ever emit (LANE SILENT on the reconcile every night). Default: none.
    let destructive: Vec<String> = std::env::var("NIGHTLY_DESTRUCTIVE_SUITES")
        .unwrap_or_default()
        .split_whitespace().map(|s| s.to_string()).collect();
    let bats_suites: Vec<String> = werk_test::nightly_bats_suites(&rows)
        .into_iter()
        .filter(|b| only.as_deref().map(|o| o == b).unwrap_or(true))
        .filter(|b| !destructive.iter().any(|d| b.ends_with(&format!("/{}", d)) || b == d))
        .collect();
    // #3922 — security-declared units fold under their own lane label so the
    // report and owner routing see ONE security lane on its own cadence.
    let sec_units = werk_test::security_units(&rows);
    let perf_units = werk_test::perf_units(&rows);
    let bats_kind = |b: &str| -> &'static str {
        if perf_units.contains(b) { "perf" } else if sec_units.contains(b) { "security" } else if b.ends_with(".sh") { "shell" }
        // #4292 — the two new file suites carry their own kind on the report
        else if werk_test::is_feature_suite(b) { "bdd" } else if werk_test::is_unittest_suite(b) { "py" } else { "bats" }
    };
    // #4160 — the run goes by TEST TYPE, in Jeff's order (2026-09-12):
    // coverage, lint, smoke (the outer legs), then security, unit, contract,
    // integration, fitness, bdd, e2e, ui, perf. Each unit sits in the stage its
    // registered testType names: a crate's typed test binaries leave its unit
    // run for their own stage, and platform/api's integration project runs as
    // its own row. The three tool pools (cargo, then npm, then files) ran every
    // type mixed in tool order; that is what this replaces.
    let integration_on = stack_down.is_none();
    let split_pkgs: Vec<String> = ts_pkgs.iter()
        .filter(|p| jest_has_hermetic_project(&format!("{}/{}", root, p)))
        .cloned().collect();
    let security_pkgs: std::collections::BTreeSet<String> = ts_pkgs.iter()
        .filter(|p| sec_units.contains(*p)).cloned().collect();
    let stages = werk_test::plan_stages(&crates, &ts_pkgs, &bats_suites, &rows, &split_pkgs, &security_pkgs, integration_on);
    let legs_noop = std::env::var("NIGHTLY_LEGS_NOOP").is_ok();
    let npm_kind_of = |p: &str| -> &'static str { if security_pkgs.contains(p) { "security" } else { "npm" } };
    // #4030 AC4 — the PLAN, printed before any lane runs. A planned unit that
    // never produces its `nightly-unit|` line is folded by nightly-suites.sh
    // into a red NEVER RAN row (`never_ran_units`): a run killed at a cap can
    // no longer report only the units it got to and read as "3 red".
    for sp in &stages {
        for c in &sp.cargo {
            let (k, u) = werk_test::stage_item_label(sp.stage, "cargo", c);
            println!("{}", werk_test::nightly_plan_line(&k, &u));
        }
        for p in &sp.npm {
            let (k, u) = werk_test::stage_item_label(sp.stage, npm_kind_of(p), p);
            println!("{}", werk_test::nightly_plan_line(&k, &u));
        }
        for b in &sp.files {
            println!("{}", werk_test::nightly_plan_line(bats_kind(b), b));
        }
        if sp.stage == "ui" && !legs_noop {
            println!("{}", werk_test::nightly_plan_line("ui", "proving/flows"));
        }
    }
    // #4022 — load-aware widths: each lane gets a share of the box, and no pool
    // takes a new unit while the 1-minute load is over cap (2× cores). Env
    // overrides keep the old knobs; the defaults are the box's. nextest and
    // jest are internally parallel, so their pools stay narrower than bats.
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
    // #3559/#3974 — platform/api's INTEGRATION jest project is only
    // constructed under RUN_INTEGRATION=true; the nightly sets it from the
    // live stack probe so integration tests run with the stack and are
    // typed-absent without it.
    if integration_on {
        std::env::set_var("RUN_INTEGRATION", "true");
    }
    let npm_workers: usize = std::env::var("NIGHTLY_NPM_WORKERS").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(nw_default);
    // (#4152: one isolation list for the npm and file lanes)
    let iso_conf = std::fs::read_to_string(
        Path::new(&root).join("platform/scripts/nightly-isolation.conf"))
        .unwrap_or_default();
    let explicit_iso: Vec<String> = iso_conf.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let bats_workers: usize = std::env::var("NIGHTLY_SUITE_WORKERS").ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(bats_default);

    // #4030 AC3 — one bats runner for the file pools: run, then store now.
    let run_bats_stored = |werk: &str, b: &str| -> (bool, Vec<(String, String)>, String) {
        // #4126 — one timestamp per unit: a slow unit names its own seconds,
        // a wedged one never prints.
        let unit_started = std::time::Instant::now();
        // #4454 — the unit starts on the trace: a hung unit reads started, no end
        emit_spine("test.unit.started", &mint_role, &card, &trace,
            &[("check", bats_kind(b)), ("unit", b), ("message", &format!("{} started", b))]);
        let r = run_bats_cases(werk, b);
        let unit_ms = unit_started.elapsed().as_millis();
        let mut cases: Vec<CaseResult> = r.1.iter()
            .map(|(n, res)| CaseResult { file_path: b.to_string(), test_name: n.clone(), result: res.clone() })
            .collect();
        // #4063/#4131 — the registry holds ONE test per .sh named by the file;
        // record the file-level verdict alongside any TAP cases so the ledger
        // cross-foots.
        if b.ends_with(".sh") || bats_kind(b) == "shell" {
            let ident = b.rsplit('/').next().unwrap_or(b);
            if !cases.iter().any(|c| c.test_name == ident) {
                cases.push(werk_test::shell_suite_case(b, r.0));
            }
        }
        println!("nightly-elapsed|{}|{}|{}ms", bats_kind(b), b, unit_ms);
        // #4155 — TAP comment lines per failed case; the unit's last lines
        // for a suite that prints no per-case reason (a shell suite, a
        // feature, a unittest file)
        let mut reasons: Vec<(String, String, String)> = werk_test::why::bats_case_reasons(&r.2)
            .into_iter().map(|(n, t)| (b.to_string(), n, t)).collect();
        if !r.0 {
            reasons.push((b.to_string(), String::new(), werk_test::why::tail_reason(&r.2, 4)));
        }
        store_unit(b, &cases, &[], &reasons, &werk_test::bats_case_times(&r.2));
        r
    };
    let (mut cargo_waits, mut npm_waits, mut bats_waits) = (0usize, 0usize, 0usize);

    for sp in &stages {
        let stage = sp.stage;
        let ui_here = stage == "ui" && !legs_noop;
        if sp.is_empty() && !ui_here {
            // AC5 — a type with nothing registered says so; the outer legs own
            // coverage, lint, smoke and the perf ratchet.
            if !werk_test::LEG_COVERED_STAGES.contains(&stage) {
                println!("{}", werk_test::nightly_stage_absent_line(stage));
            }
            continue;
        }
        println!("-- #4160 stage {}: {} cargo, {} npm, {} file suite(s){} --",
            stage, sp.cargo.len(), sp.npm.len(), sp.files.len(), if ui_here { ", playwright" } else { "" });

        // ── cargo: `crate` is its unit run, `crate#stem` one typed binary ──
        let cargo_root = root.clone();
        let (cargo_results, waits) = werk_test::run_pool_gated(&sp.cargo, cargo_workers, cap, read_loadavg, gate_wait, gate_tick, |item| werk_test::time_unit(item, || {
            // #4454 — the unit starts on the trace: a hung unit reads started, no end
            emit_spine("test.unit.started", &mint_role, &card, &trace,
                &[("check", "cargo"), ("unit", item), ("message", &format!("cargo {} started", item))]);
            let ns_bins = ns_bins_for(item.split('#').next().unwrap_or(item));
            let (c, stem) = match item.split_once('#') {
                Some((c, s)) => (c, Some(s)),
                None => (item, None),
            };
            let typed: Vec<String> = werk_test::crate_binary_types(c, &rows).into_iter().map(|(s, _)| s).collect();
            let (ok, cases, ns_len, text) = match stem {
                // a needs-stack binary with the stack down: typed skip, never run
                Some(s) if ns_bins.iter().any(|b| b == s) => (true, Vec::new(), 1, String::new()),
                Some(s) => {
                    let (ok, cases, text) = run_cargo_sel(&cargo_root, c, &q_names, &[], &[s]);
                    (ok, cases, 0, text)
                }
                None => {
                    let mut excl: Vec<&str> = ns_bins.iter().map(|s| s.as_str()).collect();
                    excl.extend(typed.iter().map(|s| s.as_str()));
                    let (ok, cases, text) = run_cargo_sel(&cargo_root, c, &q_names, &excl, &[]);
                    (ok, cases, ns_bins.iter().filter(|b| !typed.contains(b)).count(), text)
                }
            };
            // #4030 AC3 — join + store THIS unit's cases now, in the worker
            let crate_dir = format!("platform/services/{}", c);
            // #4155 — each failed case's panic block, keyed the way the case
            // is stored; the crate's tail when nextest printed none
            let panics = werk_test::why::nextest_case_reasons(&text);
            let mut reasons: Vec<(String, String, String)> = Vec::new();
            let mut matched: Vec<CaseResult> = Vec::new();
            // #4454 reopen — the build target each case ran under, in case order
            let targets: Vec<String> = werk_test::parse_nextest_case_runs(&text).into_iter().map(|(_, t, _)| t).collect();
            let mut runs: Vec<(String, String)> = Vec::new();
            for (i, (path, result)) in cases.iter().enumerate() {
                match werk_test::match_cargo_case_path(path, &crate_dir, &rows, &row_names) {
                    Some(fp) => {
                        runs.push((targets.get(i).cloned().unwrap_or_default(), path.clone()));
                        let bare = werk_test::nextest_bare_name(path).to_string();
                        if let Some(r) = werk_test::why::reason_for_nextest_path(&panics, path) {
                            reasons.push((fp.clone(), bare.clone(), r.to_string()));
                        }
                        matched.push(CaseResult { file_path: fp, test_name: bare, result: result.clone() });
                    }
                    None => { unmatched_cargo.fetch_add(1, Ordering::SeqCst); }
                }
            }
            if !ok {
                reasons.push((String::new(), String::new(), werk_test::why::tail_reason(&text, 4)));
            }
            // #4454 — nextest times by full path, keyed the way the case is stored
            let times: std::collections::HashMap<String, u64> = werk_test::nextest_case_times(&text)
                .into_iter().map(|(p, ms)| (werk_test::nextest_bare_name(&p).to_string(), ms)).collect();
            store_unit(item, &matched, &runs, &reasons, &times);
            (ok, cases, ns_len, stem.is_none() && !typed.is_empty())
        }));
        cargo_waits += waits;
        for (item, (ok, cases, ns_len, moved)) in cargo_results {
            let (kind, unit) = werk_test::stage_item_label(stage, "cargo", &item);
            werk_test::print_unit_time(&kind, &unit, &item);
            if ok && cases.is_empty() && moved && ns_len == 0 {
                println!("nightly-unit|{}|{}|skip|0 pass, 0 fail (no unit tests of its own — its typed test binaries ran in their own stages)", kind, unit);
                continue;
            }
            // #4078 — a skipped case is not a failure
            let (passed, case_failed, case_skipped) = werk_test::case_counts(cases.iter().map(|(_, r)| r.as_str()));
            println!("{}", werk_test::nightly_lane_line_with_skips(&kind, &unit, ok, passed, case_failed, case_skipped, ns_len));
            if !ok {
                any_failed = true;
                failed_count += 1;
                let msg = werk_test::unit_failure_message("cargo", &unit);
                emit_spine("test.failed", &role, &card, &trace,
                    &[("check", "cargo"), ("unit", &unit), ("message", msg.as_str())]);
            }
        }

        // ── npm: `pkg` (hermetic when it has projects), `pkg#integration` ──
        // #4152 — a package named in nightly-isolation.conf runs ALONE after
        // the pool (directing/products/cards reads Vikunja's SQLite and saw
        // "database is locked" beside another jest package).
        let npm_plan = werk_test::plan_parallel_units(&sp.npm,
            &|u| explicit_iso.iter().any(|e| e == u.split('#').next().unwrap_or(u)));
        let npm_root = root.clone();
        let run_pkg = |item: &str| werk_test::time_unit(item, || {
            // #4454 — the unit starts on the trace: a hung unit reads started, no end
            emit_spine("test.unit.started", &mint_role, &card, &trace,
                &[("check", "jest"), ("unit", item), ("message", &format!("jest {} started", item))]);
            let (p, project) = match item.split_once('#') {
                Some((p, proj)) => (p, Some(proj)),
                None if split_pkgs.iter().any(|s| s == item) => (item, Some("hermetic")),
                None => (item, None),
            };
            let (ok, cases, reasons) = run_jest_project(&npm_root, p, Some(jest_workers), project);
            let jest_times = take_case_times(&cases);
            // #4030 AC3 — stored the moment the package finishes
            store_unit(item, &cases, &[], &reasons, &jest_times);
            (ok, cases)
        });
        let (mut npm_results, w1) = werk_test::run_pool_gated(&npm_plan.parallel, npm_workers, cap, read_loadavg, gate_wait, gate_tick, run_pkg);
        let (alone_results, w2) = werk_test::run_pool_gated(&npm_plan.serialized, 1, cap, read_loadavg, gate_wait, gate_tick, run_pkg);
        npm_results.extend(alone_results);
        npm_waits += w1 + w2;
        for (item, (ok, cases)) in npm_results {
            let p = item.split('#').next().unwrap_or(&item).to_string();
            let (kind, unit) = werk_test::stage_item_label(stage, npm_kind_of(&p), &item);
            werk_test::print_unit_time(&kind, &unit, &item);
            let pkg_ns: Vec<String> = if stack_down.is_some() {
                ns_all.iter().filter(|f| f.starts_with(&format!("{}/", p))).cloned().collect()
            } else { Vec::new() };
            // #4004 — attribute each case to the package its FILE lives in:
            // jest's rootDir can reach past the package dir.
            let (mine, foreign): (Vec<_>, Vec<_>) = cases
                .into_iter()
                .partition(|c| werk_test::package_owns_case(&p, &c.file_path));
            if !foreign.is_empty() {
                let mut owners: Vec<&str> = foreign
                    .iter()
                    .map(|c| c.file_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("?"))
                    .collect();
                owners.sort_unstable();
                owners.dedup();
                println!(
                    "!! jest:{} ran {} case(s) whose files live OUTSIDE it ({}) — not counted on this row",
                    item, foreign.len(), owners.join(", ")
                );
            }
            if mine.is_empty() && !foreign.is_empty() {
                println!("!! jest:{} produced NO cases of its own — every result came from elsewhere", item);
            }
            let cases = mine;
            let (passed, case_failed, case_skipped) = werk_test::case_counts(cases.iter().map(|c| c.result.as_str()));
            // #4063 — name every failed case before the fold line
            for l in werk_test::failed_case_lines(&format!("jest:{}", item), &cases) {
                println!("{}", l);
            }
            println!("{}", werk_test::nightly_lane_line_with_skips(&kind, &unit, ok, passed, case_failed, case_skipped, pkg_ns.len()));
            if !ok {
                any_failed = true;
                failed_count += 1;
                let msg = werk_test::unit_failure_message("npm", &item);
                emit_spine("test.failed", &role, &card, &trace,
                    &[("check", "npm"), ("unit", &item), ("message", msg.as_str())]);
            }
        }

        // ── file suites (bats / sh / feature / py) ──
        // #4022 — independent subprocesses fan out; needs-stack READERS overlap
        // at 2; conf-listed MUTATORS run alone. Report order is the plan's.
        let plan = werk_test::plan_parallel_units(&sp.files,
            &|u| werk_test::unit_is_isolated(u, &rows, &explicit_iso));
        let (stack_readers, mutators) = werk_test::split_serialized(&plan.serialized, &explicit_iso);
        if !sp.files.is_empty() {
            println!("-- #4022 parallel plan ({}): {} suites fan out across {} workers, {} stack-readers at 2, {} mutators alone --",
                stage, plan.parallel.len(), bats_workers, stack_readers.len(), mutators.len());
        }
        let pool_root = root.clone();
        let (mut lane_results, w1): (Vec<(String, (bool, Vec<(String, String)>, String))>, usize) =
            werk_test::run_pool_gated(&plan.parallel, bats_workers, cap, read_loadavg, gate_wait, gate_tick, |b| werk_test::time_unit(b, || run_bats_stored(&pool_root, b)));
        let reader_root = root.clone();
        let (reader_results, w2) =
            werk_test::run_pool_gated(&stack_readers, 2, cap, read_loadavg, gate_wait, gate_tick, |b| werk_test::time_unit(b, || run_bats_stored(&reader_root, b)));
        lane_results.extend(reader_results);
        bats_waits += w1 + w2;
        for b in &mutators {
            lane_results.push((b.clone(), werk_test::time_unit(b, || run_bats_stored(&root, b))));
        }
        for (b, (ok, cases, text)) in lane_results {
            let b = &b;
            let kind = bats_kind(b);
            werk_test::print_unit_time(kind, b, b);
            // #4065 — a suite that DECLINED to run (rc=3) is its own verdict
            if werk_test::is_self_refused(&cases) {
                println!("{}", werk_test::nightly_lane_line_refused(kind, b));
                continue;
            }
            // #4273 — every case skipped, with a reason: a skip, not "no output"
            if werk_test::is_all_skipped(&cases) {
                let reason = werk_test::first_skip_reason(&text).unwrap_or_else(|| "no reason given".to_string());
                println!("{}", werk_test::nightly_lane_line_all_skipped(kind, b, cases.len(), &reason));
                continue;
            }
            // #4131 — shell suites report summary counts, not TAP cases
            let (passed, case_failed) = if cases.is_empty() && (kind == "shell" || b.ends_with(".sh")) {
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
                let msg = werk_test::unit_failure_message("bats", b);
                emit_spine("test.failed", &role, &card, &trace,
                    &[("check", "bats"), ("unit", b), ("message", msg.as_str())]);
            }
        }

        // ── ui: the playwright flows (was an outer leg, #4278) ──
        if ui_here {
            let cmd = std::env::var("NIGHTLY_PLAYWRIGHT_CMD")
                .unwrap_or_else(|_| format!("npx --no-install playwright test --reporter=line,{}", werk_test::PLAYWRIGHT_CASE_REPORTER));
            let mut c = Command::new("bash");
            c.arg("-c").arg(&cmd).current_dir(&root)
                .env("CHORUS_CONTEXT", "")
                // #4454 — the specs' own events ride the trace like every other unit's
                .env("CHORUS_TEST_EVENTS", test_events_path(&root))
                .env("CLEARING_URL", std::env::var("CLEARING_URL").unwrap_or_else(|_| "http://localhost:3470".to_string()));
            let (rc, out) = werk_test::time_unit("proving/flows", || werk_test::run_capped(c, std::time::Duration::from_secs(1800)));
            forward_test_events(&root);
            let (verdict, summary) = werk_test::ui_lane_verdict(rc, &out);
            werk_test::print_unit_time("ui", "proving/flows", "proving/flows");
            println!("nightly-unit|ui|proving/flows|{}|{}", verdict, summary);
            if verdict == "fail" {
                any_failed = true;
                failed_count += 1;
                emit_spine("test.failed", &role, &card, &trace,
                    &[("check", "ui"), ("unit", "proving/flows"), ("message", "ui flows failed in proving/flows")]);
            }
        }
    }
    println!("-- #4022 load gate: held {} time(s) at load > {:.0} (cargo {}, npm {}, bats {}) --",
        cargo_waits + npm_waits + bats_waits, cap, cargo_waits, npm_waits, bats_waits);
    // #4139 — the measured integration line: what ran, after it ran
    if stack_down.is_none() && ns_total > 0 {
        println!("{}", werk_test::integration_measured_report(ns_total, executed_ns.load(Ordering::SeqCst)));
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
    let withheld_n = withheld_total.load(Ordering::SeqCst);
    if withheld_n > 0 {
        emit_spine("testresult.writeback.withheld", &role, &card, &trace,
            &[("count", &withheld_n.to_string()), ("reason", "werk-root"), ("root", &root)]);
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

/// #3974 — bats with per-case capture: same suite-world as run_bats, but the
/// TAP output becomes per-case results for the wire-back.
fn run_bats_cases(werk: &str, suite: &str) -> (bool, Vec<(String, String)>, String) {
    let tmp = std::env::temp_dir().join(format!("werk-test-bats-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    // #3974 — one script-suite variant, two runners: .bats via bats, .sh via
    // bash (the shell tier's suites). Shell output is summary-grain (counted
    // in the lane line via parse_shell_counts); TAP suites get per-case rows.
    // #4106 — the shebang decides, not the extension (see `runner_for`).
    let suite_slug: String = suite.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    // #4292 — a feature runs alone under cucumber (from the package dir, so
    // its profile and step definitions load) and reports each scenario via
    // the JSON report; a unittest file runs under `python3 -m unittest -v`
    // from its own dir. Every other file suite keeps bats/bash.
    let (mut cmd, dir) = if werk_test::is_feature_suite(suite) {
        let rel = suite.strip_prefix("platform/tests/").unwrap_or(suite).to_string();
        let json = tmp.join(format!("cuke-{}.json", suite_slug));
        let cfg = tmp.join(format!("cuke-{}.config.js", suite_slug));
        let _ = std::fs::write(&cfg, werk_test::cuke_single_file_config(
            &format!("{}/platform/tests/cucumber.js", werk), &rel));
        let mut c = Command::new("bash");
        c.arg("-c")
            .arg(r#"npx --no-install cucumber-js --config "$4" --format "json:$2" --format summary; rc=$?; node -e "$3" "$2" "$1"; exit $rc"#)
            .arg("cuke")
            .arg(&rel)
            .arg(json.to_string_lossy().into_owned())
            .arg(werk_test::CUKE_FLATTEN_JS)
            // cucumber joins cwd + --config even when absolute, so hand it a
            // path relative to the package dir it runs from
            .arg(werk_test::relative_from(&format!("{}/platform/tests", werk), &cfg.to_string_lossy()));
        (c, format!("{}/platform/tests", werk))
    } else if werk_test::is_unittest_suite(suite) {
        let (d, file) = suite.rsplit_once('/').unwrap_or((".", suite));
        let mut c = Command::new("python3");
        c.args(["-m", "unittest", "-v", file.trim_end_matches(".py")]);
        (c, format!("{}/{}", werk, d))
    } else {
        let runner = werk_test::suite_runner(&format!("{}/{}", werk, suite));
        let mut c = Command::new(runner);
        // #4454 — each case's time on its TAP line
        if runner == "bats" {
            c.arg("-T");
        }
        c.arg(suite);
        (c, werk.to_string())
    };
    cmd.current_dir(&dir).env("CHORUS_CONTEXT", "");
    apply_suite_world(&mut cmd, werk);
    // #4022 / TD-028 — suite output goes to a FILE and the runner waits on
    // CHILD EXIT with a deadline, never on pipe-EOF. Twice today a finished
    // suite's leaked server (test-share-path-prefix's http.server, the crawler
    // bats wedge) inherited the output pipe and hung the run for as long as
    // anyone let it; a file leaves nothing to hold, and a suite that outlives
    // its budget is killed and scored failed, loudly.
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
                return (false, werk_test::parse_file_suite_cases(suite, &text), text);
            }
            // #4016 — rc=3 is a suite's SELF-REFUSAL ("I must not run here"),
            // not a failure. nightly-suites.sh learned this in #4004; this
            // runner never did, and the nightly uses THIS one — so a correctly
            // refusing suite (test-product-membrane, which bootouts every agent
            // and needs explicit authority) kept reporting "0 pass, 1 fail".
            // Two scorers, one taught. Both must agree or the fix is invisible.
            let refused = code == Some(3);
            // #4265 — rc=2 + the suite's own "UNMEASURED" is the same shape as
            // rc=3: the suite declined to measure. It scored as a failure, so
            // two suites that correctly refuse to write to production were red
            // every night for doing the right thing.
            let unmeasured = werk_test::self_declared_unmeasured(code, &text);
            // #4367 — a feature whose every red waits on a named card reports
            // those reds and does not block (werk_test::feature_reds_all_waiting)
            let waiting_only = werk_test::is_feature_suite(suite)
                && werk_test::feature_reds_all_waiting(&werk_test::parse_file_suite_cases(suite, &text));
            let ok = success || refused || unmeasured || waiting_only;
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
            let mut cases = werk_test::parse_file_suite_cases(suite, &text);
            if refused && cases.is_empty() {
                cases.push((
                    format!("SELF-REFUSED rc=3 — {} declined to run here", suite),
                    "skip".to_string(),
                ));
            }
            if unmeasured && cases.is_empty() {
                cases.push((
                    format!("UNMEASURED rc=2 — {} measured nothing here", suite),
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
/// #4265 — a bats suite may report that it measured NOTHING. Exit 2 is that
/// signal: the suite ran, found its subject unserved, and declined to score.
/// It is not a pass (nothing was proven) and not a fail (nothing was broken),
/// so it gets its own outcome instead of being folded into either.
#[derive(PartialEq)]
enum BatsOutcome { Pass, Fail, Unmeasured }

/// #4392 — the werk's package dirs that have a build script, relative to the
/// werk (depth 3, never inside node_modules, target or .git).
fn werk_packages(werk: &str) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
        if depth > 3 { return; }
        let pj = dir.join("package.json");
        if let Ok(t) = std::fs::read_to_string(&pj) {
            if t.contains("\"build\"") {
                if let Ok(rel) = dir.strip_prefix(root) { if !rel.as_os_str().is_empty() { out.push(rel.to_string_lossy().into_owned()); } }
            }
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n == "node_modules" || n == "target" || n.starts_with('.') { continue; }
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) { walk(root, &e.path(), depth + 1, out); }
        }
    }
    let mut out = Vec::new();
    walk(Path::new(werk), Path::new(werk), 0, &mut out);
    out
}

/// #4392 — build, once each, every crate and package the selected bats suites
/// need, in the werk. Returns the suites whose build failed, with why.
fn build_for_suites(werk: &str, units: &[werk_test::TestUnit]) -> std::collections::BTreeMap<String, String> {
    let packages = werk_packages(werk);
    let mut needs: Vec<(String, Vec<werk_test::BuildTarget>)> = Vec::new();
    for u in units {
        if let werk_test::TestUnit::BatsSuite(s) = u {
            if let Ok(t) = std::fs::read_to_string(format!("{}/{}", werk, s)) {
                needs.push((s.clone(), werk_test::suite_build_targets(&t, &packages)));
            }
        }
    }
    let mut built: std::collections::BTreeMap<werk_test::BuildTarget, Result<(), String>> = Default::default();
    let mut failed = std::collections::BTreeMap::new();
    for (suite, targets) in needs {
        for t in targets {
            let r = built.entry(t.clone()).or_insert_with(|| build_one(werk, &t)).clone();
            if let Err(why) = r { failed.insert(suite.clone(), why); break; }
        }
    }
    failed
}

fn build_one(werk: &str, t: &werk_test::BuildTarget) -> Result<(), String> {
    match t {
        werk_test::BuildTarget::Crate(c) => {
            let dir = format!("{}/platform/services/{}", werk, c);
            if !Path::new(&format!("{}/Cargo.toml", dir)).exists() {
                return Err(format!("no crate at platform/services/{}", c));
            }
            println!("   build: cargo build --release in platform/services/{} (a selected suite runs it)", c);
            let ok = Command::new("cargo").args(["build", "--release", "--quiet"]).current_dir(&dir)
                .status().map(|s| s.success()).unwrap_or(false);
            if ok { Ok(()) } else { Err(format!("cargo build --release failed in platform/services/{}", c)) }
        }
        werk_test::BuildTarget::Package(p) => {
            let dir = format!("{}/{}", werk, p);
            if !Path::new(&format!("{}/node_modules", dir)).exists() {
                return Err(format!("{} has no node_modules in the werk, so its dist cannot be built (npm ci there)", p));
            }
            println!("   build: npm run build in {} (a selected suite runs its dist)", p);
            let ok = Command::new("npm").args(["run", "build", "--silent"]).current_dir(&dir)
                .status().map(|s| s.success()).unwrap_or(false);
            if ok { Ok(()) } else { Err(format!("npm run build failed in {}", p)) }
        }
    }
}

fn run_bats(werk: &str, suite: &str) -> (BatsOutcome, Vec<CaseResult>) {
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
    // #4106 — same rule on the werk lane: a `*.test.sh` whose shebang says bats
    // is a bats suite.
    let runner = werk_test::suite_runner(&format!("{}/{}", werk, suite));
    // #4392 — the card lane gets the same hermetic world as the nightly lane
    // (dead nudge and pulse ports, temp stores): without it a suite that runs
    // chorus-health paged Jeff from inside a card's pipeline.
    let mut cmd = Command::new(runner);
    apply_suite_world(&mut cmd, werk);
    // #4454 — each case's time on its TAP line
    if runner == "bats" {
        cmd.arg("-T");
    }
    // #4454 — output to a FILE (a leaked server cannot hold it open, #4022),
    // echoed to the run log as before, then read for each case's verdict,
    // time and reason.
    let out_path = tmp.join(format!("card-{}.out", suite.replace('/', "_")));
    if let Ok(f) = std::fs::File::create(&out_path) {
        if let Ok(f2) = f.try_clone() {
            cmd.stdout(f).stderr(f2);
        }
    }
    let outcome = bats_outcome(
        cmd
            .arg(suite)
            .current_dir(werk)
            // #3918 — the child is a TEST: clear the runner's prod declaration so
            // the membrane classifies it from its own ambient markers and still
            // refuses it the production spine.
            .env("CHORUS_CONTEXT", "")
            .env("CHORUS_ROOT", werk)
            .env("CHORUS_LOG_FILE", tmp.join("spine.log"))
            // #4136 — a bats suite that drives the real `cards` CLI (the
            // sentinel e2e) wrote done-briefs into a LIVE role dir every night;
            // the SDK honors this seam, so every suite's briefs land in its world.
            .env("CARDS_BRIEFS_ROOT", tmp.join("briefs")),
    );
    let text = std::fs::read_to_string(&out_path).unwrap_or_default();
    print!("{}", text);
    let cases: Vec<CaseResult> = werk_test::parse_file_suite_cases(suite, &text).into_iter()
        .map(|(n, r)| CaseResult { file_path: suite.to_string(), test_name: n, result: r })
        .collect();
    note_case_times(werk_test::bats_case_times(&text));
    note_case_reasons(werk_test::why::bats_case_reasons(&text));
    (outcome, cases)
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
    // #4419 — replay a landed commit's diff (with --explain: what would this
    // land have selected?). The proof that the selection would have caught a
    // red the nightly found later.
    if let Ok(c) = std::env::var("WERK_TEST_REPLAY") {
        let out = Command::new("git")
            .args(["-C", werk, "diff", "--name-only", &format!("{c}^..{c}")])
            .output()
            .map_err(|e| format!("git diff failed: {}", e))?;
        return Ok(String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect());
    }
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
    // #4130 AC1 — tell the child it is under the nightly, and what its cap is.
    //
    // #4126 taught test-restore-drill.sh to refuse there: a ~47min restore
    // (#4043 measured 2849s) cannot finish inside a 1200s per-unit cap, so it
    // is killed, scored fail, records no PASS, and looks overdue again the next
    // night. That refusal reads $NIGHTLY_UNIT_TIMEOUT / $WERK_TEST_NIGHTLY, and
    // the runner exported NEITHER — unit_timeout() reads the variable in the
    // RUNNER's process; no child ever saw it. So the refusal could not fire and
    // the 2026-09-08 17:41 run still spent 1200.0s there. A gate that cannot
    // reach the condition it guards is the #3734 shape, and I shipped one.
    //
    // Set here rather than in suite_world_env because that list is the HERMETIC
    // world — tmp paths and dead ports that isolate a suite from the live box
    // (#3892/#3912/#3995), and it is skipped for any key already in the
    // environment so a caller can override it. These two are the opposite: not
    // isolation, but the runner telling the suite the truth about where it is,
    // and not overridable from outside the run.
    cmd.env("WERK_TEST_NIGHTLY", "1");
    cmd.env("NIGHTLY_UNIT_TIMEOUT", werk_test::unit_timeout().as_secs().to_string());
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
    cmd.env("CHORUS_TEST_EVENTS", test_events_path(werk));
}

/// #4454 — where a test child writes its own events (fixture up, ready,
/// failed; a case starting). werk-test forwards them after each unit.
fn test_events_path(werk: &str) -> String {
    let slot = Path::new(werk).file_name().and_then(|s| s.to_str()).unwrap_or("werk");
    std::env::temp_dir().join(format!("werk-test-events-{slot}-{}.tsv", std::process::id())).to_string_lossy().into_owned()
}

/// #4454 — move the children's events to the spine: the file is renamed
/// first, so a child still writing starts a new one and nothing is read twice.
fn forward_test_events(werk: &str) {
    let path = test_events_path(werk);
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let taken = format!("{}.{}", path, N.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
    if std::fs::rename(&path, &taken).is_err() {
        return;
    }
    let text = std::fs::read_to_string(&taken).unwrap_or_default();
    let _ = std::fs::remove_file(&taken);
    emit_spine_batch(&werk_test::forwardable_test_events(&text));
}

/// #3929 — probe `cargo nextest --version` ONCE per process against the pin in
/// `<werk>/.config/nextest.toml`. Absence, staleness, or a missing pin all
/// refuse the whole cargo lane loudly; there is no fallback to `cargo test`.
/// #3919 — probe the live stack the needs-stack tier depends on. Overridable
/// via WERK_STACK_PROBES="name=url,name=url" so tests bring their own world;
/// defaults to the two services the registered integration tests actually hit.
fn probe_stack() -> Vec<(String, werk_test::ProbeState, String)> {
    let spec = std::env::var("WERK_STACK_PROBES").unwrap_or_else(|_|
        "chorus-api=http://localhost:3340/api/chorus/context/health,athena-make=http://localhost:3360/".to_string());
    // #4140 — two tries with a real cap, and the probe records WHAT it saw
    // (code + ms). One 3s try with no retry called a busy athena-make DOWN on
    // 2026-09-11 08:20 and 1,484 needs-stack tests did not run. Same defect
    // class deep-health fixed in #3948.
    let cap = std::env::var("WERK_STACK_PROBE_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(10u32);
    spec.split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, url)| {
            let mut state = werk_test::ProbeState::Down;
            let mut saw: Vec<String> = Vec::new();
            for attempt in 1..=2 {
                let t0 = std::time::Instant::now();
                let out = Command::new("curl")
                    .args(["-sf", "--max-time", &cap.to_string(), "-o", "/dev/null", "-w", "%{http_code}", url])
                    .output();
                let (exit, code) = match out {
                    Ok(o) => (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).trim().to_string()),
                    Err(_) => (-1, "000".to_string()),
                };
                state = werk_test::classify_probe(exit, &code);
                saw.push(format!("{} in {}ms", if code.is_empty() { "000".to_string() } else { code }, t0.elapsed().as_millis()));
                if state == werk_test::ProbeState::Up { break; }
                if attempt == 1 { std::thread::sleep(std::time::Duration::from_secs(2)); }
            }
            (name.trim().to_string(), state, saw.join(", "))
        })
        .collect()
}

/// #4140 — the three-state verdict from the probes: Up / Down (typed skip) /
/// Unmeasurable (busy: not shown down, not run, not green).
fn stack_state_now() -> werk_test::StackState {
    let probes = probe_stack();
    werk_test::stack_state(&probes.iter().map(|(n, st, saw)| (n.as_str(), *st, saw.clone())).collect::<Vec<_>>())
}

fn stack_down_of(stack: &werk_test::StackState) -> Option<String> {
    match stack {
        werk_test::StackState::Up => None,
        werk_test::StackState::Down(d) => Some(d.clone()),
        werk_test::StackState::Unmeasurable(u) => Some(u.clone()),
    }
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
    let (ok, cases, _) = run_cargo_sel(werk, name, quarantined, ns_bins, &[]);
    (ok, cases)
}

/// #4160 — the nightly's staged cargo run: `only_bins` non-empty runs just
/// those test binaries (a typed binary in its own stage); otherwise the crate
/// runs minus `exclude_bins`. A unit run whose every binary moved to another
/// stage may have nothing left, which is a fact, not a failure.
/// #4155 — the output text comes back too, so each failed case's panic
/// block can be read as its reason.
fn run_cargo_sel(werk: &str, name: &str, quarantined: &[&str], exclude_bins: &[&str], only_bins: &[&str]) -> (bool, Vec<(String, String)>, String) {
    let dir = format!("{}/platform/services/{}", werk, name);
    if !Path::new(&format!("{}/Cargo.toml", dir)).is_file() {
        return (true, Vec::new(), String::new());
    }
    if let Err(reason) = nextest_gate(werk) {
        eprintln!("REFUSED cargo lane for {}: {}", name, reason);
        return (false, Vec::new(), format!("REFUSED cargo lane: {}", reason));
    }
    // #4022 — each crate gets its share of the CPU budget (see lane_widths);
    // NIGHTLY_NEXTEST_THREADS is set by the nightly lane, absent for card runs.
    let threads = std::env::var("NIGHTLY_NEXTEST_THREADS").ok().and_then(|v| v.parse().ok());
    let mut args: Vec<String> = if only_bins.is_empty() {
        werk_test::nextest_run_args_threads(quarantined, exclude_bins, threads)
    } else {
        werk_test::nextest_run_args_bins(quarantined, only_bins, threads)
    };
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
            // #4063 — full nextest paths: the module chain resolves same-named fns
            let cases = werk_test::parse_nextest_case_paths(&text);
            // #4454 — times and failure reasons, by the bare name a case is stored under
            note_case_times(werk_test::nextest_case_times(&text).into_iter()
                .map(|(p, ms)| (werk_test::nextest_bare_name(&p).to_string(), ms)).collect());
            note_case_reasons(werk_test::why::nextest_case_reasons(&text).into_iter()
                .map(|(p, r)| (werk_test::nextest_bare_name(&p).to_string(), r)));
            (ok, cases, text)
        }
        Err(e) => (false, Vec::new(), format!("cargo could not start: {}", e)),
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
/// #4111 — does this package's jest config declare a `hermetic` project?
/// Read, not assumed: `--selectProjects hermetic` against a config with no
/// projects is a warning and an empty run, which would be a vacuous green.
fn jest_has_hermetic_project(pkg_dir: &str) -> bool {
    for name in ["jest.config.js", "jest.config.cjs", "jest.config.ts"] {
        let p = format!("{}/{}", pkg_dir, name);
        if let Ok(src) = std::fs::read_to_string(&p) {
            if src.contains("displayName: 'hermetic'") || src.contains("displayName: \"hermetic\"") {
                return true;
            }
        }
    }
    false
}

fn run_jest(werk: &str, pkg: &str) -> (bool, Vec<CaseResult>) {
    run_jest_with(werk, pkg, None)
}

/// #4022 — jest with its share of the CPU budget (`--maxWorkers N`); None keeps
/// jest's default (a worker per core), which is what pegged the box.
fn run_jest_with(werk: &str, pkg: &str, max_workers: Option<usize>) -> (bool, Vec<CaseResult>) {
    let (ok, cases, reasons) = run_jest_project(werk, pkg, max_workers, None);
    note_case_reasons(reasons.into_iter().map(|(_, n, r)| (n, r)));
    (ok, cases)
}

/// #4160 — jest with a named project: the nightly's unit stage runs
/// `hermetic`, its integration stage runs `integration`, as two rows.
/// #4155 — returns each failed case's reason (file, case, text) with the cases.
fn run_jest_project(werk: &str, pkg: &str, max_workers: Option<usize>, project: Option<&str>) -> (bool, Vec<CaseResult>, Vec<(String, String, String)>) {
    let pkg_dir = format!("{}/{}", werk, pkg);
    if !ensure_ts_deps(werk, pkg) {
        eprintln!("!! jest:{} CHANGED but deps unavailable — FAIL LOUD", pkg);
        return (false, Vec::new(), vec![(String::new(), String::new(), format!("jest:{} deps unavailable", pkg))]);
    }
    let jest = format!("{}/node_modules/.bin/jest", pkg_dir);
    if !Path::new(&jest).exists() {
        // #3974 — a package without jest runs its OWN runner (mcp-server:
        // node:test via npm test). The old `return true` here was a silent
        // vacuous green for every non-jest package.
        let (ok, cases) = run_npm_test(werk, pkg);
        return (ok, cases, Vec::new());
    }
    let mut cmd = Command::new(&jest);
    // #3918 — test child: cleared (see child_context).
    cmd.env("CHORUS_CONTEXT", "");
    cmd.args(["--ci", "--forceExit", "--passWithNoTests", "--json"])
        .current_dir(&pkg_dir);
    // #4111 — run the HERMETIC project only. platform/api's jest config declares
    // two projects, `hermetic` and `integration`; bare jest runs both. Inside act
    // there is no live stack, so every *.integration.test.ts fails on a service
    // it cannot reach: 150 failures across 15 suites on run 29, none of them
    // about the code. The registry-unreachable fallback made it worse by running
    // the FULL package suite, turning a registry outage into a wall of red that
    // reads exactly like a broken build.
    //
    // #4111's fix selected hermetic UNCONDITIONALLY, and the "own gated lane"
    // it leaned on was this same call under RUN_INTEGRATION=true — so the
    // nightly dropped the tier too (0 platform/api integration rows stored
    // 09-08 → 09-10 while the plan line still said "1484 ran"). The config
    // already builds the integration project only when the flag is true, so
    // the flag is the whole decision.
    // #4139 — the flag decides the projects: integration on → both, off →
    // hermetic only. #4111 selected hermetic always and dropped platform/api's
    // 266 integration tests from the nightly (09-08 → 09-10, lane read green).
    let run_integration = std::env::var("RUN_INTEGRATION").map(|v| v == "true").unwrap_or(false);
    for a in werk_test::jest_project_args_for(jest_has_hermetic_project(&pkg_dir), run_integration, project) {
        cmd.arg(a);
    }
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
            {
                // #4145 — keep the WHY of every failed case in the lane output
                for l in werk_test::nightly_run::jest_failure_why(&stdout, pkg, &|f| rel_path(f, werk)) {
                    println!("{}", l);
                }
                let mut reasons = jest_reasons_via_jq(stdout.as_bytes(), werk);
                if !ok && stdout.trim().is_empty() {
                    // jest died before writing its report: the stderr tail is the reason
                    reasons.push((String::new(), String::new(), werk_test::why::tail_reason(&stderr, 4)));
                }
                (ok, jest_cases_via_jq(stdout.as_bytes(), werk), reasons)
            }
        }
        None => (false, Vec::new(), vec![(String::new(), String::new(), format!("jest:{} killed at its unit cap", pkg))]),
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
    run_npm_test_files(werk, pkg, None)
}

/// #4440 reopened — `only`: the package-relative test files a card selected.
/// None runs every test file the package's script would.
fn run_npm_test_files(werk: &str, pkg: &str, only: Option<&[String]>) -> (bool, Vec<CaseResult>) {
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
    // #4173 — the runner question comes FIRST. platform/tests declares
    // `test: cucumber-js`: its suites are .feature files owned by the bdd lane,
    // so this lane looking for *.test.ts finds none and used to fail the card
    // with "has a test script but no test files found" — a true sentence about
    // the wrong lane. A package this lane cannot attribute is not this lane's
    // to grade; the refusal below still fires for a node:test package, and an
    // EMPTY node:test package still fails loud rather than passing vacuously.
    let runner = npm_test_runner(&pkg_dir);
    if runner.is_none() {
        eprintln!("   npm:{} runs its own non-node:test runner — graded by its own lane, not here", pkg);
        return (true, Vec::new());
    }
    let files: Vec<String> = match only {
        Some(sel) => sel.to_vec(),
        None => npm_test_files(&pkg_dir),
    };
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
    // #4435 — the script's own steps before the runner (`npm run build`): the
    // files load what the build writes, so they run once, and a failed step
    // fails the package loudly instead of every case failing for a missing dist.
    for step in &runner.before {
        let mut cmd = Command::new(&step[0]);
        cmd.args(&step[1..]).current_dir(&pkg_dir);
        cmd.env("CHORUS_CONTEXT", "");
        apply_suite_world(&mut cmd, werk);
        match run_capped_unit(&mut cmd, &format!("npm:{}:{}", pkg, step.join(" "))) {
            Some((true, _, _)) => {}
            Some((false, stdout, stderr)) => {
                eprintln!("!! npm:{} `{}` failed before its tests — FAIL LOUD\n{}{}", pkg, step.join(" "), stdout, stderr);
                return (false, Vec::new());
            }
            None => return (false, Vec::new()),
        }
    }
    for rel in &files {
        let mut cmd = Command::new(&runner.prog);
        cmd.args(&runner.args).arg(rel).current_dir(&pkg_dir);
        cmd.envs(runner.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
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

/// The node:test plan behind a package's `test` script (#4435: steps that run
/// first, then the runner with its glob dropped — so one file can be appended).
fn npm_test_runner(pkg_dir: &str) -> Option<werk_test::NodeTestPlan> {
    let json = std::fs::read_to_string(format!("{}/package.json", pkg_dir)).ok()?;
    node_test_plan(&package_test_script(&json)?)
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
            if is_node_test_file(&name) {
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
    let rel: Vec<String> = files
        .iter()
        .map(|f| f.strip_prefix(&format!("{}/", pkg)).unwrap_or(f).to_string())
        .collect();
    if !Path::new(&jest).exists() {
        // #4440 reopened — a package without jest (mcp-server: node:test) ran
        // NOTHING here and returned pass: since #3912 (2026-08-17) a card never
        // ran mcp-server's tests (#4420 run 12: 30 files selected, 0.0s). Its
        // own runner runs the selected files; no runner at all is a red.
        if npm_test_runner(&pkg_dir).is_none() {
            eprintln!("!! {}: {} test file(s) selected and no runner for them (no jest, no node:test script) — FAIL LOUD", pkg, rel.len());
            return (false, Vec::new());
        }
        return run_npm_test_files(werk, pkg, Some(&rel));
    }
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
            {
                // #4145 — keep the WHY of every failed case in the lane output
                for l in werk_test::nightly_run::jest_failure_why(&stdout, pkg, &|f| rel_path(f, werk)) {
                    println!("{}", l);
                }
                {
                    note_case_reasons(werk_test::why::jest_reasons(stdout.as_bytes()).into_iter().map(|(_, n, r)| (n, r)));
                    (ok, jest_cases_via_jq(stdout.as_bytes(), werk))
                }
            }
        }
        None => (false, Vec::new()),
    }
}


/// jq-extract per-case rows from jest's --json report (curl|jq zero-dep
/// pattern, ADR-032 §6). Any jq failure yields an EMPTY set — emit is
/// best-effort, the gate verdict never depends on it.
fn jest_cases_via_jq(json: &[u8], werk: &str) -> Vec<CaseResult> {
    let jq_filter =
        r#".testResults[] | .name as $f | .assertionResults[] | [$f, .fullName, .status, (.duration // "")] | @tsv"#;
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
    let tsv = String::from_utf8_lossy(&out);
    let cases: Vec<CaseResult> = parse_case_tsv(&tsv)
        .into_iter()
        .map(|c| CaseResult { file_path: rel_path(&c.file_path, werk), ..c })
        .collect();
    // #4454 — the durations ride beside the cases (CaseResult has 30 builders)
    note_case_times(werk_test::case_tsv_times(&tsv));
    cases
}

/// #4454 — what each runner printed about its cases, by case name: elapsed
/// time and (failed) reason. Filled where the runner's raw output is in hand,
/// taken by whoever logs those cases. CaseResult has 30 builders; the facts
/// ride beside it rather than through every one.
static CASE_TIMES: std::sync::Mutex<Option<std::collections::HashMap<String, u64>>> = std::sync::Mutex::new(None);
static CASE_REASONS: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);

fn note_case_times(times: std::collections::HashMap<String, u64>) {
    if let Ok(mut m) = CASE_TIMES.lock() {
        m.get_or_insert_with(Default::default).extend(times);
    }
}

fn note_case_reasons(reasons: impl IntoIterator<Item = (String, String)>) {
    if let Ok(mut m) = CASE_REASONS.lock() {
        m.get_or_insert_with(Default::default).extend(reasons);
    }
}

fn take_case_times(cases: &[CaseResult]) -> std::collections::HashMap<String, u64> {
    let mut out = std::collections::HashMap::new();
    if let Ok(mut m) = CASE_TIMES.lock() {
        if let Some(all) = m.as_mut() {
            for c in cases {
                if let Some(ms) = all.remove(&c.test_name) {
                    out.insert(c.test_name.clone(), ms);
                }
            }
        }
    }
    out
}

/// #4454 — a card run's cases as events, the same shape the nightly logs:
/// time from the runner, reason (and its kind) for a failed case.
fn emit_case_events(unit: &str, cases: &[CaseResult], role: &str, card: &str, trace: &str) {
    let times = take_case_times(cases);
    let reasons: std::collections::HashMap<String, String> = CASE_REASONS.lock().ok()
        .and_then(|mut m| m.as_mut().map(|all| cases.iter()
            .filter_map(|c| all.remove(&c.test_name).map(|r| (c.test_name.clone(), r))).collect()))
        .unwrap_or_default();
    let lines: Vec<String> = cases.iter().map(|c| {
        let (kind, reason) = if c.result == "fail" {
            let why = werk_test::why::why_line(&c.file_path, &c.test_name, reasons.get(&c.test_name).map(String::as_str).unwrap_or(""));
            let (_, _, k, r) = werk_test::why::parse_why_line(&why).unwrap_or_default();
            (k, r)
        } else {
            Default::default()
        };
        werk_test::batch_line(&werk_test::case_event_args(c, times.get(&c.test_name).copied(), unit, &kind, &reason, role, card, trace))
    }).collect();
    emit_spine_batch(&lines);
}

/// #4155 — each failed jest case's reason, paths made werk-relative.
fn jest_reasons_via_jq(json: &[u8], werk: &str) -> Vec<(String, String, String)> {
    werk_test::why::jest_reasons(json)
        .into_iter()
        .map(|(f, n, r)| (rel_path(&f, werk), n, r))
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
    // #4396 — the live link probe measures production, not this card's tree:
    // a card lane skips it (the ratchet prints SKIPPED); the nightly runs it.
    status_ok(Command::new("bash").arg(&script).current_dir(werk).env("CHORUS_REPO", werk).env("SKIP_HREF_PROBE", "1"))
}

/// Provide a TS package's node_modules by symlinking canonical's ONLY when the
/// lockfiles match (no dep drift — #3397). Returns true if deps are present after.
fn ensure_ts_deps(werk: &str, pkg: &str) -> bool {
    if !discover_ts_packages(Path::new(werk)).iter().any(|p| p == pkg) {
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

/// #4265 — read a bats suite's exit code as a three-way outcome. 0 pass,
/// 2 unmeasured, anything else fail. A suite that cannot be launched at all
/// is a FAIL, never unmeasured: a missing runner must not read as "nothing to
/// measure".
fn bats_outcome(cmd: &mut Command) -> BatsOutcome {
    match cmd.status() {
        Ok(s) if s.success() => BatsOutcome::Pass,
        Ok(s) if s.code() == Some(2) => BatsOutcome::Unmeasured,
        _ => BatsOutcome::Fail,
    }
}

/// #4454 — many spine events from one `chorus-log --batch` process: a test
/// unit's cases, one per line (see werk_test::batch_line). A process per
/// event cost ~28ms, ten minutes over a nightly's ~20,000 cases.
fn emit_spine_batch(lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let home = match std::env::var("CHORUS_HOME") {
        Ok(h) => h,
        Err(_) => return,
    };
    let log = format!("{}/platform/scripts/chorus-log", home);
    if !Path::new(&log).is_file() {
        return;
    }
    let child = Command::new(&log).arg("--batch")
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    let batched = child.map(|mut child| {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write as _;
            let _ = stdin.write_all(lines.join("\n").as_bytes());
            let _ = stdin.write_all(b"\n");
        }
        child.wait().map(|s| s.success()).unwrap_or(false)
    }).unwrap_or(false);
    // a chorus-log that predates --batch (the installed shim, until this
    // lands) refuses it: one process per event then, slow but nothing lost
    if !batched {
        for l in lines {
            let _ = Command::new(&log).args(l.split('\t')).stdout(std::process::Stdio::null()).status();
        }
    }
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
    // #4162 — one field, testType, says what kind of proving a row is. The
    // lanes still select on two columns (layer: unit/integration/…, concern:
    // ui/perf/security), so the jq splits testType into them. A row the
    // crawler has not rewritten yet still carries the retired pair; it is
    // read as-is until the crawler's next pass migrates it.
    let jq_filter = r#".data[] | .filePath as $f | (.testType // "") as $tt | (if ($tt | IN("ui","perf","security")) then "" elif $tt != "" then $tt else (.pyramidLayer // "") end) as $l | .testName as $n | .name as $e | .hermeticity as $h | (if ($tt | IN("ui","perf","security")) then $tt elif $tt != "" then "" else (.testConcern // "") end) as $tc | (.covers | if type=="array" then .[] else . end) as $c | [$f,$c,($l // ""),($n // ""),($e // ""),($h // ""),($tc // "")] | @tsv"#;
    // #4419 — read the registry a page at a time. One 10,000-row read took
    // 12.8 s (measured 10-02: 1,000 rows 1.5 s, linear) against this 10 s cap,
    // so a run sometimes got 0 rows and "0 tests" read as an answer. Pages of
    // 1,000 each fit the cap; the links.next cursor walks them.
    let body = match fetch_all_pages(&endpoint) {
        Some(b) => b,
        None => return (Vec::new(), Vec::new(), Vec::new(), plan_source_label(false, 0)),
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
    joined: &[(CaseResult, String, String)],
    run_epoch_ms: u128,
    idx_base: usize,
    reason_of: &dyn Fn(&CaseResult) -> String,
) -> usize {
    let writeback_started = std::time::Instant::now();
    let endpoint = std::env::var("OWL_API_TESTRESULTS_BATCH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            // #4158 AC4 — this caller carries NO path shape. It ASKS the server
            // which collection it serves for TestResult (the pattern
            // chorus-crawl already uses) and posts there. A pre-#4158 server
            // answers /v1/testresults, a post-#4158 one /v1/tests/results, and
            // both are right. Hardcoding either literal is what lost 650 of 650
            // results in run 77 (2026-09-13): the class-rooted form breaks once
            // the rename lands, and the domain-rooted form 403s against a
            // server that predates it. The configured default is the fallback
            // when discovery is unreadable — degrade, never invent a route.
            let collection = std::env::var("OWL_API_TESTRESULTS")
                .unwrap_or_else(|_| "http://localhost:3360/testresults".to_string());
            let discovered = werk_test::origin_of(&collection).and_then(|origin| {
                let out = Command::new("curl")
                    .args(["-sf", "--max-time", "5", &origin])
                    .output()
                    .ok()?;
                if !out.status.success() {
                    return None;
                }
                let body = String::from_utf8_lossy(&out.stdout);
                let path = werk_test::advertised_collection(&body, "TestResult")?;
                Some(format!("{}{}", origin, path))
            });
            werk_test::testresult_batch_endpoint(discovered.as_deref().unwrap_or(&collection))
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
        .map(|(i, (c, of_test, _registered))| {
            // #4033 — names are testresult-<card>-<ts>-<idx>; with the per-unit
            // store (#4030) every unit restarted i at 0 under the run's shared
            // ts, so unit two's names were unit one's and the store answered
            // 409 for the whole chunk. idx_base makes idx run-unique.
            // #4155 — a failed case's result row carries why it failed
            werk_test::with_failure_reason(
                &test_result_payload(
                    &c.file_path, &c.test_name, &c.result, of_test, card, role, trace, ts, idx_base + i),
                &reason_of(c))
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

// #4154 — `werk-test --reconcile` (registered minus executed) retired with the
// nightly census; the crawler keeps the registry current, the runner trusts it.

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
/// #4169 — the FULL reason, no longer discarded. Mirrors diff_scoped_units'
/// unit construction and asks the shared core what it actually said, so the
/// caller can refuse a data defect ("unmapped:<file>") while letting a
/// deliberate full run ("forced" / "empty-diff") through.
fn diff_scope_reason(werk: &str, changed: &[String]) -> String {
    diff_scoped_units_inner(werk, changed).1
}

fn diff_scoped_units(werk: &str, changed: &[String]) -> Option<Vec<TestUnit>> {
    diff_scoped_units_inner(werk, changed).0
}

/// #4466 — every test suite in the werk with its text, for naming lookups.
/// Skips build output and dependencies. An unreadable suite is said out loud:
/// it cannot be searched, so a file it names might run nothing.
fn suite_texts(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if !matches!(name.as_str(), "node_modules" | ".git" | "target" | "dist") && !name.starts_with(".werk") {
                    stack.push(p);
                }
                continue;
            }
            let Ok(rel) = p.strip_prefix(root) else { continue };
            let rel = rel.to_string_lossy().to_string();
            if is_test_suite_path(&rel) {
                match std::fs::read_to_string(&p) {
                    Ok(text) => out.push((rel, text)),
                    Err(e) => eprintln!("scope(diff): cannot read suite {rel} to find what it names: {e}"),
                }
            }
        }
    }
    out.sort();
    out
}

fn diff_scoped_units_inner(werk: &str, changed: &[String]) -> (Option<Vec<TestUnit>>, String) {
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
    for pkg in discover_ts_packages(root) {
        let pkg = pkg.as_str();
        units.push(ScopeUnit { name: pkg.to_string(), dir: pkg.to_string() });
        if let Ok(content) = std::fs::read_to_string(root.join(pkg).join("package.json")) {
            if let Some(i) = content.find("\"name\"") {
                let rest = &content[i + 6..];
                if let Some(c) = rest.find(':') {
                    let rest = rest[c + 1..].trim_start();
                    if let Some(rest) = rest.strip_prefix('"') {
                        if let Some(e) = rest.find('"') {
                            ts_name_to_dir.push((rest[..e].to_string(), pkg.to_string()));
                        }
                    }
                }
            }
        }
    }
    // #4353 — data/athena/ is read by chorus-api (the athena tree handler and the
    // index), so a change there runs chorus-api's suite instead of refusing as
    // an unmapped path.
    units.push(ScopeUnit { name: "platform/api".to_string(), dir: "data/athena".to_string() });
    // #4173 — a changed suite is its own unit. Without this the scoper names it
    // and the filter below drops it, which is the same "runs nothing" the
    // irrelevant list used to produce.
    let edges: Vec<(String, String)> = scope_declared_edges(root)
        .into_iter()
        .map(|(p, d)| {
            let p2 = ts_name_to_dir.iter().find(|(n, _)| *n == p).map(|(_, d2)| d2.clone()).unwrap_or(p);
            let d2 = ts_name_to_dir.iter().find(|(n, _)| *n == d).map(|(_, dd)| dd.clone()).unwrap_or(d);
            (p2, d2)
        })
        .collect();
    // #4466 — every changed file brings in the suites that name it; a file no
    // unit claims but a suite names is covered by that suite, not refused.
    let changed = &werk_test::expand_by_naming_suites(changed, &suite_texts(root), &units, &edges);
    for f in changed {
        // A DELETED suite is in the diff and has nothing to run. #4173 deletes
        // four crawler suites with the walkers they proved; scoping them to
        // themselves would hand the runner four paths that are not on disk.
        if is_test_suite_path(f) && root.join(f).is_file() {
            units.push(ScopeUnit { name: f.clone(), dir: f.clone() });
        }
    }
    let (scoped, reason) = match scoped_test_reason(changed, &units, &edges) {
        Ok(v) => (v, "scoped".to_string()),
        Err(r) => return (None, r),
    };
    (
        Some(
            scoped
                .into_iter()
                .map(|u| {
                    if is_test_suite_path(&u.dir) {
                        TestUnit::BatsSuite(u.dir)
                    } else if u.dir.starts_with("platform/services/") {
                        TestUnit::RustCrate(u.name)
                    } else {
                        TestUnit::TsPackage(u.name)
                    }
                })
                .collect(),
        ),
        reason,
    )
}

/// #3920 — the ui lane's stack verdict. The lane is needs-stack by nature
/// (browser against live pages); reuse the SAME probe machinery so up/down has
/// one definition. Probes only when the lane actually fires.
fn stack_down_ui(_selected_ns: &[String], _ns_all: &std::collections::BTreeSet<String>) -> Option<String> {
    stack_down_of(&stack_state_now())
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

/// #4154 — `None` = UNMEASURED (playwright selected no spec: nothing ran,
/// nothing crashed). `Some(false)` is a real failure, `Some(true)` a pass.
fn run_ui_flows(werk: &str, files: &std::collections::BTreeSet<String>, quarantined: &[werk_test::Quarantined]) -> (Option<bool>, String) {
    ensure_ui_service_built(werk, "directing/clearing", "dist/server.js");
    let mut cmd = Command::new("npx");
    cmd.arg("playwright").arg("test");
    // #4454 — list stays the output werk-test reads; the second reporter puts each
    // case's start and end on the trace. Passed here, not in the root config, so
    // the nightly's `--reporter=line` gets it too.
    cmd.arg(format!("--reporter=list,{}", werk_test::PLAYWRIGHT_CASE_REPORTER));
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
    // #4454 — the specs' own events (fixtures, each case starting and ending)
    cmd.env("CHORUS_TEST_EVENTS", test_events_path(werk));
    let out = cmd.output();
    forward_test_events(werk);
    match out {
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
                    (Some(o.status.success() && f == 0), format!(" ({} passed, {} failed{}){}", p, f, skip_note, excluded))
                }
                None => {
                    let tail: Vec<&str> = text.lines().rev().take(15).collect();
                    eprintln!("{}", tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
                    // #4119 — "the filter matched no spec" and "the runner fell
                    // over" are different states and were printed as one word.
                    // Both are still red; the verdict now says which.
                    match werk_test::classify_playwright_no_summary(&text) {
                        werk_test::PlaywrightNoSummary::SelectedNoSpec => {
                            let asked: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
                            (werk_test::no_summary_verdict(werk_test::PlaywrightNoSummary::SelectedNoSpec), format!(
                                " (playwright selected NO spec — nothing ran, nothing crashed; \
filters asked for: {}){}",
                                if asked.is_empty() { "<none>".to_string() } else { asked.join(", ") },
                                excluded))
                        }
                        werk_test::PlaywrightNoSummary::Crashed =>
                            (werk_test::no_summary_verdict(werk_test::PlaywrightNoSummary::Crashed),
                             format!(" (no playwright summary — crashed before running, fail loud){}", excluded)),
                    }
                }
            }
        }
        Err(e) => (Some(false), format!(" (spawn failed: {})", e)),
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

    /// #4392 — the card lane's run_bats gives a suite the caged world and the
    /// werk as its root. The suite fails unless both hold, so a runner that
    /// hands it the live nudge path or canonical fails this test.
    #[test]
    fn the_card_lane_runs_a_suite_caged_and_rooted_in_the_werk() {
        let (dir, s) = world("cage.sh", "case \"$CHORUS_MCP_NUDGE_URL\" in *127.0.0.1:9*) ;; *) echo \"live nudge path: $CHORUS_MCP_NUDGE_URL\"; exit 1 ;; esac\n\
[ \"$CHORUS_ROOT\" = \"$PWD\" ] || { echo \"root $CHORUS_ROOT is not the werk $PWD\"; exit 1; }\n\
echo '=== Results: 1 passed, 0 failed ==='\n");
        std::env::remove_var("CHORUS_MCP_NUDGE_URL");
        let werk = std::fs::canonicalize(&dir).unwrap();
        assert!(matches!(run_bats(werk.to_str().unwrap(), &s).0, BatsOutcome::Pass));
    }

    /// NEGATIVE PROOF — the same suite run with the live nudge path fails, so
    /// the check above can go red.
    #[test]
    fn negative_proof_a_live_nudge_path_fails_the_cage_suite() {
        let (dir, s) = world("cage2.sh", "case \"$CHORUS_MCP_NUDGE_URL\" in *127.0.0.1:9*) ;; *) exit 1 ;; esac\n");
        let out = Command::new("bash").arg(&s).current_dir(&dir)
            .env("CHORUS_MCP_NUDGE_URL", "http://127.0.0.1:3341/nudge").status().unwrap();
        assert!(!out.success());
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

#[cfg(test)]
mod bats_unmeasured_4265 {
    use super::{bats_outcome, BatsOutcome};
    use std::process::Command;

    fn exiting(code: i32) -> BatsOutcome {
        bats_outcome(Command::new("sh").arg("-c").arg(format!("exit {}", code)))
    }

    #[test]
    fn exit_two_is_unmeasured_and_nothing_else_is() {
        assert!(exiting(0) == BatsOutcome::Pass);
        assert!(exiting(2) == BatsOutcome::Unmeasured);

        // NEGATIVE PROOF (#3734): the codes this must NOT swallow. A suite that
        // fails, and a suite whose runner cannot even be launched, both stay
        // FAIL — otherwise "unmeasured" becomes a way to make a red disappear,
        // which is the exact thing the hold exists to avoid.
        assert!(exiting(1) == BatsOutcome::Fail);
        assert!(exiting(70) == BatsOutcome::Fail);
        assert!(bats_outcome(&mut Command::new("/nonexistent/bats")) == BatsOutcome::Fail);
    }
}


#[cfg(test)]
mod data_athena_scope_4353 {
    use super::*;

    fn empty_world(tag: &str) -> String {
        let root = std::env::temp_dir().join(format!("werk-test-4353-{}-{}", std::process::id(), tag));
        let _ = std::fs::create_dir_all(&root);
        root.to_string_lossy().into_owned()
    }

    #[test]
    fn a_tree_json_change_runs_chorus_api() {
        let (units, reason) = diff_scoped_units_inner(&empty_world("tree"), &["data/athena/tree.json".to_string()]);
        let units = units.unwrap_or_else(|| panic!("refused: {reason}"));
        assert!(units.iter().any(|u| matches!(u, TestUnit::TsPackage(p) if p == "platform/api")), "{units:?}");
        assert!(!units.iter().any(|u| matches!(u, TestUnit::TsPackage(p) if p == "data/athena")), "the unit is chorus-api, not the data dir: {units:?}");
    }

    #[test]
    fn negative_proof_a_path_nobody_reads_still_refuses() {
        let (units, reason) = diff_scoped_units_inner(&empty_world("other"), &["data/elsewhere/x.json".to_string()]);
        assert!(units.is_none(), "an unmapped data path must still refuse: {units:?}");
        assert!(reason.contains("unmapped:data/elsewhere/x.json"), "{reason}");
    }
}

/// #4419 reopened — the registered tests that exercise each changed file run
/// (Jeff 2026-10-06: "we are tagging the suites or cases"; the card runs the
/// exercising tests, the nightly runs everything). None when the registry is
/// empty: the run says UNMEASURED, never guesses.
fn domain_select(werk: &str, changed: &[String], rows: &[TestRow]) -> Option<werk_test::DomainSelection> {
    if changed.is_empty() {
        return Some(werk_test::DomainSelection::default());
    }
    // no registry rows = nothing measured, never "0 tests in these domains"
    if rows.is_empty() {
        return None;
    }
    let cache: std::cell::RefCell<std::collections::HashMap<String, Option<String>>> = Default::default();
    let text_of = |p: &str| -> Option<String> {
        cache.borrow_mut().entry(p.to_string())
            .or_insert_with(|| std::fs::read_to_string(format!("{werk}/{p}")).ok())
            .clone()
    };
    // a test the card adds exercises what it imports before the crawler has
    // registered it
    let mut candidates: Vec<TestRow> = rows.to_vec();
    for f in changed.iter().filter(|f| werk_test::is_test_file(f) && !rows.iter().any(|r| &r.file_path == *f)) {
        candidates.push(TestRow { file_path: f.clone(), covers: String::new(), pyramid_layer: String::new(), hermeticity: String::new(), test_concern: String::new() });
    }
    let mut exercisers: std::collections::HashMap<String, Vec<String>> = Default::default();
    let unrunnable_test = |f: &str| werk_test::is_test_file(f) && !werk_test::runnable_test(f, werk_test::ts_package_of(f).is_some());
    for f in changed.iter().filter(|f| unrunnable_test(f) || (!werk_test::is_test_file(f) && !rows.iter().any(|r| &r.file_path == *f))) {
        let routes = changed_routes_of(werk, f);
        let mut ex = werk_test::exercisers_of(f, &candidates, &text_of, &crate_of, routes.as_deref());
        // #4440 — and the tests of the files that import it, transitively
        // (replaces jest --findRelatedTests as a second selection)
        if routes.is_none() {
            if let Some(pkg) = werk_test::ts_package_of(f).filter(|_| werk_test::is_code(f)) {
                let sources = package_sources(werk, &pkg);
                let is_hub = |s: &str| werk_test::exercisers_of(s, &candidates, &text_of, &crate_of, None).len() > werk_test::HUB_EXERCISERS;
                let (via, hubs) = werk_test::importers_of(f, &sources, &text_of, &is_hub);
                for i in &via {
                    ex.extend(werk_test::exercisers_of(i, &candidates, &text_of, &crate_of, None));
                }
                if !hubs.is_empty() {
                    println!("domain-select: {} — not expanded through hub(s) [{}]; their own tests run when they change", f, hubs.join(", "));
                }
                ex.sort();
                ex.dedup();
            }
        }
        // #4440 — a shared Rust file is compiled into the crates that
        // include!() it (shared/scope_units.rs → werk-test, werk-build,
        // werk-deploy); their tests exercise it
        if ex.is_empty() && f.ends_with(".rs") {
            for c in including_crates(werk, f) {
                ex.extend(candidates.iter().map(|r| &r.file_path).filter(|p| p.starts_with(&format!("{c}/"))).cloned());
            }
            ex.sort();
            ex.dedup();
        }
        // a jest setup file runs before every test of its package
        if ex.is_empty() {
            let name = f.rsplit('/').next().unwrap_or(f);
            let root = name.starts_with("jest.config.").then(|| f.rsplit_once('/').map(|(d, _)| d.to_string())).flatten();
            if let Some(pkg) = werk_test::ts_package_of(f).or(root) {
                let cfg = ["jest.config.js", "jest.config.cjs", "jest.config.ts"].iter()
                    .find_map(|c| std::fs::read_to_string(format!("{werk}/{pkg}/{c}")).ok()).unwrap_or_default();
                // the jest config itself runs under every test of its package
                if werk_test::mentions(&cfg, name) || name.starts_with("jest.config.") {
                    ex = candidates.iter().map(|r| &r.file_path).filter(|p| p.starts_with(&format!("{pkg}/"))).cloned().collect();
                }
            }
        }
        match &routes {
            Some(r) if ex.len() <= werk_test::HUB_EXERCISERS => println!(
                "domain-select: {} — changed lines sit in route(s) [{}] → {} test(s) exercise it", f, r.join(", "), ex.len()),
            _ => println!("domain-select: {} → {} test(s) exercise it", f, ex.len()),
        }
        // a test werk-test cannot run exercises nothing on a card
        ex.retain(|t| werk_test::runnable_test(t, werk_test::ts_package_of(t).is_some()));
        exercisers.insert(f.clone(), ex);
    }
    Some(werk_test::domain_selection(changed, &exercisers, rows))
}

/// The crates whose source include!()s or #[path]s a shared Rust file.
fn including_crates(werk: &str, f: &str) -> Vec<String> {
    let name = f.rsplit('/').nth(1).zip(f.rsplit('/').next()).map(|(d, n)| format!("{d}/{n}")).unwrap_or_default();
    let out = Command::new("git").args(["-C", werk, "grep", "-l", "-E", &format!(r#"(include!|#\[path).*{}"#, regex_escape(&name)), "--", "platform/services/*/src/*.rs"]).output();
    let mut crates: Vec<String> = out.ok().map(|o| String::from_utf8_lossy(&o.stdout).lines().filter_map(crate_of).collect()).unwrap_or_default();
    crates.sort();
    crates.dedup();
    crates
}

fn regex_escape(s: &str) -> String {
    s.chars().flat_map(|c| if ".+*?()[]{}|^$\\".contains(c) { vec!['\\', c] } else { vec![c] }).collect()
}

/// A TS package's tracked source files (tests excluded).
fn package_sources(werk: &str, pkg: &str) -> Vec<String> {
    Command::new("git").args(["-C", werk, "ls-files", "--", pkg]).output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines()
            .filter(|l| werk_test::is_code(l) && !werk_test::is_test_file(l) && !l.contains("/node_modules/"))
            .map(String::from).collect())
        .unwrap_or_default()
}

/// The routes a changed file's changed lines sit in, or None (no routes in
/// it, or a changed line outside every handler).
fn changed_routes_of(werk: &str, f: &str) -> Option<Vec<String>> {
    // a replay reads the file as that commit left it, so its hunk line
    // numbers point at the right lines
    let src = match std::env::var("WERK_TEST_REPLAY") {
        Ok(c) => Command::new("git").args(["-C", werk, "show", &format!("{c}:{f}")]).output().ok()
            .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()),
        Err(_) => std::fs::read_to_string(format!("{}/{}", werk, f)).ok(),
    }?;
    let extents = werk_test::route_extents(&src);
    if extents.is_empty() {
        return None;
    }
    let text: Vec<&str> = src.lines().collect();
    let lines: Vec<usize> = werk_test::changed_lines_from_unified_diff(&card_diff_of(werk, f)).into_iter()
        .filter(|l| !text.get(l.saturating_sub(1)).is_some_and(|t| werk_test::is_named_import(t)))
        .collect();
    if lines.is_empty() {
        return None;
    }
    werk_test::routes_of_changed_lines(&extents, &lines).map(|s| s.into_iter().collect())
}

/// The card's diff of one file, `-U0`, against the same base as the changed list.
fn card_diff_of(werk: &str, f: &str) -> String {
    let range = match std::env::var("WERK_TEST_REPLAY") {
        Ok(c) => format!("{c}^..{c}"),
        Err(_) => {
            let base = Command::new("git").args(["-C", werk, "merge-base", "origin/main", "HEAD"]).output().ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|| "HEAD~1".to_string());
            format!("{}..HEAD", base)
        }
    };
    Command::new("git").args(["-C", werk, "diff", "-U0", &range, "--", f]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// #4419 — the Rust crate a path lives in.
fn crate_of(path: &str) -> Option<String> {
    let rest = path.strip_prefix("platform/services/")?;
    let c = rest.split('/').next()?;
    if c.is_empty() { None } else { Some(format!("platform/services/{c}")) }
}

/// #4419 — the unit that runs a selected test file (or a whole package path).
fn unit_for_test(path: &str) -> Option<TestUnit> {
    // #4440 reopened — a shell suite (`platform/scripts/test-*.sh`, `*.test.sh`)
    // is run by the bats lane's runner (suite_runner picks bash); without this
    // a changed shell test was selected and then ran nowhere: 0 units, exit 0.
    if werk_test::is_bats_suite(path) || werk_test::is_shell_suite(path) {
        return Some(TestUnit::BatsSuite(path.to_string()));
    }
    if let Some(c) = path.strip_prefix("platform/services/") {
        let name = c.split('/').next().unwrap_or("");
        if !name.is_empty() {
            return Some(TestUnit::RustCrate(name.to_string()));
        }
    }
    // #4419 — only a REAL package (a package.json in the tree): the first live
    // domain-lane run invented tsc:platform, platform/scripts and proving/scripts
    // from test paths and scored each "deps unavailable".
    werk_test::ts_package_of(path)
        .or_else(|| if Path::new(path).extension().is_none() && !path.contains('.') { Some(path.to_string()) } else { None })
        .filter(|p| !p.starts_with("platform/services/"))
        .filter(|p| Path::new(&std::env::var("WERK_TEST_TREE_ROOT").unwrap_or_default()).join(p).join("package.json").is_file())
        .map(TestUnit::TsPackage)
}

/// #4419 — every page of a generated collection, joined as one `{"data":[…]}`.
/// None if any page fails: a partial registry is not a registry.
fn fetch_all_pages(endpoint: &str) -> Option<Vec<u8>> {
    let base = endpoint.find("://").and_then(|i| endpoint[i + 3..].find('/').map(|j| &endpoint[..i + 3 + j])).unwrap_or(endpoint).to_string();
    let first = if endpoint.contains("limit=10000") { endpoint.replace("limit=10000", "limit=1000") } else { endpoint.to_string() };
    let mut url = first;
    let mut rows: Vec<String> = Vec::new();
    for _ in 0..100 {
        let o = Command::new("curl").args(["-sf", "--max-time", "10", &url]).output().ok()?;
        if !o.status.success() {
            return None;
        }
        let jq = |filter: &str| -> Option<String> {
            let mut c = Command::new("jq")
                .args(["-r", filter])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .ok()?;
            {
                use std::io::Write;
                c.stdin.take()?.write_all(&o.stdout).ok()?;
            }
            let out = c.wait_with_output().ok()?;
            out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
        };
        for l in jq(".data[] | tojson")?.lines().filter(|l| !l.is_empty()) {
            rows.push(l.to_string());
        }
        match jq(".links.next // empty")?.trim() {
            next if !next.is_empty() => url = format!("{base}{next}"),
            _ => return Some(format!("{{\"data\":[{}]}}", rows.join(",")).into_bytes()),
        }
    }
    None
}

#[cfg(test)]
mod selected_node_test_4440 {
    use super::*;

    /// a package with no jest whose `test` script is node's own runner
    fn pkg(tag: &str) -> String {
        let root = std::env::temp_dir().join(format!("wt-4440-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&root);
        let p = root.join("pkg");
        std::fs::create_dir_all(p.join("tests")).unwrap();
        std::fs::create_dir_all(p.join("node_modules/.bin")).unwrap();
        std::fs::write(p.join("package.json"), r#"{"name":"p","scripts":{"test":"WT_FLAG=yes node --test tests/*.test.js"}}"#).unwrap();
        std::fs::write(p.join("tests/red.test.js"), "require('node:test')('red', () => { throw new Error('boom') });\n").unwrap();
        std::fs::write(p.join("tests/green.test.js"),
            "require('node:test')('green', () => { if (process.env.WT_FLAG !== 'yes') throw new Error('env not set') });\n").unwrap();
        root.to_string_lossy().into_owned()
    }

    #[test]
    fn selected_files_in_a_node_test_package_run_and_carry_the_script_env() {
        let w = pkg("green");
        let (ok, cases) = run_jest_selected(&w, "pkg", &["pkg/tests/green.test.js".to_string()]);
        assert!(ok, "{cases:?}");
        assert_eq!(cases.len(), 1, "the selected file ran: {cases:?}");
    }

    /// NEGATIVE PROOF — before #4440 this returned (true, []) without running
    #[test]
    fn a_failing_selected_node_test_file_fails_the_step() {
        let w = pkg("red");
        let (ok, cases) = run_jest_selected(&w, "pkg", &["pkg/tests/red.test.js".to_string()]);
        assert!(!ok, "a red selected file must fail: {cases:?}");
    }
}

#[cfg(test)]
mod shell_suite_unit_4440 {
    use super::*;

    #[test]
    fn a_changed_shell_test_is_a_unit_that_runs() {
        assert_eq!(unit_for_test("platform/scripts/test-nightly-npm-runner.sh"),
            Some(TestUnit::BatsSuite("platform/scripts/test-nightly-npm-runner.sh".into())));
        // NEGATIVE PROOF: an ordinary script is not a suite
        assert_eq!(unit_for_test("platform/scripts/chorus-werk-status"), None);
    }
}
