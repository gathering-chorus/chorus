//! #3402 — owl-api seam auth: verify the Chorus client's HS256 service-token JWT
//! LOCALLY (no network) at the IoC seam, per ADR-042 / #3401.
//!
//! Validates, in order (signature FIRST — never trust a claim before the sig):
//!   1. HMAC-SHA256 signature against CHORUS_SERVICE_TOKEN_SECRET (#3401's secret)
//!   2. aud == "chorus"  — replay isolation: a gathering-realm token cannot be
//!      replayed against owl-api (the boundary #3401 enforces)
//!   3. exp not passed
//!   4. webId in the phase-1 static chorus-agent set (NO graph call = non-blocking;
//!      the freeze-class we killed in #3406 stays killed). Phase-2 (post-DAL)
//!      resolves the set from the graph — explicitly later, not this slice.
//!
//! owl-api stays zero-dep for PARSING (base64url + flat-JSON claim extraction are
//! implemented here). Only the crypto PRIMITIVE (hmac/sha2) is a vetted crate —
//! hand-rolling HMAC is a security anti-pattern, so we don't.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    Missing,         // no token presented
    Malformed,       // not a 3-part JWT / undecodable / missing claim
    BadSignature,    // HMAC mismatch
    WrongAudience,   // aud != "chorus"
    Expired,         // exp <= now
    WebIdNotAllowed, // webId not in the chorus-agent set
}

#[derive(Debug, PartialEq, Eq)]
pub struct Claims {
    pub agent_id: String,
    pub web_id: String,
    pub aud: String,
    pub exp: u64,
}

/// Verify a Chorus service-token JWT. `now_secs` is injected so the check is
/// deterministic and testable. Returns the validated claims or the precise reason.
pub fn verify_token(
    token: &str,
    secret: &[u8],
    allowed_webids: &[String],
    now_secs: u64,
) -> Result<Claims, AuthError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AuthError::Missing);
    }
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(AuthError::Malformed);
    }

    // 1. Signature FIRST. HMAC-SHA256 over "header.payload"; constant-time compare
    //    via Mac::verify_slice (no timing leak, no manual byte compare).
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let provided_sig = b64url_decode(parts[2]).ok_or(AuthError::Malformed)?;
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| AuthError::Malformed)?;
    mac.update(signing_input.as_bytes());
    mac.verify_slice(&provided_sig).map_err(|_| AuthError::BadSignature)?;

    // Only now decode + trust the payload.
    let payload_bytes = b64url_decode(parts[1]).ok_or(AuthError::Malformed)?;
    let payload = std::str::from_utf8(&payload_bytes).map_err(|_| AuthError::Malformed)?;

    // 2. aud isolation (load-bearing).
    let aud = json_string(payload, "aud").ok_or(AuthError::Malformed)?;
    if aud != "chorus" {
        return Err(AuthError::WrongAudience);
    }
    // 3. expiry.
    let exp = json_number(payload, "exp").ok_or(AuthError::Malformed)?;
    if exp <= now_secs {
        return Err(AuthError::Expired);
    }
    // 4. webId in the static allow-set (no graph call).
    let web_id = json_string(payload, "webId").ok_or(AuthError::Malformed)?;
    if !allowed_webids.iter().any(|w| w == &web_id) {
        return Err(AuthError::WebIdNotAllowed);
    }

    let agent_id = json_string(payload, "agentId").unwrap_or_default();
    Ok(Claims { agent_id, web_id, aud, exp })
}

// --- zero-dep helpers (not crypto) -----------------------------------------

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url decode, no padding (JWT flavour). Returns None on any invalid char.
fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut rev = [255u8; 256];
    for (i, &c) in B64URL.iter().enumerate() {
        rev[c as usize] = i as u8;
    }
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for &b in s.as_bytes() {
        if b == b'=' {
            break; // tolerate accidental padding
        }
        let v = rev[b as usize];
        if v == 255 {
            return None;
        }
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// base64url encode, no padding. Test-only (token minting); the verify path needs
/// only decode. Gated so it isn't dead code in the prod build.
#[cfg(test)]
pub(crate) fn b64url_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 4 / 3 + 3);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &b in data {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 6 {
            bits -= 6;
            out.push(B64URL[((acc >> bits) & 0x3f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(B64URL[((acc << (6 - bits)) & 0x3f) as usize] as char);
    }
    out
}

/// Extract a string field from flat JWT-payload JSON. Minimal but careful: finds
/// `"key"`, skips `:` and whitespace, requires an opening quote, reads to the next
/// unescaped quote. Returns None if absent or not a string.
fn json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let after = &json[json.find(&needle)? + needle.len()..];
    let after = after.trim_start();
    let after = after.strip_prefix(':')?.trim_start();
    let body = after.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(chars.next()?),
            _ => out.push(c),
        }
    }
    None
}

