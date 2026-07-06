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
    affected_units, cargo_skip_args, check_plan, gate_outcome, is_self_modifying,
    parse_quarantine_rows, quarantine_report, spine_args, CheckKind, Quarantined, TestUnit,
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
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let card = positional
        .first()
        .map(|s| s.to_string())
        .ok_or("usage: werk-test <card_id> <role>")?;
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
    let units = affected_units(&changed);
    let self_mod = is_self_modifying(&changed);
    let plan = check_plan(&units);

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
    emit_spine(
        "test.started",
        &role,
        &card,
        &trace,
        &[
            ("units", &units.len().to_string()),
            ("checks_planned", &plan.len().to_string()),
        ],
    );
    let mut any_failed = false;
    let mut failed_count: usize = 0;
    for check in &plan {
        let target = check.unit.as_ref().map(unit_name).unwrap_or("workspace");
        let ok = match (&check.kind, &check.unit) {
            (CheckKind::CargoTest, Some(TestUnit::RustCrate(c))) => run_cargo(&werk, c, &q_names),
            (CheckKind::Tsc, Some(TestUnit::TsPackage(p))) => run_tsc(&werk, p),
            (CheckKind::Jest, Some(TestUnit::TsPackage(p))) => run_jest(&werk, p),
            (CheckKind::ClippyRatchet, None) => run_clippy_ratchet(&werk),
            (CheckKind::DocCoherence, None) => run_doc_coherence(&werk),
            _ => true, // unreachable given check_plan's construction
        };
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
    println!("werk-test: {} (exit {})", outcome.label(), outcome.exit_code());
    Ok(outcome.exit_code())
}

fn unit_name(u: &TestUnit) -> &str {
    match u {
        TestUnit::RustCrate(n) => n,
        TestUnit::TsPackage(p) => p,
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
/// gate; an empty quarantine set leaves the invocation byte-identical.
fn run_cargo(werk: &str, name: &str, quarantined: &[&str]) -> bool {
    let dir = format!("{}/platform/services/{}", werk, name);
    if !Path::new(&format!("{}/Cargo.toml", dir)).is_file() {
        return true;
    }
    let mut args: Vec<String> = vec!["test".into(), "--lib".into(), "--bins".into()];
    args.extend(cargo_skip_args(quarantined));
    status_ok(Command::new("cargo").args(&args).current_dir(&dir))
}

/// Fetch the quarantined test cases from the tests domain (owl-api `/tests`), via a
/// curl|jq subprocess so the verb stays zero-dep/std-only (ADR-032 §6, same pattern
/// as `emit_spine`). Best-effort: any failure (endpoint down, jq absent) yields an
/// EMPTY set — quarantine never blocks the gate from running, it only relaxes it.
/// Each row is `testName\treason\tuntil`. (Server-side `?quarantined=true` filtering
/// is a follow-on; today we pull and filter client-side.)
fn quarantined_cases() -> Vec<Quarantined> {
    let endpoint = std::env::var("OWL_API_TESTS")
        .unwrap_or_else(|_| "http://localhost:3360/tests?limit=10000".to_string());
    let jq = r#".data[] | select(.quarantined==true) | [.testName,.quarantineReason,.quarantineUntil] | @tsv"#;
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
    status_ok(Command::new(&tsc).arg("--noEmit").current_dir(&pkg_dir))
}

/// `jest --ci` per TS package, deps guarded the same way (#3397).
fn run_jest(werk: &str, pkg: &str) -> bool {
    let pkg_dir = format!("{}/{}", werk, pkg);
    if !ensure_ts_deps(werk, pkg) {
        eprintln!("!! jest:{} CHANGED but deps unavailable — FAIL LOUD", pkg);
        return false;
    }
    let jest = format!("{}/node_modules/.bin/jest", pkg_dir);
    if !Path::new(&jest).exists() {
        return true;
    }
    status_ok(
        Command::new(&jest)
            .args(["--ci", "--forceExit", "--passWithNoTests"])
            .current_dir(&pkg_dir),
    )
}

/// `clippy-ratchet.sh` — workspace-wide per-lint ratchet (counts only decrease).
fn run_clippy_ratchet(werk: &str) -> bool {
    let script = format!("{}/platform/scripts/clippy-ratchet.sh", werk);
    if !Path::new(&script).is_file() {
        return true;
    }
    status_ok(Command::new("bash").arg(&script).current_dir(werk))
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
