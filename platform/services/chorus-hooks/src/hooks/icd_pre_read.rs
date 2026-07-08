//! ICD Pre-Read Hook — block data card builds without reading domain context (#1684)
//!
//! PreToolUse hook on Write/Edit. If the file being written touches a data domain
//! (photos, music, documents, people, social), verify that the domain-context file
//! was read earlier in the session.
//!
//! Detection: role-state.sh sets card= and the board has domain: tags.
//! We check a simpler signal: /tmp/icd-read-{role}-{domain} flag files.
//! The session-start hook creates these when domain-context files are read.

use crate::state::AppState;
use crate::types::{HookInput, Role};
use std::path::Path;
use tracing::{info, warn};

/// Data domains that require ICD pre-read
const DATA_DOMAINS: &[&str] = &["photos", "music", "documents", "people", "social"];

/// Map file paths to data domains
fn detect_domain(file_path: &str) -> Option<&'static str> {
    for domain in DATA_DOMAINS {
        // Check if the file path contains domain-specific patterns
        if file_path.contains(&format!("domain:{}", domain))
            || file_path.contains(&format!("/{}/", domain))
            || file_path.contains(&format!("-{}", domain))
            || file_path.contains(&format!("{}-harvester", domain))
            || file_path.contains(&format!("{}-to-rdf", domain))
        {
            return Some(domain);
        }
    }

    // Specific file patterns
    if file_path.contains("photo") || file_path.contains("Photo") || file_path.contains("iphone")
        || file_path.contains("takeout") || file_path.contains("apple-photos")
    {
        return Some("photos");
    }
    if file_path.contains("music") || file_path.contains("Music") || file_path.contains("navidrome") {
        return Some("music");
    }

    None
}

/// Check if the domain context was read this session
fn domain_context_read(role: &Role, domain: &str) -> bool {
    let flag = format!("/tmp/icd-read-{}-{}", role.as_str(), domain);
    Path::new(&flag).exists()
}

/// Set the flag when domain context is read
pub fn mark_domain_read(role: &Role, domain: &str) {
    let flag = format!("/tmp/icd-read-{}-{}", role.as_str(), domain);
    let _ = std::fs::write(&flag, domain);
}

/// PreToolUse check — block writes to data domain files without ICD pre-read
pub async fn check(input: &HookInput, _state: &AppState) {
    let tool = input.tool_name.as_deref().unwrap_or("");

    // Only check on Write and Edit
    if tool != "Write" && tool != "Edit" {
        // But also detect Read of domain-context files — set the flag
        if tool == "Read" {
            let file_path = input.get_tool_input_str("file_path");
            if file_path.contains("domain-context-") {
                // Extract domain name from path: domain-context-{domain}.md
                if let Some(domain) = file_path
                    .rsplit("domain-context-")
                    .next()
                    .and_then(|s| s.strip_suffix(".md"))
                    .or_else(|| file_path.rsplit("domain-context-").next())
                {
                    let domain = domain.trim_end_matches(".md");
                    let role = input
                        .deploy_role
                        .as_deref()
                        .map(|r| match r {
                            "wren" => Role::Wren,
                            "silas" => Role::Silas,
                            "kade" => Role::Kade,
                            _ => Role::Unknown,
                        })
                        .unwrap_or_else(|| {
                            Role::from_cwd(input.cwd.as_deref().unwrap_or(""))
                        });
                    mark_domain_read(&role, domain);
                    info!(domain = domain, role = %role.as_str(), "Domain context read — ICD gate satisfied");
                }
            }
        }
        return;
    }

    let file_path = input.get_tool_input_str("file_path");
    if file_path.is_empty() {
        return;
    }

    // Detect domain from file path
    let domain = match detect_domain(&file_path) {
        Some(d) => d,
        None => return, // Not a data domain file — no gate
    };

    // Get role
    let role = input
        .deploy_role
        .as_deref()
        .map(|r| match r {
            "wren" => Role::Wren,
            "silas" => Role::Silas,
            "kade" => Role::Kade,
            _ => Role::Unknown,
        })
        .unwrap_or_else(|| {
            Role::from_cwd(input.cwd.as_deref().unwrap_or(""))
        });

    // Check if domain context was read
    if !domain_context_read(&role, domain) {
        let context_path = format!(
            "{}/designing/domain-context/domain-context-{}.md",
            crate::shared::state_paths::chorus_root(), domain
        );
        warn!(
            domain = domain,
            role = %role.as_str(),
            file = %file_path,
            "ICD pre-read gate: domain context not read"
        );
        // Print blocking message to stderr (Claude Code shows this)
        eprintln!(
            "⚠ ICD pre-read: read domain context before building on {} data.\n  Read: {}\n  This prevents field mapping errors (e.g., #1684 — 40% data loss from skipped ICD).",
            domain, context_path
        );
        // Don't block — warn only for now. Blocking would need "BLOCKED:" prefix.
        // TODO: Switch to blocking after team validates the detection is accurate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::types::HookInput;
    use crate::shared::state_paths::chorus_root;
    use serde_json::json;

    #[tokio::test]
    async fn does_not_panic_on_read() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(json!({"file_path": "/tmp/test.md"})),
            tool_response: None, session_id: Some("t".into()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None, stop_hook_active: None, hook_type: None,
            deploy_role: Some("silas".into()),
            trace_id: None, tool_output_is_error: None,};
        check(&input, &state).await;
    }

    #[tokio::test]
    async fn does_not_panic_on_write() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: Some("Write".to_string()),
            tool_input: Some(json!({"file_path": "/tmp/test.ts", "content": "test"})),
            tool_response: None, session_id: Some("t".into()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None, stop_hook_active: None, hook_type: None,
            deploy_role: Some("silas".into()),
            trace_id: None, tool_output_is_error: None,};
        check(&input, &state).await;
    }
}
