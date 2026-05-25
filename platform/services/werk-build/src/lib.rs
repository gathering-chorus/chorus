//! werk-build — `/werk/build` v2 logic (card #3061).
//!
//! Self-contained: std only; calls `git`, `gh`, and `build-signed.sh` as
//! subprocesses. No dependency on any other chorus code (ADR-032 §1 blueprint;
//! mirrors werk-pull #3045).
//!
//! Recompile + sign a card's crate IN THE WERK and emit its cdhash.
//! - NO install, no system mutation, no rollback — build is non-mutating
//!   (BUILD_SKIP_INSTALL=1). All install/slot logic lives in werk-deploy (#3062);
//!   build is slot-agnostic (the cdhash is the contract between them).
//! - Build-invariance: same source commit -> same cdhash (the deploy/verify gate).
//! - Refuses if the `<role>/<card>` werk/branch doesn't exist; never builds canonical.
//! - JSONL witness per step: best-effort, NEVER affects the operation.

use std::collections::BTreeSet;
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

/// Shared trace per ADR-032 §3: CHORUS_TRACE_ID env -> /tmp/<card>-trace file ->
/// mint-and-persist (write the file so downstream verbs inherit). The file is the
/// cross-process carrier (verbs are separate processes). Unlike werk-pull's
/// fresh-mint (its #3063 drift), this threads one trace across pull->...->accept.
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

pub fn jsonl_line(ts: u128, event: &str, role: &str, card: u64, trace: &str, extra: &str) -> String {
    format!(
        "{{\"ts\":{},\"event\":\"{}\",\"role\":\"{}\",\"card_id\":{},\"trace_id\":\"{}\"{}}}\n",
        ts, event, role, card, trace, extra
    )
}

/// Map a changed repo path to its owning Rust crate name, or None.
/// werk-build's scope is signed Rust crates under platform/services/<crate>/.
/// (chorus-api is TypeScript — npm build, no cdhash — a separate path.)
pub fn crate_for_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("platform/services/")?;
    let name = rest.split('/').next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

// --- side-effecting helpers ---

fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-build.jsonl");
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let line = jsonl_line(ts, event, role, card, trace, extra);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Run a CLI, capture stdout; any non-zero exit is a typed error.
fn run(cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!("{} {}: {}", cmd, args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Like `run`, with an explicit working dir + extra env (for BUILD_SKIP_INSTALL
/// and so `gh`/`build-signed.sh` resolve the repo + crate from the werk).
fn run_in_env(dir: &str, envs: &[(&str, &str)], cmd: &str, args: &[&str]) -> R<String> {
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
    // build-signed.sh prints cdhash on stdout; callers may need stderr too.
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    ))
}

/// flock guard — auto-releases on drop (and on process exit/crash, kernel-level).
pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

/// Lock the WERK's build (not canonical) — two concurrent cargo builds on one
/// target dir race. Build never touches canonical, so the lock lives in the werk.
pub fn lock(werk: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = werk.join(".git-build.lock");
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
            return Err("another build holds the werk lock (timed out after 60s)".to_string());
        }
        sleep(Duration::from_millis(100));
    }
}

fn path(p: &Path) -> R<&str> {
    p.to_str().ok_or_else(|| format!("non-utf8 path: {}", p.display()))
}

/// Extract the cdhash from build-signed.sh output (`build-signed: cdhash=<hash>`).
pub fn extract_cdhash(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Some(rest) = line.trim().strip_prefix("build-signed: cdhash=") {
            let h = rest.trim();
            if !h.is_empty() {
                return Some(h.to_string());
            }
        }
    }
    None
}

/// Which Rust crates did this card change (vs origin/main)? Empty => nothing to build.
fn detect_crates(werk_s: &str) -> R<Vec<String>> {
    let diff = run("git", &["-C", werk_s, "diff", "origin/main", "--name-only"])?;
    let mut crates: BTreeSet<String> = BTreeSet::new();
    for line in diff.lines() {
        if let Some(c) = crate_for_path(line.trim()) {
            crates.insert(c);
        }
    }
    Ok(crates.into_iter().collect())
}

/// (identifier, binary-name) for a crate. Mirrors build-signed.sh `resolve_crate`
/// for the signed crates; derives a sensible default otherwise. Needed because the
/// 1-arg `build-signed.sh <crate>` form resolves crate_dir against build-signed.sh's
/// OWN ($ROOT) canonical root — so it builds CANONICAL, not the werk. werk-build must
/// use the 3-arg `<crate-dir> <identifier> <binary>` form with the WERK's crate dir,
/// or "build the card's changes before merge" is defeated (it'd build unchanged main).
pub fn crate_spec(crate_name: &str) -> (String, String) {
    match crate_name {
        "chorus-hooks" => ("com.chorus.hook-shim".to_string(), "chorus-hook-shim".to_string()),
        "chorus-inject" => ("com.chorus.inject".to_string(), "chorus-inject".to_string()),
        other => {
            // convention: binary == crate name; identifier == com.chorus.<name w/o chorus- prefix>.
            let suffix = other.strip_prefix("chorus-").unwrap_or(other);
            (format!("com.chorus.{}", suffix), other.to_string())
        }
    }
}

/// Build + sign ONE crate IN THE WERK, build-only, return its cdhash.
/// 3-arg build-signed.sh form against the werk's crate dir (not $ROOT/canonical);
/// BUILD_SKIP_INSTALL=1 (the flag exists per #2774) — no install, no mutation.
fn build_crate(werk_s: &str, crate_name: &str) -> R<String> {
    let crate_dir = format!("{}/platform/services/{}", werk_s, crate_name);
    let (identifier, binary) = crate_spec(crate_name);
    let out = run_in_env(
        werk_s,
        &[("BUILD_SKIP_INSTALL", "1")],
        "build-signed.sh",
        &[&crate_dir, &identifier, &binary],
    )?;
    extract_cdhash(&out)
        .ok_or_else(|| format!("build of {} produced no cdhash (build-signed.sh output had no 'cdhash=' line)", crate_name))
}

