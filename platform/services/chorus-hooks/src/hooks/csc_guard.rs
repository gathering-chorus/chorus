//! CSC (Canonical Storage Convention) enforcement hook.
//! PreToolUse on Bash: blocks writes of pipeline artifacts to /tmp/,
//! warns on domain data writes outside /Volumes/Gathering/.
//! See infrastructure-constraints.md § Canonical Storage Convention.

use crate::types::{permission_deny_json, HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;

// File extensions that are pipeline artifacts
static ARTIFACT_EXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\.(jpg|jpeg|png|json|ttl|nt|nq|rdf|csv)").unwrap()
});

// Domain data paths that should be under /Volumes/Gathering/
static DOMAIN_KEYWORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(photo|music|video|social|facebook|linkedin|takeout|apple-music|thumbnail)").unwrap()
});

// Note: Rust regex crate does not support look-ahead (?!...).
// Instead we check for write-to-path and then exclude Gathering/dev/null in code.

/// Allowlisted /tmp/ paths — these are ephemeral by design, not persistence mistakes
fn is_tmp_allowlisted(path: &str) -> bool {
    // Session context files — regenerated every session start
    if path.contains("session-start-") || path.contains("session-context-") { return true; }
    // Pair session files
    if path.contains("pair-") { return true; }
    // Team scan state — polling state, not persistence
    if path.contains("claude-team-scan") { return true; }
    // Watchdog state
    if path.contains("watchdog") { return true; }
    // Chorus hooks PID/socket — runtime, not data
    if path.contains("chorus-hooks") { return true; }
    // Cruft scan output
    if path.contains("cruft-scan") { return true; }
    // Bridge uploads — audio, images, messages (#1782, #1938)
    if path.contains("bridge-audio") || path.contains("bridge-uploads") || path.contains("bridge-messages") { return true; }
    false
}

