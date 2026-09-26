//! athena-validate — the post-write conformance sweep over the live graph.
//!
//! #4167: Rust, like its three athena-* siblings. Jeff asked for this on
//! 2026-09-13; the bash it replaces is platform/scripts/athena-validate.sh.
//!
//! Jeff, 2026-09-19: "a job we keep chipping away at until it runs clean."
//! Exit codes serve that: 0 clean, 1 dirty, 2 UNMEASURED. The third one exists
//! because a sweep that could not run must never be reported as a healthy graph.
mod checks;
mod door;
mod ported;
mod registry;
mod store;

use checks::{Verdict, COMPLETENESS, SURVIVES_THE_DOOR};
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

fn main() {
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
    report.line(format!(
        "graph-scope|{}|{}",
        scope,
        if scope == "urn:chorus:" { "every chorus graph" } else { "SCOPED — not the whole store" }
    ));

    // The store checks.
    let mut store_checks = ported::all();
    store_checks.push(&COMPLETENESS);
    for check in store_checks {
        let (verdict, findings) = store::run(check);
        for f in &findings {
            report.line(format!("graph-issue|{}|{}|{}", f.check, f.subject, f.detail));
        }
        match verdict {
            Verdict::Found(n) => total += n,
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
                for f in &findings {
                    report.line(format!("graph-issue|{}|{}|{}", f.check, f.subject, f.detail));
                }
                match verdict {
                    Verdict::Found(n) => total += n,
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
