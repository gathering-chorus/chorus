//! Worktree contamination guard (#2625).
//!
//! PreToolUse on Bash: refuses `git checkout/pull/reset/switch` against the
//! shared canonical /chorus clone when another role declares `building` per
//! their role-state.json. Two contamination events on 2026-04-30 traced
//! cleanly to wren switching /chorus to main while silas was actively
//! editing on a topic branch — yanking HEAD off silas's work.
//!
//! Sibling to #2580 (git-queue branch-check, defense-in-depth at the queue
//! layer); this fires earlier, at the PreToolUse layer, before the dangerous
//! op runs.
//!
//! Convention failed twice within an hour after the wren/silas chat
//! (silas-wren-1777560652) landed the [deploy-yank] nudge convention —
//! Jeff's call: "just trying wont work — we probably need a hook if the
//! directory is so critical."
//!
//! Per-role worktrees (chorus-silas/, chorus-wren/, chorus-kade/, and topic
//! worktrees like chorus-2526/) are EXEMPT — they have isolated .git/HEAD
//! per the #2582 convention. Only the canonical shared /chorus is guarded.
//!
//! Override: CHORUS_WORKTREE_OVERRIDE=1 bypasses for legitimate cases
//! (deploy-source-clone post-merge that's pre-arranged); logged to spine.

use crate::shared::state_paths::chorus_root;
use crate::types::{HookInput, HookResponse};
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

const ROLES: &[&str] = &["silas", "wren", "kade"];

fn git_dangerous_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Word boundaries on `git` and the action keep us from matching
        // `gitconfig`, `git-queue`, `--reset`, etc. Only standalone subcommand.
        Regex::new(r"\bgit\s+(checkout|pull|reset|switch)\b").unwrap()
    })
}

#[derive(Deserialize, Debug)]
struct DeclaredState {
    state: Option<String>,
    card: Option<serde_json::Value>,
}

