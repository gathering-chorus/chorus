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

/// How long the model-resolved Principal allow-set stays fresh (seconds).
/// MUST be ≤ the CSS token TTL: revocation is a model edit (drop the
/// Principal), and the AC requires a revoked credential's writes to refuse
/// within ONE token TTL — a lazily re-resolved cache with this bound delivers
/// that without a per-request graph call (the #3406 freeze-class stays killed).
const ALLOW_TTL_SECS: u64 = 300;
/// Cooldown between re-resolve ATTEMPTS once the cache is stale.
const ALLOW_RETRY_COOLDOWN_SECS: u64 = 30;

struct JwksState {
    /// kid → SEC1 uncompressed point bytes (0x04 || x || y) for a P-256 key.
    keys: HashMap<String, Vec<u8>>,
    /// epoch-secs of the last fetch ATTEMPT (success or failure) — cooldown base.
    last_attempt: u64,
}

struct AllowState {
    webids: Vec<String>,
    /// epoch-secs of the last SUCCESSFUL resolve — freshness base.
    fetched_at: u64,
    /// epoch-secs of the last resolve ATTEMPT — retry-cooldown base.
    last_attempt: u64,
}

/// webId → role, resolved from `chorus:holdsRole` (ADR-054 §3.3). Same TTL
/// discipline as the allow-set: a role REASSIGNMENT is a model edit and must
/// take effect within one token TTL, no restart.
struct RoleState {
    pairs: Vec<(String, String)>,
    fetched_at: u64,
    last_attempt: u64,
}

/// webId → scopes, resolved from `chorus:hasScope` (#3689). Scope was a
/// SELF-DECLARED claim on the HS256 tokens — the caller chose its own
/// authorization at mint. CSS cannot issue scoped client_credentials (spiked
/// live 2026-07-30: the scope param is silently ignored), so scope becomes
/// governed model data with the same TTL discipline as the allow-set and the
/// role map: grant/revoke is a model edit, live within one token TTL.
struct ScopeState {
    grants: Vec<(String, Vec<String>)>,
    fetched_at: u64,
    last_attempt: u64,
}

/// The ES256 verifier: expected issuer, the Principal allow-set (boot-resolved
/// from the model, ADR-052 §5), the kid-keyed JWKS cache, and an injected
/// fetcher (prod: curl to CSS; tests: a stub — cases 7/8 toggle reachability
/// without flapping the real issuer).
pub struct OidcVerifier {
    issuer: String,
    /// Principal allow-set resolver (prod: the model query; tests: a stub).
    /// None = graph unreachable — DISTINCT from Some(empty) = nobody allowed.
    resolve_allow: Box<dyn Fn() -> Option<Vec<String>>>,
    /// webId → role resolver over `chorus:holdsRole` (ADR-054 §3.3). Same
    /// None-vs-Some(empty) split as the allow-set.
    resolve_roles: Box<dyn Fn() -> Option<Vec<(String, String)>>>,
    /// webId → scopes resolver over `chorus:hasScope` (#3689). Same split.
    resolve_scopes: Box<dyn Fn() -> Option<Vec<(String, Vec<String>)>>>,
    fetch: Box<dyn Fn() -> Option<String>>,
    state: RefCell<JwksState>,
    allow: RefCell<AllowState>,
    roles: RefCell<RoleState>,
    scopes: RefCell<ScopeState>,
}

impl OidcVerifier {
    pub fn new(
        issuer: &str,
        resolve_allow: impl Fn() -> Option<Vec<String>> + 'static,
        resolve_roles: impl Fn() -> Option<Vec<(String, String)>> + 'static,
        resolve_scopes: impl Fn() -> Option<Vec<(String, Vec<String>)>> + 'static,
        fetch: impl Fn() -> Option<String> + 'static,
    ) -> Self {
        Self {
            issuer: norm_iss(issuer),
            resolve_allow: Box::new(resolve_allow),
            resolve_roles: Box::new(resolve_roles),
            resolve_scopes: Box::new(resolve_scopes),
            fetch: Box::new(fetch),
            state: RefCell::new(JwksState { keys: HashMap::new(), last_attempt: 0 }),
            allow: RefCell::new(AllowState { webids: Vec::new(), fetched_at: 0, last_attempt: 0 }),
            roles: RefCell::new(RoleState { pairs: Vec::new(), fetched_at: 0, last_attempt: 0 }),
            scopes: RefCell::new(ScopeState { grants: Vec::new(), fetched_at: 0, last_attempt: 0 }),
        }
    }

