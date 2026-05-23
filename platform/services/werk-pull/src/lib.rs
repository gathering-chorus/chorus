//! werk-pull — `/werk/pull` v2 logic (card #3045).
//!
//! Self-contained: std only + a direct libc `flock` extern; calls `git` and
//! `cards` CLIs as subprocesses. No dependency on any other chorus code.
//!
//! Pull a card -> fresh isolated worktree off origin/main on branch <role>/<card>.
//! - Lock: one flock around the canonical-touching git steps (fetch + worktree add).
//! - Atomic on git+cards: if the board move fails, the worktree is rolled back.
//! - JSONL log per step: best-effort, NEVER affects the operation (not transactional).
//! - Every error handled; fail-loud preconditions; no half-pulled state.

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

/// JSONL: append one line to a local file. Best-effort, swallows its own errors,
/// so logging can NEVER affect the operation. Borg ingests this file downstream.
fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-pull.jsonl");
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let line = jsonl_line(ts, event, role, card, trace, extra);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Run a CLI, capture stdout; any non-zero exit is a typed error (no silent failure).
fn run(cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
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

/// Like `run`, but with an explicit working directory — so `gh` can infer the
/// repo from the worktree's remote for {owner}/{repo} substitution.
fn run_in(dir: &str, cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
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

/// flock guard — auto-releases on drop (and on process exit/crash, kernel-level).
pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

pub fn lock(home: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = home.join(".git/chorus-pull.lock");
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .open(&p)
        .map_err(|e| format!("cannot open lock {}: {}", p.display(), e))?;
    let start = Instant::now();
    loop {
        if unsafe { flock(f.as_raw_fd(), LOCK_EX_NB) } == 0 {
            return Ok(FlockGuard(f));
        }
        if start.elapsed() >= timeout {
            return Err("another pull holds the repo lock (timed out after 30s)".to_string());
        }
        sleep(Duration::from_millis(100));
    }
}

fn path(p: &Path) -> R<&str> {
    p.to_str().ok_or_else(|| format!("non-utf8 path: {}", p.display()))
}

/// Minimal whitespace-tolerant extractor for a JSON string field: finds `"key"`,
/// then the next quoted value. Zero-dep. The brittle alternative (substring match
/// on `"key":"val"`) broke against real `cards --json` pretty-printing (a space
/// after the colon), which a compact shim hid — found by the real run.
fn json_str_field(json: &str, key: &str) -> Option<String> {
    let i = json.find(&format!("\"{}\"", key))?;
    let after_key = &json[i + key.len() + 2..];
    let colon = after_key.find(':')?;
    let after_colon = &after_key[colon + 1..];
    let q1 = after_colon.find('"')?;
    let val = &after_colon[q1 + 1..];
    let q2 = val.find('"')?;
    Some(val[..q2].to_string())
}

/// Register the card's pipeline state in gh — the process holder (v6 diagram).
/// Writes {card#, role, trace, branch, status} as a commit-status on the branch
/// HEAD (already on GitHub). No push (see body). Queryable by card-specific context.
fn register_gh(werk_s: &str, card: u64, role: &str, trace: &str, branch: &str) -> R<()> {
    // NO git push. At pull the branch has no commits — it's origin/main's HEAD,
    // already on GitHub. gh state is an API write onto that existing commit; the
    // branch gets pushed later (at acp) through the sanctioned path, when there's
    // real work on it. Per-card isolation comes from the card-specific context.
    let sha = run("git", &["-C", werk_s, "rev-parse", "HEAD"])?.trim().to_string();
    let endpoint = format!("repos/{{owner}}/{{repo}}/statuses/{}", sha);
    let desc = format!("role={} trace={} branch={} status=pulled", role, trace, branch);
    run_in(
        werk_s,
        "gh",
        &[
            "api",
            &endpoint,
            "-f",
            "state=pending",
            "-f",
            &format!("context=chorus/pull/{}", card),
            "-f",
            &format!("description={}", desc),
        ],
    )?;
    Ok(())
}

/// Undo everything pull created, best-effort, under the lock. All-or-nothing:
/// no half-made worktree, no orphan local branch, no orphan remote ref.
fn rollback(
    home: &Path,
    home_s: &str,
    werk_s: &str,
    branch: &str,
    role: &str,
    card: u64,
    trace: &str,
    reason: &str,
    restore_status: Option<&str>,
) {
    jsonl(home, role, card, trace, "pull.rolledback", &format!(",\"reason\":\"{}\"", reason));
    if let Ok(_lock) = lock(home, Duration::from_secs(30)) {
        let _ = run("git", &["-C", home_s, "worktree", "remove", "--force", werk_s]);
        let _ = run("git", &["-C", home_s, "branch", "-D", branch]);
        // no remote-branch delete: pull never pushes, so there's no remote ref.
    }
    // if the card was already moved to WIP, put it back where it was (all-or-nothing).
    if let Some(s) = restore_status {
        let _ = run("cards", &["move", &card.to_string(), s]);
    }
}

/// Entry: parse the contract args (`werk-pull <card> <role>`, role falls back to
/// $DEPLOY_ROLE for substrate callers) + env, then run the verb.
pub fn run_pull() -> R<String> {
    let card_arg = env::args().nth(1).ok_or_else(|| "usage: werk-pull <card> <role>".to_string())?;
    let card: u64 = card_arg
        .parse()
        .map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = env::args()
        .nth(2)
        .or_else(|| env::var("DEPLOY_ROLE").ok())
        .ok_or_else(|| "usage: werk-pull <card> <role> (or set DEPLOY_ROLE)".to_string())?;
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    pull(card, &role, &home, &werk_base)
}

/// The whole verb, all inputs explicit so it is testable against a real temp
/// repo (deps injected via PATH: real `git`, `cards`). No gh, no store: the
/// worktree + board ARE the state; this only writes them.
pub fn pull(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = trace_id();
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let home_s = path(home)?.to_string();
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "pull.started", "");

    // idempotent: already pulled on the right branch -> no-op success.
    if werk.is_dir() {
        let cur = run("git", &["-C", &werk_s, "rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
        if cur.trim() == branch {
            jsonl(home, role, card, &trace, "pull.idempotent", "");
            return Ok(branch);
        }
        return Err(format!("werk {} exists on '{}', not '{}'", werk.display(), cur.trim(), branch));
    }

    // card must exist and be pullable.
    let cj = run("cards", &["view", &card.to_string(), "--json"])
        .map_err(|e| format!("card #{} not viewable: {}", card, e))?;
    let status = json_str_field(&cj, "status").unwrap_or_default();
    if status != "Next" && status != "Later" {
        jsonl(home, role, card, &trace, "pull.refused", ",\"reason\":\"wrong-status\"");
        return Err(format!("card #{} is in '{}', not Next/Later", card, status));
    }

    // --- canonical-touching steps, serialized under one flock ---
    {
        let _lock = lock(home, Duration::from_secs(30))?;
        jsonl(home, role, card, &trace, "lock.acquired", "");
        run("git", &["-C", &home_s, "fetch", "-q", "origin", "main"])?;
        jsonl(home, role, card, &trace, "fetch.done", "");
        run("git", &["-C", &home_s, "worktree", "add", "-b", &branch, &werk_s, "origin/main"])?;
        jsonl(home, role, card, &trace, "worktree.added", "");
    } // flock released here

    // node_modules — best-effort, not transactional.
    let nm = home.join("node_modules");
    if nm.exists() {
        let _ = std::os::unix::fs::symlink(&nm, werk.join("node_modules"));
    }

    // board move -> WIP (the lifecycle transition), then gh registration. If gh
    // fails, the whole pull rolls back (all-or-nothing): gh is the authoritative
    // process-state per v6, so a pull that can't record itself didn't happen.
    // Tradeoff (accepted): gh is on pull's critical path — a GitHub outage fails
    // the pull. Consistency over availability, consistent with gh-as-authoritative.
    if let Err(e) = run("cards", &["move", &card.to_string(), "WIP"]) {
        // card never moved; nothing to restore, just undo the worktree.
        rollback(home, &home_s, &werk_s, &branch, role, card, &trace, "cards-move-fail", None);
        return Err(format!("board move failed; rolled back: {}", e));
    }
    jsonl(home, role, card, &trace, "card.wip", "");

    // register the card in gh — the PROCESS HOLDER (v6 diagram). LAST: most fragile
    // (remote). On failure, all-or-nothing restores the card's prior status too.
    if let Err(e) = register_gh(&werk_s, card, role, &trace, &branch) {
        rollback(home, &home_s, &werk_s, &branch, role, card, &trace, "gh-register-fail", Some(&status));
        return Err(format!("gh registration failed; rolled back: {}", e));
    }
    jsonl(home, role, card, &trace, "gh.registered", "");

    jsonl(home, role, card, &trace, "pull.completed", &format!(",\"branch\":\"{}\"", branch));
    Ok(branch)
}
