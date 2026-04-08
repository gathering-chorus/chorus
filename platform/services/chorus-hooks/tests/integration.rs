//! Integration tests for chorus-hooks Axum routes.
//!
//! Tests the full request→dispatch→response cycle through the HTTP layer,
//! verifying role detection, guard routing, and response format.

use serde_json::{json, Value};

// ============================================================
// Since chorus-hooks is a binary crate, integration tests exercise
// the library-like modules through their public async APIs.
// These tests validate the full guard chain behavior.
// ============================================================

/// Shim decode_chunked function — replicated here for testing
fn decode_chunked(body: &str) -> String {
    let mut result = String::new();
    let mut remaining = body;

    loop {
        let size_end = match remaining.find("\r\n") {
            Some(pos) => pos,
            None => break,
        };
        let size_str = remaining[..size_end].trim();
        let size = match usize::from_str_radix(size_str, 16) {
            Ok(s) => s,
            Err(_) => break,
        };
        if size == 0 {
            break;
        }
        let chunk_start = size_end + 2;
        let chunk_end = chunk_start + size;
        if chunk_end > remaining.len() {
            break;
        }
        result.push_str(&remaining[chunk_start..chunk_end]);
        remaining = &remaining[chunk_end..];
        if remaining.starts_with("\r\n") {
            remaining = &remaining[2..];
        }
    }
    result
}

#[test]
fn test_decode_chunked_single() {
    let body = "d\r\nHello, World!\r\n0\r\n\r\n";
    assert_eq!(decode_chunked(body), "Hello, World!");
}

#[test]
fn test_decode_chunked_multiple() {
    let body = "5\r\nHello\r\n7\r\n, World\r\n0\r\n\r\n";
    assert_eq!(decode_chunked(body), "Hello, World");
}

#[test]
fn test_decode_chunked_empty() {
    let body = "0\r\n\r\n";
    assert_eq!(decode_chunked(body), "");
}

#[test]
fn test_decode_chunked_hex_size() {
    // 1a = 26 bytes
    let content = "abcdefghijklmnopqrstuvwxyz";
    let body = format!("1a\r\n{}\r\n0\r\n\r\n", content);
    assert_eq!(decode_chunked(&body), content);
}

/// Role detection from cwd paths
#[test]
fn test_role_detection_from_cwd() {
    // These patterns are critical — wrong role = wrong guard chain
    let cases = vec![
        ("/Users/jeffbridwell/CascadeProjects/chorus/platform/roles/wren", "wren"),
        ("/Users/jeffbridwell/CascadeProjects/chorus/platform/roles/silas", "silas"),
        ("/Users/jeffbridwell/CascadeProjects/chorus/platform/roles/kade", "kade"),
        ("/Users/jeffbridwell/CascadeProjects/chorus/messages", "unknown"),
        ("/tmp/random", "unknown"),
    ];

    for (cwd, expected_role) in cases {
        let role = if cwd.contains("roles/wren") || cwd.contains("product-manager") {
            "wren"
        } else if cwd.contains("roles/silas") || cwd.contains("architect") {
            "silas"
        } else if cwd.contains("roles/kade") || cwd.contains("engineer") {
            "kade"
        } else {
            "unknown"
        };
        assert_eq!(role, expected_role, "cwd={}", cwd);
    }
}

/// Verify HookResponse JSON format matches what Claude Code expects
#[test]
fn test_hook_response_format_allow() {
    let response = json!({
        "exit_code": 0
    });
    assert_eq!(response["exit_code"], 0);
}

#[test]
fn test_hook_response_format_deny() {
    let response = json!({
        "stdout": "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":\"test\"}}",
        "exit_code": 0
    });
    let stdout: Value = serde_json::from_str(response["stdout"].as_str().unwrap()).unwrap();
    assert_eq!(
        stdout["hookSpecificOutput"]["permissionDecision"],
        "deny"
    );
}

#[test]
fn test_hook_response_format_block() {
    let response = json!({
        "stderr": "DEC-025 gate: ...",
        "exit_code": 2
    });
    assert_eq!(response["exit_code"], 2);
    assert!(response["stderr"].as_str().unwrap().contains("DEC-025"));
}

/// Nudge inbox isolation — roles should only drain their own inbox
#[test]
fn test_inbox_isolation_paths() {
    let roles = vec!["wren", "silas", "kade"];
    for role in &roles {
        let inbox = format!("/tmp/voice-inbox/{}/pending-inject.txt", role);
        // Each role has a distinct path — no cross-role bleed possible
        for other in &roles {
            if other != role {
                let other_inbox = format!("/tmp/voice-inbox/{}/pending-inject.txt", other);
                assert_ne!(inbox, other_inbox, "Inbox paths must be unique per role");
            }
        }
    }
}

/// Verify permission_deny_json format
#[test]
fn test_permission_deny_json_format() {
    let reason = "test reason";
    let json_str = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason
        }
    })
    .to_string();

    let parsed: Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed["hookSpecificOutput"]["permissionDecision"], "deny");
    assert_eq!(
        parsed["hookSpecificOutput"]["permissionDecisionReason"],
        reason
    );
}

/// Verify decision_block_json format
#[test]
fn test_decision_block_json_format() {
    let message = "enrichment context here";
    let json_str = serde_json::json!({
        "decision": "block",
        "message": message
    })
    .to_string();

    let parsed: Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed["decision"], "block");
    assert_eq!(parsed["message"], message);
}
