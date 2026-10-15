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
//! #4515 landed mode — `athena-deploy --card <n> --landed --target canonical` is
//! the athena pipeline's Land. It runs after the werk land, when the werk may be
//! gone: the card's change is the one commit it squashed onto origin/main, read
//! from canonical ($CHORUS_HOME), and the deploy is stamped with that commit. It
//! writes prod, so the caller says `--target canonical` (the #4080 guard's
//! DEPLOY_TARGET, passed by the pipeline step, never set by this code on its
//! own); without it the run is REFUSED. A landed commit with no model source is
//! `result=none`, exit 0.
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
    /// #4515 — the landed commit the deploy is stamped with (landed mode only).
    pub commit: Option<String>,
    /// #4515 — where the model files are read from when that is not CHORUS_ROOT:
    /// a landed card's snapshot of its merged commit (landed mode only).
    pub files_root: Option<String>,
    /// #4536 — where the card's changed-subject files are written (removed after the run).
    pub scratch: Option<String>,
}

/// The paths of a commit's tree that a deploy reads: every roles/*/ontology
/// file, designing's TTL and JSONL (data, ontology, the retirements ledger),
/// and the domain-set manifest. Pure, over `git ls-tree -r --name-only`.
pub fn model_paths(ls_tree: &str) -> Vec<String> {
    ls_tree
        .lines()
        .map(str::trim)
        .filter(|p| {
            (p.starts_with("roles/") && p.contains("/ontology/"))
                || (p.starts_with("designing/") && (p.ends_with(".ttl") || p.ends_with(".jsonl")))
                || *p == "platform/config/domain-set-manifest.txt"
        })
        .map(String::from)
        .collect()
}

/// #4515 — the model files AS OF a commit, written under `dest` (same layout):
/// `git archive <sha> -- <model paths> | tar -x`. A landed card deploys what it
/// landed, never what canonical's working tree holds now (a later edit, or a
/// later card's change to the same file). Nothing is written into `home`.
pub fn snapshot(home: &str, sha: &str, dest: &str) -> Result<(), String> {
    let ls = Command::new("git").args(["-C", home, "ls-tree", "-r", "--name-only", sha]).output()
        .map_err(|e| format!("git: {e}"))?;
    if !ls.status.success() {
        return Err(format!("git ls-tree {sha} failed in {home}: {}", String::from_utf8_lossy(&ls.stderr).trim()));
    }
    let paths = model_paths(&String::from_utf8_lossy(&ls.stdout));
    if paths.is_empty() {
        return Err(format!("commit {sha} holds no model file to snapshot"));
    }
    let _ = std::fs::remove_dir_all(dest);
    std::fs::create_dir_all(dest).map_err(|e| format!("{dest}: {e}"))?;
    let mut archive = Command::new("git").args(["-C", home, "archive", sha, "--"]).args(&paths)
        .stdout(std::process::Stdio::piped()).spawn().map_err(|e| format!("git archive: {e}"))?;
    let tar = Command::new("tar").args(["-x", "-C", dest]).stdin(archive.stdout.take().expect("piped"))
        .output().map_err(|e| format!("tar: {e}"))?;
    let a = archive.wait().map_err(|e| format!("git archive: {e}"))?;
    if !a.success() || !tar.status.success() {
        return Err(format!("could not snapshot {sha} into {dest}: {}", String::from_utf8_lossy(&tar.stderr).trim()));
    }
    Ok(())
}

