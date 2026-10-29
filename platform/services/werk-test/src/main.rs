// @domain: builds — placed by chorus-crawl's domain rules (#4538)
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
    jest_plan, parse_rows_and_names, plan_source_label, plan_units_from_rows,
    JestPlan,
    rel_path, scope_rows, scoped_requires_model, spine_args,
    is_test_suite_path, scope_declared_edges, scoped_test_reason,
    unmapped_path,
    undeclared_gaps, CaseResult, CheckKind, Quarantined, ScopeUnit, TestRow, TestUnit,
    discover_ts_packages, set_repo_root,
    is_node_test_file, node_test_plan, package_test_script,
};

mod all_scope;

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
        Ok(code) => {
            record_demo_verdict(&args, code);
            std::process::exit(code)
        }
        // #4536 — every Err from run() is raised before a lane starts (no werk,
        // bad args, own build missing), so the run never started: REFUSED, not RED.
        Err(e) => {
            let v = werk_test::Verdict::Refused(e);
            eprintln!("werk-test: {}", v.label());
            std::process::exit(v.exit_code());
        }
    }
}

/// #4474 — the gate's verdict is the demo's `tests:` badge. werk.yml (v1) wrote
/// it with an inline `werk-demo test-result` after this verb; werk v2 runs only
/// verbs, so its demo said "tests: unmeasured" (run 13). The verb records its
/// own verdict for `werk-test <card> <role>`; nightly and other modes never do.
fn demo_verdict_call(args: &[String], code: i32) -> Option<(String, String, &'static str)> {
    let card = args.first().filter(|a| !a.is_empty() && a.chars().all(|c| c.is_ascii_digit()))?;
    let role = args.get(1).filter(|a| !a.starts_with('-'))?;
    Some((card.clone(), role.clone(), if code == 0 { "pass" } else { "fail" }))
}

/// #3983 — `<state>:<n>` from this card run, for the demo and accept.
/// #4536 — (Test entity, current result) from the registry read, for flip-only writes.
static PRIOR_RESULTS: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
static CARD_COVERAGE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn record_demo_verdict(args: &[String], code: i32) {
    if let Some((card, role, verdict)) = demo_verdict_call(args, code) {
        let mut argv = vec!["test-result".to_string(), card.clone(), verdict.to_string()];
        if let Some(c) = CARD_COVERAGE.get() {
            argv.push(c.clone());
        }
        let ok = std::process::Command::new("werk-demo")
            .args(&argv)
            .env("DEPLOY_ROLE", &role)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("werk-test: WARN werk-demo test-result write failed — the demo badge falls back to its self-run (#3638)");
        }
    }
}

