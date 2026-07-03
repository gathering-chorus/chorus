//! werk-sync — canonical recovery as a verb (#3300, ADR-038 port of the Bash
//! `chorus-werk-sync`; ADR-032 §1 blueprint: zero-dep std-only Rust, flock,
//! subprocess git, spine emits, typed refusals).
//!
//! Since #2863 canonical SYNC is automatic inside the build path; this verb is the
//! MANUAL recovery surface for when that aborts:
//! - `werk-sync repair`  — re-attach a detached canonical HEAD to refs/heads/main via
//!   git PLUMBING (symbolic-ref / update-ref / read-tree, bypassing blocked porcelain),
//!   fast-forward to origin/main, align the working tree. Emits `canonical.repaired`.
//! - `werk-sync recover` — #2909: losslessly stash every dirty (M/A/D, NOT untracked)
//!   non-union file to ~/.chorus/recovery/<ts>/<path-hash> with a manifest.tsv, restore
//!   the tree from HEAD, then fetch + ff-only to origin/main. Emits
//!   `canonical.recovery.stashed` per file + `canonical.recovery.completed`.
//!
//! Behavior parity with the Bash is the contract (#3300 AC2): same lock file
//! (`$CHORUS_HOME/.git-commit.lock` — cross-tool exclusion with the commit path),
//! same recovery layout, same event names, same untracked/union exclusions.

use std::env;
use std::fs::{self, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}
const LOCK_EX_NB: i32 = 0x02 | 0x04;
const LOCK_UN: i32 = 0x08;

pub type R<T> = Result<T, String>;

// --- pure helpers (unit-tested) ---

/// The two verb modes. There is deliberately NO default: bare invocation is a
/// usage error (sync-proper retired by #2863 — the build path owns it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Repair,
    Recover,
}

pub const USAGE: &str = "usage: werk-sync {repair|recover}\n  repair   — re-attach HEAD to main, ff to origin/main (detached-HEAD recovery)\n  recover  — auto-stash dirty files to ~/.chorus/recovery/<ts>/, then ff to origin/main (#2909)\n  Sync is automatic inside the build path (#2863). Use these only when it aborts.";

/// #3300 — the CLI seam (the #3294 pattern: parsing pure + unit-tested).
pub fn parse_sync_args(args: &[String]) -> R<Mode> {
    match args.first().map(|s| s.as_str()) {
        Some("repair") => Ok(Mode::Repair),
        Some("recover") => Ok(Mode::Recover),
        _ => Err(USAGE.to_string()),
    }
}

/// Parse `git status --porcelain` into the stash set: entries whose FIRST status
/// char is one of ` MAD` (Bash parity: `grep -E '^[ MAD]'`). Untracked (`??`) is
/// deliberately EXCLUDED — recover preserves tracked work; untracked files survive
/// an ff-merge untouched. Returns (porcelain-2-char-status, path).
pub fn dirty_paths(porcelain: &str) -> Vec<(String, String)> {
    porcelain
        .lines()
        .filter(|l| l.len() > 3)
        .filter(|l| matches!(l.as_bytes()[0], b' ' | b'M' | b'A' | b'D'))
        .map(|l| (l[..2].to_string(), l[3..].to_string()))
        .collect()
}

/// One manifest.tsv row: `hash<TAB>original_path` (Bash parity — a human recovers
/// by joining the hash-named file in the recovery dir back to its repo path).
pub fn manifest_line(hash: &str, path: &str) -> String {
    format!("{}\t{}\n", hash, path)
}

/// The chorus-log positional contract (role is always `system` — parity with the
/// Bash emit()). Pure so the spine wiring is testable without a subprocess.
pub fn spine_args(event: &str, extras: &[(&str, &str)]) -> Vec<String> {
    let mut v = vec![event.to_string(), "system".to_string()];
    for (k, val) in extras {
        v.push(format!("{}={}", k, val));
    }
    v
}

// --- side-effecting helpers ---

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

