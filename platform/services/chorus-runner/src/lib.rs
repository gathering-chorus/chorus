//! #4512 — the Services tool runner core.
//!
//! One MCP runner, written once (Services product design, #4509): it speaks
//! the 2026-07-28 release and still answers 2025 clients, it knows who is
//! calling from a verified identity token, its tools are generated from what
//! the model door serves, and every failed call leaves one log line naming
//! caller, tool, reason, status and trace.
//!
//! The pure parts live here so they are tested without a network; `main.rs`
//! wires them to rmcp, axum, the door and the store.

pub mod server;
pub mod skills;
pub mod tasks;

use chorus_oidc::auth::AuthError;
use chorus_oidc::oidc::{verify_any, OidcVerifier};
use serde_json::{json, Map, Value};

/// Who is calling: the principal name the model resolves for the token's
/// webId (`wren`, `silas`, `principal-crawler`), never a header the caller
/// typed. Put on the request by the auth gate; read by every tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller(pub String);

/// A refused caller: always 401, always with the reason named.
#[derive(Debug, PartialEq, Eq)]
pub struct Refusal {
    pub reason: &'static str,
}

/// The reason a token was refused, in words a caller can act on.
pub fn refusal_reason(e: &AuthError) -> &'static str {
    match e {
        AuthError::Missing => "no bearer token: mint one with chorus-identity-token",
        AuthError::Hs256Retired => "HS256 tokens are retired: mint an ES256 token with chorus-identity-token",
        AuthError::Malformed => "token is not a well-formed JWT",
        AuthError::BadSignature => "token signature does not verify (forged, tampered, or signed by a key the issuer never published)",
        AuthError::WrongAudience => "token audience is not chorus",
        AuthError::Expired => "token has expired: mint a new one",
        AuthError::WebIdNotAllowed => "token's webId is not a known principal",
        AuthError::UnknownAlg => "token algorithm is not ES256",
        AuthError::IssuerMismatch => "token was not issued by the chorus identity provider",
        AuthError::JwksUnreachable => "the issuer's signing keys could not be fetched, so no token can be checked",
        AuthError::UnknownKey => "signing key unknown: rotated, or never published",
    }
}

/// The bearer value of an Authorization header, if it carries one.
pub fn bearer(header: Option<&str>) -> Option<&str> {
    let h = header?.trim();
    h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer ")).map(str::trim)
}

/// THE gate: every request is verified before any tool code runs. An absent,
/// forged, expired or unknown token is refused with its reason; a verified
/// one yields the caller's principal name (the role name when no principal
/// name resolves, as the door does).
pub fn authenticate(header: Option<&str>, verifier: &OidcVerifier, now_secs: u64) -> Result<Caller, Refusal> {
    let token = bearer(header).unwrap_or("");
    match verify_any(token, verifier, now_secs) {
        Ok(claims) => {
            let name = verifier.principal_for(&claims.web_id, now_secs).unwrap_or(claims.agent_id);
            if name.is_empty() {
                Err(Refusal { reason: refusal_reason(&AuthError::WebIdNotAllowed) })
            } else {
                Ok(Caller(name))
            }
        }
        Err(e) => Err(Refusal { reason: refusal_reason(&e) }),
    }
}

/// The 401 body: the reason, named.
pub fn refusal_body(r: &Refusal) -> String {
    json!({ "error": "authn", "reason": r.reason }).to_string()
}

/// One error line per failed call: caller, tool, reason, status, trace.
pub fn error_line(caller: &str, tool: &str, reason: &str, status: u16, trace: &str) -> String {
    json!({
        "event": "runner.call.failed",
        "caller": caller,
        "tool": tool,
        "reason": reason,
        "status": status,
        "trace": trace,
    })
    .to_string()
}

// ── Door tools, generated from what the door serves ────────────────────────