    /// Boot-prime the allow-set (same posture as warm_fetch: loud on failure,
    /// never boot-blocking). Returns how many Principal webids were cached.
    pub fn warm_allow(&self, now_secs: u64) -> usize {
        let mut al = self.allow.borrow_mut();
        al.last_attempt = now_secs;
        if let Some(v) = (self.resolve_allow)() {
            al.webids = v;
            al.fetched_at = now_secs;
        }
        al.webids.len()
    }

    /// Membership with TTL'd lazy refresh: past ALLOW_TTL_SECS the set is
    /// re-resolved (cooldown-bounded) so a model-side revocation — dropping the
    /// Principal — takes effect within one TTL, no restart (the #3613 AC's
    /// revocation drill). Resolve FAILURE empties the set (fail-closed): a
    /// write needs the store anyway, so refusing authz when the store is
    /// unreachable refuses nothing that could have succeeded.
    fn allowed(&self, web_id: &str, now_secs: u64) -> bool {
        let mut al = self.allow.borrow_mut();
        let stale = now_secs.saturating_sub(al.fetched_at) >= ALLOW_TTL_SECS;
        let can_retry = now_secs.saturating_sub(al.last_attempt) >= ALLOW_RETRY_COOLDOWN_SECS
            || al.last_attempt == 0;
        if stale && can_retry {
            al.last_attempt = now_secs;
            match (self.resolve_allow)() {
                Some(v) => {
                    al.webids = v;
                    al.fetched_at = now_secs;
                }
                None => al.webids.clear(),
            }
        }
        al.webids.iter().any(|w| w == web_id)
    }

    /// Boot-prime the webId→role map. Returns how many holdsRole edges cached.
    pub fn warm_roles(&self, now_secs: u64) -> usize {
        let mut rl = self.roles.borrow_mut();
        rl.last_attempt = now_secs;
        if let Some(v) = (self.resolve_roles)() {
            rl.pairs = v;
            rl.fetched_at = now_secs;
        }
        rl.pairs.len()
    }

    /// ADR-054 §3.3 — the caller's role, ASKED of the graph. A WebID with no
    /// `holdsRole` edge has NO role (None), which is the honest answer for a
    /// service or guest Principal: it is a real, allowed identity that holds no
    /// role, and downstream authZ compares against `ownedBy` and fails closed.
    /// Resolve failure empties the map (fail-closed), same posture as `allowed`.
    pub fn role_for(&self, web_id: &str, now_secs: u64) -> Option<String> {
        let mut rl = self.roles.borrow_mut();
        let stale = now_secs.saturating_sub(rl.fetched_at) >= ALLOW_TTL_SECS;
        let can_retry = now_secs.saturating_sub(rl.last_attempt) >= ALLOW_RETRY_COOLDOWN_SECS
            || rl.last_attempt == 0;
        if stale && can_retry {
            rl.last_attempt = now_secs;
            match (self.resolve_roles)() {
                Some(v) => {
                    rl.pairs = v;
                    rl.fetched_at = now_secs;
                }
                None => rl.pairs.clear(),
            }
        }
        rl.pairs.iter().find(|(w, _)| w == web_id).map(|(_, r)| r.clone())
    }

