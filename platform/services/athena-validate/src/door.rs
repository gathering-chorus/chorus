//! Does what the store holds survive the door?
//!
//! #4167. The check nothing has ever run, and the one that cost the most.
//!
//! On 2026-09-19 the product `werk` held five `hasDomain` edges in
//! `urn:chorus:domains:products`, and `GET /v1/products/products/werk` returned
//! no `hasDomain` key at all — not truncated to one, absent. Every other check
//! in this sweep would have called that graph clean, because the row is in the
//! right graph, carries its required fields and has a valid owner. The data was
//! fine; the read was lossy, and nothing looked.
//!
//! So this check does not ask the store a question. It asks the store and the
//! door the SAME question and compares the answers, which is the only way the
//! difference between them can be seen at all.

use crate::checks::Verdict;
use crate::store::Finding;
use std::process::Command;

pub fn api_base() -> String {
    std::env::var("CHORUS_OWL_API").unwrap_or_else(|_| "http://localhost:3360".into())
}

/// Predicates the door is not expected to project: audit fields it generates
/// itself, and rdf:type, which it serves under its own key.
const NOT_PROJECTED: [&str; 5] = ["type", "created", "creator", "modified", "iri"];

/// Which of a subject's stored predicates are missing from the served JSON.
///
/// `stored` is the local names held in the graph; `served` is the keys the API
/// returned. The answer is stored-minus-served, minus the audit fields the door
/// owns. Kept pure so it can be tested without a store or a live route — the
/// fetching is separate on purpose.
pub fn dropped(stored: &[String], served: &[String]) -> Vec<String> {
    stored
        .iter()
        .filter(|p| !NOT_PROJECTED.contains(&p.as_str()))
        .filter(|p| !served.iter().any(|s| s == *p))
        .cloned()
        .collect()
}

/// Top-level keys of the served object, read without a JSON dependency
/// (ADR-032 §1: std only). Good enough for the question asked — we need the key
/// names, not the values.
fn keys_of(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find('"') {
        rest = &rest[i + 1..];
        let Some(j) = rest.find('"') else { break };
        let key = &rest[..j];
        rest = &rest[j + 1..];
        // A key is followed by a colon; a value is not.
        if rest.trim_start().starts_with(':') && !key.is_empty() && !out.contains(&key.to_string()) {
            out.push(key.to_string());
        }
    }
    out
}

fn is_unknown_route(body: &str) -> bool {
    body.contains("\"error\"") && body.contains("unknown route")
}

/// The door answered, and said the row is not there: an Error envelope with a
/// 404. The store holds the row, so this is a real finding, but a different one
/// from a dropped field — the whole row is unreachable, not part of it.
fn is_not_found(body_keys: &[String], body: &str) -> bool {
    body_keys.iter().any(|k| k == "kind") && body.contains("\"kind\": \"Error\"") && body.contains("\"status\": 404")
}

