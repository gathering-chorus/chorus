//! Worktree contamination guard (#2625, RCA fixes #2626).
//!
//! PreToolUse on Bash: refuses `git checkout/pull/reset/switch` against the
//! shared canonical /chorus clone. Per-role worktrees (`/chorus-silas/`,
//! `/chorus-wren/`, `/chorus-kade/`, topic worktrees like `/chorus-2526/`)
//! are exempt by construction (#2582 — isolated `.git/HEAD`).
//!
//! ## RCA + fixes (2026-04-30, post-team-block)
//!
//! Initial #2625 hook had three structural bugs that blocked the team's
//! normal commit/merge ops within hours of landing:
//!
//! 1. **Substring match.** `\bgit\s+(checkout|pull|reset|switch)\b` matched
//!    the literal text anywhere in the Bash command — including inside
//!    quoted nudge bodies, commit messages, card descriptions. Kade #2626.
//!
//! 2. **cwd misread.** Hook read `input.cwd` (Claude Code's session cwd,
//!    fixed at session start = `/chorus` for all three roles). When a
//!    Bash command did `cd /chorus-silas && git checkout`, the hook still
//!    saw `/chorus` and blocked. The exempt-by-prefix logic never fired.
//!
//! 3. **Override env didn't propagate.** Setting `CHORUS_WORKTREE_OVERRIDE=1`
//!    as a prefix on a Bash command sets env for the bash subshell only —
//!    not for Claude Code's spawn of the hook shim. JSON-injection path
//!    via shim.rs works only when env is set in the Claude Code session.
//!
//! ## Fix shape
//!
//! - **Tokenize the command** by shell separators (`;`, `&&`, `||`, `|`,
//!   newline) and check each segment as a real sub-command. Match `git X`
//!   only when it's the actual invocation (segment starts with `git `).
//! - **Track effective cwd** by walking the segments — if a segment is
//!   `cd <path>`, that becomes the cwd for subsequent dangerous git checks
//!   in the same command. So `cd /chorus-silas && git checkout main` is
//!   evaluated at `/chorus-silas`, not at `input.cwd`.
//! - **Magic-comment override.** Command containing `# worktree-override`
//!   exempts the whole command. This is for inline cases where roles need
//!   to bypass without shell-level env setup. Audited via spine event.
//!   The original env-based override stays as a secondary path.

use crate::shared::state_paths::chorus_root;
use crate::types::{HookInput, HookResponse};
use regex::Regex;
use std::sync::OnceLock;

/// Match `git <subcommand>` only when it's the actual invocation:
/// at start of a command segment (after optional whitespace).
/// Substring matches inside quoted args / commit messages don't trigger.
fn git_dangerous_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*git\s+(checkout|pull|reset|switch)\b").unwrap()
    })
}

/// Match `cd <path>` (with optional trailing `&&` etc) to extract the target.
fn cd_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*cd\s+(\S+)").unwrap()
    })
}

/// Per-role worktrees AND topic worktrees are exempt — anything matching
/// `/chorus-<suffix>` rather than the bare `/chorus/` path.
fn is_per_role_worktree(cwd: &str) -> bool {
    cwd.contains("/chorus-")
}

fn is_shared_canonical(cwd: &str, canonical: &str) -> bool {
    if cwd.is_empty() || canonical.is_empty() {
        return false;
    }
    if is_per_role_worktree(cwd) {
        return false;
    }
    cwd == canonical || cwd.starts_with(&format!("{}/", canonical))
}

