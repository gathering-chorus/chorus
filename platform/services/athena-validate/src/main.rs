//! athena-validate — the post-write conformance sweep over the live graph.
//!
//! #4167: Rust, like its three athena-* siblings. Jeff asked for this on
//! 2026-09-13; the bash it replaces is platform/scripts/athena-validate.sh.
//!
//! Jeff, 2026-09-19: "a job we keep chipping away at until it runs clean."
//! Exit codes serve that: 0 clean, 1 dirty, 2 UNMEASURED. The third one exists
//! because a sweep that could not run must never be reported as a healthy graph.
mod card;
mod checks;
mod door;
mod hierarchy;
mod migration;
mod ported;
mod registry;
mod store;

use checks::{Verdict, ALLOWED_VALUES, COMPLETENESS, DOMAIN_CLAIMS_VOCABULARY, EXERCISES_FROM_A_TEST, PATTERN, SURVIVES_THE_DOOR, TOOL_NAME};
use std::io::Write;

/// Every line the run produced, printed AND written to the report file.
///
/// #4167 AC3 — /borg/graph-validate.html reads ATHENA_VALIDATE_REPORT and is not
/// modified by this card, so the format is a contract: one graph-issue line per
/// violation, then exactly one graph-summary. The bash wrote it; if the Rust
/// only printed to stdout, the page would go blank the day the schedule flipped
/// and nothing would say why.
struct Report {
    lines: Vec<String>,
}

impl Report {
    fn new() -> Self {
        Report { lines: Vec::new() }
    }
    fn line(&mut self, s: String) {
        println!("{s}");
        self.lines.push(s);
    }
    /// Write the file, or say loudly that it could not be written. A report that
    /// silently fails to land is the page going stale with nobody told.
    fn flush(&self) {
        let Ok(path) = std::env::var("ATHENA_VALIDATE_REPORT") else { return };
        match std::fs::File::create(&path) {
            Ok(mut f) => {
                for l in &self.lines {
                    let _ = writeln!(f, "{l}");
                }
                let _ = f.flush();
            }
            Err(e) => eprintln!("athena-validate: could not write {path}: {e}"),
        }
    }
}

/// Say what the run found, on the spine.
///
/// #4167 AC4 — the bash emitted graph.validate.* and #4166 added it precisely
/// because "the answer lives in a log nobody opens, which is how this sweep went
/// unread for weeks". A rewrite that drops the emit makes the sweep silent again
/// while every test still passes.
///
/// Best effort by design: a missing chorus-log must not fail the sweep, because
/// the sweep's answer about the graph is still true when the logger is absent.
fn emit_spine(event: &str, fields: &[String]) {
    let home = std::env::var("CHORUS_HOME")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
    // ATHENA_VALIDATE_NUDGE=0 is what every test run sets, and it means "this is
    // not a real run — do not reach a person". The spine is reaching a person:
    // six membrane.violation events fired at 22:09 while the suites were green,
    // because the emit I added writes a production surface from a test context.
    // The membrane was right. Honour the same switch the bash used.
    if std::env::var("ATHENA_VALIDATE_NUDGE").as_deref() == Ok("0") {
        return;
    }
    let log = format!("{home}/platform/scripts/chorus-log");
    if !std::path::Path::new(&log).is_file() {
        return;
    }
    let role = std::env::var("DEPLOY_ROLE").unwrap_or_else(|_| "system".to_string());
    let _ = std::process::Command::new("bash")
        .arg(&log)
        .arg(event)
        .arg(role)
        .args(fields)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

// #4446 — a scheduled job logs its own failure.
#[allow(dead_code)] // each crate uses part of the shared helper
mod service_lifecycle {
    include!("../../shared/service_lifecycle.rs");
}

fn env_nonempty(key: &str, default: &str) -> String {
    std::env::var(key).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_string())
}

