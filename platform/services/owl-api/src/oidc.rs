//! #3613 / ADR-052 — Solid-OIDC (CSS) identity at the owl-api write seam.
//!
//! CSS (:3001) is the one issuer: it signs ES256 tokens with a private key that
//! never leaves it; this module verifies with the PUBLIC key from `/.oidc/jwks`
//! (ADR-042 §7's "signer holds private key; generated APIs verify with public").
//! The seam does authN ONLY (ADR-052 §3): prove the token, yield its WebID.
//! Role mapping / acts-as / scope policy stay in the authz layer.
//!
//! Migration posture (ADR-052 §8): `verify_any` dual-verifies — an ES256 token
//! verifies here against the CSS JWKS; an HS256 token verifies in `mod auth`
//! (the legacy path, retired per-writer by #3611 and DELETED at cutover; test
//! case 9's deletion asserts the cutover). Dispatch is by the token header's
//! `alg`, read UNTRUSTED purely to select the verify path — the same standard
//! pattern as auth.rs key-selection: nothing from the header is believed until
//! the signature verifies under the selected mechanism, and neither path's key
//! material is usable by the other (no alg-confusion surface: HS256 verifies
//! against a shared secret, ES256 against CSS's public key — disjoint stores).
//!
//! JWKS posture (ADR-052 §2): kid-keyed cache + fetch-with-cooldown, boot
//! warm-fetch that WARNS but never blocks boot, serve-from-cache on CSS blips.
//! Fail-closed ONLY when the token's kid has no cached key AND CSS is
//! unreachable — a genuinely unverifiable token, never a transient outage
//! masquerading as one (spec cases 7 + 8 define this exact boundary).
//!
//! Zero-dep ethos: JWT/JWKS parsing is hand-built here (same discipline as
//! auth.rs). Only the crypto PRIMITIVE (p256 ECDSA verify) is a vetted
//! RustCrypto crate — hand-rolling ECDSA is a security anti-pattern, exactly
//! the #3402 exception extended to the asymmetric upgrade.

use crate::auth::{self, AuthError, Claims, KeyRegistry};
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use std::cell::RefCell;
use std::collections::HashMap;

/// How long after a JWKS fetch ATTEMPT before we try again (seconds). Bounds
/// the refetch storm an unknown-kid flood could cause; within the window an
/// uncached kid stays fail-closed (JwksUnreachable) rather than re-fetching.
const JWKS_FETCH_COOLDOWN_SECS: u64 = 30;

struct JwksState {
    /// kid → SEC1 uncompressed point bytes (0x04 || x || y) for a P-256 key.
    keys: HashMap<String, Vec<u8>>,
    /// epoch-secs of the last fetch ATTEMPT (success or failure) — cooldown base.
    last_attempt: u64,
}

/// The ES256 verifier: expected issuer, the Principal allow-set (boot-resolved
/// from the model, ADR-052 §5), the kid-keyed JWKS cache, and an injected
/// fetcher (prod: curl to CSS; tests: a stub — cases 7/8 toggle reachability
/// without flapping the real issuer).
pub struct OidcVerifier {
    issuer: String,
    allow: Vec<String>,
    fetch: Box<dyn Fn() -> Option<String>>,
    state: RefCell<JwksState>,
}

impl OidcVerifier {
    pub fn new(
        issuer: &str,
        allow: Vec<String>,
        fetch: impl Fn() -> Option<String> + 'static,
    ) -> Self {
        Self {
            issuer: norm_iss(issuer),
            allow,
            fetch: Box::new(fetch),
            state: RefCell::new(JwksState { keys: HashMap::new(), last_attempt: 0 }),
        }
    }

    /// Boot warm-fetch (ADR-052 §2a): populate the cache so a CSS blip after
    /// boot still verifies cached kids. CSS-down-at-boot is a LOUD warning,
    /// never a boot blocker. Returns how many keys were cached.
    pub fn warm_fetch(&self, now_secs: u64) -> usize {
        let mut st = self.state.borrow_mut();
        st.last_attempt = now_secs;
        if let Some(body) = (self.fetch)() {
            for (kid, point) in parse_jwks(&body) {
                st.keys.insert(kid, point);
            }
        }
        st.keys.len()
    }

