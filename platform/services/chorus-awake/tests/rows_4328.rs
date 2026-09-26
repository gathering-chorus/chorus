//! #4328 — the rows a login writes and the edits that keep them true.
//! Jeff 2026-09-26 08:29: "is ur session better now?" Each rule is also shown
//! refusing the state it exists to catch (#3734).

use chorus_awake::rows::*;
use serde_json::json;

#[test]
fn a_run_names_its_session_and_the_run_it_replaced() {
    let r = run_row("silas", "silas-run-a", "session-silas-x", "conv-1", "2026-09-26T09:00:00Z", Some("sessionrun-silas-run-0"));
    assert_eq!(r["runOf"], "session-silas-x");
    assert_eq!(r["previousRun"], "sessionrun-silas-run-0");
    assert_eq!(r["ownedBy"], "principal-silas");
    assert_eq!(r["conversationId"], "conv-1");
    assert!(r.get("runEndedAt").is_none(), "a new run is live");
}

#[test]
fn a_first_run_carries_no_previous_run() {
    assert!(run_row("silas", "n", "s", "c", "t", None).get("previousRun").is_none());
    assert!(run_row("silas", "n", "s", "c", "t", Some("")).get("previousRun").is_none());
}

#[test]
fn a_new_presence_is_unknown_not_reachable() {
    let p = presence_row("kade", "kade-presence-a", "run-a", "%3", "/dev/ttys003", "jeffbridwell");
    assert_eq!(p["reachability"], "unknown");
    assert!(p.get("lastDeliveredAt").is_none());
    assert_eq!(p["presenceOf"], "run-a");
}

#[test]
fn only_a_delivery_makes_a_presence_reachable() {
    let p = presence_row("kade", "n", "run-a", "%3", "", "u");
    let d = delivered_presence(p.clone(), "2026-09-26T09:05:00Z").unwrap();
    assert_eq!(d["reachability"], "reachable");
    assert_eq!(d["lastDeliveredAt"], "2026-09-26T09:05:00Z");
    // negative proof: an ordinary turn (no delivery) never marks it reachable
    let (_, delivered) = turn_facts(r#"{"session_id":"c","prompt":"work status"}"#);
    assert!(!delivered);
    let (_, delivered) = turn_facts(r#"{"session_id":"c","prompt":"[nudge from wren | 2026-09-26 09:00 Boston] hi"}"#);
    assert!(delivered);
}

#[test]
fn the_boot_context_points_at_its_run() {
    let c = boot_context_row("wren", "wren-boot-a", "run-a", "t");
    assert_eq!(c["contextOf"], "run-a");
    assert_eq!(c["contextKind"], "boot");
}

#[test]
fn the_session_says_its_role_and_start() {
    let s = with_role_and_start(json!({"name":"session-silas-x","tokenId":"j"}), "silas", "2026-09-26T09:00:00Z");
    assert_eq!(s["actsAs"], "role-silas");
    assert_eq!(s["startedAt"], "2026-09-26T09:00:00Z");
    assert_eq!(s["tokenId"], "j", "the rest of the row is kept for the whole-row PUT");
}

#[test]
fn an_ended_run_says_why_and_a_bad_reason_is_refused() {
    let r = run_row("silas", "n", "s", "c", "t", None);
    let e = ended_run(r.clone(), "2026-09-26T10:00:00Z", "restart").unwrap();
    assert_eq!(e["endReason"], "restart");
    assert_eq!(e["runEndedAt"], "2026-09-26T10:00:00Z");
    // negative proof: a word the shape refuses never becomes a row
    assert!(ended_run(r, "t", "stopped").is_none());
}

#[test]
fn seen_updates_the_same_row() {
    let s = json!({"name":"session-silas-x","lastSeenAt":"2026-09-26T09:00:00Z"});
    let s2 = seen_session(s, "2026-09-26T09:07:00Z").unwrap();
    assert_eq!(s2["name"], "session-silas-x");
    assert_eq!(s2["lastSeenAt"], "2026-09-26T09:07:00Z");
    assert!(seen_session(json!({}), "t").is_none(), "no saved row, no write");
}

#[test]
fn seen_is_throttled_but_a_delivery_always_writes() {
    assert!(seen_due(None, 1000, 60, false));
    assert!(!seen_due(Some(990), 1000, 60, false));
    assert!(seen_due(Some(900), 1000, 60, false));
    assert!(seen_due(Some(990), 1000, 60, true));
}

#[test]
fn the_real_conversation_id_replaces_pending_once() {
    let r = run_row("silas", "n", "s", "pending", "t", None);
    let r2 = with_conversation(r, "4d39d28c").unwrap();
    assert_eq!(r2["conversationId"], "4d39d28c");
    assert!(with_conversation(r2.clone(), "4d39d28c").is_none(), "same id, no write");
    assert!(with_conversation(r2, "").is_none(), "no id, no write");
}

#[test]
fn the_sweep_closes_only_dead_open_sessions() {
    let listing = json!({"data":[
        {"name":"dead","sessionState":"open","expiresAt":"2026-09-20T00:10:00Z","tokenId":"j","ownedBy":"principal-kade"},
        {"name":"live-login","sessionState":"open","expiresAt":"2026-09-26T08:10:00Z","tokenId":"j","ownedBy":"principal-silas"},
        {"name":"recent-turn","sessionState":"open","expiresAt":"2026-09-26T08:10:00Z","lastSeenAt":"2026-09-26T08:55:00Z","tokenId":"j","ownedBy":"principal-wren"},
        {"name":"closed","sessionState":"closed","expiresAt":"2026-09-20T00:10:00Z","tokenId":"j","ownedBy":"principal-kade"},
        {"name":"not-expired","sessionState":"open","expiresAt":"2026-09-26T09:30:00Z","tokenId":"j","ownedBy":"principal-kade"}
    ]}).to_string();
    let names: Vec<String> = expired_open(&listing, "2026-09-26T09:00:00Z", &["live-login".to_string()]).iter().map(|r| r["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, vec!["dead"]);
}
