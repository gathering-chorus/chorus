//! Persistent role observation — ambient gemba via hooks service.
//!
//! Captures a rolling digest of what each role is doing, written to shared
//! observation logs that any session can read on boot. The hooks service
//! is always running, so observations survive session boundaries.
//!
//! Design:
//! - PostToolUse events are digested into compact observations
//! - Observations written to /tmp/claude-team-scan/{role}-observations.jsonl
//! - Andon state determines which roles are active (worth observing)
//! - Session boot reads missed observations via `load_since()`
//! - No in-session cron needed — the service IS the observer

use crate::shared::state_paths::chorus_root;
use crate::state::{chorus_log, AppState};
use crate::types::{HookInput, Role};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SCAN_DIR: &str = "/tmp/claude-team-scan";
const MAX_OBSERVATIONS_PER_ROLE: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub ts: String,
    pub role: String,
    pub tool: String,
    pub action: String,
    /// Compact summary of what the role is doing
    pub digest: String,
    /// Card being worked on (from andon state)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
}

/// #2891 — Error context extracted from a tool's PostToolUse response.
/// Crate-private: only consumed by `observe()` and tests in this module.
#[allow(dead_code)] // lib-target dead-code analysis can't trace reachability through bin entry
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ToolErrorContext {
    /// Full error text (stderr / error message), no truncation per AC.
    pub error_full: String,
    /// Exit code when the response surfaces one (Bash typically does).
    pub exit_code: Option<i64>,
}

/// #2891 — Regex for "Exit code N" pattern Claude Code writes into Bash
/// tool_response on non-zero exit. Mirrors stop_on_error::EXIT_CODE_RE.
#[allow(dead_code)] // lib-target dead-code analysis can't trace reachability through bin entry
static OBSERVER_EXIT_CODE_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(?i)exit code (\d+)").unwrap()
});

/// #2891 — Detect tool failure from PostToolUse `tool_response`.
///
/// Claude Code's tool_response varies by tool:
/// - Bash: bare string like `"ls: ...\nExit code 1"` OR object with stdout/stderr
/// - Edit/Write: object with `is_error` / `isError` boolean on failure
///
/// Detection signals (any one fires error):
/// 1. Structured `is_error` / `isError` true
/// 2. Structured `interrupted` true
/// 3. Explicit `exit_code` / `exitCode` field != 0
/// 4. Regex-matched "Exit code N" with N > 0 in the response text
///
/// Returns None on success or when no signal fires.
#[allow(dead_code)] // lib-target dead-code analysis can't trace reachability through bin entry
pub(crate) fn detect_tool_error(input: &HookInput) -> Option<ToolErrorContext> {
    let resp = input.tool_response.as_ref()?;

    let (response_text, is_err_flag, interrupted, struct_exit_code) = match resp {
        serde_json::Value::String(s) => (s.clone(), false, false, None),
        serde_json::Value::Object(_) => {
            let is_err_flag = resp.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false)
                || resp.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
            let interrupted = resp.get("interrupted").and_then(|v| v.as_bool()).unwrap_or(false);
            let exit_code = resp
                .get("exit_code")
                .or_else(|| resp.get("exitCode"))
                .and_then(|v| v.as_i64());
            // Prefer stderr, then error, then content; fall back to full stringify.
            let text = resp
                .get("stderr")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| resp.get("error").and_then(|v| v.as_str()))
                .or_else(|| resp.get("content").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| resp.to_string());
            (text, is_err_flag, interrupted, exit_code)
        }
        _ => return None,
    };

    // Regex pull from text (Claude Code's Bash format) when no explicit field.
    let regex_exit: Option<i64> = OBSERVER_EXIT_CODE_RE
        .captures(&response_text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i64>().ok());

    let exit_code = struct_exit_code.or(regex_exit);
    let exit_nonzero = exit_code.map(|n| n != 0).unwrap_or(false);

    if !is_err_flag && !interrupted && !exit_nonzero {
        return None;
    }

    let error_full = if response_text.is_empty() {
        resp.to_string()
    } else {
        response_text
    };

    Some(ToolErrorContext {
        error_full,
        exit_code,
    })
}