    /// Verify an ES256/WebID token. Signature first under the kid-selected CSS
    /// public key, then iss / aud / exp / allow-set. Every claim is untrusted
    /// until the signature verifies.
    pub fn verify(&self, token: &str, now_secs: u64) -> Result<Claims, AuthError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(AuthError::Missing);
        }
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(AuthError::Malformed);
        }
        let header_bytes = auth::b64url_decode(parts[0]).ok_or(AuthError::Malformed)?;
        let header = std::str::from_utf8(&header_bytes).map_err(|_| AuthError::Malformed)?;
        if auth::json_string(header, "alg").as_deref() != Some("ES256") {
            return Err(AuthError::UnknownAlg);
        }
        let kid = auth::json_string(header, "kid").ok_or(AuthError::Malformed)?;

        // kid → public key: cache first; on miss, one cooldown-bounded refetch
        // (key rotation lands here). Fail-closed boundary (spec 7 vs 8): no
        // cached key + no reachable JWKS = JwksUnreachable; a SUCCESSFUL fetch
        // that still lacks the kid = a key CSS never published = BadSignature.
        let point = {
            let mut st = self.state.borrow_mut();
            if !st.keys.contains_key(&kid) {
                if st.last_attempt != 0
                    && now_secs.saturating_sub(st.last_attempt) < JWKS_FETCH_COOLDOWN_SECS
                {
                    return Err(AuthError::JwksUnreachable);
                }
                st.last_attempt = now_secs;
                match (self.fetch)() {
                    None => return Err(AuthError::JwksUnreachable),
                    Some(body) => {
                        for (k, p) in parse_jwks(&body) {
                            st.keys.insert(k, p);
                        }
                        if !st.keys.contains_key(&kid) {
                            return Err(AuthError::BadSignature);
                        }
                    }
                }
            }
            st.keys.get(&kid).cloned().ok_or(AuthError::BadSignature)?
        };

        // 1. Signature FIRST (never trust a claim before the sig). JWS ES256
        //    signature = raw r||s (64 bytes) over `header.payload`.
        let vkey = VerifyingKey::from_sec1_bytes(&point).map_err(|_| AuthError::BadSignature)?;
        let sig_bytes = auth::b64url_decode(parts[2]).ok_or(AuthError::Malformed)?;
        let sig = Signature::from_slice(&sig_bytes).map_err(|_| AuthError::BadSignature)?;
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        vkey.verify(signing_input.as_bytes(), &sig)
            .map_err(|_| AuthError::BadSignature)?;

        // Only now read the payload.
        let payload_bytes = auth::b64url_decode(parts[1]).ok_or(AuthError::Malformed)?;
        let payload = std::str::from_utf8(&payload_bytes).map_err(|_| AuthError::Malformed)?;

        // 2. Issuer — the token must be CSS's, not merely validly signed by
        //    SOMEONE (spec case 4).
        let iss = auth::json_string(payload, "iss").ok_or(AuthError::Malformed)?;
        if norm_iss(&iss) != self.issuer {
            return Err(AuthError::IssuerMismatch);
        }
        // 3. Audience — a valid CSS signature minted for another service must
        //    not write to chorus (spec case 5; the check a naive seam skips).
        //    CSS client_credentials tokens carry aud=solid (the issuer's own
        //    audience); chorus-minted service tokens carry aud=chorus. Both
        //    are OUR issuer's audiences; anything else is another service's.
        let aud = auth::json_string(payload, "aud").ok_or(AuthError::Malformed)?;
        if aud != "chorus" && aud != "solid" {
            return Err(AuthError::WrongAudience);
        }
        // 4. Expiry.
        let exp = auth::json_number(payload, "exp").ok_or(AuthError::Malformed)?;
        if exp <= now_secs {
            return Err(AuthError::Expired);
        }
        // 5. WebID (Solid-OIDC claim `webid`; tolerate `webId` from our own
        //    minters) against the Principal allow-set (ADR-052 §5: the
        //    allow-set is Principal.webId alone, fail-closed on absent —
        //    parity with the HS256 registry semantics).
        let web_id = auth::json_string(payload, "webid")
            .or_else(|| auth::json_string(payload, "webId"))
            .ok_or(AuthError::Malformed)?;
        if !self.allow.iter().any(|w| w == &web_id) {
            return Err(AuthError::WebIdNotAllowed);
        }

        let scope = auth::json_string_array(payload, "scope");
        let agent_id = crate::role_from_webid(&web_id).unwrap_or_default();
        Ok(Claims { agent_id, web_id, aud, exp, scope })
    }
}

