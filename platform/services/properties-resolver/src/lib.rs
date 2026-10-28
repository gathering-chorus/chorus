// @domain: value-streams — picked by its owner, Kade, from the crawler's candidates (#4538)
//! properties-resolver — the PURE cascade precedence core (#3437, Properties C).
//!
//! Config-as-data resolution: given a TOTALLY-ORDERED scope chain and a key,
//! decide the effective value. The nearest (most-specific) scope that SETS the
//! key wins; scopes that don't set it are inherited through. This is the
//! "effective config" the projection (Properties A) bakes in.
//!
//! Deliberately hermetic — no Fuseki, no I/O, no graph. The graph-walk that
//! BUILDS the chain (ownership edges from #3433: a node's single-parent
//! Service > Domain > Product > ValueStream/Step) and the athena-make projection
//! (`/properties/effective?node=X&key=Y`) wire on top of this core. Keeping the
//! precedence logic pure makes it unit-testable in isolation and linkable by any
//! consumer (athena-make, werk-test, tagging-lift) without a live graph.
//!
//! Contract pinned with Kade (navigator, 2026-06-16):
//!  - total-ordering is a PRECONDITION — a malformed chain is an error, never a
//!    silent pick (the model question lives in the chain-builder, not here);
//!  - provenance: the winning Property IRI + value_type ride in the Resolution
//!    (audit "why did this resolve?" + coercion without a re-fetch);
//!  - explicit-empty (present, value == "") is an OVERRIDE that wins and stops;
//!    only an ABSENT datum falls through.

/// The structural scope kinds a property can attach to, in cascade order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// #3838 — a ROLE is a scope. Added so a per-role config value (the first
    /// is response.word.cap) can be set as DATA rather than compiled in. Jeff,
    /// 2026-08-12: "if we need to change it in the future, we don't have to
    /// make another code change." This is that one code change; every value
    /// after it is a property edit.
    ///
    /// Ranked MOST specific — above Service — because a role's setting is about
    /// the actor, and an actor's own setting should not be overridden by the
    /// service it happens to be calling.
    Role,
    Service,
    Domain,
    Product,
    ValueStreamStep,
    ValueStream,
}

impl ScopeKind {
    /// Specificity rank — higher wins. Role is most specific; ValueStream is the
    /// broadest default. Encodes Role > Service > Domain > Product >
    /// ValueStreamStep > ValueStream.
    pub fn rank(self) -> u8 {
        match self {
            ScopeKind::Role => 6,
            ScopeKind::Service => 5,
            ScopeKind::Domain => 4,
            ScopeKind::Product => 3,
            ScopeKind::ValueStreamStep => 2,
            ScopeKind::ValueStream => 1,
        }
    }
}

/// One config datum attached to a scope: a `chorus:Property` individual
/// (`iri`) carrying `propertyKey` / `propertyValue` / `propertyValueType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyDatum {
    pub iri: String,
    pub key: String,
    pub value: String,
    pub value_type: String,
}

/// One node in the scope chain: a structural node and the properties it sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeNode {
    pub kind: ScopeKind,
    pub iri: String,
    pub properties: Vec<PropertyDatum>,
}

/// The resolved effective value, with provenance for audit + coercion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub value: String,
    pub value_type: String,
    pub winning_scope_iri: String,
    pub winning_scope_kind: ScopeKind,
    pub winning_property_iri: String,
}

/// The chain violated the total-ordering precondition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CascadeError {
    MalformedChain(String),
}

/// Resolve the effective value for `key` over a totally-ordered `scope_chain`
/// (most-specific first).
///
/// - `Err(MalformedChain)` if the chain is not strictly descending by
///   specificity (duplicate or increasing rank) — the core refuses to guess.
/// - `Ok(Some(Resolution))` for the nearest scope that SETS `key` (present —
///   value may be empty, an explicit override).
/// - `Ok(None)` if no scope in the chain sets `key`.
pub fn decide_effective_value(
    scope_chain: &[ScopeNode],
    key: &str,
) -> Result<Option<Resolution>, CascadeError> {
    // PRECONDITION: strictly descending specificity, no ties. Skipping a level
    // (e.g. service then product, no domain) is allowed; equal or increasing
    // rank is not — that means the chain-builder handed us ambiguity.
    for w in scope_chain.windows(2) {
        let (a, b) = (w[0].kind.rank(), w[1].kind.rank());
        if a <= b {
            return Err(CascadeError::MalformedChain(format!(
                "chain not strictly descending by specificity: {:?}(rank {}) then {:?}(rank {})",
                w[0].kind, a, w[1].kind, b
            )));
        }
    }

    // Nearest-wins: the first scope with a datum PRESENT for `key` wins.
    // Match on presence-of-datum (find by key), never on non-empty value — so an
    // explicit value == "" overrides instead of silently inheriting the parent.
    for node in scope_chain {
        if let Some(d) = node.properties.iter().find(|p| p.key == key) {
            return Ok(Some(Resolution {
                value: d.value.clone(),
                value_type: d.value_type.clone(),
                winning_scope_iri: node.iri.clone(),
                winning_scope_kind: node.kind,
                winning_property_iri: d.iri.clone(),
            }));
        }
    }
    Ok(None)
}

