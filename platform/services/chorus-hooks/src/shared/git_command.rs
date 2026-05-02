//! Env-scrubbed `git` command builder (#2589).
//!
//! Under per-role worktrees, parent processes (pre-commit hook, role-state
//! daemon) often have GIT_DIR / GIT_INDEX_FILE / GIT_WORK_TREE set in their
//! environment for their own bookkeeping. When chorus-hooks code calls
//! `Command::new("git")` to query repo state, the spawned `git` inherits
//! those vars and resolves paths against whatever the parent had set —
//! NOT the cwd the caller meant.
//!
//! Symptom (from #2560 wave 2): `git log -- <file>` returned empty even
//! though the file demonstrably had history, because the inherited GIT_DIR
//! pointed at a different worktree's pointer-file.
//!
//! Fix: every git-spawn site goes through `git_command()`, which builds a
//! `Command` with the three GIT_* vars removed. cwd-based resolution wins.
//!
//! Caller still does `.args([...]).current_dir(...).output()` etc. — the
//! helper only differs from `Command::new("git")` in the env scrub.

use std::process::Command;

/// Build a `git` Command with GIT_DIR / GIT_INDEX_FILE / GIT_WORK_TREE
/// removed from the child env. Use this everywhere chorus-hooks spawns
/// git instead of `Command::new("git")`.
pub fn git_command() -> Command {
    let mut cmd = Command::new("git");
    cmd.env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE");
    cmd
}
