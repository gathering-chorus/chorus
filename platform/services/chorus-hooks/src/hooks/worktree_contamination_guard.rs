//! Worktree contamination guard (#2625).
//!
//! PreToolUse on Bash: refuses `git checkout/pull/reset/switch` against the
//! shared canonical /chorus clone. The shared clone is read-only for
//! dangerous git ops by invariant — every role works in their per-role
//! worktree (`/chorus-silas/`, `/chorus-wren/`, `/chorus-kade/`, or topic
//! worktrees like `/chorus-2526/`) per #2582. Per-role worktrees have
//! isolated `.git/HEAD` so no cross-role contamination is possible there.
//!
//! Two contamination events on 2026-04-30: wren switched /chorus to main
//! while silas was actively editing on a topic branch (10:00 + 10:49). The
//! convention agreed in chat silas-wren-1777560652 ([deploy-yank] nudge
//! before switch) failed within an hour. Jeff's call: "just trying wont
//! work — we probably need a hook if the directory is so critical."
//!
//! Sibling to #2580 (git-queue branch-check, defense-in-depth at the queue
//! layer); this fires earlier, at PreToolUse, before the dangerous op runs.
//!
//! The hook does NOT consult role-state files. The original draft did,
//! looking up "is another role building right now" to fire conditionally.
//! Jeff's call (2026-04-30): role-state is unreliable substrate (sticky
//! card-fields, no cleanup on session death), and the simpler invariant
//! "shared /chorus is canonical, dangerous git ops require a per-role
//! worktree or explicit override" doesn't need that lookup. Anyone running
//! dangerous git ops on /chorus is in the wrong place. Hook says so.
//!
//! Override: `CHORUS_WORKTREE_OVERRIDE=1` bypasses for legitimate cases
//! (deploy-source-clone post-merge that's pre-arranged); usage emits a
//! `worktree.override.used` spine event for audit.

use crate::shared::state_paths::chorus_root;
use crate::types::{HookInput, HookResponse};
use regex::Regex;
use std::sync::OnceLock;

fn git_dangerous_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Word boundaries on `git` and the action keep us from matching
        // `gitconfig`, `git-queue`, `--reset`, etc. Only standalone subcommand.
        Regex::new(r"\bgit\s+(checkout|pull|reset|switch)\b").unwrap()
    })
}

/// True if cwd is the shared canonical /chorus clone. Per-role worktrees
/// (chorus-silas/, chorus-wren/, chorus-kade/, topic worktrees like
/// chorus-2526/) are exempt because they have isolated .git/HEAD by
/// construction (#2582).
fn is_shared_canonical(cwd: &str, canonical: &str) -> bool {
    if cwd.is_empty() || canonical.is_empty() {
        return false;
    }
    // Per-role / topic worktrees are NOT canonical. Detect by sibling-path
    // prefix: anything containing "/chorus-<suffix>" rather than the bare
    // "/chorus/" path is a worktree.
    if cwd.contains("/chorus-") {
        return false;
    }
    cwd == canonical || cwd.starts_with(&format!("{}/", canonical))
}

/// Extract the dangerous git subcommand for the deny message. Defensive
/// default to "checkout".
fn extracted_subcommand(command: &str) -> String {
    git_dangerous_re()
        .captures(command)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_else(|| "checkout".to_string())
}

/// Core check, parameterized for tests. Production wrapper below.
fn check_with(input: &HookInput, canonical_root: &str, override_set: bool) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let command = input.get_tool_input_str("command");
    if !git_dangerous_re().is_match(&command) {
        return HookResponse::allow();
    }

    if override_set {
        return HookResponse::allow();
    }

    let cwd = input.cwd.as_deref().unwrap_or("");
    if !is_shared_canonical(cwd, canonical_root) {
        return HookResponse::allow();
    }

    let my_role = input.role().as_str().to_string();
    let subcommand = extracted_subcommand(&command);
    let other_worktree = format!("~/CascadeProjects/chorus-{}/", my_role);

    let stderr = format!(
        "BLOCKED: shared /chorus is read-only for dangerous git ops.\n\
         `git {subcommand}` on the canonical clone yanks HEAD for any role\n\
         currently working there — today's contamination class (#2625).\n\n\
         Options:\n\
           1. Run from your per-role worktree: {other_worktree}\n\
              (isolated .git/HEAD by construction — no cross-role race, #2582)\n\
           2. Set CHORUS_WORKTREE_OVERRIDE=1 if this is a pre-arranged\n\
              deploy-source-clone op (audited via worktree.override.used spine event)\n\n\
         Hook: worktree_contamination_guard (#2625)"
    );
    HookResponse::block_with_stderr(&stderr)
}