/// Called on every PostToolUse — digest the tool call into an observation
pub async fn observe(input: &HookInput, _state: &AppState) {
    let role = input.role();
    if role == Role::Unknown {
        return;
    }

    let tool = input.tool_name_str();

    // Skip noisy tools that don't carry signal
    if matches!(tool, "Read" | "Glob" | "Grep" | "TaskList" | "TaskGet") {
        return;
    }

    // #2891 — Detect tool failure from response. The read-only filter below
    // is relaxed on the error path so a failing grep/cat/lookup still emits
    // signal (the assumption that followed it is wrong).
    let mut error = detect_tool_error(input);

    // #2891 — Drop benign-exit Bash from the error path. grep-no-match,
    // diff-found-differences, test-false etc. are not errors semantically —
    // they're expected non-zero exits. Reuses stop_on_error::BENIGN_COMMANDS
    // for a single source of truth (no competing implementations).
    if tool == "Bash" && error.is_some() {
        let cmd = input.get_tool_input_str("command");
        if super::stop_on_error::is_benign_bash(&cmd) {
            error = None;
        }
    }

    // #2220 — Read-only Bash filter (success path only). #2891 keeps the
    // success-path drop intact but lets errored read-only commands through
    // to the observer.error emission below.
    let is_read_only_cmd = tool == "Bash"
        && is_read_only_bash(&input.get_tool_input_str("command"));

    if is_read_only_cmd && error.is_none() {
        return;
    }

    let action = tool.to_string();
    let digest = digest_tool_call(input);

    if digest.is_empty() && error.is_none() {
        return;
    }

    // #2120 — Prefer inference from this tool call; fall back to declared state
    let inferred = infer_card_from_input(input);
    let card = inferred.clone().or_else(|| read_role_card(role.as_str()));

    // #2120 — Inline subsecond reconciliation: if this tool call carries a
    // strong card signal and it differs from declared state, flip declared.json
    // immediately (respecting a brief manual-override window).
    if let Some(inferred_card) = inferred.as_deref() {
        write_inferred_state(role.as_str(), inferred_card);
    }

    // Success-path digest: existing behavior. Skipped for read-only commands
    // (they only reach here on error) and when the digest is empty.
    if !is_read_only_cmd && !digest.is_empty() {
        let obs = Observation {
            ts: Utc::now().with_timezone(&super::clock_sync::boston_offset_pub()).format("%Y-%m-%dT%H:%M:%S%z").to_string(),
            role: role.as_str().to_string(),
            tool: tool.to_string(),
            action,
            digest: digest.clone(),
            card: card.clone(),
        };

        write_observation(&obs).await;
    }

    // #2891 — On error, emit observer.error spine event paired with the
    // corresponding observer.digest by {role, ~timestamp, digest}.
    if let Some(err) = error {
        let role_owned = role.as_str().to_string();
        let tool_owned = tool.to_string();
        let digest_owned = digest;
        let exit_owned = err.exit_code.map(|n| n.to_string());
        let err_text = err.error_full;
        let card_owned = card;
        tokio::spawn(async move {
            let mut kvs: Vec<(&str, &str)> = vec![
                ("tool", tool_owned.as_str()),
                ("digest", digest_owned.as_str()),
                ("error_full", err_text.as_str()),
            ];
            if let Some(ref ec) = exit_owned {
                kvs.push(("exit_code", ec.as_str()));
            }
            if let Some(ref c) = card_owned {
                kvs.push(("card", c.as_str()));
            }
            chorus_log("observer.error", role_owned.as_str(), &kvs).await;
        });
    }
}

/// #2220 — Classify a Bash command as read-only (observation noise) or
/// potentially mutating (carries decision/catch signal).
///
/// Read-only = first non-env token is a pure-read utility AND no write
/// redirect (>/>>/tee) is present. Conservative: any unknown command counts
/// as mutating to avoid silently losing signal.
pub fn is_read_only_bash(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.contains(" > ") || trimmed.contains(">>") || trimmed.contains(" | tee ") {
        return false;
    }
    let first = trimmed.split_whitespace()
        .find(|tok| !tok.contains('=') && *tok != "sudo" && *tok != "bash" && *tok != "-c")
        .unwrap_or("");
    let first = first.rsplit('/').next().unwrap_or(first).trim_matches('"').trim_matches('\'');
    matches!(
        first,
        "grep" | "egrep" | "fgrep" | "rg" | "ack"
        | "cat" | "head" | "tail" | "less" | "more"
        | "ls" | "ll" | "la" | "find" | "fd" | "tree"
        | "wc" | "sort" | "uniq" | "cut" | "tr" | "awk" | "sed"
        | "stat" | "file" | "du" | "df"
        | "echo" | "printf" | "pwd" | "which" | "whereis" | "type"
        | "curl" | "wget" | "ping" | "dig" | "host" | "nslookup"
        | "ps" | "top" | "htop"
        | "sqlite3" | "jq" | "yq"
    )
}

/// #2120 — The card a role is working on is the WIP card they own on the
/// board. Period. Commenting on someone else's card, writing a brief, or
/// mentioning a card in a commit doesn't count as "working on" — only
/// ownership does. If a role has no WIP card, their card state is blank.
///
/// Reads /tmp/board-wip-snapshot.json (maintained by pulse::assemble_board).
pub fn wip_card_owned_by(role: &str) -> Option<String> {
    let content = std::fs::read_to_string("/tmp/board-wip-snapshot.json").ok()?;
    let arr = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    for card in arr.as_array()? {
        let owner = card.get("owner")?.as_str()?.to_lowercase();
        if owner == role.to_lowercase() {
            let id = card.get("id")?.as_u64()?;
            return Some(id.to_string());
        }
    }
    None
}

