use crate::shared::state_paths::chorus_root;
use crate::state::chorus_log;
use crate::types::{permission_ask_json, permission_deny_json, HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;

static KILL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(kill|pkill|killall)\s").unwrap()
});

static GIT_COMMIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+commit\b").unwrap()
});

static GIT_ADD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+add\b").unwrap()
});

// #2598: extend the raw-git refusal surface to push/rebase/cherry-pick/reset.
// Substrate-uniformity (Jeff 2026-04-29) — the werk pipeline (werk-commit /
// werk-push) is the only sanctioned path for any state-mutating git op on
// canonical (git-queue.sh retired #3182/#3223). Read-only ops (log, status,
// diff) are always allowed.
static GIT_PUSH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+push\b").unwrap()
});

static GIT_REBASE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+rebase\b").unwrap()
});

// #2789 — `git rebase --abort`, `--continue`, `--skip`, `--edit-todo`, `--quit`,
// `--show-current-patch` are rebase-cleanup / rebase-inspect flags, not the
// mutating form. They manage in-progress rebase state created earlier; blocking
// them strands roles in stuck-rebase state with no recovery (Wren 2026-05-07
// 17:30 — got blocked aborting her own stale rebase). The mutating form
// `git rebase <ref>` (or bare `git rebase` defaulting to @{upstream}) stays
// blocked. Pattern: any of these specific cleanup flags after `git rebase` is
// allowed-through, even though GIT_REBASE_RE matches.
static GIT_REBASE_CLEANUP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+rebase\s+--(abort|continue|skip|edit-todo|quit|show-current-patch)\b").unwrap()
});

static GIT_CHERRY_PICK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+cherry-pick\b").unwrap()
});

static GIT_RESET_HARD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+reset\s+--hard\b").unwrap()
});

// #2711 — Mode-A close (Candidate A from #2706). Deny raw checkout/switch/
// branch-create at command-position; branch/worktree lifecycle is owned by
// chorus-werk (/pull creates, /acp tears down; git-queue.sh retired #3182).
// Substring/heredoc scoping inherited from #2698 strip_quoted_runs() — the
// same matching infrastructure.
static GIT_CHECKOUT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+checkout\b").unwrap()
});

static GIT_SWITCH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+switch\b").unwrap()
});

// `git branch` (no args) is read-only list — pass. `git branch <name>` is
// create — block. Match only the create form (followed by a non-flag word).
static GIT_BRANCH_CREATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgit\s+branch\s+[^-\s]\S*").unwrap()
});

static HEREDOC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<<['"]?EOF"#).unwrap()
});

static TERRAFORM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bterraform\s+(apply|destroy)\b").unwrap()
});

fn team_repo_root() -> &'static str { chorus_root() }

