//! #3672 — the respond-first gate (#3218) must never trap.
//!
//! 2026-07-23, twice in one day: a role owed a reply, its session had no
//! chorus_nudge_message MCP attach (the post-crash default), and the gate
//! refused every tool — including the Bash transport for the very reply it
//! demanded. 125 stop fires; a human had to free the role. These tests ARE the
//! trap: they fail on the pre-#3672 gate and pin the two invariants:
//!   1. a reply attempt is never refused (the cure always passes)
//!   2. refusal is bounded in code — past the cap the gate degrades to nagging

use chorus_hooks::nudge_gate::{
    gate_decision, is_reply_path, note_refusal, reset_refusals, GateDecision,
    PRETOOL_REFUSAL_CAP, STOP_REFUSAL_CAP,
};

// --- invariant 1: the cure always passes -----------------------------------

#[test]
fn mcp_nudge_tool_is_the_reply_path() {
    assert!(is_reply_path("mcp__chorus-api__chorus_nudge_message", ""));
}

#[test]
fn bash_transport_for_the_reply_is_the_reply_path() {
    // The documented fallback for a session with no MCP attach — the exact
    // command that was refused ~15 times on 2026-07-23 while the gate demanded it.
    let cmd = r#"~/.chorus/bin/chorus-mcp-call.sh silas chorus_nudge_message '{"to":"wren","message":"got it"}'"#;
    assert!(is_reply_path("Bash", cmd));
    let canonical = r#"bash /Users/x/CascadeProjects/chorus/platform/scripts/chorus-mcp-call.sh silas chorus_nudge_message '{"to":"wren","message":"hi"}'"#;
    assert!(is_reply_path("Bash", canonical));
}

#[test]
fn unrelated_tools_are_not_the_reply_path() {
    assert!(!is_reply_path("Bash", "ls -la"));
    assert!(!is_reply_path("Read", ""));
    assert!(!is_reply_path("PushNotification", ""));
}

#[test]
fn reply_path_is_allowed_even_mid_debt_at_any_refusal_count() {
    for n in 0..10 {
        assert_eq!(gate_decision(true, n, PRETOOL_REFUSAL_CAP), GateDecision::Allow);
    }
}

// --- invariant 2: refusal is bounded in code -------------------------------

#[test]
fn pretool_refuses_up_to_cap_then_degrades() {
    for n in 0..PRETOOL_REFUSAL_CAP {
        assert_eq!(gate_decision(false, n, PRETOOL_REFUSAL_CAP), GateDecision::Refuse);
    }
    assert_eq!(
        gate_decision(false, PRETOOL_REFUSAL_CAP, PRETOOL_REFUSAL_CAP),
        GateDecision::Degrade,
        "the call after the cap must go through — a session must regain its hands"
    );
}

#[test]
fn stop_blocks_at_most_twice_per_debt() {
    for n in 0..STOP_REFUSAL_CAP {
        assert_eq!(gate_decision(false, n, STOP_REFUSAL_CAP), GateDecision::Refuse);
    }
    assert_eq!(
        gate_decision(false, STOP_REFUSAL_CAP, STOP_REFUSAL_CAP),
        GateDecision::Degrade,
        "the 2026-07-23 loop hit 125 stop fires; the bound is {STOP_REFUSAL_CAP}"
    );
}

// --- the counter: same debt accumulates, new debt or a clear resets --------

#[test]
fn refusal_counter_accumulates_per_debt_and_resets_on_new_debt() {
    reset_refusals("testrole-a");
    assert_eq!(note_refusal("pre", "testrole-a", "debt-1"), 0);
    assert_eq!(note_refusal("pre", "testrole-a", "debt-1"), 1);
    assert_eq!(note_refusal("pre", "testrole-a", "debt-1"), 2);
    // a NEW inbound nudge is a fresh debt — fresh chances to refuse
    assert_eq!(note_refusal("pre", "testrole-a", "debt-2"), 0);
    reset_refusals("testrole-a");
}

#[test]
fn scopes_and_roles_count_independently() {
    reset_refusals("testrole-b");
    reset_refusals("testrole-c");
    assert_eq!(note_refusal("pre", "testrole-b", "d"), 0);
    assert_eq!(note_refusal("stop", "testrole-b", "d"), 0, "stop scope has its own count");
    assert_eq!(note_refusal("pre", "testrole-c", "d"), 0, "another role has its own count");
    assert_eq!(note_refusal("pre", "testrole-b", "d"), 1);
    reset_refusals("testrole-b");
    reset_refusals("testrole-c");
}

#[test]
fn clearing_the_debt_resets_the_counter() {
    reset_refusals("testrole-d");
    note_refusal("pre", "testrole-d", "d1");
    note_refusal("pre", "testrole-d", "d1");
    reset_refusals("testrole-d"); // debt cleared (owes_response returned None)
    assert_eq!(
        note_refusal("pre", "testrole-d", "d1"),
        0,
        "after a clear, a re-trap starts counting from zero"
    );
    reset_refusals("testrole-d");
}

// --- end-to-end shape of the trap ------------------------------------------

#[test]
fn trapped_session_with_no_mcp_attach_regains_tools_within_the_bound() {
    // The 2026-07-23 silas trap, replayed against the decision layer: debt held,
    // every reply transport broken (is_reply=false for every call the session
    // can make). The session must regain tool access within PRETOOL_REFUSAL_CAP
    // refusals — not after 125 human-rescued fires.
    reset_refusals("testrole-e");
    let mut allowed_at = None;
    for attempt in 0..20 {
        let n = note_refusal("pre", "testrole-e", "wren-nudge-22909");
        match gate_decision(false, n, PRETOOL_REFUSAL_CAP) {
            GateDecision::Refuse => continue,
            GateDecision::Degrade | GateDecision::Allow => {
                allowed_at = Some(attempt);
                break;
            }
        }
    }
    assert_eq!(
        allowed_at,
        Some(PRETOOL_REFUSAL_CAP),
        "session must regain its hands exactly after the cap"
    );
    reset_refusals("testrole-e");
}