/// #2120 — Card = board WIP ownership. No ownership, no card.
pub fn infer_card_from_input(input: &HookInput) -> Option<String> {
    let role = input.role();
    if role == crate::types::Role::Unknown {
        return None;
    }
    wip_card_owned_by(role.as_str())
}

/// Retained for its future utility but unused in the live path after
/// Jeff's 2026-04-17 direction: no card fallback from tool-call patterns.
/// If ownership says no card, that IS the state. Kept behind `#[cfg(test)]`
/// so the regex coverage doesn't rot if we ever want it as a weak signal.
#[cfg(test)]
fn infer_card_from_tool_patterns(input: &HookInput) -> Option<String> {
    use regex::Regex;
    let tool = input.tool_name_str();

    match tool {
        "Bash" => {
            let cmd = input.get_tool_input_str("command");
            if cmd.is_empty() {
                return None;
            }

            let cards_op = Regex::new(
                r"\bcards\s+(?:move|comment|done|demo|reject|block)\s+(\d{3,6})\b"
            ).ok()?;
            if let Some(c) = cards_op.captures(&cmd).and_then(|c| c.get(1)) {
                return Some(c.as_str().to_string());
            }

            let commit_hash = Regex::new(r#"-m\s+["']?[^"']*?#(\d{3,6})\b"#).ok()?;
            if let Some(c) = commit_hash.captures(&cmd).and_then(|c| c.get(1)) {
                return Some(c.as_str().to_string());
            }

            let chat_topic = Regex::new(r#"chat(?:\.sh)?\s+\S+\s+\S+\s+["']?#(\d{3,6})\b"#).ok()?;
            if let Some(c) = chat_topic.captures(&cmd).and_then(|c| c.get(1)) {
                return Some(c.as_str().to_string());
            }

            None
        }
        "Write" | "Edit" => {
            let path = input.get_tool_input_str("file_path");
            if path.is_empty() || !path.contains("/briefs/") {
                return None;
            }
            let brief = Regex::new(r"/briefs/[^/]*?(?:demo|card)-(\d{3,6})").ok()?;
            brief
                .captures(&path)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        }
        _ => None,
    }
}

/// #2168 AC-9 — Reconciler writes to `<role>-inferred.json` exclusively.
/// Never touches `<role>-declared.json` (that's role_state.rs territory).
///
/// Schema: {role, card, ts, source:"inferred"} — minimal. Filename carries
/// the writer-identity semantic, so no card_declared/card_inferred sub-fields.
/// `ts` also serves as last_emit for readers: treat inferred.json as stale
/// if ts older than ~5 min (role has gone quiet, inferred card is historical).
/// TTL is enforced by readers, not the writer.
///
/// De-dup: skip write when prev inferred.card equals the new one. Atomic
/// tmp-then-rename to survive concurrent tool calls.
///
/// #2120 predecessor mutated declared.json and enforced a manual-override
/// window. Override logic is now a read-side concern — pulse/tile combine
/// declared + inferred and surface divergence.
fn write_inferred_state(role: &str, inferred_card: &str) {
    let path = format!("{}/{}-inferred.json", SCAN_DIR, role);
    let tmp = format!("{}/{}-inferred.json.tmp.{}", SCAN_DIR, role, std::process::id());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // De-dup: skip write if prev inferred.card matches. Tolerate absent or
    // unparseable prev — any failure in read path means we write fresh.
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(prev) = serde_json::from_str::<serde_json::Value>(&content) {
            let prev_card = prev
                .get("card")
                .map(|v| {
                    if v.is_number() {
                        v.to_string()
                    } else {
                        v.as_str().unwrap_or("").to_string()
                    }
                })
                .unwrap_or_default();
            if prev_card == inferred_card {
                return;
            }
        }
    }

    let card_val: serde_json::Value = match inferred_card.parse::<i64>() {
        Ok(n) => serde_json::Value::Number(n.into()),
        Err(_) => serde_json::Value::String(inferred_card.to_string()),
    };

    let json = serde_json::json!({
        "role": role,
        "card": card_val,
        "ts": now,
        "source": "inferred",
    });

    let out = match serde_json::to_string(&json) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Atomic write: tmp + rename
    if std::fs::write(&tmp, &out).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Digest a tool call into a compact human-readable summary
fn digest_tool_call(input: &HookInput) -> String {
    let tool = input.tool_name_str();

    match tool {
        "Bash" => {
            let cmd = input.get_tool_input_str("command");
            let first_line = cmd.lines().next().unwrap_or("");
            // Extract the meaningful part of the command
            let short: String = first_line.chars().take(120).collect();
            if short.is_empty() {
                return String::new();
            }
            // Classify
            if short.contains("cargo test") || short.contains("npm test") || short.contains("jest") {
                format!("running tests: {}", truncate(&short, 80))
            } else if short.contains("cargo build") || short.contains("npm run build") {
                format!("building: {}", truncate(&short, 80))
            } else if short.contains("git commit") || short.contains("git-queue") {
                "committing changes".to_string()
            } else if short.contains("board-ts") || short.contains("/cards ") {
                format!("board op: {}", truncate(&short, 80))
            } else if short.contains("app-state") {
                format!("service op: {}", truncate(&short, 80))
            } else if short.contains("nudge") {
                format!("nudging: {}", truncate(&short, 80))
            } else if short.contains("role-state") {
                format!("state change: {}", truncate(&short, 80))
            } else {
                format!("bash: {}", truncate(&short, 80))
            }
        }
        "Write" => {
            let path = input.get_tool_input_str("file_path");
            let short_path = short_path(&path);
            format!("writing {}", short_path)
        }
        "Edit" => {
            let path = input.get_tool_input_str("file_path");
            let short_path = short_path(&path);
            format!("editing {}", short_path)
        }
        "Agent" => {
            let desc = input.get_tool_input_str("description");
            if desc.is_empty() {
                "spawning agent".to_string()
            } else {
                format!("agent: {}", truncate(&desc, 80))
            }
        }
        "Skill" => {
            let skill = input.get_tool_input_str("skill");
            format!("skill: /{}", skill)
        }
        _ => String::new(),
    }
}

/// Write an observation to the role's observation log
async fn write_observation(obs: &Observation) {
    let dir = PathBuf::from(SCAN_DIR);
    let _ = tokio::fs::create_dir_all(&dir).await;

    let log_path = dir.join(format!("{}-observations.jsonl", obs.role));

    // Append
    if let Ok(line) = serde_json::to_string(obs) {
        use tokio::io::AsyncWriteExt;
        if let Ok(mut f) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await
        {
            let _ = f.write_all(line.as_bytes()).await;
            let _ = f.write_all(b"\n").await;
        }
    }

    // Rotate if too large
    rotate_if_needed(&log_path, &obs.role).await;

    // Log to spine
    let role = obs.role.clone();
    let digest = obs.digest.clone();
    tokio::spawn(async move {
        chorus_log(
            "observer.digest",
            &role,
            &[("digest", &digest)],
        )
        .await;
    });
}

/// Keep only the last MAX_OBSERVATIONS_PER_ROLE entries
async fn rotate_if_needed(path: &PathBuf, _role: &str) {
    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(_) => return,
    };

    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= MAX_OBSERVATIONS_PER_ROLE {
        return;
    }

    // Keep last N
    let keep = &lines[lines.len() - MAX_OBSERVATIONS_PER_ROLE..];
    let new_content = keep.join("\n") + "\n";
    let _ = tokio::fs::write(path, new_content).await;
}

/// Read the card a role is currently working on.
/// Primary: recent card.pulled event from chorus.log (most current).
/// Fallback: andon state file (for backwards compat when log is unavailable).
fn read_role_card(role: &str) -> Option<String> {
    // Primary: check chorus.log for most recent card.pulled by this role
    let log_path = format!("{}/platform/logs/chorus.log", chorus_root());
    if let Ok(content) = std::fs::read_to_string(log_path) {
        for line in content.lines().rev().take(200) {
            if line.contains("card.pulled") && line.contains(&format!("\"role\":\"{}\"", role)) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(card) = parsed.get("card").and_then(|c| c.as_str()) {
                        return Some(card.to_string());
                    }
                }
            }
        }
    }

    // Fallback: andon state file
    let state_file = PathBuf::from(format!("{}/{}-declared.json", SCAN_DIR, role));
    let content = std::fs::read_to_string(&state_file).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    parsed
        .get("card")
        .and_then(|c| {
            if c.is_number() {
                Some(c.to_string())
            } else {
                c.as_str().map(|s| s.to_string())
            }
        })
}

