//! #3431 — native worktree teardown, ported from bash `chorus-werk remove`.
//!
//! Semantics preserved verbatim from platform/scripts/chorus-werk cmd_remove
//! (each carries its incident lineage):
//!   - .werk-mcp daemon teardown before the dirty-check (#3016)
//!   - local-gone is NOT "already removed" until the remote ref is confirmed
//!     gone too — propagate the delete on remote-only orphans (#3498)
//!   - refuse on dirty werk, never silently lose work
//!   - two-tier merged-ness proof before branch delete (#3014): Tier 1 is an
//!     acceptance commit for THIS card on origin/main (the only check that
//!     survives squash-merge); Tier 2 (fallback) is patch-id via `git cherry`.
//!     Tier 1 matches on captured subjects, never `log | grep` (#3018 SIGPIPE),
//!     and fetch failures are surfaced in the refusal, never swallowed.
//!   - best-effort remote-ref delete + worktree prune, then spine emit
//!
//! Worktree management is version-control — the verb's own domain (card AC).
//! Spine logging stays a dependency: emitted through the injected `emit` hook
//! so callers keep their chorus-log path and tests observe events hermetically.

use std::path::{Path, PathBuf};
use std::process::Command;

// #4771 AC4 — the remote branch delete reaches GitHub with the land's deploy
// credential, never a keyring.
include!("../../shared/deploy_credential.rs");

/// #3638 — pipeline-regenerated files: the run's own gate steps rewrite these in
/// the werk AFTER werk-commit, so a clean land arrives at teardown "dirty" with
/// churn that is derived, not work (bit #3634 and #3421 on accept). Shared with
/// werk-commit's conflict-hold messaging via the existing path-dep.
/// #3928 — EMPTY on purpose. Its one entry, `knowledge/doc-coherence.md`, is
/// gitignored now, so it can no longer dirty a werk. That single file cost three
/// rebases on #3911 and a July note on #3622, and this constant plus the sweep's
/// twin were both written to live with it instead of removing it.
///
/// The seam stays: the next committed generated artifact will arrive here, and
/// the answer then is to ignore it at the source too, not to grow this list.
pub const GENERATED_FILES: &[&str] = &[];

/// The dirty paths from a `status --porcelain` listing that are NOT regenerated
/// artifacts. Empty ⇒ the werk's only dirt is generated churn, safe to discard.
/// Assumes porcelain v1's fixed two-char status + space prefix (`XY path`); a
/// rename line (`R  old -> new`) keeps its arrow form, which can never equal a
/// GENERATED_FILES entry — renames always read as real dirt (refuse), the safe side.
pub fn non_generated_dirty(porcelain: &str) -> Vec<String> {
    porcelain
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| l[3..].trim().to_string())
        .filter(|p| !GENERATED_FILES.iter().any(|g| p == g))
        .collect()
}

/// The dirty paths that ARE regenerated artifacts — the subset actually discarded
/// (witnessed precisely, not as the whole static list).
pub fn generated_dirty(porcelain: &str) -> Vec<String> {
    porcelain
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| l[3..].trim().to_string())
        .filter(|p| GENERATED_FILES.iter().any(|g| p == g))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Teardown {
    /// worktree and/or branch removed; remote delete attempted.
    Removed,
    /// local + remote both already gone — full no-op.
    AlreadyRemoved,
    /// local already gone; orphaned remote ref found and deleted (#3498).
    OrphanPropagated,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TeardownError {
    /// uncommitted changes in the werk — commit, stash, or abandon explicitly.
    Dirty(String),
    /// branch has commits not on origin/main and no acp commit — real work at risk.
    Unmerged(String),
    /// merge state could not be determined (cherry failed / fetch stale).
    Undetermined(String),
    /// git worktree remove / branch -D itself failed.
    GitFail(String),
}

impl std::fmt::Display for TeardownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeardownError::Dirty(m) => write!(f, "werk-dirty: {}", m),
            TeardownError::Unmerged(m) => write!(f, "branch-unmerged: {}", m),
            TeardownError::Undetermined(m) => write!(f, "merge-state-undetermined: {}", m),
            TeardownError::GitFail(m) => write!(f, "git-fail: {}", m),
        }
    }
}

