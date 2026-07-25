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
    // #3613 / ADR-052 — ES256/OIDC verify reasons (produced by mod oidc; live
    // here so the seam has ONE error taxonomy across both verify paths).
    UnknownAlg,      // header alg is neither HS256 nor ES256 territory
    IssuerMismatch,  // validly signed, but iss != the CSS issuer
    JwksUnreachable, // kid uncached AND the JWKS endpoint unreachable — fail-closed
}

#[derive(Debug, PartialEq, Eq)]
pub struct Claims {
    pub agent_id: String,
    pub web_id: String,
    pub aud: String,
    pub exp: u64,
    pub scope: Vec<String>, // #3564 — target graphs this token permits (empty = legacy/unscoped)
}

/// #3573 (b) per-product keys — webId → that product's signing-key BYTES. Resolved
/// once at boot from the registry ROWS (phase-1 static; phase-2 model-projected) +
/// an env-reader (key_id → bytes). The registry IS the allow-set: an unknown webId
/// has no key ⇒ fail-closed (WebIdNotAllowed). A KNOWN webId whose env key is unset
/// is kept with EMPTY bytes ⇒ verify returns BadSignature (the #3402 "no secret =
/// no access, fail-closed AS 401" semantics preserved, NOT silently demoted to 403).
pub struct KeyRegistry {
    by_webid: std::collections::HashMap<String, Vec<u8>>,
}
impl KeyRegistry {
    pub fn key_for(&self, web_id: &str) -> Option<&[u8]> {
        self.by_webid.get(web_id).map(|v| v.as_slice())
    }
    /// rows = (webId, product, key_id); read_env_key = key_id → bytes. A known row is
    /// ALWAYS inserted (missing env → empty bytes → BadSignature, not WebIdNotAllowed),
    /// so a configured-but-unset secret stays a 401 fail-closed and only a genuinely
    /// unknown webId is the 403.
    pub fn resolve(
        rows: &[(String, String, String)],
        read_env_key: impl Fn(&str) -> Option<Vec<u8>>,
    ) -> Self {
        let mut by_webid = std::collections::HashMap::new();
        for (web_id, _product, key_id) in rows {
            let bytes = read_env_key(key_id).unwrap_or_default();
            by_webid.insert(web_id.clone(), bytes);
        }
        Self { by_webid }
    }
}

/// Verify a Chorus service-token JWT. `now_secs` is injected so the check is
/// deterministic and testable. Returns the validated claims or the precise reason.
pub fn verify_token(
    token: &str,
    registry: &KeyRegistry,
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

    // #3573 (b) KEY-SELECTION — read the CLAIMED webId from the payload (UNTRUSTED)
    // ONLY to pick which product's key to test against (the standard JWT `kid`
    // pattern). We trust NOTHING from the payload until the signature verifies under
    // the selected key; an attacker claiming webId=X but lacking X's product key gets
    // X's key selected → sig FAILS. So per-product keys make scope UN-FORGEABLE, not
    // merely enforced.
    let payload_bytes = b64url_decode(parts[1]).ok_or(AuthError::Malformed)?;
    let payload = std::str::from_utf8(&payload_bytes).map_err(|_| AuthError::Malformed)?;
    let web_id = json_string(payload, "webId").ok_or(AuthError::Malformed)?;

    // Unknown webId ⇒ no product key ⇒ fail-closed (the registry IS the allow-set).
    let secret = registry.key_for(&web_id).ok_or(AuthError::WebIdNotAllowed)?;
    // FAIL CLOSED: a KNOWN webId whose env key is unset (empty) must never verify —
    // HMAC with a zero-length key would "verify" a token signed with the same empty
    // key (gate-ops/cold-eyes #3402). No key configured = no access (401, not 403).
    if secret.is_empty() {
        return Err(AuthError::BadSignature);
    }

    // 1. Signature FIRST, under the SELECTED key. constant-time compare via verify_slice.
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let provided_sig = b64url_decode(parts[2]).ok_or(AuthError::Malformed)?;
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| AuthError::Malformed)?;
    mac.update(signing_input.as_bytes());
    mac.verify_slice(&provided_sig).map_err(|_| AuthError::BadSignature)?;

    // Only now trust the rest of the payload.
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
    // 4. scope — the graphs this token may write. Parsed here, ENFORCED at the write
    //    route by scope_allows (#3567, landed). Absent ⇒ empty ⇒ legacy/unscoped.
    let scope = json_string_array(payload, "scope");

    let agent_id = json_string(payload, "agentId").unwrap_or_default();
    Ok(Claims { agent_id, web_id, aud, exp, scope })
}