/// Load observations for a role since a given ISO timestamp
#[allow(dead_code)]
pub fn load_since(role: &str, since: &str) -> Vec<Observation> {
    let log_path = PathBuf::from(format!("{}/{}-observations.jsonl", SCAN_DIR, role));
    let content = match std::fs::read_to_string(&log_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    content
        .lines()
        .filter_map(|line| serde_json::from_str::<Observation>(line).ok())
        .filter(|obs| obs.ts.as_str() > since)
        .collect()
}

/// Load observations for all OTHER roles since a given timestamp.
/// This is what a session calls on boot to catch up.
#[allow(dead_code)]
pub fn load_missed(my_role: &str, since: &str) -> Vec<Observation> {
    let roles = ["wren", "silas", "kade"];
    let mut all = Vec::new();
    for role in &roles {
        if *role == my_role {
            continue;
        }
        // Only load if the role was recently active
        if !is_role_active(role) {
            continue;
        }
        all.extend(load_since(role, since));
    }
    all.sort_by(|a, b| a.ts.cmp(&b.ts));
    all
}

/// Check if a role has been active recently (andon state exists and is not stale)
#[allow(dead_code)]
fn is_role_active(role: &str) -> bool {
    let state_file = PathBuf::from(format!("{}/{}-declared.json", SCAN_DIR, role));
    let content = match std::fs::read_to_string(&state_file) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };

    // Check if session is alive
    if parsed.get("session_alive").and_then(|v| v.as_bool()) == Some(false) {
        return false;
    }

    // Check if state was updated within last 30 minutes
    if let Some(ts) = parsed.get("ts").and_then(|v| v.as_u64()) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        return now.saturating_sub(ts) < 1800;
    }

    false
}

