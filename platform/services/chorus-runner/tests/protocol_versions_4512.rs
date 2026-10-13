// @test-type: unit
// @domain: services
//! #4512 AC1 — the runner answers the 2026-07-28 release that Claude Code
//! tries first (the old server 400s it: "Unsupported protocol version",
//! measured 2026-10-12 19:49), and still answers 2025-11-25 clients.

mod common;
use common::*;
use http::StatusCode;

#[tokio::test]
async fn a_2025_11_25_client_initializes() {
    let (status, body) = send(initialize("2025-11-25", Some(&good_token()))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("\"protocolVersion\":\"2025-11-25\""), "{body}");
}

#[tokio::test]
async fn a_2026_07_28_client_initializes_or_is_served_without_one() {
    let (status, body) = send(initialize("2026-07-28", Some(&good_token()))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("Unsupported protocol version"), "{body}");
}

#[tokio::test]
async fn a_2026_07_28_client_lists_tools_with_no_session() {
    let (status, body) = send(rpc("tools/list", r#"{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"t","version":"0"},"io.modelcontextprotocol/clientCapabilities":{}}}"#, "2026-07-28", &good_token())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("door_list") && body.contains("door_get"), "{body}");
}

#[tokio::test]
async fn tools_list_carries_the_doors_collections() {
    let (status, body) = send(rpc("tools/list", r#"{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"t","version":"0"},"io.modelcontextprotocol/clientCapabilities":{}}}"#, "2026-07-28", &good_token())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("/cards/cards"), "the enum comes from the served list: {body}");
}