/// Blocks dangerous infra commands for all roles (#1714, #2020, #2119)
/// All services are LaunchAgents — no container runtime, no Terraform-managed infra.
pub async fn check(input: &HookInput) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let cmd = input.get_tool_input_str("command");
    let cwd = input.cwd.as_deref().unwrap_or("");
    if cmd.is_empty() {
        return HookResponse::allow();
    }
    tracing::trace!(hook = "infra_guardrails", phase = "receive", cmd_len = cmd.len(), "checking bash command");

    // Skip heredocs and echo/printf (absorbed from app_state_guard #1862)
    if cmd.contains("<<") {
        return HookResponse::allow();
    }
    let first_line = cmd.lines().next().unwrap_or("").trim_start();
    if first_line.starts_with("echo ")
        || first_line.starts_with("printf ")
        || first_line.starts_with("cat >")
        || first_line.starts_with("cat <<")
    {
        return HookResponse::allow();
    }

    // Allow service-lifecycle.sh (absorbed from app_state_guard #1862)
    // Allow agent-state.sh for LaunchAgent lifecycle (#2009)
    if cmd.contains("service-lifecycle.sh") || cmd.contains("app-state.sh") || cmd.contains("agent-state.sh") {
        return HookResponse::allow();
    }

    // kill/pkill/killall — exempt signal-0 (liveness check, read-only) (#2047)
    if KILL_RE.is_match(&cmd) {
        let is_signal_0 = cmd.contains("kill -0 ") || cmd.contains("kill --signal 0 ");
        if !is_signal_0 {
            log_guardrail("deny", "kill").await;
            return HookResponse::deny(&permission_deny_json(
                "BLOCKED: Manual process killing is prohibited. Use agent-state.sh for LaunchAgents."
            ));
        }
        log_guardrail("allow", "kill-signal-0").await;
    }

    // git mutating ops in team repo only — #2598 extended push/rebase/cherry-pick/reset
    // #2698 — match against the quote-stripped cmd. The original regexes used
    // \b word-boundary which fired inside quoted args (cards add --desc "...git
    // commit..." / nudge bodies / grep patterns). Stripping quoted runs scopes
    // matches to command-position. bash -c "..." sub-shell invocations of raw
    // git also become invisible to this guard — that's an explicit-bypass case
    // documented like the heredoc skip.
    let cmd_for_match = strip_quoted_runs(&cmd);
    // #2789 — rebase cleanup flags (--abort/--continue/etc) are NOT mutations
    // of new history; they manage in-progress rebase state. Allow them through.
    let is_rebase_mut = GIT_REBASE_RE.is_match(&cmd_for_match)
        && !GIT_REBASE_CLEANUP_RE.is_match(&cmd_for_match);
    let is_git_mut = GIT_COMMIT_RE.is_match(&cmd_for_match)
        || GIT_ADD_RE.is_match(&cmd_for_match)
        || GIT_PUSH_RE.is_match(&cmd_for_match)
        || is_rebase_mut
        || GIT_CHERRY_PICK_RE.is_match(&cmd_for_match)
        || GIT_RESET_HARD_RE.is_match(&cmd_for_match)
        || GIT_CHECKOUT_RE.is_match(&cmd_for_match)
        || GIT_SWITCH_RE.is_match(&cmd_for_match)
        || GIT_BRANCH_CREATE_RE.is_match(&cmd_for_match);
    if is_git_mut {
        // Skip heredocs
        if HEREDOC_RE.is_match(&cmd) {
            log_guardrail("allow", "git-in-heredoc").await;
        } else {
            // Check if we're in the team repo using actual CWD (#2078)
            let in_team_repo = cwd.starts_with(team_repo_root())
                || cmd.contains(team_repo_root());

            if in_team_repo {
                if GIT_COMMIT_RE.is_match(&cmd) {
                    log_guardrail("deny", "git-commit").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git commit is prohibited in the chorus repo. Commits land through the werk pipeline — werk-commit (and /acp) stage + commit + push your card's werk under the lock. Loose-canonical recovery is Jeff/werk-only; no agent direct-git on canonical. For a foreign repo, use `git -C <repo-path> commit`."
                    ));
                }
                if GIT_ADD_RE.is_match(&cmd) {
                    log_guardrail("deny", "git-add").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git add is prohibited in the chorus repo. The werk pipeline (werk-commit) stages + commits your card's werk atomically under the lock. For a foreign repo, use `git -C <repo-path> add`."
                    ));
                }
                if GIT_PUSH_RE.is_match(&cmd) {
                    log_guardrail("deny", "git-push").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git push is prohibited in the chorus repo (#2598). Pushes happen inside the werk pipeline — werk-push validates branch + role before pushing. For a foreign repo, use `git -C <repo-path> push`."
                    ));
                }
                if GIT_REBASE_RE.is_match(&cmd) && !GIT_REBASE_CLEANUP_RE.is_match(&cmd) {
                    log_guardrail("deny", "git-rebase").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git rebase is prohibited in the chorus repo (#2598). The werk pipeline rebases onto origin/main internally; canonical re-alignment is chorus-werk-sync (Jeff/werk-only). (Cleanup flags --abort/--continue/--skip/--edit-todo/--quit/--show-current-patch are allowed and not blocked here.)"
                    ));
                }
                if GIT_CHERRY_PICK_RE.is_match(&cmd) {
                    log_guardrail("deny", "git-cherry-pick").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git cherry-pick is prohibited in the chorus repo (#2598). The werk pipeline owns history on canonical (Jeff/werk-only). For a foreign repo, use `git -C <repo-path> cherry-pick`."
                    ));
                }
                if GIT_RESET_HARD_RE.is_match(&cmd) {
                    log_guardrail("deny", "git-reset-hard").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git reset --hard is prohibited in the chorus repo (#2598). Canonical re-alignment is chorus-werk-sync (Jeff/werk-only). For a foreign repo, use `git -C <repo-path> reset --hard`."
                    ));
                }
                if GIT_CHECKOUT_RE.is_match(&cmd_for_match) {
                    log_guardrail("deny", "git-checkout").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git checkout is prohibited in the chorus repo (#2706 Mode-A close). Branch/worktree lifecycle is managed by chorus-werk — /pull creates the card's branch + worktree, /acp tears it down. For a foreign repo, use `git -C <repo-path> checkout`."
                    ));
                }
                if GIT_SWITCH_RE.is_match(&cmd_for_match) {
                    log_guardrail("deny", "git-switch").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git switch is prohibited in the chorus repo (#2706 Mode-A close). chorus-werk manages branches (/pull creates the card's branch). For a foreign repo, use `git -C <repo-path> switch`."
                    ));
                }
                if GIT_BRANCH_CREATE_RE.is_match(&cmd_for_match) {
                    log_guardrail("deny", "git-branch-create").await;
                    return HookResponse::deny(&permission_deny_json(
                        "BLOCKED: Direct git branch <name> (create) is prohibited in the chorus repo (#2706 Mode-A close). chorus-werk creates the card's branch on /pull. (Read-only `git branch` with no args still passes.)"
                    ));
                }
            }
        }
    }

    // terraform apply/destroy (ask, not deny)
    if TERRAFORM_RE.is_match(&cmd) {
        log_guardrail("ask", "terraform-direct").await;
        return HookResponse::deny(&permission_ask_json(
            "Direct terraform apply/destroy detected. All services are LaunchAgents — no Terraform-managed infrastructure."
        ));
    }

    HookResponse::allow()
}

