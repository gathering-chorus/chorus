// @test-type: unit
// @domain: services
//! #4512 AC2 — the caller is a verified identity token; anything else is
//! refused 401 with its reason named, before any tool code runs.
//! Driven through the real router (gate + rmcp service), no network: the
//! verifier's keys and principals are stubs, the tokens are real ES256.

mod common;
use common::*;
use http::StatusCode;

#[tokio::test]
async fn no_token_is_refused_and_says_so() {
    let (status, body) = send(initialize("2025-11-25", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(body.contains("no bearer token"), "{body}");
}

#[tokio::test]
async fn a_token_signed_under_an_unknown_kid_is_refused() {
    let (status, body) = send(initialize("2025-11-25", Some(&mint("not-our-key", now() + 600)))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    // #4183 reopen 2 — an unknown kid is its own reason, never "forged"
    assert!(body.contains("signing key unknown"), "{body}");
    assert!(!body.contains("signature does not verify"), "{body}");
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let (status, body) = send(initialize("2025-11-25", Some(&mint(KID, now() - 60)))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(body.contains("expired"), "{body}");
}

#[tokio::test]
async fn a_tampered_signature_is_refused() {
    // Flip a char well inside the signature: the last base64 char carries
    // padding bits, so changing it can leave the decoded bytes the same.
    let mut t: Vec<char> = mint(KID, now() + 600).chars().collect();
    let i = t.len() - 20;
    t[i] = if t[i] == 'A' { 'B' } else { 'A' };
    let t: String = t.into_iter().collect();
    let (status, body) = send(initialize("2025-11-25", Some(&t))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(body.contains("signature does not verify"), "{body}");
}

/// NEGATIVE PROOF of the gate: the same request with a valid token is NOT
/// refused. A gate that refused everything would pass the four tests above.
#[tokio::test]
async fn a_valid_token_passes_the_gate() {
    let (status, body) = send(initialize("2025-11-25", Some(&mint(KID, now() + 600)))).await;
    assert_ne!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(status, StatusCode::OK, "{body}");
}
