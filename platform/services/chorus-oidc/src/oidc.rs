//! #3613 / ADR-052 — Solid-OIDC (CSS) identity at the athena-make write seam.
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

use crate::auth::{self, AuthError, Claims};
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use std::sync::Mutex;
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

/// #4196 — webId → the Principal's local name (`principal-silas` → `silas`),
/// resolved from the same allow-set graph. This is WHO the caller is, as a
/// name the door can stamp on a row and compare to a row's owner. Jeff,
/// 2026-09-16 17:16: a row's owner is a principal, a user — not a role. Before
/// this the door only knew the caller's HAT (`agent_id`, from holdsRole), so a
/// user with a permission and no hat could not own or write anything.
struct PrincipalState {
    pairs: Vec<(String, String)>,
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
    resolve_allow: Box<dyn Fn() -> Option<Vec<String>> + Send + Sync>,
    /// webId → role resolver over `chorus:holdsRole` (ADR-054 §3.3). Same
    /// None-vs-Some(empty) split as the allow-set.
    resolve_roles: Box<dyn Fn() -> Option<Vec<(String, String)>> + Send + Sync>,
    /// webId → scopes resolver over `chorus:hasScope` (#3689). Same split.
    resolve_scopes: Box<dyn Fn() -> Option<Vec<(String, Vec<String>)>> + Send + Sync>,
    fetch: Box<dyn Fn() -> Option<String> + Send + Sync>,
    /// #4196 — webId → principal local name. Opt-in via `with_principal_names`
    /// so the eighteen existing constructors keep their shape; absent, the
    /// door falls back to the hat (`agent_id`), which is the pre-#4196 world.
    resolve_principals: Option<Box<dyn Fn() -> Option<Vec<(String, String)>> + Send + Sync>>,
    state: Mutex<JwksState>,
    allow: Mutex<AllowState>,
    roles: Mutex<RoleState>,
    scopes: Mutex<ScopeState>,
    principals: Mutex<PrincipalState>,
}

