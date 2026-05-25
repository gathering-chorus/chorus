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

/// Entry: parse the contract args (`werk-push <card> <role>`, role falls back to
/// $DEPLOY_ROLE) + env, then run the verb.
pub fn run_push() -> R<String> {
    let card_arg = env::args().nth(1).ok_or_else(|| "usage: werk-push <card> <role>".to_string())?;
    let card: u64 = card_arg
        .parse()
        .map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = env::args()
        .nth(2)
        .or_else(|| env::var("DEPLOY_ROLE").ok())
        .ok_or_else(|| "usage: werk-push <card> <role> (or set DEPLOY_ROLE)".to_string())?;
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    push(card, &role, &home, &werk_base)
}

/// The whole verb, all inputs explicit so it is testable against a real temp repo
/// (deps injected via PATH: real `git`, shimmed `gh`). Returns the pushed sha.
pub fn push(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = trace_id();
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "push.started", "");

    // #3012/#3013 fix: deterministic werk, REFUSE if absent. No canonical fallback.
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "push.refused", ",\"reason\":\"no-werk\"");
        return Err(format!("no werk for #{} at {} — pull + commit the card first", card, werk.display()));
    }
    // werk must be on the card's branch (carries the #2580 cross-role intent).
    let cur = run_in(&werk_s, "git", &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    if cur != branch {
        jsonl(home, role, card, &trace, "push.refused", ",\"reason\":\"wrong-branch\"");
        return Err(format!("werk is on '{}', not '{}' — refusing to push", cur, branch));
    }

    // nothing-to-push: no commits ahead of origin/main -> refuse (commit first).
    let ahead = run_in(&werk_s, "git", &["rev-list", "--count", "origin/main..HEAD"])
        .unwrap_or_default()
        .trim()
        .parse::<u64>()
        .unwrap_or(0);
    if ahead == 0 {
        jsonl(home, role, card, &trace, "push.refused", ",\"reason\":\"nothing-to-push\"");
        return Err(format!("#{}: nothing to push (no commits ahead of origin/main — commit first)", card));
    }

    let sha = run_in(&werk_s, "git", &["rev-parse", "HEAD"])?.trim().to_string();

    // idempotent: remote already at this sha -> no-op success (still ensure gh).
    let remote = run_in(&werk_s, "git", &["ls-remote", "origin", &branch]).unwrap_or_default();
    if remote.split_whitespace().next() == Some(sha.as_str()) {
        jsonl(home, role, card, &trace, "push.idempotent", &format!(",\"sha\":\"{}\"", sha));
        return Ok(sha);
    }

    // --- push, serialized under the flock. Sanctioned-pusher sentinel for #2598. ---
    {
        let _lock = lock(home, Duration::from_secs(30))?;
        jsonl(home, role, card, &trace, "lock.acquired", "");
        run_in_env(&werk_s, "git", &["push", "origin", &branch], &[("_GIT_QUEUE_PUSH", "1")])
            .map_err(|e| format!("push failed: {}", e))?;
    }
    jsonl(home, role, card, &trace, "pushed", &format!(",\"sha\":\"{}\"", sha));

    // gh registration LAST (most fragile). On failure, delete the ref we just
    // created (delete IS a push -> carries the sentinel) so there's no orphan ref.
    if let Err(e) = register_gh(&werk_s, card, role, &trace, &branch, &sha) {
        jsonl(home, role, card, &trace, "push.rolledback", ",\"reason\":\"gh-register-fail\"");
        if let Ok(_lock) = lock(home, Duration::from_secs(30)) {
            let _ = run_in_env(&werk_s, "git", &["push", "origin", "--delete", &branch], &[("_GIT_QUEUE_PUSH", "1")]);
        }
        return Err(format!("gh registration failed; deleted the pushed ref: {}", e));
    }
    jsonl(home, role, card, &trace, "gh.registered", "");

    jsonl(home, role, card, &trace, "push.completed", &format!(",\"sha\":\"{}\"", sha));
    Ok(sha)
}