/// #4774 — map `(scope, name)` to the node local-name used by `/effective/:node/:key`.
/// Roles carry the `role-` prefix (`role-borg-queen`), whereas services/domains/products
/// use their bare local-name (`pulse`, `athena`, `chorus`).
pub fn target_node_for_scope(scope: &str, name: &str) -> String {
    if scope == "role" && !name.starts_with("role-") {
        format!("role-{name}")
    } else {
        name.to_string()
    }
}

/// #4774 — extract the `value` field (string, integer, or boolean) from an
/// `/effective/:node/:key` 200 JSON payload. Returns `None` if an `"error"` key
/// is present or `"value"` is absent.
pub fn extract_effective_value_json(body: &str) -> Option<String> {
    if body.contains("\"error\"") {
        return None;
    }
    let marker = "\"value\":";
    let start = body.find(marker)? + marker.len();
    let rest = body[start..].trim_start();
    if let Some(quoted) = rest.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = quoted.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
            } else if c == '"' {
                return Some(out);
            } else {
                out.push(c);
            }
        }
        None
    } else {
        let end = rest.find([',', '}', ' ', '\r', '\n']).unwrap_or(rest.len());
        let raw = rest[..end].trim();
        (!raw.is_empty()).then(|| raw.to_string())
    }
}

/// Canonical port-slot order for known agent roles (#4458 / #4774).
pub const CANONICAL_AGENT_SLOT_ORDER: [&str; 5] = [
    "silas",
    "kade",
    "wren",
    "abby-normal",
    "borg-queen",
];

fn extract_json_str_field(obj: &str, field: &str) -> Option<String> {
    let pat = format!("\"{field}\"");
    let idx = obj.find(&pat)? + pat.len();
    let rest = obj[idx..].trim_start().strip_prefix(':')?.trim_start();
    let quoted = rest.strip_prefix('"')?;
    let end = quoted.find('"')?;
    Some(quoted[..end].to_string())
}

/// #4774 — extract `"roleKind": "agent"` role names from `/v1/roles/roles` JSON.
/// Preserves `CANONICAL_AGENT_SLOT_ORDER` for existing roles so per-role port slots
/// never shift, and appends any newly added agent roles in deterministic order.
/// Returns `None` if zero agent roles are present.
pub fn extract_agent_roles_json(body: &str) -> Option<Vec<String>> {
    let data_idx = body.find("\"data\"")?;
    let mut rest = &body[data_idx..];
    let mut found = Vec::new();
    while let Some(open) = rest.find('{') {
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('}') else {
            break;
        };
        let obj = &after_open[..close];
        if extract_json_str_field(obj, "roleKind").as_deref() == Some("agent") {
            if let Some(name) = extract_json_str_field(obj, "name") {
                if !name.is_empty() && !found.contains(&name) {
                    found.push(name);
                }
            }
        }
        rest = &after_open[close + 1..];
    }
    if found.is_empty() {
        return None;
    }
    let mut ordered = Vec::with_capacity(found.len());
    for slot_role in CANONICAL_AGENT_SLOT_ORDER {
        if found.iter().any(|r| r == slot_role) {
            ordered.push(slot_role.to_string());
        }
    }
    let mut extras: Vec<String> = found
        .into_iter()
        .filter(|r| !CANONICAL_AGENT_SLOT_ORDER.contains(&r.as_str()))
        .collect();
    extras.sort();
    ordered.extend(extras);
    Some(ordered)
}

/// #4774 — zero-dep HTTP GET against `athena-make` (`door_url`, e.g. `http://127.0.0.1:3360`).
pub fn http_get_door(door_url: &str, path: &str) -> Result<(u16, String), String> {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
    use std::time::Duration;

    let stripped = door_url
        .trim()
        .strip_prefix("http://")
        .unwrap_or(door_url.trim())
        .trim_end_matches('/');
    let host_port = if stripped.contains(':') {
        stripped.to_string()
    } else {
        format!("{stripped}:80")
    };
    let addr: SocketAddr = host_port
        .to_socket_addrs()
        .map_err(|e| format!("resolve {host_port}: {e}"))?
        .next()
        .ok_or_else(|| format!("no address for {host_port}"))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map_err(|e| format!("connect {addr}: {e}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host_port}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write {addr}: {e}"))?;
    let mut raw = String::new();
    stream
        .read_to_string(&mut raw)
        .map_err(|e| format!("read {addr}: {e}"))?;
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .or_else(|| raw.split_once("\n\n"))
        .ok_or_else(|| "malformed HTTP response".to_string())?;
    let status_line = head.lines().next().unwrap_or("");
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("invalid HTTP status line: {status_line}"))?;
    Ok((code, body.to_string()))
}