/// Split a Bash command into segments by shell separators (`;`, `&&`, `||`,
/// `|`, newline). Quotes are NOT honored — segments inside quotes will be
/// over-split, but that's safe for our purposes (we only act on segments
/// that start with `git X` after the split, and dangerous-text-in-quotes
/// won't pass the leading-`git ` anchor).
fn split_segments(command: &str) -> Vec<String> {
    // Replace separators with `\n`, then split. Order matters — `&&` before `&`.
    let normalized = command
        .replace("&&", "\n")
        .replace("||", "\n")
        .replace(';', "\n")
        .replace('|', "\n")
        .replace('\r', "\n");
    normalized.split('\n').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Walk command segments to find a dangerous git invocation. Track effective
/// cwd via `cd` segments. Returns Some(subcommand) if a dangerous op is
/// invoked from a non-exempt cwd; None otherwise.
fn find_dangerous_op(command: &str, input_cwd: &str) -> Option<(String, String)> {
    let mut effective_cwd = input_cwd.to_string();
    for segment in split_segments(command) {
        if let Some(captures) = cd_re().captures(&segment) {
            // cd <path> — update effective cwd
            if let Some(target) = captures.get(1) {
                effective_cwd = target.as_str().trim_matches(&['\'', '"'][..]).to_string();
            }
            continue;
        }
        if let Some(captures) = git_dangerous_re().captures(&segment) {
            let subcommand = captures.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            return Some((subcommand, effective_cwd.clone()));
        }
    }
    None
}

/// Magic-comment override: command containing `# worktree-override` (or
/// `#worktree-override`) exempts the entire command. Audited via spine.
fn has_magic_override(command: &str) -> bool {
    command.contains("# worktree-override") || command.contains("#worktree-override")
}

/// Core check. Parameterized for tests.
fn check_with(input: &HookInput, canonical_root: &str, env_override: bool) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }
    let command = input.get_tool_input_str("command");
    if command.is_empty() {
        return HookResponse::allow();
    }
    if env_override || has_magic_override(&command) {
        return HookResponse::allow();
    }

    let input_cwd = input.cwd.as_deref().unwrap_or("");
    let Some((subcommand, effective_cwd)) = find_dangerous_op(&command, input_cwd) else {
        return HookResponse::allow();
    };

    // Effective cwd is what matters — the dangerous op runs there, not at
    // Claude Code's session cwd. If the role cd'd into a per-role worktree
    // before the git op, exempt.
    if is_per_role_worktree(&effective_cwd) {
        return HookResponse::allow();
    }
    if !is_shared_canonical(&effective_cwd, canonical_root) {
        return HookResponse::allow();
    }

    let my_role = input.role().as_str().to_string();
    let other_worktree = format!("~/CascadeProjects/chorus-{}/", my_role);
    let stderr = format!(
        "BLOCKED: shared /chorus is read-only for dangerous git ops.\n\
         `git {subcommand}` on the canonical clone yanks HEAD for any role\n\
         currently working there — today's contamination class (#2625).\n\n\
         Options:\n\
           1. Run from your per-role worktree: {other_worktree}\n\
              Either prefix the command with `cd {other_worktree} &&`,\n\
              or launch a session from there. Per-role worktrees have\n\
              isolated .git/HEAD by construction (#2582).\n\
           2. Inline override: append `  # worktree-override` to the\n\
              command (audited via worktree.override.used spine event).\n\
           3. Session-level: set CHORUS_WORKTREE_OVERRIDE=1 in your shell\n\
              before launching Claude Code (also audited).\n\n\
         Hook: worktree_contamination_guard (#2625)"
    );
    HookResponse::block_with_stderr(&stderr)
}