/// Production entry point. Reads canonical root from chorus_root(), checks
/// override flag from JSON (shim wraps env→json since env doesn't cross the
/// unix-socket boundary), emits spine event when override is used.
pub async fn check(input: &HookInput) -> HookResponse {
    let canonical = chorus_root();
    // Override is injected via JSON by the shim wrapper (see shim.rs).
    // Reading env directly would only work for in-process callers, not for
    // the production daemon which is a separate process from shim's env.
    let override_set = input.chorus_worktree_override.unwrap_or(false);

    let response = check_with(input, canonical, override_set);

    if override_set
        && input.tool_name_str() == "Bash"
        && git_dangerous_re().is_match(&input.get_tool_input_str("command"))
    {
        let cmd = input.get_tool_input_str("command");
        let role = input.role().as_str();
        let trunc = if cmd.len() > 80 { &cmd[..80] } else { cmd.as_str() };
        crate::state::chorus_log(
            "worktree.override.used",
            role,
            &[("command", trunc), ("hook", "worktree_contamination_guard")],
        )
        .await;
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_input(tool: &str, command: &str, cwd: &str, role: &str) -> HookInput {
        serde_json::from_value(json!({
            "tool_name": tool,
            "tool_input": { "command": command },
            "cwd": cwd,
            "deploy_role": role,
        }))
        .unwrap()
    }

    const CANONICAL: &str = "/Users/jeff/chorus";

    #[test]
    fn allow_non_bash_tool() {
        let input = make_input("Edit", "anything", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, false);
        assert!(r.exit_code == 0);
    }

    #[test]
    fn allow_bash_non_dangerous_git() {
        let input = make_input("Bash", "git status", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, false);
        assert!(r.exit_code == 0, "git status is not dangerous");
    }

    #[test]
    fn allow_unrelated_command() {
        let input = make_input("Bash", "ls -la", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, false);
        assert!(r.exit_code == 0);
    }

    #[test]
    fn block_checkout_on_canonical() {
        let input = make_input("Bash", "git checkout main", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, false);
        assert!(r.exit_code != 0, "must block");
        let stderr = r.stderr.clone().unwrap_or_default();
        assert!(stderr.contains("git checkout"));
        assert!(stderr.contains("chorus-wren"), "must point at wren's worktree");
        assert!(stderr.contains("CHORUS_WORKTREE_OVERRIDE"));
    }

    #[test]
    fn block_pull_on_canonical() {
        let input = make_input("Bash", "git pull origin main", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, false);
        assert!(r.exit_code != 0);
        assert!(r.stderr.clone().unwrap_or_default().contains("git pull"));
    }

    #[test]
    fn block_reset_and_switch() {
        for cmd in &["git reset --hard HEAD", "git switch other-branch"] {
            let input = make_input("Bash", cmd, CANONICAL, "wren");
            let r = check_with(&input, CANONICAL, false);
            assert!(r.exit_code != 0, "must block: {cmd}");
        }
    }

    #[test]
    fn allow_per_role_worktree() {
        // Wren in her own worktree — exempt by construction (#2582)
        let cwd = "/Users/jeff/chorus-wren";
        let input = make_input("Bash", "git checkout main", cwd, "wren");
        let r = check_with(&input, CANONICAL, false);
        assert!(r.exit_code == 0, "per-role worktree must be exempt");
    }

    #[test]
    fn allow_topic_worktree() {
        // chorus-2526 = silas's existing topic worktree
        let cwd = "/Users/jeff/chorus-2526";
        let input = make_input("Bash", "git checkout main", cwd, "silas");
        let r = check_with(&input, CANONICAL, false);
        assert!(r.exit_code == 0, "topic worktree must be exempt");
    }

    #[test]
    fn allow_with_override() {
        let input = make_input("Bash", "git checkout main", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, true);
        assert!(r.exit_code == 0, "override must allow");
    }

    #[test]
    fn block_subdir_of_canonical() {
        // /chorus/roles/wren is still inside canonical clone
        let cwd = "/Users/jeff/chorus/roles/wren";
        let input = make_input("Bash", "git checkout main", cwd, "wren");
        let r = check_with(&input, CANONICAL, false);
        assert!(r.exit_code != 0, "subdir of canonical still guarded");
    }

    #[test]
    fn allow_word_boundary_false_positives() {
        // "git checkout" must match, but "gitcheckout" / "git-queue" / etc must not
        for cmd in &[
            "echo gitcheckout",
            "git config something",
            "git-queue.sh commit foo",
            "make build && git_log",
        ] {
            let input = make_input("Bash", cmd, CANONICAL, "wren");
            let r = check_with(&input, CANONICAL, false);
            assert!(r.exit_code == 0, "word-boundary false positive: {cmd}");
        }
    }
}