async fn log_guardrail(decision: &str, pattern: &str) {
    tracing::trace!(
        hook = "infra_guardrails",
        phase = "execute",
        decision,
        pattern,
        "guardrail decision"
    );
    chorus_log(
        "guard.rule.decided",
        "system",
        &[("decision", decision), ("pattern", pattern)],
    )
    .await;
}

// #2698 — Replace single- and double-quoted runs with spaces before the git-mut
// regex check, so quoted arg content (card descriptions, nudge bodies, grep
// patterns) doesn't false-positive trigger blocks. Single quotes don't process
// escapes; double quotes recognize \" as an escaped delimiter. Spaces preserve
// token boundaries — `cards add --desc 'X'` becomes `cards add --desc       `,
// not `cards add --desc`, so adjacent tokens don't accidentally fuse.
fn strip_quoted_runs(cmd: &str) -> String {
    let mut out = String::with_capacity(cmd.len());
    let mut chars = cmd.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                out.push(' ');
                for c2 in chars.by_ref() {
                    if c2 == '\'' {
                        out.push(' ');
                        break;
                    }
                    out.push(' ');
                }
            }
            '"' => {
                out.push(' ');
                let mut prev_backslash = false;
                for c2 in chars.by_ref() {
                    if c2 == '"' && !prev_backslash {
                        out.push(' ');
                        break;
                    }
                    prev_backslash = c2 == '\\' && !prev_backslash;
                    out.push(' ');
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn kade_bash(cmd: &str) -> HookInput {
        HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": cmd})),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/roles/kade", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            chorus_worktree_override: None, trace_id: None, tool_output_is_error: None,}
    }

    fn silas_bash(cmd: &str) -> HookInput {
        HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": cmd})),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            chorus_worktree_override: None, trace_id: None, tool_output_is_error: None,}
    }

    // === heredoc/echo skip (absorbed from app_state_guard #1862) ===
    // Uses `kill -9` as the would-be-blocked command to verify the skip path.

    #[tokio::test]
    async fn test_allow_heredoc_skip() {
        let input = kade_bash("cat <<EOF\nkill -9 1234\nEOF");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_allow_echo_skip() {
        let input = kade_bash("echo 'kill -9 1234'");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === service-lifecycle.sh allowlist (absorbed from app_state_guard #1862) ===

    #[tokio::test]
    async fn test_allow_service_lifecycle() {
        let input = kade_bash("bash service-lifecycle.sh restart app");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === git commit/add in team repo ===

    #[tokio::test]
    async fn test_deny_git_commit() {
        let input = kade_bash("git commit -m 'test'");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        let body = r.stdout.unwrap();
        assert!(body.contains("werk-commit"), "names the real path: {body}");
        assert!(!body.contains("git-queue"), "no phantom tool: {body}");
    }

    #[tokio::test]
    async fn test_deny_git_add() {
        let input = kade_bash("git add .");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        let body = r.stdout.unwrap();
        assert!(body.contains("werk-commit"), "names the real path: {body}");
        assert!(!body.contains("git-queue"), "no phantom tool: {body}");
    }

    #[tokio::test]
    async fn test_allow_git_in_heredoc() {
        let input = kade_bash("cat <<'EOF'\ngit commit -m 'example'\nEOF");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === terraform (ask, not deny) ===

    #[tokio::test]
    async fn test_ask_terraform_apply() {
        let input = kade_bash("terraform apply");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("terraform"));
    }

    #[tokio::test]
    async fn test_ask_terraform_destroy() {
        let input = kade_bash("terraform destroy");
        let r = check(&input).await;
        assert!(r.stdout.is_some());
        assert!(r.stdout.unwrap().contains("terraform"));
    }

    #[tokio::test]
    async fn test_terraform_message_references_launchagents() {
        let input = kade_bash("terraform apply");
        let r = check(&input).await;
        let msg = r.stdout.unwrap();
        assert!(msg.contains("LaunchAgents"), "should reference LaunchAgents");
    }

    // === #2047: signal-0 liveness check exemption ===

    #[tokio::test]
    async fn test_allow_kill_signal_0() {
        let input = silas_bash("kill -0 12345");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "kill -0 should be allowed (liveness check)");
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_allow_kill_signal_0_flag() {
        let input = silas_bash("kill --signal 0 12345");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "kill --signal 0 should be allowed");
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_deny_kill_9_still_blocked() {
        let input = silas_bash("kill -9 12345");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "kill -9 should still be blocked");
        assert!(r.stdout.unwrap().contains("Manual process killing"));
    }

    // === Safe commands pass ===

    #[tokio::test]
    async fn test_allow_ls() {
        let input = kade_bash("ls -la");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_allow_app_state_sh() {
        let input = kade_bash("bash ../chorus/platform/scripts/app-state.sh status");
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === Non-Bash tool bypass ===

    #[tokio::test]
    async fn test_allow_write_tool() {
        let input = HookInput {
            tool_name: Some("Write".to_string()),
            tool_input: Some(json!({"file_path": "/tmp/test"})),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/roles/kade", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            chorus_worktree_override: None, trace_id: None, tool_output_is_error: None,};
        let r = check(&input).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === #2698: substring-match scope-too-broad ===
    // Hook regexes use \b word-boundary which matches inside quoted strings too.
    // Result: card descriptions, nudge bodies, grep patterns containing literal
    // git-mut text trigger false-positive blocks. Fix scopes matches to
    // command-position by stripping single- and double-quoted runs before check.

    #[tokio::test]
    async fn test_allow_git_commit_in_single_quoted_arg() {
        let input = kade_bash("cards add --desc 'X uses git commit internally'");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git inside single-quoted arg must not block; got: {:?}", r.stdout);
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_allow_git_push_in_double_quoted_arg() {
        let input = kade_bash(r#"nudge wren "remote delete via git push origin --delete""#);
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git inside double-quoted arg must not block; got: {:?}", r.stdout);
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_allow_git_reset_hard_in_grep_pattern() {
        let input = kade_bash("grep -e 'git reset --hard' file.txt");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git inside grep -e arg must not block; got: {:?}", r.stdout);
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_allow_git_cherry_pick_in_nudge_body() {
        let input = kade_bash(r#"nudge wren "use heredoc for git cherry-pick""#);
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git in quoted nudge body must not block; got: {:?}", r.stdout);
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_deny_real_git_commit_after_quoted_substring_strip() {
        let input = kade_bash("git commit -m 'has git push in body'");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "real git commit at command-position must still block");
        let body = r.stdout.unwrap();
        assert!(body.contains("werk-commit") && !body.contains("git-queue"),
            "blocks naming the real werk path, not the retired git-queue.sh: {body}");
    }

    #[tokio::test]
    async fn test_deny_real_git_push_with_quoted_arg() {
        let input = kade_bash(r#"git push origin "main:main""#);
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "real git push at command-position must still block");
    }

    // === #2711: deny raw git checkout / switch / branch (Mode-A close) ===
    // Candidate A from #2706: roles must route working-tree mutation through
    // git-queue.sh do_checkout/do_switch/do_branch (#2710). Deny-list bites
    // only after #2712 migrates skills off raw checkout.

    #[tokio::test]
    async fn test_deny_git_checkout_in_team_repo() {
        let input = kade_bash("git checkout main");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "raw git checkout must block");
        let body = r.stdout.unwrap();
        assert!(body.contains("chorus-werk"), "stderr names the real path: {body}");
        assert!(!body.contains("git-queue"), "stderr must not name retired git-queue.sh: {body}");
        assert!(body.contains("checkout"), "stderr names the op: {body}");
    }

    #[tokio::test]
    async fn test_deny_git_checkout_b_new_branch() {
        let input = kade_bash("git checkout -b kade/2711");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "raw git checkout -b must block");
    }

    #[tokio::test]
    async fn test_deny_git_switch_in_team_repo() {
        let input = kade_bash("git switch main");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "raw git switch must block");
    }

    #[tokio::test]
    async fn test_deny_git_branch_create() {
        let input = kade_bash("git branch kade/feature");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "raw git branch <new> must block");
    }

    #[tokio::test]
    async fn test_allow_git_checkout_in_quoted_arg() {
        // #2698 scoping: substring inside quoted args must not block.
        let input = kade_bash("nudge wren \"use git checkout SHA -- file for recovery\"");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git checkout inside quoted arg must not block: {:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_allow_git_checkout_in_heredoc() {
        let input = kade_bash("cat <<'EOF'\ngit checkout main\nEOF");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git checkout inside heredoc must not block (bypass)");
    }

    // #3290 — regression: NO mutating-git deny string may name the retired
    // git-queue.sh (deleted #3182/#3223). A guard that prescribes a phantom
    // tool forces the exact ungoverned hand-hacks it exists to prevent. Every
    // deny an agent sees must name a REAL path (a werk verb, or `git -C` for a
    // foreign repo). (Replaces the old test_allow_git_queue_sh_checkout, which
    // exercised a command that no longer exists on disk.)
    #[tokio::test]
    async fn test_no_deny_string_names_retired_git_queue() {
        // ALL 9 mutating-git deny paths — not just the 5 named in AC1. AC3 says
        // "zero git-queue references remain in the guardrail deny strings" (every
        // one), and AC2 says each names a real path. cherry-pick + reset --hard
        // were the gap the cold-eyes review caught.
        let ops = [
            "git commit -m 'x'",
            "git add .",
            "git push",
            "git rebase main",
            "git cherry-pick abc123",
            "git reset --hard HEAD~1",
            "git checkout main",
            "git switch main",
            "git branch newbranch",
        ];
        for op in ops {
            let r = check(&kade_bash(op)).await;
            let body = r.stdout.unwrap_or_else(|| panic!("`{op}` must be denied in team repo"));
            assert!(
                !body.to_lowercase().contains("git-queue"),
                "deny string for `{op}` still names the retired git-queue.sh: {body}"
            );
            assert!(
                body.contains("werk") || body.contains("git -C"),
                "deny string for `{op}` names no real path (werk verb / git -C): {body}"
            );
        }
    }

    #[tokio::test]
    async fn test_allow_git_branch_list_readonly() {
        // `git branch` with no args is a list operation (read-only).
        // `git branch -d <name>` is delete (not the create path #2711 covers).
        // Today the regex blocks both for safety; if it's too aggressive we
        // can scope further. Test pins current behavior.
        let input = kade_bash("git branch");
        let r = check(&input).await;
        // Read-only `git branch` list passes — only `git branch <name>` (create) blocks.
        assert!(r.stdout.is_none(), "git branch (list, no args) must pass: {:?}", r.stdout);
    }

    // === #2789 — git rebase cleanup flags must NOT be blocked ===
    // 2026-05-07: Wren got stuck mid-rebase, ran `git rebase --abort` to clean
    // up, and was blocked. The cleanup flags don't mutate new history; they
    // manage in-progress rebase state that already exists. Blocking them
    // strands roles in stuck-rebase with no recovery path.

    #[tokio::test]
    async fn test_allow_git_rebase_abort() {
        let input = kade_bash("git rebase --abort");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git rebase --abort must NOT block (cleanup, not mutation): {:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_allow_git_rebase_continue() {
        let input = kade_bash("git rebase --continue");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git rebase --continue must NOT block (resume, not new mutation): {:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_allow_git_rebase_skip() {
        let input = kade_bash("git rebase --skip");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git rebase --skip must NOT block (cleanup of one commit in in-progress rebase): {:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_allow_git_rebase_quit() {
        let input = kade_bash("git rebase --quit");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git rebase --quit must NOT block: {:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_allow_git_rebase_show_current_patch() {
        let input = kade_bash("git rebase --show-current-patch");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git rebase --show-current-patch is read-only inspection: {:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_allow_git_rebase_edit_todo() {
        let input = kade_bash("git rebase --edit-todo");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "git rebase --edit-todo edits the todo file in-progress, not history: {:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_deny_git_rebase_with_ref() {
        // The mutating form — block stays.
        let input = kade_bash("git rebase main");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "git rebase main must still block (mutation form)");
        let body = r.stdout.unwrap();
        assert!(body.contains("werk") && !body.contains("git-queue"),
            "blocks naming the real werk path, not the retired git-queue.sh: {body}");
    }

    #[tokio::test]
    async fn test_deny_git_rebase_bare() {
        // Bare `git rebase` defaults to @{upstream} — also a mutation, blocks.
        let input = kade_bash("git rebase");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "bare git rebase must still block (defaults to @{{upstream}} rebase, mutation)");
    }

    #[tokio::test]
    async fn test_deny_git_rebase_interactive_with_ref() {
        let input = kade_bash("git rebase -i HEAD~3");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "git rebase -i must still block (interactive rebase mutation)");
    }
}