    /// #3689 — the caller's scopes, ASKED of the graph. No edge ⇒ no scopes
    /// (a scoped write refuses); resolve failure empties the map (fail closed,
    /// never stale grants); refresh on the ALLOW_TTL cadence so revocation is
    /// a model edit that lands within one token TTL.
    pub fn scopes_for(&self, web_id: &str, now_secs: u64) -> Vec<String> {
        let mut sc = self.scopes.borrow_mut();
        let stale = now_secs.saturating_sub(sc.fetched_at) >= ALLOW_TTL_SECS;
        let can_retry = now_secs.saturating_sub(sc.last_attempt) >= ALLOW_RETRY_COOLDOWN_SECS
            || sc.last_attempt == 0;
        if stale && can_retry {
            sc.last_attempt = now_secs;
            match (self.resolve_scopes)() {
                Some(v) => {
                    sc.grants = v;
                    sc.fetched_at = now_secs;
                }
                None => sc.grants.clear(),
            }
        }
        sc.grants
            .iter()
            .find(|(w, _)| w == web_id)
            .map(|(_, g)| g.clone())
            .unwrap_or_default()
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
        if !self.allowed(&web_id, now_secs) {
            return Err(AuthError::WebIdNotAllowed);
        }

        // 6. Role — ASKED of the graph (`?principal chorus:holdsRole ?role`),
        //    never parsed out of the WebID string (ADR-054 §3.3). Renaming an
        //    agent's WebID, or a WebID whose string encodes no role, now
        //    resolves correctly; a Principal that holds no role gets none.
        // #3689 — scope comes FROM THE MODEL (chorus:hasScope), never from a
        // claim. A claim was self-declared at mint; CSS cannot mint scoped
        // client_credentials anyway (spiked live 2026-07-30). The HS256 arm in
        // verify_any keeps its claim scope until #3689 deletes it — migration
        // bridge only.
        let scope = self.scopes_for(&web_id, now_secs);
        let agent_id = self.role_for(&web_id, now_secs).unwrap_or_default();
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
    // #3689 — THE CUTOVER. The HS256 arm is DELETED: one verify path, one key
    // model. Every caller migrated (werk-test + the four ops scripts) mints
    // ES256 identity via chorus-identity-token; scope is model data
    // (chorus:hasScope). A stale HS256 token — or alg=none, or anything that
    // is not ES256 — gets a TYPED refusal naming the retirement, never a
    // silent accept and never a fallback. `registry` stays in the signature
    // until the chorus-api envelope door migrates (its own card); it is unused
    // here by design.
    let _ = registry;
    match alg.as_deref() {
        Some("ES256") => oidc.verify(t, now_secs),
        Some("HS256") => Err(AuthError::Hs256Retired),
        _ => Err(AuthError::UnknownAlg),
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

/// None = graph unreachable (caller decides the fail-closed posture);
/// Some(empty) = reachable and genuinely nobody allowed.
pub fn resolve_principal_webids(query: impl Fn(&str) -> Option<String>) -> Option<Vec<String>> {
    query(PRINCIPAL_ALLOW_QUERY).map(|body| crate::select_v(&body))
}

/// ADR-054 §3.3 — resolve webId→role from `chorus:holdsRole`, the edge that
/// makes role assignment GOVERNED DATA rather than a WebID naming convention.
/// Emitted as one `?v` row per edge (`"<webid> <role-iri>"`) so the DAL's proven
/// single-var extractor parses it — a WebID can carry no space, so the first
/// one is an unambiguous separator. The role IRI travels WHOLE: naming it here
/// would trade the WebID convention this card retires for an IRI convention.
/// Principals with no `holdsRole` are simply absent: allowed to authenticate,
/// holding no role.
pub const PRINCIPAL_ROLE_QUERY: &str = "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE { GRAPH <urn:chorus:domains:security> { ?p a chorus:Principal ; chorus:webId ?w ; chorus:holdsRole ?r } BIND(CONCAT(STR(?w), \" \", STR(?r)) AS ?v) }";

/// A role IRI's name: the local part after the last `#`, `/` or `:`, minus a
/// `role-` prefix if the IRI uses one. Convention-TOLERANT by construction —
/// `…#role-wren`, `…#wren`, `…/roles/wren` and `urn:chorus:roles:wren` all name
/// `wren` — so no edge is silently dropped for not matching a fragment shape.
fn role_name(role_iri: &str) -> Option<&str> {
    let local = role_iri.rsplit(['#', '/', ':']).next()?;
    let name = local.strip_prefix("role-").unwrap_or(local);
    (!name.is_empty()).then_some(name)
}

/// None = graph unreachable (caller fails closed); Some(empty) = reachable and
/// genuinely no role assignments. A row this cannot read is DROPPED and SAID —
/// fail-closed is right, but a silently vanishing role assignment would be a
/// authZ refusal with no stated cause.
pub fn resolve_principal_roles(
    query: impl Fn(&str) -> Option<String>,
) -> Option<Vec<(String, String)>> {
    query(PRINCIPAL_ROLE_QUERY).map(|body| {
        crate::select_v(&body)
            .into_iter()
            .filter_map(|row| {
                let unreadable = || {
                    eprintln!(
                        "chorus-oidc: WARNING — unreadable holdsRole row {:?}; that Principal carries NO role until the edge is fixed (#3688)",
                        row
                    );
                    None
                };
                let Some((w, r)) = row.split_once(' ') else { return unreadable() };
                match role_name(r) {
                    Some(name) if !w.is_empty() => Some((w.to_string(), name.to_string())),
                    _ => unreadable(),
                }
            })
            .collect()
    })
}

/// #3689 — resolve webId→scopes from `chorus:hasScope`: the graphs a Principal
/// may write, as GOVERNED DATA. One `?v` row per edge ("<webid> <scope-uri>");
/// a webid carries no space so the first is the separator; multiple edges per
/// Principal are grouped by the resolver. Principals with no edge are simply
/// absent: they authenticate, and any scoped write refuses.
pub const PRINCIPAL_SCOPE_QUERY: &str = "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE { GRAPH <urn:chorus:domains:security> { ?p a chorus:Principal ; chorus:webId ?w ; chorus:hasScope ?s } BIND(CONCAT(STR(?w), \" \", STR(?s)) AS ?v) }";

/// None = graph unreachable (caller fails closed); Some(empty) = reachable and
/// no grants exist. Rows without a separator are dropped and said.
pub fn resolve_principal_scopes(
    query: impl Fn(&str) -> Option<String>,
) -> Option<Vec<(String, Vec<String>)>> {
    query(PRINCIPAL_SCOPE_QUERY).map(|body| {
        let mut grants: Vec<(String, Vec<String>)> = Vec::new();
        for row in crate::select_v(&body) {
            let Some((w, sc)) = row.split_once(' ') else {
                eprintln!("chorus-oidc: WARNING — unreadable hasScope row {:?}; that grant is INERT until the edge is fixed (#3689)", row);
                continue;
            };
            if w.is_empty() || sc.is_empty() { continue; }
            match grants.iter_mut().find(|(gw, _)| gw == w) {
                Some((_, list)) => list.push(sc.to_string()),
                None => grants.push((w.to_string(), vec![sc.to_string()])),
            }
        }
        grants
    })
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
    /// The graph's holdsRole edges for the stub allow-set (ADR-054 §3.3).
    fn roles() -> Vec<(String, String)> {
        vec![(wren_webid(), "wren".to_string()), (silas_webid(), "silas".to_string())]
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
        let v = OidcVerifier::new(ISSUER, || Some(allow()), || Some(roles()), || Some(vec![]), move || Some(jwks.clone()));
        v.warm_allow(NOW);
        v
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
        assert_eq!(c.agent_id, "wren", "role resolved from the graph, not any env stamp");
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
        let v = OidcVerifier::new(ISSUER, || Some(allow()), || Some(roles()), || Some(vec![]), || None); // CSS down, JWKS cache empty
        v.warm_allow(NOW);
        assert_eq!(v.verify(&token_valid(), NOW), Err(AuthError::JwksUnreachable));
    }

    // case 8 — jwks-blip-resilient (the paired positive control): kid already
    // cached ⇒ a CSS blip does NOT fail an otherwise-valid write.
    #[test]
    fn jwks_blip_resilient() {
        let up = Rc::new(Cell::new(true));
        let up_c = up.clone();
        let jwks = jwks_json(&css_key(), KID);
        let v = OidcVerifier::new(ISSUER, || Some(allow()), || Some(roles()), || Some(vec![]), move || {
            if up_c.get() { Some(jwks.clone()) } else { None }
        });
        v.warm_allow(NOW);
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
        let v = OidcVerifier::new(ISSUER, || Some(allow()), || Some(roles()), || Some(vec![]), move || {
            calls_c.set(calls_c.get() + 1);
            Some(jwks.clone())
        });
        v.warm_fetch(NOW);
        v.warm_allow(NOW);
        let rotated = mint_es256(&css_key(), "rotated-kid", &payload(ISSUER, "chorus", &wren_webid(), NOW + 3600));
        // inside the cooldown: no refetch, fail closed
        assert_eq!(v.verify(&rotated, NOW + 5), Err(AuthError::JwksUnreachable));
        assert_eq!(calls.get(), 1, "cooldown suppressed the refetch");
        // after the cooldown: refetch happens (rotation pickup path) — the stub
        // still lacks the kid, so it refuses as an unpublished key.
        assert_eq!(v.verify(&rotated, NOW + JWKS_FETCH_COOLDOWN_SECS + 1), Err(AuthError::BadSignature));
        assert_eq!(calls.get(), 2, "post-cooldown verify refetched the JWKS");
    }

    // case 9 — #3689 CUTOVER: the deletion this test's predecessor promised.
    // (hs256_legacy_allows said "DELETE this test when the last HS256 writer
    // migrates — its deletion asserts the cutover." This is that assertion.)
    // A validly-signed HS256 token is REFUSED with the typed retirement error.
    #[test]
    fn hs256_is_refused_with_a_typed_error() {
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
        assert_eq!(verify_any(&hs, &reg, &v, NOW), Err(AuthError::Hs256Retired),
            "a valid HS256 signature is refused BY POLICY, with the reason named");
        // ES256 still verifies through the same single entry.
        assert!(verify_any(&token_valid(), &reg, &v, NOW).is_ok());
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
        let v = OidcVerifier::new(ISSUER, || Some(vec![silas_webid()]), || Some(roles()), || Some(vec![]), {
            let jwks = jwks_json(&css_key(), KID);
            move || Some(jwks.clone())
        });
        v.warm_allow(NOW);
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

    // case 11 (unit half) — revocation-drill: dropping the Principal from the
    // model refuses that WebID within one ALLOW_TTL (≤ token TTL), no restart.
    // The live half (real CSS cred revoked, real store) runs at land time.
    #[test]
    fn revocation_propagates_within_one_ttl() {
        let revoked = Rc::new(Cell::new(false));
        let revoked_c = revoked.clone();
        let jwks = jwks_json(&css_key(), KID);
        let v = OidcVerifier::new(
            ISSUER,
            move || Some(if revoked_c.get() { vec![] } else { vec![wren_webid(), silas_webid()] }),
            || Some(roles()),
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        assert!(v.verify(&token_valid(), NOW).is_ok(), "pre-revocation: verifies");
        revoked.set(true); // the model edit: Principal dropped
        // inside the TTL the cache may still allow — that's the accepted bound
        assert!(v.verify(&token_valid(), NOW + 10).is_ok(), "within TTL: stale cache may allow");
        // past the TTL the refresh runs and the WebID is refused
        assert_eq!(
            v.verify(&token_valid(), NOW + ALLOW_TTL_SECS + 1),
            Err(AuthError::WebIdNotAllowed),
            "past one TTL: revoked Principal is refused, no restart"
        );
    }

    // allow-set resolver unreachable at refresh time ⇒ fail-closed (empty),
    // never stale-forever: authz refuses when membership cannot be proven.
    #[test]
    fn allow_refresh_failure_fails_closed() {
        let up = Rc::new(Cell::new(true));
        let up_c = up.clone();
        let jwks = jwks_json(&css_key(), KID);
        let v = OidcVerifier::new(
            ISSUER,
            move || if up_c.get() { Some(vec![wren_webid()]) } else { None },
            || Some(roles()),
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        assert!(v.verify(&token_valid(), NOW).is_ok());
        up.set(false); // graph goes unreachable
        assert_eq!(
            v.verify(&token_valid(), NOW + ALLOW_TTL_SECS + 1),
            Err(AuthError::WebIdNotAllowed),
            "membership unprovable ⇒ refused (a write needs the store anyway)"
        );
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
        assert_eq!(got, Some(vec![wren_webid(), silas_webid()]));
        assert_eq!(resolve_principal_webids(|_| None), None, "unreachable is DISTINCT from empty");
    }

    // -----------------------------------------------------------------------
    // #3688 / ADR-054 §3.3 — role is ASKED of the graph (chorus:holdsRole),
    // never parsed out of the WebID string.
    // -----------------------------------------------------------------------

    /// A WebID whose STRING says one thing and whose holdsRole edge says
    /// another: the graph wins. This is the case the retired parser got wrong —
    /// it read the path segment and never consulted the model. Renaming an
    /// agent's WebID, or a WebID that encodes no role at all, resolves here.
    #[test]
    fn role_comes_from_holds_role_not_the_webid_string() {
        let jwks = jwks_json(&css_key(), KID);
        // the pod segment reads "wren"; the model says this Principal holds silas
        let v = OidcVerifier::new(
            ISSUER,
            || Some(allow()),
            || Some(vec![(wren_webid(), "silas".to_string())]),
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        v.warm_roles(NOW);
        let c = v.verify(&token_valid(), NOW).expect("token verifies");
        assert_eq!(
            c.agent_id, "silas",
            "role came from holdsRole; the string parser would have said wren"
        );
    }

    /// An opaque WebID — nothing in the string to parse — still resolves,
    /// because the edge carries the role. The parser returned None here.
    #[test]
    fn opaque_webid_still_resolves_a_role() {
        let opaque = "https://id.lightlifeurbangardens.com/a7f3e1c9".to_string();
        let jwks = jwks_json(&css_key(), KID);
        let o2 = opaque.clone();
        let v = OidcVerifier::new(
            ISSUER,
            move || Some(vec![o2.clone()]),
            {
                let o = opaque.clone();
                move || Some(vec![(o.clone(), "kade".to_string())])
            },
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        v.warm_roles(NOW);
        let t = mint_es256(&css_key(), KID, &payload(ISSUER, "chorus", &opaque, NOW + 3600));
        assert_eq!(v.verify(&t, NOW).expect("verifies").agent_id, "kade");
    }

    /// A Principal that holds NO role — a service or a guest — authenticates
    /// but carries no role, so downstream ownedBy authZ fails closed. The
    /// parser handed such a caller a role-shaped string from its own WebID
    /// (`marknakib`), which is exactly the guest-authorization surface #3682
    /// closes at the door.
    #[test]
    fn principal_without_holds_role_carries_no_role() {
        let guest = "https://id.lightlifeurbangardens.com/marknakib/profile/card#me".to_string();
        let jwks = jwks_json(&css_key(), KID);
        let g2 = guest.clone();
        let v = OidcVerifier::new(
            ISSUER,
            move || Some(vec![g2.clone()]),
            || Some(roles()), // guest is allowed, but holds no role
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        v.warm_roles(NOW);
        let t = mint_es256(&css_key(), KID, &payload(ISSUER, "chorus", &guest, NOW + 3600));
        let c = v.verify(&t, NOW).expect("a guest still authenticates");
        assert_eq!(c.web_id, guest);
        assert_eq!(c.agent_id, "", "no holdsRole edge ⇒ no role, not a parsed one");
    }

    /// Reassignment drill, the role twin of the revocation drill: editing the
    /// holdsRole edge in the model takes effect within ONE TTL, no restart.
    #[test]
    fn role_reassignment_lands_within_one_ttl() {
        let jwks = jwks_json(&css_key(), KID);
        let reassigned = Rc::new(Cell::new(false));
        let rc = Rc::clone(&reassigned);
        let v = OidcVerifier::new(
            ISSUER,
            || Some(allow()),
            move || {
                Some(vec![(
                    wren_webid(),
                    if rc.get() { "kade".to_string() } else { "wren".to_string() },
                )])
            },
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        v.warm_roles(NOW);
        assert_eq!(v.verify(&token_valid(), NOW).unwrap().agent_id, "wren");
        reassigned.set(true); // the model edit
        assert_eq!(
            v.verify(&token_valid(), NOW + 10).unwrap().agent_id,
            "wren",
            "within TTL: the stale map may still answer — the accepted bound"
        );
        assert_eq!(
            v.verify(&token_valid(), NOW + ALLOW_TTL_SECS + 1).unwrap().agent_id,
            "kade",
            "past one TTL: the reassignment is live, no restart"
        );
    }

    /// Graph unreachable ⇒ no role (fail-closed), matching the allow-set's
    /// posture: a write needs the store anyway.
    #[test]
    fn role_map_fails_closed_when_graph_unreachable() {
        let jwks = jwks_json(&css_key(), KID);
        let up = Rc::new(Cell::new(true));
        let uc = Rc::clone(&up);
        let v = OidcVerifier::new(
            ISSUER,
            || Some(allow()),
            move || if uc.get() { Some(roles()) } else { None },
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        v.warm_roles(NOW);
        assert_eq!(v.verify(&token_valid(), NOW).unwrap().agent_id, "wren");
        up.set(false);
        assert_eq!(
            v.verify(&token_valid(), NOW + ALLOW_TTL_SECS + 1).unwrap().agent_id,
            "",
            "graph unreachable ⇒ no role rather than a stale or guessed one"
        );
    }

    /// The query asks the holdsRole EDGE in the security graph, and unreachable
    /// stays distinct from empty.
    #[test]
    fn principal_role_query_asks_the_holds_role_edge() {
        let body = format!(
            r#"{{"head":{{"vars":["v"]}},"results":{{"bindings":[{{"v":{{"type":"literal","value":"{} https://jeffbridwell.com/chorus#role-wren"}}}},{{"v":{{"type":"literal","value":"{} https://jeffbridwell.com/chorus#role-silas"}}}},{{"v":{{"type":"literal","value":"malformed-no-separator"}}}}]}}}}"#,
            wren_webid(),
            silas_webid()
        );
        let got = resolve_principal_roles(|q| {
            assert!(q.contains("chorus:holdsRole"), "asks the holdsRole edge");
            assert!(q.contains("urn:chorus:domains:security"), "scoped to the security graph");
            Some(body.clone())
        });
        assert_eq!(
            got,
            Some(roles()),
            "pairs parse; a row without a separator is dropped, never half-parsed"
        );
        assert_eq!(resolve_principal_roles(|_| None), None, "unreachable is DISTINCT from empty");
    }

    /// The role IRI's SHAPE is not a second naming convention: whatever the
    /// roles domain mints — `#role-wren`, a bare `#wren`, a path IRI — names
    /// the same role. This is the residual coupling Kade and Wren both flagged
    /// on the first present; a fragment-prefix assumption would have dropped
    /// these rows silently, which is a refusal with no stated cause.
    #[test]
    fn role_iri_shape_is_not_a_naming_convention() {
        for iri in [
            "https://jeffbridwell.com/chorus#role-wren",
            "https://jeffbridwell.com/chorus#wren",
            "https://jeffbridwell.com/chorus/roles/wren",
            "urn:chorus:roles:role-wren",
        ] {
            let body = format!(
                r#"{{"head":{{"vars":["v"]}},"results":{{"bindings":[{{"v":{{"type":"literal","value":"{} {}"}}}}]}}}}"#,
                wren_webid(),
                iri
            );
            assert_eq!(
                resolve_principal_roles(|_| Some(body.clone())),
                Some(vec![(wren_webid(), "wren".to_string())]),
                "{} names role wren",
                iri
            );
        }
        // an edge that names nothing is dropped (and warned), not half-read
        let empty = format!(
            r#"{{"head":{{"vars":["v"]}},"results":{{"bindings":[{{"v":{{"type":"literal","value":"{} https://jeffbridwell.com/chorus#"}}}}]}}}}"#,
            wren_webid()
        );
        assert_eq!(resolve_principal_roles(|_| Some(empty.clone())), Some(vec![]));
    }


    // -----------------------------------------------------------------------
    // #3689 — scope is MODEL DATA, not a token claim. CSS cannot issue scoped
    // client_credentials (spiked live 2026-07-30: scope param silently
    // ignored), and the HS256 scope claim was self-declared at mint — the
    // caller chose its own authorization. chorus:hasScope edges on the
    // Principal replace both: governance-chosen, TTL'd, revocable by model
    // edit, resolved at the door exactly like holdsRole.
    // -----------------------------------------------------------------------

    /// An ES256 token with NO scope claim gets its scopes FROM THE MODEL.
    #[test]
    fn es256_scopes_come_from_the_model() {
        let jwks = jwks_json(&css_key(), KID);
        let v = OidcVerifier::new(
            ISSUER,
            || Some(allow()),
            || Some(roles()),
            || Some(vec![(wren_webid(), vec!["urn:chorus:domains:tests".to_string()])]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        let c = v.verify(&token_valid(), NOW).expect("verifies");
        assert_eq!(c.scope, vec!["urn:chorus:domains:tests"],
            "scope resolved from chorus:hasScope, not from any claim");
    }

    /// A Principal with no hasScope edge gets NO scopes — it can authenticate
    /// but a scoped write refuses. Absence is absence, never a default.
    #[test]
    fn principal_without_has_scope_carries_no_scope() {
        let jwks = jwks_json(&css_key(), KID);
        let v = OidcVerifier::new(
            ISSUER, || Some(allow()), || Some(roles()),
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        let c = v.verify(&token_valid(), NOW).expect("verifies");
        assert!(c.scope.is_empty(), "no edge ⇒ no scope, not a default");
    }

    /// Scope revocation is a model edit and lands within one TTL, no restart —
    /// the same drill as the allow-set and holdsRole.
    #[test]
    fn scope_revocation_lands_within_one_ttl() {
        let jwks = jwks_json(&css_key(), KID);
        let revoked = Rc::new(Cell::new(false));
        let rc = Rc::clone(&revoked);
        let v = OidcVerifier::new(
            ISSUER, || Some(allow()), || Some(roles()),
            move || Some(if rc.get() { vec![] } else {
                vec![(wren_webid(), vec!["urn:chorus:ontology".to_string()])] }),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        assert!(!v.verify(&token_valid(), NOW).unwrap().scope.is_empty());
        revoked.set(true);
        assert!(v.verify(&token_valid(), NOW + ALLOW_TTL_SECS + 1).unwrap().scope.is_empty(),
            "past one TTL the revocation is live");
    }

    /// Graph unreachable ⇒ NO scopes (fail closed), matching allowed()/role_for().
    #[test]
    fn scope_map_fails_closed_when_graph_unreachable() {
        let jwks = jwks_json(&css_key(), KID);
        let up = Rc::new(Cell::new(true));
        let uc = Rc::clone(&up);
        let v = OidcVerifier::new(
            ISSUER, || Some(allow()), || Some(roles()),
            move || if uc.get() { Some(vec![(wren_webid(), vec!["urn:chorus:ontology".to_string()])]) } else { None },
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        assert!(!v.verify(&token_valid(), NOW).unwrap().scope.is_empty());
        up.set(false);
        assert!(v.verify(&token_valid(), NOW + ALLOW_TTL_SECS + 1).unwrap().scope.is_empty(),
            "unreachable graph ⇒ no scopes, never stale grants");
    }

    /// The query asks the hasScope edge in the security graph; unreachable is
    /// distinct from empty; rows without a separator are dropped and said.
    #[test]
    fn principal_scope_query_asks_the_has_scope_edge() {
        let body = format!(
            r#"{{"head":{{"vars":["v"]}},"results":{{"bindings":[{{"v":{{"type":"literal","value":"{} urn:chorus:domains:tests"}}}},{{"v":{{"type":"literal","value":"{} urn:chorus:ontology"}}}}]}}}}"#,
            wren_webid(), wren_webid()
        );
        let got = resolve_principal_scopes(|q| {
            assert!(q.contains("chorus:hasScope"), "asks the hasScope edge");
            assert!(q.contains("urn:chorus:domains:security"), "scoped to the security graph");
            Some(body.clone())
        });
        let m = got.expect("reachable");
        assert_eq!(m, vec![(wren_webid(), vec!["urn:chorus:domains:tests".to_string(), "urn:chorus:ontology".to_string()])]);
        assert_eq!(resolve_principal_scopes(|_| None), None, "unreachable is DISTINCT from empty");
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
