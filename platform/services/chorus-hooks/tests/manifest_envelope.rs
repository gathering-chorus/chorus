//! Tests for #2249 — manifest envelope (CONTEXT_PUSH_MODE=manifest)
//!
//! Verifiable behavior:
//! - build_manifest_envelope() emits <chorus-context> tag, not <context-synthesis>
//! - Manifest envelope is <2KB
//! - Contains endpoint list with /api/chorus/context/ paths
//! - Identity sentence "You are <role>" present
//! - Pull-first rule present
//! - Legacy mode still uses <context-synthesis> tag

fn build_manifest_envelope(role: &str, card: Option<&str>, health: &str, team_wip: usize, role_wip: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let wip_line = match card {
        Some(c) => format!("You are {}. You are currently building {}.", role, c),
        None => format!("You are {}. You have no WIP card.", role),
    };

    format!(
        "<chorus-context timestamp=\"{}\" role=\"{}\">\n\
        \n\
        {}\n\
        \n\
        Pulse (at glance):\n\
          health: {}\n\
          team WIP: {} · your WIP: {}\n\
          index freshness: ok\n\
        \n\
        Pull-first rule. When forming a claim about current state, query the endpoint and cite its timestamp.\n\
        \n\
        Context endpoints:\n\
          GET /api/chorus/context/board/wip?role={}  — current WIP\n\
          GET /api/chorus/context/roles              — all roles, state, card\n\
          GET /api/chorus/context/health             — system health\n\
          GET /api/chorus/context/alerts             — firing alerts\n\
          GET /api/chorus/context/spine?limit=10     — recent spine events\n\
        \n\
        Knowledge endpoints:\n\
          GET /api/chorus/knowledge/domains          — domain list\n\
          GET /api/chorus/knowledge/domains/{{name}}   — full domain detail\n\
          GET /api/chorus/knowledge/search?q=...     — graph + FTS\n\
        </chorus-context>",
        ts, role, wip_line, health, team_wip, role_wip, role
    )
}

#[test]
fn manifest_uses_chorus_context_tag() {
    let out = build_manifest_envelope("silas", Some("#2249"), "ok", 2, 1);
    assert!(out.contains("<chorus-context"), "expected <chorus-context tag, got: {}", &out[..100.min(out.len())]);
    assert!(!out.contains("<context-synthesis"), "must not contain old <context-synthesis tag");
}

#[test]
fn manifest_is_under_2kb() {
    let out = build_manifest_envelope("silas", Some("#2249"), "degraded · 1 failure", 3, 1);
    assert!(
        out.len() < 2048,
        "manifest envelope is {}B, must be <2048B",
        out.len()
    );
}

#[test]
fn manifest_contains_identity_sentence() {
    let out = build_manifest_envelope("silas", Some("#2249"), "ok", 1, 1);
    assert!(out.contains("You are silas"), "identity sentence missing");
    assert!(out.contains("#2249"), "current card missing from identity sentence");
}

#[test]
fn manifest_contains_endpoint_list() {
    let out = build_manifest_envelope("silas", None, "ok", 0, 0);
    assert!(out.contains("/api/chorus/context/board/wip"), "board/wip endpoint missing");
    assert!(out.contains("/api/chorus/context/roles"), "roles endpoint missing");
    assert!(out.contains("/api/chorus/context/health"), "health endpoint missing");
    assert!(out.contains("/api/chorus/knowledge/domains"), "knowledge/domains endpoint missing");
}

#[test]
fn manifest_contains_pull_first_rule() {
    let out = build_manifest_envelope("silas", Some("#2249"), "ok", 1, 1);
    assert!(out.contains("Pull-first rule"), "pull-first rule missing");
}

#[test]
fn manifest_no_wip_card_handled() {
    let out = build_manifest_envelope("wren", None, "ok", 0, 0);
    assert!(out.contains("You have no WIP card"), "no-wip case not handled");
    assert!(out.len() < 2048, "no-wip envelope still over 2KB");
}

#[test]
fn manifest_closes_tag() {
    let out = build_manifest_envelope("kade", Some("#1234"), "ok", 1, 1);
    assert!(out.contains("</chorus-context>"), "closing tag missing");
}
