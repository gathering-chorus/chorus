//! #3191 — per-prompt context-inject must INJECT into the model.
//!
//! For UserPromptSubmit the harness reads stdout `hookSpecificOutput.additionalContext`;
//! stderr (exit 0) is shown to the user but never reaches the model. Before this fix the
//! assembled block was returned on stderr, so the model received none of it. These tests
//! pin the routing: context block → stdout additionalContext; warnings → stderr.

use chorus_hooks::{build_user_prompt_response, HookResponse};

// AC1 — the context block injects via stdout additionalContext, NOT stderr.
#[test]
fn context_block_injects_via_stdout_not_stderr() {
    let r: HookResponse =
        build_user_prompt_response(Some("<chorus-context>hi</chorus-context>"), None, 0, &[]);
    let so = r.stdout.expect("context block must be on stdout");
    let v: serde_json::Value = serde_json::from_str(&so).expect("stdout must be valid JSON");
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
    assert!(v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .contains("<chorus-context>"));
    assert!(r.stderr.is_none(), "context block must NOT be on stderr");
}

// AC2 — warnings go to stderr; the block never leaks onto stderr.
#[test]
fn warnings_go_to_stderr_and_block_stays_off_it() {
    let r = build_user_prompt_response(
        Some("<chorus-context>BLOCKMARKER</chorus-context>"),
        None,
        0,
        &[Some("clock skew warning"), None, Some("pattern: ideation")],
    );
    let se = r.stderr.expect("warnings belong on stderr");
    assert!(se.contains("clock skew warning"));
    assert!(se.contains("pattern: ideation"));
    assert!(
        !se.contains("BLOCKMARKER"),
        "context block must never appear on stderr"
    );
}

// AC3 — a guard permission-decision on stdout is preserved, not clobbered.
#[test]
fn guard_decision_on_stdout_not_clobbered() {
    let r = build_user_prompt_response(Some("ctx"), Some(r#"{"decision":"block"}"#), 2, &[]);
    assert_eq!(r.stdout.as_deref(), Some(r#"{"decision":"block"}"#));
    assert_eq!(r.exit_code, 2);
}

// Nothing to inject and no warnings → empty response.
#[test]
fn empty_when_nothing_to_say() {
    let r = build_user_prompt_response(None, None, 0, &[None, None]);
    assert!(r.stdout.is_none());
    assert!(r.stderr.is_none());
}
