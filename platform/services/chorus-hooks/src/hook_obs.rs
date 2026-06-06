//! #3252 — Hook observability: pure helpers for the IoC interceptor.
//!
//! The dispatch wrappers (pre_tool_use / post_tool_use / user_prompt_submit /
//! stop_hook) are the single inversion-of-control seam: each wraps an `_inner`
//! returning `(module, HookResponse)`, then times it and logs uniformly. These
//! pure functions are what that seam calls — kept side-effect-free so they're
//! hermetically testable without a daemon, a socket, or the filesystem.
//!
//! The join key is `trace` (NOT a new hook-only id): `chorus-log` already
//! stamps `"trace":<id>` from CHORUS_TRACE_ID (#2897), so emitting the same
//! field on every hook.decision threads hooks AND demo/build/deploy verbs
//! (#3023/#3270) into one `grep <trace_id> chorus.log` flow.

use crate::types::HookResponse;

/// Classify a hook dispatch result into a uniform decision label.
///
/// Order matters and mirrors the historical inline logic in `pre_tool_use`:
/// a non-zero exit is a hard BLOCK; a `deny` permission JSON on stdout is a
/// DENY; a stderr message with exit 0 is an advisory WARN; otherwise allow.
pub fn classify_decision(resp: &HookResponse) -> &'static str {
    if resp.exit_code != 0 {
        "BLOCK"
    } else if resp.stdout.as_deref().map(|s| s.contains("deny")).unwrap_or(false) {
        "DENY"
    } else if resp.stderr.is_some() {
        "WARN"
    } else {
        "allow"
    }
}

/// The MUST-carry field set for a `hook.decision` event / hooks.log line.
/// Centralized so the spine emit and the hooks.log line can't drift apart,
/// and so a test can assert the contract in one place.
pub const HOOK_DECISION_FIELDS: &[&str] =
    &["timestamp", "hook", "tool", "role", "module", "decision", "trace", "latency_ms", "session_id"];

/// Render one JSON line for hooks.log (#3252 makes hooks.log JSON, queryable
/// alongside chorus.log). Every MUST-carry field is present; `reason` is the
/// only optional tail (first line of the block/warn message, empty on allow).
/// Pure: timestamp is passed in, never read from the clock here.
#[allow(clippy::too_many_arguments)]
pub fn hook_log_json(
    timestamp: &str,
    hook: &str,
    tool: &str,
    role: &str,
    module: &str,
    decision: &str,
    trace: &str,
    latency_ms: u64,
    session_id: &str,
    reason: &str,
) -> String {
    serde_json::json!({
        "timestamp": timestamp,
        "appName": "chorus-hooks",
        "component": "dispatch",
        "hook": hook,
        "tool": tool,
        "role": role,
        "module": module,
        "decision": decision,
        "trace": trace,
        "latency_ms": latency_ms,
        "session_id": session_id,
        "reason": reason,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_block_on_nonzero_exit() {
        let r = HookResponse::block_with_stderr("nope");
        assert_eq!(classify_decision(&r), "BLOCK");
    }

    #[test]
    fn classify_deny_on_permission_json() {
        let r = HookResponse::deny(r#"{"permissionDecision":"deny"}"#);
        assert_eq!(classify_decision(&r), "DENY");
    }

    #[test]
    fn classify_warn_on_stderr_with_zero_exit() {
        let r = HookResponse::warn_stderr("heads up");
        assert_eq!(classify_decision(&r), "WARN");
    }

    #[test]
    fn classify_allow_on_clean_pass() {
        assert_eq!(classify_decision(&HookResponse::allow()), "allow");
    }

    #[test]
    fn hooks_log_line_is_valid_json_with_all_must_carry_fields() {
        let line = hook_log_json(
            "2026-06-06T12:00:00.000-0400",
            "pre_tool_use", "Bash", "silas", "sparql_guard",
            "BLOCK", "demo-3252-abc", 7, "sess1234", "blocked: raw sparql",
        );
        let v: serde_json::Value = serde_json::from_str(&line).expect("hooks.log line must be valid JSON");
        for f in HOOK_DECISION_FIELDS {
            assert!(v.get(*f).is_some(), "hooks.log line missing MUST-carry field: {f}");
        }
        // trace is the join key — must be the shared id verbatim, not mangled.
        assert_eq!(v["trace"], "demo-3252-abc");
        assert_eq!(v["latency_ms"], 7);
    }

    #[test]
    fn allow_line_carries_empty_reason_but_keeps_trace() {
        let line = hook_log_json(
            "2026-06-06T12:00:00.000-0400",
            "post_tool_use", "Edit", "kade", "-", "allow", "demo-9", 1, "s", "",
        );
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["decision"], "allow");
        assert_eq!(v["trace"], "demo-9");
        assert_eq!(v["reason"], "");
    }
}