/// ADR-052 §8 dual-verify: ONE entry the seam calls for every token. The
/// header `alg` (untrusted) selects the verify path; each path then proves the
/// token cryptographically or refuses. Either valid identity is accepted
/// during rollout; the HS256 arm is deleted when #3611 migrates the last
/// writer.
pub fn verify_any(
    token: &str,
    registry: &KeyRegistry,
    oidc: &OidcVerifier,
    now_secs: u64,
) -> Result<Claims, AuthError> {
    let t = token.trim();
    if t.is_empty() {
        return Err(AuthError::Missing);
    }
    let alg = t
        .split('.')
        .next()
        .and_then(auth::b64url_decode)
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|h| auth::json_string(&h, "alg"));
    match alg.as_deref() {
        Some("ES256") => oidc.verify(t, now_secs),
        // HS256 and everything else (including alg=none) goes to the legacy
        // verifier, which refuses anything that isn't a validly HMAC-signed
        // chorus token — alg=none dies on BadSignature, not on a bypass.
        _ => auth::verify_token(t, registry, now_secs),
    }
}

/// The GET-seam gate, dual-verify edition — same contract as auth::seam_auth
/// (None = proceed; Some((code, body)) = short-circuit), same 401/403 split,
/// but every token goes through verify_any. auth::seam_auth stays untouched as
/// the legacy-only reference and dies with the HS256 arm.
pub fn seam_auth_any(
    path: &str,
    authorization: &str,
    registry: &KeyRegistry,
    oidc: &OidcVerifier,
    now_secs: u64,
    secured: &[String],
) -> Option<(u16, String)> {
    if !auth::is_secured(path, secured) {
        return None;
    }
    let token = authorization
        .strip_prefix("Bearer ")
        .or_else(|| authorization.strip_prefix("bearer "))
        .unwrap_or("");
    match verify_any(token, registry, oidc, now_secs) {
        Ok(_) => None,
        Err(AuthError::WebIdNotAllowed) => {
            Some((403, auth::err_body("forbidden", &AuthError::WebIdNotAllowed)))
        }
        Err(e) => Some((401, auth::err_body("unauthorized", &e))),
    }
}

/// ADR-052 §5 — resolve the Principal allow-set from the model at boot (one
/// query, no per-request graph call: the #3406 freeze-class stays killed).
/// `query` is injected (prod: sparql_json against Fuseki). Empty/unreachable ⇒
/// empty allow-set ⇒ every ES256 token is WebIdNotAllowed (fail-closed) while
/// the HS256 dual path keeps existing writers alive — ADR-052 §8's "no interim
/// weakening" in both directions.
/// The variable is `?v` because select_v (the DAL's proven single-var
/// extractor) parses exactly that seam.
pub const PRINCIPAL_ALLOW_QUERY: &str = "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE { GRAPH <urn:chorus:domains:security> { ?p a chorus:Principal ; chorus:webId ?v } }";

pub fn resolve_principal_webids(query: impl Fn(&str) -> Option<String>) -> Vec<String> {
    match query(PRINCIPAL_ALLOW_QUERY) {
        Some(body) => crate::select_v(&body),
        None => Vec::new(),
    }
}

/// Issuer equality with trailing-slash tolerance — `http://localhost:3001`
/// and `http://localhost:3001/` are the same issuer, and CSS emits the
/// slashed form.
fn norm_iss(s: &str) -> String {
    s.trim_end_matches('/').to_string()
}