/// Reads /tmp/claude-team-scan/<role>-declared.json. Returns None if missing
/// or unparseable. team_scan_dir is parameterized so tests inject a tempdir.
fn read_declared(team_scan_dir: &str, role: &str) -> Option<DeclaredState> {
    let path = format!("{}/{}-declared.json", team_scan_dir, role);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// True if cwd is the shared canonical /chorus clone. Per-role worktrees
/// (chorus-silas/, chorus-wren/, chorus-kade/, topic worktrees) are exempt
/// because they have isolated .git/HEAD by construction (#2582).
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

/// Extract the dangerous git subcommand for the deny message ("checkout",
/// "pull", "reset", "switch"). Defensive default to "checkout".
fn extracted_subcommand(command: &str) -> String {
    git_dangerous_re()
        .captures(command)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_else(|| "checkout".to_string())
}

/// Core check, parameterized for tests. Production wrapper below.
fn check_with(
    input: &HookInput,
    canonical_root: &str,
    team_scan_dir: &str,
    override_set: bool,
) -> HookResponse {
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

    for role in ROLES {
        if *role == my_role {
            continue;
        }
        let Some(state) = read_declared(team_scan_dir, role) else {
            continue;
        };
        if state.state.as_deref() != Some("building") {
            continue;
        }
        let card_str = match state.card {
            Some(serde_json::Value::Number(n)) => format!(" card=#{}", n),
            Some(serde_json::Value::String(s)) => format!(" card=#{}", s),
            _ => String::new(),
        };
        let other_worktree = format!("~/CascadeProjects/chorus-{}/", my_role);
        let stderr = format!(
            "BLOCKED: {role} is building{card_str} on shared /chorus.\n\
             `git {subcommand}` would yank HEAD off their work (today's contamination class — #2625).\n\n\
             Options:\n\
               1. Nudge {role} to commit/push first, then proceed\n\
               2. Wait for {role} to declare idle\n\
               3. Run from your per-role worktree ({other_worktree}) — exempt by construction (#2582)\n\
               4. Set CHORUS_WORKTREE_OVERRIDE=1 if this is a pre-arranged deploy-source-clone op (audited via spine)\n\n\
             Hook: worktree_contamination_guard (#2625)"
        );
        return HookResponse::block_with_stderr(&stderr);
    }

    HookResponse::allow()
}

/// Production entry point. Reads canonical root from chorus_root(), team
/// scan dir from /tmp/claude-team-scan, checks env for override, and emits
/// a spine event when the override is used.
pub async fn check(input: &HookInput) -> HookResponse {
    let canonical = chorus_root();
    let team_scan_dir = "/tmp/claude-team-scan";
    let override_set = std::env::var("CHORUS_WORKTREE_OVERRIDE").is_ok();

    let response = check_with(input, canonical, team_scan_dir, override_set);

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
    use std::fs;
    use tempfile::TempDir;

    fn make_input(tool: &str, command: &str, cwd: &str, role: &str) -> HookInput {
        serde_json::from_value(json!({
            "tool_name": tool,
            "tool_input": { "command": command },
            "cwd": cwd,
            "deploy_role": role,
        }))
        .unwrap()
    }

    fn write_declared(dir: &TempDir, role: &str, state: &str, card: Option<u32>) {
        let body = if let Some(c) = card {
            json!({"role": role, "state": state, "card": c})
        } else {
            json!({"role": role, "state": state})
        };
        let path = dir.path().join(format!("{}-declared.json", role));
        fs::write(&path, body.to_string()).unwrap();
    }

    const CANONICAL: &str = "/Users/jeff/chorus";

    #[test]
    fn allow_non_bash_tool() {
        let input = make_input("Edit", "anything", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, "/tmp/nonexistent", false);
        assert!(r.exit_code == 0, "non-Bash tool must allow");
    }

    #[test]
    fn allow_bash_non_dangerous_git() {
        let input = make_input("Bash", "git status", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, "/tmp/nonexistent", false);
        assert!(r.exit_code == 0, "git status is not dangerous");
    }

    #[test]
    fn allow_bash_unrelated_command() {
        let input = make_input("Bash", "ls -la", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, "/tmp/nonexistent", false);
        assert!(r.exit_code == 0);
    }

    #[test]
    fn allow_when_no_other_role_building() {
        let dir = TempDir::new().unwrap();
        write_declared(&dir, "silas", "idle", None);
        write_declared(&dir, "kade", "waiting", None);
        let input = make_input("Bash", "git checkout main", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), false);
        assert!(r.exit_code == 0, "no other role building -> allow");
    }

    #[test]
    fn block_when_other_role_building_with_card() {
        let dir = TempDir::new().unwrap();
        write_declared(&dir, "silas", "building", Some(2625));
        let input = make_input("Bash", "git checkout main", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), false);
        assert!(r.exit_code != 0, "must block when other role building");
        let stderr = r.stderr.clone().unwrap_or_default();
        assert!(stderr.contains("silas"), "must name building role: {stderr}");
        assert!(stderr.contains("#2625"), "must name card: {stderr}");
        assert!(stderr.contains("git checkout"), "must name git op: {stderr}");
    }

    #[test]
    fn block_on_pull_too() {
        let dir = TempDir::new().unwrap();
        write_declared(&dir, "silas", "building", Some(42));
        let input = make_input("Bash", "git pull origin main", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), false);
        assert!(r.exit_code != 0);
        assert!(r.stderr.clone().unwrap_or_default().contains("git pull"));
    }

    #[test]
    fn block_on_reset_and_switch() {
        let dir = TempDir::new().unwrap();
        write_declared(&dir, "silas", "building", Some(42));
        for cmd in &["git reset --hard HEAD", "git switch other-branch"] {
            let input = make_input("Bash", cmd, CANONICAL, "wren");
            let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), false);
            assert!(r.exit_code != 0, "must block: {cmd}");
        }
    }

    #[test]
    fn allow_per_role_worktree() {
        let dir = TempDir::new().unwrap();
        write_declared(&dir, "silas", "building", Some(2625));
        let cwd = "/Users/jeff/chorus-wren";
        let input = make_input("Bash", "git checkout main", cwd, "wren");
        let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), false);
        assert!(r.exit_code == 0, "per-role worktree must be exempt");
    }

    #[test]
    fn allow_topic_worktree() {
        let dir = TempDir::new().unwrap();
        write_declared(&dir, "silas", "building", Some(2625));
        let cwd = "/Users/jeff/chorus-2526";
        let input = make_input("Bash", "git checkout main", cwd, "silas");
        let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), false);
        assert!(r.exit_code == 0, "topic worktree must be exempt");
    }

    #[test]
    fn allow_self_building() {
        let dir = TempDir::new().unwrap();
        write_declared(&dir, "wren", "building", Some(2624));
        let input = make_input("Bash", "git checkout main", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), false);
        assert!(r.exit_code == 0, "my own building state must not block me");
    }

    #[test]
    fn allow_with_override() {
        let dir = TempDir::new().unwrap();
        write_declared(&dir, "silas", "building", Some(2625));
        let input = make_input("Bash", "git checkout main", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), true);
        assert!(r.exit_code == 0, "override must allow");
    }

    #[test]
    fn allow_when_state_file_missing() {
        let dir = TempDir::new().unwrap();
        let input = make_input("Bash", "git checkout main", CANONICAL, "wren");
        let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), false);
        assert!(r.exit_code == 0, "missing state files allow");
    }

    #[test]
    fn block_subdir_of_canonical_dangerous_op() {
        let dir = TempDir::new().unwrap();
        write_declared(&dir, "silas", "building", Some(2625));
        let cwd = "/Users/jeff/chorus/roles/wren";
        let input = make_input("Bash", "git checkout main", cwd, "wren");
        let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), false);
        assert!(r.exit_code != 0, "subdir of canonical still guarded");
    }

    #[test]
    fn allow_word_boundary_false_positives() {
        let dir = TempDir::new().unwrap();
        write_declared(&dir, "silas", "building", Some(2625));
        for cmd in &[
            "echo gitcheckout",
            "git config something",
            "git-queue.sh commit foo",
            "make build && git_log",
        ] {
            let input = make_input("Bash", cmd, CANONICAL, "wren");
            let r = check_with(&input, CANONICAL, dir.path().to_str().unwrap(), false);
            assert!(r.exit_code == 0, "word-boundary false positive: {cmd}");
        }
    }
}
