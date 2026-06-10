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

/// #3162 — INHERIT the shared trace instead of fresh-minting (the #3045 verb
/// contract): `CHORUS_TRACE_ID` env → `<trace_dir>/<card>-trace` file → mint+persist.
/// The persisted file threads ONE trace across pull → commit → push → acp, so a
/// failed commit shows on the SAME card thread the pull opened. `resolve_trace_in`
/// is the testable core (all inputs explicit); `resolve_trace` is the real entry.
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

/// #3162 — the spine event contract (#3135 AUDITABLE): the args handed to
/// `chorus-log` so a commit's lifecycle + failures are Loki-queryable, keyed by
/// card + the inherited trace. Pure → testable. Mirrors werk-pull::spine_args.
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

/// #3162 — emit one event to the ONE spine (`chorus.log` via the canonical
/// `chorus-log` subprocess) so a commit's failures/lifecycle are Loki-queryable,
/// keyed by card + the inherited trace. Best-effort like the jsonl witness — it
/// NEVER blocks or fails the commit (logging can't affect the operation). The
/// jsonl witness stays (verbose local); the spine is the queryable record (#3135).
/// Mirrors werk-pull::emit_spine.
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
    let _ = c.output(); // best-effort; a missing/failing chorus-log never affects the commit
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
        Err(_) => {
            // #3304 — HOLD, don't abort: leave the conflict markers in the werk for the
            // human to edit. The resolution is reachable only through the verb
            // (--continue / --abort); the guard stays whole — no raw git instructed.
            let files: Vec<String> = run_in(werk_s, "git", &["diff", "--name-only", "--diff-filter=U"])
                .unwrap_or_default()
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            jsonl(home, role, card, trace, "rebase.conflict.held",
                &format!(",\"files\":\"{}\"", files.join(",")));
            emit_spine(home, "commit.conflict", role, card, trace,
                &[("disposition", "held"), ("reason", "rebase-conflict"), ("files", &files.join(","))]);
            Err(conflict_hold_message(card, role, &files))
        }
    }
}

/// #3304 — is a rebase currently held in this werk? (worktree-aware: `--git-path`
/// resolves rebase state under .git/worktrees/<name>/.)
fn rebase_in_progress(werk_s: &str) -> bool {
    for state in ["rebase-merge", "rebase-apply"] {
        if let Ok(p) = run_in(werk_s, "git", &["rev-parse", "--git-path", state]) {
            if Path::new(p.trim()).exists() {
                return true;
            }
        }
    }
    false
}