/// Production entry point. Override flag comes from JSON (shim.rs injects
/// from env at shim-spawn time) — env on a per-command bash prefix does
/// NOT propagate, that's a known limitation. Magic-comment is the inline
/// escape; env/JSON path is for session-level set-and-forget.
pub async fn check(input: &HookInput) -> HookResponse {
    let canonical = chorus_root();
    let env_override = input.chorus_worktree_override.unwrap_or(false);
    let response = check_with(input, canonical, env_override);

    // Audit any override usage — env or magic-comment
    if input.tool_name_str() == "Bash" {
        let command = input.get_tool_input_str("command");
        if !command.is_empty() && find_dangerous_op(&command, input.cwd.as_deref().unwrap_or("")).is_some() {
            let used_override = env_override || has_magic_override(&command);
            if used_override {
                let role = input.role().as_str();
                let trunc = if command.len() > 80 { &command[..80] } else { command.as_str() };
                let kind = if env_override { "env" } else { "magic-comment" };
                crate::state::chorus_log(
                    "worktree.override.used",
                    role,
                    &[("command", trunc), ("kind", kind), ("hook", "worktree_contamination_guard")],
                )
                .await;
            }
        }
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

    // --- Allow cases ---

    #[test]
    fn allow_non_bash_tool() {
        let input = make_input("Edit", "anything", CANONICAL, "wren");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    #[test]
    fn allow_git_status() {
        let input = make_input("Bash", "git status", CANONICAL, "wren");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    #[test]
    fn allow_unrelated_command() {
        let input = make_input("Bash", "ls -la", CANONICAL, "wren");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    #[test]
    fn allow_per_role_worktree_cwd() {
        let cwd = "/Users/jeff/chorus-wren";
        let input = make_input("Bash", "git checkout main", cwd, "wren");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    #[test]
    fn allow_with_env_override() {
        let input = make_input("Bash", "git checkout main", CANONICAL, "wren");
        assert_eq!(check_with(&input, CANONICAL, true).exit_code, 0);
    }

    // --- RCA fix #1: substring-match false positives ---

    #[test]
    fn allow_substring_in_quoted_message() {
        // Kade's #2626 reproduction — `git checkout` text inside a commit
        // message must NOT trigger the hook
        let input = make_input(
            "Bash",
            r#"git-queue.sh commit foo.rs -- -m "explains git checkout behavior""#,
            CANONICAL,
            "wren",
        );
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0,
            "must not match 'git checkout' inside quoted commit message");
    }

    #[test]
    fn allow_substring_in_nudge_body() {
        let input = make_input(
            "Bash",
            r#"nudge wren "explain git pull semantics" --from silas"#,
            CANONICAL,
            "silas",
        );
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0,
            "must not match 'git pull' inside nudge body");
    }

    #[test]
    fn allow_echo_with_dangerous_text() {
        let input = make_input("Bash", "echo 'git reset is dangerous'", CANONICAL, "wren");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    #[test]
    fn allow_word_boundary_false_positives() {
        for cmd in &[
            "gitcheckout",
            "git config something",
            "git-queue.sh commit foo",
            "make build && git_log",
        ] {
            let input = make_input("Bash", cmd, CANONICAL, "wren");
            assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0,
                "false positive: {cmd}");
        }
    }

    // --- RCA fix #2: cwd misread (effective cwd from cd prefix) ---

    #[test]
    fn allow_cd_to_per_role_worktree_then_git() {
        // input.cwd is /chorus (Claude Code's session cwd) but the actual
        // op runs in /chorus-silas via cd prefix
        let cmd = "cd /Users/jeff/chorus-silas && git checkout main";
        let input = make_input("Bash", cmd, CANONICAL, "silas");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0,
            "cd to per-role worktree must be honored");
    }

    #[test]
    fn allow_cd_to_topic_worktree_then_git() {
        let cmd = "cd /Users/jeff/chorus-2526 && git pull";
        let input = make_input("Bash", cmd, CANONICAL, "silas");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    #[test]
    fn allow_cd_chained_to_per_role() {
        // Multiple cd in chain — last cd before git wins
        let cmd = "cd /tmp && cd /Users/jeff/chorus-wren && git pull";
        let input = make_input("Bash", cmd, CANONICAL, "wren");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    #[test]
    fn block_cd_to_canonical_then_git() {
        // Even with cd prefix, if it lands at canonical /chorus, block
        let cmd = "cd /Users/jeff/chorus && git checkout main";
        let input = make_input("Bash", cmd, CANONICAL, "wren");
        assert_ne!(check_with(&input, CANONICAL, false).exit_code, 0,
            "cd to canonical clone must still block");
    }

    // --- RCA fix #3: magic-comment override ---

    #[test]
    fn allow_magic_comment_override() {
        let cmd = "git checkout main  # worktree-override";
        let input = make_input("Bash", cmd, CANONICAL, "wren");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    #[test]
    fn allow_magic_comment_no_space() {
        let cmd = "git pull #worktree-override";
        let input = make_input("Bash", cmd, CANONICAL, "wren");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    // --- Block cases (still working as designed) ---

    #[test]
    fn block_direct_checkout_on_canonical() {
        let input = make_input("Bash", "git checkout main", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, false);
        assert_ne!(r.exit_code, 0);
        let stderr = r.stderr.clone().unwrap_or_default();
        assert!(stderr.contains("git checkout"));
        assert!(stderr.contains("chorus-wren"));
        assert!(stderr.contains("worktree-override"), "must mention magic-comment escape: {stderr}");
    }

    #[test]
    fn block_pull_on_canonical_subdir() {
        let cwd = "/Users/jeff/chorus/roles/wren";
        let input = make_input("Bash", "git pull", cwd, "wren");
        assert_ne!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    #[test]
    fn block_reset_and_switch() {
        for cmd in &["git reset --hard HEAD", "git switch other"] {
            let input = make_input("Bash", cmd, CANONICAL, "wren");
            assert_ne!(check_with(&input, CANONICAL, false).exit_code, 0,
                "must block: {cmd}");
        }
    }

    // --- Integration: real git-queue.sh-shaped flows ---

    #[test]
    fn allow_git_queue_commit_with_dangerous_text_in_message() {
        // git-queue.sh commit with a message that mentions dangerous git ops
        let cmd = r#"DEPLOY_ROLE=silas bash platform/scripts/git-queue.sh commit foo.rs -- -m "fix: handle git checkout edge case""#;
        let input = make_input("Bash", cmd, CANONICAL, "silas");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0,
            "git-queue.sh wraps git internally; outer command has no dangerous git invocation");
    }

    #[test]
    fn allow_git_queue_push() {
        let cmd = r#"DEPLOY_ROLE=silas bash platform/scripts/git-queue.sh push"#;
        let input = make_input("Bash", cmd, CANONICAL, "silas");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }

    #[test]
    fn allow_gh_pr_merge_with_dangerous_text() {
        let cmd = r#"gh pr merge 71 --squash --delete-branch -m "merge git checkout fix""#;
        let input = make_input("Bash", cmd, CANONICAL, "silas");
        assert_eq!(check_with(&input, CANONICAL, false).exit_code, 0);
    }
}