/// gh: set chorus/build/<card> success on the werk HEAD carrying the cdhash, and
/// carry prior chorus/*/<card> statuses forward onto this SHA (ADR-032 §5).
/// Best-effort: gh failure does not fail a successful build (build is non-mutating;
/// the cdhash is already produced — gh is observability of the process state).
fn register_gh(werk_s: &str, card: u64, role: &str, trace: &str, cdhash: &str) {
    let sha = match run("git", &["-C", werk_s, "rev-parse", "HEAD"]) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return,
    };
    let endpoint = format!("repos/{{owner}}/{{repo}}/statuses/{}", sha);
    let desc = format!("role={} trace={} cdhash={} status=built", role, trace, cdhash);
    let _ = run_in_env(
        werk_s,
        &[],
        "gh",
        &[
            "api",
            &endpoint,
            "-f",
            "state=success",
            "-f",
            &format!("context=chorus/build/{}", card),
            "-f",
            &format!("description={}", desc),
        ],
    );
    // carry-forward: re-apply any chorus/*/<card> statuses from origin/main's HEAD
    // (where pull set its status pre-commit) onto this new SHA, so the whole chain
    // reads on one commit. Best-effort, never fatal.
    carry_forward(werk_s, card, &sha);
}

/// Re-apply prior chorus/*/<card> commit-statuses onto `sha` (ADR-032 §5).
fn carry_forward(werk_s: &str, card: u64, sha: &str) {
    let base = match run("git", &["-C", werk_s, "rev-parse", "origin/main"]) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return,
    };
    if base == sha {
        return;
    }
    let listing = run_in_env(
        werk_s,
        &[],
        "gh",
        &["api", &format!("repos/{{owner}}/{{repo}}/commits/{}/statuses", base)],
    )
    .unwrap_or_default();
    let needle = format!("/{}", card);
    // crude scan for our card's chorus/* contexts already present on base.
    for ctx_line in listing.split('"') {
        if ctx_line.starts_with("chorus/") && ctx_line.ends_with(&needle) && ctx_line != format!("chorus/build/{}", card) {
            let endpoint = format!("repos/{{owner}}/{{repo}}/statuses/{}", sha);
            let _ = run_in_env(
                werk_s,
                &[],
                "gh",
                &[
                    "api",
                    &endpoint,
                    "-f",
                    "state=success",
                    "-f",
                    &format!("context={}", ctx_line),
                    "-f",
                    "description=carried-forward",
                ],
            );
        }
    }
}

/// Entry: parse `werk-build <card> <role>` (role falls back to $DEPLOY_ROLE).
pub fn run_build() -> R<String> {
    let card_arg = env::args().nth(1).ok_or_else(|| "usage: werk-build <card> <role>".to_string())?;
    let card: u64 = card_arg.parse().map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = env::args()
        .nth(2)
        .or_else(|| env::var("DEPLOY_ROLE").ok())
        .ok_or_else(|| "usage: werk-build <card> <role> (or set DEPLOY_ROLE)".to_string())?;
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    build(card, &role, &home, &werk_base)
}

/// The whole verb, all inputs explicit so it is testable against a real temp repo
/// (deps injected via PATH: real `git`, shimmed `build-signed.sh`/`gh`).
/// Returns a comma-joined `crate=cdhash` summary.
pub fn build(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = resolve_trace(card);
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "build.started", "");

    // no-werk-refuse guard (ADR-032 §4): never build canonical.
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "build.refused", ",\"reason\":\"no-werk\"");
        return Err(format!("no werk at {} — pull #{} first (build never touches canonical)", werk.display(), card));
    }
    let cur = run("git", &["-C", &werk_s, "rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    if cur.trim() != branch {
        jsonl(home, role, card, &trace, "build.refused", ",\"reason\":\"branch-mismatch\"");
        return Err(format!("werk {} is on '{}', not '{}'", werk.display(), cur.trim(), branch));
    }

    let crates = detect_crates(&werk_s)?;
    if crates.is_empty() {
        jsonl(home, role, card, &trace, "build.refused", ",\"reason\":\"no-crate-changed\"");
        return Err(format!("card #{} changed no Rust crate under platform/services/ — nothing to build", card));
    }

    // one lock around all the crate builds (cargo can't race the target dir).
    let _lock = lock(&werk, Duration::from_secs(60))?;
    jsonl(home, role, card, &trace, "lock.acquired", "");

    let mut summary: Vec<String> = Vec::new();
    for c in &crates {
        jsonl(home, role, card, &trace, "crate.build.started", &format!(",\"crate\":\"{}\"", c));
        let cdhash = build_crate(&werk_s, c)?;
        jsonl(home, role, card, &trace, "crate.build.completed", &format!(",\"crate\":\"{}\",\"cdhash\":\"{}\"", c, cdhash));
        // gh status per crate (last write wins for the chorus/build/<card> context;
        // the summary carries every crate=cdhash).
        register_gh(&werk_s, card, role, &trace, &cdhash);
        summary.push(format!("{}={}", c, cdhash));
    }

    let joined = summary.join(",");
    jsonl(home, role, card, &trace, "build.completed", &format!(",\"built\":\"{}\"", joined));
    Ok(joined)
}