/// Minimal JWKS parse: every EC/P-256 key object in `"keys":[…]` →
/// (kid, SEC1 uncompressed point). Hand-built like auth.rs's claim readers —
/// object-scoped (brace-balanced scan), so one key's fields never bleed into
/// another's. x/y are the fixed 32-byte base64url coordinates (RFC 7518);
/// shorter decodes are left-padded, longer rejected.
fn parse_jwks(body: &str) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let Some(keys_at) = body.find("\"keys\"") else { return out };
    let after = &body[keys_at..];
    let Some(arr_start) = after.find('[') else { return out };
    let arr = &after[arr_start..];
    let mut depth = 0usize;
    let mut obj_start = None;
    let mut in_str = false;
    let mut prev_escape = false;
    for (i, c) in arr.char_indices() {
        if in_str {
            if prev_escape {
                prev_escape = false;
            } else if c == '\\' {
                prev_escape = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    obj_start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(s) = obj_start.take() {
                        if let Some(entry) = parse_jwk_object(&arr[s..=i]) {
                            out.push(entry);
                        }
                    }
                }
            }
            ']' if depth == 0 => break,
            _ => {}
        }
    }
    out
}

fn parse_jwk_object(obj: &str) -> Option<(String, Vec<u8>)> {
    if auth::json_string(obj, "kty").as_deref() != Some("EC")
        || auth::json_string(obj, "crv").as_deref() != Some("P-256")
    {
        return None;
    }
    let kid = auth::json_string(obj, "kid")?;
    let x = coord32(&auth::json_string(obj, "x")?)?;
    let y = coord32(&auth::json_string(obj, "y")?)?;
    let mut point = Vec::with_capacity(65);
    point.push(0x04);
    point.extend_from_slice(&x);
    point.extend_from_slice(&y);
    Some((kid, point))
}

/// Decode a JWK coordinate to exactly 32 bytes (left-pad short, reject long).
fn coord32(b64: &str) -> Option<[u8; 32]> {
    let raw = auth::b64url_decode(b64)?;
    if raw.len() > 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out[32 - raw.len()..].copy_from_slice(&raw);
    Some(out)
}