/// #3304 — `werk-commit <card> <role> --continue`: finish a HELD rebase through the
/// verb. Stages the human's resolution (editing files is the only thing they did),
/// then runs `git rebase --continue` INTERNALLY with the sanctioned sentinel — the
/// guard is never unblocked, no raw git crosses the human's hands. If a later
/// commit in the replay conflicts again, the werk holds again (same instruction).
pub fn commit_continue(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = resolve_trace(card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();
    if !werk.is_dir() {
        return Err(format!("no werk for #{} at {} — pull the card first", card, werk.display()));
    }
    if !rebase_in_progress(&werk_s) {
        jsonl(home, role, card, &trace, "continue.refused", ",\"reason\":\"no-rebase\"");
        return Err(format!("#{}: no rebase in progress — nothing to --continue", card));
    }
    let _lock = lock(home, Duration::from_secs(30))?;
    // stage the human's resolution; rebase --continue requires resolved paths staged.
    run_in(&werk_s, "git", &["add", "-A"])?;
    // GIT_EDITOR=true: keep the replayed commit's original message non-interactively.
    match run_in_env(&werk_s, "git", &["rebase", "--continue"],
        &[("_GIT_QUEUE_INTERNAL", "1"), ("GIT_EDITOR", "true")]) {
        Ok(_) => {
            let sha = run_in(&werk_s, "git", &["rev-parse", "HEAD"])?.trim().to_string();
            jsonl(home, role, card, &trace, "rebase.resolved", &format!(",\"sha\":\"{}\"", sha));
            emit_spine(home, "commit.completed", role, card, &trace,
                &[("sha", &sha), ("via", "continue")]);
            Ok(sha)
        }
        Err(_) if rebase_in_progress(&werk_s) => {
            // a later commit in the replay conflicted — hold again, same contract.
            let files: Vec<String> = run_in(&werk_s, "git", &["diff", "--name-only", "--diff-filter=U"])
                .unwrap_or_default()
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            jsonl(home, role, card, &trace, "rebase.conflict.held",
                &format!(",\"files\":\"{}\"", files.join(",")));
            Err(conflict_hold_message(card, role, &files))
        }
        Err(e) => {
            jsonl(home, role, card, &trace, "continue.failed", "");
            emit_spine(home, "commit.failed", role, card, &trace,
                &[("disposition", "fail"), ("reason", "continue-fail")]);
            Err(format!("--continue failed: {}", e))
        }
    }
}

/// #3304 — `werk-commit <card> <role> --abort`: restore the pre-rebase state through
/// the verb. The card commit is preserved exactly as it was before the rebase began.
pub fn commit_abort(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = resolve_trace(card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();
    if !werk.is_dir() {
        return Err(format!("no werk for #{} at {} — pull the card first", card, werk.display()));
    }
    if !rebase_in_progress(&werk_s) {
        jsonl(home, role, card, &trace, "abort.refused", ",\"reason\":\"no-rebase\"");
        return Err(format!("#{}: no rebase in progress — nothing to --abort", card));
    }
    let _lock = lock(home, Duration::from_secs(30))?;
    run_in_env(&werk_s, "git", &["rebase", "--abort"], &[("_GIT_QUEUE_INTERNAL", "1")])
        .map_err(|e| format!("--abort failed: {}", e))?;
    let sha = run_in(&werk_s, "git", &["rev-parse", "HEAD"])?.trim().to_string();
    jsonl(home, role, card, &trace, "rebase.aborted", &format!(",\"sha\":\"{}\"", sha));
    emit_spine(home, "commit.completed", role, card, &trace,
        &[("sha", &sha), ("via", "abort"), ("disposition", "restored")]);
    Ok(format!("rebase aborted — werk restored to pre-rebase state at {}", sha))
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

/// #3304 — the four verb modes. Flow = commit + rebase (#3186); Atomic = commit
/// without rebase (#3295 escape valve); Continue / Abort = in-verb resolution of a
/// HELD rebase conflict (the guard stays whole — the human never runs raw git).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Flow,
    Atomic,
    Continue,
    Abort,
}

#[derive(Debug)]
pub struct ParsedCommit {
    pub card: u64,
    pub role: Option<String>,
    pub summary: String,
    pub mode: Mode,
}

/// #3304 — the CLI seam (the #3294 pattern: flags recognized anywhere, parsing
/// unit-tested where the contract lives). Modes are mutually exclusive.
pub fn parse_commit_args(raw: &[String]) -> R<ParsedCommit> {
    let usage = "usage: werk-commit <card> <role> [summary] [--atomic | --continue | --abort]";
    let atomic = raw.iter().any(|a| a == "--atomic");
    let cont = raw.iter().any(|a| a == "--continue");
    let abort = raw.iter().any(|a| a == "--abort");
    if [atomic, cont, abort].iter().filter(|b| **b).count() > 1 {
        return Err(format!("--atomic / --continue / --abort are mutually exclusive. {}", usage));
    }
    let mode = if atomic {
        Mode::Atomic
    } else if cont {
        Mode::Continue
    } else if abort {
        Mode::Abort
    } else {
        Mode::Flow
    };
    let pos: Vec<&String> = raw.iter().filter(|a| !a.starts_with("--")).collect();
    let card_arg = pos.first().ok_or_else(|| usage.to_string())?;
    let card: u64 = card_arg
        .parse()
        .map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = pos.get(1).map(|s| s.to_string());
    let summary = if pos.len() > 2 {
        pos[2..].iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ")
    } else {
        String::new()
    };
    Ok(ParsedCommit { card, role, summary, mode })
}

/// #3304 — the held-conflict instruction. Names the conflicted files and BOTH
/// in-verb follow-ups; never instructs raw git (the resolution is reachable only
/// through the verb — Silas's load-bearing constraint).
pub fn conflict_hold_message(card: u64, role: &str, files: &[String]) -> String {
    format!(
        "rebase-conflict HELD for #{}: conflict markers are in your werk — edit the file(s) to \
         resolve, then `werk-commit {} {} --continue` to finish (or `werk-commit {} {} --abort` \
         to restore the pre-rebase state). Conflicted: {}",
        card, card, role, card, role,
        if files.is_empty() { "(unknown)".to_string() } else { files.join(", ") }
    )
}

/// Entry: parse the contract args (`werk-commit <card> <role> [summary...]`, role
/// falls back to $DEPLOY_ROLE) + env, then run the verb in the parsed mode.
pub fn run_commit() -> R<String> {
    let raw: Vec<String> = env::args().skip(1).collect();
    let p = parse_commit_args(&raw)?;
    let role = p
        .role
        .or_else(|| env::var("DEPLOY_ROLE").ok())
        .ok_or_else(|| "usage: werk-commit <card> <role> [summary] [--atomic | --continue | --abort] (or set DEPLOY_ROLE)".to_string())?;
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    match p.mode {
        Mode::Flow => commit(p.card, &role, &p.summary, &home, &werk_base),
        Mode::Atomic => commit_atomic(p.card, &role, &p.summary, &home, &werk_base),
        Mode::Continue => commit_continue(p.card, &role, &home, &werk_base),
        Mode::Abort => commit_abort(p.card, &role, &home, &werk_base),
    }
}

/// Flow entry: commit the werk AND rebase it onto current origin/main (#3186).
pub fn commit(card: u64, role: &str, summary: &str, home: &Path, werk_base: &Path) -> R<String> {
    commit_inner(card, role, summary, home, werk_base, true)
}

/// #3295 — `commit --atomic`: commit the werk WITHOUT the rebase-onto-main step —
/// the escape from the #3223 rebase-conflict deadlock (work is never trapped behind
/// a conflict; resolve the rebase separately). Same pure core as `commit()`, minus
/// the rebase (ADR-037 D#5: one implementation, two entry points).
pub fn commit_atomic(card: u64, role: &str, summary: &str, home: &Path, werk_base: &Path) -> R<String> {
    commit_inner(card, role, summary, home, werk_base, false)
}

/// The whole verb, all inputs explicit so it is testable against a real temp repo
/// (deps injected via PATH: real `git`, shimmed `cards`/`gh`). Returns the commit sha.
/// `rebase` gates the #3186 rebase-onto-current-main step: true = flow (`commit`),
/// false = `--atomic` (`commit_atomic`, commit-without-rebase).
fn commit_inner(card: u64, role: &str, summary: &str, home: &Path, werk_base: &Path, rebase: bool) -> R<String> {
    // #3306 — fail LOUD up front: CHORUS_HOME must be a git repo. Otherwise lock()
    // later dies with a cryptic `os error 2` (and the pre-#3306 binary silently exited
    // 0 + emitted nothing — self-masking, the reason #3295 looked broken but wasn't).
    // Verbs already check CHORUS_HOME is SET; this adds the is-a-git-repo check. The
    // spine lives under home, so when home itself is bad the loud Err + non-zero exit
    // IS the signal (we cannot log to a home that has no spine).
    if !home.join(".git").exists() {
        return Err(format!(
            "CHORUS_HOME is not a git repo: {} has no .git — point CHORUS_HOME at the chorus repo root",
            home.display()
        ));
    }
    let trace = resolve_trace(card); // #3162 — inherit the shared trace, not fresh-mint
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "commit.started", "");
    emit_spine(home, "commit.started", role, card, &trace, &[]);

    // #3012/#3013 fix: deterministic werk, REFUSE if absent. No canonical fallback.
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "commit.refused", ",\"reason\":\"no-werk\"");
        emit_spine(home, "commit.refused", role, card, &trace, &[("disposition", "refuse"), ("reason", "no-werk")]);
        return Err(format!("no werk for #{} at {} — pull the card first", card, werk.display()));
    }
    // werk must be on the card's branch (guards same-role wrong-card, #2641 class).
    let cur = run_in(&werk_s, "git", &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    if cur != branch {
        jsonl(home, role, card, &trace, "commit.refused", ",\"reason\":\"wrong-branch\"");
        emit_spine(home, "commit.refused", role, card, &trace, &[("disposition", "refuse"), ("reason", "wrong-branch")]);
        return Err(format!("werk is on '{}', not '{}' — refusing to commit", cur, branch));
    }

    // #3186: refresh origin/main so the rebase below targets CURRENT main, not the
    // stale pull-time ref. Best-effort — a LOCAL commit should not hard-require the
    // network; if fetch fails we rebase onto the freshest known main and the
    // downstream refuse-if-stale guard (werk-deploy / werk-merge) is the backstop.
    // --atomic skips the fetch+rebase entirely (commit-without-rebase).
    if rebase && run_in(&werk_s, "git", &["fetch", "-q", "origin", "main"]).is_err() {
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
        // #3162 — this refusal was fully SILENT (the bug). Surface it on the spine + jsonl.
        jsonl(home, role, card, &trace, "commit.refused", ",\"reason\":\"nothing-to-commit\"");
        emit_spine(home, "commit.refused", role, card, &trace, &[("disposition", "refuse"), ("reason", "nothing-to-commit")]);
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
                .map_err(|e| {
                    // #3162 — the pre-commit gate block was fully SILENT (the other key
                    // bug). Surface commit.failed on the spine before bubbling the error.
                    jsonl(home, role, card, &trace, "commit.failed", ",\"reason\":\"pre-commit-gate\"");
                    emit_spine(home, "commit.failed", role, card, &trace, &[("disposition", "fail"), ("reason", "pre-commit-gate")]);
                    format!("commit failed (pre-commit gate?): {}", e)
                })?;
            jsonl(home, role, card, &trace, "committed", "");
        }

        // #3186 — close the pull->commit staleness window: rebase the card's commit(s)
        // onto current origin/main. all-or-nothing — conflict aborts cleanly and refuses
        // (werk preserved). This makes everything downstream current by construction.
        // --atomic (#3295) skips this: commit-without-rebase, the #3223 deadlock escape.
        if rebase {
            rebase_onto_origin_main(&werk_s, home, role, card, &trace)?;
        }
    } // flock released

    // Commit only — the atomic verb stops here. The commit is LOCAL: not pushed,
    // and it gets NO gh status, because gh can't see an unpushed sha. That honest
    // gap is the point of the split — "committed but not pushed" is now a distinct,
    // visible state, not hidden inside a fused commit+push. Pushing + the
    // chorus/push gh status is the separate werk-push verb.
    let sha = run_in(&werk_s, "git", &["rev-parse", "HEAD"])?.trim().to_string();
    jsonl(home, role, card, &trace, "commit.completed", &format!(",\"sha\":\"{}\"", sha));
    emit_spine(home, "commit.completed", role, card, &trace, &[("sha", &sha)]);
    Ok(sha)
}
