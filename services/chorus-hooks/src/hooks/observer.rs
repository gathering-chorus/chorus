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

    let action = tool.to_string();
    let digest = digest_tool_call(input);

    if digest.is_empty() {
        return;
    }

    let card = read_role_card(role.as_str());

    let obs = Observation {
        ts: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        role: role.as_str().to_string(),
        tool: tool.to_string(),
        action,
        digest,
        card,
    };

    write_observation(&obs).await;
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
            } else if short.contains("board-ts") {
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

/// Read the card a role is currently working on from andon state
fn read_role_card(role: &str) -> Option<String> {
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
    let stripped = path
        .strip_prefix("/Users/jeffbridwell/CascadeProjects/")
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
        }
    }

    // === digest_tool_call tests ===

    #[test]
    fn test_digest_bash_cargo_test() {
        let input = make_post_input(
            "Bash",
            json!({"command": "cargo test 2>&1"}),
            "/Users/jeffbridwell/CascadeProjects/architect",
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("running tests:"), "got: {}", d);
    }

    #[test]
    fn test_digest_bash_build() {
        let input = make_post_input(
            "Bash",
            json!({"command": "cargo build --release"}),
            "/Users/jeffbridwell/CascadeProjects/architect",
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("building:"), "got: {}", d);
    }

    #[test]
    fn test_digest_bash_board() {
        let input = make_post_input(
            "Bash",
            json!({"command": "bash ../messages/scripts/board-ts move 1594 Done"}),
            "/Users/jeffbridwell/CascadeProjects/architect",
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("board op:"), "got: {}", d);
    }

    #[test]
    fn test_digest_bash_nudge() {
        let input = make_post_input(
            "Bash",
            json!({"command": "bash ../messages/scripts/nudge.sh wren 'done with 1594'"}),
            "/Users/jeffbridwell/CascadeProjects/architect",
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("nudging:"), "got: {}", d);
    }

    #[test]
    fn test_digest_bash_generic() {
        let input = make_post_input(
            "Bash",
            json!({"command": "ls -la /tmp"}),
            "/Users/jeffbridwell/CascadeProjects/architect",
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("bash:"), "got: {}", d);
    }

    #[test]
    fn test_digest_write() {
        let input = make_post_input(
            "Write",
            json!({"file_path": "/Users/jeffbridwell/CascadeProjects/messages/services/chorus-hooks/src/hooks/observer.rs", "content": "test"}),
            "/Users/jeffbridwell/CascadeProjects/architect",
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("writing "), "got: {}", d);
        assert!(d.contains("observer.rs"), "got: {}", d);
    }

    #[test]
    fn test_digest_edit() {
        let input = make_post_input(
            "Edit",
            json!({"file_path": "/Users/jeffbridwell/CascadeProjects/architect/CLAUDE.md", "old_string": "a", "new_string": "b"}),
            "/Users/jeffbridwell/CascadeProjects/architect",
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("editing "), "got: {}", d);
    }

    #[test]
    fn test_digest_agent() {
        let input = make_post_input(
            "Agent",
            json!({"description": "Find chorus-hooks source files"}),
            "/Users/jeffbridwell/CascadeProjects/architect",
        );
        let d = digest_tool_call(&input);
        assert!(d.starts_with("agent:"), "got: {}", d);
    }

    #[test]
    fn test_digest_skill() {
        let input = make_post_input(
            "Skill",
            json!({"skill": "reboot"}),
            "/Users/jeffbridwell/CascadeProjects/architect",
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
            "/Users/jeffbridwell/CascadeProjects/architect",
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

    // === short_path ===

    #[test]
    fn test_short_path_strips_prefix() {
        let p = short_path("/Users/jeffbridwell/CascadeProjects/architect/CLAUDE.md");
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
}