#[cfg(test)]
mod role_scope_3838 {
    use super::*;

    /// #3838 — a Role's own setting must beat every broader scope.
    ///
    /// The point of adding Role to the cascade is that Jeff can throttle ONE
    /// role without touching code or the others. If Service outranked Role, a
    /// role-level cap would be silently overridden by whatever service the
    /// role happened to be calling — the setting would appear to be saved and
    /// have no effect, which is the worst of both.
    #[test]
    fn role_beats_service() {
        assert!(ScopeKind::Role.rank() > ScopeKind::Service.rank());
    }

    /// NEGATIVE PROOF: ranks stay strictly ordered and unique.
    ///
    /// Two scopes sharing a rank makes precedence a coin-flip, and the
    /// resolver's own malformed_duplicate_rank test proves it errors on that.
    /// Inserting Role at the top is exactly where a duplicate would be
    /// introduced by accident, so pin the whole ladder rather than one pair.
    #[test]
    fn ranks_are_strictly_descending_and_unique() {
        let ladder = [
            ScopeKind::Role,
            ScopeKind::Service,
            ScopeKind::Domain,
            ScopeKind::Product,
            ScopeKind::ValueStreamStep,
            ScopeKind::ValueStream,
        ];
        for pair in ladder.windows(2) {
            assert!(
                pair[0].rank() > pair[1].rank(),
                "{:?} must outrank {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    /// A role-scoped value wins over a domain-scoped default — the actual
    /// behaviour Jeff asked for, exercised through the resolver rather than
    /// asserted on the enum.
    #[test]
    fn role_scoped_word_cap_overrides_the_domain_default() {
        // Chain is MOST specific first — the resolver refuses a chain that is
        // not strictly descending, so Role leads.
        let chain = vec![
            ScopeNode {
                kind: ScopeKind::Role,
                iri: "chorus:role-wren".into(),
                properties: vec![PropertyDatum {
                    iri: "chorus:prop-wren-word-cap".into(),
                    key: "response.word.cap".into(),
                    value: "40".into(),
                    value_type: "integer".into(),
                }],
            },
            ScopeNode {
                kind: ScopeKind::Domain,
                iri: "chorus:roles".into(),
                properties: vec![PropertyDatum {
                    iri: "chorus:prop-team-word-cap".into(),
                    key: "response.word.cap".into(),
                    value: "100".into(),
                    value_type: "integer".into(),
                }],
            },
        ];
        let got = decide_effective_value(&chain, "response.word.cap")
            .expect("chain is well-formed")
            .expect("the key is set somewhere in the chain");
        assert_eq!(got.value, "40", "the role's own cap must win");
        assert_eq!(got.winning_property_iri, "chorus:prop-wren-word-cap");
        assert_eq!(got.winning_scope_kind, ScopeKind::Role);
    }

    /// NEGATIVE PROOF: with no role-scoped value, the domain default is what
    /// resolves — so the test above is proving precedence, not just reading
    /// back the last thing pushed.
    #[test]
    fn without_a_role_value_the_domain_default_resolves() {
        let chain = vec![ScopeNode {
            kind: ScopeKind::Domain,
            iri: "chorus:roles".into(),
            properties: vec![PropertyDatum {
                iri: "chorus:prop-team-word-cap".into(),
                key: "response.word.cap".into(),
                value: "100".into(),
                value_type: "integer".into(),
            }],
        }];
        let got = decide_effective_value(&chain, "response.word.cap")
            .expect("well-formed")
            .expect("set at domain");
        assert_eq!(got.value, "100");
        assert_eq!(got.winning_scope_kind, ScopeKind::Domain);
    }

    /// NEGATIVE PROOF: an unset key resolves to None, never to a number.
    ///
    /// The enforcement point must be able to tell "no cap configured" from "cap
    /// is zero". If this ever returned a default, the governed value would
    /// quietly become a constant again — the exact thing this card removes.
    #[test]
    fn an_unconfigured_cap_is_none_not_a_default() {
        let chain = vec![ScopeNode {
            kind: ScopeKind::Role,
            iri: "chorus:role-wren".into(),
            properties: vec![],
        }];
        assert!(decide_effective_value(&chain, "response.word.cap")
            .expect("well-formed")
            .is_none());
    }
}