// ---------------------------------------------------------------------------
// ADR-052 test spec (roles/silas/adr/ADR-052-test-spec.md) — cases 1–10.
// Tier note (the spec's coverage rule — name the tier so nothing silently
// degrades to "not actually exercised"):
//   · cases 1–6, 8–10 run HERE, headless, against a stub JWKS/issuer keypair
//     (the seam-unit tier; store-landing + spine assertions for case 1/10 are
//     the live integration run).
//   · case 7 runs HERE with the stub fetcher toggled unreachable.
//   · case 11 (revocation-drill) is a LIVE-CSS integration drill — it cannot
//     be honest against a stub (revocation is the issuer's behavior), so it is
//     scripted with the live issuer at land time, not faked here.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::b64url_encode;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;
    use std::cell::Cell;
    use std::rc::Rc;

    const ISSUER: &str = "http://localhost:3001/";
    const KID: &str = "css-test-key-1";
    const NOW: u64 = 1_760_000_000;

    fn wren_webid() -> String {
        "http://localhost:3000/pods/chorus/_agents/wren/profile/card.ttl#me".to_string()
    }
    fn silas_webid() -> String {
        "http://localhost:3000/pods/chorus/_agents/silas/profile/card.ttl#me".to_string()
    }
    fn allow() -> Vec<String> {
        vec![wren_webid(), silas_webid()]
    }

    /// The stub CSS keypair — deterministic, tests-only. Its VERIFYING half is
    /// published through the stub JWKS exactly the way CSS publishes its key.
    fn css_key() -> SigningKey {
        SigningKey::from_slice(&[7u8; 32]).expect("valid P-256 scalar")
    }
    /// A DIFFERENT issuer's keypair — for the foreign-signer arm of case 4.
    fn foreign_key() -> SigningKey {
        SigningKey::from_slice(&[9u8; 32]).expect("valid P-256 scalar")
    }

    fn jwks_json(key: &SigningKey, kid: &str) -> String {
        let point = key.verifying_key().to_encoded_point(false);
        format!(
            r#"{{"keys":[{{"kty":"EC","crv":"P-256","alg":"ES256","kid":"{}","x":"{}","y":"{}"}}]}}"#,
            kid,
            b64url_encode(point.x().unwrap()),
            b64url_encode(point.y().unwrap()),
        )
    }

    fn mint_es256(key: &SigningKey, kid: &str, payload: &str) -> String {
        let header = b64url_encode(
            format!(r#"{{"alg":"ES256","typ":"JWT","kid":"{}"}}"#, kid).as_bytes(),
        );
        let p = b64url_encode(payload.as_bytes());
        let signing_input = format!("{}.{}", header, p);
        let sig: Signature = key.sign(signing_input.as_bytes());
        format!("{}.{}.{}", header, p, b64url_encode(&sig.to_bytes()))
    }

    fn payload(iss: &str, aud: &str, webid: &str, exp: u64) -> String {
        format!(
            r#"{{"iss":"{}","aud":"{}","webid":"{}","exp":{}}}"#,
            iss, aud, webid, exp
        )
    }

    fn verifier() -> OidcVerifier {
        let jwks = jwks_json(&css_key(), KID);
        OidcVerifier::new(ISSUER, allow(), move || Some(jwks.clone()))
    }

    fn token_valid() -> String {
        mint_es256(&css_key(), KID, &payload(ISSUER, "chorus", &wren_webid(), NOW + 3600))
    }

    // case 1 — valid-allows (seam-unit half: verify yields the WebID; the
    // store-landing + spine-event half is the live integration run at land).
    #[test]
    fn valid_allows() {
        let v = verifier();
        let c = v.verify(&token_valid(), NOW).expect("valid CSS token verifies");
        assert_eq!(c.web_id, wren_webid());
        assert_eq!(c.agent_id, "wren", "agent derived from the WebID, not any env stamp");
    }

    // case 2 — forged-401: signature byte-tampered → refused, reason signature.
    #[test]
    fn forged_401() {
        let v = verifier();
        let t = token_valid();
        let mut parts: Vec<String> = t.split('.').map(String::from).collect();
        // flip one signature byte (the #3643 negative control, promoted to a unit)
        let mut sig = crate::auth::b64url_decode(&parts[2]).unwrap();
        sig[10] ^= 0x01;
        parts[2] = b64url_encode(&sig);
        let tampered = parts.join(".");
        assert_eq!(v.verify(&tampered, NOW), Err(AuthError::BadSignature));
    }

    // case 3 — expired-401.
    #[test]
    fn expired_401() {
        let v = verifier();
        let t = mint_es256(&css_key(), KID, &payload(ISSUER, "chorus", &wren_webid(), NOW - 1));
        assert_eq!(v.verify(&t, NOW), Err(AuthError::Expired));
    }

    // case 4 — wrong-issuer-401, both arms: (a) our key, foreign iss claim →
    // issuer-mismatch; (b) a genuinely foreign signer (its kid unknown to CSS's
    // JWKS) → refused before any claim is believed.
    #[test]
    fn wrong_issuer_401() {
        let v = verifier();
        let t = mint_es256(&css_key(), KID, &payload("http://evil.example/", "chorus", &wren_webid(), NOW + 3600));
        assert_eq!(v.verify(&t, NOW), Err(AuthError::IssuerMismatch));

        // foreign signer publishing its own kid: fetch succeeds but CSS never
        // published that kid → BadSignature (an unpublished key), no write.
        // Fresh verifier: arm (a)'s fetch started v's cooldown window, and a
        // cooldown-suppressed lookup is (correctly) JwksUnreachable, not this arm.
        let v2 = verifier();
        let t2 = mint_es256(&foreign_key(), "foreign-kid", &payload(ISSUER, "chorus", &wren_webid(), NOW + 3600));
        assert_eq!(v2.verify(&t2, NOW), Err(AuthError::BadSignature));
    }

    // case 5 — wrong-audience-401: valid signature is NOT sufficient.
    #[test]
    fn wrong_audience_401() {
        let v = verifier();
        let t = mint_es256(&css_key(), KID, &payload(ISSUER, "some-other-service", &wren_webid(), NOW + 3600));
        assert_eq!(v.verify(&t, NOW), Err(AuthError::WrongAudience));
    }

    // case 6 — no-token-401: no anonymous fallback, no DEPLOY_ROLE read.
    #[test]
    fn no_token_401() {
        let v = verifier();
        assert_eq!(v.verify("", NOW), Err(AuthError::Missing));
        let reg = KeyRegistry::resolve(&[], |_| None);
        let r = seam_auth_any("/schema/domain", "", &reg, &v, NOW, &["/schema/domain".to_string()]);
        assert_eq!(r.map(|(c, _)| c), Some(401));
    }

    // case 7 — jwks-unreachable-failclosed: kid uncached AND CSS unreachable
    // ⇒ 401, never allow-on-error.
    #[test]
    fn jwks_unreachable_failclosed() {
        let v = OidcVerifier::new(ISSUER, allow(), || None); // CSS down, cache empty
        assert_eq!(v.verify(&token_valid(), NOW), Err(AuthError::JwksUnreachable));
    }

    // case 8 — jwks-blip-resilient (the paired positive control): kid already
    // cached ⇒ a CSS blip does NOT fail an otherwise-valid write.
    #[test]
    fn jwks_blip_resilient() {
        let up = Rc::new(Cell::new(true));
        let up_c = up.clone();
        let jwks = jwks_json(&css_key(), KID);
        let v = OidcVerifier::new(ISSUER, allow(), move || {
            if up_c.get() { Some(jwks.clone()) } else { None }
        });
        assert_eq!(v.warm_fetch(NOW), 1, "boot warm-fetch caches the CSS key");
        up.set(false); // CSS blips
        let c = v.verify(&token_valid(), NOW + 60).expect("cached kid verifies through the blip");
        assert_eq!(c.web_id, wren_webid());
    }

    // cases 7+8 boundary — unknown kid during the blip stays fail-closed even
    // though ANOTHER kid is cached: we fail only with NO usable key for THIS
    // token, and we don't hammer CSS inside the cooldown window.
    #[test]
    fn unknown_kid_during_blip_fails_closed() {
        let jwks = jwks_json(&css_key(), KID);
        let calls = Rc::new(Cell::new(0u32));
        let calls_c = calls.clone();
        let v = OidcVerifier::new(ISSUER, allow(), move || {
            calls_c.set(calls_c.get() + 1);
            Some(jwks.clone())
        });
        v.warm_fetch(NOW);
        let rotated = mint_es256(&css_key(), "rotated-kid", &payload(ISSUER, "chorus", &wren_webid(), NOW + 3600));
        // inside the cooldown: no refetch, fail closed
        assert_eq!(v.verify(&rotated, NOW + 5), Err(AuthError::JwksUnreachable));
        assert_eq!(calls.get(), 1, "cooldown suppressed the refetch");
        // after the cooldown: refetch happens (rotation pickup path) — the stub
        // still lacks the kid, so it refuses as an unpublished key.
        assert_eq!(v.verify(&rotated, NOW + JWKS_FETCH_COOLDOWN_SECS + 1), Err(AuthError::BadSignature));
        assert_eq!(calls.get(), 2, "post-cooldown verify refetched the JWKS");
    }

    // case 9 — hs256-legacy-allows (migration only; DELETE this test when
    // #3611 retires the last HS256 writer — its deletion asserts the cutover).
    #[test]
    fn hs256_legacy_allows() {
        let secret: &[u8] = b"test-chorus-service-token-secret";
        let reg = KeyRegistry::resolve(
            &[(wren_webid(), "chorus".to_string(), "K".to_string())],
            |_| Some(secret.to_vec()),
        );
        let hs = crate::auth::mint_hs256_for_tests(
            secret,
            &format!(r#"{{"agentId":"wren","webId":"{}","aud":"chorus","exp":{}}}"#, wren_webid(), NOW + 3600),
        );
        let v = verifier();
        let c = verify_any(&hs, &reg, &v, NOW).expect("legacy HS256 accepted during rollout");
        assert_eq!(c.web_id, wren_webid());
        // and the ES256 lane works through the SAME entry — dual-verify, one seam.
        let c2 = verify_any(&token_valid(), &reg, &v, NOW).expect("ES256 accepted");
        assert_eq!(c2.web_id, wren_webid());
    }

    // case 10 — attribution-is-webid: the actor is the VERIFIED WebID; nothing
    // in the verify path reads DEPLOY_ROLE (the claims are a pure function of
    // token + JWKS + clock — proven by construction here: same token, same
    // result, no env in the signature of any function on the path).
    #[test]
    fn attribution_is_webid() {
        let v = verifier();
        let t = mint_es256(&css_key(), KID, &payload(ISSUER, "chorus", &silas_webid(), NOW + 3600));
        let c = v.verify(&t, NOW).expect("verifies");
        assert_eq!(c.web_id, silas_webid());
        assert_eq!(c.agent_id, "silas", "actor derives from the token's WebID alone");
    }

    // isolation (ADR-052 §6): a token carrying wren's webid cannot act as
    // silas — allow-set membership is per-WebID, and the seam yields exactly
    // the verified WebID; there is no claim an agent can add to act as another.
    #[test]
    fn webid_outside_allow_set_403s() {
        let v = OidcVerifier::new(ISSUER, vec![silas_webid()], {
            let jwks = jwks_json(&css_key(), KID);
            move || Some(jwks.clone())
        });
        // wren's (valid, CSS-signed) token against a silas-only allow-set
        assert_eq!(v.verify(&token_valid(), NOW), Err(AuthError::WebIdNotAllowed));
        let reg = KeyRegistry::resolve(&[], |_| None);
        let r = seam_auth_any(
            "/schema/domain",
            &format!("Bearer {}", token_valid()),
            &reg,
            &v,
            NOW,
            &["/schema/domain".to_string()],
        );
        assert_eq!(r.map(|(c, _)| c), Some(403), "authenticated-but-not-permitted is 403");
    }

    // alg=none / unknown-alg hardening: dispatch can't be tricked into a
    // signature-free path — anything not ES256 lands in the HS256 verifier and
    // dies on its signature check.
    #[test]
    fn alg_none_is_refused() {
        let header = b64url_encode(br#"{"alg":"none","typ":"JWT"}"#);
        let p = b64url_encode(payload(ISSUER, "chorus", &wren_webid(), NOW + 3600).as_bytes());
        let t = format!("{}.{}.", header, p);
        let reg = KeyRegistry::resolve(
            &[(wren_webid(), "chorus".to_string(), "K".to_string())],
            |_| Some(b"secret".to_vec()),
        );
        let v = verifier();
        assert!(verify_any(&t, &reg, &v, NOW).is_err(), "alg=none must never verify");
    }

    // model-resolved allow-set (ADR-052 §5): resolves Principal.webId rows;
    // unreachable graph ⇒ EMPTY set ⇒ fail-closed for ES256 while HS256 keeps
    // existing writers alive (no interim weakening in either direction).
    #[test]
    fn principal_allow_set_resolves_and_fails_closed() {
        let body = format!(
            r#"{{"head":{{"vars":["v"]}},"results":{{"bindings":[{{"v":{{"type":"literal","value":"{}"}}}},{{"v":{{"type":"literal","value":"{}"}}}}]}}}}"#,
            wren_webid(),
            silas_webid()
        );
        let got = resolve_principal_webids(|q| {
            assert!(q.contains("chorus:Principal"), "queries the Principal class");
            assert!(q.contains("urn:chorus:domains:security"), "scoped to the security domain graph");
            Some(body.clone())
        });
        assert_eq!(got, vec![wren_webid(), silas_webid()]);
        assert!(resolve_principal_webids(|_| None).is_empty(), "unreachable ⇒ empty ⇒ fail-closed");
    }

    // JWKS parse hardening: multiple keys, non-EC keys skipped, fields never
    // bleed across key objects.
    #[test]
    fn jwks_parse_is_object_scoped() {
        let k1 = css_key();
        let k2 = foreign_key();
        let p1 = k1.verifying_key().to_encoded_point(false);
        let p2 = k2.verifying_key().to_encoded_point(false);
        let body = format!(
            r#"{{"keys":[
                {{"kty":"RSA","kid":"rsa-1","n":"xxxx","e":"AQAB"}},
                {{"kty":"EC","crv":"P-256","kid":"a","x":"{}","y":"{}"}},
                {{"kty":"EC","crv":"P-256","kid":"b","x":"{}","y":"{}"}}
            ]}}"#,
            b64url_encode(p1.x().unwrap()),
            b64url_encode(p1.y().unwrap()),
            b64url_encode(p2.x().unwrap()),
            b64url_encode(p2.y().unwrap()),
        );
        let keys = parse_jwks(&body);
        assert_eq!(keys.len(), 2, "RSA key skipped, both EC keys parsed");
        assert_eq!(keys[0].0, "a");
        assert_eq!(keys[1].0, "b");
        assert_ne!(keys[0].1, keys[1].1, "each key got ITS OWN coordinates");
    }
}
