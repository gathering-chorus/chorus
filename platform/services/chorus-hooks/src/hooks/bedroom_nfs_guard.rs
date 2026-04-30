use crate::types::{HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;

static WRITE_VERBS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(rsync|sips|convert|magick|python3|pillow|ffmpeg|ffprobe|mv |cp |rm |mkdir).*(/Volumes/Gathering/)").unwrap()
});

static RSYNC_TO_GATHERING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"rsync.*\s+/Volumes/Gathering/").unwrap()
});

pub fn check(input: &HookInput) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let cmd = input.get_tool_input_str("command");
    if cmd.is_empty() {
        return HookResponse::allow();
    }

    let first_line = cmd.lines().next().unwrap_or("");

    // Skip heredocs and echo/printf
    if cmd.contains("<<") {
        return HookResponse::allow();
    }
    let fl_trimmed = first_line.trim_start();
    if fl_trimmed.starts_with("echo ")
        || fl_trimmed.starts_with("printf ")
        || fl_trimmed.starts_with("cat >")
        || fl_trimmed.starts_with("cat <<")
    {
        return HookResponse::allow();
    }

    // Allow if command already uses SSH to Bedroom
    if cmd.contains("ssh bedroom") || cmd.contains("ssh jeffbridwell@192.168.86.242") {
        return HookResponse::allow();
    }

    // Allow scp TO bedroom
    if cmd.contains("scp ") && cmd.contains(" bedroom:") {
        return HookResponse::allow();
    }

    // Must reference /Volumes/Gathering/
    if !cmd.contains("/Volumes/Gathering/") {
        return HookResponse::allow();
    }

    if WRITE_VERBS_RE.is_match(&cmd) {
        return HookResponse::allow_with_message(&serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": "\u{26a0} DEC-089: Processing data over NFS to /Volumes/Gathering/. Should this run on Bedroom via SSH instead? Use: ssh bedroom \"command\" or scp script bedroom:/tmp/ && ssh bedroom \"python3 /tmp/script.py\". NFS reads for app display are fine."
            }
        }).to_string());
    }

    if RSYNC_TO_GATHERING_RE.is_match(&cmd) {
        return HookResponse::allow_with_message(&serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": "\u{26a0} DEC-089: rsync writing to /Volumes/Gathering/ over NFS. Bulk writes should happen on Bedroom via SSH. Use: ssh bedroom \"rsync ...\" with Bedroom-local paths."
            }
        }).to_string());
    }

    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use crate::shared::state_paths::chorus_root;
    use serde_json::json;

    fn make_input(tool: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(json!({"command": "echo test", "file_path": "/tmp/test", "skill": "demo"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
            chorus_worktree_override: None,}
    }

    #[test]
    fn allows_non_matching_tool() {
        let input = make_input("Read");
        let r = check(&input);
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn allows_normal_input() {
        let input = make_input("Bash");
        let r = check(&input);
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn handles_nfs_path() {
        let mut input = make_input("Bash");
        input.tool_input = Some(serde_json::json!({"command": "cp file.txt /Volumes/Gathering/data/"}));
        let r = check(&input);
        assert!(r.exit_code == 0);
    }
}