impl OidcVerifier {
    pub fn new(
        issuer: &str,
        resolve_allow: impl Fn() -> Option<Vec<String>> + Send + Sync + 'static,
        resolve_roles: impl Fn() -> Option<Vec<(String, String)>> + Send + Sync + 'static,
        resolve_scopes: impl Fn() -> Option<Vec<(String, Vec<String>)>> + Send + Sync + 'static,
        fetch: impl Fn() -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            issuer: norm_iss(issuer),
            resolve_allow: Box::new(resolve_allow),
            resolve_roles: Box::new(resolve_roles),
            resolve_scopes: Box::new(resolve_scopes),
            fetch: Box::new(fetch),
            resolve_principals: None,
            state: Mutex::new(JwksState { keys: HashMap::new(), last_attempt: 0 }),
            allow: Mutex::new(AllowState { webids: Vec::new(), fetched_at: 0, last_attempt: 0 }),
            roles: Mutex::new(RoleState { pairs: Vec::new(), fetched_at: 0, last_attempt: 0 }),
            scopes: Mutex::new(ScopeState { grants: Vec::new(), fetched_at: 0, last_attempt: 0 }),
            principals: Mutex::new(PrincipalState { pairs: Vec::new(), fetched_at: 0, last_attempt: 0 }),
        }
    }

    /// #4196 — wire the webId → principal-name resolver (prod: the model query;
    /// tests: a stub). Same None-vs-Some(empty) split as the other three.
    pub fn with_principal_names(
        mut self,
        resolve: impl Fn() -> Option<Vec<(String, String)>> + Send + Sync + 'static,
    ) -> Self {
        self.resolve_principals = Some(Box::new(resolve));
        self
    }

    /// #4196 — the caller's NAME as a Principal (`silas`, `jeff`, `crawler`),
    /// asked of the graph on the ALLOW_TTL cadence. None when no resolver is
    /// wired, the graph is unreachable, or the WebID owns no Principal; the
    /// caller decides the fail-closed posture. Never parsed out of the WebID.
    pub fn principal_for(&self, web_id: &str, now_secs: u64) -> Option<String> {
        let resolve = self.resolve_principals.as_ref()?;
        let mut pl = self.principals.lock().unwrap_or_else(|e| e.into_inner());
        // never fetched (fetched_at == 0) is stale by definition, not fresh
        let stale = pl.fetched_at == 0 || now_secs.saturating_sub(pl.fetched_at) >= ALLOW_TTL_SECS;
        let can_retry = now_secs.saturating_sub(pl.last_attempt) >= ALLOW_RETRY_COOLDOWN_SECS
            || pl.last_attempt == 0;
        if stale && can_retry {
            pl.last_attempt = now_secs;
            match resolve() {
                Some(v) => {
                    pl.pairs = v;
                    pl.fetched_at = now_secs;
                }
                None => pl.pairs.clear(),
            }
        }
        pl.pairs.iter().find(|(w, _)| w == web_id).map(|(_, p)| p.clone())
    }

    /// Boot-prime the allow-set (same posture as warm_fetch: loud on failure,
    /// never boot-blocking). Returns how many Principal webids were cached.
    pub fn warm_allow(&self, now_secs: u64) -> usize {
        let mut al = self.allow.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut al = self.allow.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut rl = self.roles.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut rl = self.roles.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut sc = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
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
            let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
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

/// THE one verify entry the seam calls for every token. The header `alg`
/// (untrusted) is read only to produce a PRECISE refusal for legacy tokens —
/// there is one verify path: ES256 against the CSS JWKS (#3689/#3719).
pub fn verify_any(
    token: &str,
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
    // #3689 — THE CUTOVER (completed by #3719: the envelope door + sdk/mcp
    // minters migrated too, KeyRegistry deleted). One verify path, one key
    // model: ES256 identity via chorus-identity-token; scope is model data
    // (chorus:hasScope). A stale HS256 token — or alg=none, or anything that
    // is not ES256 — gets a TYPED refusal naming the retirement, never a
    // silent accept and never a fallback.
    match alg.as_deref() {
        Some("ES256") => oidc.verify(t, now_secs),
        Some("HS256") => Err(AuthError::Hs256Retired),
        _ => Err(AuthError::UnknownAlg),
    }
}

/// The GET-seam gate (None = proceed; Some((code, body)) = short-circuit),
/// 401/403 split; every token goes through verify_any. (auth::seam_auth, the
/// HS256 reference it once mirrored, died with the machinery — #3719.)
pub fn seam_auth_any(
    path: &str,
    authorization: &str,
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
    match verify_any(token, oidc, now_secs) {
        Ok(_) => None,
        Err(AuthError::WebIdNotAllowed) => {
            Some((403, auth::err_body("forbidden", &AuthError::WebIdNotAllowed)))
        }
        Err(e) => Some((401, auth::err_body("unauthorized", &e))),
    }
}

/// #3785 — THE ONE NAME. The graph the doors read their allow-set from, declared
/// once and resolved by every reader, because on 2026-08-06 it was spelled by
/// hand in twelve places across Rust, TypeScript, shell and the ontology files.
///
/// That day the ten Principal records were consolidated into the graph athena-make
/// SERVES from, and the copies in the graph the doors READ were retired as
/// duplicates. They were not duplicates — they were the same records with two
/// consumers. Jeff was locked out of the Clearing within minutes, and the
/// governed writer refused the operator who had just deleted his own WebID.
/// Nothing anywhere announced which graph it was reading, so the split was
/// invisible right up until it wasn't.
///
/// Overridable by env so the graph can be moved by configuration rather than by
/// a coordinated edit across four languages — which is the move that caused the
/// incident. `graph_provenance()` exists so a door can SAY where it read.
pub fn allow_set_graph() -> String {
    if let Some(g) = RESOLVED_ALLOW_GRAPH.get() {
        return g.clone();
    }
    std::env::var("CHORUS_ALLOW_SET_GRAPH")
        .unwrap_or_else(|_| "urn:chorus:domains:security".to_string())
}

/// #4220 — WHERE PRINCIPALS LIVE IS A MODEL FACT, NOT A CONFIG FACT.
///
/// On 2026-09-19 I moved the twelve Principal rows into the identity graph,
/// which is where PrincipalShape now says they live and where the only route
/// serving them already pointed. Every authenticated write in the system
/// started answering "authn-missing" within a minute: this resolver was still
/// reading the security graph, so no WebID resolved to a Principal and the door
/// refused everyone. I rolled the rows back.
///
/// The comment above records the SAME incident from 2026-08-06 and prescribes
/// "moved by configuration rather than a coordinated edit across four
/// languages". Configuration is better than four edits and still wrong: it is
/// three env vars in three deploys that must change in the same instant, and
/// whichever one lags locks everybody out. The model already states the answer
/// — PrincipalShape's chorus:instancesGraph — so the door asks it once at boot
/// and every reader here follows.
///
/// Unreadable model → the default stands. An empty answer must never widen to
/// "any graph" or narrow to none; it means keep reading where we read before.
static RESOLVED_ALLOW_GRAPH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub const PRINCIPAL_HOME_QUERY: &str = "PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?g WHERE { GRAPH <urn:chorus:ontology> { ?shape sh:targetClass chorus:Principal ; chorus:instancesGraph ?g } } LIMIT 1";

/// Pure: pick the home from what the model answered. `rows` is the select's
/// values; anything unusable leaves the current answer alone.
pub fn principal_home_from(rows: &[String], current: &str) -> String {
    match rows.iter().find(|r| r.starts_with("urn:chorus:")) {
        Some(g) => g.clone(),
        None => current.to_string(),
    }
}

/// Called once at boot, before the first token is verified. Returns the graph
/// every reader in this process will use, and says so to the log.
pub fn prime_allow_set_graph(query: impl Fn(&str) -> Option<String>) -> String {
    let current = std::env::var("CHORUS_ALLOW_SET_GRAPH")
        .unwrap_or_else(|_| "urn:chorus:domains:security".to_string());
    let resolved = match query(PRINCIPAL_HOME_QUERY) {
        Some(body) => principal_home_from(&crate::select_v(&body), &current),
        None => current.clone(),
    };
    let _ = RESOLVED_ALLOW_GRAPH.set(resolved.clone());
    resolved
}

/// What a door prints at startup and on every allow-set refresh. The missing
/// line of 2026-08-06: two consumers on two graphs, and no surface named its
/// source, so the only way to discover the split was to break it.
pub fn graph_provenance(count: usize, reason: &str) -> String {
    format!(
        "allow-set: resolved {} principal webid(s) from <{}> ({})",
        count,
        allow_set_graph(),
        reason
    )
}

/// ADR-052 §5 — resolve the Principal allow-set from the model at boot (one
/// query, no per-request graph call: the #3406 freeze-class stays killed).
/// `query` is injected (prod: sparql_json against Fuseki). Empty/unreachable ⇒
/// empty allow-set ⇒ every ES256 token is WebIdNotAllowed (fail-closed) while
/// the HS256 dual path keeps existing writers alive — ADR-052 §8's "no interim
/// weakening" in both directions.
/// The variable is `?v` because select_v (the DAL's proven single-var
/// extractor) parses exactly that seam.
pub fn principal_allow_query() -> String {
    format!("PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE {{ GRAPH <{}> {{ ?p a chorus:Principal ; chorus:webId ?v }} }}", allow_set_graph())
}

/// None = graph unreachable (caller decides the fail-closed posture);
/// Some(empty) = reachable and genuinely nobody allowed.
pub fn resolve_principal_webids(query: impl Fn(&str) -> Option<String>) -> Option<Vec<String>> {
    query(&principal_allow_query()).map(|body| crate::select_v(&body))
}

/// #4196 — webId → the Principal's LOCAL NAME, one `?v` row per principal as
/// `"<webid> <name>"` (a WebID carries no space, so the first one separates).
/// The name is the IRI's local part minus the `principal-` prefix the mint
/// adds (ADR-040), so it is the same token `chorus-identity-token <name>`
/// mints for and the same string the door stamps as a row's owner.
pub fn principal_name_query() -> String {
    format!(
        "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE {{ GRAPH <{}> {{ ?p a chorus:Principal ; chorus:webId ?w }} BIND(CONCAT(STR(?w), \" \", REPLACE(REPLACE(STR(?p), \".*[#/]\", \"\"), \"^principal-\", \"\")) AS ?v) }}",
        allow_set_graph()
    )
}

/// None = graph unreachable (caller fails closed); Some(empty) = no principals.
/// Rows without a separator are dropped and said, like the role resolver.
pub fn resolve_principal_names(
    query: impl Fn(&str) -> Option<String>,
) -> Option<Vec<(String, String)>> {
    query(&principal_name_query()).map(|body| {
        let mut pairs: Vec<(String, String)> = Vec::new();
        for row in crate::select_v(&body) {
            let Some((w, n)) = row.split_once(' ') else {
                eprintln!("chorus-oidc: WARNING — unreadable principal-name row {:?}; that principal has no name at the door until fixed (#4196)", row);
                continue;
            };
            if w.is_empty() || n.is_empty() { continue; }
            pairs.push((w.to_string(), n.to_string()));
        }
        pairs
    })
}

/// ADR-054 §3.3 — resolve webId→role from `chorus:holdsRole`, the edge that
/// makes role assignment GOVERNED DATA rather than a WebID naming convention.
/// Emitted as one `?v` row per edge (`"<webid> <role-iri>"`) so the DAL's proven
/// single-var extractor parses it — a WebID can carry no space, so the first
/// one is an unambiguous separator. The role IRI travels WHOLE: naming it here
/// would trade the WebID convention this card retires for an IRI convention.
/// Principals with no `holdsRole` are simply absent: allowed to authenticate,
/// holding no role.
pub fn principal_role_query() -> String {
    format!("PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE {{ GRAPH <{}> {{ ?p a chorus:Principal ; chorus:webId ?w ; chorus:holdsRole ?r }} BIND(CONCAT(STR(?w), \" \", STR(?r)) AS ?v) }}", allow_set_graph())
}

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
    query(&principal_role_query()).map(|body| {
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
///
/// #3728 — ONE source of truth. The query text is bound here via `include_str!`
/// from `platform/api/src/sparql/principal-scope.rq`, the SAME file the
/// chorus-api TS door reads at runtime. Two doors, one query, cannot drift; a
/// move or delete of that file breaks THIS build loud (compile-time embed).
/// The file lives in the api tree because that is the one package whose build
/// copies `src/sparql → dist/sparql`, giving the TS door a runtime-resolvable
/// path; Rust binds from anywhere at compile.
pub const PRINCIPAL_SCOPE_QUERY_TEMPLATE: &str =
    include_str!("../../../api/src/sparql/principal-scope.rq");

/// #3785 — the scope query resolves the SAME one name. It ships as a .rq file so
/// the TypeScript door can read it at runtime, so the graph is substituted here
/// rather than templated in the file: a .rq with a placeholder would be invalid
/// SPARQL and could not be validated by anything that parses it.
/// #4224 — PRINCIPALS AND PERMISSIONS DO NOT SHARE A HOME.
///
/// The template used to hold both classes inside ONE `GRAPH` clause, and this
/// function swapped that one graph for the resolved principal home. On
/// 2026-09-19 the twelve Principal rows moved to `urn:chorus:domains:identity`
/// while the 46 Permission rows stayed in `urn:chorus:domains:security`: the
/// join then asked identity for Permissions, found none, and every governed
/// write in the system refused for four minutes (427 failed crawler writes).
///
/// The template now names the two graphs separately. Permissions are read from
/// the security graph, which is where PermissionShape says they live. Principals
/// are read from `PRINCIPAL_HOME_MARKER`, a graph that exists nowhere, and this
/// function is the only thing that turns it into a real one. An unsubstituted
/// marker therefore resolves zero grants and the door fails closed — the safe
/// direction, and the one a reader can see in the query text.
pub const PRINCIPAL_HOME_MARKER: &str = "urn:chorus:principal-home";

pub fn principal_scope_query() -> String {
    principal_scope_query_for(&allow_set_graph())
}

/// The substitution as a pure function of the home, so a test can ask what the
/// query looks like for a home that is NOT the default without touching the
/// process-wide resolved graph.
pub fn principal_scope_query_for(home: &str) -> String {
    PRINCIPAL_SCOPE_QUERY_TEMPLATE.replace(PRINCIPAL_HOME_MARKER, home)
}

/// None = graph unreachable (caller fails closed); Some(empty) = reachable and
/// no grants exist. Rows without a separator are dropped and said.
pub fn resolve_principal_scopes(
    query: impl Fn(&str) -> Option<String>,
) -> Option<Vec<(String, Vec<String>)>> {
    query(&principal_scope_query()).map(|body| {
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

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
        let r = seam_auth_any("/schema/domain", "", &v, NOW, &["/schema/domain".to_string()]);
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
        let up = Arc::new(AtomicBool::new(true));
        let up_c = up.clone();
        let jwks = jwks_json(&css_key(), KID);
        let v = OidcVerifier::new(ISSUER, || Some(allow()), || Some(roles()), || Some(vec![]), move || {
            if up_c.load(Ordering::SeqCst) { Some(jwks.clone()) } else { None }
        });
        v.warm_allow(NOW);
        assert_eq!(v.warm_fetch(NOW), 1, "boot warm-fetch caches the CSS key");
        up.store(false, Ordering::SeqCst); // CSS blips
        let c = v.verify(&token_valid(), NOW + 60).expect("cached kid verifies through the blip");
        assert_eq!(c.web_id, wren_webid());
    }

    // cases 7+8 boundary — unknown kid during the blip stays fail-closed even
    // though ANOTHER kid is cached: we fail only with NO usable key for THIS
    // token, and we don't hammer CSS inside the cooldown window.
    #[test]
    fn unknown_kid_during_blip_fails_closed() {
        let jwks = jwks_json(&css_key(), KID);
        let calls = Arc::new(AtomicU32::new(0));
        let calls_c = calls.clone();
        let v = OidcVerifier::new(ISSUER, || Some(allow()), || Some(roles()), || Some(vec![]), move || {
            calls_c.fetch_add(1, Ordering::SeqCst);
            Some(jwks.clone())
        });
        v.warm_fetch(NOW);
        v.warm_allow(NOW);
        let rotated = mint_es256(&css_key(), "rotated-kid", &payload(ISSUER, "chorus", &wren_webid(), NOW + 3600));
        // inside the cooldown: no refetch, fail closed
        assert_eq!(v.verify(&rotated, NOW + 5), Err(AuthError::JwksUnreachable));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "cooldown suppressed the refetch");
        // after the cooldown: refetch happens (rotation pickup path) — the stub
        // still lacks the kid, so it refuses as an unpublished key.
        assert_eq!(v.verify(&rotated, NOW + JWKS_FETCH_COOLDOWN_SECS + 1), Err(AuthError::BadSignature));
        assert_eq!(calls.load(Ordering::SeqCst), 2, "post-cooldown verify refetched the JWKS");
    }

    // case 9 — #3689 CUTOVER: the deletion this test's predecessor promised.
    // (hs256_legacy_allows said "DELETE this test when the last HS256 writer
    // migrates — its deletion asserts the cutover." This is that assertion.)
    // A validly-signed HS256 token is REFUSED with the typed retirement error.
    #[test]
    fn hs256_is_refused_with_a_typed_error() {
        let secret: &[u8] = b"test-chorus-service-token-secret";
        let hs = crate::auth::mint_hs256_for_tests(
            secret,
            &format!(r#"{{"agentId":"wren","webId":"{}","aud":"chorus","exp":{}}}"#, wren_webid(), NOW + 3600),
        );
        let v = verifier();
        assert_eq!(verify_any(&hs, &v, NOW), Err(AuthError::Hs256Retired),
            "a valid HS256 signature is refused BY POLICY, with the reason named");
        // ES256 still verifies through the same single entry.
        assert!(verify_any(&token_valid(), &v, NOW).is_ok());
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
        let r = seam_auth_any(
            "/schema/domain",
            &format!("Bearer {}", token_valid()),
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
        let revoked = Arc::new(AtomicBool::new(false));
        let revoked_c = revoked.clone();
        let jwks = jwks_json(&css_key(), KID);
        let v = OidcVerifier::new(
            ISSUER,
            move || Some(if revoked_c.load(Ordering::SeqCst) { vec![] } else { vec![wren_webid(), silas_webid()] }),
            || Some(roles()),
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        assert!(v.verify(&token_valid(), NOW).is_ok(), "pre-revocation: verifies");
        revoked.store(true, Ordering::SeqCst); // the model edit: Principal dropped
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
        let up = Arc::new(AtomicBool::new(true));
        let up_c = up.clone();
        let jwks = jwks_json(&css_key(), KID);
        let v = OidcVerifier::new(
            ISSUER,
            move || if up_c.load(Ordering::SeqCst) { Some(vec![wren_webid()]) } else { None },
            || Some(roles()),
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        assert!(v.verify(&token_valid(), NOW).is_ok());
        up.store(false, Ordering::SeqCst); // graph goes unreachable
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
        let v = verifier();
        assert!(verify_any(&t, &v, NOW).is_err(), "alg=none must never verify");
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
        let reassigned = Arc::new(AtomicBool::new(false));
        let rc = Arc::clone(&reassigned);
        let v = OidcVerifier::new(
            ISSUER,
            || Some(allow()),
            move || {
                Some(vec![(
                    wren_webid(),
                    if rc.load(Ordering::SeqCst) { "kade".to_string() } else { "wren".to_string() },
                )])
            },
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        v.warm_roles(NOW);
        assert_eq!(v.verify(&token_valid(), NOW).unwrap().agent_id, "wren");
        reassigned.store(true, Ordering::SeqCst); // the model edit
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
        let up = Arc::new(AtomicBool::new(true));
        let uc = Arc::clone(&up);
        let v = OidcVerifier::new(
            ISSUER,
            || Some(allow()),
            move || if uc.load(Ordering::SeqCst) { Some(roles()) } else { None },
            || Some(vec![]),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        v.warm_roles(NOW);
        assert_eq!(v.verify(&token_valid(), NOW).unwrap().agent_id, "wren");
        up.store(false, Ordering::SeqCst);
        assert_eq!(
            v.verify(&token_valid(), NOW + ALLOW_TTL_SECS + 1).unwrap().agent_id,
            "",
            "graph unreachable ⇒ no role rather than a stale or guessed one"
        );
    }

    /// The query asks the holdsRole EDGE in the security graph, and unreachable
    /// stays distinct from empty.
    // #4196 — the caller's NAME as a Principal comes from the graph, not the WebID.
    #[test]
    fn principal_names_resolve_from_rows_and_a_missing_resolver_yields_no_name() {
        let body = r#"{"results":{"bindings":[
            {"v":{"value":"https://id.example/silas/profile/card#me silas"}},
            {"v":{"value":"https://id.example/jeff/profile/card#me jeff"}},
            {"v":{"value":"unreadable-row-with-no-space"}}
        ]}}"#;
        let pairs = resolve_principal_names(|_| Some(body.to_string())).unwrap();
        assert_eq!(pairs.len(), 2, "the unreadable row is dropped, not guessed");
        assert_eq!(pairs[1], ("https://id.example/jeff/profile/card#me".to_string(), "jeff".to_string()));
        // NEGATIVE: unreachable graph → None (caller fails closed), not an empty list
        assert!(resolve_principal_names(|_| None).is_none());
        // NEGATIVE: a verifier with no resolver wired names nobody
        let v = OidcVerifier::new("https://id.example/", || Some(vec![]), || Some(vec![]), || Some(vec![]), || None);
        assert_eq!(v.principal_for("https://id.example/jeff/profile/card#me", 0), None);
        let v = v.with_principal_names(move || resolve_principal_names(|_| Some(body.to_string())));
        assert_eq!(v.principal_for("https://id.example/jeff/profile/card#me", 0).as_deref(), Some("jeff"));
        assert_eq!(v.principal_for("https://id.example/nobody/profile/card#me", 0), None);
    }

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
        let revoked = Arc::new(AtomicBool::new(false));
        let rc = Arc::clone(&revoked);
        let v = OidcVerifier::new(
            ISSUER, || Some(allow()), || Some(roles()),
            move || Some(if rc.load(Ordering::SeqCst) { vec![] } else {
                vec![(wren_webid(), vec!["urn:chorus:ontology".to_string()])] }),
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        assert!(!v.verify(&token_valid(), NOW).unwrap().scope.is_empty());
        revoked.store(true, Ordering::SeqCst);
        assert!(v.verify(&token_valid(), NOW + ALLOW_TTL_SECS + 1).unwrap().scope.is_empty(),
            "past one TTL the revocation is live");
    }

    /// Graph unreachable ⇒ NO scopes (fail closed), matching allowed()/role_for().
    #[test]
    fn scope_map_fails_closed_when_graph_unreachable() {
        let jwks = jwks_json(&css_key(), KID);
        let up = Arc::new(AtomicBool::new(true));
        let uc = Arc::clone(&up);
        let v = OidcVerifier::new(
            ISSUER, || Some(allow()), || Some(roles()),
            move || if uc.load(Ordering::SeqCst) { Some(vec![(wren_webid(), vec!["urn:chorus:ontology".to_string()])]) } else { None },
            move || Some(jwks.clone()),
        );
        v.warm_allow(NOW);
        assert!(!v.verify(&token_valid(), NOW).unwrap().scope.is_empty());
        up.store(false, Ordering::SeqCst);
        assert!(v.verify(&token_valid(), NOW + ALLOW_TTL_SECS + 1).unwrap().scope.is_empty(),
            "unreachable graph ⇒ no scopes, never stale grants");
    }

    /// The query asks the hasScope edge in the security graph; unreachable is
    /// distinct from empty; rows without a separator are dropped and said.
    #[test]
    fn principal_scope_query_asks_the_permission_rows_not_the_has_scope_edge() {
        let body = format!(
            r#"{{"head":{{"vars":["v"]}},"results":{{"bindings":[{{"v":{{"type":"literal","value":"{} urn:chorus:domains:tests"}}}},{{"v":{{"type":"literal","value":"{} urn:chorus:ontology"}}}}]}}}}"#,
            wren_webid(), wren_webid()
        );
        let got = resolve_principal_scopes(|q| {
            // #4183 — a held scope is a Permission row (agent + accessTo + mode
            // acl:Write); the hasScope literal is retired and must never be asked.
            assert!(q.contains("chorus:Permission") && q.contains("chorus:accessTo"), "asks the Permission rows");
            assert!(!q.contains("chorus:hasScope"), "never asks the retired hasScope edge");
            assert!(q.contains("urn:chorus:domains:security"), "scoped to the security graph");
            Some(body.clone())
        });
        let m = got.expect("reachable");
        assert_eq!(m, vec![(wren_webid(), vec!["urn:chorus:domains:tests".to_string(), "urn:chorus:ontology".to_string()])]);
        assert_eq!(resolve_principal_scopes(|_| None), None, "unreachable is DISTINCT from empty");
    }

    // -----------------------------------------------------------------------
    // #4224 — the scope query reads TWO graphs. Permissions from the security
    // graph, Principals from wherever the model says Principals live. On
    // 2026-09-19 those were the same graph, then they weren't, and one clause
    // holding both classes took every governed write down.
    // -----------------------------------------------------------------------

    /// NEGATIVE PROOF. The fixture is the state that broke prod: a principal
    /// home that is NOT the security graph. Collapse the two clauses back into
    /// one — substitute the security literal as the old code did, or move the
    /// Permission triples under the home clause — and this goes red.
    #[test]
    fn permissions_read_from_security_and_principals_from_a_different_home() {
        let home = "urn:chorus:domains:identity";
        let q = principal_scope_query_for(home);
        assert_ne!(home, "urn:chorus:domains:security", "the fixture must be the split case");

        let sec = q
            .find("GRAPH <urn:chorus:domains:security>")
            .expect("permissions are read from the security graph");
        let hom = q
            .find(&format!("GRAPH <{home}>"))
            .expect("principals are read from the resolved home");
        assert_ne!(sec, hom, "two graph clauses, not one");

        // Each class sits under its own graph: the Permission triples appear
        // between the security clause and the home clause, the Principal
        // triples after the home clause. Merging them moves one of these.
        let perm = q.find("chorus:Permission").expect("asks Permission");
        let prin = q.find("chorus:Principal").expect("asks Principal");
        assert!(sec < perm && perm < hom, "Permission belongs to the security clause");
        assert!(hom < prin, "Principal belongs to the home clause");
    }

    /// NEGATIVE PROOF. The marker must resolve nowhere until a door substitutes
    /// it. If someone "fixes" the template by writing a real graph in place of
    /// the marker, an un-primed door silently reads that graph instead of
    /// failing closed — so the template carrying the marker is itself the check.
    #[test]
    fn the_unsubstituted_marker_is_not_a_real_graph_and_never_survives() {
        assert!(
            PRINCIPAL_SCOPE_QUERY_TEMPLATE.contains(PRINCIPAL_HOME_MARKER),
            "the shipped template names the marker, not a graph"
        );
        assert!(
            !PRINCIPAL_HOME_MARKER.starts_with("urn:chorus:domains:"),
            "the marker must not look like a domain graph that could hold rows"
        );
        for home in ["urn:chorus:domains:security", "urn:chorus:domains:identity"] {
            let q = principal_scope_query_for(home);
            assert!(!q.contains(PRINCIPAL_HOME_MARKER), "substitution leaves no marker for {home}");
        }
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

#[cfg(test)]
mod allow_graph_from_model_4220 {
    use super::*;

    #[test]
    fn the_model_answer_wins() {
        let rows = vec!["urn:chorus:domains:identity".to_string()];
        assert_eq!(principal_home_from(&rows, "urn:chorus:domains:security"), "urn:chorus:domains:identity");
    }

    #[test]
    fn an_unreadable_model_leaves_the_door_where_it_was() {
        // NEGATIVE PROOF of the failure that locked every write on 2026-09-19:
        // an empty or unusable answer must NOT move the door, and must not
        // resolve to nothing.
        let none: Vec<String> = vec![];
        assert_eq!(principal_home_from(&none, "urn:chorus:domains:security"), "urn:chorus:domains:security");
        let junk = vec!["".to_string(), "not-a-graph".to_string()];
        assert_eq!(principal_home_from(&junk, "urn:chorus:domains:security"), "urn:chorus:domains:security");
    }

    #[test]
    fn priming_from_a_dead_store_keeps_the_default() {
        let g = prime_allow_set_graph(|_| None);
        assert_eq!(g, "urn:chorus:domains:security");
        // and every reader agrees with what was primed
        assert_eq!(allow_set_graph(), g);
    }
}
