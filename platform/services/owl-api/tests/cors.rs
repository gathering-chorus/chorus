//! #3373: pages ride beside their generated APIs — every response carries
//! CORS so a localhost-served page (chorus-api :3340) can read the
//! localhost-BOUND generated API (:3360) cross-origin. The listener binds
//! 127.0.0.1 and the tunnel never exposes it, so the permissive origin is
//! loopback-scoped; the #3355 expiry tooth (security ADR #3372) replaces it
//! when generated authn lands.

use owl_api::http_response;

#[test]
fn response_carries_cors_allow_origin() {
    let resp = http_response("200 OK", "{}");
    assert!(
        resp.contains("Access-Control-Allow-Origin: *"),
        "generated API responses must be page-consumable (CORS), got: {}",
        resp
    );
}

#[test]
fn response_shape_unchanged_for_existing_consumers() {
    let body = "{\"a\":1}";
    let resp = http_response("200 OK", body);
    assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(resp.contains("Content-Type: application/json"));
    assert!(resp.contains(&format!("Content-Length: {}", body.len())));
    assert!(resp.ends_with(body));
}

#[test]
fn error_responses_carry_cors_too() {
    // The page must be able to READ a 404 (e.g. v1-only ids probing /domains/:name)
    // instead of the browser masking it as an opaque network error.
    let resp = http_response("404 Not Found", "{\"error\":\"not-found\"}");
    assert!(resp.contains("Access-Control-Allow-Origin: *"));
}