/// Extract an integer field (e.g. exp) from flat JWT-payload JSON.
fn json_number(json: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{}\"", key);
    let after = &json[json.find(&needle)? + needle.len()..];
    let after = after.trim_start().strip_prefix(':')?.trim_start();
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

// --- the seam gate (#3402: ONE secured surface, Gall's Law) -----------------

/// The single secured surface for this slice. `/schema/domain` is served from the
/// route table with NO SPARQL call, so it proves end-to-end without a live graph —
/// and growth is just adding the next path here (the seam is structural).
pub fn is_secured(path: &str) -> bool {
    path == "/schema/domain" || path.starts_with("/schema/")
}

/// Phase-1 static chorus-agent web-id set (NO graph call = non-blocking at the seam,
/// keeping the #3406 freeze-class killed). Phase-2 (post-DAL) resolves from the graph.
pub fn chorus_agent_webids() -> Vec<String> {
    ["wren", "silas", "kade"]
        .iter()
        .map(|r| format!("http://localhost:3000/pods/chorus/_agents/{}/profile/card.ttl#me", r))
        .collect()
}

/// THE SEAM AUTH CHECK. Returns:
///   - None        → proceed (unsecured surface, OR secured + valid credential)
///   - Some((c,b)) → short-circuit with this HTTP code+body (401 unauth / 403 forbidden)
///
/// Mixed-state holds by construction: a non-secured path returns None before any
/// token work, so legacy + not-yet-grown surfaces are completely untouched.
pub fn seam_auth(
    path: &str,
    authorization: &str,
    secret: &[u8],
    allowed_webids: &[String],
    now_secs: u64,
) -> Option<(u16, String)> {
    if !is_secured(path) {
        return None;
    }
    let token = authorization
        .strip_prefix("Bearer ")
        .or_else(|| authorization.strip_prefix("bearer "))
        .unwrap_or("");
    match verify_token(token, secret, allowed_webids, now_secs) {
        Ok(_) => None,
        // authenticated-but-not-permitted is 403; everything else (no/forged/expired
        // token, wrong aud) is 401.
        Err(AuthError::WebIdNotAllowed) => Some((403, err_body("forbidden", &AuthError::WebIdNotAllowed))),
        Err(e) => Some((401, err_body("unauthorized", &e))),
    }
}

