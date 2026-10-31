// @domain: spine — placed by chorus-crawl's domain rules (#4645)
use crate::shared::state_paths::chorus_root;
use crate::state::chorus_log;
use crate::types::{permission_ask_json, permission_deny_json, HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;

/// #4025 — first time this guard ran in this process; a lower bound on daemon
/// uptime for the deny message (the daemon has been alive at least this long).
static DAEMON_FIRST_SEEN: LazyLock<std::time::Instant> = LazyLock::new(std::time::Instant::now);

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

// #4202 — `security login-keychain -s` / `default-keychain -s` rewrite which
// keychain the CALLER's login session uses, and under `sudo -u chorus-<role>`
// they still land on the caller: Jeff's Terminal. On 2026-09-17 that left his
// session pointing at no login keychain, so every process that touched the
// keychain raised a password panel nothing could answer, no agent could reach,
// and no kill command closed. He hard powered off the Library Mac.
// The read forms (`security login-keychain`, no -s) stay allowed — that is how
// chorus-keychain-repair verifies itself.
static KEYCHAIN_SET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bsecurity\s+(login-keychain|default-keychain)\b[^\n]*\s-s\b").unwrap()
});

// Any `security` subcommand driven at a role account from another account's
// shell. A role account's keychain is that account's own business; reaching it
// through sudo is the shape that produced the unclosable panel.
static SUDO_ROLE_SECURITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bsudo\b[^\n]*\s-u\s+chorus-\w+[^\n]*\bsecurity\s").unwrap()
});

// #4707 — a land is Jeff's go and nothing else (DEC-048). On 10-23 at 06:03 an
// agent enqueued cicd-land by raw curl and #4704 merged with accepter=jeff while
// Jeff was away; at 19:17 the day before a bare werk-merge recorded "flow". The
// go step is the only door: dagu itself enqueues cicd-land from it, and that
// never passes through a tool call. Same rule for every actor and every model.
// Defense in depth only: the signed go in werk-merge is the guard (#4707 AC1).
static LAND_ENQUEUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bdagu\b[^\n;&|]*\b(enqueue|start|retry|restart)\b[^\n;&|]*\bcicd-land\b").unwrap()
});

// Matched on the raw command: the URL is usually quoted, and quoted runs are
// stripped before the command-position checks.
static LAND_API_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(curl|wget|http)\b[^\n]*dags/cicd-land/(enqueue|start|retry)").unwrap()
});

static WERK_MERGE_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\bchorus-runner\s+call\s+werk-merge\b|(^|[\s;&|/])werk-merge\s+\d)").unwrap()
});

const LAND_DENY: &str = "BLOCKED: only Jeff's go lands a card (#4707, DEC-048). cicd-land runs from the card's go step, which dagu answers itself; a raw `dagu enqueue cicd-land`, a POST to /dags/cicd-land, or a direct werk-merge call skips the go. Same rule for every actor. Bring the demo to Jeff with `/cws <card>`. A land that stopped after his go is retried on that go with `platform/scripts/werk-land-retry <card>` (#4707 reopen), never by hand.";

// #4810 — npm ci / install deletes node_modules before it installs. When a werk's
// node_modules is a symlink into canonical, that delete empties canonical's live
// tree: 10-21 (wren-4672/pulse) and 10-30 13:54:37 (cards — the cards CLI broke for
// every role). A memory note did not stop the second one; a refusal does.
static NPM_INSTALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bnpm\s+(ci|install|i|clean-install|add)\b").unwrap()
});

static CD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[;&|(]\s*)cd\s+([^\s;&|)]+)").unwrap()
});