fn main() {
    // #4446 — under launchd, a failed run is logged as service.failed (shared/service_lifecycle.rs).
    service_lifecycle::run_as_job();
    let args: Vec<String> = std::env::args().collect();
    // #4536 — an argument this binary does not know refuses. Before, `--help`
    // fell through to a full sweep of prod: on 10-15 one ran 27 minutes under
    // load before anyone noticed.
    if let Err(bad) = known_args(&args[1..]) {
        eprintln!("athena-validate: unknown argument '{bad}'. Known: migration · --card <n> · --staging · --store-only · --baseline <file> · --write-baseline <file>");
        std::process::exit(2);
    }
    // #4536 — a card is red only on what it adds. `--baseline <file>` holds the
    // issue keys prod already has (written by `--write-baseline` on a prod
    // sweep); an issue whose key is in it is counted as known, never red.
    // `--staging` reads prod's baseline from its default home unless one is
    // named, so the generated pipeline step needs no new argument (Kade 10:46).
    let writing = args.iter().any(|a| a == "--write-baseline");
    let baseline_path = flag_value(&args, "--baseline").or_else(|| {
        (args.iter().any(|a| a == "--staging") && !writing).then(default_baseline_path)
    });
    // no trace and no override: there is no baseline to read, which is UNMEASURED
    let baseline_path = baseline_path.map(|p| if p.is_empty() { "(no CHORUS_TRACE_ID)".to_string() } else { p });
    // `--write-baseline` with no value writes this run's default path (the
    // pipeline's "before" step passes no argument)
    let write_baseline = flag_value(&args, "--write-baseline").or_else(|| {
        args.iter().any(|a| a == "--write-baseline").then(default_baseline_path).filter(|p| !p.is_empty())
    });
    if let Some(p) = &write_baseline {
        if let Some(dir) = std::path::Path::new(p).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
    // no baseline = cannot tell new from old: the sweep still runs and reports,
    // and the verdict is UNMEASURED with the reason, never a sweep-wide red
    let mut baseline_missing: Option<String> = None;
    let baseline: Option<std::collections::HashSet<String>> = match &baseline_path {
        None => None,
        Some(p) => match std::fs::read_to_string(p) {
            Ok(t) => Some(t.lines().filter(|l| !l.is_empty()).map(String::from).collect()),
            Err(e) => {
                baseline_missing = Some(format!("no baseline at {p}: {e}"));
                None
            }
        },
    };
    let mut known = 0usize;
    let mut keys: Vec<String> = Vec::new();
    // #4515 — `migration`: the V1→V2 per-domain stage matrix (was the MCP tool
    // chorus_migration_readout). Its own report, not the sweep.
    if args.get(1).map(String::as_str) == Some("migration") {
        std::process::exit(migration::run());
    }
    // #4515 — `--card <n>`: the Model step for one card's change, not the store sweep.
    if let Some(i) = args.iter().position(|a| a == "--card") {
        match args.get(i + 1).filter(|v| !v.starts_with("--")) {
            Some(n) => std::process::exit(card::run(n)),
            None => {
                eprintln!("athena-validate: --card needs a card number");
                std::process::exit(2);
            }
        }
    }
    // #4515 — `--staging`: the athena pipeline's Validate step sweeps the STAGING
    // dataset its Deploy step just loaded, never prod. Store and door both move,
    // the same way `athena-deploy staging` points its load and its served check:
    // $CHORUS_FUSEKI_BASE/$STAGING_DATASET/query and the staging athena-make
    // ($ATHENA_MAKE_STAGING_URL). The flag wins over an inherited FUSEKI_QUERY or
    // CHORUS_OWL_API: a prod endpoint in the scheduler's env must not turn a
    // staging check into a prod one.
    if args.iter().any(|a| a == "--staging") {
        let base = env_nonempty("CHORUS_FUSEKI_BASE", "http://localhost:3030");
        let stg = env_nonempty("STAGING_DATASET", "staging");
        let door = env_nonempty("ATHENA_MAKE_STAGING_URL", "http://localhost:3368");
        let query = format!("{}/{stg}/query", base.trim_end_matches('/'));
        std::env::set_var("FUSEKI_QUERY", &query);
        std::env::set_var("CHORUS_OWL_API", &door);
        println!("graph-target|staging|{query}|{door}");
    }
    //
    // --store-only: run the store checks and skip the door comparison.
    //
    // Not a convenience flag. A suite that stubs the STORE but not the door gets
    // an honest UNMEASURED from the door check and therefore exit 2, which looks
    // identical to a broken sweep. Naming the skip explicitly keeps "we did not
    // ask" separate from "we asked and could not tell" — the same distinction
    // this whole crate exists to hold.
    let store_only = std::env::args().any(|a| a == "--store-only");
    let mut report = Report::new();
    let mut total = 0usize;
    let mut unmeasured = 0usize;

    // #4239 — say what was looked at, first line, always. A clean run against one
    // graph and a clean run against the store are the same word otherwise, and the
    // second is the only one that means the graph is clean.
    let scope = store::graph_scope();
    // #4467 — exact graphs, the land's scope. A bad name stops the run as
    // UNMEASURED; an empty list says the store checks were not run, by name.
    let exact = match store::exact_graphs() {
        Ok(e) => e,
        Err(why) => {
            report.line(format!("graph-scope|?|{why}"));
            report.line("graph-summary|UNMEASURED|bad-scope".to_string());
            report.flush();
            std::process::exit(2);
        }
    };
    match &exact {
        Some(g) if g.is_empty() => report.line("graph-scope||SCOPED — no instance graph named, store checks not run".to_string()),
        Some(g) => report.line(format!("graph-scope|{}|SCOPED — these graphs only", g.join(" "))),
        None => report.line(format!(
            "graph-scope|{}|{}",
            scope,
            if scope == "urn:chorus:" { "every chorus graph" } else { "SCOPED — not the whole store" }
        )),
    }

    // The store checks.
    let mut store_checks = ported::all();
    store_checks.push(&COMPLETENESS);
    // #4358 — sh:in and sh:pattern, the two value constraints the DAL enforces
    // at the door, swept over what is already in the store.
    store_checks.push(&ALLOWED_VALUES);
    store_checks.push(&PATTERN);
    // #4499 — only a test says what it exercises
    store_checks.push(&EXERCISES_FROM_A_TEST);
    store_checks.push(&DOMAIN_CLAIMS_VOCABULARY);
    // #4515 — a tool is named for a Product row and linked to the step it runs
    store_checks.push(&TOOL_NAME);
    // #4481 — one parent per row, position read up the chain, no unlisted links across hierarchies
    store_checks.extend(hierarchy::all());
    if matches!(&exact, Some(g) if g.is_empty()) {
        store_checks.clear();
    }
    for check in store_checks {
        let (verdict, findings) = store::run(check);
        let found: Vec<String> = findings.iter().map(|f| format!("{}|{}|{}", f.check, f.subject, f.detail)).collect();
        let (fresh, old_ones) = split_by_baseline(&found, baseline.as_ref());
        known += old_ones;
        let new_here = fresh.len();
        for key in fresh {
            report.line(format!("graph-issue|{key}"));
        }
        keys.extend(found);
        match verdict {
            Verdict::Found(n) => total += if baseline.is_some() { new_here } else { n },
            Verdict::Clean => {}
            Verdict::Unmeasured(ref why) => {
                unmeasured += 1;
                report.line(format!("graph-issue|{}|UNMEASURED|{}", check.id, why));
            }
        }
    }

    // The door check: one sampled row per served collection, stored predicates
    // against served keys. A discovery it cannot read is unmeasured, never an
    // empty walk reporting clean.
    if store_only {
        // Say it out loud in the report: a skipped check is never a passed one.
        report.line(format!("graph-issue|{}|SKIPPED|--store-only", SURVIVES_THE_DOOR.id));
    } else {
    match registry::served_collections(&door::api_base()) {
        Err(why) => {
            unmeasured += 1;
            report.line(format!("graph-issue|{}|UNMEASURED|{}", SURVIVES_THE_DOOR.id, why));
        }
        Ok(served) => {
            for s in &served {
                let (verdict, findings) = door::check_collection(&s.collection);
                let found: Vec<String> = findings.iter().map(|f| format!("{}|{}|{}", f.check, f.subject, f.detail)).collect();
                let (fresh, old_ones) = split_by_baseline(&found, baseline.as_ref());
                known += old_ones;
                let new_here = fresh.len();
                for key in fresh {
                    report.line(format!("graph-issue|{key}"));
                }
                keys.extend(found);
                match verdict {
                    Verdict::Found(n) => total += if baseline.is_some() { new_here } else { n },
                    Verdict::Clean => {}
                    Verdict::Unmeasured(ref why) => {
                        unmeasured += 1;
                        report.line(format!("graph-issue|{}|UNMEASURED|{} {}", SURVIVES_THE_DOOR.id, s.kind, why));
                    }
                }
            }
        }
    }

    }

    if let Some(p) = &write_baseline {
        // a baseline from a run that could not measure everything would hide gaps
        if unmeasured == 0 {
            keys.sort();
            keys.dedup();
            if let Err(e) = std::fs::write(p, keys.join("\n") + "\n") {
                // #4536 (Silas 10:56) — the before step exits 0 only when its
                // baseline exists; a write that failed is UNMEASURED here, not
                // a missing file one step later
                report.line(format!("graph-baseline|NOT WRITTEN|{p}: {e}"));
                report.line(format!("graph-summary|UNMEASURED|baseline not written: {e}"));
                report.flush();
                emit_spine("graph.validate.unmeasured", &["reason=baseline-not-written".to_string()]);
                std::process::exit(2);
            } else {
                report.line(format!("graph-baseline|{}|written to {p}", keys.len()));
                // #4536 — the "before" step records what main already has; it
                // judges nothing, so a written baseline ends clean. Without
                // this the pipeline stopped on the 3,030 old issues it exists
                // to record. An unwritten one falls through to UNMEASURED/dirty.
                report.line(format!("graph-summary|baseline|{} issue(s) recorded, not judged", keys.len()));
                report.flush();
                emit_spine("graph.validate.baseline", &[format!("recorded={}", keys.len())]);
                return;
            }
        } else {
            report.line(format!("graph-baseline|NOT WRITTEN|{unmeasured} check(s) unmeasured"));
        }
    }
    if baseline.is_some() {
        report.line(format!("graph-known|{known}|already in prod's baseline, not this card's"));
    }
    if let Some(why) = &baseline_missing {
        report.line(format!("graph-summary|UNMEASURED|{why}"));
        report.flush();
        emit_spine("graph.validate.unmeasured", &["reason=no-baseline".to_string()]);
        std::process::exit(2);
    }
    if unmeasured > 0 {
        report.line("graph-summary|UNMEASURED|unreachable".to_string());
        report.flush();
        emit_spine("graph.validate.unmeasured", &[format!("checks_unmeasured={unmeasured}")]);
        std::process::exit(2);
    }
    if total > 0 {
        report.line(format!("graph-summary|{total}|dirty"));
        report.flush();
        emit_spine(
            "graph.validate.completed",
            &[format!("issues={total}"), "verdict=dirty".to_string()],
        );
        std::process::exit(1);
    }
    report.line("graph-summary|0|clean".to_string());
    report.flush();
    emit_spine(
        "graph.validate.completed",
        &["issues=0".to_string(), "verdict=clean".to_string()],
    );
}

/// #4536 — the issues a run reports as its own: those whose key is not in
/// prod's baseline. Returns (new keys, count already in the baseline). With no
/// baseline every issue is new — the old sweep-wide behaviour.
fn split_by_baseline(found: &[String], baseline: Option<&std::collections::HashSet<String>>) -> (Vec<String>, usize) {
    match baseline {
        None => (found.to_vec(), 0),
        Some(b) => {
            let fresh: Vec<String> = found.iter().filter(|k| !b.contains(*k)).cloned().collect();
            let known = found.len() - fresh.len();
            (fresh, known)
        }
    }
}

/// Where a run's "before" sweep leaves its baseline and the "after" sweep reads
/// it: one file per pipeline run, keyed on CHORUS_TRACE_ID (= DAG_RUN_ID in
/// every athena step), so two runs can never read each other's (Wren 10:53).
/// ATHENA_VALIDATE_BASELINE overrides; with neither, there is no default.
fn default_baseline_path() -> String {
    if let Some(p) = std::env::var("ATHENA_VALIDATE_BASELINE").ok().filter(|v| !v.is_empty()) {
        return p;
    }
    let trace = std::env::var("CHORUS_TRACE_ID").unwrap_or_default();
    let trace: String = trace.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    if trace.is_empty() {
        return String::new();
    }
    format!("{}/.chorus/state/athena-validate/{trace}.baseline", std::env::var("HOME").unwrap_or_default())
}

/// #4536 — every argument this binary takes. Anything else refuses.
fn known_args(args: &[String]) -> Result<(), String> {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "migration" if i == 0 => {}
            "--staging" | "--store-only" => {}
            "--card" | "--baseline" => i += 1,
            "--write-baseline" => {
                if args.get(i + 1).is_some_and(|v| !v.starts_with("--")) {
                    i += 1;
                }
            }
            other => return Err(other.to_string()),
        }
        i += 1;
    }
    Ok(())
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).filter(|v| !v.starts_with("--")).cloned()
}

