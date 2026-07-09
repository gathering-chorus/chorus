//! werk-push — `/werk/push` v2 logic (atomic PUSH verb, split from #3056).
//!
//! Self-contained: std only + a direct libc `flock` extern; calls `git` and `gh`
//! as subprocesses. No dependency on any other chorus code.
//!
//! ATOMIC VERB — push ONLY. Pushes a card's already-committed branch `<role>/<card>`
//! to origin and registers the `chorus/push/<card>` gh status on the pushed sha
//! (now visible on GitHub). This is the state that commit-only deliberately leaves
//! absent: bundled commit+push HID push; split, "committed but not pushed" vs
//! "pushed" are distinct, separately-traceable states in gh / act / loki.
//!
//! - Refuse-if-no-werk (deterministic `werk_base/<role>-<card>`, no canonical
//!   fallback — the #3012/#3013 fix), wrong-branch, and nothing-to-push.
//! - Sanctioned-pusher sentinel `_GIT_QUEUE_PUSH=1` satisfies the pre-push hook
//!   (#2598); the wrong-branch refusal carries the #2580 cross-role intent.
//! - All-or-nothing: if gh registration fails after the push, the remote ref is
//!   deleted (delete IS a push, so it also carries the sentinel) — no orphan ref.
//! - Idempotent: a branch already on origin at the same sha is a no-op success.
//! - JSONL witness per step: best-effort, NEVER affects the operation.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// #3513 — the ONE shared failure classifier (failure_class / fail_extra).
include!("../../shared/failure_class.rs");

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

pub fn trace_id() -> String {
    let ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}", ns, std::process::id())
}

