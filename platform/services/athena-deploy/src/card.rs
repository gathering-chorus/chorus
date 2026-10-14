//! #4515 — `athena-deploy --card <n>` (Land, prod) and `athena-deploy staging
//! --card <n>` (Deploy, staging): one card's model change, two targets.
//!
//! Both do the same four things the old athena.yml did across its legs, scoped to
//! the card instead of a typed class or the whole model set:
//!   1. resolve the card (shared/card_scope.rs): its werk and its changed TTL files;
//!      no werk or no model change is REFUSED by name
//!   2. load ONLY those files: a file listed in the domain-set manifest deploys its
//!      whole set into that set's graph; every other file merges into the ontology
//!      graph (the same additive per-subject merge as a full deploy)
//!   3. restart athena-make (athena-serve --kickstart; it reads shapes at boot)
//!   4. the served check (old `deploy-served`): every class the card declares must
//!      appear in a route of athena-make's discovery document, and that route must
//!      answer. "Deployed but not served" is a named failure.
//!
//! Staging first copies prod's model graphs into the staging dataset (staging copy)
//! and points the load at it; the served check reads the staging athena-make.

use crate::card_scope::{self, Card};
use crate::{parse_domain_sets, DomainSet};
use std::process::Command;

/// What a card's deploy loads: ontology files (absolute) and whole domain sets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardPlan {
    pub ontology: Vec<String>,
    pub sets: Vec<DomainSet>,
}

/// Split a card's changed TTL files (werk-relative) between the domain sets they
/// belong to and the ontology graph. A file in a set brings its whole set (its
/// merge is the set's unit); a set is listed once. Pure.
pub fn plan(card_ttl: &[String], werk: &str, sets: &[DomainSet]) -> CardPlan {
    let mut p = CardPlan::default();
    for f in card_ttl {
        match sets.iter().find(|s| s.files.iter().any(|sf| sf == f)) {
            Some(s) => {
                if !p.sets.iter().any(|x| x.name == s.name && x.graph == s.graph) {
                    p.sets.push(s.clone());
                }
            }
            None => p.ontology.push(format!("{}/{}", werk.trim_end_matches('/'), f)),
        }
    }
    p
}

/// The (kind, collection) pairs of athena-make's discovery document. A
/// discovery entry is `{ "kind": "<Class>", "collection": "/v1/<domain>/<seg>", … }`;
/// the document's own `"kind": "Discovery"` carries no collection and is skipped.
pub fn discovery_routes(disco: &str) -> Vec<(String, String)> {
    fn first_string(s: &str) -> Option<String> {
        let rest = s.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
        rest.find('"').map(|e| rest[..e].to_string())
    }
    let mut out = Vec::new();
    for chunk in disco.split("\"kind\"").skip(1) {
        let Some(kind) = first_string(chunk) else { continue };
        let Some(at) = chunk.find("\"collection\"") else { continue };
        if let Some(col) = first_string(&chunk[at + "\"collection\"".len()..]) {
            out.push((kind, col));
        }
    }
    out
}

/// The served check over a discovery document. `answers(route)` says whether a
/// GET of that route answered 2xx. Ok = one line per class; Err = every failure,
/// each naming its class. A document with no routes is UNMEASURED, never "no
/// class is served" and never a pass.
pub fn served_check(disco: &str, classes: &[String], answers: &dyn Fn(&str) -> bool) -> Result<Vec<String>, Vec<String>> {
    if classes.is_empty() {
        return Ok(vec!["served: the card's model files declare no class — nothing to serve".into()]);
    }
    let routes = discovery_routes(disco);
    if routes.is_empty() {
        return Err(vec![
            "served: UNMEASURED — the discovery document is unavailable or lists no route; cannot establish which classes are served".into(),
        ]);
    }
    let (mut ok, mut bad) = (Vec::new(), Vec::new());
    for c in classes {
        match routes.iter().find(|(k, _)| k == c) {
            None => bad.push(format!(
                "served: {c} DEPLOYED BUT NOT SERVED — it appears in no route of the discovery document (the model is in the store and the API cannot see it, #3577)"
            )),
            Some((_, route)) if answers(route) => ok.push(format!("served: {c} at {route} answers")),
            Some((_, route)) => {
                let bare = format!("/{}", route.rsplit('/').next().unwrap_or(""));
                let mut m = format!("served: {c} is declared at {route} but that route does not answer");
                if bare != *route && answers(&bare) {
                    m.push_str(&format!(" ({bare} DOES answer: discovery advertises one path and the router serves another)"));
                }
                bad.push(m);
            }
        }
    }
    if bad.is_empty() { Ok(ok) } else { Err(bad) }
}