/// The collections the door serves, from its own discovery document
/// (`GET /`: `{"kind":"Discovery","primitives":[{"kind":"ADR",
/// "collection":"/v1/decisions/adrs","openapi":…},…]}`), as `/<domain>/<collection>`.
/// Nothing here is hand-listed.
pub fn served_collections(discovery: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(discovery).map_err(|e| format!("door discovery is not JSON: {e}"))?;
    let prims = v
        .get("primitives")
        .and_then(Value::as_array)
        .ok_or("door discovery has no primitives list")?;
    let mut out: Vec<String> = prims
        .iter()
        .filter_map(|p| p.get("collection").and_then(Value::as_str))
        .map(|c| c.strip_prefix("/v1").unwrap_or(c).to_string())
        .filter(|c| c.starts_with('/'))
        .collect();
    out.sort();
    out.dedup();
    if out.is_empty() {
        return Err("door serves no collections".into());
    }
    Ok(out)
}

/// A tool the runner offers: name, description and its input schema.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: String,
    pub input_schema: Map<String, Value>,
}

/// The door tools: list and get, over exactly the collections the door
/// serves. The collection is an enum generated from the door's own list, so
/// a class the door adds appears here on the next refresh, and one it drops
/// is gone.
pub fn door_tools(collections: &[String]) -> Vec<ToolSpec> {
    let enum_values: Vec<Value> = collections.iter().map(|c| Value::String(c.clone())).collect();
    let collection = json!({ "type": "string", "enum": enum_values, "description": "A collection the model door serves, as /<domain>/<collection>" });
    let list = json!({
        "type": "object",
        "properties": {
            "collection": collection,
            "limit": { "type": "integer", "minimum": 1, "maximum": 500, "description": "At most this many rows (default 50)" }
        },
        "required": ["collection"]
    });
    let get = json!({
        "type": "object",
        "properties": {
            "collection": collection,
            "name": { "type": "string", "description": "The row's name, as the door lists it" }
        },
        "required": ["collection", "name"]
    });
    vec![
        ToolSpec {
            name: "door_list",
            description: format!("List rows of one of the {} collections the model door serves.", collections.len()),
            input_schema: list.as_object().cloned().unwrap_or_default(),
        },
        ToolSpec {
            name: "door_get",
            description: "Get one row, by name, from a collection the model door serves.".into(),
            input_schema: get.as_object().cloned().unwrap_or_default(),
        },
    ]
}

/// A door call refused before it reaches the door: the reason, and the HTTP
/// status it is reported under.
#[derive(Debug, PartialEq, Eq)]
pub struct BadCall {
    pub reason: String,
    pub status: u16,
}

fn safe_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 200 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The door path a tool call reads, or why the call is refused. Only a
/// collection the door serves, and only a plain row name, ever reach the door.
pub fn door_path(tool: &str, args: &Map<String, Value>, collections: &[String]) -> Result<String, BadCall> {
    let bad = |reason: String| BadCall { reason, status: 400 };
    let collection = args.get("collection").and_then(Value::as_str).ok_or_else(|| bad("collection is required".into()))?;
    if !collections.iter().any(|c| c == collection) {
        return Err(bad(format!("the door does not serve {collection}; it serves: {}", collections.join(", "))));
    }
    match tool {
        "door_list" => {
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50).clamp(1, 500);
            Ok(format!("/v1{collection}?limit={limit}"))
        }
        "door_get" => {
            let name = args.get("name").and_then(Value::as_str).ok_or_else(|| bad("name is required".into()))?;
            if !safe_name(name) {
                return Err(bad(format!("not a row name: {name:?}")));
            }
            Ok(format!("/v1{collection}/{name}"))
        }
        other => Err(BadCall { reason: format!("no such tool: {other}"), status: 404 }),
    }
}

// ── Trace ──────────────────────────────────────────────────────────────────

/// A 32-hex trace id, unique enough for one box: nanoseconds plus a counter.
pub fn new_trace_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let ns = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0) as u64;
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}{:016x}", ns, n ^ (std::process::id() as u64) << 32)
}

/// The W3C traceparent the runner passes on, so the door's log carries the
/// same trace as the runner's (the header the 2026-07-28 spec names).
pub fn traceparent(trace: &str) -> String {
    format!("00-{}-{}-01", trace, &trace[16..32])
}
