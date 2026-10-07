//! #4444 — Abby's Gemini policy carried none of the 21 deny rules the Claude
//! roles run under and allowed every shell command (Wren measured 10-07 08:06).
//! The deny rules now come from the same settings file as the allow rules and
//! sit above them, so `kill`, `rm -rf` and the rest are refused for her too.
use chorus_agent::allowed_tools::{gemini_denied, gemini_policy_toml};
use serde_json::json;
use std::path::Path;

fn settings() -> serde_json::Value {
    json!({ "permissions": {
        "allow": ["Bash(*)"],
        "deny": ["Bash(rm -rf *)", "Bash(kill *)", "Bash(git push * --force* origin main*)", "Read(/secret/**)"]
    } })
}

#[test]
fn each_claude_bash_deny_rule_becomes_a_command_regex() {
    let (denied, unmapped) = gemini_denied(&settings());
    assert_eq!(denied, vec!["rm -rf .*", "kill .*", "git push .* --force.* origin main.*"]);
    assert_eq!(unmapped, vec!["Read(/secret/**)"]);
}

#[test]
fn regex_characters_in_a_rule_are_matched_literally() {
    let (denied, _) = gemini_denied(&json!({"permissions":{"deny":["Bash(a.b+c (d)*)"]}}));
    assert_eq!(denied, vec![r"a\.b\+c \(d\).*"]);
}

#[test]
fn the_policy_denies_above_the_allow_rules() {
    let (denied, _) = gemini_denied(&settings());
    let toml = gemini_policy_toml(&["run_shell_command".to_string()], &denied, Path::new("/s.json"));
    let deny = toml.split("[[rule]]").find(|r| r.contains("kill .*")).expect("a kill rule");
    assert!(deny.contains("decision = \"deny\""), "{deny}");
    assert!(deny.contains("commandRegex = \"kill .*\""), "{deny}");
    assert!(deny.contains("priority = 200"), "{deny}");
    let allow = toml.split("[[rule]]").find(|r| r.contains("allowRedirection")).unwrap();
    assert!(allow.contains("priority = 100"), "{allow}");
}

#[test]
fn negative_proof_settings_without_deny_rules_add_no_deny_rule() {
    let (denied, unmapped) = gemini_denied(&json!({"permissions":{"allow":["Bash(*)"]}}));
    assert!(denied.is_empty() && unmapped.is_empty());
    let toml = gemini_policy_toml(&["run_shell_command".to_string()], &denied, Path::new("/s.json"));
    assert!(!toml.contains("deny"), "{toml}");
}

/// #4444 — Jeff wants Abby's MCP tools to match Wren's and Kade's; her role
/// settings filtered chorus-api down to 1 of 54 tools. The door and her
/// policy decide what she may do, so the settings must not filter.
fn includes_every_tool(settings: &serde_json::Value) -> bool {
    settings.pointer("/mcpServers/chorus-api").is_some_and(|s| s.get("includeTools").is_none())
}

#[test]
fn abbys_settings_give_her_every_chorus_api_tool() {
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../..");
    let text = std::fs::read_to_string(root.join("roles/abby-normal/.gemini/settings.json")).unwrap();
    assert!(includes_every_tool(&serde_json::from_str(&text).unwrap()), "{text}");
}

#[test]
fn negative_proof_a_tool_filter_is_caught() {
    assert!(!includes_every_tool(&json!({"mcpServers":{"chorus-api":{"includeTools":["chorus_nudge_message"]}}})));
    assert!(!includes_every_tool(&json!({"mcpServers":{}})));
}
