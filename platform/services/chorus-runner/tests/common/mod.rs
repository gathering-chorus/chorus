// @test-type: unit
// @domain: services
//! Shared by the #4512 tests: a stub verifier with real ES256 tokens, and the
//! real router driven in-process.
#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use chorus_oidc::oidc::OidcVerifier;
use chorus_runner::server::router;
use http::{Request, StatusCode};
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use tower::ServiceExt;

pub const ISSUER: &str = "http://localhost:3001/";
pub const KID: &str = "css-runner-test-1";

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

pub fn b64url(data: &[u8]) -> String {
    pub const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for i in 0..=chunk.len() {
            out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

pub fn key() -> SigningKey {
    SigningKey::from_slice(&[11u8; 32]).unwrap()
}
pub fn webid() -> String {
    "http://localhost:3000/pods/chorus/_agents/wren/profile/card.ttl#me".into()
}

pub fn jwks() -> String {
    let p = key().verifying_key().to_encoded_point(false);
    format!(
        r#"{{"keys":[{{"kty":"EC","crv":"P-256","alg":"ES256","kid":"{KID}","x":"{}","y":"{}"}}]}}"#,
        b64url(p.x().unwrap()),
        b64url(p.y().unwrap())
    )
}

pub fn mint(kid: &str, exp: u64) -> String {
    let h = b64url(format!(r#"{{"alg":"ES256","typ":"JWT","kid":"{kid}"}}"#).as_bytes());
    let p = b64url(format!(r#"{{"iss":"{ISSUER}","aud":"chorus","webid":"{}","exp":{exp}}}"#, webid()).as_bytes());
    let sig: Signature = key().sign(format!("{h}.{p}").as_bytes());
    format!("{h}.{p}.{}", b64url(&sig.to_bytes()))
}

pub fn app() -> axum::Router {
    // Hermetic: no test reaches the live door. A closed port makes every
    // discovery read fail, so the stub list below is what the tools carry.
    std::env::set_var("CHORUS_DOOR", "http://127.0.0.1:9");
    let j = jwks();
    let v = OidcVerifier::new(
        ISSUER,
        || Some(vec![webid()]),
        || Some(vec![(webid(), "wren".into())]),
        || Some(vec![]),
        move || Some(j.clone()),
    );
    v.warm_allow(now());
    router(Arc::new(v), Arc::new(RwLock::new(vec!["/cards/cards".into()])))
}

pub fn initialize(version: &str, token: Option<&str>) -> Request<axum::body::Body> {
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{version}","capabilities":{{}},"clientInfo":{{"name":"t","version":"0"}}}}}}"#
    );
    let mut r = Request::post("/mcp")
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    if let Some(t) = token {
        r = r.header("authorization", format!("Bearer {t}"));
    }
    r.body(axum::body::Body::from(body)).unwrap()
}

pub async fn send(req: Request<axum::body::Body>) -> (StatusCode, String) {
    let resp = app().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}


/// A JSON-RPC POST with an explicit protocol-version header (2026-07-28
/// requests carry no initialize; the version rides every request).
pub fn rpc(method: &str, params: &str, version: &str, token: &str) -> Request<axum::body::Body> {
    let body = format!(r#"{{"jsonrpc":"2.0","id":7,"method":"{method}","params":{params}}}"#);
    Request::post("/mcp")
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", version)
        .header("mcp-method", method)
        .header("authorization", format!("Bearer {token}"))
        .body(axum::body::Body::from(body))
        .unwrap()
}

pub fn good_token() -> String {
    mint(KID, now() + 600)
}