fn err_body(kind: &str, e: &AuthError) -> String {
    format!("{{ \"error\": \"{}\", \"reason\": \"{:?}\" }}", kind, e)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-chorus-service-token-secret";
    fn wren_webid() -> String {
        "http://localhost:3000/pods/chorus/_agents/wren/profile/card.ttl#me".to_string()
    }
    fn allowed() -> Vec<String> {
        vec![wren_webid()]
    }

    /// Mint a real HS256 JWT for the given payload + secret (same crypto path).
    fn mint(secret: &[u8], payload: &str) -> String {
        let header = b64url_encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let p = b64url_encode(payload.as_bytes());
        let signing = format!("{}.{}", header, p);
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(signing.as_bytes());
        let sig = b64url_encode(&mac.finalize().into_bytes());
        format!("{}.{}.{}", header, p, sig)
    }
    fn payload(aud: &str, webid: &str, exp: u64) -> String {
        format!(
            r#"{{"agentId":"wren","webId":"{}","aud":"{}","exp":{}}}"#,
            webid, aud, exp
        )
    }

    #[test]
    fn valid_token_passes() {
        let t = mint(SECRET, &payload("chorus", &wren_webid(), 9999999999));
        let c = verify_token(&t, SECRET, &allowed(), 1000).expect("should verify");
        assert_eq!(c.web_id, wren_webid());
        assert_eq!(c.aud, "chorus");
        assert_eq!(c.agent_id, "wren");
    }

    #[test]
    fn missing_token_rejected() {
        assert_eq!(verify_token("", SECRET, &allowed(), 1000), Err(AuthError::Missing));
    }

    #[test]
    fn non_three_part_rejected() {
        assert_eq!(verify_token("a.b", SECRET, &allowed(), 1000), Err(AuthError::Malformed));
    }

    #[test]
    fn forged_signature_rejected() {
        // minted with the WRONG secret → HMAC mismatch under the real secret
        let t = mint(b"attacker-secret", &payload("chorus", &wren_webid(), 9999999999));
        assert_eq!(verify_token(&t, SECRET, &allowed(), 1000), Err(AuthError::BadSignature));
    }

    #[test]
    fn tampered_payload_rejected() {
        // valid token, then swap the payload segment for a different (unsigned) one
        let good = mint(SECRET, &payload("chorus", &wren_webid(), 9999999999));
        let evil_payload = b64url_encode(payload("chorus", &wren_webid(), 1).as_bytes());
        let parts: Vec<&str> = good.split('.').collect();
        let tampered = format!("{}.{}.{}", parts[0], evil_payload, parts[2]);
        assert_eq!(verify_token(&tampered, SECRET, &allowed(), 1000), Err(AuthError::BadSignature));
    }

    #[test]
    fn wrong_audience_rejected() {
        // a valid gathering-realm token (signed with the same secret) must NOT work
        // against owl-api — aud isolation (#3401).
        let t = mint(SECRET, &payload("gathering", &wren_webid(), 9999999999));
        assert_eq!(verify_token(&t, SECRET, &allowed(), 1000), Err(AuthError::WrongAudience));
    }

    #[test]
    fn expired_rejected() {
        let t = mint(SECRET, &payload("chorus", &wren_webid(), 500));
        assert_eq!(verify_token(&t, SECRET, &allowed(), 1000), Err(AuthError::Expired));
    }

    #[test]
    fn webid_not_in_set_rejected() {
        let stranger = "http://localhost:3000/pods/chorus/_agents/stranger/profile/card.ttl#me";
        let t = mint(SECRET, &payload("chorus", stranger, 9999999999));
        assert_eq!(verify_token(&t, SECRET, &allowed(), 1000), Err(AuthError::WebIdNotAllowed));
    }

    // --- seam gate (the end-to-end 200/401/403 + mixed-state proof) ---

    #[test]
    fn secured_surface_with_valid_token_proceeds() {
        let t = mint(SECRET, &payload("chorus", &wren_webid(), 9999999999));
        let bearer = format!("Bearer {}", t);
        assert_eq!(seam_auth("/schema/domain", &bearer, SECRET, &allowed(), 1000), None);
    }

    #[test]
    fn secured_surface_without_token_is_401() {
        let r = seam_auth("/schema/domain", "", SECRET, &allowed(), 1000);
        assert_eq!(r.map(|(c, _)| c), Some(401));
    }

    #[test]
    fn secured_surface_with_forged_token_is_401() {
        let t = mint(b"attacker", &payload("chorus", &wren_webid(), 9999999999));
        let bearer = format!("Bearer {}", t);
        assert_eq!(seam_auth("/schema/domain", &bearer, SECRET, &allowed(), 1000).map(|(c, _)| c), Some(401));
    }

    #[test]
    fn secured_surface_with_unlisted_webid_is_403() {
        let stranger = "http://localhost:3000/pods/chorus/_agents/stranger/profile/card.ttl#me";
        let t = mint(SECRET, &payload("chorus", stranger, 9999999999));
        let bearer = format!("Bearer {}", t);
        assert_eq!(seam_auth("/schema/domain", &bearer, SECRET, &allowed(), 1000).map(|(c, _)| c), Some(403));
    }

    #[test]
    fn unsecured_surface_passes_without_any_token() {
        // mixed-state: /domains is not yet grown into the secured set → untouched.
        assert_eq!(seam_auth("/domains", "", SECRET, &allowed(), 1000), None);
        assert_eq!(seam_auth("/domains/chorus", "garbage", SECRET, &allowed(), 1000), None);
        assert_eq!(seam_auth("/health", "", SECRET, &allowed(), 1000), None);
    }

    #[test]
    fn chorus_agent_set_contains_the_three_roles() {
        let set = chorus_agent_webids();
        assert!(set.iter().any(|w| w.contains("/wren/")));
        assert!(set.iter().any(|w| w.contains("/silas/")));
        assert!(set.iter().any(|w| w.contains("/kade/")));
    }
}
