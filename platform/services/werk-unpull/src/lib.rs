//! werk-unpull — the /pull inverse as a verb (#3299, ADR-038 port of the TS
//! `chorus_unpull_card`; ADR-032 §1 blueprint: zero-dep std-only Rust, subprocess
//! CLIs at the absolute home path, spine emits, typed refusals).
//!
//! Reverses a pull WITHOUT losing work: validate (card is WIP + owned by the
//! caller) → werk pre-flight (exists, is a worktree, CLEAN — refuse-if-dirty is
//! the load-bearing guarantee) → `cards move <id> Next` → `chorus-werk remove`
//! (worktree + branch teardown) → best-effort `role-state idle` → emit
//! `card.unpulled`. Both mutate steps are idempotent (already-Next /
//! already-removed complete, not refuse) so a partial unpull re-runs to done.
//!
//! Refusal taxonomy (parity with the TS tool): card-not-found | wrong-status |
//! wrong-owner | werk-not-initialized | werk-corrupt | werk-dirty | move-fail |
//! branch-close-fail.

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

// #3513 — the ONE shared failure classifier (failure_class / fail_extra).
include!("../../shared/failure_class.rs");

pub type R<T> = Result<T, String>;

// --- pure helpers (unit-tested) ---

pub fn branch_name(role: &str, card: u64) -> String {
    format!("{}/{}", role, card)
}

/// #3299 — the CLI seam (the #3294 pattern). `--atomic` is recognized anywhere but
/// non-branching: unpull is in the ADR-037 atomic-FREE group (local, reversible —
/// the werk is recreatable from origin/main; the refuse-if-dirty guard protects
/// the only unrecoverable thing).
pub fn parse_unpull_args(args: &[String], deploy_role: Option<String>) -> R<(u64, String, bool)> {
    let atomic = args.iter().any(|a| a == "--atomic");
    let pos: Vec<&String> = args.iter().filter(|a| a.as_str() != "--atomic").collect();
    let card_arg = pos
        .first()
        .ok_or_else(|| "usage: werk-unpull <card> <role> [--atomic]".to_string())?;
    let card: u64 = card_arg
        .parse()
        .map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = pos
        .get(1)
        .map(|s| s.to_string())
        .or(deploy_role)
        .ok_or_else(|| "usage: werk-unpull <card> <role> [--atomic] (or set DEPLOY_ROLE)".to_string())?;
    Ok((card, role, atomic))
}

/// Validate the card's board state for unpull. Owner compares case-insensitively
/// (the board title-cases it: 'Kade'). Returns the typed refusal reason on miss.
pub fn validate_card(status: &str, owner: &str, role: &str, card: u64) -> Result<(), (String, String)> {
    if status != "WIP" {
        return Err((
            "wrong-status".to_string(),
            format!("card #{} is in '{}' — must be WIP", card, status),
        ));
    }
    if owner.to_lowercase() != role {
        return Err((
            "wrong-owner".to_string(),
            format!("card #{} is owned by '{}' — must be {}", card, owner, role),
        ));
    }
    Ok(())
}

/// Minimal JSON string-field extractor for `cards view --json` (status/owner) —
/// tolerant of whitespace, no JSON dep (zero-dep rule).
pub fn json_str_field(json: &str, field: &str) -> Option<String> {
    let key = format!("\"{}\"", field);
    let at = json.find(&key)?;
    let rest = &json[at + key.len()..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    rest[1..].find('"').map(|end| rest[1..1 + end].to_string())
}

pub fn jsonl_line(ts: u128, event: &str, role: &str, card: u64, trace: &str, extra: &str) -> String {
    format!(
        "{{\"ts\":{},\"event\":\"{}\",\"role\":\"{}\",\"card_id\":{},\"trace_id\":\"{}\"{}}}\n",
        ts, event, role, card, trace, extra
    )
}

pub fn spine_args(event: &str, role: &str, card: u64, trace: &str, extras: &[(&str, &str)]) -> Vec<String> {
    let mut v = vec![
        event.to_string(),
        role.to_string(),
        format!("card={}", card),
        format!("trace={}", trace),
    ];
    for (k, val) in extras {
        v.push(format!("{}={}", k, val));
    }
    v
}

fn trace_id() -> String {
    let ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}", ns, std::process::id())
}

/// #3162 inherit-not-mint: CHORUS_TRACE_ID env → /tmp/<card>-trace file → mint.
pub fn resolve_trace(card: u64) -> String {
    if let Ok(t) = env::var("CHORUS_TRACE_ID") {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    if let Ok(t) = std::fs::read_to_string(format!("/tmp/{}-trace", card)) {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    trace_id()
}

// --- side-effecting helpers ---

fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-unpull.jsonl");
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(jsonl_line(ts, event, role, card, trace, extra).as_bytes());
    }
}

