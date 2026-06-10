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

/// Shared trace per ADR-032 §3 (#3135 CONSISTENT): resolve by precedence —
/// `CHORUS_TRACE_ID` env → `<trace_dir>/<card>-trace` file → mint-and-persist
/// (write the file so downstream verbs inherit). The persisted file is the
/// cross-process carrier that threads ONE trace across pull → demo → acp,
/// replacing pull's #3063 fresh-mint drift. `resolve_trace_in` is the testable
/// core (trace dir injected, all inputs explicit); `resolve_trace` is the real
/// entry (env + /tmp). Mirrors werk-build's resolve_trace.
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

/// The spine event contract (#3135 AUDITABLE): the args handed to `chorus-log` so
/// a verb's lifecycle is queryable in Loki, keyed by card + trace. Pure → testable.
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

/// Emit one lifecycle event to the ONE spine (`~/.chorus/chorus.log`) via the
/// canonical `chorus-log` subprocess — best-effort, like the jsonl witness, never
/// blocks the verb. Mirrors werk-demo / werk-accept. The jsonl witness stays
/// (verbose local trace); the spine is the Loki-queryable record (#3135 replaces
/// werk-pull's jsonl-witness-only, which promtail never tailed).
fn emit_spine(home: &Path, event: &str, role: &str, card: u64, trace: &str, extras: &[(&str, &str)]) {
    let log = home.join("platform/scripts/chorus-log");
    let log_s = match path(&log) {
        Ok(s) => s,
        Err(_) => return,
    };
    let args = spine_args(event, role, card, trace, extras);
    let mut argv: Vec<&str> = vec![log_s];
    argv.extend(args.iter().map(|s| s.as_str()));
    let _ = run("bash", &argv);
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

/// Resolve a chorus script (cards, role-state, ...) to its absolute path under
/// $CHORUS_HOME/platform/scripts. #3151: werk-pull is exec'd by the chorus-mcp
/// daemon, whose PATH does NOT include platform/scripts, so bare-name lookups
/// ("cards", "role-state") failed with "No such file or directory" — breaking
/// /pull team-wide after #3135. Resolve them absolutely from `home`, the same
/// hermetic pattern emit_spine uses for chorus-log, so werk-pull never depends on
/// the caller's PATH. (git/gh stay bare — they're on the system PATH.)
fn script(home: &Path, name: &str) -> String {
    home.join("platform/scripts").join(name).to_string_lossy().into_owned()
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
    // #3161: a rollback is a real failure — emit it to the ONE spine (not just the
    // untailed jsonl witness) so it's Loki-queryable and counts in the #3165 rollup.
    emit_spine(home, "pull.rolledback", role, card, trace, &[("disposition", "rollback"), ("reason", reason)]);
    if let Ok(_lock) = lock(home, Duration::from_secs(30)) {
        let _ = run("git", &["-C", home_s, "worktree", "remove", "--force", werk_s]);
        let _ = run("git", &["-C", home_s, "branch", "-D", branch]);
        // no remote-branch delete: pull never pushes, so there's no remote ref.
    }
    // if the card was already moved to WIP, put it back where it was (all-or-nothing).
    if let Some(s) = restore_status {
        let _ = run(&script(home, "cards"), &["move", &card.to_string(), s]);
    }
}

/// Entry: parse the contract args (`werk-pull <card> <role>`, role falls back to
/// $DEPLOY_ROLE for substrate callers) + env, then run the verb.
///
/// #3294 — `parse_pull_args` recognizes `--atomic` ANYWHERE (the standalone-worktree
/// door; the push/merge/accept seam pattern). pull is the ADR-037 --atomic-FREE group
/// (local/reversible worktree creation, no approval), so --atomic is recognized but
/// non-branching. Pure + testable: run_pull feeds it env::args, so the CLI seam is
/// covered, not just the lib fn.
pub fn parse_pull_args(args: &[String], deploy_role: Option<String>) -> R<(u64, String, bool)> {
    let atomic = args.iter().any(|a| a == "--atomic");
    let pos: Vec<&String> = args.iter().filter(|a| a.as_str() != "--atomic").collect();
    let card_arg = pos
        .first()
        .ok_or_else(|| "usage: werk-pull <card> <role> [--atomic]".to_string())?;
    let card: u64 = card_arg
        .parse()
        .map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = pos
        .get(1)
        .map(|s| s.to_string())
        .or(deploy_role)
        .ok_or_else(|| "usage: werk-pull <card> <role> [--atomic] (or set DEPLOY_ROLE)".to_string())?;
    Ok((card, role, atomic))
}

/// Conforms to ADR-032 (zero-dep Rust, flock, spine emit, all-or-nothing rollback) +
/// ADR-037 (--atomic; pull = free group, local/reversible worktree creation, no approval).
pub fn run_pull() -> R<String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let (card, role, _atomic) = parse_pull_args(&args, env::var("DEPLOY_ROLE").ok())?;
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    pull(card, &role, &home, &werk_base)
}

/// The whole verb, all inputs explicit so it is testable against a real temp
/// repo (deps injected via PATH: real `git`, `cards`). No gh, no store: the
/// worktree + board ARE the state; this only writes them.
pub fn pull(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = resolve_trace(card); // #3135: thread one trace across pull→demo→acp (was fresh-mint)
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
    let cj = match run(&script(home, "cards"), &["view", &card.to_string(), "--json"]) {
        Ok(s) => s,
        Err(e) => {
            // #3161: card-not-found is a refusal — emit to the spine, not silence.
            jsonl(home, role, card, &trace, "pull.refused", ",\"reason\":\"card-not-found\"");
            emit_spine(home, "pull.refused", role, card, &trace, &[("disposition", "refuse"), ("reason", "card-not-found")]);
            return Err(format!("card #{} not viewable: {}", card, e));
        }
    };
    let status = json_str_field(&cj, "status").unwrap_or_default();
    if status != "Next" && status != "Later" {
        jsonl(home, role, card, &trace, "pull.refused", ",\"reason\":\"wrong-status\"");
        // #3161: refusal to the ONE spine (was jsonl-witness-only, invisible to Loki).
        emit_spine(home, "pull.refused", role, card, &trace, &[("disposition", "refuse"), ("reason", "wrong-status")]);
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
    if let Err(e) = run(&script(home, "cards"), &["move", &card.to_string(), "WIP"]) {
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

    // role-state: declare building (parity with the live card-pull, which werk-pull
    // lacked). Best-effort — a local status declaration; its failure doesn't unwind
    // a pull that already moved the board + made the werk.
    let _ = run(&script(home, "role-state"), &[role, "building"]);

    jsonl(home, role, card, &trace, "pull.completed", &format!(",\"branch\":\"{}\"", branch));
    // #3135 AUDITABLE: card.pulled to the ONE spine (Loki-queryable), carrying the
    // shared trace so this pull correlates with the card's demo + acp.
    emit_spine(home, "card.pulled", role, card, &trace, &[]);
    Ok(branch)
}