static NPM_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"--prefix[ =]([^\s;&|)]+)").unwrap()
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
    // #4707 — the werk-merge MCP tool is the same door as the CLI.
    if input.tool_name_str() == "mcp__chorus-api__werk-merge" {
        log_guardrail("deny", "land-without-go").await;
        return HookResponse::deny(&permission_deny_json(LAND_DENY));
    }
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let cmd = input.get_tool_input_str("command");
    let cwd = input.cwd.as_deref().unwrap_or("");
    if cmd.is_empty() {
        return HookResponse::allow();
    }

    // #4707 — before the heredoc/echo skips: a land wrapped in a heredoc is still a land.
    if is_land_bypass(&cmd) {
        log_guardrail("deny", "land-without-go").await;
        return HookResponse::deny(&permission_deny_json(LAND_DENY));
    }
    tracing::trace!(hook = "infra_guardrails", phase = "receive", cmd_len = cmd.len(), "checking bash command");

    // #4810 — before the heredoc/echo skips: nothing is deleted yet when this refuses.
    if let Some((dir, target)) = npm_through_symlink(&cmd, cwd) {
        log_guardrail("deny", "npm-through-symlink").await;
        return HookResponse::deny(&permission_deny_json(&format!(
            "BLOCKED: npm would install in {dir} through its node_modules symlink -> {target} (#4810). \
             npm ci/install deletes node_modules first, so it would empty {target}; on 10-30 that \
             broke the cards CLI for every role. Remove the link first (`rm {dir}/node_modules` \
             deletes only the link), then install. werk-pull gives each werk a real node_modules.",
            dir = dir.display(),
            target = target.display(),
        )));
    }

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

    // #4025 — a restart of THIS daemon, arriving at THIS daemon. The daemon is
    // answering the request, so it is alive; the only thing the command can do
    // is SIGTERM a working guard and fail-close the whole team until launchd
    // brings it back (5 deaths in 2 days, every one a role obeying the shim's
    // old "unreachable → kickstart" recipe after a timeout under load). The
    // genuine recovery path is untouched: when the daemon is DOWN the shim
    // cannot reach this code and its connect-failure carve-out lets the same
    // command through (shared::recovery). Checked BEFORE the agent-state allow
    // below, or `agent-state.sh chorus-hooks restart` would slip past.
    if crate::shared::recovery::is_hooks_restart_command(&cmd) {
        log_guardrail("deny", "hooks-restart-while-alive").await;
        let uptime = DAEMON_FIRST_SEEN.elapsed().as_secs();
        return HookResponse::deny(&permission_deny_json(&format!(
            "BLOCKED: chorus-hooks is ALIVE (pid {}, answering for {}s) — it just evaluated this command. \
             Restarting it SIGTERMs a working guard and locks every role out until launchd respawns it (#4025). \
             A shim timeout under load means BUSY, not dead: retry the tool call. \
             If the daemon is truly down, this exact command is allowed by the shim's connect-failure carve-out; \
             a new binary deploys via `chorus-deploy chorus-hooks` / `agent-state.sh chorus-hooks deploy`.",
            std::process::id(), uptime
        )));
    }

    // Allow service-lifecycle.sh (absorbed from app_state_guard #1862)
    // Allow agent-state.sh for LaunchAgent lifecycle (#2009)
    // Allow chorus-scratch-kill (#3750) — the owner-sanctioned, spine-witnessed
    // path for UNMANAGED scratch processes. It refuses managed-service pids
    // itself (typed, tested), so this is not a kill bypass: raw kill/pkill
    // stays blocked below.
    if cmd.contains("service-lifecycle.sh") || cmd.contains("app-state.sh") || cmd.contains("agent-state.sh")
        || cmd.contains("chorus-scratch-kill") {
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

    // #4202 — keychain settings are the one surface where a mistake reaches a
    // GUI panel: no agent can see it, no agent can close it, and the person in
    // front of the machine has no working answer. Refuse both shapes.
    if KEYCHAIN_SET_RE.is_match(&cmd) || SUDO_ROLE_SECURITY_RE.is_match(&cmd) {
        log_guardrail("deny", "keychain-mutation").await;
        return HookResponse::deny(&permission_deny_json(
            "BLOCKED: keychain settings are not an agent surface (#4202). \
             `security login-keychain -s` / `default-keychain -s` apply to the CALLING login session — \
             Jeff's Terminal — even wrapped in `sudo -u chorus-<role>`. On 2026-09-17 that pointed his \
             session at no login keychain; every process that touched it raised a password panel he \
             could not answer and no agent could close, and he hard powered off the Mac. \
             Read forms (`security login-keychain` with no -s) are allowed. \
             To repair a session that is already stuck, Jeff runs `chorus-keychain-repair` himself."
        ));
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

/// #4707 — a command that lands a card without the go step. Pure.
fn is_land_bypass(cmd: &str) -> bool {
    let positional = strip_quoted_runs(cmd);
    LAND_ENQUEUE_RE.is_match(&positional)
        || LAND_API_RE.is_match(cmd)
        || WERK_MERGE_CALL_RE.is_match(&positional)
}

/// #4810 — the directory an `npm ci/install` in `cmd` installs into, and the
/// symlink target of its node_modules, when that node_modules is a symlink. The
/// directory is `--prefix`, else the last `cd` before npm, else `cwd`. Pure but
/// for the one lstat.
fn npm_through_symlink(cmd: &str, cwd: &str) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let positional = strip_quoted_runs(cmd);
    let m = NPM_INSTALL_RE.find(&positional)?;
    let unquote = |s: &str| s.trim_matches(|c| c == '\'' || c == '"').to_string();
    let after = cmd.get(m.start()..).unwrap_or("");
    let before = cmd.get(..m.start()).unwrap_or("");
    let named = NPM_PREFIX_RE.captures(after)
        .or_else(|| CD_RE.captures_iter(before).last())
        .map(|c| unquote(&c[1]));
    let mut dir = std::path::PathBuf::from(cwd);
    if let Some(d) = named {
        let d = match d.strip_prefix("~/") {
            Some(rest) => format!("{}/{}", std::env::var("HOME").unwrap_or_default(), rest),
            None => d,
        };
        dir.push(d);
    }
    let nm = dir.join("node_modules");
    if !nm.symlink_metadata().ok()?.file_type().is_symlink() {
        return None;
    }
    let target = std::fs::read_link(&nm).ok()?;
    Some((dir, target))
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
            tool_use_id: None,
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": cmd})),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/roles/kade", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            card_type: None, entrypoint: None,
            trace_id: None, tool_output_is_error: None,}
    }

    fn silas_bash(cmd: &str) -> HookInput {
        HookInput {
            tool_use_id: None,
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": cmd})),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            card_type: None, entrypoint: None,
            trace_id: None, tool_output_is_error: None,}
    }

    // === #4707 — a land without Jeff's go is refused, for every actor ===

    fn abby_bash(cmd: &str) -> HookInput {
        let mut i = kade_bash(cmd);
        i.cwd = Some(format!("{}/roles/abby-normal", chorus_root()));
        i.deploy_role = Some("abby-normal".to_string());
        i
    }

    fn denied_as_land(r: &HookResponse) -> bool {
        r.stdout.as_deref().is_some_and(|o| o.contains("\"permissionDecision\":\"deny\"") && o.contains("#4707"))
    }

    #[tokio::test]
    async fn test_4707_abbys_0603_curl_replayed_is_refused() {
        let r = check(&abby_bash("curl -s -X POST 'http://localhost:8526/api/v1/dags/cicd-land/enqueue' -d '{\"params\":\"CARD=4704 ROLE=abby-normal\"}'")).await;
        assert!(denied_as_land(&r), "{:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_4707_raw_dagu_enqueue_of_the_land_is_refused_for_claude_and_abby() {
        for input in [
            kade_bash("env -u CHORUS_ROLE dagu --config platform/config/dagu.yaml enqueue cicd-land -- CARD=4704 ROLE=kade"),
            abby_bash("dagu --config platform/config/dagu.yaml enqueue cicd-land -- CARD=4704"),
            silas_bash("dagu --config platform/config/dagu.yaml retry cicd-land --run-id 034x"),
        ] {
            let r = check(&input).await;
            assert!(denied_as_land(&r), "{:?}", r.stdout);
        }
    }

    /// #4707 reopen — the retry verb is the one way back for a land that stopped
    /// after the go; it checks the recorded go itself, so the hook lets it through.
    #[tokio::test]
    async fn test_4707_the_land_retry_verb_passes_and_the_refusal_names_it() {
        for input in [silas_bash("platform/scripts/werk-land-retry 4715"), abby_bash("platform/scripts/werk-land-retry 4715 --dry-run")] {
            let r = check(&input).await;
            assert!(r.stdout.is_none(), "{:?}", r.stdout);
        }
        let r = check(&silas_bash("dagu --config platform/config/dagu.yaml retry --run-id=034x --step=skill-borg-ask --downstream cicd-land")).await;
        assert!(denied_as_land(&r), "{:?}", r.stdout);
        assert!(r.stdout.as_deref().unwrap_or("").contains("werk-land-retry <card>"), "{:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_4707_direct_werk_merge_is_refused_cli_and_mcp() {
        for cmd in ["ROLE=abby-normal ~/.chorus/bin/chorus-runner call werk-merge card=4704",
                    "/Users/x/CascadeProjects/chorus-werk/wren-bin/werk-merge 4704 wren --atomic"] {
            let r = check(&abby_bash(cmd)).await;
            assert!(denied_as_land(&r), "{cmd}: {:?}", r.stdout);
        }
        let mut mcp = kade_bash("");
        mcp.tool_name = Some("mcp__chorus-api__werk-merge".to_string());
        mcp.tool_input = Some(json!({"card": "4704"}));
        assert!(denied_as_land(&check(&mcp).await));
    }

    /// #4707 AC2 — Abby's real path (agy): her native PreToolUse JSON, mapped by
    /// her AgentRuntime row exactly as the runtime hook maps it, then the land
    /// guard. `chorus_hooks::` resolves in both the lib and the bin build (lib.rs:
    /// extern crate self), and keeps runtime names out of runtime_rows.rs (R8).
    async fn through_agy(command: &str) -> HookResponse {
        use chorus_hooks::runtime_rows::{canonical, row_in, table, Moment};
        let t = table();
        let row = row_in(&t, "agy").expect("agy row in the generated table");
        let cwd = format!("{}/roles/abby-normal", chorus_root());
        let raw = json!({"conversationId": "c-4707", "toolCall": {"name": "run_command",
            "args": {"CommandLine": command, "Cwd": cwd}}});
        let mut v = canonical(row, Moment::BeforeTool, &raw).expect("agy maps run_command");
        v["deploy_role"] = json!("abby-normal");
        let input: HookInput = serde_json::from_value(v).expect("canonical JSON is a HookInput");
        assert_eq!(input.tool_name_str(), "Bash", "agy run_command reaches the guard as Bash");
        check(&input).await
    }

    #[tokio::test]
    async fn test_4707_abbys_curl_through_her_agy_hook_path_is_refused() {
        let r = through_agy("curl -s -X POST 'http://localhost:8526/api/v1/dags/cicd-land/enqueue' -d '{}'").await;
        assert!(denied_as_land(&r), "{:?}", r.stdout);
        let r = through_agy("dagu --config platform/config/dagu.yaml enqueue cicd-land -- CARD=4704").await;
        assert!(denied_as_land(&r), "{:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_4707_negative_proof_agy_path_lets_the_sanctioned_enqueue_through() {
        let r = through_agy("dagu --config platform/config/dagu.yaml enqueue cicd -- CARD=4707 ROLE=abby-normal").await;
        assert!(r.stdout.is_none(), "{:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_4707_a_land_inside_a_heredoc_is_still_refused() {
        let r = check(&abby_bash("bash <<EOF\ndagu enqueue cicd-land -- CARD=4704\nEOF")).await;
        assert!(denied_as_land(&r), "{:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_4707_negative_proof_the_sanctioned_paths_still_pass() {
        for cmd in [
            "env -u CHORUS_ROLE -u ROLE dagu --config platform/config/dagu.yaml enqueue cicd -- CARD=4707 ROLE=wren",
            "env -u CHORUS_ROLE -u ROLE dagu --config platform/config/dagu.yaml enqueue pair -- CARD=4707 ROLE=wren NAVIGATOR=silas",
            "dagu --config platform/config/dagu.yaml status cicd-land --run-id 034kthMf",
            "grep -n werk-merge platform/pipelines/cicd-land.yaml",
            "sed -n 1,40p platform/services/werk-merge/src/lib.rs",
        ] {
            let r = check(&kade_bash(cmd)).await;
            assert!(r.stdout.is_none(), "{cmd} must pass: {:?}", r.stdout);
        }
    }

    // === #4025 — restart of the LIVE daemon is refused; deploy + other agents pass ===

    #[tokio::test]
    async fn test_4025_kickstart_of_live_hooks_daemon_is_denied_naming_alive_state() {
        let r = check(&kade_bash("launchctl kickstart -k gui/$UID/com.chorus.hooks")).await;
        let out = r.stdout.expect("must deny");
        assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
        assert!(out.contains("ALIVE"), "refusal must name the daemon's state: {out}");
        assert!(out.contains("#4025"), "{out}");
    }

    #[tokio::test]
    async fn test_4025_agent_state_restart_of_hooks_is_denied_before_the_agent_state_allow() {
        let r = check(&silas_bash("bash platform/scripts/agent-state.sh chorus-hooks restart")).await;
        assert!(r.stdout.is_some(), "agent-state restart of hooks must not slip past the agent-state allow");
    }

    #[tokio::test]
    async fn test_4025_negative_proof_deploy_of_hooks_still_allowed() {
        // A new binary legitimately restarts the service — deploy is not a restart.
        let r = check(&silas_bash("bash platform/scripts/agent-state.sh chorus-hooks deploy")).await;
        assert!(r.stdout.is_none(), "deploy must pass: {:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_4025_negative_proof_kickstart_of_another_agent_still_allowed() {
        let r = check(&silas_bash("launchctl kickstart -k gui/$UID/com.chorus.athena-make")).await;
        assert!(r.stdout.is_none(), "other agents are not this daemon: {:?}", r.stdout);
    }

    #[tokio::test]
    async fn test_4025_negative_proof_bare_mention_is_not_a_restart() {
        let r = check(&silas_bash("grep -n com.chorus.hooks ~/Library/LaunchAgents/com.chorus.hooks.plist")).await;
        assert!(r.stdout.is_none(), "reading the plist is not a restart: {:?}", r.stdout);
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

    /// #3750 — the sanctioned scratch-kill path is allowed (the script itself
    /// refuses managed pids, typed + bats-proven). The negative side of this
    /// pair is test_deny_kill_9_still_blocked below: raw kill stays BLOCKED,
    /// so this allowlist entry is a path, not a hole.
    #[tokio::test]
    async fn test_allow_chorus_scratch_kill() {
        let input = silas_bash("chorus-scratch-kill 18044 leftover demo viewer");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "chorus-scratch-kill should be allowed (#3750)");
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
            tool_use_id: None,
            tool_name: Some("Write".to_string()),
            tool_input: Some(json!({"file_path": "/tmp/test"})),
            tool_response: None,
            session_id: None,
            cwd: Some(format!("{}/roles/kade", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            card_type: None, entrypoint: None,
            trace_id: None, tool_output_is_error: None,};
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

    // === #4202: keychain settings are not an agent surface ===
    // Negative proof first: the exact commands typed on 2026-09-17 that pointed
    // Jeff's login session at no keychain. If the guard can't refuse THESE, it
    // refuses nothing that matters.

    #[tokio::test]
    async fn test_deny_login_keychain_set_under_sudo_to_role_account() {
        let input = silas_bash(
            "sudo -u chorus-silas -H security login-keychain -d user -s /Users/chorus-silas/Library/Keychains/login.keychain-db"
        );
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "the command that broke Jeff's session must block");
        let body = r.stdout.unwrap();
        assert!(body.contains("chorus-keychain-repair"), "deny names the repair Jeff runs: {body}");
    }

    #[tokio::test]
    async fn test_deny_login_keychain_set_without_sudo() {
        let input = silas_bash("security login-keychain -d user -s ~/Library/Keychains/login.keychain-db");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "setting the login keychain must block with or without sudo");
    }

    #[tokio::test]
    async fn test_deny_default_keychain_set() {
        let input = kade_bash("security default-keychain -s /Users/chorus-kade/Library/Keychains/login.keychain-db");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "setting the default keychain must block");
    }

    #[tokio::test]
    async fn test_deny_sudo_role_account_any_security_subcommand() {
        let input = silas_bash("sudo -u chorus-wren security create-keychain -p hunter chorus.keychain");
        let r = check(&input).await;
        assert!(r.stdout.is_some(), "any security subcommand aimed at a role account must block");
    }

    // The separating half: the guard must still pass the READ forms, or it
    // cannot tell "asking what the setting is" from "changing it", and the
    // repair loses the only way it can verify itself.

    #[tokio::test]
    async fn test_allow_login_keychain_read() {
        let input = silas_bash("security login-keychain");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "reading the login keychain setting must pass");
    }

    #[tokio::test]
    async fn test_allow_default_keychain_read() {
        let input = silas_bash("security default-keychain && security list-keychains");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "reading keychain settings must pass");
    }

    #[tokio::test]
    async fn test_allow_sudo_role_account_without_security() {
        let input = silas_bash("sudo -u chorus-silas id -un");
        let r = check(&input).await;
        assert!(r.stdout.is_none(), "sudo to a role account is not itself the problem");
    }

    // #4810 — npm through a symlinked node_modules. A fixture "canonical" tree and
    // a fixture werk package whose node_modules links to it.
    fn npm_world() -> (tempfile::TempDir, String, String) {
        let t = tempfile::tempdir().unwrap();
        let canon = t.path().join("canon/node_modules");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("marker"), "live").unwrap();
        let linked = t.path().join("werk/linked");
        std::fs::create_dir_all(&linked).unwrap();
        std::os::unix::fs::symlink(&canon, linked.join("node_modules")).unwrap();
        let real = t.path().join("werk/real/node_modules");
        std::fs::create_dir_all(&real).unwrap();
        let w = t.path().join("werk").to_string_lossy().into_owned();
        let c = canon.to_string_lossy().into_owned();
        (t, w, c)
    }

    fn bash_in(cmd: &str, cwd: &str) -> HookInput {
        let mut i = kade_bash(cmd);
        i.cwd = Some(cwd.to_string());
        i
    }

    #[tokio::test]
    async fn test_4810_deny_npm_ci_through_symlink_naming_target() {
        let (_t, werk, canon) = npm_world();
        for cmd in ["npm ci", "npm install --no-audit", "npm i"] {
            let r = check(&bash_in(cmd, &format!("{werk}/linked"))).await;
            let out = r.stdout.unwrap_or_default();
            assert!(out.contains("BLOCKED") && out.contains(&canon), "{cmd}: must refuse naming {canon}: {out}");
        }
        assert!(std::path::Path::new(&canon).join("marker").exists(), "canonical untouched");
    }

    #[tokio::test]
    async fn test_4810_deny_via_cd_and_prefix() {
        let (_t, werk, canon) = npm_world();
        for cmd in [format!("cd {werk}/linked && npm ci"), format!("npm ci --prefix {werk}/linked"), "cd linked; npm install".to_string()] {
            let r = check(&bash_in(&cmd, &werk)).await;
            let out = r.stdout.unwrap_or_default();
            assert!(out.contains(&canon), "{cmd}: must refuse: {out}");
        }
    }

    // The separating half: a real node_modules, or a non-installing npm, passes.
    #[tokio::test]
    async fn test_4810_allow_real_node_modules_and_read_forms() {
        let (_t, werk, _canon) = npm_world();
        let r = check(&bash_in("npm ci", &format!("{werk}/real"))).await;
        assert!(r.stdout.is_none(), "a real node_modules installs freely");
        for cmd in ["npm ls --depth=0", "npm test", "npm run build"] {
            let r = check(&bash_in(cmd, &format!("{werk}/linked"))).await;
            assert!(r.stdout.is_none(), "{cmd} does not delete node_modules");
        }
        let r = check(&bash_in("grep 'npm ci' notes.md", &format!("{werk}/linked"))).await;
        assert!(r.stdout.is_none(), "quoted text is not a command");
    }
}