fn git_out(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut c = Command::new("git");
    with_deploy_credential(&mut c, "git"); // #4771 AC4
    let out = c.arg("-C").arg(dir).args(args).output()
        .map_err(|e| format!("git spawn failed: {}", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(format!("git {:?}: {}", args, String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn git_ok(dir: &Path, args: &[&str]) -> bool {
    git_out(dir, args).is_ok()
}

/// `#<card>` with a non-digit (or end) boundary, so #300 never matches #3000.
/// Manual scan — the verb crates are zero-dependency by blueprint (#3045).
pub fn subjects_reference_card(subjects: &str, card: u64) -> bool {
    let needle = format!("#{}", card);
    for line in subjects.lines() {
        let mut from = 0;
        while let Some(pos) = line[from..].find(&needle) {
            let end = from + pos + needle.len();
            match line[end..].chars().next() {
                Some(c) if c.is_ascii_digit() => { from = end; }
                _ => return true,
            }
        }
    }
    false
}

/// #3016 — tear down a werk-deployed chorus-mcp daemon before the dirty-check
/// (the .werk-mcp marker is untracked and would otherwise refuse the remove).
fn teardown_werk_mcp(werk: &Path, emit: &mut dyn FnMut(&str, &[(&str, &str)])) {
    let marker = werk.join(".werk-mcp");
    let label_file = marker.join("label");
    if !label_file.is_file() {
        return;
    }
    if let Ok(label) = std::fs::read_to_string(&label_file) {
        let label = label.trim();
        if !label.is_empty() {
            let uid = libc_getuid();
            let _ = Command::new("launchctl")
                .args(["bootout", &format!("gui/{}/{}", uid, label)]).output();
            if let Some(home_dir) = std::env::var_os("HOME") {
                let plist = PathBuf::from(home_dir)
                    .join("Library/LaunchAgents").join(format!("{}.plist", label));
                let _ = std::fs::remove_file(plist);
            }
            emit("werk.mcp.teardown", &[("label", label)]);
        }
    }
    let _ = std::fs::remove_dir_all(&marker);
}

// getuid via libc extern — same pattern as the crates' flock extern (zero deps).
extern "C" {
    fn getuid() -> u32;
}
fn libc_getuid() -> u32 {
    unsafe { getuid() }
}

/// Port of `chorus-werk remove <role> <card>`. `home` is canonical CHORUS_HOME
/// (the repo whose worktrees/branches are managed); `werk_base` holds
/// `<role>-<card>` werk dirs. `emit` receives spine events (event, extras) —
/// callers wire it to their chorus-log dependency.
pub fn teardown_werk(
    home: &Path,
    werk_base: &Path,
    role: &str,
    card: u64,
    emit: &mut dyn FnMut(&str, &[(&str, &str)]),
) -> Result<Teardown, TeardownError> {
    // Callers' spine emitters already carry role/card/trace — extras here are
    // only the fields the teardown itself owns (branch, via).
    let werk = werk_base.join(format!("{}-{}", role, card));
    let branch = format!("{}/{}", role, card);
    // #4771 AC4 — arm the deploy credential when this account has one; with
    // none, git still never reaches a keyring, so a remote delete that needs
    // it fails and says so (teardown.auth.failed) instead of using Jeff's login.
    if armed_credential().is_none() {
        let _ = arm_deploy_credential(home);
    }

    let werk_present = werk.is_dir();
    let branch_present = git_ok(home, &["rev-parse", "--verify", &format!("refs/heads/{}", branch)]);

    if werk_present {
        teardown_werk_mcp(&werk, emit);
    }

    // Idempotent locally — but propagate the delete on a remote-only orphan (#3498).
    if !werk_present && !branch_present {
        let _ = git_out(home, &["worktree", "prune"]);
        if git_ok(home, &["ls-remote", "--exit-code", "--heads", "origin", &branch]) {
            if let Err(e) = git_out(home, &["push", "origin", "--delete", &branch]) {
                // #4771 AC4 — best-effort as before, but a refused credential is said.
                if is_auth_error(&e) {
                    emit("teardown.auth.failed", &[("branch", &branch), ("reason", &auth_fail_message(&e))]);
                }
            }
            emit("card.branch.closed", &[("branch", &branch), ("via", "orphan-propagate")]);
            return Ok(Teardown::OrphanPropagated);
        }
        return Ok(Teardown::AlreadyRemoved);
    }

    // Refuse on dirty werk — never silently lose work. #3638 carve-out: if the
    // ONLY dirt is pipeline-regenerated files (the run's own gate steps rewrite
    // them after werk-commit — hit accept on #3634 and #3421), discard that churn
    // and proceed; the committed version is the record. Any real dirt still refuses.
    if werk_present {
        let dirty = git_out(&werk, &["status", "--porcelain"])
            .map_err(TeardownError::GitFail)?;
        if !dirty.trim().is_empty() {
            let real = non_generated_dirty(&dirty);
            if real.is_empty() {
                let discarded = generated_dirty(&dirty);
                for f in &discarded {
                    let _ = git_out(&werk, &["checkout", "--", f]);
                }
                emit("teardown.generated.discarded", &[("files", &discarded.join(","))]);
            } else {
                return Err(TeardownError::Dirty(format!(
                    "{} has uncommitted changes ({}) — refusing remove (commit, stash, or abandon explicitly)",
                    werk.display(),
                    real.join(", ")
                )));
            }
        }
        git_out(home, &["worktree", "remove", "--force", &werk.to_string_lossy()])
            .map_err(|e| TeardownError::GitFail(format!("worktree remove failed: {}", e)))?;
    }

    if branch_present {
        // Tier 1 (#3014): an acceptance commit referencing #<card> on origin/main
        // is the only merge proof that survives squash-merge. Subjects are
        // captured then scanned — no `log | grep` pipe (#3018 SIGPIPE).
        let mut acp_committed = false;
        let mut fetch_failed = false;
        for attempt in 1..=3u8 {
            if git_out(home, &["fetch", "--quiet", "origin", "main"]).is_err() {
                fetch_failed = true;
            }
            let subjects = git_out(home, &["log", "origin/main", "-n", "300", "--format=%s"])
                .unwrap_or_default();
            if subjects_reference_card(&subjects, card) {
                acp_committed = true;
                fetch_failed = false;
                break;
            }
            if attempt < 3 {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }

        if !acp_committed {
            // Tier 2 — patch-id fallback; surface (don't swallow) fetch failure.
            let stale_note = if fetch_failed {
                " (note: could not refresh origin/main — merge state may be stale)"
            } else {
                ""
            };
            match git_out(home, &["cherry", "origin/main", &branch]) {
                Err(e) => {
                    return Err(TeardownError::Undetermined(format!(
                        "no acp commit on main, git cherry failed: {}{}", e, stale_note
                    )));
                }
                Ok(cherry) => {
                    if cherry.lines().any(|l| l.starts_with('+')) {
                        return Err(TeardownError::Unmerged(format!(
                            "'{}' has commits not on origin/main by content and no acp #{} commit on main{} — merge or abandon explicitly (git branch -D)",
                            branch, card, stale_note
                        )));
                    }
                }
            }
        }

        git_out(home, &["branch", "-D", &branch])
            .map_err(|e| TeardownError::GitFail(format!("branch -D {} failed: {}", branch, e)))?;
    }

    // Best-effort remote delete (#3498 — the propagation that used to no-op) + prune.
    if let Err(e) = git_out(home, &["push", "origin", "--delete", &branch]) {
        // #4771 AC4 — best-effort as before, but a refused credential is said.
        if is_auth_error(&e) {
            emit("teardown.auth.failed", &[("branch", &branch), ("reason", &auth_fail_message(&e))]);
        }
    }
    let _ = git_out(home, &["worktree", "prune"]);

    emit("card.branch.closed", &[("branch", &branch)]);
    Ok(Teardown::Removed)
}

#[cfg(test)]
mod teardown_credential_4771 {
    use super::*;

    #[test]
    fn armed_teardown_git_uses_only_the_deploy_credential_helper() {
        let _g = CRED_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        use std::os::unix::fs::PermissionsExt;
        let f = std::env::temp_dir().join(format!("td4771-{}", std::process::id()));
        std::fs::write(&f, "tok").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::env::set_var(CRED_FILE_ENV, &f);
        let armed = git_out(Path::new("."), &["config", "--get-all", "credential.helper"]).unwrap_or_default();
        std::env::remove_var(CRED_FILE_ENV);
        let unarmed = git_out(Path::new("."), &["config", "--get-all", "credential.helper"]).unwrap_or_default();
        let _ = std::fs::remove_file(&f);
        let last = armed.lines().last().unwrap_or("");
        assert!(last.starts_with("!f()") && last.contains(CRED_FILE_ENV), "armed: {armed:?}");
        // NEGATIVE PROOF: unarmed, no keyring helper survives.
        assert_eq!(unarmed.lines().last().unwrap_or("x"), "", "unarmed: {unarmed:?}");
    }
}
