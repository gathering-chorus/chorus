//! werk-commit — `/werk/commit` v2 logic (card #3056).
//!
//! Self-contained: std only + a direct libc `flock` extern; calls `git`, `cards`,
//! and `gh` CLIs as subprocesses. No dependency on any other chorus code.
//!
//! Commit + push a card's werk -> a commit on `<role>/<card>` pushed to origin,
//! registered in gh as the process holder. The v2 verb-binary blueprint is
//! werk-pull (#3045); this adds the commit+push specifics.
//!
//! Key fix vs the old chorus_commit path (#3012/#3013 resolver bug): the werk is
//! derived deterministically as `werk_base/<role>-<card>` and REFUSED if absent —
//! there is no fuzzy "resolve working tree" that falls back to canonical. In the
//! ephemeral per-card werk model (#2913) the werk holds ONLY this card's changes,
//! so `git add -A` (gitignore-filtered) stages exactly the card's files: no per-
//! file pathspec dance (#3053 was a shared-werk relic), and untracked NEW files
//! are staged too (the #3085 gap).
//!
//! - Lock: one flock around the canonical-touching git steps (add/commit/push).
//! - All-or-nothing: on push or gh failure, the local commit is soft-reset and any
//!   pushed remote ref is deleted — no half state. (Delta vs pull, which never pushes.)
//! - JSONL log per step: best-effort, NEVER affects the operation (not transactional).

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

/// Team commit-message convention: "<role>: #<card> — <summary>". An empty
/// summary degrades to "<role>: #<card>" (no dangling em-dash) so the header
/// always parses.
pub fn commit_message(role: &str, card: u64, summary: &str) -> String {
    let s = summary.trim();
    if s.is_empty() {
        format!("{}: #{}", role, card)
    } else {
        format!("{}: #{} — {}", role, card, s)
    }
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

/// JSONL: append one line to a local file. Best-effort, swallows its own errors,
/// so logging can NEVER affect the operation. Borg ingests this file downstream.
fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-commit.jsonl");
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let line = jsonl_line(ts, event, role, card, trace, extra);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Run a CLI in a working dir with extra env vars, capture stdout; any non-zero
/// exit is a typed error (no silent failure). Zero exit with warnings on stdout is
/// SUCCESS — the #2936 fix (green pre-commit must not read as commit-fail): only the
/// exit code decides, never substring-matching the output.
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

/// flock guard — auto-releases on drop (and on process exit/crash, kernel-level).
pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

pub fn lock(home: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = home.join(".git/chorus-commit.lock");
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
            return Err("another commit holds the repo lock (timed out after 30s)".to_string());
        }
        sleep(Duration::from_millis(100));
    }
}

fn path(p: &Path) -> R<&str> {
    p.to_str().ok_or_else(|| format!("non-utf8 path: {}", p.display()))
}

/// Register the commit in gh — the process holder (v6 diagram). state=success on
/// the just-pushed sha, context chorus/commit/<card>, carrying the sha + trace so
/// the pipeline state is queryable per card.
fn register_gh(werk_s: &str, card: u64, role: &str, trace: &str, branch: &str, sha: &str) -> R<()> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/statuses/{}", sha);
    let desc = format!("role={} trace={} branch={} sha={} status=committed", role, trace, branch, sha);
    run_in(
        werk_s,
        "gh",
        &[
            "api",
            &endpoint,
            "-f",
            "state=success",
            "-f",
            &format!("context=chorus/commit/{}", card),
            "-f",
            &format!("description={}", desc),
        ],
    )?;
    Ok(())
}

/// Undo everything this invocation created, best-effort, under the lock.
/// All-or-nothing: soft-reset the local commit we made (keeps the work in the
/// tree) and delete any remote ref we pushed. `committed`/`pushed` gate each
/// undo so an amend/re-commit (prior pushed commit) is never clobbered.
#[allow(clippy::too_many_arguments)]
fn rollback(
    home: &Path,
    werk_s: &str,
    branch: &str,
    role: &str,
    card: u64,
    trace: &str,
    reason: &str,
    committed: bool,
    pushed: bool,
) {
    jsonl(home, role, card, trace, "commit.rolledback", &format!(",\"reason\":\"{}\"", reason));
    if let Ok(_lock) = lock(home, Duration::from_secs(30)) {
        if pushed {
            // we created the remote ref this run; remove it. delete IS a push, so
            // it needs the sanctioned-pusher sentinel too (else the pre-push hook
            // refuses the rollback and leaves an orphan remote ref).
            let _ = run_in_env(werk_s, "git", &["push", "origin", "--delete", branch], &[("_GIT_QUEUE_PUSH", "1")]);
        }
        if committed {
            // undo our commit, keep the changes in the working tree.
            let _ = run_in(werk_s, "git", &["reset", "--soft", "HEAD~1"]);
        }
    }
}

/// Entry: parse the contract args (`werk-commit <card> <role> [summary...]`, role
/// falls back to $DEPLOY_ROLE) + env, then run the verb.
pub fn run_commit() -> R<String> {
    let card_arg = env::args().nth(1).ok_or_else(|| "usage: werk-commit <card> <role> [summary]".to_string())?;
    let card: u64 = card_arg
        .parse()
        .map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = env::args()
        .nth(2)
        .or_else(|| env::var("DEPLOY_ROLE").ok())
        .ok_or_else(|| "usage: werk-commit <card> <role> [summary] (or set DEPLOY_ROLE)".to_string())?;
    let summary = env::args().skip(3).collect::<Vec<_>>().join(" ");
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    commit(card, &role, &summary, &home, &werk_base)
}