/// Removes a landed card's snapshot when the run ends, whichever way.
struct SnapshotDir(Option<String>);
impl Drop for SnapshotDir {
    fn drop(&mut self) {
        if let Some(d) = &self.0 {
            let _ = std::fs::remove_dir_all(d);
        }
    }
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
        return Ok(vec!["served: the card adds or changes no class — nothing new to serve".into()]);
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

/// #4531 — the local names of the classes in these triples that a route should
/// serve: every declared class except shape classes (ToolInput), whose instances
/// are SHACL shapes. Pure.
pub fn served_classes(nt: &str) -> Vec<String> {
    let shapes = card_scope::shape_classes(nt);
    card_scope::declared_classes(nt)
        .into_iter()
        .filter(|c| !shapes.contains(c))
        .map(|c| card_scope::local_name(&c))
        .collect()
}

/// #4536 — the served classes the card answers for: the ones it added or
/// changed (card_scope::changed_classes, #4547's rule). A class declared in a
/// touched file with the same triples as `base_nt` is not this card's: touching
/// werk-domains.ttl must not make a card answer for 14 werk-v1 classes the door
/// never served. Pure.
pub fn changed_served_classes(nt: &str, base_nt: &str) -> Vec<String> {
    let mine: Vec<String> = card_scope::changed_classes(nt, base_nt).iter().map(|c| card_scope::local_name(c)).collect();
    served_classes(nt).into_iter().filter(|c| mine.contains(c)).collect()
}

/// A card file as it was before the card: main for a werk, the squash commit's
/// parent for a landed card (read from canonical; the snapshot is no git tree).
fn base_ntriples(card: &Card, rel: &str) -> String {
    let git = if card.commit.is_some() { card_scope::chorus_home() } else { card.werk.clone() };
    card_scope::ntriples_at(&git, &card_scope::base_rev(card), rel)
}

/// #4536 — the triples a card's deploy loads from one file: every subject whose
/// triples differ from `base_nt` (card_scope::changed_subjects), with the blank
/// nodes they reach (a shape's sh:property lists). A subject the card did not
/// change is not loaded: run 10 re-staged werk-domains.ttl's untyped code,
/// pipelines and version-control fragments into the ontology graph, rows that
/// live in the domains graph and change through the door. Pure.
pub fn changed_triples(nt: &str, base_nt: &str) -> String {
    let subj = |l: &str| l.split_whitespace().next().unwrap_or("").to_string();
    let mut keep: Vec<String> = card_scope::changed_subjects(nt, base_nt);
    let mut i = 0;
    while i < keep.len() {
        let s = keep[i].clone();
        for l in nt.lines().filter(|l| subj(l) == s) {
            for t in l.split_whitespace().skip(1) {
                if t.starts_with("_:") && !keep.iter().any(|k| k == t) {
                    keep.push(t.to_string());
                }
            }
        }
        i += 1;
    }
    nt.lines().filter(|l| keep.contains(&subj(l))).map(|l| format!("{l}\n")).collect()
}

/// The local names of the classes the card's TTL files add or change (through riot).
pub fn card_classes(card: &Card) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for f in card.ttl() {
        let nt = card_scope::ntriples(&[card.abs(&f)]).map_err(|e| format!("card #{}: {f}: {e}", card.card))?;
        let base_nt = base_ntriples(card, &f);
        for n in changed_served_classes(&nt, &base_nt) {
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

/// How `--card` finds the card's change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Deploy, staging: the card's werk, origin/main...HEAD.
    Staging,
    /// Land from the werk (prod): the card's werk, origin/main...HEAD.
    Werk,
    /// Land after the werk land (prod): canonical, the card's squash commit.
    Landed,
}

/// The card options after `--card <n>`: `--landed`, `--target <t>`. Pure.
/// The only target is canonical, and a landed run needs it said; staging reads
/// the werk, so `--landed` there is refused. An unknown option is refused,
/// never ignored.
pub fn card_mode(args: &[String]) -> Result<(Mode, Option<String>), String> {
    let staging = args.get(1).map(String::as_str) == Some("staging");
    let mut rest = args.iter().skip(if staging { 2 } else { 1 });
    let (mut landed, mut target) = (false, None);
    while let Some(a) = rest.next() {
        match a.as_str() {
            "--card" => { rest.next(); }
            "--landed" => landed = true,
            "--target" => match rest.next().map(String::as_str) {
                Some("canonical") => target = Some("canonical".to_string()),
                Some(t) => return Err(format!("--target {t}: the only target is canonical (prod, on purpose); staging is `athena-deploy staging --card <n>`")),
                None => return Err("--target needs a value (canonical)".into()),
            },
            other => return Err(format!("unknown option '{other}' with --card — refusing")),
        }
    }
    match (staging, landed) {
        (true, true) => Err("staging reads the card's werk; --landed is the prod Land only".into()),
        (true, false) if target.is_some() => Err("staging writes the staging dataset; --target canonical is the prod Land only".into()),
        (true, false) => Ok((Mode::Staging, None)),
        (false, true) if target.is_none() => Err(
            "--landed writes prod: say --target canonical on purpose (#4080), the pipeline's Land step passes it".into()),
        (false, true) => Ok((Mode::Landed, target)),
        (false, false) => Ok((Mode::Werk, target)),
    }
}

/// Resolve the card and build its plan and class list. Nothing is written.
pub fn prepare(card_id: &str) -> Result<(Card, CardPlan, Vec<String>), CardError> {
    prepare_mode(card_id, Mode::Werk)
}

/// prepare, for a mode: a landed card is resolved on canonical and its model
/// files are read from a snapshot of its merged commit (`card.werk` and the
/// plan's files_root are that snapshot; the caller removes it).
pub fn prepare_mode(card_id: &str, mode: Mode) -> Result<(Card, CardPlan, Vec<String>), CardError> {
    let mut card = match mode {
        Mode::Landed => card_scope::resolve_landed(card_id),
        _ => card_scope::resolve(card_id),
    }
    .map_err(CardError::Refused)?;
    let mut files_root = None;
    if let (Mode::Landed, Some(sha)) = (mode, card.commit.clone()) {
        let dest = std::env::temp_dir()
            .join(format!("athena-deploy-landed-{}-{}-{}", card.card, &sha[..sha.len().min(12)], std::process::id()))
            .display().to_string();
        snapshot(&card.werk, &sha, &dest).map_err(|e| CardError::Refused(format!("card #{}: {e}", card.card)))?;
        card.werk = dest.clone();
        files_root = Some(dest);
    }
    let ttl = card.ttl();
    let manifest = env_or("DOMAIN_SET_MANIFEST", &card.abs("platform/config/domain-set-manifest.txt"));
    let text = std::fs::read_to_string(&manifest)
        .map_err(|e| CardError::Refused(format!("card #{}: domain-set manifest unreadable at {manifest}: {e}", card.card)))?;
    let sets = parse_domain_sets(&text).map_err(CardError::Refused)?;
    let mut p = plan(&ttl, &card.werk, &sets);
    p.commit = card.commit.clone();
    p.files_root = files_root;
    // #4536 — an ontology file loads only the subjects the card changed
    if !p.ontology.is_empty() {
        let dir = std::env::temp_dir().join(format!("athena-deploy-card-{}-{}", card.card, std::process::id())).display().to_string();
        std::fs::create_dir_all(&dir).map_err(|e| CardError::Failed(format!("{dir}: {e}")))?;
        p.scratch = Some(dir.clone());
        let werk = format!("{}/", card.werk.trim_end_matches('/'));
        let mut files = Vec::new();
        for abs in &p.ontology {
            let rel = abs.strip_prefix(&werk).unwrap_or(abs);
            let nt = card_scope::ntriples(&[abs.clone()]).map_err(|e| CardError::Refused(format!("card #{}: {rel}: {e}", card.card)))?;
            let changed = changed_triples(&nt, &base_ntriples(&card, rel));
            if changed.is_empty() {
                continue;
            }
            let out = format!("{dir}/{}.nt", rel.replace('/', "_"));
            std::fs::write(&out, changed).map_err(|e| CardError::Failed(format!("{out}: {e}")))?;
            files.push(out);
        }
        p.ontology = files;
    }
    let classes = card_classes(&card).map_err(CardError::Refused)?;
    Ok((card, p, classes))
}

/// `athena-deploy [staging] --card <n> [--landed] [--target canonical]`.
pub fn run(card_id: &str, mode: Mode, target: Option<&str>) -> Result<String, CardError> {
    let staging = mode == Mode::Staging;
    // the caller's explicit say-so feeds the #4080 guard; nothing here sets it unasked
    if let Some(t) = target {
        std::env::set_var("DEPLOY_TARGET", t);
    }
    if mode == Mode::Landed {
        // a land that carried no model source has nothing for athena: none, not a refusal
        let card = card_scope::resolve_landed(card_id).map_err(CardError::Refused)?;
        if card.sources.is_empty() {
            let sha = card.commit.clone().unwrap_or_default();
            return Ok(format!(
                "card #{}: landed commit {} carries no model source — nothing to deploy\ncard #{}: result=none",
                card.card, &sha[..sha.len().min(12)], card.card
            ));
        }
    }
    let (card, p, classes) = prepare_mode(card_id, mode)?;
    let _snapshot = SnapshotDir(p.files_root.clone());
    let _scratch = SnapshotDir(p.scratch.clone());
    let from = match &card.commit {
        Some(sha) => format!("a snapshot of landed commit {} ({})", &sha[..sha.len().min(12)], card.werk),
        None => card.werk.clone(),
    };
    let mut out = vec![format!(
        "card #{}: {} ontology file(s), {} domain set(s), {} class(es) from {}",
        card.card, p.ontology.len(), p.sets.len(), classes.len(), from
    )];
    // the line a run that stops at the target check still shows (no store reached)
    if std::env::var("ATHENA_DEPLOY_TARGET_CHECK_ONLY").as_deref() == Ok("1") {
        println!("{}", out[0]);
    }
    let (api, label) = if staging {
        let base = env_or("CHORUS_FUSEKI_BASE", "http://localhost:3030");
        let stg = env_or("STAGING_DATASET", "staging");
        let manifest = format!("{}/.chorus/staging/manifest.tsv", std::env::var("HOME").unwrap_or_default());
        // #4536 — reuse the copy this run's baseline was measured on; never copy again
        let stamp = std::fs::read_to_string(crate::staging::trace_path(&manifest)).ok();
        let trace = std::env::var("CHORUS_TRACE_ID").ok();
        out.push(crate::staging::copy_is_this_runs(stamp.as_deref(), trace.as_deref()).map_err(CardError::Refused)?);
        // the load goes to staging, never prod
        std::env::set_var("FUSEKI_GSP", format!("{base}/{stg}/data"));
        std::env::set_var("FUSEKI_QUERY", format!("{base}/{stg}/query"));
        std::env::set_var("FUSEKI_UPDATE", format!("{base}/{stg}/update"));
        (env_or("ATHENA_MAKE_STAGING_URL", "http://localhost:3368"), env_or("ATHENA_MAKE_STAGING_LABEL", "com.chorus.athena-make.staging"))
    } else {
        (env_or("ATHENA_MAKE_URL", "http://localhost:3360"), env_or("ATHENA_MAKE_LABEL", "com.chorus.athena-make"))
    };
    // git, the spine and the #4080 guard work on the card's tree: the werk, or
    // for a landed card canonical (its files come from the snapshot, files_root)
    let root = if mode == Mode::Landed { card_scope::chorus_home() } else { card.werk.clone() };
    std::env::set_var("CHORUS_ROOT", &root);
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