pub fn check(input: &HookInput) -> HookResponse {
    let tool = input.tool_name_str();

    // AC#1: Block Write/Edit to /tmp/ unless allowlisted (#1938)
    if tool == "Write" || tool == "Edit" {
        let file_path = input.get_tool_input_str("file_path");
        if file_path.starts_with("/tmp/") && !is_tmp_allowlisted(&file_path) {
            let suggestion = if file_path.contains(".json") {
                "Use ~/.chorus/ or data/ for JSON state files."
            } else if file_path.contains(".md") {
                "Use your role's directory (architect/, product-manager/, engineer/) for markdown."
            } else if file_path.contains(".sh") {
                "Use platform/scripts/ for shell scripts."
            } else {
                "Use a project-scoped path. /tmp/ is cleaned on reboot."
            };
            return HookResponse::deny(&permission_deny_json(
                &format!("CSC violation: writing to /tmp/ is not persistence. {suggestion} \
                          Allowlisted: session-start-*, pair-*, claude-team-scan/, watchdog/, chorus-hooks*.")
            ));
        }
    }

    if tool != "Bash" {
        return HookResponse::allow();
    }

    let cmd = input.get_tool_input_str("command");
    if cmd.is_empty() {
        return HookResponse::allow();
    }

    // Skip commands that are clearly not file operations:
    // nudge, echo, curl POST (message sending), board-ts, chorus-log, chat
    if cmd.contains("nudge.sh")
        || cmd.contains("board-ts") || cmd.contains("/cards ")
        || cmd.contains("chorus-log")
        || cmd.contains("chat.sh")
        || cmd.contains("curl -s -X POST http://localhost:3470")
        || cmd.starts_with("echo ")
    {
        return HookResponse::allow();
    }

    // Check 1: Block artifact writes to /tmp/
    // Key distinction: /tmp/ as DESTINATION is a violation.
    // /tmp/ as SOURCE (reading, moving out of) is allowed — that's CSC cleanup.
    let has_file_output = cmd.contains("sips ")
        || cmd.contains("convert ")
        || cmd.contains("ffmpeg ")
        || cmd.contains("python3 ") && (cmd.contains("--out") || cmd.contains("> /tmp/"));
    let has_artifact_ext = ARTIFACT_EXT.is_match(&cmd);

    // Detect /tmp/ as write destination (not source)
    let tmp_is_destination =
        // Redirect output to /tmp/
        cmd.contains("> /tmp/") || cmd.contains(">> /tmp/")
        // --out /tmp/
        || cmd.contains("--out /tmp/") || cmd.contains("--out=/tmp/")
        // cp/scp/mv where /tmp/ is the LAST path argument (destination)
        // If the command ends with /tmp/... it's a destination
        || (cmd.contains("cp ") || cmd.contains("scp ") || cmd.contains("mv "))
           && cmd.split_whitespace().last().map_or(false, |last| last.starts_with("/tmp/"));

    if (has_file_output || has_artifact_ext) && tmp_is_destination {
        // Allow reads, mkdir, status checks, and moves OUT of /tmp/
        if cmd.contains("cat /tmp/") || cmd.contains("ls /tmp/")
            || cmd.contains("mkdir") || cmd.contains("wc -l")
            || cmd.contains("head ") || cmd.contains("tail ")
            || cmd.contains("rm /tmp/") || cmd.contains("rm -rf /tmp/")
        {
            return HookResponse::allow();
        }

        return HookResponse::deny(&permission_deny_json(
            "CSC violation: pipeline artifacts go to /Volumes/Gathering/<domain>/generated/, not /tmp/. \
             Thumbnails, harvest output, TTL, and N-Triples are persistent data. \
             See infrastructure-constraints.md § Canonical Storage Convention."
        ));
    }

    // Check 2: Warn on domain data writes outside /Volumes/Gathering/
    // Only trigger on actual file write commands with domain keywords
    let is_domain_write = DOMAIN_KEYWORDS.is_match(&cmd)
        && (cmd.contains("sips ") || cmd.contains("cp ") || cmd.contains("mv ")
            || cmd.contains("scp ") || cmd.contains("> /"));
    // Check for writes to non-Gathering paths (done in code since regex doesn't support look-ahead)
    let writes_outside = is_domain_write
        && (cmd.contains("--out ") || cmd.contains("> /") || cmd.contains(">> /"))
        && !cmd.contains("/Volumes/Gathering/")
        && !cmd.contains("/Volumes/VideosNew/Gathering/")
        && !cmd.contains("/dev/null");

    if is_domain_write && writes_outside && !tmp_is_destination {
        if cmd.starts_with("ssh ") || cmd.contains("git ") || cmd.contains("cat ") {
            return HookResponse::allow();
        }

        return HookResponse::warn_stderr(
            "CSC warning: domain data should go to /Volumes/Gathering/<domain>/source/ or generated/. \
             See infrastructure-constraints.md § Canonical Storage Convention."
        );
    }

    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn make_bash(cmd: &str) -> HookInput {
        HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": cmd})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        }
    }

    fn make_write(path: &str) -> HookInput {
        HookInput {
            tool_name: Some("Write".to_string()),
            tool_input: Some(json!({"file_path": path, "content": "test"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        }
    }

    fn make_edit(path: &str) -> HookInput {
        HookInput {
            tool_name: Some("Edit".to_string()),
            tool_input: Some(json!({"file_path": path, "old_string": "a", "new_string": "b"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        }
    }

    // === Write/Edit /tmp blocking (#1938) ===

    #[test]
    fn blocks_write_to_tmp_random_file() {
        let r = check(&make_write("/tmp/my-state.json"));
        assert!(r.stdout.is_some(), "Should block Write to /tmp/my-state.json");
    }

    #[test]
    fn blocks_edit_to_tmp_random_file() {
        let r = check(&make_edit("/tmp/some-script.sh"));
        assert!(r.stdout.is_some(), "Should block Edit to /tmp/some-script.sh");
    }

    #[test]
    fn allows_write_to_session_start() {
        let r = check(&make_write("/tmp/session-start-silas.md"));
        assert_eq!(r.exit_code, 0, "session-start files are allowlisted");
    }

    #[test]
    fn allows_write_to_pair_file() {
        let r = check(&make_write("/tmp/pair-2022.md"));
        assert_eq!(r.exit_code, 0, "pair files are allowlisted");
    }

    #[test]
    fn allows_write_to_team_scan() {
        let r = check(&make_write("/tmp/claude-team-scan/silas-declared.json"));
        assert_eq!(r.exit_code, 0, "team-scan files are allowlisted");
    }

    #[test]
    fn allows_write_to_watchdog() {
        let r = check(&make_write("/tmp/watchdog/silas.state"));
        assert_eq!(r.exit_code, 0, "watchdog files are allowlisted");
    }

    #[test]
    fn allows_write_outside_tmp() {
        let r = check(&make_write("/Users/jeffbridwell/CascadeProjects/chorus/architect/test.md"));
        assert_eq!(r.exit_code, 0, "Non-/tmp writes should pass");
    }

    // === Existing Bash tests ===

    #[test]
    fn allows_normal_bash_commands() {
        let input = make_bash("ls -la /Users/jeffbridwell/CascadeProjects");
        let r = check(&input);
        assert_eq!(r.exit_code, 0);
        assert!(r.stdout.is_none());
    }

    #[test]
    fn allows_non_bash_tools() {
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(json!({"file_path": "/tmp/test.txt"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
        };
        let r = check(&input);
        assert_eq!(r.exit_code, 0);
    }
}

    #[test]
    fn allows_read_commands() {
        let input = HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(serde_json::json!({"command": "cat /tmp/session-start-silas.md"})),
            tool_response: None, session_id: Some("test".into()),
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".into()),
            prompt: None, stop_hook_active: None, hook_type: None,
            deploy_role: Some("silas".into()),
        };
        let r = check(&input);
        assert_eq!(r.exit_code, 0);
    }
