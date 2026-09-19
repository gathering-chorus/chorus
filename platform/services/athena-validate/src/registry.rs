//! The subject list the door check walks.
//!
//! #4167. The door check compares one subject's stored predicates against the
//! keys the API returns for it. To do that it needs to know which collections
//! exist and which class each serves — and the only honest source for that is
//! the door's own discovery endpoint, because the question being asked is
//! precisely "does the door agree with the store".
//!
//! Sampling, not exhaustive: the fields a route projects are a property of the
//! route, not of the row, so one row per collection finds a dropped field just
//! as surely as ten thousand would, in a sweep that has to finish.

use std::process::Command;

#[derive(Debug)]
pub struct Served {
    pub collection: String,
    pub kind: String,
}

/// Read `/v1` and return (collection, kind) for every generated route.
///
/// A discovery endpoint that cannot be read is an error, never an empty list:
/// an empty list would make the door check silently pass by walking nothing,
/// which is the vacuous-pass shape this whole crate exists to make unwriteable.
pub fn served_collections(api_base: &str) -> Result<Vec<Served>, String> {
    let out = Command::new("curl")
        .arg("-sS")
        .arg("--max-time")
        .arg("30")
        .arg(format!("{api_base}/v1"))
        .output()
        .map_err(|e| format!("curl failed to start: {e}"))?;
    if !out.status.success() {
        return Err(format!("discovery unreachable at {api_base}/v1"));
    }
    let body = String::from_utf8_lossy(&out.stdout).to_string();
    if !body.contains("\"primitives\"") {
        return Err(format!("no primitives in {api_base}/v1"));
    }
    let list = parse_primitives(&body);
    if list.is_empty() {
        return Err("discovery returned zero collections".into());
    }
    Ok(list)
}

/// Pull the kind/collection pairs out of the discovery JSON without a JSON
/// dependency (ADR-032 §1: std only).
pub fn parse_primitives(body: &str) -> Vec<Served> {
    let mut out = Vec::new();
    for chunk in body.split("\"kind\":").skip(1) {
        let Some(kind) = between_quotes(chunk) else { continue };
        let Some(coll_at) = chunk.find("\"collection\":") else { continue };
        let Some(collection) = between_quotes(&chunk[coll_at + "\"collection\":".len()..]) else {
            continue;
        };
        // Routes are served as /v1/<domain>/<plural>; the leading slash is the
        // discovery form and the caller appends the subject name.
        let collection = collection.trim_start_matches('/').to_string();
        if !collection.is_empty() && !kind.is_empty() {
            out.push(Served { collection, kind });
        }
    }
    out
}

fn between_quotes(s: &str) -> Option<String> {
    let a = s.find('"')?;
    let rest = &s[a + 1..];
    let b = rest.find('"')?;
    Some(rest[..b].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{ "apiVersion": "v1", "service": "athena-make", "count": 2,
      "primitives": [
        { "kind": "CodeFile", "collection": "/code/files", "openapi": "/code/files/openapi.json" },
        { "kind": "Product", "collection": "/products/products" } ] }"#;

    #[test]
    fn collections_and_kinds_are_read_from_discovery() {
        let p = parse_primitives(SAMPLE);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].kind, "CodeFile");
        assert_eq!(p[0].collection, "code/files");
        assert_eq!(p[1].kind, "Product");
    }

    /// NEGATIVE PROOF (#3734): the state this function exists to prevent is a
    /// door check that walks nothing and reports clean. A discovery response
    /// with no primitives, and an unreachable endpoint, must both be errors —
    /// never an empty Ok list.
    #[test]
    fn negative_proof_empty_discovery_is_an_error_not_an_empty_walk() {
        assert!(parse_primitives("{}").is_empty());
        let r = served_collections("http://127.0.0.1:9");
        assert!(r.is_err(), "unreachable discovery returned {r:?}");
    }
}
