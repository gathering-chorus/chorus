//! werk-acp — atomic orchestration verb (#3176). Composes the existing atomic
//! verbs into ONE all-or-nothing accept:
//!
//!   werk-commit → werk-push → werk-deploy(--target canonical) → werk-accept
//!
//! Moves the acp orchestration OUT of the chorus_acp MCP handler (TS glue: git,
//! gh, launchctl, stepEmit per phase) into a clean verb binary — "mcp is not an
//! orchestration layer" (Jeff). acp was the ONLY verb whose orchestration leaked
//! into the MCP layer, which is both why it was unreadable and the merged≠live
//! cause (a best-effort kickstart that emitted 'completed' on error).
//!
//! THE GATING CONTRACT (#3176 AC2 — the merged≠live fix): werk-deploy(canonical)
//! builds + deploys + VERIFIES running==built. If prod doesn't come up == built,
//! werk-deploy exits non-zero and acp FAILS THERE — werk-accept is never reached,
//! so the source never merges to main. Accepted ⇒ live, enforced by construction.
//!
//! TWO IDENTITIES. The BUILDER (werk location) owns commit/push/deploy. The
//! ACCEPTER (the identity invoking acp, from $DEPLOY_ROLE) owns the finalize:
//! werk-accept enforces DEC-048 can_accept (jeff/wren, never self-accept-own) on
//! THAT identity. acp does not re-implement authority — it threads the accepter
//! into the werk-accept sub-step and inherits the gate.
//!
//! COMPOSITION, not import (ADR-032 §1): the sub-verbs are resolved on PATH and
//! exec'd as subprocesses (like the others exec git/cards), with one shared
//! trace_id threaded via CHORUS_TRACE_ID so the whole accept is one trace.
//!
//! werk-merge (#3175) is NOT yet a separate step — merge currently lives inside
//! werk-accept (as it did on #3186). When #3175 lands, the merge moves to a
//! werk-merge step before deploy and accept becomes finalize-only; acp gains one
//! line, the contract is unchanged.
//!
//! All-or-nothing: the first failing step stops the chain with a typed reason.
//! werk-deploy owns its own rollback on a failed verify (prior binary restored),
//! so a deploy failure leaves prod consistent AND main unmerged. A failure AFTER
//! a successful deploy (i.e. at accept) leaves prod live + source unmerged — the
//! normal test-in-prod-before-merge state; werk-accept is idempotent, so re-run
//! finalizes. This is honest and recoverable, not a half-written merge.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}
const LOCK_EX_NB: i32 = 0x02 | 0x04; // LOCK_EX | LOCK_NB
const LOCK_UN: i32 = 0x08;

pub type R<T> = Result<T, String>;

// --- pure helpers (unit-tested) ---

pub fn branch_name(role: &str, card: u64) -> String {
    format!("{}/{}", role, card)
}

fn mint_trace() -> String {
    let ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}", ns, std::process::id())
}

