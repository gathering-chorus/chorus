//! athena-deploy core — the Athena value-stream DEPLOY verb (model → live graph).
//!
//! Atomic-verb contract (peer of werk-*, ADR-032/037):
//! - Zero-dep std-only; curl/riot/chorus-log invoked as subprocesses.
//! - Idempotent: same TTL → same graph (delete-staged-subjects-then-insert).
//! - ADDITIVE per-domain: a deploy replaces only the deploying domain's OWN subjects;
//!   sibling domains + live-loaded instance data survive (fixes the #3540/#3496 clobber).
//! - Never a whole-graph COPY/clear (dodges #3496's NodeTableTRDF large-clear failure);
//!   stages via GSP POST, merges via one SPARQL transaction.
//! - Emits spine events athena.deployed / athena.deploy.failed.
//! - Exit 0 deployed+verified; 1 on any failure (thin main maps Result→exit code).

use std::path::Path;
use std::process::Command;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// The model SET to deploy: an explicit `TTL` override (single member) else the
/// default set (chorus.ttl + werk-domains.ttl). Pure — unit-tested.
pub fn model_set(root: &str, ttl_override: Option<String>) -> Vec<String> {
    match ttl_override {
        Some(t) if !t.is_empty() => vec![t],
        _ => vec![
            format!("{root}/roles/silas/ontology/chorus.ttl"),
            format!("{root}/roles/kade/ontology/werk-domains.ttl"),
        ],
    }
}

/// The additive-merge update: DELETE only the triples whose SUBJECT is (re)defined
/// in staging, then INSERT staging — one transaction. Touches only the deploying
/// domain's own subjects; leaves every sibling's triples intact. Pure — unit-tested.
pub fn merge_sparql(staging: &str, ontology: &str) -> String {
    format!(
        "DELETE {{ GRAPH <{ont}> {{ ?s ?p ?o }} }} \
         WHERE {{ GRAPH <{stg}> {{ ?s ?sp ?so }} GRAPH <{ont}> {{ ?s ?p ?o }} }} ; \
         INSERT {{ GRAPH <{ont}> {{ ?s ?p ?o }} }} \
         WHERE {{ GRAPH <{stg}> {{ ?s ?p ?o }} }}",
        ont = ontology,
        stg = staging,
    )
}

fn ok_http(code: &str) -> bool {
    matches!(code, "200" | "201" | "204")
}