/// The whole verb, all inputs explicit so it is testable against a real temp repo
/// (deps injected via PATH: real `git`, shimmed `cards`/`gh`). Returns the commit sha.
pub fn commit(card: u64, role: &str, summary: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = trace_id();
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "commit.started", "");

    // #3012/#3013 fix: deterministic werk, REFUSE if absent. No canonical fallback.
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "commit.refused", ",\"reason\":\"no-werk\"");
        return Err(format!("no werk for #{} at {} — pull the card first", card, werk.display()));
    }
    // werk must be on the card's branch (guards same-role wrong-card, #2641 class).
    let cur = run_in(&werk_s, "git", &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    if cur != branch {
        jsonl(home, role, card, &trace, "commit.refused", ",\"reason\":\"wrong-branch\"");
        return Err(format!("werk is on '{}', not '{}' — refusing to commit", cur, branch));
    }

    // idempotent: clean tree already ahead of origin/main -> already committed.
    let dirty = !run_in(&werk_s, "git", &["status", "--porcelain"])?.trim().is_empty();
    let ahead = run_in(&werk_s, "git", &["rev-list", "--count", "origin/main..HEAD"])
        .unwrap_or_default()
        .trim()
        .parse::<u64>()
        .unwrap_or(0);
    if !dirty && ahead > 0 {
        let sha = run_in(&werk_s, "git", &["rev-parse", "HEAD"])?.trim().to_string();
        jsonl(home, role, card, &trace, "commit.idempotent", &format!(",\"sha\":\"{}\"", sha));
        return Ok(sha);
    }
    if !dirty && ahead == 0 {
        return Err(format!("#{}: nothing to commit (werk clean, no commits ahead of origin/main)", card));
    }

    let msg = commit_message(role, card, summary);

    // --- canonical-touching steps, serialized under one flock ---
    {
        let _lock = lock(home, Duration::from_secs(30))?;
        jsonl(home, role, card, &trace, "lock.acquired", "");

        // ephemeral per-card werk => `add -A` stages EXACTLY this card's changes
        // (gitignore filters dist/node_modules); untracked NEW files included (#3085).
        run_in(&werk_s, "git", &["add", "-A"])?;
        jsonl(home, role, card, &trace, "staged", "");

        // commit runs the pre-commit hook (the quality gate). #2936: only a non-zero
        // exit is a failure — `run_in_env` returns Err only on non-zero, so a green
        // hook with warnings on stdout is success, never misread as commit-fail.
        //
        // _GIT_QUEUE_INTERNAL=1 is the sanctioned-committer sentinel (mirrors
        // git-queue.sh:301): the pre-commit hook blocks any commit lacking it ("all
        // commits must go through git-queue.sh"). werk-commit is git-queue.sh's v2
        // peer, so it sets the sentinel — every quality gate still runs, only the
        // bypass-block is satisfied. (Found by the #3047 pull->commit integration test.)
        run_in_env(&werk_s, "git", &["commit", "-m", &msg], &[("_GIT_QUEUE_INTERNAL", "1")])
            .map_err(|e| format!("commit failed (pre-commit gate?): {}", e))?;
        jsonl(home, role, card, &trace, "committed", "");
    } // flock released; re-acquired inside push so the lock window stays tight

    let sha = run_in(&werk_s, "git", &["rev-parse", "HEAD"])?.trim().to_string();

    // push <role>/<card> to origin (creates the remote ref; pull never pushed).
    // most-fragile-last ordering: any failure here rolls back the local commit.
    {
        let _lock = lock(home, Duration::from_secs(30))?;
        // _GIT_QUEUE_PUSH=1 is the sanctioned-pusher sentinel the pre-push hook
        // requires (#2598). werk-commit already refused a wrong-branch werk above,
        // which carries the #2580 cross-role intent (you can only push your own
        // card's branch). Found by the #3047 integration test.
        if let Err(e) = run_in_env(&werk_s, "git", &["push", "origin", &branch], &[("_GIT_QUEUE_PUSH", "1")]) {
            drop(_lock);
            // commit always precedes push, so the local commit exists to undo.
            rollback(home, &werk_s, &branch, role, card, &trace, "push-fail", true, false);
            return Err(format!("push failed; rolled back local commit: {}", e));
        }
    }
    jsonl(home, role, card, &trace, "pushed", &format!(",\"sha\":\"{}\"", sha));

    // gh registration LAST (most fragile). On failure, undo commit AND remote ref.
    if let Err(e) = register_gh(&werk_s, card, role, &trace, &branch, &sha) {
        // commit + push both succeeded to reach here; undo both.
        rollback(home, &werk_s, &branch, role, card, &trace, "gh-register-fail", true, true);
        return Err(format!("gh registration failed; rolled back commit + remote ref: {}", e));
    }
    jsonl(home, role, card, &trace, "gh.registered", "");

    jsonl(home, role, card, &trace, "commit.completed", &format!(",\"sha\":\"{}\"", sha));
    Ok(sha)
}