/// #3573 — parse a flat JSON string-array (`"scope":["g1","g2"]`). Zero-dep, same
/// spirit as json_string/json_number. Absent/not-an-array ⇒ empty vec.
pub(crate) fn json_string_array(json: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{}\"", key);
    let Some(i) = json.find(&needle) else { return Vec::new() };
    let after = json[i + needle.len()..].trim_start();
    let Some(after) = after.strip_prefix(':') else { return Vec::new() };
    let after = after.trim_start();
    let Some(mut rest) = after.strip_prefix('[') else { return Vec::new() };
    let mut out = Vec::new();
    loop {
        rest = rest.trim_start();
        let Some(body) = rest.strip_prefix('"') else { break };
        let mut s = String::new();
        let mut chars = body.chars();
        let mut consumed = 0usize;
        while let Some(c) = chars.next() {
            consumed += c.len_utf8();
            match c {
                '"' => break,
                '\\' => { if let Some(n) = chars.next() { consumed += n.len_utf8(); s.push(n); } }
                _ => s.push(c),
            }
        }
        out.push(s);
        rest = body[consumed..].trim_start();
        rest = rest.strip_prefix(',').unwrap_or(rest);
    }
    out
}

// --- zero-dep helpers (not crypto) -----------------------------------------

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url decode, no padding (JWT flavour). Returns None on any invalid char.
pub(crate) fn b64url_decode(s: &str) -> Option<Vec<u8>> {
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
pub(crate) fn json_string(json: &str, key: &str) -> Option<String> {
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
pub(crate) fn json_number(json: &str, key: &str) -> Option<u64> {
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

/// The single secured surface for this slice (Gall's Law: exactly ONE, per the AC).
/// `/schema/domain` is served from the route table with NO SPARQL call, so it proves
/// end-to-end without a live graph. Growth = add the next exact path here (or a
/// prefix) — the seam is structural; the check stays in one place.
pub fn is_secured(path: &str, secured: &[String]) -> bool {
    // Normalize FIRST: strip query + fragment. handle_inner serves `/schema/domain?x`
    // (it matches on the path prefix), so the gate must match the same request — else
    // appending `?anything` bypasses auth (gate cold-eyes #3402). Path only, trailing-slash tolerant.
    // #3414 — MODEL-DRIVEN: `secured` is the generator's projection of the OWL auth
    // annotation (RouteTable.secured, serialized in routes.json), NOT a hardcoded constant.
    // Mark a surface secured in the model → regenerate → it lands in this set → enforced here.
    // Empty set = model declares nothing secured = nothing gated (mixed-state by construction).
    let norm = |s: &str| s.split(['?', '#']).next().unwrap_or(s).trim_end_matches('/').to_string();
    let p = norm(path);
    secured.iter().any(|s| norm(s) == p)
}

/// Phase-1 static chorus-agent web-id set (NO graph call = non-blocking at the seam,
/// keeping the #3406 freeze-class killed). Phase-2 (post-DAL) resolves from the graph.
pub fn chorus_agent_webids() -> Vec<String> {
    ["wren", "silas", "kade"]
        .iter()
        .map(|r| format!("http://localhost:3000/pods/chorus/_agents/{}/profile/card.ttl#me", r))
        .collect()
}

/// #3573 (b) PHASE-1 registry rows — STATIC, no model predicate (Silas, model
/// steward, 2026-06-23). The 3 chorus agents → product "chorus" → key_id
/// "CHORUS_SERVICE_TOKEN_SECRET" (the env var owl-api already loads, #3402). A
/// `product→signingKeyId` predicate earns its place only when a 2nd key (gathering)
/// exists to distinguish; realm-isolation is delivered by SCOPE, not key count. The
/// KeyRegistry/verify_token contract does NOT change phase-1→2 — only this row
/// SOURCE swaps static → model-projected (project_key_registry). See #3573 card.
pub fn phase1_registry_rows() -> Vec<(String, String, String)> {
    chorus_agent_webids()
        .into_iter()
        .map(|w| (w, "chorus".to_string(), "CHORUS_SERVICE_TOKEN_SECRET".to_string()))
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
    registry: &KeyRegistry,
    now_secs: u64,
    secured: &[String],
) -> Option<(u16, String)> {
    if !is_secured(path, secured) {
        return None;
    }
    let token = authorization
        .strip_prefix("Bearer ")
        .or_else(|| authorization.strip_prefix("bearer "))
        .unwrap_or("");
    match verify_token(token, registry, now_secs) {
        Ok(_) => None,
        // authenticated-but-not-permitted is 403; everything else (no/forged/expired
        // token, wrong aud) is 401.
        Err(AuthError::WebIdNotAllowed) => Some((403, err_body("forbidden", &AuthError::WebIdNotAllowed))),
        Err(e) => Some((401, err_body("unauthorized", &e))),
    }
}

pub(crate) fn err_body(kind: &str, e: &AuthError) -> String {
    format!("{{ \"error\": \"{}\", \"reason\": \"{:?}\" }}", kind, e)
}

/// Test-only HS256 minter (module-level so mod oidc's dual-verify test — spec
/// case 9 — mints a REAL legacy token through the same crypto path). Dies with
/// the HS256 arm at #3611 cutover.
#[cfg(test)]
pub(crate) fn mint_hs256_for_tests(secret: &[u8], payload: &str) -> String {
    let header = b64url_encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let p = b64url_encode(payload.as_bytes());
    let signing = format!("{}.{}", header, p);
    let mut mac = HmacSha256::new_from_slice(secret).unwrap();
    mac.update(signing.as_bytes());
    let sig = b64url_encode(&mac.finalize().into_bytes());
    format!("{}.{}.{}", header, p, sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-chorus-service-token-secret";
    fn wren_webid() -> String {
        "http://localhost:3000/pods/chorus/_agents/wren/profile/card.ttl#me".to_string()
    }
    // #3573 — build a KeyRegistry for tests (child mod can construct the private field).
    fn reg(pairs: &[(&str, &[u8])]) -> KeyRegistry {
        let mut m = std::collections::HashMap::new();
        for (w, k) in pairs { m.insert(w.to_string(), k.to_vec()); }
        KeyRegistry { by_webid: m }
    }
    fn reg_ok() -> KeyRegistry { reg(&[(&wren_webid(), SECRET)]) }
    fn payload_scoped(aud: &str, webid: &str, exp: u64, scope: &[&str]) -> String {
        let scope_json = scope.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(",");
        format!(
            r#"{{"agentId":"wren","webId":"{}","aud":"{}","exp":{},"scope":[{}]}}"#,
            webid, aud, exp, scope_json
        )
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
        let c = verify_token(&t, &reg_ok(), 1000).expect("should verify");
        assert_eq!(c.web_id, wren_webid());
        assert_eq!(c.aud, "chorus");
        assert_eq!(c.agent_id, "wren");
    }

    #[test]
    fn missing_token_rejected() {
        assert_eq!(verify_token("", &reg_ok(), 1000), Err(AuthError::Missing));
    }

    #[test]
    fn non_three_part_rejected() {
        assert_eq!(verify_token("a.b", &reg_ok(), 1000), Err(AuthError::Malformed));
    }

    #[test]
    fn forged_signature_rejected() {
        // minted with the WRONG secret → HMAC mismatch under the real secret
        let t = mint(b"attacker-secret", &payload("chorus", &wren_webid(), 9999999999));
        assert_eq!(verify_token(&t, &reg_ok(), 1000), Err(AuthError::BadSignature));
    }

    #[test]
    fn tampered_payload_rejected() {
        // valid token, then swap the payload segment for a different (unsigned) one
        let good = mint(SECRET, &payload("chorus", &wren_webid(), 9999999999));
        let evil_payload = b64url_encode(payload("chorus", &wren_webid(), 1).as_bytes());
        let parts: Vec<&str> = good.split('.').collect();
        let tampered = format!("{}.{}.{}", parts[0], evil_payload, parts[2]);
        assert_eq!(verify_token(&tampered, &reg_ok(), 1000), Err(AuthError::BadSignature));
    }

    #[test]
    fn wrong_audience_rejected() {
        // a valid gathering-realm token (signed with the same secret) must NOT work
        // against owl-api — aud isolation (#3401).
        let t = mint(SECRET, &payload("gathering", &wren_webid(), 9999999999));
        assert_eq!(verify_token(&t, &reg_ok(), 1000), Err(AuthError::WrongAudience));
    }

    #[test]
    fn expired_rejected() {
        let t = mint(SECRET, &payload("chorus", &wren_webid(), 500));
        assert_eq!(verify_token(&t, &reg_ok(), 1000), Err(AuthError::Expired));
    }

    #[test]
    fn webid_not_in_set_rejected() {
        let stranger = "http://localhost:3000/pods/chorus/_agents/stranger/profile/card.ttl#me";
        let t = mint(SECRET, &payload("chorus", stranger, 9999999999));
        assert_eq!(verify_token(&t, &reg_ok(), 1000), Err(AuthError::WebIdNotAllowed));
    }

    // --- seam gate (the end-to-end 200/401/403 + mixed-state proof) ---

    // #3414 — the model-driven secured-set the generator PROJECTS from the OWL
    // (RouteTable.secured → routes.json). Tests pass it explicitly; here it's the
    // #3402 surface, but MEMBERSHIP — not a hardcoded constant — is what gates.
    fn secured() -> Vec<String> { vec!["/schema/domain".to_string()] }

    #[test]
    fn model_driven_secured_set_gates_by_membership() {
        // #3414: is_secured is no longer a constant — a path is secured IFF it's in the
        // projected set. In-set → secured; out-of-set → open; query/fragment + trailing
        // slash normalized; empty set → nothing gated.
        let set = secured();
        assert!(is_secured("/schema/domain", &set), "in-set path is secured");
        assert!(is_secured("/schema/domain/", &set), "trailing slash still secured");
        assert!(is_secured("/schema/domain?x=1", &set), "query string still secured");
        assert!(!is_secured("/schema/domain", &[]), "empty set → model declares nothing secured");
        assert!(!is_secured("/domains", &set), "out-of-set path is open");
        // a DIFFERENT projection secures a DIFFERENT surface — proof it follows the data, not a constant
        let other = vec!["/schema/product".to_string()];
        assert!(is_secured("/schema/product", &other) && !is_secured("/schema/domain", &other),
            "the secured surface follows the projected set, not a hardcoded route");
    }

    #[test]
    fn secured_surface_with_valid_token_proceeds() {
        let t = mint(SECRET, &payload("chorus", &wren_webid(), 9999999999));
        let bearer = format!("Bearer {}", t);
        assert_eq!(seam_auth("/schema/domain", &bearer, &reg_ok(), 1000, &secured()), None);
    }

    #[test]
    fn secured_surface_without_token_is_401() {
        let r = seam_auth("/schema/domain", "", &reg_ok(), 1000, &secured());
        assert_eq!(r.map(|(c, _)| c), Some(401));
    }

    #[test]
    fn secured_surface_with_forged_token_is_401() {
        let t = mint(b"attacker", &payload("chorus", &wren_webid(), 9999999999));
        let bearer = format!("Bearer {}", t);
        assert_eq!(seam_auth("/schema/domain", &bearer, &reg_ok(), 1000, &secured()).map(|(c, _)| c), Some(401));
    }

    #[test]
    fn secured_surface_with_unlisted_webid_is_403() {
        let stranger = "http://localhost:3000/pods/chorus/_agents/stranger/profile/card.ttl#me";
        let t = mint(SECRET, &payload("chorus", stranger, 9999999999));
        let bearer = format!("Bearer {}", t);
        assert_eq!(seam_auth("/schema/domain", &bearer, &reg_ok(), 1000, &secured()).map(|(c, _)| c), Some(403));
    }

    #[test]
    fn unsecured_surface_passes_without_any_token() {
        // mixed-state: /domains is not in the projected secured set → untouched.
        assert_eq!(seam_auth("/domains", "", &reg_ok(), 1000, &secured()), None);
        assert_eq!(seam_auth("/domains/chorus", "garbage", &reg_ok(), 1000, &secured()), None);
        assert_eq!(seam_auth("/health", "", &reg_ok(), 1000, &secured()), None);
    }

    #[test]
    fn chorus_agent_set_contains_the_three_roles() {
        let set = chorus_agent_webids();
        assert!(set.iter().any(|w| w.contains("/wren/")));
        assert!(set.iter().any(|w| w.contains("/silas/")));
        assert!(set.iter().any(|w| w.contains("/kade/")));
    }

    #[test]
    fn empty_secret_fails_closed() {
        // A token "signed" with the empty key must NOT verify under an empty secret.
        // No secret configured = no access — never fail open (gate-ops/cold-eyes #3402).
        let t = mint(b"", &payload("chorus", &wren_webid(), 9999999999));
        assert_eq!(verify_token(&t, &reg(&[(&wren_webid(), b"")]), 1000), Err(AuthError::BadSignature));
    }

    #[test]
    fn empty_secret_secured_surface_is_401() {
        // at the seam, an unset secret rejects the secured surface (fail closed)
        let t = mint(b"", &payload("chorus", &wren_webid(), 9999999999));
        let bearer = format!("Bearer {}", t);
        assert_eq!(
            seam_auth("/schema/domain", &bearer, &reg(&[(&wren_webid(), b"")]), 1000, &secured()).map(|(c, _)| c),
            Some(401)
        );
    }

    #[test]
    fn query_string_does_not_bypass_the_gate() {
        // handle_inner serves /schema/domain?x, so the gate must catch it too — a
        // missing token on the query-string form is still 401 (no bypass).
        assert_eq!(
            seam_auth("/schema/domain?bypass=true", "", &reg_ok(), 1000, &secured()).map(|(c, _)| c),
            Some(401)
        );
        // and a valid token on the query-string form still proceeds
        let t = mint(SECRET, &payload("chorus", &wren_webid(), 9999999999));
        let bearer = format!("Bearer {}", t);
        assert_eq!(seam_auth("/schema/domain?x=1", &bearer, &reg_ok(), 1000, &secured()), None);
    }

    // --- #3573 (b) per-product key-selection + scope claim ---

    #[test]
    fn right_product_key_verifies_and_carries_scope() {
        let r = reg(&[(&wren_webid(), SECRET)]);
        let t = mint(SECRET, &payload_scoped("chorus", &wren_webid(), 9999999999, &["urn:chorus:domains:tests"]));
        let c = verify_token(&t, &r, 1000).expect("verifies under its own product key");
        assert_eq!(c.scope, vec!["urn:chorus:domains:tests".to_string()]);
    }

    #[test]
    fn other_products_key_cannot_sign_for_this_webid() {
        // THE structural win: a token signed with gathering's key but claiming a chorus
        // webId fails — the registry selects chorus's key (by webId), which ≠ signer's key.
        let chorus_key: &[u8] = b"chorus-key";
        let gathering_key: &[u8] = b"gathering-key";
        let r = reg(&[(&wren_webid(), chorus_key)]);
        let t = mint(gathering_key, &payload_scoped("chorus", &wren_webid(), 9999999999, &["urn:chorus:domains:tests"]));
        assert_eq!(verify_token(&t, &r, 1000), Err(AuthError::BadSignature));
    }

    #[test]
    fn unknown_webid_fails_closed() {
        let r = reg(&[]); // empty registry: no product key for anyone
        let stranger = "http://localhost:3000/pods/chorus/_agents/stranger/profile/card.ttl#me";
        let t = mint(b"any", &payload_scoped("chorus", stranger, 9999999999, &[]));
        assert_eq!(verify_token(&t, &r, 1000), Err(AuthError::WebIdNotAllowed));
    }

    #[test]
    fn scope_claim_parses_into_claims() {
        let r = reg(&[(&wren_webid(), SECRET)]);
        let t = mint(SECRET, &payload_scoped("chorus", &wren_webid(), 9999999999, &["urn:chorus:domains:tests", "urn:chorus:instances"]));
        let c = verify_token(&t, &r, 1000).expect("verifies");
        assert_eq!(c.scope, vec!["urn:chorus:domains:tests".to_string(), "urn:chorus:instances".to_string()]);
    }
}