/// Run a curl invocation, returning its stdout (trimmed) or a failure message.
fn curl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("curl")
        .args(args)
        .output()
        .map_err(|e| format!("curl spawn failed: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Best-effort spine emit (never blocks the deploy result).
fn emit_spine(chorus_log: &str, event: &str, role: &str, fields: &[(&str, String)]) {
    let mut args: Vec<String> = vec![event.to_string(), role.to_string()];
    for (k, v) in fields {
        args.push(format!("{k}={v}"));
    }
    let _ = Command::new(chorus_log).args(&args).output();
}

pub fn run_athena_deploy() -> Result<String, String> {
    let root = env_or("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus");
    let ontology = env_or("ONTOLOGY_GRAPH", "urn:chorus:ontology");
    let gsp = env_or("FUSEKI_GSP", "http://localhost:3030/pods/data");
    let query = env_or("FUSEKI_QUERY", "http://localhost:3030/pods/query");
    let update = env_or("FUSEKI_UPDATE", "http://localhost:3030/pods/update");
    let chorus_log = env_or("CHORUS_LOG", &format!("{root}/platform/scripts/chorus-log"));
    let role = std::env::var("DEPLOY_ROLE")
        .or_else(|_| std::env::var("CHORUS_ROLE"))
        .unwrap_or_else(|_| "system".to_string());
    let ttl_override = std::env::var("TTL").ok();
    let set = model_set(&root, ttl_override);
    let staging = format!("{ontology}-staging-deploy");

    let fail = |reason: &str| -> String {
        emit_spine(&chorus_log, "athena.deploy.failed", &role,
            &[("graph", ontology.clone()), ("reason", reason.to_string())]);
        format!("athena-deploy: {reason}")
    };

    // Validate every member exists + is riot-valid (don't deploy a broken model).
    for ttl in &set {
        if !Path::new(ttl).exists() {
            return Err(fail(&format!("ttl-not-found:{ttl}")));
        }
        if riot_available() {
            let status = Command::new("riot").arg("--validate").arg(ttl)
                .output().map(|o| o.status.success()).unwrap_or(false);
            if !status {
                return Err(fail(&format!("riot-invalid:{ttl}")));
            }
        }
    }

    // Step 1: stage the SET into a fresh staging graph (GSP POST merges).
    let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null",
        &format!("{gsp}?graph={staging}")]);
    for ttl in &set {
        let code = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
            "-H", "Content-Type: text/turtle", "--data-binary", &format!("@{ttl}"),
            &format!("{gsp}?graph={staging}")])?;
        if !ok_http(&code) {
            return Err(fail(&format!("staging-load-http-{code}")));
        }
    }

    // Step 2: additive merge (delete-staged-subjects-then-insert), one transaction.
    let sparql = merge_sparql(&staging, &ontology);
    let mcode = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
        "-H", "Content-Type: application/sparql-update", "--data-binary", &sparql, &update])?;
    let _ = curl(&["-s", "-X", "DELETE", "-o", "/dev/null", &format!("{gsp}?graph={staging}")]);
    if !ok_http(&mcode) {
        return Err(fail(&format!("merge-http-{mcode}")));
    }

    // Step 3: verify the graph is non-empty (proof, not assumption).
    let body = curl(&["-s", "--data-urlencode",
        &format!("query=ASK {{ GRAPH <{ontology}> {{ ?s ?p ?o }} }}"),
        "-H", "Accept: application/sparql-results+json", &query])?;
    if !body.replace(' ', "").contains("\"boolean\":true") {
        return Err(fail("verify-empty"));
    }

    emit_spine(&chorus_log, "athena.deployed", &role,
        &[("graph", ontology.clone()), ("members", set.len().to_string())]);
    Ok(format!(
        "athena-deploy: deployed {} model file(s) -> <{}> (additive merge, siblings preserved)",
        set.len(), ontology
    ))
}

fn riot_available() -> bool {
    Command::new("sh").arg("-c").arg("command -v riot")
        .output().map(|o| o.status.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_set_default_is_the_two_member_set() {
        let s = model_set("/R", None);
        assert_eq!(s.len(), 2);
        assert!(s[0].ends_with("/roles/silas/ontology/chorus.ttl"));
        assert!(s[1].ends_with("/roles/kade/ontology/werk-domains.ttl"));
    }

    #[test]
    fn model_set_honors_ttl_override_as_single_member() {
        let s = model_set("/R", Some("/x/werk-domains.ttl".into()));
        assert_eq!(s, vec!["/x/werk-domains.ttl".to_string()]);
    }

    #[test]
    fn model_set_empty_override_falls_back_to_default() {
        assert_eq!(model_set("/R", Some(String::new())).len(), 2);
    }

    #[test]
    fn merge_sparql_deletes_only_staged_subjects_then_inserts() {
        let q = merge_sparql("urn:stg", "urn:ont");
        // DELETE is scoped to subjects that appear in staging (the join on ?s),
        // never an unconditional clear of the ontology graph.
        assert!(q.contains("DELETE { GRAPH <urn:ont>"));
        assert!(q.contains("GRAPH <urn:stg> { ?s ?sp ?so }"));
        assert!(q.contains("INSERT { GRAPH <urn:ont>"));
        // Must NOT contain a whole-graph COPY/CLEAR/DROP (the #3496 clobber).
        assert!(!q.to_uppercase().contains("COPY"));
        assert!(!q.to_uppercase().contains("CLEAR"));
        assert!(!q.to_uppercase().contains("DROP"));
    }

    #[test]
    fn ok_http_accepts_2xx_only() {
        assert!(ok_http("200") && ok_http("201") && ok_http("204"));
        assert!(!ok_http("500") && !ok_http("000") && !ok_http("404"));
    }
}
