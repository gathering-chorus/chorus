//! werk-commit — `/werk/commit` v2 logic (card #3056).
//!
//! Self-contained: std only + a direct libc `flock` extern; calls `git` as a
//! subprocess. No dependency on any other chorus code.
//!
//! ATOMIC VERB — commit ONLY. Stages + commits a card's werk to a LOCAL commit on
//! `<role>/<card>`. It does NOT push and writes NO gh status (gh can't see an
//! unpushed sha). Pushing + the chorus/push gh status is the sibling werk-push
//! verb. The split (#3056) is deliberate: bundled commit+push HID push — you
//! couldn't see "committed but not pushed". Atomic verbs make each step a visible,
//! separately-traceable state in gh / act / loki. Blueprint: werk-pull (#3045).
//!
//! Key fix vs the old chorus_commit path (#3012/#3013 resolver bug): the werk is
//! derived deterministically as `werk_base/<role>-<card>` and REFUSED if absent —
//! there is no fuzzy "resolve working tree" that falls back to canonical. In the
//! ephemeral per-card werk model (#2913) the werk holds ONLY this card's changes,
//! so `git add -A` (gitignore-filtered) stages exactly the card's files: no per-
//! file pathspec dance (#3053 was a shared-werk relic), and untracked NEW files
//! are staged too (the #3085 gap).
//!
//! #3186 — rebase-or-refuse: before the commit lands, the werk is rebased onto
//! CURRENT origin/main (fetched first), closing the pull->commit staleness window.
//! Everything downstream (push/merge/build/deploy) then operates on a current base
//! BY CONSTRUCTION. all-or-nothing: a rebase conflict aborts and refuses cleanly
//! (typed `rebase-conflict`, werk preserved) rather than landing on a stale base.
//!
//! - Lock: one flock around the canonical-touching git steps (add + commit + rebase).
//! - No rollback step: add + commit is atomic (a failed pre-commit gate leaves
//!   staged changes but no commit; a re-run is idempotent). Nothing fragile follows.
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

/// #3186 — the rebase-or-refuse invariant. Rebase the card's commit(s) onto the
/// CURRENT origin/main, closing the pull->commit staleness window so push / merge
/// / build / deploy all operate on a current base BY CONSTRUCTION. all-or-nothing:
/// a no-op when already current (behind == 0), and on conflict `git rebase --abort`
/// restores the branch EXACTLY to its pre-rebase state (the card commit is
/// preserved) before returning a typed `rebase-conflict` refusal — never a
/// swallowed half-merge. (origin/main must already be fetched-current by the caller.)
fn rebase_onto_origin_main(werk_s: &str, home: &Path, role: &str, card: u64, trace: &str) -> R<()> {
    let behind = run_in(werk_s, "git", &["rev-list", "--count", "HEAD..origin/main"])
        .unwrap_or_default()
        .trim()
        .parse::<u64>()
        .unwrap_or(0);
    if behind == 0 {
        return Ok(()); // already on current origin/main — nothing to rebase.
    }
    jsonl(home, role, card, trace, "rebase.started", &format!(",\"behind\":{}", behind));
    // sentinel set defensively in case any commit-path hook fires during replay.
    match run_in_env(werk_s, "git", &["rebase", "origin/main"], &[("_GIT_QUEUE_INTERNAL", "1")]) {
        Ok(_) => {
            jsonl(home, role, card, trace, "rebase.done", &format!(",\"onto\":\"origin/main\",\"behind\":{}", behind));
            Ok(())
        }
        Err(e) => {
            // restore the branch to its pre-rebase state — werk untouched, work kept.
            let _ = run_in(werk_s, "git", &["rebase", "--abort"]);
            jsonl(home, role, card, trace, "rebase.refused", ",\"reason\":\"rebase-conflict\"");
            Err(format!(
                "rebase-conflict: #{} conflicts with current origin/main — resolve and re-run \
                 (werk restored, commit preserved): {}",
                card, e
            ))
        }
    }
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

    // #3186: refresh origin/main so the rebase below targets CURRENT main, not the
    // stale pull-time ref. Best-effort — a LOCAL commit should not hard-require the
    // network; if fetch fails we rebase onto the freshest known main and the
    // downstream refuse-if-stale guard (werk-deploy / werk-merge) is the backstop.
    if run_in(&werk_s, "git", &["fetch", "-q", "origin", "main"]).is_err() {
        jsonl(home, role, card, &trace, "fetch.warn", ",\"reason\":\"fetch-failed\"");
    }

    let dirty = !run_in(&werk_s, "git", &["status", "--porcelain"])?.trim().is_empty();
    let ahead = run_in(&werk_s, "git", &["rev-list", "--count", "origin/main..HEAD"])
        .unwrap_or_default()
        .trim()
        .parse::<u64>()
        .unwrap_or(0);
    // Nothing to commit AND nothing already committed -> genuine no-op. (A clean werk
    // that is merely BEHIND has no card work to preserve, so there is nothing to
    // rebase either — the next real commit rebases it.)
    if !dirty && ahead == 0 {
        return Err(format!("#{}: nothing to commit (werk clean, no commits ahead of origin/main)", card));
    }

    let msg = commit_message(role, card, summary);

    // --- canonical-touching steps, serialized under one flock ---
    // Both the commit (if dirty) and the rebase-onto-current-main run under the same
    // lock. The idempotent case (clean + already-ahead) skips the commit but STILL
    // rebases — so re-running werk-commit on a now-stale already-committed werk brings
    // it current (the rebase-or-refuse invariant is unconditional, not only-on-fresh-work).
    {
        let _lock = lock(home, Duration::from_secs(30))?;
        jsonl(home, role, card, &trace, "lock.acquired", "");

        if dirty {
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
        }

        // #3186 — close the pull->commit staleness window: rebase the card's commit(s)
        // onto current origin/main. all-or-nothing — conflict aborts cleanly and refuses
        // (werk preserved). This makes everything downstream current by construction.
        rebase_onto_origin_main(&werk_s, home, role, card, &trace)?;
    } // flock released

    // Commit only — the atomic verb stops here. The commit is LOCAL: not pushed,
    // and it gets NO gh status, because gh can't see an unpushed sha. That honest
    // gap is the point of the split — "committed but not pushed" is now a distinct,
    // visible state, not hidden inside a fused commit+push. Pushing + the
    // chorus/push gh status is the separate werk-push verb.
    let sha = run_in(&werk_s, "git", &["rev-parse", "HEAD"])?.trim().to_string();
    jsonl(home, role, card, &trace, "commit.completed", &format!(",\"sha\":\"{}\"", sha));
    Ok(sha)
}