#[cfg(test)]
mod demo_verdict_4474 {
    use super::demo_verdict_call;
    fn a(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn a_card_run_records_its_verdict_for_the_demo() {
        assert_eq!(demo_verdict_call(&a(&["4474", "kade"]), 0), Some(("4474".into(), "kade".into(), "pass")));
        assert_eq!(demo_verdict_call(&a(&["4474", "kade"]), 1), Some(("4474".into(), "kade".into(), "fail")));
    }

    /// NEGATIVE PROOF — the nightly and flag modes must never write a card's badge
    #[test]
    fn other_modes_record_nothing() {
        assert_eq!(demo_verdict_call(&a(&["run", "--scope=all"]), 0), None);
        assert_eq!(demo_verdict_call(&a(&["4474", "--json"]), 0), None);
        assert_eq!(demo_verdict_call(&a(&[]), 0), None);
    }
}

/// Parse `card` and `role`, find the card's werk, detect affected units on the
/// diff, run the planned checks, emit typed failures to the spine, and gate.
fn run(args: &[String]) -> Result<i32, String> {
    // #3920 fold — the nightly's lane runs through THIS verb, so nextest
    // (#3929), the needs-stack typed skips (#3919), and the per-case result
    // writes (#3592) apply at 03:00 identically to the gate.
    // #4198 — `werk-test --scope-of [--root <dir>] <path>...`: what each path,
    // changed on its own, would run on a card — read-only, nothing runs. The
    // answer to "is this file claimed by a unit, or refused?" without making a
    // card find out at its test step.
    if args.first().map(String::as_str) == Some("--scope-of") {
        return Ok(scope_of(&args[1..]));
    }
    // #4540 — the nightly is the same steps with scope = all, plus notify
    if let Some(step) = args.first().map(String::as_str) {
        let all = args.iter().any(|a| a == "--scope=all");
        if step == "notify" || (all && werk_test::run_dir::STEPS.contains(&step)) {
            std::env::set_var(werk_test::LANE_ENV, werk_test::lane_name(true));
            return all_scope::step_all(step, &args[1..], args, &pin_or_handoff);
        }
    }
    // #4540 — the nightly's read-only answers, by name: `last-run`
    // (daily-review), `load-gate` (load-reclassify), `classify` (the row fold).
    // The 03:00 run itself is the dagu nightly calling the steps above.
    if let Some(r) = all_scope::read_mode(args) {
        return r;
    }
    // `lane` — the one queue the run step drives as its child (every type in
    // Jeff's order, the stage pick in WERK_TEST_STAGES). Internal to `run`.
    if args.first().map(String::as_str) == Some("lane") {
        // #4531 — every suite this run spawns learns its lane: timing checks
        // run in the nightly only (Jeff 2026-10-14)
        std::env::set_var(werk_test::LANE_ENV, werk_test::lane_name(true));
        return run_nightly(&args[1..]);
    }
    std::env::set_var(werk_test::LANE_ENV, werk_test::lane_name(false));
    // #4540 — `werk-test <step> <card> <role>` runs one step; without a step
    // name the five run in order in this process (fixtures, hand runs).
    let all_args = args;
    let step: Option<String> = args.first().filter(|a| werk_test::run_dir::STEPS.contains(&a.as_str())).cloned();
    let args: &[String] = if step.is_some() { &args[1..] } else { args };
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
    // #4442 — a card that changes werk-test is judged by its own build. Hand the
    // run to the werk's werk-test (exec: same args, same env, one verdict record).
    let current = std::env::current_exe().unwrap_or_default();
    let already_own = std::env::var(werk_test::OWN_BUILD_ENV).as_deref() == Ok("1");
    match werk_test::own_build_to_run(&werk, &changed, &current, already_own, &|p| p.is_file()) {
        Ok(Some(own)) => {
            use std::os::unix::process::CommandExt;
            println!("werk-test: this card changes werk-test — handing the run to its own build {}", own.display());
            let err = std::process::Command::new(&own).args(all_args).env(werk_test::OWN_BUILD_ENV, "1").exec();
            return Err(format!("could not start the card's own werk-test {}: {}", own.display(), err));
        }
        Ok(None) => {
            // #4537 row 13 — a stale slot copy hands the run to the installed werk-test
            let installed = Path::new(&std::env::var("HOME").unwrap_or_default()).join(".chorus/bin/werk-test-bin");
            let already = std::env::var(werk_test::STALE_SLOT_ENV).as_deref() == Ok("1");
            let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
            if let Some(inst) = werk_test::stale_slot_redirect(&current, &installed, already, &mtime) {
                use std::os::unix::process::CommandExt;
                println!("werk-test: the slot copy {} is older than the installed one and this card does not build werk-test — using the installed werk-test {}",
                    current.display(), inst.display());
                emit_spine("test.slot.stale", &role, &card, &trace, &[("path", &current.display().to_string())]);
                let err = std::process::Command::new(&inst).args(all_args).env(werk_test::STALE_SLOT_ENV, "1").exec();
                return Err(format!("could not start the installed werk-test {}: {}", inst.display(), err));
            }
        }
        Err(why) => {
            println!("werk-test: BLOCK — {}", why);
            emit_spine("test.own_build.missing", &role, &card, &trace, &[("message", &why)]);
            return Ok(1);
        }
    }

    let runs_base = werk_test::run_dir::runs_base();
    let key = werk_test::run_dir::run_key(&trace, step.as_deref(), &card, std::process::id())?;
    let dir = werk_test::run_dir::RunDir::create(werk_test::run_dir::folder_for(&runs_base, &key)?)?;
    if let Some(code) = pin_or_handoff(&dir, step.as_deref(), all_args)? {
        return Ok(code);
    }
    let _ = PROGRESS_CTX.set((role.clone(), card.clone(), trace.clone()));
    let ctx = Ctx { args: args.to_vec(), card, role, werk, trace, changed, dir };
    match step.as_deref() {
        // #4540 (Wren 10-16) — a step run names its scope; no default, so a
        // child pipeline called without SCOPE refuses here, never runs "diff" by luck
        Some("scope") if flag_value(args, "--scope").as_deref() != Some("diff") => {
            Err("`werk-test scope <card> <role>` needs --scope=diff (or --scope=all for the nightly): a step has no default scope".into())
        }
        Some("scope") => Ok(step_scope(&ctx)?.unwrap_or(0)),
        Some("prepare") => step_prepare(&ctx),
        // #4540 (Jeff 10-16 B) — `run` is the one queue in his type order;
        // `run --test-type=perf` runs perf alone after it, on a quiet box
        Some("run") => match flag_value(args, "--test-type") {
            Some(t) => step_run(&ctx, &t),
            None => {
                use_type_order(&ctx.dir);
                for t in werk_test::run_dir::queue_types_in(&werk_test::stage_order()) {
                    step_run(&ctx, t)?;
                }
                Ok(0)
            }
        },
        Some("report") => step_report(&ctx, true),
        Some("verdict") => step_verdict(&ctx),
        _ => {
            // all five here: what dagu runs as steps, in one process
            let out = (|| -> Result<i32, String> {
                if let Some(code) = step_scope(&ctx)? {
                    return Ok(code);
                }
                step_prepare(&ctx)?;
                use_type_order(&ctx.dir);
                for t in werk_test::run_dir::queue_types_in(&werk_test::stage_order()) {
                    step_run(&ctx, t)?;
                }
                step_run(&ctx, "perf")?;
                step_report(&ctx, false)?;
                step_verdict(&ctx)
            })();
            let _ = std::fs::remove_dir_all(&ctx.dir.path);
            out
        }
    }
}

/// #4540 (Silas 10-16) — one run, one binary: scope pins the binary it is
/// into the run folder; a later step hands the run to that copy. `None` = go on.
fn pin_or_handoff(dir: &werk_test::run_dir::RunDir, step: Option<&str>, all_args: &[String]) -> Result<Option<i32>, String> {
    let current = std::env::current_exe().map_err(|e| format!("own path: {}", e))?;
    match step {
        Some("scope") => werk_test::run_dir::pin_binary(dir, &current).map(|_| None),
        Some(_) => {
            let already = std::env::var(werk_test::run_dir::PINNED_ENV).as_deref() == Ok("1");
            if already {
                // #4562 — informational, so stdout: /cws reads stderr's tail as the reason a step stopped
                println!("werk-test: running the run's pinned build {}", current.display());
            }
            match werk_test::run_dir::pinned_to_exec(dir, &current, already) {
                Some(pinned) => {
                    use std::os::unix::process::CommandExt;
                    let err = std::process::Command::new(&pinned).args(all_args).env(werk_test::run_dir::PINNED_ENV, "1").exec();
                    Err(format!("could not start the run's pinned werk-test {}: {}", pinned.display(), err))
                }
                None => Ok(None),
            }
        }
        None => Ok(None),
    }
}

/// #4540 — what every step knows before it starts: the card, its werk and diff.
struct Ctx {
    args: Vec<String>,
    card: String,
    role: String,
    werk: String,
    trace: String,
    changed: Vec<String>,
    dir: werk_test::run_dir::RunDir,
}

/// #4540 — the registry rows scope read, from the run folder.
fn rows_from(dir: &werk_test::run_dir::RunDir) -> Result<(Vec<TestRow>, Vec<String>, Vec<String>, &'static str), String> {
    let tsv = dir.need("rows", "scope")?.join("\n");
    let source = dir.need("source", "scope")?;
    let (rows, names, entities) = parse_rows_and_names(&tsv);
    let label = if source.first().map(String::as_str) == Some("model") { plan_source_label(true, rows.len()) } else { plan_source_label(false, 0) };
    Ok((rows, names, entities, label))
}

/// #4540 — the selection both prepare and run need: what is in the units,
/// which packages jest narrowed, which files it chose.
struct Picks {
    units: Vec<TestUnit>,
    narrowed: Vec<String>,
    chosen: std::collections::BTreeSet<String>,
    crate_dirs: Vec<String>,
}

impl Picks {
    fn new(werk: &str, units: &[TestUnit], jplan: &JestPlan) -> Picks {
        let (narrowed, chosen) = match jplan {
            JestPlan::Selected(sels) => (
                sels.iter().map(|s| s.package.clone()).collect(),
                sels.iter().flat_map(|s| s.test_files.iter().cloned()).collect(),
            ),
            JestPlan::FullFallback { .. } => (Vec::new(), Default::default()),
        };
        let crate_dirs = units.iter().filter_map(|u| match u {
            TestUnit::RustCrate(c) => Some(crate_dir(werk, c)),
            _ => None,
        }).collect();
        Picks { units: units.to_vec(), narrowed, chosen, crate_dirs }
    }
    fn in_units(&self, f: &str) -> bool {
        self.crate_dirs.iter().any(|d| f.starts_with(&format!("{}/", d)))
            || self.units.iter().any(|u| matches!(u, TestUnit::TsPackage(p) if f.starts_with(&format!("{}/", p))))
    }
    fn narrows(&self, f: &str) -> bool {
        self.narrowed.iter().any(|p| f.starts_with(&format!("{p}/")))
    }
    fn picked(&self, f: &str) -> bool {
        self.in_units(f) && (!self.narrows(f) || self.chosen.contains(f))
    }
    /// (every needs-stack file, the selected ones, the selected rows, the unit's rows)
    fn needs_stack(&self, rows: &[TestRow]) -> (std::collections::BTreeSet<String>, Vec<String>, usize, usize) {
        let ns_all = werk_test::needs_stack_files(rows);
        let selected: Vec<String> = ns_all.iter().filter(|f| self.picked(f)).cloned().collect();
        let in_units = |f: &str| self.in_units(f);
        let tests = werk_test::needs_stack_in_selection(rows, &self.narrowed, &self.chosen, &in_units);
        let registered = rows.iter().filter(|r| r.hermeticity == "needs-stack" && self.in_units(&r.file_path)).count();
        (ns_all, selected, tests, registered)
    }
}

/// #4540 step 1 — scope: what this diff runs, and why. `Some(code)` ends the
/// run here (a select-only or explain run, or nothing to plan).
fn step_scope(ctx: &Ctx) -> Result<Option<i32>, String> {
    let (args, werk, changed, role, card, trace) = (&ctx.args[..], ctx.werk.as_str(), &ctx.changed, ctx.role.as_str(), ctx.card.as_str(), ctx.trace.as_str());

    let legacy_units = affected_units(&changed);
    // #3634 — stage 2: derive the plan from the tests domain. Model rows (filePath,
    // covers) widen the legacy path-derived units to every unit holding tests that
    // cover a touched domain — UNION, never smaller (the superset AC). A failed
    // fetch degrades to the legacy plan, loudly (test.plan.degraded), never silently.
    let tsv = fetch_rows_tsv();
    let (rows, _row_names, _row_entities, plan_source) = rows_of_tsv(tsv.as_deref());
    ctx.dir.put("rows", &tsv.unwrap_or_default().lines().map(|l| l.to_string()).collect::<Vec<_>>())?;
    ctx.dir.put("source", &[if plan_source == "model" { "model" } else { "fallback" }.to_string()])?;
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
                // #4198 — a changed file that runs nothing says so by name, so a
                // script no test reads is visible on the card, not silently passed
                for f in changed.iter().filter(|f| werk_test::scope_irrelevant(f)) {
                    if matches!(diff_scoped_units(&werk, std::slice::from_ref(f)), Some(u) if u.is_empty()) {
                        println!("scope(diff): {} runs nothing: no unit claims it and no test names it", f);
                        emit_spine("test.scope.nothing", &role, &card, &trace, &[("path", f)]);
                    }
                }
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
    // #4539 AC4 — changed paths no test claims; UNMEASURED once the run's list exists
    let mut uncovered: Vec<String> = Vec::new();
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
            // #4539 AC4 — a changed path that no test claims is UNMEASURED, said
            // with its path, never a silent pass. Prose and role state (the
            // scope_irrelevant list) change no build or test output.
            for f in werk_test::uncovered_changes(&d.untagged, &refused) {
                println!("domain-select: {} — no registered test exercises it, and it is in no package", f);
                uncovered.push(format!("no test covers {}", f));
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
        return Ok(Some(0));
    }
    // #3917 AC5 — `--explain` answers "what would this diff measure?" without
    // running anything. The question had no answer before: the only way to learn
    // the gate had selected nothing was to read a green summary and disbelieve it.
    if args.iter().any(|a| a == "--explain") {
        println!("changed: {} file(s)", changed.len());
        for u in &units {
            println!("  unit: {}", unit_name(u));
        }
        if units.is_empty() {
            println!("  {} — this diff would be waved through", gate_outcome(0, false).label());
        }
        return Ok(Some(0));
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

    // #4539 — a test whose header says needs: machine or nightly proves the box
    // or the clock, not the card: a card run does not select it and says so in
    // one line outside the verdict; the nightly runs it.
    let picks = Picks::new(werk, &units, &jplan);
    let nightly_candidates: Vec<String> = {
        let mut c: Vec<String> = rows.iter()
            .map(|r| r.file_path.clone())
            .chain(changed.iter().filter(|f| werk_test::is_card_test(f)).cloned())
            .filter(|f| picks.picked(f))
            .collect();
        c.extend(units.iter().filter_map(|u| match u { TestUnit::BatsSuite(s) => Some(s.clone()), _ => None }));
        c.sort();
        c.dedup();
        c
    };
    let nightly_only = werk_test::nightly_only_files(&nightly_candidates, &rows, &|f: &str| {
        std::fs::read_to_string(format!("{}/{}", werk, f)).ok()
    });
    println!("{}", werk_test::nightly_only_report(&nightly_only));
    // #4540 — the types this plan runs; a planned type with no result reads UNMEASURED
    let mut types: Vec<String> = plan.iter().map(|c| werk_test::run_dir::check_type(c.kind).to_string()).collect();
    if werk_test::ui_plan(werk_test::ui_lane_fires(changed) || std::env::var("WERK_TEST_FULL").map(|v| v == "1").unwrap_or(false),
                          werk_test::ui_files(&rows).len()).is_some() {
        types.push("ui".to_string());
    }
    types.sort();
    types.dedup();
    let d = &ctx.dir;
    d.put("units", &werk_test::run_dir::units_lines(&units))?;
    d.put("jest", &werk_test::run_dir::jest_lines(&jplan))?;
    d.put("nightly", &werk_test::run_dir::pair_lines(nightly_only.iter().map(|(a, b)| (a, b))))?;
    d.put("uncovered", &uncovered)?;
    d.put("types", &types)?;
    note_type_order(d)?;
    let _ = self_mod;
    Ok(None)
}

/// #4540 step 2 — prepare: build what the suites run, read the holds, probe
/// the stack. Writes `unbuilt`, `holds`, `stack`.
fn step_prepare(ctx: &Ctx) -> Result<i32, String> {
    let (werk, changed, role, card, trace) = (ctx.werk.as_str(), &ctx.changed, ctx.role.as_str(), ctx.card.as_str(), ctx.trace.as_str());
    let d = &ctx.dir;
    let (rows, _, _, plan_source) = rows_from(d)?;
    let units = werk_test::run_dir::units_of(&d.need("units", "scope")?)?;
    let jplan = werk_test::run_dir::jest_of(&d.need("jest", "scope")?)?;
    let plan = check_plan(&units);
    let self_mod = is_self_modifying(changed);
    let args: &[String] = &ctx.args;

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

    d.put("unbuilt", &werk_test::run_dir::pair_lines(unbuilt.iter()))?;
    // Quarantined cases (flaky holds), fetched from the tests domain (#2530).
    // #4539 — only holds whose `until` is today or later apply; an expired one
    // runs, and both are named on this line.
    let today = today_iso();
    let all_holds = quarantined_cases();
    let quarantined = werk_test::active_holds(&all_holds, &today);
    println!("{}", werk_test::quarantine_report_on(&all_holds, &today));
    d.put("holds", &werk_test::run_dir::holds_lines(&quarantined))?;
    println!(
        "-- werk-test #{} ({}) — {} unit(s), {} check(s){} --",
        card,
        role,
        units.len(),
        plan.len(),
        if self_mod { ", changes the test gate — judged by its own build, a red blocks" } else { "" }
    );
    if plan_source == "fallback" {
        emit_spine("test.plan.degraded", role, card, trace,
            &[("reason", "tests-domain-unreachable"), ("plan", "legacy-lanes")]);
    }
    emit_spine("test.started", role, card, trace,
        &[("units", &units.len().to_string()), ("checks_planned", &plan.len().to_string()), ("plan_source", plan_source)]);
    // #3919 — the integration tier. Registered needs-stack tests inside the
    // selected units run ONLY with the live stack; without it they are a TYPED,
    // counted SKIPPED state — never green-by-default, never silence.
    let picks = Picks::new(werk, &units, &jplan);
    let (_ns_all, selected_ns, selected_ns_tests, registered_in_unit) = picks.needs_stack(&rows);
    if selected_ns_tests == 0 && registered_in_unit > 0 {
        println!("{}", werk_test::integration_report_none_selected(registered_in_unit));
    }
    // #4336 — an empty needs-stack selection is only "nothing needs the stack"
    // when the registry answered; on the fallback plan, probe.
    let stack = if selected_ns.is_empty() && plan_source != "fallback" {
        werk_test::StackState::Up
    } else {
        stack_state_now()
    };
    let stack_down: Option<String> = stack_down_of(&stack);
    if let werk_test::StackState::Unmeasurable(u) = &stack {
        println!("{}", werk_test::integration_report_unmeasurable(selected_ns_tests, u));
        emit_spine("test.integration.unmeasurable", role, card, trace,
            &[("count", &selected_ns_tests.to_string()), ("saw", u)]);
    } else {
        // #4251 — no plan line when the stack is up; the measured line after the run is the honest one
        if stack_down.is_some() {
            println!("{}", werk_test::integration_report(selected_ns_tests, stack_down.as_deref()));
        } else if selected_ns_tests == 0 {
            println!("{}", werk_test::integration_report(0, None));
        }
        if let Some(down) = &stack_down {
            emit_spine("test.integration.skipped", role, card, trace,
                &[("count", &selected_ns_tests.to_string()), ("stack_down", down)]);
        }
    }
    d.put("stack", &werk_test::run_dir::stack_lines(&stack))?;
    Ok(0)
}

/// #4540 step 3 — run one test type's checks. Writes `ran-<type>` whole; exits
/// 0 once written (the verdict gates), non-zero only when it could not run.
fn step_run(ctx: &Ctx, test_type: &str) -> Result<i32, String> {
    if !werk_test::run_dir::TEST_TYPES.contains(&test_type) {
        return Err(format!("unknown test type {:?}; one of {}", test_type, werk_test::run_dir::TEST_TYPES.join(", ")));
    }
    let (werk, changed, role, card, trace) = (ctx.werk.as_str(), &ctx.changed, ctx.role.as_str(), ctx.card.as_str(), ctx.trace.as_str());
    let d = &ctx.dir;
    let (rows, row_names, _, _) = rows_from(d)?;
    let units = werk_test::run_dir::units_of(&d.need("units", "scope")?)?;
    let jplan = werk_test::run_dir::jest_of(&d.need("jest", "scope")?)?;
    let nightly_only = werk_test::run_dir::pairs_of(&d.need("nightly", "scope")?);
    let unbuilt: std::collections::BTreeMap<String, String> = werk_test::run_dir::pairs_of(&d.need("unbuilt", "prepare")?).into_iter().collect();
    let quarantined = werk_test::run_dir::holds_of(&d.need("holds", "prepare")?);
    let stack = werk_test::run_dir::stack_of(&d.need("stack", "prepare")?)?;
    let _ = HOLDS.set(quarantined.clone());
    let q_names: Vec<&str> = quarantined.iter().map(|q| q.case.as_str()).collect();
    let stack_down = stack_down_of(&stack);
    // #4102 — the werk lane sets RUN_INTEGRATION from the probe, as the nightly does
    if stack_down.is_none() {
        std::env::set_var("RUN_INTEGRATION", "true");
    }
    let picks = Picks::new(werk, &units, &jplan);
    let (ns_all, selected_ns, _, _) = picks.needs_stack(&rows);
    let ns_excluded: Vec<String> = if stack_down.is_some() { selected_ns.clone() } else { Vec::new() }
        .into_iter()
        .chain(nightly_only.iter().map(|(f, _)| f.clone()))
        .collect();
    let plan: Vec<_> = check_plan(&units).into_iter()
        .filter(|c| werk_test::run_dir::check_type(c.kind) == test_type)
        .collect();
    let mut any_failed = false;
    let mut failed_units: Vec<String> = Vec::new();
    let mut unmeasured: Vec<String> = Vec::new();
    let mut failed_count: usize = 0;
    let mut all_cases: Vec<CaseResult> = Vec::new();
    let mut unmatched_cargo: usize = 0;
    let mut unit_costs: Vec<(String, f64)> = Vec::new();

    for check in &plan {
        let target = check.unit.as_ref().map(unit_name).unwrap_or("workspace");
        let check_started = std::time::Instant::now();
        let cases_before = all_cases.len();
        let unmatched_before = unmatched_cargo;
        let mut nothing_selected = false;
        // #4539 — the cargo binaries this check ran (empty = all) and left out
        let mut cargo_bins: (Vec<String>, Vec<String>) = Default::default();
        // #4454 — the unit starts on the trace: a hung unit reads started, no end
        emit_spine("test.unit.started", &role, &card, &trace,
            &[("check", check.kind.label()), ("unit", target), ("message", &format!("{} {} started", check.kind.label(), target))]);
        let ok = match (&check.kind, &check.unit) {
            (CheckKind::CargoTest, Some(TestUnit::RustCrate(c))) => {
                // stack-down: this crate's needs-stack integration binaries
                // (tests/<stem>.rs → binary <stem>) drop out of the run, typed.
                let crate_prefix = format!("{}/tests/", crate_dir(&werk, c));
                let ns_bins: Vec<String> = ns_excluded.iter()
                    .filter_map(|f| f.strip_prefix(&crate_prefix))
                    .filter_map(|rest| rest.strip_suffix(".rs"))
                    .filter(|stem| !stem.contains('/'))
                    .map(|s| s.to_string())
                    .collect();
                let ns_refs: Vec<&str> = ns_bins.iter().map(|s| s.as_str()).collect();
                // #4440 reopen — only test files changed: run just those binaries
                let only = werk_test::cargo_only_bins(c, &changed);
                // #4539 — the only-bins path must honour the same exclusions: a
                // card that edits a nightly-only test binary does not run it
                let kept = werk_test::only_bins_kept(&only, &ns_bins);
                cargo_bins = (kept.clone(), ns_bins.clone());
                let (ok, cases, text) = if !only.is_empty() && kept.is_empty() {
                    println!("cargo-select: {} → {} changed test binar(y/ies), all nightly only or stack-down: not run", c, only.len());
                    nothing_selected = true;
                    (true, Vec::new(), String::new())
                } else if only.is_empty() {
                    run_cargo_sel(&werk, c, &q_names, &ns_refs, &[])
                } else {
                    let only = kept;
                    println!("cargo-select: {} → only {} (no src change; the tests that changed run)", c, only.join(", "));
                    let only_refs: Vec<&str> = only.iter().map(|s| s.as_str()).collect();
                    run_cargo_sel(&werk, c, &q_names, &ns_refs, &only_refs)
                };
                let crate_dir = crate_dir(&werk, c);
                // #4685 — a new, not-yet-registered test file keeps its cases
                let targets: Vec<String> = werk_test::parse_nextest_case_runs(&text).into_iter().map(|(_, t, _)| t).collect();
                let on_disk = |fp: &str| Path::new(&werk).join(fp).is_file();
                for (i, (path, result)) in cases.into_iter().enumerate() {
                    let target = targets.get(i).map(String::as_str).unwrap_or("");
                    match werk_test::resolve_cargo_case(&path, target, &crate_dir, &rows, &row_names, on_disk) {
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
                                    println!("   jest:{} — selection all needs-stack (stack down) or nightly only, not run", p);
                                    // #4539 — a selection that is all nightly-only was never
                                    // selected for a card run: not "ran nothing"
                                    if werk_test::left_to_nightly(&sel.test_files, &nightly_only) {
                                        nothing_selected = true;
                                    }
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
                                // #4536 run 15 — nothing of the card's lives here: not a run that proved nothing
                                nothing_selected = true;
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
                    unmeasured.push(format!("{} — not built", target));
                    true
                } else if nightly_only.iter().any(|(f, _)| f == s) {
                    // #4539 — named in the "run nightly only" line; not a result,
                    // and not "ran nothing" either: it was never selected
                    println!("   bats:{} — runs nightly only, not in a card run", s);
                    nothing_selected = true;
                    true
                } else if std::fs::read_to_string(format!("{}/{}", werk, s))
                    .is_ok_and(|t| werk_test::header_says_sends_live_alerts(&t)) {
                    // #4393 rule 4 — a suite that sends live alerts never runs in
                    // a card run (3968 and 4027 paged Silas and Jeff on #4335);
                    // the nightly or a deliberate hand run still runs it
                    println!("   bats:{} — sends live alerts: not run in a card run (UNMEASURED; the nightly runs it)", s);
                    unmeasured.push(format!("{} — sends live alerts, the nightly runs it", target));
                    true
                } else {
                    let (outcome, cases) = run_bats(&werk, s);
                    all_cases.extend(cases);
                    match outcome {
                        BatsOutcome::Pass => true,
                        BatsOutcome::Unmeasured => {
                            // #4537 — a runner that could not start names itself below
                            if !werk_test::lane_unmeasured_pending() {
                                unmeasured.push(format!("{} — suite reported unmeasured", target));
                            }
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
        // #4537 — a lane that could not measure: UNMEASURED with its reason
        for why in werk_test::take_lane_unmeasured() {
            unmeasured.push(format!("{} — {}", target, why));
        }
        // #4539 — each held cargo case in this crate did not run (nextest's
        // filterset): UNMEASURED, named. jest and bats note their own holds.
        if let (CheckKind::CargoTest, Some(TestUnit::RustCrate(c))) = (&check.kind, &check.unit) {
            let dir = crate_dir(&werk, c);
            let held = werk_test::holds_in(&quarantined, &dir);
            let ran = werk_test::holds_in_ran_bins(&held, &dir, &cargo_bins.0, &cargo_bins.1);
            let text_of = |f: &str| std::fs::read_to_string(format!("{}/{}", werk, f)).ok();
            for line in werk_test::hold_lines(target, &werk_test::holds_still_present(&ran, &text_of)) {
                unmeasured.push(line);
            }
        }
        // #4536 — a case lane that passed with 0 cases ran nothing: UNMEASURED, not green.
        let ran = all_cases.len() - cases_before + (unmatched_cargo - unmatched_before);
        let already = unmeasured.last().is_some_and(|u| werk_test::unmeasured_is(u, target));
        if !already && !nothing_selected && werk_test::reports_cases(check.kind, target) && werk_test::ran_nothing(check.kind, ok, ran) {
            unmeasured.push(format!("{} — ran nothing", target));
        }
        let secs = check_started.elapsed().as_secs_f64();
        // #4454 — this check's cases on the trace, as soon as it ends, and
        // whatever its fixtures said
        emit_case_events(target, &all_cases[cases_before..], &role, &card, &trace);
        forward_test_events(&werk);
        unit_costs.push((format!("{}:{}", check.kind.label(), target), secs));
        let verdict = if unmeasured.last().is_some_and(|u| werk_test::unmeasured_is(u, target)) {
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
            failed_units.push(format!("{}:{}", check.kind.label(), target));
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
    // #3920 — the browser lane: registered ui files run as ONE workspace check
    // when the diff touches a ui surface; it is the ui type's step.
    let ui_set = werk_test::ui_files(&rows);
    let ui_fired = werk_test::ui_lane_fires(changed)
        || std::env::var("WERK_TEST_FULL").map(|v| v == "1").unwrap_or(false);
    let ui_check = werk_test::ui_plan(ui_fired, ui_set.len()).filter(|_| test_type == "ui");
    if let Some(kind) = ui_check {
        if let Some(down) = &stack_down_ui(&selected_ns, &ns_all) {
            println!("   {}: SKIPPED (stack-down: {}) — {} registered ui file(s) not run; typed skip", kind.label(), down, ui_set.len());
            emit_spine("test.integration.skipped", &role, &card, &trace,
                &[("count", &ui_set.len().to_string()), ("stack_down", down), ("lane", "ui")]);
        } else {
            let (ok, summary) = run_ui_flows(&werk, &ui_set, &quarantined);
            // #4539 — a held spec is excluded before the run; it is UNMEASURED too
            for h in &quarantined {
                if ui_set.iter().any(|f| f.rsplit('/').next() == Some(h.case.as_str())) {
                    unmeasured.push(format!("{} — quarantined until {}: {} ({})", kind.label(), h.until, h.case, h.reason));
                }
            }
            let word = match ok { Some(true) => "ok", Some(false) => "FAIL", None => "UNMEASURED" };
            println!("   {}:workspace … {}{}", kind.label(), word, summary);
            match ok {
                Some(false) => {
                    any_failed = true;
                    failed_count += 1;
                    failed_units.push(format!("{}:workspace", kind.label()));
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
    } else if ui_fired && test_type == "ui" {
        println!("   ui-flows: none registered testConcern=ui — explicit absence (#3443)");
    }

    let _ = (any_failed, failed_count);
    let result = werk_test::run_dir::TypeResult {
        cases: all_cases, unmeasured, failed: failed_units, unit_secs: unit_costs, unmatched_cargo,
    };
    d.put(&format!("ran-{}", test_type), &result.lines())?;
    Ok(0)
}

/// #4540 step 4 — report: the run's evidence (suite run, card coverage, cost,
/// spine). Reads every `ran-*`; never gates. `steps`: dagu ran the steps, so
/// the demo badge is written here.
fn step_report(ctx: &Ctx, steps: bool) -> Result<i32, String> {
    let (werk, changed, role, card, trace) = (ctx.werk.as_str(), &ctx.changed, ctx.role.as_str(), ctx.card.as_str(), ctx.trace.as_str());
    let d = &ctx.dir;
    let (rows, row_names, row_entities, plan_source) = rows_from(d)?;
    let units = werk_test::run_dir::units_of(&d.need("units", "scope")?)?;
    let jplan = werk_test::run_dir::jest_of(&d.need("jest", "scope")?)?;
    let stack = werk_test::run_dir::stack_of(&d.need("stack", "prepare")?)?;
    let plan = check_plan(&units);
    let self_mod = is_self_modifying(changed);
    let all = werk_test::run_dir::TypeResult::all(d);
    let (all_cases, unit_costs, unmatched_cargo) = (&all.cases, &all.unit_secs, all.unmatched_cargo);
    let failed_count = all.failed.len();
    let any_failed = failed_count > 0;
    let phase_elapsed: f64 = unit_costs.iter().map(|(_, s)| s).sum();

    // #3953/#3955 — the test phase reports what it selects cost: elapsed vs the
    // table's in-run target (col3). Over-target WARNS LOUDLY (spine event + the
    // per-unit culprit table) and NEVER blocks a green run — Jeff's ruling after
    // run 66 failed 1642 green tests at 701s.
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

    let outcome = gate_outcome(units.len(), any_failed);
    let execution_duration_ms = (phase_elapsed * 1000.0) as u128;
    let execution_extras = werk_test::completed_extras(&outcome, units.len(), plan.len(), failed_count, execution_duration_ms, self_mod);
    let execution_refs: Vec<(&str, &str)> = execution_extras.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    emit_spine("test.execution.completed", role, card, trace, &execution_refs);
    let writeback_started = std::time::Instant::now();
    // #4579 reopen — the run no longer becomes a TestSuiteRun in the graph
    // (Jeff 10-18 19:10); the run folder and the spine carry it.
    if unmatched_cargo > 0 {
        emit_spine("testresult.unmatched", role, card, trace,
            &[("count", &unmatched_cargo.to_string()), ("kind", "cargo-ambiguous-or-unregistered")]);
    }
    // #4139 — the measured integration line: what ran, after it ran
    let picks = Picks::new(werk, &units, &jplan);
    let (_, selected_ns, selected_ns_tests, _) = picks.needs_stack(&rows);
    if stack_down_of(&stack).is_none() && selected_ns_tests > 0 {
        let executed_ns = all_cases.iter().filter(|c| selected_ns.iter().any(|f| f == &c.file_path)).count();
        println!("{}", werk_test::integration_measured_report(selected_ns_tests, executed_ns));
    }
    let (joined, unregistered) = werk_test::join_cases(all_cases, &rows, &row_names, &row_entities);
    // #3983 — the card's own tests: recorded with the verdict, read at accept
    let (cov_state, cov_ran) = werk_test::card_coverage(plan_source, units.len(), joined.len());
    println!("card-coverage: {} ({} registered case(s) covering this diff ran)", cov_state, cov_ran);
    emit_spine("test.card.coverage", role, card, trace,
        &[("state", cov_state), ("ran", &cov_ran.to_string()), ("units", &units.len().to_string())]);
    let _ = CARD_COVERAGE.set(format!("{}:{}", cov_state, cov_ran));
    if unregistered > 0 {
        emit_spine("testresult.unregistered", role, card, trace, &[("count", &unregistered.to_string())]);
        println!("executed-but-unregistered: {} case(s) (no registered Test identity — not posted)", unregistered);
    }
    // #4536 — a card run writes no Test row; only the canonical nightly does.
    let mut completed = werk_test::completed_extras(&outcome, units.len(), plan.len(), failed_count, execution_duration_ms, self_mod);
    completed.push(("execution_duration_ms".into(), execution_duration_ms.to_string()));
    completed.push(("writeback_duration_ms".into(), writeback_started.elapsed().as_millis().to_string()));
    let completed_refs: Vec<(&str, &str)> = completed.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    emit_spine("test.completed", role, card, trace, &completed_refs);
    if steps {
        let code = run_verdict(d)?.exit_code();
        record_demo_verdict(&[card.to_string(), role.to_string()], code);
    }
    Ok(0)
}

/// #4540 — the verdict over every type's result, the uncovered paths, and any
/// planned type that never reported.
fn run_verdict(d: &werk_test::run_dir::RunDir) -> Result<werk_test::Verdict, String> {
    let all = werk_test::run_dir::TypeResult::all(d);
    let mut unmeasured: Vec<String> = d.need("uncovered", "scope")?;
    for t in d.need("types", "scope")? {
        if d.get(&format!("ran-{}", t)).is_none() {
            unmeasured.push(format!("{} — did not report (its run step did not finish)", t));
        }
    }
    unmeasured.extend(all.unmeasured.iter().cloned());
    Ok(werk_test::verdict(all.cases.len(), !all.failed.is_empty(), &unmeasured))
}

/// #4540 step 5 — verdict: the one gate. RED wins; then UNMEASURED; then CLEAN.
fn step_verdict(ctx: &Ctx) -> Result<i32, String> {
    let d = &ctx.dir;
    let all = werk_test::run_dir::TypeResult::all(d);
    let v = run_verdict(d)?;
    // #4265 — an unmeasured suite is reported before the verdict
    if let werk_test::Verdict::Unmeasured(u) = &v {
        if !u.is_empty() {
            println!("   UNMEASURED: {} suite(s) scored nothing — {}", u.len(), u.join(", "));
        }
    }
    if !all.failed.is_empty() {
        println!("   RED in: {}", all.failed.join(", "));
        // #4540 — the line /cws reads off the verdict step: which types went red
        let red: Vec<&str> = werk_test::run_dir::TEST_TYPES.iter().copied()
            .filter(|t| d.get(&format!("ran-{}", t)).is_some_and(|l| !werk_test::run_dir::TypeResult::of(&l).failed.is_empty()))
            .collect();
        println!("verdict: RED — {} type{} failed: {}", red.len(), if red.len() == 1 { "" } else { "s" }, red.join(", "));
    }
    let exit = v.exit_code();
    println!("werk-test: {} (exit {})", v.label(), exit);
    d.put("verdict", &[exit.to_string()])?;
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
        .filter(|c| Path::new(&format!("{}/{}/Cargo.toml", root, crate_dir(&root, c))).is_file())
        .collect();
    if let Some(o) = &only {
        if crates.is_empty() {
            return Err(format!("--crate={} holds no registered tests", o));
        }
    }

    // #4539 — one rule with the card run: an expired hold runs, a held case
    // does not run in any lane and reads UNMEASURED "quarantined until <date>"
    let today = today_iso();
    let all_holds = quarantined_cases();
    let quarantined = werk_test::active_holds(&all_holds, &today);
    let _ = HOLDS.set(quarantined.clone());
    let q_names: Vec<&str> = quarantined.iter().map(|q| q.case.as_str()).collect();
    println!("{}", werk_test::quarantine_report_on(&all_holds, &today));

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
    let mint_role = std::env::var("WERK_TEST_MINT_ROLE").unwrap_or_else(|_| "nightly".to_string());
    // #4063 — a werk-rooted nightly never writes prod's ledger (see
    // nightly_writeback_withheld). Withheld cases are counted and named once,
    // never silently, and never as "lost": nothing was promised to the store.
    let withheld = werk_test::nightly_writeback_withheld(&root);
    let withheld_total = AtomicUsize::new(0);
    // #4139 — needs-stack cases that actually produced a result this run
    let executed_ns = AtomicUsize::new(0);
    if withheld {
        println!("!! werk nightly: results will NOT be written to the prod test ledger — root {} is a werk; only the canonical nightly writes Test rows (#4063, #4536)", root);
    }
    // #4144 AC3 — one poster for the run: a unit's results are stored, then
    // its census lines printed, off the worker's path. A killed run loses only
    // what is still queued; the run joins the poster before it counts.
    let (post_tx, poster) = {
        let (mint_role, card, trace) = (mint_role.clone(), card.clone(), trace.clone());
        werk_test::spawn_poster(move |job: &werk_test::PostJob<(Vec<(CaseResult, String, String)>, Vec<String>)>| {
            let (joined, reasons) = &job.payload;
            let unit = job.unit.as_str();
            let reason_of = |c: &CaseResult| -> String {
                joined.iter().position(|(j, _, _)| j == c).and_then(|i| reasons.get(i).cloned()).unwrap_or_default()
            };
            let stored = store_latest_results(&mint_role, &card, &trace, joined, run_epoch_ms, &reason_of);
            // #4145 — the run's own record of what it posted, one line per case,
            // so the census is "registry minus these" and never a ledger walk.
            // #4247 — the result rides with the identity. Without it the log had a
            // result per SUITE and an identity per TEST, two units that cannot be
            // compared; the census below reads this same line.
            for (c, _, registered) in joined.iter() {
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
                    werk_test::run_record::registered_suffix_len(&c.test_name, registered)
                );
            }
            println!("nightly-stored|{}|{} of {}", unit, stored, joined.len());
            stored
        })
    };
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
        // what this run expects the store to hold, for the storage line (#4536)
        expected_total.fetch_add(joined.len(), Ordering::SeqCst);
        unregistered_total.fetch_add(unregistered, Ordering::SeqCst);
        // #4144 AC3 — the post goes to the poster; this worker starts its next
        // unit now. Each failed case's reason travels with the job.
        let reasons_owned: Vec<String> = joined.iter().map(|(c, _, _)| reason_of(c)).collect();
        let expected = joined.len();
        let _ = post_tx.send(werk_test::PostJob { unit: unit.to_string(), payload: (joined, reasons_owned), expected });
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
    // wrapper's WERK_TEST_DESTRUCTIVE_SUITES names them; same list, same seam.
    // #4135 — test-product-membrane.sh was excluded here as destructive (it
    // boots every com.chorus.* agent). Since #4131 it is a freshness VERDICT
    // under the nightly and refuses attended runs without an explicit grant,
    // so it is safe to plan; excluding it left a registered row no lane could
    // ever emit (LANE SILENT on the reconcile every night). Default: none.
    let destructive: Vec<String> = std::env::var("WERK_TEST_DESTRUCTIVE_SUITES")
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
    // #4540 (Jeff 10-16 B) — the queue runs every type but perf; perf runs
    // alone after it as its own step. The step names which side it is.
    let stages: Vec<werk_test::StagePlan> = match std::env::var(werk_test::STAGES_ENV).ok().as_deref() {
        Some(pick) => stages.into_iter().filter(|sp| werk_test::stage_picked(pick, sp.stage)).collect(),
        None => stages,
    };
    let legs_noop = std::env::var("WERK_TEST_LEGS_NOOP").is_ok();
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
    let cap: f64 = std::env::var("WERK_TEST_LOAD_CAP").ok().and_then(|v| v.parse().ok())
        .unwrap_or_else(|| werk_test::load_cap(budget));
    let gate_wait = std::time::Duration::from_secs(
        std::env::var("WERK_TEST_GATE_MAX_WAIT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(180));
    let gate_tick = std::time::Duration::from_secs(5);
    println!("-- #4022 load-aware: budget {} cores, cap load {:.0}, cargo {}×{} threads, npm {}×{} jest workers, bats {} --",
        budget, cap, cw_default, nextest_threads, nw_default, jest_workers, bats_default);
    let cargo_workers: usize = std::env::var("WERK_TEST_CARGO_WORKERS").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(cw_default);
    if std::env::var("WERK_TEST_NEXTEST_THREADS").is_err() {
        std::env::set_var("WERK_TEST_NEXTEST_THREADS", nextest_threads.to_string());
    }
    let ns_bins_for = |c: &str| -> Vec<String> {
        if stack_down.is_some() {
            let crate_prefix = format!("{}/tests/", crate_dir(&root, c));
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
    let npm_workers: usize = std::env::var("WERK_TEST_NPM_WORKERS").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(nw_default);
    // (#4152: one isolation list for the npm and file lanes)
    let iso_conf = std::fs::read_to_string(
        Path::new(&root).join("platform/scripts/nightly-isolation.conf"))
        .unwrap_or_default();
    let explicit_iso: Vec<String> = iso_conf.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let bats_workers: usize = std::env::var("WERK_TEST_SUITE_WORKERS").ok()
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
    // #4022 reopen 2026-10-14 — ONE queue for every stage. Each stage used to
    // run its cargo, npm and file pools one after another and wait for the
    // last to drain: on 10-14 one suite or none ran for 3,134s of 6,961s.
    // Every job now goes on one queue in stage order; a lane never runs past
    // its width (the widths sum to one CPU budget), isolated jobs start only
    // when nothing else runs, stack-readers stay at 2. The report below still
    // prints stage by stage, so a red stays under its stage (Silas, 08:50).
    struct QueuePlan { npm: Vec<String>, npm_alone: Vec<String>, files: Vec<String>, readers: Vec<String>, mutators: Vec<String> }
    enum QueueOut {
        Cargo((bool, Vec<(String, String)>, usize, bool)),
        Npm((bool, Vec<CaseResult>)),
        Files((bool, Vec<(String, String)>, String)),
    }
    let queue_plans: Vec<QueuePlan> = stages.iter().map(|sp| {
        let npm_plan = werk_test::plan_parallel_units(&sp.npm,
            &|u| explicit_iso.iter().any(|e| e == u.split('#').next().unwrap_or(u)));
        let plan = werk_test::plan_parallel_units(&sp.files,
            &|u| werk_test::unit_is_isolated(u, &rows, &explicit_iso));
        let (readers, mutators) = werk_test::split_serialized(&plan.serialized, &explicit_iso);
        QueuePlan { npm: npm_plan.parallel, npm_alone: npm_plan.serialized, files: plan.parallel, readers, mutators }
    }).collect();
    let mut jobs: Vec<werk_test::QueueJob> = Vec::new();
    let job = |si: usize, tool: &str, item: &str, lane: &str, alone: bool| werk_test::QueueJob {
        key: format!("{}|{}|{}", si, tool, item), lane: lane.to_string(), alone };
    for (si, sp) in stages.iter().enumerate() {
        let qp = &queue_plans[si];
        jobs.extend(sp.cargo.iter().map(|c| job(si, "cargo", c, "cargo", false)));
        jobs.extend(qp.npm.iter().map(|p| job(si, "npm", p, "npm", false)));
        jobs.extend(qp.files.iter().map(|b| job(si, "files", b, "files", false)));
        jobs.extend(qp.readers.iter().map(|b| job(si, "files", b, "reader", false)));
    }
    for (si, qp) in queue_plans.iter().enumerate() {
        jobs.extend(qp.npm_alone.iter().map(|p| job(si, "npm", p, "alone", true)));
        jobs.extend(qp.mutators.iter().map(|b| job(si, "files", b, "alone", true)));
    }
    let cargo_root = root.clone();
    let run_cargo = |item: &str| werk_test::time_unit(item, || {
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
            let crate_dir = crate_dir(&root, c);
            // #4155 — each failed case's panic block, keyed the way the case
            // is stored; the crate's tail when nextest printed none
            let panics = werk_test::why::nextest_case_reasons(&text);
            let mut reasons: Vec<(String, String, String)> = Vec::new();
            let mut matched: Vec<CaseResult> = Vec::new();
            // #4454 reopen — the build target each case ran under, in case order
            let targets: Vec<String> = werk_test::parse_nextest_case_runs(&text).into_iter().map(|(_, t, _)| t).collect();
            let mut runs: Vec<(String, String)> = Vec::new();
            // #4685 — a new, not-yet-registered test file keeps its cases
            let on_disk = |fp: &str| Path::new(&cargo_root).join(fp).is_file();
            for (i, (path, result)) in cases.iter().enumerate() {
                let target = targets.get(i).map(String::as_str).unwrap_or("");
                match werk_test::resolve_cargo_case(path, target, &crate_dir, &rows, &row_names, on_disk) {
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
            // #4539 — nextest's filterset kept this crate's held cases out
            note_holds(&holds_here(&crate_dir));
            note_nightly_unmeasured(item);
            (ok, cases, ns_len, stem.is_none() && !typed.is_empty())
        });
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
            note_nightly_unmeasured(item);
            (ok, cases)
        });
    println!("-- #4022 one queue: {} jobs across {} stages · cargo {}, npm {}, files {}, stack-readers 2, isolated alone --",
        jobs.len(), stages.len(), cargo_workers, npm_workers, bats_workers);
    let (ran_list, gate_waits) = werk_test::run_queue_gated(&jobs,
        &[("cargo", cargo_workers), ("npm", npm_workers), ("files", bats_workers), ("reader", 2), ("alone", 1)],
        cap, read_loadavg, gate_wait, gate_tick, |j| {
            let mut parts = j.key.splitn(3, '|');
            let (_, tool, item) = (parts.next(), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            match tool {
                "cargo" => QueueOut::Cargo(run_cargo(item)),
                "npm" => QueueOut::Npm(run_pkg(item)),
                _ => QueueOut::Files(werk_test::time_unit(item, || {
                    let r = run_bats_stored(&root, item);
                    note_nightly_unmeasured(item);
                    r
                })),
            }
        });
    let mut ran: std::collections::HashMap<String, QueueOut> = ran_list.into_iter().collect();

    for (si, sp) in stages.iter().enumerate() {
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
        let cargo_results: Vec<(String, (bool, Vec<(String, String)>, usize, bool))> = sp.cargo.iter()
            .map(|c| match ran.remove(&format!("{}|cargo|{}", si, c)) {
                Some(QueueOut::Cargo(r)) => (c.clone(), r),
                _ => (c.clone(), (false, Vec::new(), 0, false)),
            }).collect();
        for (item, (ok, cases, ns_len, moved)) in cargo_results {
            let (kind, unit) = werk_test::stage_item_label(stage, "cargo", &item);
            werk_test::print_unit_time(&kind, &unit, &item);
            if let Some(why) = nightly_unmeasured(&item).filter(|_| ok) {
                println!("{}", werk_test::nightly_lane_line_unmeasured(&kind, &unit, &why));
                continue;
            }
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
        let npm_results: Vec<(String, (bool, Vec<CaseResult>))> = queue_plans[si].npm.iter()
            .chain(queue_plans[si].npm_alone.iter())
            .map(|p| match ran.remove(&format!("{}|npm|{}", si, p)) {
                Some(QueueOut::Npm(r)) => (p.clone(), r),
                _ => (p.clone(), (false, Vec::new())),
            }).collect();
        for (item, (ok, cases)) in npm_results {
            let p = item.split('#').next().unwrap_or(&item).to_string();
            let (kind, unit) = werk_test::stage_item_label(stage, npm_kind_of(&p), &item);
            werk_test::print_unit_time(&kind, &unit, &item);
            if let Some(why) = nightly_unmeasured(&item).filter(|_| ok) {
                println!("{}", werk_test::nightly_lane_line_unmeasured(&kind, &unit, &why));
                continue;
            }
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
        let qp = &queue_plans[si];
        if !sp.files.is_empty() {
            println!("-- #4022 parallel plan ({}): {} suites fan out across {} workers, {} stack-readers at 2, {} mutators alone --",
                stage, qp.files.len(), bats_workers, qp.readers.len(), qp.mutators.len());
        }
        let lane_results: Vec<(String, (bool, Vec<(String, String)>, String))> = qp.files.iter()
            .chain(qp.readers.iter()).chain(qp.mutators.iter())
            .map(|b| match ran.remove(&format!("{}|files|{}", si, b)) {
                Some(QueueOut::Files(r)) => (b.clone(), r),
                _ => (b.clone(), (false, Vec::new(), "the queue returned no result for this suite".to_string())),
            }).collect();
        for (b, (ok, cases, text)) in lane_results {
            let b = &b;
            let kind = bats_kind(b);
            werk_test::print_unit_time(kind, b, b);
            if let Some(why) = nightly_unmeasured(b).filter(|_| ok) {
                println!("{}", werk_test::nightly_lane_line_unmeasured(kind, b, &why));
                continue;
            }
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
            let cmd = std::env::var("WERK_TEST_PLAYWRIGHT_CMD")
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
    println!("-- #4022 load gate: held {} time(s) at load > {:.0} (one queue) --", gate_waits, cap);
    // #4139 — the measured integration line: what ran, after it ran
    if stack_down.is_none() && ns_total > 0 {
        println!("{}", werk_test::integration_measured_report(ns_total, executed_ns.load(Ordering::SeqCst)));
    }
    let total_units = crates.len() + ts_pkgs.len() + bats_suites.len();

    let outcome = gate_outcome(total_units, any_failed);
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
    // #4144 AC3 — every queued unit post lands before the run counts what it stored
    drop(post_tx);
    let poster_wait = std::time::Instant::now();
    stored_total.fetch_add(poster.join().unwrap_or(0), Ordering::SeqCst);
    println!("nightly: poster drained in {} ms after the last unit (#4144)", poster_wait.elapsed().as_millis());
    let writeback_started = std::time::Instant::now();
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
    if let Some(line) = werk_test::storage_line(expected, stored) {
        println!("werk-test: {}", line);
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
    let bar_secs: u64 = std::env::var("WERK_TEST_ELAPSED_BAR_SECS").ok()
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
    // #4536 — storage never changes the verdict; its line printed above.
    let v = werk_test::verdict(expected, any_failed, &[]);
    let exit = v.exit_code();
    println!("werk-test: {} (exit {})", v.label(), exit);
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
            // #4539 — the card run's rule: a held case does not run
            let held = holds_here(suite);
            let names = std::fs::read_to_string(format!("{}/{}", werk, suite))
                .map(|t| werk_test::bats_case_names(&t)).unwrap_or_default();
            match werk_test::bats_keep(&names, &held) {
                werk_test::BatsKeep::All => {}
                werk_test::BatsKeep::Only(pat) => {
                    c.arg("--filter").arg(pat);
                    note_holds(&held);
                }
                werk_test::BatsKeep::Nothing => {
                    note_holds(&held);
                    return (true, Vec::new(), format!("every case in {} quarantined: not run", suite));
                }
            }
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
    // env-prefixes WERK_TEST_SUITE_TIMEOUT=<lane cap, 7200s> onto the werk-test
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
        .map(|fin| {
            report_strays(suite, &fin);
            (fin.code, fin.success, fin.timed_out)
        });
    match outcome {
        Ok((code, success, timed_out)) => {
            let mut text = std::fs::read_to_string(&out_path).unwrap_or_default();
            let _ = std::fs::remove_file(&out_path);
            if timed_out {
                text.push_str(&format!(
                    "\nSUITE TIMED OUT after {}s — killed by the runner (deadline is child-exit, not pipe-EOF)\n",
                    timeout_secs));
                eprintln!("!! {} timed out after {}s — killed", suite, timeout_secs);
                // #4537 — killed at the cap: UNMEASURED unless a case already failed
                let cases = werk_test::parse_file_suite_cases(suite, &text);
                if cases.iter().any(|(_, r)| r == "fail") {
                    return (false, cases, text);
                }
                return (werk_test::lane_unmeasured(format!("killed at {}s", timeout_secs)), cases, text);
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
            let rel = crate_dir(werk, c);
            let dir = format!("{}/{}", werk, rel);
            if !Path::new(&format!("{}/Cargo.toml", dir)).exists() {
                return Err(format!("no crate at {}", rel));
            }
            println!("   build: cargo build --release in {} (a selected suite runs it)", rel);
            let ok = Command::new("cargo").args(["build", "--release", "--quiet"]).current_dir(&dir)
                .status().map(|s| s.success()).unwrap_or(false);
            if ok { Ok(()) } else { Err(format!("cargo build --release failed in {}", rel)) }
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

/// #4393 — the card's diff of one suite (from where the werk left main) is
/// assertion lines only.
fn assert_only_in_card(werk: &str, suite: &str) -> bool {
    let base = Command::new("git").args(["-C", werk, "merge-base", "origin/main", "HEAD"]).output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    if base.is_empty() {
        return false;
    }
    let diff = Command::new("git").args(["-C", werk, "diff", "-U0", &base, "--", suite]).output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    werk_test::assert_only_change(&diff)
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
        // #4393 rule 1 — an edit to assertion lines only runs the suite's
        // hermetic cases: a case tagged needs-stack stands up what the edit
        // cannot have changed
        if assert_only_in_card(werk, suite) {
            println!("   bats:{} — assert-only edit: hermetic cases only (--filter-tags '!needs-stack')", suite);
            cmd.args(["--filter-tags", "!needs-stack"]);
        }
        // #4539 — bats has no exclude: the filter names the cases that run
        let held = holds_here(suite);
        let names = std::fs::read_to_string(format!("{}/{}", werk, suite))
            .map(|t| werk_test::bats_case_names(&t)).unwrap_or_default();
        match werk_test::bats_keep(&names, &held) {
            werk_test::BatsKeep::All => {}
            werk_test::BatsKeep::Only(pat) => {
                cmd.arg("--filter").arg(pat);
                note_holds(&held);
            }
            werk_test::BatsKeep::Nothing => {
                println!("   bats:{} — every case quarantined: not run", suite);
                note_holds(&held);
                return (BatsOutcome::Pass, Vec::new());
            }
        }
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
    // #4537 — a case the membrane stopped from writing prod measured nothing:
    // UNMEASURED and named, unless another case failed for a real reason
    let (refused, other_failed) = werk_test::membrane_refused_cases(&text);
    let outcome = if outcome == BatsOutcome::Fail && !refused.is_empty() && other_failed == 0 {
        werk_test::lane_unmeasured(format!("test tried to write prod: {}", refused.join(", ")));
        BatsOutcome::Unmeasured
    } else {
        outcome
    };
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
            TestUnit::RustCrate(c) => (format!("{}/tests", crate_dir(werk, c)), ".rs"),
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
    // night. That refusal reads $WERK_TEST_UNIT_TIMEOUT / $WERK_TEST_NIGHTLY, and
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
    cmd.env("WERK_TEST_UNIT_TIMEOUT", werk_test::unit_timeout().as_secs().to_string());
    // #4474 run 19 — every suite is rooted in the tree under test. v1 pinned
    // this from outside (`CHORUS_ROOT="$WERK" werk-test`, #3701); under werk v2
    // the scheduler's CHORUS_ROOT is canonical, so a jest suite read main's
    // data and failed the card's own rename. bats had the pin
    // (#4392); jest and cargo did not.
    cmd.env("CHORUS_ROOT", werk);
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
    // #4785 — werk-test runner env: export CHORUS_STATE_DIR (same source as sessions, #4535)
    // and CHORUS_HOOKS_RUN_DIR so child test suites reach the shared daemon socket under run-agents/.
    let state_dir = std::env::var("CHORUS_STATE_DIR").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
        let home = std::env::var("CHORUS_HOME").unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
        if let Some(user) = home.strip_prefix("/Users/").and_then(|rest| rest.split('/').next()) {
            format!("/Users/{user}/.chorus")
        } else {
            format!("{}/.chorus", std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string()))
        }
    });
    cmd.env("CHORUS_STATE_DIR", &state_dir);
    let hooks_run_dir = std::env::var("CHORUS_HOOKS_RUN_DIR").ok().filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("{state_dir}/run-agents"));
    cmd.env("CHORUS_HOOKS_RUN_DIR", &hooks_run_dir);
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
            // #4537 — no cargo at all: the build cannot start
            Err(e) => Err(format!("build did not start: {}", werk_test::did_not_start("cargo", &e))),
        }
    })
}

/// #4160 — the nightly's staged cargo run: `only_bins` non-empty runs just
/// those test binaries (a typed binary in its own stage); otherwise the crate
/// runs minus `exclude_bins`. A unit run whose every binary moved to another
/// stage may have nothing left, which is a fact, not a failure.
/// #4155 — the output text comes back too, so each failed case's panic
/// block can be read as its reason.
fn run_cargo_sel(werk: &str, name: &str, quarantined: &[&str], exclude_bins: &[&str], only_bins: &[&str]) -> (bool, Vec<(String, String)>, String) {
    let dir = format!("{}/{}", werk, crate_dir(werk, name));
    if !Path::new(&format!("{}/Cargo.toml", dir)).is_file() {
        // #4537 — was a silent green
        return (werk_test::lane_unmeasured(format!("no Cargo.toml in {}", crate_dir(werk, name))), Vec::new(), String::new());
    }
    // #4494 — start from prod's build cache when this werk has none
    let canonical = std::env::var("CHORUS_HOME")
        .unwrap_or_else(|_| format!("{}/CascadeProjects/chorus", std::env::var("HOME").unwrap_or_default()));
    if Path::new(werk) != Path::new(&canonical) {
        println!("build-cache: {}", werk_test::seed_target(werk, &canonical, &crate_dir(werk, name)));
    }
    if let Err(reason) = nextest_gate(werk) {
        // #4537 — the toolchain, not the card: UNMEASURED, never a red with no failing case
        eprintln!("UNMEASURED cargo lane for {}: {}", name, reason);
        let why = if reason.starts_with("build did not start") { reason.clone() } else { format!("nextest {}", reason) };
        return (werk_test::lane_unmeasured(why), Vec::new(), format!("UNMEASURED cargo lane: {}", reason));
    }
    // #4537 — another build holding this crate's build lock: wait up to a cap,
    // then UNMEASURED naming the holder (never an hour of "Blocking waiting")
    if let Err(why) = wait_for_build_lock(&dir) {
        eprintln!("UNMEASURED cargo lane for {}: {}", name, why);
        return (werk_test::lane_unmeasured(why.clone()), Vec::new(), why);
    }
    // #4022 — each crate gets its share of the CPU budget (see lane_widths);
    // WERK_TEST_NEXTEST_THREADS is set by the nightly lane, absent for card runs.
    let threads = std::env::var("WERK_TEST_NEXTEST_THREADS").ok().and_then(|v| v.parse().ok());
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
    match output_with_progress(&mut cmd, name) {
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
        Err(e) => {
            let why = format!("build did not start: {}", werk_test::did_not_start("cargo", &e));
            (werk_test::lane_unmeasured(why.clone()), Vec::new(), why)
        }
    }
}

/// #4537 — the processes holding a crate's cargo build lock open, as
/// "pid N (command)". Empty when free or when it cannot be read.
fn build_lock_holders(crate_dir: &str) -> Vec<String> {
    let target = Command::new("cargo").args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(crate_dir).output().ok()
        .and_then(|o| werk_test::json_str_field(&String::from_utf8_lossy(&o.stdout), "target_directory"))
        .unwrap_or_else(|| format!("{}/target", crate_dir));
    let me = std::process::id().to_string();
    let mut out = Vec::new();
    for lock in [format!("{}/debug/.cargo-lock", target), format!("{}/release/.cargo-lock", target)] {
        if !Path::new(&lock).is_file() {
            continue;
        }
        let pids = Command::new("lsof").args(["-t", &lock]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        for pid in pids.split_whitespace().filter(|p| *p != me) {
            let cmd = Command::new("ps").args(["-o", "command=", "-p", pid]).output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().chars().take(80).collect::<String>()).unwrap_or_default();
            out.push(format!("pid {} ({})", pid, cmd));
        }
    }
    out
}

/// #4537 — wait while another process holds the build lock, up to
/// WERK_TEST_LOCK_CAP seconds (default 600); past it, the reason to report.
fn wait_for_build_lock(crate_dir: &str) -> Result<(), String> {
    let cap = std::env::var("WERK_TEST_LOCK_CAP").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(600);
    let start = std::time::Instant::now();
    loop {
        let holders = build_lock_holders(crate_dir);
        if holders.is_empty() {
            return Ok(());
        }
        if start.elapsed().as_secs() >= cap {
            return Err(format!("build did not start: lock held by {} after {}s", holders.join(", "), cap));
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// #4453 — who a card run's progress events belong to (role, card, trace);
/// set once when the run starts. Unset (the nightly), no progress is emitted.
static PROGRESS_CTX: std::sync::OnceLock<(String, String, String)> = std::sync::OnceLock::new();

/// #4453 — `cmd.output()`, but nextest's live lines are read as they come and
/// a `test.unit.progress` event goes on the run's trace every 15 seconds, so a
/// long crate shows cases passed and failed while it is still running.
fn output_with_progress(cmd: &mut Command, unit: &str) -> std::io::Result<std::process::Output> {
    use std::io::{BufRead, Read};
    let Some((role, card, trace)) = PROGRESS_CTX.get() else { return cmd.output() };
    let mut child = cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()?;
    let mut out = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        buf
    });
    let mut err_buf = Vec::new();
    let mut progress = werk_test::CargoProgress::default();
    let mut last = std::time::Instant::now();
    let mut lines = std::io::BufReader::new(child.stderr.take().expect("piped stderr"));
    let mut line = Vec::new();
    while lines.read_until(b'\n', &mut line).unwrap_or(0) > 0 {
        progress.feed(&String::from_utf8_lossy(&line));
        err_buf.extend_from_slice(&line);
        line.clear();
        if last.elapsed() >= std::time::Duration::from_secs(15) {
            last = std::time::Instant::now();
            emit_spine("test.unit.progress", role, card, trace,
                &[("check", "cargo-test"), ("unit", unit), ("passed", &progress.passed.to_string()),
                  ("failed", &progress.failed.to_string()),
                  ("total", &progress.total.map(|n| n.to_string()).unwrap_or_default()),
                  ("message", &format!("cargo-test {}: {}", unit, progress.line()))]);
        }
    }
    let status = child.wait()?;
    let stdout = reader.join().unwrap_or_default();
    Ok(std::process::Output { status, stdout, stderr: err_buf })
}

/// Fetch the quarantined test cases from the tests domain (athena-make `/tests`), via a
/// curl|jq subprocess so the verb stays zero-dep/std-only (ADR-032 §6, same pattern
/// as `emit_spine`). Best-effort: any failure (endpoint down, jq absent) yields an
/// EMPTY set — quarantine never blocks the gate from running, it only relaxes it.
/// Each row is `testName\treason\tuntil\tfilePath` (#4539 added the file). (Server-side `?quarantined=true` filtering
/// is a follow-on; today we pull and filter client-side.)
/// #4539 — today's date in Boston, `YYYY-MM-DD`, the form quarantineUntil uses.
/// Empty when `date` fails; then every hold reads expired and runs (loud, never
/// a silent skip).
fn today_iso() -> String {
    Command::new("date").env("TZ", "America/New_York").arg("+%Y-%m-%d").output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// #4539 — the run's active holds, set once in the card run so every lane
/// runner reads the same set without threading it through each signature.
static HOLDS: std::sync::OnceLock<Vec<Quarantined>> = std::sync::OnceLock::new();

/// #4539 — the active holds in one unit dir or file (empty outside a card run).
fn holds_here(unit_path: &str) -> Vec<&'static Quarantined> {
    HOLDS.get().map(|h| werk_test::holds_in(h, unit_path)).unwrap_or_default()
}

/// #4539 — each held case that did not run is a lane note: the card run and
/// the nightly both read it as UNMEASURED "quarantined until <date>".
fn note_holds(held: &[&Quarantined]) {
    for h in held {
        werk_test::lane_unmeasured(format!("quarantined until {}: {} ({})", h.until, h.case, h.reason));
    }
}

fn quarantined_cases() -> Vec<Quarantined> {
    let endpoint = std::env::var("OWL_API_TESTS")
        .unwrap_or_else(|_| "http://localhost:3360/tests?limit=10000".to_string());
    // #3766 — athena-make serves quarantined as the STRING "true" (SHACL string field),
    // so a boolean-only compare NEVER matched: the quarantine gate was vacuous from
    // the day it shipped ("quarantined: none" = type mismatch, not an empty set).
    // Found live 2026-08-06 by writing a quarantine row and running this exact jq.
    let jq = r#".data[] | select(.quarantined==true or .quarantined=="true") | [.testName,.quarantineReason,.quarantineUntil,.filePath] | @tsv"#;
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
    if !werk_test::tsc_applies(Path::new(&pkg_dir)) {
        eprintln!("-- tsc:{} has no tsconfig.json (JavaScript) — nothing to typecheck", pkg);
        return true;
    }
    if !ensure_ts_deps(werk, pkg) {
        // #4537 — deps the runner could not provide: UNMEASURED, not a red
        eprintln!("!! tsc:{} deps unavailable — UNMEASURED", pkg);
        return werk_test::lane_unmeasured(format!("deps unavailable for {}", pkg));
    }
    let tsc = format!("{}/node_modules/.bin/tsc", pkg_dir);
    if !Path::new(&tsc).exists() {
        // #4537 — was a silent green
        return werk_test::lane_unmeasured(format!("no tsc in {}", pkg));
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
        eprintln!("!! jest:{} deps unavailable — UNMEASURED", pkg);
        return (werk_test::lane_unmeasured(format!("deps unavailable for {}", pkg)), Vec::new(),
            vec![(String::new(), String::new(), format!("jest:{} deps unavailable", pkg))]);
    }
    let jest = format!("{}/node_modules/.bin/jest", pkg_dir);
    if !Path::new(&jest).exists() {
        // #3974 — a package without jest runs its OWN runner (agent-adapters:
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
    // #4539 — a held case does not run
    let held = holds_here(pkg);
    if let Some(pat) = werk_test::jest_hold_pattern(&held) {
        cmd.arg("--testNamePattern").arg(pat);
        note_holds(&held);
    }
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
                for l in werk_test::run_record::jest_failure_why(&stdout, pkg, &|f| rel_path(f, werk)) {
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
        // #4537 — killed at the cap with no case failed: UNMEASURED
        None => (werk_test::lane_unmeasured(format!("killed at {}s", werk_test::unit_timeout().as_secs())), Vec::new(),
            vec![(String::new(), String::new(), format!("jest:{} killed at its unit cap", pkg))]),
    }
}

/// #4128 — name what a unit left running: reaped strays and any that survived
/// the reap, one lane line each (`nightly-stray|<unit>|reaped|<what>`), so a
/// leak is reported the night it happens.
fn report_strays(unit: &str, fin: &werk_test::Finished) {
    for s in &fin.strays {
        println!("nightly-stray|{}|reaped|{}", unit, s);
        eprintln!("!! {} left a process running — reaped: {}", unit, s);
    }
    for s in &fin.unreaped {
        println!("nightly-stray|{}|STILL RUNNING|{}", unit, s);
        eprintln!("!! {} left a process the runner could not stop: {}", unit, s);
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
    report_strays(label, &fin);
    let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
    let mut stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    if fin.timed_out {
        let note = format!("!! {} killed after {}s — per-unit wall cap (WERK_TEST_UNIT_TIMEOUT, #4030)",
            label, timeout.as_secs());
        eprintln!("{}", note);
        stderr.push('\n');
        stderr.push_str(&note);
        return Some((false, stdout, stderr));
    }
    Some((fin.success, stdout, stderr))
}

/// #3974 — the non-jest package runner: `npm test` (agent-adapters = node:test).
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
    //   registered  platform/<pkg>/tests/word-cap.test.ts :: <case>
    //   stored      platform/<pkg>/                       :: <case>
    //
    // The reconcile census joins on (file, name), so nothing matched and 245
    // passing tests of the old MCP server (retired #4515) were counted as
    // "never ran" every night — about half the whole never-ran gap, and none of it real.
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
        eprintln!("!! jest:{} deps unavailable — UNMEASURED", pkg);
        return (werk_test::lane_unmeasured(format!("deps unavailable for {}", pkg)), Vec::new());
    }
    let jest = format!("{}/node_modules/.bin/jest", pkg_dir);
    let rel: Vec<String> = files
        .iter()
        .map(|f| f.strip_prefix(&format!("{}/", pkg)).unwrap_or(f).to_string())
        .collect();
    if !Path::new(&jest).exists() {
        // #4440 reopened — a package without jest (node:test, e.g. agent-adapters) ran
        // NOTHING here and returned pass: since #3912 (2026-08-17) a card never
        // ran the old MCP server's tests (#4420 run 12: 30 files selected, 0.0s). Its
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
    // #4539 — a held case does not run
    let held: Vec<&Quarantined> = holds_here(pkg).into_iter().filter(|h| files.contains(&h.file)).collect();
    if let Some(pat) = werk_test::jest_hold_pattern(&held) {
        cmd.arg("--testNamePattern").arg(pat);
        note_holds(&held);
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
                for l in werk_test::run_record::jest_failure_why(&stdout, pkg, &|f| rel_path(f, werk)) {
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
        return werk_test::lane_unmeasured("missing clippy-ratchet.sh"); // #4537 — was a silent green
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
        return werk_test::lane_unmeasured("missing doc-coherence-ratchet.test.sh"); // #4537 — was a silent green
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

/// #4537 — the nightly runs units on worker threads; a worker hands its lane
/// notes to the stage that prints the unit's line.
static WERK_TEST_UNMEASURED: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

fn note_nightly_unmeasured(item: &str) {
    let notes = werk_test::take_lane_unmeasured();
    if !notes.is_empty() {
        if let Ok(mut m) = WERK_TEST_UNMEASURED.lock() {
            m.push((item.to_string(), notes.join("; ")));
        }
    }
}

fn nightly_unmeasured(item: &str) -> Option<String> {
    WERK_TEST_UNMEASURED.lock().ok()?.iter().find(|(i, _)| i == item).map(|(_, r)| r.clone())
}

/// #4265 — read a bats suite's exit code as a three-way outcome. 0 pass,
/// 2 unmeasured, anything else fail. #4537: a suite that cannot be launched
/// is UNMEASURED "missing <runner>" — #4265's worry (a missing runner reading
/// as "nothing to measure") is answered because UNMEASURED blocks a go.
fn bats_outcome(cmd: &mut Command) -> BatsOutcome {
    let runner = cmd.get_program().to_string_lossy().rsplit('/').next().unwrap_or("bats").to_string();
    match cmd.status() {
        Err(e) => {
            werk_test::lane_unmeasured(werk_test::did_not_start(&runner, &e));
            BatsOutcome::Unmeasured
        }
        Ok(s) if s.success() => BatsOutcome::Pass,
        Ok(s) if s.code() == Some(2) => BatsOutcome::Unmeasured,
        // #4515 — rc=3 is a suite's SELF-REFUSAL (#4016/#4256): it declined to
        // run here (e.g. a suite that will not write into the live store).
        // The nightly lane already scores it skip; this card lane scored it FAIL,
        // so a suite doing the right thing was a red on every card that touched it.
        Ok(s) if s.code() == Some(3) => BatsOutcome::Unmeasured,
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
/// #4540 — the test types in the model's order (TestType rows, typeOrder),
/// read from the store at scope. Empty when the store does not answer or holds
/// no rows; the run then says so and keeps the built-in order.
pub(crate) fn fetch_type_order() -> Vec<String> {
    let endpoint = std::env::var("FUSEKI_QUERY").unwrap_or_else(|_| "http://localhost:3030/pods/query".into());
    let q = "PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> \
             SELECT ?label ?order WHERE { GRAPH ?g { ?t a chorus:TestType ; rdfs:label ?label ; chorus:typeOrder ?order } } ORDER BY ?order";
    let out = Command::new("curl")
        .args(["-sfS", "--max-time", "15", "-H", "Accept: text/tab-separated-values", "--data-urlencode"])
        .arg(format!("query={q}"))
        .arg(&endpoint)
        .output();
    let Ok(out) = out else { return Vec::new() };
    if !out.status.success() {
        return Vec::new();
    }
    werk_test::type_order_of_tsv(&String::from_utf8_lossy(&out.stdout))
}

/// #4540 — what scope says about the order, and the file it hands the steps.
pub(crate) fn note_type_order(dir: &werk_test::run_dir::RunDir) -> Result<(), String> {
    let names = fetch_type_order();
    match werk_test::order_from_model(&names) {
        Some(o) => println!("type order (from the model): {}", o.join(", ")),
        None => println!("type order: built-in — the model {} (TestType rows, typeOrder)",
            if names.is_empty() { "holds no rows" } else { "holds a partial or bad order" }),
    }
    dir.put("type-order", &names)
}

/// #4540 — the run step hands the scope's order to the queue it starts.
pub(crate) fn use_type_order(dir: &werk_test::run_dir::RunDir) {
    if let Some(names) = dir.get("type-order") {
        std::env::set_var(werk_test::TYPE_ORDER_ENV, names.join(","));
    }
}

/// #4540 — the registry read as jq TSV; None when the door or jq failed.
fn fetch_rows_tsv() -> Option<String> {
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
    let jq_filter = r#".data[] | .filePath as $f | (.testType // "") as $tt | (if ($tt | IN("ui","perf","security")) then "" elif $tt != "" then $tt else (.pyramidLayer // "") end) as $l | .testName as $n | .name as $e | .hermeticity as $h | (if ($tt | IN("ui","perf","security")) then $tt elif $tt != "" then "" else (.testConcern // "") end) as $tc | (.covers | if type=="array" then .[] else . end) as $c | [$f,$c,($l // ""),($n // ""),($e // ""),($h // ""),($tc // ""),(.result // ""),(.testNeeds // "")] | @tsv"#;
    // #4419 — read the registry a page at a time. One 10,000-row read took
    // 12.8 s (measured 10-02: 1,000 rows 1.5 s, linear) against this 10 s cap,
    // so a run sometimes got 0 rows and "0 tests" read as an answer. Pages of
    // 1,000 each fit the cap; the links.next cursor walks them.
    let body = match fetch_all_pages(&endpoint) {
        Some(b) => b,
        None => return None,
    };
    let mut jq = match Command::new("jq")
        .args(["-r", jq_filter])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return None,
    };
    if let Some(mut stdin) = jq.stdin.take() {
        use std::io::Write;
        if stdin.write_all(&body).is_err() {
            return None;
        }
    }
    let out = match jq.wait_with_output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return None,
    };
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// #4540 — rows, names and entities from the registry TSV; no TSV = the fallback plan.
fn rows_of_tsv(tsv: Option<&str>) -> (Vec<TestRow>, Vec<String>, Vec<String>, &'static str) {
    let Some(tsv) = tsv else { return (Vec::new(), Vec::new(), Vec::new(), plan_source_label(false, 0)) };
    let (rows, names, entities) = parse_rows_and_names(tsv);
    // #4536 — each row's current result, so only flipped cases are written
    let _ = PRIOR_RESULTS.set(entities.iter().cloned().zip(werk_test::parse_prior_results(tsv)).collect());
    let label = plan_source_label(true, rows.len());
    (rows, names, entities, label)
}

fn fetch_test_rows() -> (Vec<TestRow>, Vec<String>, Vec<String>, &'static str) {
    rows_of_tsv(fetch_rows_tsv().as_deref())
}

/// #3592 — shared token acquisition: $CHORUS_WRITE_TOKEN, else mint. Pulled out
/// of post_suite_run so TestResult posts reuse ONE token per run.
fn write_token(role: &str) -> Option<String> {
    std::env::var("CHORUS_WRITE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| mint_token(role))
}

/// #4536 — each Test row holds its latest result, updated in place; no
/// TestResult row is minted (1,028,337 of them on 10-14, ~6,500 per run).
/// Only cases whose result flipped against the row's prior are PATCHed (351 of
/// 9,705 on 10-15), as a JSON merge patch on the Test itself. Returns how many
/// of `joined` the store now holds correctly: unchanged + patched ok. Storage
/// is never the verdict (AC3); a refusal is counted and named on the spine.
fn store_latest_results(
    role: &str,
    card: &str,
    trace: &str,
    joined: &[(CaseResult, String, String)],
    run_epoch_ms: u128,
    reason_of: &dyn Fn(&CaseResult) -> String,
) -> usize {
    let started = std::time::Instant::now();
    let priors = PRIOR_RESULTS.get().cloned().unwrap_or_default();
    let (entities, results): (Vec<String>, Vec<String>) = priors.into_iter().unzip();
    let patches = werk_test::latest_patches(joined, &entities, &results, card, run_epoch_ms, reason_of);
    let unchanged = joined.len() - patches.len();
    let base = werk_test::test_collection_base(&std::env::var("OWL_API_TESTS")
        .unwrap_or_else(|_| "http://localhost:3360/tests".to_string()));
    let mut ok = 0usize;
    let mut first_fail: Option<String> = None;
    if !patches.is_empty() {
        match write_token(role) {
            Some(token) => {
                for (entity, body) in &patches {
                    let url = format!("{}/{}", base, entity);
                    let code = Command::new("curl")
                        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "10",
                               "-X", "PATCH", "-H", "Content-Type: application/merge-patch+json",
                               "-H", &format!("Authorization: Bearer {}", token),
                               "--data-binary", body, &url])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                        .unwrap_or_else(|_| "000".into());
                    if code.starts_with('2') {
                        ok += 1;
                    } else if first_fail.is_none() {
                        first_fail = Some(code);
                    }
                }
            }
            None => first_fail = Some("no-write-token".into()),
        }
    }
    let failed = patches.len() - ok;
    let (n_patch, n_ok, n_fail, n_same, ms) = (patches.len().to_string(), ok.to_string(),
        failed.to_string(), unchanged.to_string(), started.elapsed().as_millis().to_string());
    let mut extras: Vec<(&str, &str)> = vec![("flipped", &n_patch), ("patched", &n_ok),
        ("failed", &n_fail), ("unchanged", &n_same), ("duration_ms", &ms)];
    if let Some(c) = &first_fail {
        extras.push(("first_fail_http", c));
    }
    emit_spine("test.latest.stored", role, card, trace, &extras);
    unchanged + ok
}

// #4154 — `werk-test --reconcile` (registered minus executed) retired with the
// nightly census; the crawler keeps the registry current, the runner trusts it.

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
/// #4198 — one line per path: `scoped <units>`, `runs nothing (<why>)`, or
/// `REFUSED unmapped`. Exit 2 when any path is refused, so a check can gate on it.
fn scope_of(args: &[String]) -> i32 {
    let mut root = std::env::var("CHORUS_ROOT").unwrap_or_else(|_| ".".to_string());
    let mut paths: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--root" {
            if let Some(r) = it.next() {
                root = r.clone();
            }
        } else {
            paths.push(a.clone());
        }
    }
    let mut refused = 0;
    for p in &paths {
        let (units, reason) = diff_scoped_units_inner(&root, std::slice::from_ref(p));
        let verdict = match units {
            Some(u) if u.is_empty() => "runs nothing (no build or test input)".to_string(),
            Some(u) => format!("scoped {}", u.iter().map(|t| format!("{:?}", t)).collect::<Vec<_>>().join(", ")),
            None if unmapped_path(&reason).is_some() => {
                refused += 1;
                "REFUSED unmapped".to_string()
            }
            None => format!("full ({})", reason),
        };
        println!("{}\t{}", p, verdict);
    }
    if refused > 0 { 2 } else { 0 }
}

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
    // #4491 — every crate home (crate_dirs.rs), not only platform/services
    for (name, dir) in werk_test::crate_dirs(root) {
        units.push(ScopeUnit { name, dir });
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
                    } else if root.join(&u.dir).join("Cargo.toml").is_file() {
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
/// The ui-flows child's environment. #4235 run 1: under werk v2 the scheduler
/// sets no CHORUS_ROOT, so every spec that starts its own Clearing died on
/// "CHORUS_ROOT must be set" (7 red, none about the card). The flows are rooted
/// in the tree under test, as every other suite is (apply_suite_world, #4474).
fn ui_flows_env(werk: &str) -> Vec<(&'static str, String)> {
    vec![
        ("CHORUS_CONTEXT", String::new()), // #3918 — test child stays refusable
        ("CHORUS_ROOT", werk.to_string()),
        // #4454 — the specs' own events (fixtures, each case starting and ending)
        ("CHORUS_TEST_EVENTS", test_events_path(werk)),
    ]
}

#[cfg(test)]
mod ui_flows_env_4235 {
    #[test]
    fn the_flows_are_rooted_in_the_werk_under_test() {
        let env = super::ui_flows_env("/w/kade-4235");
        assert!(env.contains(&("CHORUS_ROOT", "/w/kade-4235".to_string())), "{env:?}");
        // NEGATIVE PROOF: without the pin the own-Clearing fixture refuses to start
        assert!(!super::ui_flows_env("/w/x").iter().any(|(k, v)| *k == "CHORUS_ROOT" && v.is_empty()));
        assert!(env.iter().any(|(k, v)| *k == "CHORUS_CONTEXT" && v.is_empty()));
    }
}

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
    for (k, v) in ui_flows_env(werk) {
        cmd.env(k, v);
    }
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
        // #4537 — the harness could not launch: UNMEASURED, not a red
        Err(e) => (None, format!(" (UNMEASURED — {})", werk_test::did_not_start("playwright", &e))),
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
        let (dir, s) = world("cage.sh", "case \"$CHORUS_PULSE_URL\" in *127.0.0.1:9*) ;; *) echo \"live nudge path: $CHORUS_PULSE_URL\"; exit 1 ;; esac\n\
[ \"$CHORUS_ROOT\" = \"$PWD\" ] || { echo \"root $CHORUS_ROOT is not the werk $PWD\"; exit 1; }\n\
echo '=== Results: 1 passed, 0 failed ==='\n");
        std::env::remove_var("CHORUS_PULSE_URL");
        let werk = std::fs::canonicalize(&dir).unwrap();
        assert!(matches!(run_bats(werk.to_str().unwrap(), &s).0, BatsOutcome::Pass));
    }

    /// #4474 — every runner, not only bats, hands the suite the werk as its root
    #[test]
    fn every_suite_child_is_rooted_in_the_werk_4474() {
        let mut cmd = Command::new("true");
        apply_suite_world(&mut cmd, "/tmp/some-werk");
        let root = cmd.get_envs().find(|(k, _)| *k == "CHORUS_ROOT").and_then(|(_, v)| v);
        assert_eq!(root, Some(std::ffi::OsStr::new("/tmp/some-werk")));
        // NEGATIVE PROOF — a scheduler's canonical root must not leak through
        let mut cmd2 = Command::new("true");
        cmd2.env("CHORUS_ROOT", "/canonical");
        apply_suite_world(&mut cmd2, "/tmp/other-werk");
        let root2 = cmd2.get_envs().find(|(k, _)| *k == "CHORUS_ROOT").and_then(|(_, v)| v);
        assert_eq!(root2, Some(std::ffi::OsStr::new("/tmp/other-werk")));
    }

    /// #4785 — werk-test runner exports CHORUS_STATE_DIR and CHORUS_HOOKS_RUN_DIR
    #[test]
    fn every_suite_child_inherits_state_dir_4785() {
        let mut cmd = Command::new("true");
        apply_suite_world(&mut cmd, "/tmp/some-werk");
        let state = cmd.get_envs().find(|(k, _)| *k == "CHORUS_STATE_DIR").and_then(|(_, v)| v);
        assert!(state.is_some(), "CHORUS_STATE_DIR must be exported");
        let hooks = cmd.get_envs().find(|(k, _)| *k == "CHORUS_HOOKS_RUN_DIR").and_then(|(_, v)| v);
        assert!(hooks.is_some(), "CHORUS_HOOKS_RUN_DIR must be exported");

        // NEGATIVE PROOF — explicit override in parent is preserved
        let mut cmd2 = Command::new("true");
        std::env::set_var("CHORUS_HOOKS_RUN_DIR", "/custom/run-agents");
        apply_suite_world(&mut cmd2, "/tmp/some-werk");
        std::env::remove_var("CHORUS_HOOKS_RUN_DIR");
        let hooks2 = cmd2.get_envs().find(|(k, _)| *k == "CHORUS_HOOKS_RUN_DIR").and_then(|(_, v)| v);
        assert_eq!(hooks2, Some(std::ffi::OsStr::new("/custom/run-agents")));
    }

    /// NEGATIVE PROOF — the same suite run with the live nudge path fails, so
    /// the check above can go red.
    #[test]
    fn negative_proof_a_live_nudge_path_fails_the_cage_suite() {
        let (dir, s) = world("cage2.sh", "case \"$CHORUS_PULSE_URL\" in *127.0.0.1:9*) ;; *) exit 1 ;; esac\n");
        let out = Command::new("bash").arg(&s).current_dir(&dir)
            .env("CHORUS_PULSE_URL", "http://127.0.0.1:3475/api/nudge").status().unwrap();
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
        std::env::set_var("WERK_TEST_SUITE_TIMEOUT", "1");
        std::env::remove_var("WERK_TEST_UNIT_TIMEOUT");
        let (dir, s) = world("slowish.sh",
            "sleep 3\necho '=== Results: 1 passed, 0 failed ==='\n");
        let (ok, _, text) = run_bats_cases(dir.to_str().unwrap(), &s);
        assert!(ok, "a suite inside the UNIT cap must survive a lane-cap leak: {}", text);
        // Leg 2 — the UNIT cap (2s) kills a hung suite, loud (#4022 AC4's half):
        // a suite that never exits breaches its budget, dies, reads as a fail.
        std::env::set_var("WERK_TEST_UNIT_TIMEOUT", "2");
        let (dir, s) = world("hung.sh", "echo started\nsleep 300\n");
        let t0 = std::time::Instant::now();
        let _ = werk_test::take_lane_unmeasured();
        let (ok, _, text) = run_bats_cases(dir.to_str().unwrap(), &s);
        // #4537 — killed at the cap with no case failed: UNMEASURED, not red
        assert!(ok, "{}", text);
        assert_eq!(werk_test::take_lane_unmeasured(), vec!["killed at 2s".to_string()]);
        assert!(t0.elapsed().as_secs() < 30);
        assert!(text.contains("SUITE TIMED OUT"));
        // NEGATIVE PROOF: a case that already failed before the cap stays red
        let (dir, s) = world("failed-then-hung.sh",
            "echo 'not ok 1 the product broke'\nsleep 300\n");
        let (ok, cases, text) = run_bats_cases(dir.to_str().unwrap(), &s);
        std::env::remove_var("WERK_TEST_UNIT_TIMEOUT");
        std::env::remove_var("WERK_TEST_SUITE_TIMEOUT");
        assert!(cases.iter().any(|(_, r)| r == "fail"), "{:?} {}", cases, text);
        assert!(!ok, "a failed case before the cap must stay red: {}", text);
        assert!(werk_test::take_lane_unmeasured().is_empty());
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
    fn exit_two_and_three_are_unmeasured_and_nothing_else_is() {
        assert!(exiting(0) == BatsOutcome::Pass);
        assert!(exiting(2) == BatsOutcome::Unmeasured);
        // #4515 — rc=3 self-refusal, same verdict the nightly lane gives it
        assert!(exiting(3) == BatsOutcome::Unmeasured);

        // NEGATIVE PROOF (#3734): the codes this must NOT swallow. A suite that
        // fails stays FAIL — otherwise "unmeasured" becomes a way to make a red
        // disappear, which is the exact thing the hold exists to avoid.
        assert!(exiting(1) == BatsOutcome::Fail);
        assert!(exiting(70) == BatsOutcome::Fail);
        // #4537 — a runner that cannot even be launched is UNMEASURED and says
        // which runner (it blocks a go; it is not the product breaking)
        let _ = werk_test::take_lane_unmeasured();
        assert!(bats_outcome(&mut Command::new("/nonexistent/bats")) == BatsOutcome::Unmeasured);
        assert_eq!(werk_test::take_lane_unmeasured(), vec!["missing bats".to_string()]);
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
    let candidates = werk_test::card_candidates(changed, rows);
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
    // #4499 — from the one code-unit generator (the edges the crawler writes),
    // not a grep of crate sources here
    let mut crates: Vec<String> = werk_test::code_units::units_in(Path::new(werk)).unwrap_or_default().into_iter()
        .filter(|u| u.includes.iter().any(|i| i == f)).map(|u| u.path).collect();
    crates.sort();
    crates.dedup();
    crates
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
    // #4491 — any crate home; the manifest is checked in the tree under test
    // (platform/services keeps its old answer with or without a tree to look in)
    let root = std::env::var("WERK_TEST_TREE_ROOT").unwrap_or_default();
    werk_test::crate_dir_of_path(path, &|d: &str| {
        d.starts_with("platform/services/") || Path::new(&root).join(d).join("Cargo.toml").is_file()
    })
}

/// #4491 — the repo-relative dir of a crate named by its package name.
fn crate_dir(werk: &str, name: &str) -> String {
    werk_test::crate_rel_dir(Path::new(werk), name)
}

/// #4419 — the unit that runs a selected test file (or a whole package path).
fn unit_for_test(path: &str) -> Option<TestUnit> {
    // #4440 reopened — a shell suite (`platform/scripts/test-*.sh`, `*.test.sh`)
    // is run by the bats lane's runner (suite_runner picks bash); without this
    // a changed shell test was selected and then ran nowhere: 0 units, exit 0.
    if werk_test::is_bats_suite(path) || werk_test::is_shell_suite(path) {
        return Some(TestUnit::BatsSuite(path.to_string()));
    }
    if let Some(dir) = crate_of(path) {
        let root = std::env::var("WERK_TEST_TREE_ROOT").unwrap_or_default();
        let name = std::fs::read_to_string(Path::new(&root).join(&dir).join("Cargo.toml")).ok()
            .and_then(|t| werk_test::cargo_package_name(&t))
            .unwrap_or_else(|| dir.rsplit('/').next().unwrap_or(&dir).to_string());
        return Some(TestUnit::RustCrate(name));
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

#[cfg(test)]
mod silent_passes_4537 {
    use super::*;

    fn empty_werk(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("werk-test-4537-sp-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// #4537 — the 4 silent passes at main (Silas's map) each say UNMEASURED
    /// with their reason instead of a green that ran nothing.
    #[test]
    fn each_silent_pass_is_unmeasured_with_its_reason() {
        let w = empty_werk("missing");
        let ws = w.to_str().unwrap();
        let _ = werk_test::take_lane_unmeasured();
        assert!(run_clippy_ratchet(ws));
        assert!(run_doc_coherence(ws));
        let (ok, cases, _) = run_cargo_sel(ws, "platform/services/gone", &[], &[], &[]);
        assert!(ok && cases.is_empty());
        let notes = werk_test::take_lane_unmeasured();
        assert_eq!(notes[0], "missing clippy-ratchet.sh");
        assert_eq!(notes[1], "missing doc-coherence-ratchet.test.sh");
        assert!(notes[2].starts_with("no Cargo.toml in "), "{:?}", notes);
    }

    /// NEGATIVE PROOF: with the scripts present and passing, nothing is noted.
    #[test]
    fn negative_proof_present_scripts_note_nothing() {
        let w = empty_werk("present");
        for g in ["platform/scripts/clippy-ratchet.sh", "platform/tests/doc-coherence-ratchet.test.sh"] {
            let p = w.join(g);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
        }
        let ws = w.to_str().unwrap();
        let _ = werk_test::take_lane_unmeasured();
        assert!(run_clippy_ratchet(ws));
        assert!(run_doc_coherence(ws));
        assert!(werk_test::take_lane_unmeasured().is_empty());
    }
}