/// GET without the Fuseki credential (athena-make is not the store).
fn get_body(url: &str) -> String {
    Command::new("curl").args(["-s", "-m", "10", url]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

fn get_answers(url: &str) -> bool {
    Command::new("curl").args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "-m", "10", url]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().starts_with('2')).unwrap_or(false)
}

/// The served check against a live athena-make at `api`.
pub fn served_check_live(api: &str, classes: &[String]) -> Result<Vec<String>, Vec<String>> {
    let api = api.trim_end_matches('/');
    let disco = get_body(&format!("{api}/"));
    served_check(&disco, classes, &|route: &str| get_answers(&format!("{api}{route}")))
}

/// The local names of the classes the card's TTL files declare (through riot).
pub fn card_classes(card: &Card) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for f in card.ttl() {
        let nt = card_scope::ntriples(&[card.abs(&f)]).map_err(|e| format!("card #{}: {f}: {e}", card.card))?;
        for c in card_scope::declared_classes(&nt) {
            let n = card_scope::local_name(&c);
            if !out.contains(&n) {
                out.push(n);
            }
        }
    }
    Ok(out)
}

/// A card run's failure: REFUSED before anything was written (exit 2) or
/// FAILED on the way (exit 1).
#[derive(Debug, PartialEq, Eq)]
pub enum CardError {
    Refused(String),
    Failed(String),
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_string())
}

/// Resolve the card and build its plan and class list. Nothing is written.
pub fn prepare(card_id: &str) -> Result<(Card, CardPlan, Vec<String>), CardError> {
    let card = card_scope::resolve(card_id).map_err(CardError::Refused)?;
    let ttl = card.ttl();
    let manifest = env_or("DOMAIN_SET_MANIFEST", &card.abs("platform/config/domain-set-manifest.txt"));
    let text = std::fs::read_to_string(&manifest)
        .map_err(|e| CardError::Refused(format!("card #{}: domain-set manifest unreadable at {manifest}: {e}", card.card)))?;
    let sets = parse_domain_sets(&text).map_err(CardError::Refused)?;
    let p = plan(&ttl, &card.werk, &sets);
    let classes = card_classes(&card).map_err(CardError::Refused)?;
    Ok((card, p, classes))
}

/// `athena-deploy [staging] --card <n>`.
pub fn run(card_id: &str, staging: bool) -> Result<String, CardError> {
    let (card, p, classes) = prepare(card_id)?;
    let mut out = vec![format!(
        "card #{}: {} ontology file(s), {} domain set(s), {} class(es) from {}",
        card.card, p.ontology.len(), p.sets.len(), classes.len(), card.werk
    )];
    let (api, label) = if staging {
        let base = env_or("CHORUS_FUSEKI_BASE", "http://localhost:3030");
        let prod = env_or("STAGING_PROD_DATASET", "pods");
        let stg = env_or("STAGING_DATASET", "staging");
        let manifest = format!("{}/.chorus/staging/manifest.tsv", std::env::var("HOME").unwrap_or_default());
        out.push(crate::staging::copy(&base, &prod, &stg, &manifest).map_err(|e| CardError::Failed(format!("staging copy: {e}")))?);
        // the load goes to staging, never prod
        std::env::set_var("FUSEKI_GSP", format!("{base}/{stg}/data"));
        std::env::set_var("FUSEKI_QUERY", format!("{base}/{stg}/query"));
        std::env::set_var("FUSEKI_UPDATE", format!("{base}/{stg}/update"));
        (env_or("ATHENA_MAKE_STAGING_URL", "http://localhost:3368"), env_or("ATHENA_MAKE_STAGING_LABEL", "com.chorus.athena-make.staging"))
    } else {
        (env_or("ATHENA_MAKE_URL", "http://localhost:3360"), env_or("ATHENA_MAKE_LABEL", "com.chorus.athena-make"))
    };
    std::env::set_var("CHORUS_ROOT", &card.werk);
    out.push(crate::run_athena_deploy_plan(Some(&p)).map_err(CardError::Failed)?);

    // athena-make reads the shapes at boot: restart it and wait for healthy.
    let serve = env_or("ATHENA_SERVE_BIN", "athena-serve");
    let s = Command::new(&serve)
        .args([label.as_str(), &format!("{}/health", api.trim_end_matches('/')), "--kickstart", "--timeout", "90"])
        .output()
        .map_err(|e| CardError::Failed(format!("{serve} not runnable: {e} — loaded but not served")))?;
    if !s.status.success() {
        return Err(CardError::Failed(format!(
            "{serve} {label}: {} — loaded but athena-make is not healthy",
            String::from_utf8_lossy(&s.stderr).trim()
        )));
    }
    match served_check_live(&api, &classes) {
        Ok(lines) => out.extend(lines),
        Err(lines) => return Err(CardError::Failed(lines.join("\n"))),
    }
    Ok(out.join("\n"))
}
