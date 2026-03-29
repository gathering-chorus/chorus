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