/// Best-effort spine emit via the canonical chorus-log (absolute home path — the
/// #3151 daemon-PATH lesson). A missing/failing chorus-log never affects recovery.
/// CHORUS_LOG_BIN overrides the path — the same stub contract nightly-suites.sh
/// documents, so hermetic tests can capture emits without a real canonical tree
/// (test-chorus-werk-sync-auto-repair.sh's `canonical.repaired` red, #3606).
fn emit(home: &Path, event: &str, extras: &[(&str, &str)]) {
    let log = env::var_os("CHORUS_LOG_BIN")
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .unwrap_or_else(|| home.join("platform/scripts/chorus-log"));
    if !log.exists() {
        return;
    }
    let mut c = Command::new("bash");
    c.arg(&log);
    for a in spine_args(event, extras) {
        c.arg(a);
    }
    let _ = c.output();
}

/// flock guard on the SAME lock file the Bash used (`.git-commit.lock`) so
/// werk-sync excludes against every other canonical-touching git op.
pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

pub fn lock(home: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = home.join(".git-commit.lock");
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
            return Err("another git op holds .git-commit.lock (timed out after 30s)".to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn require_git_repo(home: &Path) -> R<String> {
    if !home.join(".git").exists() {
        return Err(format!(
            "CHORUS_HOME={} is not a git repo — point CHORUS_HOME at the chorus repo root",
            home.display()
        ));
    }
    home.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("non-utf8 path: {}", home.display()))
}

/// `werk-sync repair` — detached/corrupted canonical → attached at origin/main.
/// PLUMBING only (symbolic-ref / update-ref / read-tree): porcelain checkout is
/// guard-blocked, and plumbing also works from states porcelain refuses.
pub fn repair(home: &Path) -> R<String> {
    let home_s = require_git_repo(home)?;
    let from = run_in(&home_s, "git", &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "?".to_string());

    let _lock = lock(home, Duration::from_secs(30))?;
    run_in(&home_s, "git", &["symbolic-ref", "HEAD", "refs/heads/main"])
        .map_err(|e| format!("repair failed: could not set HEAD → refs/heads/main: {}", e))?;
    let _ = run_in(&home_s, "git", &["fetch", "--quiet", "origin", "main"]);
    if let Ok(origin_main) = run_in(&home_s, "git", &["rev-parse", "origin/main"]) {
        let origin_main = origin_main.trim();
        run_in(&home_s, "git", &["update-ref", "refs/heads/main", origin_main])
            .map_err(|e| format!("repair failed: could not update refs/heads/main → {}: {}", origin_main, e))?;
    }
    run_in(&home_s, "git", &["read-tree", "-u", "--reset", "HEAD"])
        .map_err(|e| format!("repair failed: could not align working tree to HEAD: {}", e))?;

    let to = run_in(&home_s, "git", &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "?".to_string());
    emit(home, "canonical.repaired", &[("from", &from), ("to", &to)]);
    if from == to {
        Ok(format!("werk-sync: repair — already on main at {} (no-op)", from))
    } else {
        Ok(format!("werk-sync: repair — {} → {} (HEAD attached to main)", from, to))
    }
}

fn path_hash(home_s: &str, path: &str) -> String {
    // shasum subprocess (Bash parity) — zero-dep rules out an in-crate sha256.
    let out = Command::new("shasum")
        .args(["-a", "256"])
        .arg("/dev/stdin")
        .current_dir(home_s)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            if let Some(si) = c.stdin.as_mut() {
                let _ = si.write_all(path.as_bytes());
            }
            c.wait_with_output()
        });
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .split_whitespace()
            .next()
            .unwrap_or("")
            .chars()
            .take(12)
            .collect(),
        // hash failure must not lose work — fall back to a sanitized path name.
        _ => path.replace('/', "_").chars().take(48).collect(),
    }
}

fn file_sha(p: &Path) -> String {
    Command::new("shasum")
        .args(["-a", "256"])
        .arg(p)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default()
}

