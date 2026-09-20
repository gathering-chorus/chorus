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

/// Fetch one row's served keys. An unreachable or non-JSON response is
/// Unmeasured, never an empty key list — an empty list would read as "the door
/// dropped everything", which is a different and much louder wrong answer.
pub fn served_keys(collection: &str, name: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/v1/{}/{}", api_base(), collection, name);
    let out = Command::new("curl")
        .arg("-sS")
        .arg("--max-time")
        .arg("30")
        .arg(&url)
        .output()
        .map_err(|e| format!("curl failed to start: {e}"))?;
    if !out.status.success() {
        return Err(format!("door unreachable at {url}"));
    }
    let body = String::from_utf8_lossy(&out.stdout).to_string();
    if body.trim().is_empty() {
        return Err(format!("empty response from {url}"));
    }
    if !body.contains('{') {
        return Err(format!("non-JSON response from {url}"));
    }
    Ok(keys_of(&body))
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

/// Build findings for one subject.
pub fn check_subject(collection: &str, name: &str, stored: &[String]) -> (Verdict, Vec<Finding>) {
    match served_keys(collection, name) {
        Err(why) => (Verdict::Unmeasured(why), vec![]),
        Ok(served) => {
            let missing = dropped(stored, &served);
            if missing.is_empty() {
                (Verdict::Clean, vec![])
            } else {
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
        }
    }
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
        let (v, f) = check_subject("products/products", "werk", &["hasDomain".to_string()]);
        std::env::remove_var("CHORUS_OWL_API");
        assert!(v.is_unmeasured(), "dead door reported {v:?}");
        assert!(f.is_empty());
    }
}
