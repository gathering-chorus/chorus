//! Seam-auth SHARED PIECES: the error taxonomy, verified Claims, the zero-dep
//! JWT parsing helpers, and the model-driven is_secured gate.
//!
//! History: this module was the #3402 HS256 verify path (shared-secret HMAC +
//! static allow-set + KeyRegistry). That entire machinery — verify_token,
//! KeyRegistry, seam_auth, phase1_registry_rows — was DELETED by #3689/#3719:
//! the ONE verify path is mod oidc's ES256/CSS door (identity from the token,
//! scope from the model's chorus:hasScope). The only HS256 code left is the
//! test-only minter that pins the typed Hs256Retired refusal.

#[cfg(test)]
use hmac::{Hmac, Mac};
#[cfg(test)]
use sha2::Sha256;

#[cfg(test)]
type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    Missing,         // no token presented
    Hs256Retired,    // #3689 — valid-looking HS256 presented after the cutover; mint ES256 via chorus-identity-token
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

pub(crate) fn err_body(kind: &str, e: &AuthError) -> String {
    format!("{{ \"error\": \"{}\", \"reason\": \"{:?}\" }}", kind, e)
}

/// Test-only HS256 minter — the ONLY HS256 code in the workspace. It exists so
/// mod oidc's cutover test can mint a REAL legacy token and prove it gets the
/// typed Hs256Retired refusal (#3689/#3719). Never compiled into prod.
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

    // #3414 — the model-driven secured-set the generator PROJECTS from the OWL
    // (RouteTable.secured → routes.json). MEMBERSHIP — not a hardcoded constant —
    // is what gates. (The HS256 verify/seam tests that lived here died with the
    // machinery they tested, #3719; the live gate's auth tests are mod oidc's.)
    #[test]
    fn model_driven_secured_set_gates_by_membership() {
        let set = vec!["/schema/domain".to_string()];
        assert!(is_secured("/schema/domain", &set), "in-set path is secured");
        assert!(is_secured("/schema/domain/", &set), "trailing slash still secured");
        assert!(is_secured("/schema/domain?x=1", &set), "query string still secured");
        assert!(!is_secured("/schema/domain", &[]), "empty set → model declares nothing secured");
        assert!(!is_secured("/domains", &set), "out-of-set path is open");
        let other = vec!["/schema/product".to_string()];
        assert!(is_secured("/schema/product", &other) && !is_secured("/schema/domain", &other),
            "the secured surface follows the projected set, not a hardcoded route");
    }

    #[test]
    fn json_helpers_parse_flat_jwt_payloads() {
        let payload = r#"{"agentId":"wren","webId":"urn:w","aud":"chorus","exp":42,"scope":["a","b"]}"#;
        assert_eq!(json_string(payload, "aud").as_deref(), Some("chorus"));
        assert_eq!(json_number(payload, "exp"), Some(42));
        assert_eq!(json_string_array(payload, "scope"), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(json_string(payload, "absent"), None);
    }

    #[test]
    fn b64url_roundtrips_and_rejects_invalid() {
        assert_eq!(b64url_decode(&b64url_encode(b"chorus")), Some(b"chorus".to_vec()));
        assert_eq!(b64url_decode("not+valid/chars"), None);
    }
}
