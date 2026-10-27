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
pub mod client;

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

/// #4515 (Jeff 2026-10-14) — rows are read and written with one family of
/// tools named after the door's own URL shape, /v1/<domain>/<class>[/<id>]:
/// (name, HTTP method, needs an id, carries fields, what it does).
pub const ATHENA_TOOLS: [(&str, &str, bool, bool, &str); 5] = [
    ("athena-get", "GET", true, false, "Get one row, by id, from a class the model door serves (GET /v1/<domain>/<class>/<id>)."),
    ("athena-list", "GET", false, false, "List rows of a class the model door serves (GET /v1/<domain>/<class>)."),
    ("athena-add", "POST", false, true, "Add a row to a class the model door serves, as the caller (POST /v1/<domain>/<class>)."),
    ("athena-set", "PATCH", true, true, "Change fields of one row, as the caller (PATCH /v1/<domain>/<class>/<id>)."),
    // #4762 — a retired row leaves the store in the same change (Jeff 2026-10-27)
    ("athena-delete", "DELETE", true, false, "Delete one row, as the caller; the door refuses a caller who does not own it (DELETE /v1/<domain>/<class>/<id>)."),
];

/// The athena tools, over exactly the collections the door serves: the
/// served list is named in each description, and a call is checked against
/// it, so a class the door adds is callable on the next refresh and one it
/// drops is gone. One table, one constructor.
pub fn athena_tools(collections: &[String]) -> Vec<ToolSpec> {
    let served = collections.join(", ");
    ATHENA_TOOLS
        .iter()
        .map(|(name, _, needs_id, has_fields, what)| {
            let mut props = json!({
                "domain": { "type": "string", "pattern": "^[a-z][a-z0-9-]*$", "description": "The domain, as in /v1/<domain>/<class>" },
                "class": { "type": "string", "pattern": "^[a-z][a-z0-9-]*$", "description": "The class collection, as in /v1/<domain>/<class>" }
            });
            let mut required = vec!["domain", "class"];
            if *needs_id {
                props["id"] = json!({ "type": "string", "pattern": "^[a-z0-9][a-z0-9-]*$", "description": "The row's id, as the door lists it" });
                required.push("id");
            }
            if *has_fields {
                props["fields"] = json!({ "type": "object", "description": "The row's fields, as a JSON object" });
                required.push("fields");
            }
            if *name == "athena-list" {
                props["limit"] = json!({ "type": "integer", "minimum": 1, "maximum": 500, "description": "At most this many rows (default 50)" });
            }
            let schema = json!({ "type": "object", "properties": props, "required": required });
            ToolSpec {
                name,
                description: format!("{what} The door serves {} collections: {served}", collections.len()),
                input_schema: schema.as_object().cloned().unwrap_or_default(),
            }
        })
        .collect()
}

pub fn is_athena_tool(name: &str) -> bool {
    ATHENA_TOOLS.iter().any(|t| t.0 == name)
}

/// A door call refused before it reaches the door: the reason, and the HTTP
/// status it is reported under.
#[derive(Debug, PartialEq, Eq)]
pub struct BadCall {
    pub reason: String,
    pub status: u16,
}

fn segment_ok(s: &str, first_may_be_digit: bool) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_lowercase() || (first_may_be_digit && c.is_ascii_digit()) => {}
        _ => return false,
    }
    s.len() <= 200 && cs.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// One athena call, decided before anything is sent: (method, door path,
/// JSON body). Domain and class must match ^[a-z][a-z0-9-]*$ and the id
/// ^[a-z0-9][a-z0-9-]*$ — anything else is refused by name, so no path can
/// be smuggled in — and the collection must be one the door serves.
pub fn athena_call(tool: &str, args: &Map<String, Value>, collections: &[String]) -> Result<(String, String, Option<String>), BadCall> {
    let bad = |reason: String| BadCall { reason, status: 400 };
    let Some((_, method, needs_id, has_fields, _)) = ATHENA_TOOLS.iter().find(|t| t.0 == tool) else {
        return Err(BadCall { reason: format!("no such tool: {tool}"), status: 404 });
    };
    let text = |k: &str| -> Result<&str, BadCall> {
        args.get(k).and_then(Value::as_str).ok_or_else(|| bad(format!("{tool}: {k} is required")))
    };
    let domain = text("domain")?;
    if !segment_ok(domain, false) {
        return Err(bad(format!("{tool}: domain {domain:?} is not a domain name (^[a-z][a-z0-9-]*$)")));
    }
    let class = text("class")?;
    if !segment_ok(class, false) {
        return Err(bad(format!("{tool}: class {class:?} is not a class name (^[a-z][a-z0-9-]*$)")));
    }
    let collection = format!("/{domain}/{class}");
    if !collections.iter().any(|c| *c == collection) {
        return Err(bad(format!("{tool}: the door does not serve {collection}; it serves: {}", collections.join(", "))));
    }
    let mut path = format!("/v1{collection}");
    if *needs_id {
        let id = text("id")?;
        if !segment_ok(id, true) {
            return Err(bad(format!("{tool}: id {id:?} is not a row id (^[a-z0-9][a-z0-9-]*$)")));
        }
        path = format!("{path}/{id}");
    }
    if tool == "athena-list" {
        let limit = match args.get("limit") {
            Some(Value::String(s)) => s.trim().parse::<u64>().ok(),
            Some(v) => v.as_u64(),
            None => Some(50),
        }
        .ok_or_else(|| bad(format!("{tool}: limit is not a number")))?;
        path = format!("{path}?limit={}", limit.clamp(1, 500));
    }
    let body = if *has_fields {
        // an object from an MCP caller; a JSON string from `chorus-runner call … fields=`
        let fields = match args.get("fields") {
            Some(Value::Object(m)) => Some(m.clone()),
            Some(Value::String(s)) => serde_json::from_str::<Value>(s).ok().and_then(|v| v.as_object().cloned()),
            _ => None,
        }
        .ok_or_else(|| bad(format!("{tool}: fields is required, as a JSON object")))?;
        if fields.is_empty() {
            return Err(bad(format!("{tool}: fields is empty; there is nothing to write")));
        }
        Some(Value::Object(fields).to_string())
    } else {
        None
    };
    Ok((method.to_string(), path, body))
}

/// The door's base URL, only if it is on this machine: the caller's
/// credential never leaves the box.
pub fn loopback_base(base: &str) -> Result<String, BadCall> {
    let rest = base.strip_prefix("http://").ok_or_else(|| BadCall { reason: format!("the door {base} is not a loopback http URL"), status: 503 })?;
    let host = rest.split(['/', '?']).next().unwrap_or("");
    let host = host.rsplit_once(':').map(|(h, p)| if p.chars().all(|c| c.is_ascii_digit()) { h } else { host }).unwrap_or(host);
    if matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        Ok(base.trim_end_matches('/').to_string())
    } else {
        Err(BadCall { reason: format!("the door {base} is not on this machine; the caller's credential stays on loopback"), status: 503 })
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