fn emit_spine(home: &Path, event: &str, role: &str, card: u64, trace: &str, extras: &[(&str, &str)]) {
    let log = home.join("platform/scripts/chorus-log");
    if !log.exists() {
        return;
    }
    let mut c = Command::new("bash");
    c.arg(&log);
    for a in spine_args(event, role, card, trace, extras) {
        c.arg(a);
    }
    let _ = c.output();
}

fn script(home: &Path, name: &str) -> String {
    home.join("platform/scripts").join(name).to_string_lossy().to_string()
}

fn run(cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        return Err(format!(
            "{}: {}{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim(),
            if stdout.trim().is_empty() { String::new() } else { format!(" {}", stdout.trim()) }
        ));
    }
    Ok(stdout)
}

fn run_in(dir: &str, cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!("{} {}: {}", cmd, args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The whole verb, all inputs explicit (testable against fixture homes with
/// shimmed cards / chorus-werk / role-state / chorus-log). Returns the prior branch.
pub fn unpull(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = resolve_trace(card);
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));

    jsonl(home, role, card, &trace, "unpull.started", "");
    emit_spine(home, "unpull.started", role, card, &trace, &[]);

    let refuse = |step: &str, reason: &str, detail: String| -> String {
        jsonl(home, role, card, &trace, "unpull.refused",
            &format!(",\"step\":\"{}\",\"reason\":\"{}\",\"failureClass\":\"{}\"", step, reason, failure_class(reason)));
        emit_spine(home, "unpull.refused", role, card, &trace,
            &[("disposition", "refuse"), ("step", step), ("reason", reason), ("failureClass", failure_class(reason))]);
        format!("{}: {}", reason, detail.lines().next().unwrap_or(""))
    };

    // ── Step 1: validate — card exists, WIP, owned by the caller.
    let cj = run(&script(home, "cards"), &["view", &card.to_string(), "--json"])
        .map_err(|e| refuse("validate", "card-not-found", e))?;
    let status = json_str_field(&cj, "status").unwrap_or_default();
    let owner = json_str_field(&cj, "owner").unwrap_or_default();
    validate_card(&status, &owner, role, card)
        .map_err(|(reason, detail)| refuse("validate", &reason, detail))?;

    // ── Step 2: werk pre-flight — the refuse-if-dirty guarantee. The werk DOES
    // exist at unpull time (created at pull); surface dirt with a typed reason
    // BEFORE any teardown so uncommitted work is never dropped.
    if !werk.is_dir() {
        return Err(refuse("werk-preflight", "werk-not-initialized",
            format!("werk path does not exist: {} — the card's werk may already be removed", werk.display())));
    }
    if !werk.join(".git").exists() {
        return Err(refuse("werk-preflight", "werk-not-initialized",
            format!("werk path exists but is not a git worktree: {}", werk.display())));
    }
    let werk_s = werk.to_string_lossy().to_string();
    let dirty = run_in(&werk_s, "git", &["status", "--porcelain"])
        .map_err(|e| refuse("werk-preflight", "werk-corrupt", format!("git status failed at {}: {}", werk_s, e)))?;
    if !dirty.trim().is_empty() {
        return Err(refuse("werk-preflight", "werk-dirty",
            format!("werk has uncommitted changes — commit or recover them first:\n{}", dirty.trim())));
    }

    // ── Step 3: board back to Next (idempotent on already-Next).
    if let Err(e) = run(&script(home, "cards"), &["move", &card.to_string(), "Next"]) {
        let lc = e.to_lowercase();
        if !lc.contains("already") {
            return Err(refuse("cards-move", "move-fail", e));
        }
        jsonl(home, role, card, &trace, "cards.move.idempotent", "");
    }

    // ── Step 4: tear down worktree + branch (idempotent on already-removed).
    // chorus-werk remove also refuses dirty — Step 2 surfaced it earlier, typed.
    let mut branch_closed = true;
    if let Err(e) = run(&script(home, "chorus-werk"), &["remove", role, &card.to_string()]) {
        let lc = e.to_lowercase();
        if lc.contains("already removed") {
            jsonl(home, role, card, &trace, "werk.remove.idempotent", "");
        } else {
            return Err(refuse("werk-close", "branch-close-fail", e));
        }
    }
    let _ = branch_closed; // closed (or already was) on every non-refused path.
    branch_closed = true;

    // ── Step 5: role-state idle (best-effort, non-fatal — a local declaration).
    let _ = run(&script(home, "role-state"), &[role, "idle"]);

    // ── Step 6: witness. card.unpulled is the contract event (parity).
    jsonl(home, role, card, &trace, "unpull.completed",
        &format!(",\"prior_branch\":\"{}\",\"branch_closed\":{}", branch, branch_closed));
    emit_spine(home, "card.unpulled", role, card, &trace, &[("prior_branch", &branch)]);
    Ok(branch)
}

/// Entry: parse the contract args + env, run the verb.
pub fn run_unpull() -> R<String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let (card, role, _atomic) = parse_unpull_args(&args, env::var("DEPLOY_ROLE").ok())?;
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    unpull(card, &role, &home, &werk_base)
}