/// `werk-sync recover` — #2909: stash dirty tracked files losslessly, then ff.
/// `recovery_base`/`ts` are injected so the e2e is hermetic (no $HOME mutation).
pub fn recover(home: &Path, recovery_base: &Path, ts: &str) -> R<String> {
    let home_s = require_git_repo(home)?;
    let recovery_dir = recovery_base.join(ts);

    let _lock = lock(home, Duration::from_secs(30))?;

    let porcelain = run_in(&home_s, "git", &["status", "--porcelain"])?;
    let mut recovered: u64 = 0;
    for (_status, path) in dirty_paths(&porcelain) {
        // merge=union files auto-merge on pull — leave them in place (Bash parity).
        if let Ok(attr) = run_in(&home_s, "git", &["check-attr", "merge", &path]) {
            if attr.trim().ends_with(": merge: union") {
                continue;
            }
        }
        let hash = path_hash(&home_s, &path);
        fs::create_dir_all(&recovery_dir)
            .map_err(|e| format!("recover: cannot create {}: {}", recovery_dir.display(), e))?;

        let abs = home.join(&path);
        let mut content_sha = String::new();
        if abs.is_file() {
            let dst = recovery_dir.join(&hash);
            fs::copy(&abs, &dst).map_err(|e| format!("recover: copy {} failed: {}", path, e))?;
            content_sha = file_sha(&dst);
        }
        // append manifest row.
        {
            use std::io::Write;
            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(recovery_dir.join("manifest.tsv"))
                .map_err(|e| format!("recover: manifest open failed: {}", e))?;
            f.write_all(manifest_line(&hash, &path).as_bytes())
                .map_err(|e| format!("recover: manifest write failed: {}", e))?;
        }
        // restore from HEAD; A-state (not in HEAD) → remove + unstage instead.
        if run_in(&home_s, "git", &["cat-file", "-e", &format!("HEAD:{}", path)]).is_ok() {
            let _ = run_in(&home_s, "git", &["checkout", "HEAD", "--", &path]);
        } else {
            let _ = fs::remove_file(&abs);
            let _ = run_in(&home_s, "git", &["reset", "-q", "HEAD", "--", &path]);
        }
        let rec_path = recovery_dir.join(&hash);
        emit(home, "canonical.recovery.stashed", &[
            ("path", &path),
            ("recovery_path", &rec_path.to_string_lossy()),
            ("sha", &content_sha),
        ]);
        recovered += 1;
    }

    run_in(&home_s, "git", &["fetch", "--quiet", "origin", "main"])
        .map_err(|e| format!("recover: git fetch origin main failed: {}", e))?;
    run_in(&home_s, "git", &["merge", "--ff-only", "origin/main"])
        .map_err(|e| format!("recover: ff-only merge failed after stash — try `werk-sync repair`: {}", e))?;

    let to = run_in(&home_s, "git", &["rev-parse", "--short", "HEAD"])?
        .trim()
        .to_string();
    emit(home, "canonical.recovery.completed", &[("recovered", &recovered.to_string()), ("to", &to)]);
    if recovered > 0 {
        Ok(format!(
            "werk-sync: recover — stashed {} file(s) to {}/, synced to {}\n  Manifest: {}/manifest.tsv (one row per stashed file: hash<TAB>original_path)",
            recovered,
            recovery_dir.display(),
            to,
            recovery_dir.display()
        ))
    } else {
        Ok(format!("werk-sync: recover — no dirty files; synced to {} (no-op)", to))
    }
}

/// Entry: parse argv + env, dispatch. Recovery base defaults to ~/.chorus/recovery
/// (Bash parity); CHORUS_RECOVERY_BASE overrides for tests.
pub fn run_sync() -> R<String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mode = parse_sync_args(&args)?;
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    match mode {
        Mode::Repair => repair(&home),
        Mode::Recover => {
            let base = env::var("CHORUS_RECOVERY_BASE")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    PathBuf::from(env::var("HOME").unwrap_or_default()).join(".chorus/recovery")
                });
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            // %Y%m%dT%H%M%S parity matters less than uniqueness; date via subprocess
            // keeps the human-readable form the Bash produced.
            let stamp = Command::new("date")
                .arg("+%Y%m%dT%H%M%S")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|| ts.to_string());
            recover(&home, &base, &stamp)
        }
    }
}