/// Format observations into a human-readable summary for session boot
#[allow(dead_code)]
pub fn format_missed_summary(observations: &[Observation]) -> String {
    if observations.is_empty() {
        return String::new();
    }

    let mut summary = String::from("## Cross-role activity since your last session\n\n");

    // Group by role
    let mut by_role: std::collections::HashMap<&str, Vec<&Observation>> = std::collections::HashMap::new();
    for obs in observations {
        by_role.entry(obs.role.as_str()).or_default().push(obs);
    }

    for (role, obs) in &by_role {
        summary.push_str(&format!("**{}** ({} actions):\n", role, obs.len()));
        // Show last 10 per role
        let show = if obs.len() > 10 { &obs[obs.len() - 10..] } else { obs.as_slice() };
        for o in show {
            let ts_short: String = o.ts.chars().skip(11).take(5).collect(); // HH:MM
            let card_label = o.card.as_deref().map(|c| format!(" [#{}]", c)).unwrap_or_default();
            summary.push_str(&format!("  - {} {}{}\n", ts_short, o.digest, card_label));
        }
        summary.push('\n');
    }

    summary
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Find the last char boundary at or before max
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

fn short_path(path: &str) -> String {
    // Strip common prefix to show just the meaningful part
    let prefix = format!("{}/", chorus_root());
    let stripped = path
        .strip_prefix(&prefix)
        .unwrap_or(path);
    truncate(stripped, 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_post_input(tool: &str, tool_input: serde_json::Value, cwd: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(tool_input),
            tool_response: Some(json!("ok")),
            session_id: Some("test".to_string()),
            cwd: Some(cwd.to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
            trace_id: None, tool_output_is_error: None,}
    }

    // === digest_tool_call tests ===

    #[test]
    fn test_digest_bash_cargo_test() {
        let input = make_post_input(
            "Bash",
            json!({"command": "cargo test 2>&1"}),
            &format!("{}/architect", chorus_root()),
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("running tests:"), "got: {}", d);
    }

    #[test]
    fn test_digest_bash_build() {
        let input = make_post_input(
            "Bash",
            json!({"command": "cargo build --release"}),
            &format!("{}/architect", chorus_root()),
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("building:"), "got: {}", d);
    }

    #[test]
    fn test_digest_bash_board() {
        let input = make_post_input(
            "Bash",
            json!({"command": "bash ../chorus/platform/scripts/cards move 1594 Done"}),
            &format!("{}/architect", chorus_root()),
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("board op:"), "got: {}", d);
    }

    #[test]
    fn test_digest_bash_nudge() {
        let input = make_post_input(
            "Bash",
            json!({"command": "bash ../chorus/platform/scripts/nudge.sh wren 'done with 1594'"}),
            &format!("{}/architect", chorus_root()),
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("nudging:"), "got: {}", d);
    }

    // === is_read_only_bash tests (#2220) ===

    #[test]
    fn is_read_only_bash_catches_common_reads() {
        assert!(is_read_only_bash("grep foo /etc/passwd"));
        assert!(is_read_only_bash("cat /tmp/file"));
        assert!(is_read_only_bash("ls -la /var"));
        assert!(is_read_only_bash("find . -name '*.ts'"));
        assert!(is_read_only_bash("wc -l file.txt"));
        assert!(is_read_only_bash("/usr/bin/grep foo bar"));
        assert!(is_read_only_bash("curl -s http://localhost:3340/api/chorus/health"));
        assert!(is_read_only_bash("sqlite3 ~/.chorus/index.db 'SELECT COUNT(*) FROM messages'"));
    }

    #[test]
    fn is_read_only_bash_rejects_mutations() {
        assert!(!is_read_only_bash("cards move 1234 Done"));
        assert!(!is_read_only_bash("chorus-log some.event silas"));
        assert!(!is_read_only_bash("launchctl kickstart -k gui/$(id -u)/com.foo"));
        assert!(!is_read_only_bash("git commit -m 'x'"));
        assert!(!is_read_only_bash("npm install"));
    }

    #[test]
    fn is_read_only_bash_rejects_redirects() {
        assert!(!is_read_only_bash("grep foo bar > /tmp/out"));
        assert!(!is_read_only_bash("cat file >> /tmp/acc"));
    }

    #[test]
    fn is_read_only_bash_handles_env_prefix() {
        assert!(is_read_only_bash("FOO=bar grep baz file"));
        assert!(is_read_only_bash("HOME=/tmp cat /etc/hosts"));
    }

    #[test]
    fn is_read_only_bash_empty_is_noop() {
        assert!(is_read_only_bash(""));
        assert!(is_read_only_bash("   "));
    }

    #[test]
    fn test_digest_bash_generic() {
        let input = make_post_input(
            "Bash",
            json!({"command": "ls -la /tmp"}),
            &format!("{}/architect", chorus_root()),
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("bash:"), "got: {}", d);
    }

    #[test]
    fn test_digest_write() {
        let input = make_post_input(
            "Write",
            json!({"file_path": &format!("{}/platform/services/chorus-hooks/src/hooks/observer.rs", chorus_root()), "content": "test"}),
            &format!("{}/architect", chorus_root()),
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("writing "), "got: {}", d);
        assert!(d.contains("observer.rs"), "got: {}", d);
    }

    #[test]
    fn test_digest_edit() {
        let input = make_post_input(
            "Edit",
            json!({"file_path": &format!("{}/architect/CLAUDE.md", chorus_root()), "old_string": "a", "new_string": "b"}),
            &format!("{}/architect", chorus_root()),
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("editing "), "got: {}", d);
    }

    #[test]
    fn test_digest_agent() {
        let input = make_post_input(
            "Agent",
            json!({"description": "Find chorus-hooks source files"}),
            &format!("{}/architect", chorus_root()),
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("agent:"), "got: {}", d);
    }

    #[test]
    fn test_digest_skill() {
        let input = make_post_input(
            "Skill",
            json!({"skill": "reboot"}),
            &format!("{}/architect", chorus_root()),
        );
        let d = digest_tool_call(&input);
        assert_eq!(d, "skill: /reboot");
    }

    #[test]
    fn test_digest_read_skipped() {
        // Read is filtered out before digest is called, but if called directly:
        let input = make_post_input(
            "Read",
            json!({"file_path": "/tmp/test"}),
            &format!("{}/architect", chorus_root()),
        );
        let d = digest_tool_call(&input);
        assert!(d.is_empty(), "Read should produce empty digest, got: {}", d);
    }

    // === Observation serialization ===

    #[test]
    fn test_observation_roundtrip() {
        let obs = Observation {
            ts: "2026-03-21T17:00:00Z".to_string(),
            role: "silas".to_string(),
            tool: "Bash".to_string(),
            action: "Bash".to_string(),
            digest: "running tests: cargo test".to_string(),
            card: Some("1594".to_string()),
        };
        let json = serde_json::to_string(&obs).unwrap();
        let back: Observation = serde_json::from_str(&json).unwrap();
        assert_eq!(back.role, "silas");
        assert_eq!(back.digest, "running tests: cargo test");
        assert_eq!(back.card, Some("1594".to_string()));
    }

    #[test]
    fn test_observation_no_card() {
        let obs = Observation {
            ts: "2026-03-21T17:00:00Z".to_string(),
            role: "wren".to_string(),
            tool: "Write".to_string(),
            action: "Write".to_string(),
            digest: "writing briefs/test.md".to_string(),
            card: None,
        };
        let json = serde_json::to_string(&obs).unwrap();
        assert!(!json.contains("card"), "card:None should be skipped");
    }

    // === detect_tool_error tests (#2891) ===

    fn make_input_with_response(tool: &str, tool_input: serde_json::Value, response: serde_json::Value) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(tool_input),
            tool_response: Some(response),
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
            trace_id: None,
            tool_output_is_error: None,
        }
    }

    #[test]
    fn detect_tool_error_none_on_success() {
        let input = make_input_with_response(
            "Bash",
            json!({"command": "echo hi"}),
            json!({"stdout": "hi\n", "stderr": "", "interrupted": false}),
        );
        assert!(detect_tool_error(&input).is_none());
    }

    #[test]
    fn detect_tool_error_catches_is_error_snake() {
        let input = make_input_with_response(
            "Bash",
            json!({"command": "false"}),
            json!({"stdout": "", "stderr": "command failed", "is_error": true}),
        );
        let err = detect_tool_error(&input).expect("should detect error");
        assert_eq!(err.error_full, "command failed");
    }

    #[test]
    fn detect_tool_error_catches_is_error_camel() {
        let input = make_input_with_response(
            "Edit",
            json!({"file_path": "/no/such/file"}),
            json!({"content": "File does not exist", "isError": true}),
        );
        let err = detect_tool_error(&input).expect("should detect camelCase error");
        assert_eq!(err.error_full, "File does not exist");
    }

    #[test]
    fn detect_tool_error_catches_interrupted() {
        let input = make_input_with_response(
            "Bash",
            json!({"command": "sleep 100"}),
            json!({"stdout": "", "stderr": "", "interrupted": true}),
        );
        assert!(detect_tool_error(&input).is_some());
    }

    #[test]
    fn detect_tool_error_catches_nonzero_exit() {
        let input = make_input_with_response(
            "Bash",
            json!({"command": "git push"}),
            json!({"stdout": "", "stderr": "remote rejected", "exit_code": 1}),
        );
        let err = detect_tool_error(&input).expect("nonzero exit_code is error");
        assert_eq!(err.error_full, "remote rejected");
        assert_eq!(err.exit_code, Some(1));
    }

    #[test]
    fn detect_tool_error_none_on_zero_exit() {
        let input = make_input_with_response(
            "Bash",
            json!({"command": "git status"}),
            json!({"stdout": "clean", "stderr": "", "exit_code": 0}),
        );
        assert!(detect_tool_error(&input).is_none());
    }

    #[test]
    fn detect_tool_error_none_on_bare_string_response_success() {
        // Plain string with no "Exit code N" → success.
        let input = make_input_with_response(
            "Bash",
            json!({"command": "echo hi"}),
            json!("ok"),
        );
        assert!(detect_tool_error(&input).is_none());
    }

    #[test]
    fn detect_tool_error_catches_bare_string_exit_code() {
        // Claude Code's actual Bash error shape: bare string with "Exit code N".
        let input = make_input_with_response(
            "Bash",
            json!({"command": "ls /nope"}),
            json!("ls: /nope: No such file or directory\nExit code 1"),
        );
        let err = detect_tool_error(&input).expect("should detect bare-string exit code");
        assert_eq!(err.exit_code, Some(1));
        assert!(err.error_full.contains("No such file or directory"));
    }

    #[test]
    fn detect_tool_error_bare_string_zero_is_success() {
        let input = make_input_with_response(
            "Bash",
            json!({"command": "true"}),
            json!("Exit code 0"),
        );
        assert!(detect_tool_error(&input).is_none());
    }

    #[test]
    fn detect_tool_error_preserves_full_stderr_no_truncation() {
        // AC: "all the data" — no 200-byte truncation. Build a 50KB stderr
        // and verify it round-trips intact.
        let huge = "E".repeat(50_000);
        let input = make_input_with_response(
            "Bash",
            json!({"command": "cargo test"}),
            json!({"stdout": "", "stderr": huge.clone(), "is_error": true}),
        );
        let err = detect_tool_error(&input).expect("should detect");
        assert_eq!(err.error_full.len(), 50_000);
        assert_eq!(err.error_full, huge);
    }

    #[test]
    fn detect_tool_error_still_flags_benign_at_detection_layer() {
        // detect_tool_error itself is shape-agnostic — it reports exit_code=1
        // regardless of whether the command was benign. The benign-skip is an
        // observe()-layer concern (uses BENIGN_COMMANDS list). This test pins
        // the contract so a future refactor doesn't accidentally couple them.
        let input = make_input_with_response(
            "Bash",
            json!({"command": "grep foo /nonexistent"}),
            json!("Exit code 1"),
        );
        let err = detect_tool_error(&input).expect("detect layer still fires");
        assert_eq!(err.exit_code, Some(1));
    }

    #[test]
    fn benign_bash_classifier_skips_no_match_grep() {
        // grep-no-match is the canonical benign exit. Pin it.
        assert!(super::super::stop_on_error::is_benign_bash("grep foo /etc/passwd"));
        assert!(super::super::stop_on_error::is_benign_bash("rg pattern src/"));
        assert!(super::super::stop_on_error::is_benign_bash("diff a.txt b.txt"));
        assert!(super::super::stop_on_error::is_benign_bash("git status"));
        assert!(super::super::stop_on_error::is_benign_bash("git diff HEAD~1"));
        assert!(super::super::stop_on_error::is_benign_bash("test -f /tmp/x"));
        // Word-boundary: "grepy" must NOT match "grep"
        assert!(!super::super::stop_on_error::is_benign_bash("grepy foo"));
        assert!(!super::super::stop_on_error::is_benign_bash("diffstat report"));
        // Env prefix tolerated
        assert!(super::super::stop_on_error::is_benign_bash("FOO=bar grep baz"));
        // Path prefix tolerated
        assert!(super::super::stop_on_error::is_benign_bash("/usr/bin/grep foo bar"));
        // Mutating commands are NOT benign
        assert!(!super::super::stop_on_error::is_benign_bash("rm -rf /tmp/x"));
        assert!(!super::super::stop_on_error::is_benign_bash("git commit -m x"));
    }

    #[test]
    fn detect_tool_error_falls_back_to_error_then_content_then_stringify() {
        // Prefer stderr → error → content → full stringify
        let no_stderr = make_input_with_response(
            "Tool",
            json!({}),
            json!({"error": "boom", "is_error": true}),
        );
        assert_eq!(detect_tool_error(&no_stderr).unwrap().error_full, "boom");

        let no_error_field = make_input_with_response(
            "Tool",
            json!({}),
            json!({"content": "denied", "is_error": true}),
        );
        assert_eq!(detect_tool_error(&no_error_field).unwrap().error_full, "denied");
    }

    // === short_path ===

    #[test]
    fn test_short_path_strips_prefix() {
        let p = short_path(&format!("{}/architect/CLAUDE.md", chorus_root()));
        assert_eq!(p, "architect/CLAUDE.md");
    }

    #[test]
    fn test_short_path_no_prefix() {
        let p = short_path("/tmp/test.txt");
        assert_eq!(p, "/tmp/test.txt");
    }

    // === format_missed_summary ===

    #[test]
    fn test_format_empty() {
        assert!(format_missed_summary(&[]).is_empty());
    }

    #[test]
    fn test_format_with_observations() {
        let obs = vec![
            Observation {
                ts: "2026-03-21T17:00:00Z".to_string(),
                role: "kade".to_string(),
                tool: "Bash".to_string(),
                action: "Bash".to_string(),
                digest: "running tests: npm test".to_string(),
                card: Some("1556".to_string()),
            },
            Observation {
                ts: "2026-03-21T17:05:00Z".to_string(),
                role: "kade".to_string(),
                tool: "Edit".to_string(),
                action: "Edit".to_string(),
                digest: "editing handler.ts".to_string(),
                card: Some("1556".to_string()),
            },
        ];
        let summary = format_missed_summary(&obs);
        assert!(summary.contains("kade"));
        assert!(summary.contains("2 actions"));
        assert!(summary.contains("running tests"));
        assert!(summary.contains("[#1556]"));
    }

    // === truncate ===

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let result = truncate("hello world this is a long string", 10);
        assert_eq!(result, "hello worl...");
    }

    // === load_since (filesystem-dependent, test the filter logic) ===

    #[test]
    fn test_load_since_nonexistent() {
        let result = load_since("nonexistent_role_xyz", "2026-01-01T00:00:00Z");
        assert!(result.is_empty());
    }

    // === #2120 — infer_card_from_input ===

    #[test]
    fn test_infer_card_from_cards_move() {
        let input = make_post_input(
            "Bash",
            json!({"command": "bash /path/to/cards move 1234 WIP"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("1234".to_string()));
    }

    #[test]
    fn test_infer_card_from_cards_comment() {
        let input = make_post_input(
            "Bash",
            json!({"command": "cards comment 2119 'acp brief'"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("2119".to_string()));
    }

    #[test]
    fn test_infer_card_from_cards_done() {
        let input = make_post_input(
            "Bash",
            json!({"command": "bash cards done 2117"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("2117".to_string()));
    }

    #[test]
    fn test_infer_card_from_cards_demo() {
        let input = make_post_input(
            "Bash",
            json!({"command": "cards demo 2120"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("2120".to_string()));
    }

    #[test]
    fn test_infer_card_from_cards_reject() {
        let input = make_post_input(
            "Bash",
            json!({"command": "cards reject 1999 'blocked by X'"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("1999".to_string()));
    }

    #[test]
    fn test_infer_card_from_cards_block() {
        let input = make_post_input(
            "Bash",
            json!({"command": "cards block 1998 'waiting on Y'"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("1998".to_string()));
    }

    #[test]
    fn test_infer_card_from_commit_hash_marker() {
        let input = make_post_input(
            "Bash",
            json!({"command": "git-queue.sh commit foo.rs -- -m \"silas: acp #2114 ship thing\""}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("2114".to_string()));
    }

    #[test]
    fn test_infer_card_from_acp_phrase() {
        let input = make_post_input(
            "Bash",
            json!({"command": "bash git-queue commit -- -m 'acp #2117 — extend daily-review'"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("2117".to_string()));
    }

    #[test]
    fn test_infer_card_from_swat_phrase() {
        let input = make_post_input(
            "Bash",
            json!({"command": "git-queue.sh commit -- -m 'silas: swat #2130 stale test'"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("2130".to_string()));
    }

    #[test]
    fn test_infer_card_from_chat_topic() {
        let input = make_post_input(
            "Bash",
            json!({"command": "bash chat.sh start kade '#1847 buildout review'"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("1847".to_string()));
    }

    #[test]
    fn test_infer_card_from_brief_demo_path() {
        let input = make_post_input(
            "Write",
            json!({"file_path": "/Users/jeffbridwell/CascadeProjects/chorus/roles/wren/briefs/2026-04-16-demo-2114.md", "content": "x"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("2114".to_string()));
    }

    #[test]
    fn test_infer_card_from_brief_card_path() {
        let input = make_post_input(
            "Write",
            json!({"file_path": "/Users/jeffbridwell/CascadeProjects/chorus/roles/kade/briefs/2026-04-11-card-1832-done.md", "content": "x"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), Some("1832".to_string()));
    }

    #[test]
    fn test_infer_card_none_for_generic_bash() {
        let input = make_post_input(
            "Bash",
            json!({"command": "ls -la /tmp && df -h"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), None);
    }

    #[test]
    fn test_infer_card_none_for_generic_write() {
        let input = make_post_input(
            "Write",
            json!({"file_path": "/tmp/scratch.md", "content": "x"}),
            "/tmp",
        );
        assert_eq!(infer_card_from_tool_patterns(&input), None);
    }
}
