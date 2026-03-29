use crate::state::{append_log, AppState};
use crate::types::{HookInput, HookResponse};
use chrono::Utc;
use regex::Regex;
use std::sync::LazyLock;

static ERROR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)Error:|ENOENT|ECONNREFUSED|Permission denied|command not found|fatal:|npm ERR!|EACCES|No such file|ETIMEDOUT|\bOOM\b|\bkilled\b|No space left|ENOMEM|syntax error").unwrap()
});

static SEVERITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)exit code [1-9]|command failed|non-zero").unwrap()
});

static PRE_COMMIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Pre-commit|Trivy|pre-commit|npm audit|TypeScript passed|Lint passed").unwrap()
});

/// PreToolUse path — log every tool invocation
pub async fn pre_tool_use(input: &HookInput, state: &AppState) -> HookResponse {
    let role = input.role();
    let tool = input.tool_name_str();

    let detail = match tool {
        "Bash" => input.get_tool_input_str("command"),
        "Read" => input.get_tool_input_str("file_path"),
        "Write" | "Edit" => input.get_tool_input_str("file_path"),
        "Glob" | "Grep" => input.get_tool_input_str("pattern"),
        "Task" => input.get_tool_input_str("description"),
        "WebFetch" => input.get_tool_input_str("url"),
        "WebSearch" => input.get_tool_input_str("query"),
        _ => String::new(),
    };

    let detail_clean: String = detail
        .replace('"', "'")
        .replace('\n', " ")
        .chars()
        .take(200)
        .collect();

    let ts = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let line = serde_json::json!({
        "timestamp": ts,
        "level": "info",
        "appName": "tool-telemetry",
        "component": "pre",
        "event": "tool_call",
        "role": role.as_str(),
        "tool": tool,
        "detail": detail_clean
    })
    .to_string();

    let log_path = state.config.log_dir.join("permission-prompts.log");
    append_log(&log_path, &line).await;

    HookResponse::allow()
}

/// PostToolUse:Bash path — capture errors + fingerprint
pub async fn post_tool_use_bash(input: &HookInput, state: &AppState) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let response = input.tool_response_str();
    if response.is_empty() {
        return HookResponse::allow();
    }

    let command = input.get_tool_input_str("command");
    let cmd_first = command.lines().next().unwrap_or("");

    // Skip commands that search for errors or produce noisy output
    let skip_patterns = [
        "grep", "board-ts", "command-errors.log", "auto-error-carded",
        "--dry-run", "--status", "chorus-test", "jest", "vitest", "npm",
        "playwright", "npx", "git commit", "git add", "git push",
        "werk-init", "close-out", "reboot", "trivy", "Trivy scan",
        "pre-commit", "tail", "head", "cat", "smoke-check", "--help",
        "--version",
    ];
    for pat in &skip_patterns {
        if cmd_first.contains(pat) {
            return HookResponse::allow();
        }
    }

    // Skip pre-commit hook output
    let first_lines: String = response.lines().take(5).collect::<Vec<_>>().join("\n");
    if PRE_COMMIT_RE.is_match(&first_lines) {
        return HookResponse::allow();
    }

    // Scan for error keywords
    let error_line = response.lines().find(|l| ERROR_RE.is_match(l));
    let error_line = match error_line {
        Some(l) => l,
        None => return HookResponse::allow(),
    };

    let role = input.role();
    let severity = if SEVERITY_RE.is_match(&response) {
        "error"
    } else {
        "warn"
    };

    // Fingerprint
    let cmd_base: String = command
        .split_whitespace()
        .next()
        .unwrap_or("")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .chars()
        .take(20)
        .collect();

    let fingerprint = if error_line.contains("ENOENT") || error_line.contains("No such file") {
        format!("ENOENT_{}", cmd_base)
    } else if error_line.contains("ECONNREFUSED") {
        format!("ECONNREFUSED_{}", cmd_base)
    } else if error_line.contains("Permission denied") || error_line.contains("EACCES") {
        format!("PERM_DENIED_{}", cmd_base)
    } else if error_line.contains("command not found") {
        format!("CMD_NOT_FOUND_{}", cmd_base)
    } else if error_line.contains("fatal:") {
        format!("GIT_FATAL_{}", cmd_base)
    } else if error_line.contains("No space left") {
        format!("DISK_FULL_{}", cmd_base)
    } else if error_line.contains("ETIMEDOUT") || error_line.contains("timed out") {
        format!("TIMEOUT_{}", cmd_base)
    } else if error_line.contains("OOM") || error_line.contains("ENOMEM") {
        format!("OOM_{}", cmd_base)
    } else if error_line.contains("syntax error") || error_line.contains("Syntax error") {
        format!("SYNTAX_ERR_{}", cmd_base)
    } else if error_line.contains("npm ERR!") {
        format!("NPM_ERR_{}", cmd_base)
    } else {
        // SHA-256 fallback — use a simple hash
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        error_line.hash(&mut hasher);
        format!("{:08x}", hasher.finish() & 0xFFFFFFFF)
    };

    let ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let date_str = Utc::now().format("%Y-%m-%d").to_string();
    let cmd_short: String = command.lines().next().unwrap_or("").chars().take(200).collect();
    let err_short: String = error_line.chars().take(300).collect();

    let entry = serde_json::json!({
        "ts": ts,
        "role": role.as_str(),
        "severity": severity,
        "cmd": cmd_short,
        "error": err_short,
        "fingerprint": fingerprint,
        "date": date_str
    })
    .to_string();

    let log_path = state.config.log_dir.join("command-errors.log");
    append_log(&log_path, &entry).await;

    // Struggle signal (3+ errors in 60s)
    let recent_count = state.record_error(role.as_str()).await;
    let struggle_dir = std::path::PathBuf::from("/tmp/claude-team-scan");
    let _ = tokio::fs::create_dir_all(&struggle_dir).await;
    let struggle_file = struggle_dir.join(format!("{}.struggling", role.as_str()));
    if recent_count >= 3 {
        let _ = tokio::fs::write(&struggle_file, ts.as_bytes()).await;
    } else {
        let _ = tokio::fs::remove_file(&struggle_file).await;
    }

    HookResponse::allow()
}