/// The string value of the first `"key": "value"` in a body, without a JSON
/// crate. Enough for "name" in a listing and "iri" in a row.
fn value_of(body: &str, key: &str) -> Option<String> {
    let at = body.find(&format!("\"{key}\""))?;
    let rest = &body[at + key.len() + 2..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

fn get(url: &str) -> Result<String, String> {
    let out = Command::new("curl")
        .arg("-sS").arg("--max-time").arg("30").arg(url)
        .output()
        .map_err(|e| format!("curl failed to start: {e}"))?;
    if !out.status.success() {
        return Err(format!("door unreachable at {url}"));
    }
    let body = String::from_utf8_lossy(&out.stdout).to_string();
    if !body.contains('{') {
        return Err(format!("non-JSON response from {url}"));
    }
    if is_unknown_route(&body) {
        return Err(format!("unknown route {url}"));
    }
    Ok(body)
}

/// One row per collection, chosen by the DOOR: the first name its listing
/// hands out, read back by that name, compared with the store under the IRI
/// the read names (#4331). Sampling from the store instead asked the door for
/// localnames it never serves (`card-3325` where the door's name is `3325`), so
/// the check measured its own guess, not the door.
pub fn check_collection(collection: &str) -> (Verdict, Vec<Finding>) {
    let base = format!("{}/v1/{}", api_base(), collection);
    let list = match get(&format!("{base}?limit=1")) {
        Ok(b) => b,
        Err(why) => return (Verdict::Unmeasured(why), vec![]),
    };
    let Some(name) = value_of(&list, "name") else {
        return (Verdict::Clean, vec![]); // an empty collection has no row to lose
    };
    let item = match get(&format!("{base}/{name}")) {
        Ok(b) => b,
        Err(why) => return (Verdict::Unmeasured(why), vec![]),
    };
    let keys = keys_of(&item);
    if is_not_found(&keys, &item) {
        return (
            Verdict::Found(1),
            vec![Finding {
                check: "row-not-found-at-the-door".into(),
                subject: name.clone(),
                detail: format!("the listing names it, GET /v1/{collection}/{name} is 404"),
            }],
        );
    }
    let Some(iri) = value_of(&item, "iri") else {
        return (Verdict::Unmeasured(format!("no iri in /v1/{collection}/{name}")), vec![]);
    };
    let Some(stored) = crate::store::predicates_of(&iri) else {
        return (Verdict::Unmeasured(format!("store unreadable for {iri}")), vec![]);
    };
    findings_for(&name, &stored, &keys)
}

fn findings_for(name: &str, stored: &[String], served: &[String]) -> (Verdict, Vec<Finding>) {
    let missing = dropped(stored, served);
    if missing.is_empty() {
        return (Verdict::Clean, vec![]);
    }
    let n = missing.len();
    (
        Verdict::Found(n),
        missing
            .into_iter()
            .map(|p| Finding {
                check: "field-dropped-by-the-door".into(),
                subject: name.to_string(),
                detail: format!("{p} is in the store and not in the response"),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_read_without_a_json_crate() {
        let body = r#"{ "data": { "name": "werk", "label": "Werk", "docState": "current" } }"#;
        let k = keys_of(body);
        assert!(k.contains(&"name".to_string()));
        assert!(k.contains(&"label".to_string()));
        assert!(k.contains(&"docState".to_string()));
    }

    /// NEGATIVE PROOF (#3734): the real case. werk holds hasDomain in the store
    /// and the door returned no such key on 2026-09-19. The check must report
    /// it; if it ever returns clean for this input, it has stopped being able to
    /// see the defect it was written for.
    #[test]
    fn negative_proof_the_werk_case_is_reported() {
        let stored = vec!["label".into(), "docState".into(), "hasDomain".into()];
        let served = vec!["label".into(), "docState".into()];
        let missing = dropped(&stored, &served);
        assert_eq!(missing, vec!["hasDomain".to_string()]);
    }

    #[test]
    fn a_row_that_survives_intact_is_clean() {
        let stored = vec!["label".into(), "hasDomain".into()];
        let served = vec!["label".into(), "hasDomain".into()];
        assert!(dropped(&stored, &served).is_empty());
    }

    /// The door generates its own audit fields; their absence from the store is
    /// not a drop, and counting them would make every row dirty forever — a
    /// check that is always red is as useless as one that is never red.
    #[test]
    fn audit_fields_are_not_counted_as_dropped() {
        let stored = vec!["type".into(), "created".into(), "creator".into(), "label".into()];
        let served = vec!["label".into()];
        assert!(dropped(&stored, &served).is_empty());
    }

    /// NEGATIVE PROOF: a door that cannot be reached is Unmeasured. Without
    /// this, an unreachable API returns no keys, every stored predicate reads as
    /// dropped, and the sweep screams about a defect that isn't there.
    #[test]
    fn negative_proof_an_unreachable_door_is_unmeasured_not_a_flood() {
        std::env::set_var("CHORUS_OWL_API", "http://127.0.0.1:9");
        let (v, f) = check_collection("products/products");
        std::env::remove_var("CHORUS_OWL_API");
        assert!(v.is_unmeasured(), "dead door reported {v:?}");
        assert!(f.is_empty());
    }

    /// NEGATIVE PROOF (#4331): the body the door returned for every sampled row
    /// on 2026-09-26. Read as a row, its keys ("error", "served") made every
    /// stored field look dropped. It must be recognised as the check's own
    /// wrong URL, never as a row.
    #[test]
    fn negative_proof_an_unknown_route_is_not_read_as_a_row() {
        let body = r#"{ "error": "unknown route", "served": ["/security/apisurfaces", "/roles/agentroles"] }"#;
        assert!(is_unknown_route(body));
        let row = r#"{ "kind": "AgentRole", "data": { "name": "role-kade", "label": "Kade" } }"#;
        assert!(!is_unknown_route(row));
    }

    /// NEGATIVE PROOF (#4331): a 404 Error envelope for a row the store holds
    /// (live: /v1/roles/agentroles/role-kade, which the listing serves) is a
    /// missing row, not a row with no fields.
    #[test]
    fn negative_proof_a_404_envelope_is_a_missing_row() {
        let body = r#"{ "apiVersion": "v1", "kind": "Error", "data": { "type": "/errors/not-found", "title": "Not Found", "status": 404, "detail": "no such agentrole: role-kade" } }"#;
        assert!(is_not_found(&keys_of(body), body));
        let row = r#"{ "apiVersion": "v1", "kind": "AgentRole", "data": { "name": "role-kade", "status": "" } }"#;
        assert!(!is_not_found(&keys_of(row), row));
    }
}