#[cfg(test)]
mod args_4536 {
    use super::*;
    fn v(a: &[&str]) -> Vec<String> { a.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn known_flags_pass_and_help_refuses() {
        assert!(known_args(&v(&["--staging", "--baseline", "/tmp/b"])).is_ok());
        assert!(known_args(&v(&["--card", "4536"])).is_ok());
        assert!(known_args(&v(&["migration"])).is_ok());
        assert!(known_args(&v(&["--staging", "--write-baseline"])).is_ok(), "the before step passes no path");
        // NEGATIVE PROOF: the 10-15 27-minute prod sweep started from `--help`
        assert_eq!(known_args(&v(&["--help"])), Err("--help".to_string()));
        assert_eq!(known_args(&v(&["--staging", "--bogus"])), Err("--bogus".to_string()));
    }

    #[test]
    fn only_issues_missing_from_the_baseline_count_as_the_cards() {
        let found = v(&["v1-row|a|x", "v1-row|b|y", "dangling-edge|c|z"]);
        let base: std::collections::HashSet<String> = v(&["v1-row|a|x", "v1-row|b|y"]).into_iter().collect();
        let (fresh, known) = split_by_baseline(&found, Some(&base));
        assert_eq!((fresh, known), (v(&["dangling-edge|c|z"]), 2));
    }

    /// NEGATIVE PROOF: with a baseline present, an issue not in it is still the
    /// card's (red); and with no baseline every issue counts.
    #[test]
    fn negative_proof_a_baseline_never_hides_a_new_issue() {
        let base: std::collections::HashSet<String> = v(&["v1-row|a|x"]).into_iter().collect();
        let (fresh, _) = split_by_baseline(&v(&["v1-row|NEW|x"]), Some(&base));
        assert_eq!(fresh.len(), 1, "an issue the baseline does not hold must stay red");
        let (all, known) = split_by_baseline(&v(&["v1-row|a|x", "v1-row|b|y"]), None);
        assert_eq!((all.len(), known), (2, 0));
    }

    #[test]
    fn a_flag_value_is_read_and_a_missing_one_is_none() {
        assert_eq!(flag_value(&v(&["x", "--baseline", "/tmp/b"]), "--baseline"), Some("/tmp/b".to_string()));
        assert_eq!(flag_value(&v(&["x", "--baseline", "--staging"]), "--baseline"), None);
    }
}