/// Shared trace per ADR-032 §3: CHORUS_TRACE_ID env → /tmp/<card>-trace → mint+persist.
/// The SAME id is threaded into every sub-verb so the whole accept is one trace.
pub fn resolve_trace(card: u64) -> String {
    if let Ok(t) = env::var("CHORUS_TRACE_ID") {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    let p = format!("/tmp/{}-trace", card);
    if let Ok(t) = fs::read_to_string(&p) {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    let t = mint_trace();
    let _ = fs::write(&p, &t);
    t
}

/// The four steps of an accept, in order. `verb` is the binary resolved on PATH;
/// `accepter_step` marks the one step whose DEPLOY_ROLE is the ACCEPTER, not the
/// builder (werk-accept's can_accept gates on the accepter — DEC-048).
pub struct Step {
    pub label: &'static str,
    pub verb: &'static str,
    pub extra_args: &'static [&'static str],
    pub accepter_step: bool,
}

/// The composition plan (#3176 AC1). Order is the contract: deploy (build+verify)
/// GATES accept — accept can only run if every prior step, deploy included, passed.
pub fn plan() -> [Step; 4] {
    [
        Step { label: "commit", verb: "werk-commit", extra_args: &[], accepter_step: false },
        Step { label: "push", verb: "werk-push", extra_args: &[], accepter_step: false },
        Step { label: "deploy", verb: "werk-deploy", extra_args: &["--target", "canonical"], accepter_step: false },
        Step { label: "accept", verb: "werk-accept", extra_args: &[], accepter_step: true },
    ]
}

// --- side-effecting helpers ---

fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-acp.jsonl");
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let line = format!(
        "{{\"ts\":{},\"event\":\"{}\",\"role\":\"{}\",\"card_id\":{},\"trace_id\":\"{}\"{}}}\n",
        ts, event, role, card, trace, extra
    );
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Exec a sub-verb (resolved on PATH) in `dir` with extra env, capture stdout.
/// Non-zero exit is a typed error carrying the sub-verb's stderr (no swallowing —
/// the opposite of the best-effort kickstart this verb retires). Zero exit with
/// warnings on stdout is SUCCESS (only the exit code decides).
fn run_verb(dir: &str, verb: &str, args: &[&str], envs: &[(&str, &str)]) -> R<String> {
    let mut c = Command::new(verb);
    c.args(args).current_dir(dir);
    for (k, v) in envs {
        c.env(k, v);
    }
    let out = c.output().map_err(|e| format!("{} failed to start: {}", verb, e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

/// Serialize the whole accept under one lock (lives in the werk) so two acps of
/// the same card can't interleave their sub-verbs.
pub fn lock(werk: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = werk.join(".git-acp.lock");
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&p)
        .map_err(|e| format!("cannot open lock {}: {}", p.display(), e))?;
    let start = Instant::now();
    loop {
        if unsafe { flock(f.as_raw_fd(), LOCK_EX_NB) } == 0 {
            return Ok(FlockGuard(f));
        }
        if start.elapsed() >= timeout {
            return Err("another acp holds the werk lock (timed out)".to_string());
        }
        sleep(Duration::from_millis(100));
    }
}

fn path(p: &Path) -> R<&str> {
    p.to_str().ok_or_else(|| format!("non-utf8 path: {}", p.display()))
}

/// Entry: `werk-acp <card> <builder-role>`. The ACCEPTER is the calling identity
/// ($DEPLOY_ROLE) — distinct from the builder, so `DEPLOY_ROLE=jeff werk-acp 3176
/// kade` accepts kade's card as jeff. builder falls back to $DEPLOY_ROLE too.
pub fn run_acp() -> R<String> {
    let card_arg = env::args().nth(1).ok_or_else(|| "usage: werk-acp <card> <builder-role>".to_string())?;
    let card: u64 = card_arg
        .parse()
        .map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let builder = env::args()
        .nth(2)
        .filter(|s| !s.starts_with("--"))
        .or_else(|| env::var("DEPLOY_ROLE").ok())
        .ok_or_else(|| "usage: werk-acp <card> <builder-role> (or set DEPLOY_ROLE)".to_string())?;
    // The accepter is the invoking identity. Falls back to the builder (a role
    // accepting its own card → werk-accept's can_accept refuses, as it should).
    let accepter = env::var("DEPLOY_ROLE").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| builder.clone());
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    acp(card, &builder, &accepter, &home, &werk_base)
}

/// The whole orchestration, all inputs explicit so it is testable against a temp
/// repo with the sub-verbs PATH-shimmed. Returns a per-step summary.
pub fn acp(card: u64, builder: &str, accepter: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = resolve_trace(card);
    let branch = branch_name(builder, card);
    let werk = werk_base.join(format!("{}-{}", builder, card));
    let werk_s = path(&werk)?.to_string();
    let card_s = card.to_string();

    jsonl(home, builder, card, &trace, "acp.started", &format!(",\"accepter\":\"{}\"", accepter));

    // Same refusals as the other verbs: deterministic werk, on the card's branch.
    if !werk.is_dir() {
        jsonl(home, builder, card, &trace, "acp.refused", ",\"reason\":\"no-werk\"");
        return Err(format!("no werk for #{} at {} — pull the card first", card, werk.display()));
    }
    let cur = run_verb(&werk_s, "git", &["-C", &werk_s, "rev-parse", "--abbrev-ref", "HEAD"], &[]).unwrap_or_default();
    if cur.trim() != branch {
        jsonl(home, builder, card, &trace, "acp.refused", ",\"reason\":\"branch-mismatch\"");
        return Err(format!("werk is on '{}', not '{}' — refusing to acp", cur.trim(), branch));
    }

    let home_s = path(home)?;
    let werk_base_s = path(werk_base)?;

    // One lock around the whole composition.
    let _lock = lock(&werk, Duration::from_secs(600))?;
    jsonl(home, builder, card, &trace, "lock.acquired", "");

    let mut done: Vec<String> = Vec::new();
    for step in plan().iter() {
        // The accept step runs as the ACCEPTER (DEC-048 authority); every other
        // step runs as the builder (operating on the builder's werk).
        let step_role = if step.accepter_step { accepter } else { builder };
        let mut args: Vec<&str> = vec![&card_s, builder];
        args.extend_from_slice(step.extra_args);
        let envs = [
            ("CHORUS_TRACE_ID", trace.as_str()),
            ("DEPLOY_ROLE", step_role),
            ("CHORUS_ROLE", step_role),
            ("CHORUS_HOME", home_s),
            ("CHORUS_WERK_BASE", werk_base_s),
        ];
        jsonl(home, builder, card, &trace, "step.started", &format!(",\"step\":\"{}\",\"verb\":\"{}\"", step.label, step.verb));
        match run_verb(&werk_s, step.verb, &args, &envs) {
            Ok(_) => {
                jsonl(home, builder, card, &trace, "step.completed", &format!(",\"step\":\"{}\"", step.label));
                done.push(step.label.to_string());
            }
            Err(e) => {
                // All-or-nothing: stop at the first failure with a typed reason.
                // A deploy failure here is THE gating contract — accept is never
                // reached, so the source never merges (no merged≠live).
                jsonl(home, builder, card, &trace, "acp.failed",
                    &format!(",\"step\":\"{}\",\"reason\":\"{}-fail\"", step.label, step.label));
                return Err(format!(
                    "acp failed at {} (#{}): {} — card stays WIP, no merge (steps done: [{}])",
                    step.label, card, e, done.join(", ")
                ));
            }
        }
    }

    jsonl(home, builder, card, &trace, "acp.completed", &format!(",\"steps\":\"{}\"", done.join(",")));
    Ok(format!("#{} accepted ({}) — steps: {}", card, accepter, done.join(" → ")))
}