/// #3163 — resolve the card's trace by the #3045 verb contract (inherit, don't mint):
/// `CHORUS_TRACE_ID` env -> `<trace_dir>/<card>-trace` file -> mint+persist. The
/// persisted file threads ONE trace across pull -> commit -> push -> acp, so a rejected
/// push shows on the SAME card thread as its commit. Pure (dir injected) -> testable.
/// Mirrors werk-commit::resolve_trace_in.
pub fn resolve_trace_in(card: u64, env_trace: Option<&str>, trace_dir: &Path) -> String {
    if let Some(t) = env_trace {
        let t = t.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    let p = trace_dir.join(format!("{}-trace", card));
    if let Ok(t) = fs::read_to_string(&p) {
        let t = t.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    let t = trace_id();
    let _ = fs::write(&p, &t); // best-effort persist; downstream verbs read it
    t
}

pub fn resolve_trace(card: u64) -> String {
    let env = env::var("CHORUS_TRACE_ID").ok();
    resolve_trace_in(card, env.as_deref(), Path::new("/tmp"))
}

/// #3163 — the spine event contract (#3135 AUDITABLE): the args handed to `chorus-log`
/// so a push's failures/lifecycle are Loki-queryable, keyed by card + the inherited
/// trace. Pure -> testable. Mirrors werk-commit::spine_args / werk-pull::spine_args.
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

pub fn jsonl_line(ts: u128, event: &str, role: &str, card: u64, trace: &str, extra: &str) -> String {
    format!(
        "{{\"ts\":{},\"event\":\"{}\",\"role\":\"{}\",\"card_id\":{},\"trace_id\":\"{}\"{}}}\n",
        ts, event, role, card, trace, extra
    )
}

// --- side-effecting helpers ---

fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-push.jsonl");
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let line = jsonl_line(ts, event, role, card, trace, extra);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// #3163 — emit one event to the ONE spine (`chorus.log` via the canonical `chorus-log`
/// subprocess) so a push's failures/lifecycle are Loki-queryable, keyed by card + the
/// inherited trace. Best-effort like the jsonl witness — it NEVER blocks or fails the
/// push (logging can't affect the operation). The jsonl witness stays (verbose local);
/// the spine is the queryable record (#3135). Mirrors werk-commit::emit_spine.
fn emit_spine(home: &Path, event: &str, role: &str, card: u64, trace: &str, extras: &[(&str, &str)]) {
    let log = home.join("platform/scripts/chorus-log");
    let log_s = match log.to_str() {
        Some(s) => s,
        None => return,
    };
    let args = spine_args(event, role, card, trace, extras);
    let mut c = Command::new("bash");
    c.arg(log_s);
    for a in &args {
        c.arg(a);
    }
    let _ = c.output(); // best-effort; a missing/failing chorus-log never affects the push
}

/// Run a CLI in a working dir with extra env vars; non-zero exit is a typed error.
fn run_in_env(dir: &str, cmd: &str, args: &[&str], envs: &[(&str, &str)]) -> R<String> {
    let mut c = Command::new(cmd);
    c.args(args).current_dir(dir);
    for (k, v) in envs {
        c.env(k, v);
    }
    let out = c.output().map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!(
            "{} {}: {}",
            cmd,
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_in(dir: &str, cmd: &str, args: &[&str]) -> R<String> {
    run_in_env(dir, cmd, args, &[])
}

pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

pub fn lock(home: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = home.join(".git/chorus-push.lock");
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false) // a lock file is opened only for flock — never truncate it.
        .open(&p)
        .map_err(|e| format!("cannot open lock {}: {}", p.display(), e))?;
    let start = Instant::now();
    loop {
        if unsafe { flock(f.as_raw_fd(), LOCK_EX_NB) } == 0 {
            return Ok(FlockGuard(f));
        }
        if start.elapsed() >= timeout {
            return Err("another push holds the repo lock (timed out after 30s)".to_string());
        }
        sleep(Duration::from_millis(100));
    }
}

fn path(p: &Path) -> R<&str> {
    p.to_str().ok_or_else(|| format!("non-utf8 path: {}", p.display()))
}

/// Register the push in gh — the process holder (v6 diagram). state=success on the
/// just-pushed sha (now on GitHub), context chorus/push/<card>, carrying the sha +
/// trace so the pipeline state is queryable per card and distinct from commit.
fn register_gh(werk_s: &str, card: u64, role: &str, trace: &str, branch: &str, sha: &str) -> R<()> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/statuses/{}", sha);
    let desc = format!("role={} trace={} branch={} sha={} status=pushed", role, trace, branch, sha);
    run_in(
        werk_s,
        "gh",
        &[
            "api", &endpoint,
            "-f", "state=success",
            "-f", &format!("context=chorus/push/{}", card),
            "-f", &format!("description={}", desc),
        ],
    )?;
    Ok(())
}

/// #3296 — parse the contract args + recognize `--atomic` ANYWHERE. push is in the
/// ADR-037 --atomic-FREE group (reversible, no approval), so the flag is accepted +
/// reported but does NOT branch behavior — push() is the one path. Pure + testable:
/// run_push feeds it env::args, so the CLI seam is covered, not just the lib fn (the
/// #3306 lesson — an untested flag-parse is how a green binary dies in prod).
pub fn parse_push_args(args: &[String], deploy_role: Option<String>) -> R<(u64, String, bool)> {
    let atomic = args.iter().any(|a| a == "--atomic");
    let pos: Vec<&String> = args.iter().filter(|a| a.as_str() != "--atomic").collect();
    let card_arg = pos
        .first()
        .ok_or_else(|| "usage: werk-push <card> <role> [--atomic]".to_string())?;
    let card: u64 = card_arg
        .parse()
        .map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = pos
        .get(1)
        .map(|s| s.to_string())
        .or(deploy_role)
        .ok_or_else(|| "usage: werk-push <card> <role> [--atomic] (or set DEPLOY_ROLE)".to_string())?;
    Ok((card, role, atomic))
}

/// Entry: parse the contract args (`werk-push <card> <role> [--atomic]`, role falls
/// back to $DEPLOY_ROLE) + env, then run the verb. Conforms to ADR-032 (verb contract)
/// + ADR-037 (--atomic): push is the reversible/free group, so --atomic is recognized
/// but non-branching — push() is always the one path.
pub fn run_push() -> R<String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let (card, role, _atomic) = parse_push_args(&args, env::var("DEPLOY_ROLE").ok())?;
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    push(card, &role, &home, &werk_base)
}

/// The whole verb, all inputs explicit so it is testable against a real temp repo
/// (deps injected via PATH: real `git`, shimmed `gh`). Returns the pushed sha.
pub fn push(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    // #3163: inherit the card's trace (env -> /tmp/<card>-trace -> mint), so a rejected
    // push lands on the SAME thread as its commit — not a fresh-minted orphan trace.
    let trace = resolve_trace(card);
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "push.started", "");
    emit_spine(home, "push.started", role, card, &trace, &[]);

    // #3012/#3013 fix: deterministic werk, REFUSE if absent. No canonical fallback.
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "push.refused", &fail_extra("no-werk"));
        emit_spine(home, "push.refused", role, card, &trace, &[("reason", "no-werk"), ("failureClass", failure_class("no-werk")), ("disposition", "refuse")]);
        return Err(format!("no werk for #{} at {} — pull + commit the card first", card, werk.display()));
    }
    // werk must be on the card's branch (carries the #2580 cross-role intent).
    let cur = run_in(&werk_s, "git", &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    if cur != branch {
        jsonl(home, role, card, &trace, "push.refused", &fail_extra("wrong-branch"));
        emit_spine(home, "push.refused", role, card, &trace, &[("reason", "wrong-branch"), ("failureClass", failure_class("wrong-branch")), ("disposition", "refuse")]);
        return Err(format!("werk is on '{}', not '{}' — refusing to push", cur, branch));
    }

    // nothing-to-push: no commits ahead of origin/main -> two distinct states (#3632),
    // mirroring werk-commit's resume disposition:
    //   RESUME: the card's work already LANDED on origin/main (Tier-1 proof — a main
    //   subject referencing #<card>). Nothing-ahead is the SUCCESS state of a
    //   completed push; a stale pre-merge remote branch ref (the #2588 post-recovery
    //   shape) is converged to local under the same lease the real push uses.
    //   GENUINE EMPTY: no card work anywhere — refuse loudly (commit first), as before.
    let ahead = run_in(&werk_s, "git", &["rev-list", "--count", "origin/main..HEAD"])
        .unwrap_or_default()
        .trim()
        .parse::<u64>()
        .unwrap_or(0);
    if ahead == 0 {
        let subjects = run_in(&werk_s, "git", &["log", "origin/main", "-n", "300", "--format=%s"])
            .unwrap_or_default();
        if werk_teardown::subjects_reference_card(&subjects, card) {
            let sha = run_in(&werk_s, "git", &["rev-parse", "HEAD"])?.trim().to_string();
            // best-effort remote-branch convergence — the branch ref only; failure is
            // non-fatal (teardown's orphan-propagation is the backstop, #3498).
            let _ = run_in_env(&werk_s, "git",
                &["push", "--force-with-lease", "origin", &format!("HEAD:refs/heads/{}", branch)],
                &[("_GIT_QUEUE_INTERNAL", "1")]);
            jsonl(home, role, card, &trace, "push.idempotent",
                &format!(",\"reason\":\"work-already-on-main\",\"sha\":\"{}\"", sha));
            emit_spine(home, "push.idempotent", role, card, &trace,
                &[("reason", "work-already-on-main"), ("sha", &sha)]);
            return Ok(sha);
        }
        jsonl(home, role, card, &trace, "push.refused", &fail_extra("nothing-to-push"));
        emit_spine(home, "push.refused", role, card, &trace, &[("reason", "nothing-to-push"), ("failureClass", failure_class("nothing-to-push")), ("disposition", "refuse")]);
        return Err(format!("#{}: nothing to push (no commits ahead of origin/main — commit first)", card));
    }

    let sha = run_in(&werk_s, "git", &["rev-parse", "HEAD"])?.trim().to_string();

    // idempotent: remote already at this sha -> no-op success (still ensure gh).
    let remote = run_in(&werk_s, "git", &["ls-remote", "origin", &branch]).unwrap_or_default();
    if remote.split_whitespace().next() == Some(sha.as_str()) {
        jsonl(home, role, card, &trace, "push.idempotent", &format!(",\"sha\":\"{}\"", sha));
        emit_spine(home, "push.idempotent", role, card, &trace, &[("sha", &sha)]);
        return Ok(sha);
    }

    // --- push, serialized under the flock. Sanctioned-pusher sentinel for #2598. ---
    // #3194: re-running the chain on an already-pushed card is THE common path (deploy
    // fails often). werk-commit unconditionally rebases onto current main (#3186),
    // rewriting history, so the branch diverges from its OWN earlier push and a plain
    // push is non-ff. When the remote branch already exists, push with
    // --force-with-lease=<branch> (NO explicit sha): git uses our remote-tracking ref
    // as the expected value, so it re-points our own card branch to the rebased history
    // but REFUSES if origin moved to anything we haven't fetched (a peer push) — a
    // lease, never a blind force. wrong-branch above already proved it's our own branch.
    // Fresh branch (remote absent) stays a plain push.
    let remote_exists = !remote.trim().is_empty();
    let lease = format!("--force-with-lease={}", branch);
    let push_args: Vec<&str> = if remote_exists {
        vec!["push", "origin", &branch, &lease]
    } else {
        vec!["push", "origin", &branch]
    };
    {
        let _lock = lock(home, Duration::from_secs(30))?;
        jsonl(home, role, card, &trace, "lock.acquired", "");
        if let Err(e) = run_in_env(&werk_s, "git", &push_args, &[("_GIT_QUEUE_PUSH", "1")]) {
            // #3163: the non-fast-forward reject (or the force-with-lease lease-guard
            // refuse) — the most common real push failure in a shared repo — was fully
            // silent before, returning Err with no record. Surface push.failed on the ONE
            // spine, carrying the INHERITED trace + reason + disposition, so the break
            // shows on the card's thread the instant it happens (merged-not-live precursor).
            jsonl(home, role, card, &trace, "push.failed", &fail_extra("push-rejected"));
            emit_spine(home, "push.failed", role, card, &trace, &[("reason", "push-rejected"), ("failureClass", failure_class("push-rejected")), ("disposition", "refuse")]);
            return Err(format!("push failed: {}", e));
        }
    }
    jsonl(home, role, card, &trace, "pushed", &format!(",\"sha\":\"{}\"", sha));

    // gh registration LAST (most fragile). On failure, delete the ref we just
    // created (delete IS a push -> carries the sentinel) so there's no orphan ref.
    if let Err(e) = register_gh(&werk_s, card, role, &trace, &branch, &sha) {
        jsonl(home, role, card, &trace, "push.rolledback", ",\"reason\":\"gh-register-fail\"");
        emit_spine(home, "push.rolledback", role, card, &trace, &[("reason", "gh-register-fail"), ("disposition", "refuse")]);
        if let Ok(_lock) = lock(home, Duration::from_secs(30)) {
            let _ = run_in_env(&werk_s, "git", &["push", "origin", "--delete", &branch], &[("_GIT_QUEUE_PUSH", "1")]);
        }
        return Err(format!("gh registration failed; deleted the pushed ref: {}", e));
    }
    jsonl(home, role, card, &trace, "gh.registered", "");

    jsonl(home, role, card, &trace, "push.completed", &format!(",\"sha\":\"{}\"", sha));
    emit_spine(home, "push.completed", role, card, &trace, &[("sha", &sha)]);
    Ok(sha)
}
