//! #4432 — Abby's Gemini allowed tools come from the Claude roles' allow
//! rules (Jeff 2026-10-06: approving her tool calls "is a big blocker").
use chorus_agent::allowed_tools::{gemini_allowed, install_gemini_allowed};
use serde_json::{json, Value};
use std::fs;

/// The 26 rules in ~/.claude/settings.json on 2026-10-06, verbatim.
fn claude_rules() -> Value {
    json!({ "permissions": { "allow": [
        "Read", "Write", "Edit", "Glob", "Grep", "WebFetch(*)", "WebSearch", "Skill(*)", "Task(*)", "NotebookEdit(*)", "Bash(*)",
        "Bash(chorus-deploy chorus-api*)", "Bash(/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-deploy chorus-api*)",
        "Bash(deploy-daemon-card.sh*)", "Bash(/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/deploy-daemon-card.sh*)",
        "mcp__ast-grep__*", "mcp__cclsp__*", "mcp__chorus-api__*", "mcp__claude_ai_Gmail__*", "mcp__claude_ai_Google_Calendar__*",
        "mcp__claude_ai_Google_Drive__*", "mcp__ide__*", "Bash(git push --force-with-lease origin *)", "Bash(git push --force-with-lease *)",
        "Bash(launchctl kickstart -k gui/501/com.chorus.:*)", "Bash(launchctl kickstart gui/501/com.chorus.:*)"
    ] } })
}

#[test]
fn the_claude_rules_become_gemini_tool_names() {
    let (allowed, unmapped) = gemini_allowed(&claude_rules()).unwrap();
    for name in ["read_file", "read_many_files", "list_directory", "write_file", "replace", "glob", "grep_search",
                 "web_fetch", "google_web_search", "run_shell_command", "mcp_chorus-api_*", "mcp_ast-grep_*"] {
        assert!(allowed.contains(&name.to_string()), "{name} missing from {allowed:?}");
    }
    assert!(allowed.contains(&"run_shell_command(git push --force-with-lease origin)".to_string()), "{allowed:?}");
    assert_eq!(unmapped, vec!["NotebookEdit(*)".to_string()]);
}

#[test]
fn negative_proof_a_rule_with_no_gemini_tool_is_reported_never_guessed() {
    let (allowed, unmapped) = gemini_allowed(&json!({"permissions":{"allow":["Read","NotebookEdit(*)"]}})).unwrap();
    assert_eq!(allowed, vec!["read_file", "read_many_files", "list_directory"]);
    assert_eq!(unmapped, vec!["NotebookEdit(*)"]);
}

#[test]
fn negative_proof_no_allow_list_or_nothing_mappable_is_an_error() {
    assert!(gemini_allowed(&json!({"permissions":{}})).unwrap_err().contains("permissions.allow"));
    assert!(gemini_allowed(&json!({"permissions":{"allow":["NotebookEdit(*)"]}})).unwrap_err().contains("no Gemini tool"));
}

#[test]
fn install_writes_a_gemini_policy_file_with_allow_rules() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("claude-settings.json");
    fs::write(&source, claude_rules().to_string()).unwrap();
    let home = dir.path().join("home");
    install_gemini_allowed(&source, &home).unwrap();
    let toml = fs::read_to_string(home.join(".gemini/policies/chorus-allowed.toml")).unwrap();
    assert!(toml.contains("\"run_shell_command\""), "{toml}");
    assert!(toml.contains("\"mcp_chorus-api_*\""), "{toml}");
    assert!(toml.contains("commandPrefix = \"git push --force-with-lease origin\""), "{toml}");
    assert_eq!(toml.matches("decision = \"allow\"").count(), 1 + 8, "one plain rule + 8 shell prefixes:\n{toml}");
    assert!(!toml.contains("NotebookEdit") && !toml.contains("deny"), "{toml}");
}

#[test]
fn negative_proof_an_unreadable_source_refuses_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let err = install_gemini_allowed(&dir.path().join("absent.json"), &home).unwrap_err();
    assert!(err.contains("cannot read the allow rules"), "{err}");
    assert!(!home.join(".gemini/policies/chorus-allowed.toml").exists());
}

#[test]
fn negative_proof_a_gemini_session_with_no_allow_rules_source_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let err = chorus_agent::server::gemini_allowed_installed_in(None, home.path()).unwrap_err();
    assert!(err.contains("CHORUS_ALLOW_RULES_FILE is unset"), "{err}");
    assert!(!home.path().join(".gemini/policies/chorus-allowed.toml").exists());
}

/// Writes the policy generated from the live Claude settings into OUT_HOME
/// (run by hand with --ignored; used for the 10-06 live proof as Abby).
#[test]
#[ignore]
fn write_the_live_policy() {
    let source = std::env::var("CHORUS_ALLOW_RULES_FILE").unwrap();
    let home = std::env::var("OUT_HOME").unwrap();
    let (allowed, unmapped) = install_gemini_allowed(std::path::Path::new(&source), std::path::Path::new(&home)).unwrap();
    println!("ALLOWED={} UNMAPPED={unmapped:?}", allowed.len());
}
