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
    /// #4536 — the retirement lines this card added, as a file: the deploy runs
    /// these, never the whole ledger.
    pub retirements: Option<String>,
    /// #4576 — the row-change lines this card added (designing/schemas/row-changes.jsonl),
    /// as a file: each is one call through athena-make, posted as its owner.
    pub row_changes: Option<String>,
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

/// #4536 — the ledger lines a card added: lines in `werk` that `base` does not
/// hold. Run 11 executed 16 old ledger retirements in staging (deploys-domain,
/// borg, chorus, …) that prod still holds, and counted the edges they left
/// dangling as the card's. Pure.
pub fn added_lines(werk: &str, base: &str) -> String {
    let old: std::collections::HashSet<&str> = base.lines().map(str::trim).collect();
    werk.lines().filter(|l| !l.trim().is_empty() && !old.contains(l.trim())).map(|l| format!("{l}\n")).collect()
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
    let dir = std::env::temp_dir().join(format!("athena-deploy-card-{}-{}", card.card, std::process::id())).display().to_string();
    std::fs::create_dir_all(&dir).map_err(|e| CardError::Failed(format!("{dir}: {e}")))?;
    p.scratch = Some(dir.clone());
    // #4536 — the deploy runs only the retirement lines this card added
    let ledger = "designing/schemas/model-retirements.jsonl";
    let added = if card.sources.iter().any(|s| s == ledger) {
        let werk_text = std::fs::read_to_string(card.abs(ledger)).unwrap_or_default();
        let git = if card.commit.is_some() { card_scope::chorus_home() } else { card.werk.clone() };
        let base = Command::new("git").args(["-C", &git, "show", &format!("{}:{ledger}", card_scope::base_rev(&card))]).output()
            .ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        added_lines(&werk_text, &base)
    } else {
        String::new()
    };
    let rfile = format!("{dir}/retirements.jsonl");
    std::fs::write(&rfile, added).map_err(|e| CardError::Failed(format!("{rfile}: {e}")))?;
    p.retirements = Some(rfile);
    // #4576 — and only the row changes this card added: rows go through the door
    // when the card that changes them lands, never re-seeded from a file (Jeff 10-06).
    let rows_ledger = "designing/schemas/row-changes.jsonl";
    let rows_added = if card.sources.iter().any(|s| s == rows_ledger) {
        let werk_text = std::fs::read_to_string(card.abs(rows_ledger)).unwrap_or_default();
        let git = if card.commit.is_some() { card_scope::chorus_home() } else { card.werk.clone() };
        let base = Command::new("git").args(["-C", &git, "show", &format!("{}:{rows_ledger}", card_scope::base_rev(&card))]).output()
            .ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        added_lines(&werk_text, &base)
    } else {
        String::new()
    };
    let wfile = format!("{dir}/row-changes.jsonl");
    std::fs::write(&wfile, rows_added).map_err(|e| CardError::Failed(format!("{wfile}: {e}")))?;
    p.row_changes = Some(wfile);
    // #4536 — an ontology file loads only the subjects the card changed
    if !p.ontology.is_empty() {
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
    if let Some(r) = &p.retirements {
        std::env::set_var("RETIREMENTS_FILE", r);
    }
    out.push(crate::run_athena_deploy_plan(Some(&p)).map_err(CardError::Failed)?);

    // #4578 — staging serves the card's own athena-make when the run built one
    // (only when the card changed athena-make: the role slot's build, which the
    // pipeline's tools step installed); the restart
    // below picks it up through the launcher's pointer file. Prod never does.
    // #4578 reopen — and its own athena-model (the DAL the door runs for every
    // write): #4576 changed the DAL's stamps and staging kept writing with the
    // installed one, so a run could not prove them.
    if staging {
        for (name, pointer) in [("athena-make", staging_bin_pointer()), ("athena-model", staging_model_pointer())] {
            let card_bin = card_slot_bin(&card, name).map_err(CardError::Refused)?;
            if let Some(line) = point_staging_at(card_bin.as_deref(), &pointer)
                .map_err(CardError::Refused)? {
                out.push(line);
            }
        }
    }
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
    // #4576 — the card's row changes, through the door it just restarted
    if let Some(f) = &p.row_changes {
        let text = std::fs::read_to_string(f).unwrap_or_default();
        out.extend(post_row_changes(&text, &api).map_err(CardError::Failed)?);
    }
    Ok(out.join("\n"))
}

/// #4578 — the build of `name` (athena-make, athena-model) staging should run for
/// this card, or None. Only a werk card whose diff touches that crate has a build
/// of its own; every other card keeps staging on the installed binary (a role
/// slot can be weeks older than prod). A landed card never points staging anywhere.
fn card_slot_bin(card: &Card, name: &str) -> Result<Option<String>, String> {
    if card.commit.is_some() {
        return Ok(None);
    }
    let git = |args: &[&str]| -> String {
        Command::new("git").arg("-C").arg(&card.werk).args(args).output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
    };
    let base = card_scope::base_rev(card);
    let src = format!("platform/services/{name}");
    let changed = !git(&["diff", "--name-only", &base, "--", &src]).is_empty();
    let last = git(&["log", "-1", "--format=%ct", &format!("{base}..HEAD"), "--", &src]).parse::<u64>().ok();
    let slot = slot_bin(&card.werk, name);
    let built = std::fs::metadata(&slot).and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
    staging_card_bin(changed, &slot, built, last)
}

/// The role slot next to a werk: `<base>/<role>-<card>` -> `<base>/<role>-bin/<name>`.
pub fn slot_bin(werk: &str, name: &str) -> String {
    let p = std::path::Path::new(werk);
    let slot = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let role = slot.rsplit_once('-').map(|(r, _)| r).unwrap_or(slot);
    let base = p.parent().map(|b| b.display().to_string()).unwrap_or_default();
    format!("{base}/{role}-bin/{name}")
}

/// Unix seconds as `YYYY-MM-DDTHH:MM:SSZ` (civil-from-days; no date crate here).
pub fn iso_utc(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Pure: whether staging serves the slot build. Not changed = None. Changed, but
/// the slot build is missing or older than the card's last athena-make commit =
/// REFUSED, naming both times, so a run never proves a build the card replaced.
pub fn staging_card_bin(changed: bool, slot: &str, built: Option<u64>, last_commit: Option<u64>) -> Result<Option<String>, String> {
    if !changed {
        return Ok(None);
    }
    let name = std::path::Path::new(slot).file_name().and_then(|n| n.to_str()).unwrap_or("the crate");
    let Some(built) = built else {
        return Err(format!("the card changes {name} but {slot} does not exist — run the tools step (werk-deploy --target werk) first (#4578)"));
    };
    if let Some(last) = last_commit.filter(|l| *l > built) {
        return Err(format!("{slot} was built at {}, before the card's last {name} commit at {} — rebuild it before staging serves it (#4578)", iso_utc(built), iso_utc(last)));
    }
    Ok(Some(slot.to_string()))
}

/// #4578 — the file the staging athena-make launcher reads to serve a card's
/// build (com.chorus.athena-make.staging names it as ATHENA_MAKE_BIN_FILE).
pub fn staging_bin_pointer() -> String {
    env_or("ATHENA_MAKE_BIN_FILE", &format!("{}/.chorus/staging/athena-make.bin", std::env::var("HOME").unwrap_or_default()))
}

/// #4578 reopen — the file naming the card's athena-model, the DAL (the launcher exports
/// it as CHORUS_MODEL_BIN; com.chorus.athena-make.staging names it as ATHENA_MODEL_BIN_FILE).
pub fn staging_model_pointer() -> String {
    env_or("ATHENA_MODEL_BIN_FILE", &format!("{}/.chorus/staging/athena-model.bin", std::env::var("HOME").unwrap_or_default()))
}

/// Point staging at `card_bin` (None or empty = the card built none: the pointer
/// is removed, so a run that died before its release cannot leave the next card
/// on its build). A path that is not an executable file is REFUSED here, before
/// any restart, so the run never proves yesterday's binary while saying it
/// proved its own.
pub fn point_staging_at(card_bin: Option<&str>, pointer: &str) -> Result<Option<String>, String> {
    let Some(bin) = card_bin.map(str::trim).filter(|b| !b.is_empty()) else {
        if std::path::Path::new(pointer).exists() {
            std::fs::remove_file(pointer).map_err(|e| format!("{pointer}: {e}"))?;
        }
        return Ok(None);
    };
    let exec = std::fs::metadata(bin).map(|m| {
        use std::os::unix::fs::PermissionsExt;
        m.is_file() && m.permissions().mode() & 0o111 != 0
    }).unwrap_or(false);
    if !exec {
        return Err(format!("the card build '{bin}' is not an executable file — staging would run the installed one (#4578)"));
    }
    if let Some(dir) = std::path::Path::new(pointer).parent() { let _ = std::fs::create_dir_all(dir); }
    std::fs::write(pointer, format!("{bin}\n")).map_err(|e| format!("{pointer}: {e}"))?;
    let name = std::path::Path::new(bin).file_name().and_then(|n| n.to_str()).unwrap_or("athena-make");
    Ok(Some(format!("staging {name}: serving the card build {bin}")))
}

/// #4578 — `athena-deploy staging release`: staging goes back to the installed
/// athena-make. Removes the pointer and restarts the staging unit. Safe to run
/// when nothing was pointed (the end of every Athena run calls it).
pub fn release_staging() -> Result<String, String> {
    let mut had = false;
    for pointer in [staging_bin_pointer(), staging_model_pointer()] {
        if std::path::Path::new(&pointer).exists() {
            std::fs::remove_file(&pointer).map_err(|e| format!("{pointer}: {e}"))?;
            had = true;
        }
    }
    let api = env_or("ATHENA_MAKE_STAGING_URL", "http://localhost:3368");
    let label = env_or("ATHENA_MAKE_STAGING_LABEL", "com.chorus.athena-make.staging");
    if !had { return Ok("staging athena-make: already the installed binary".into()); }
    let serve = env_or("ATHENA_SERVE_BIN", "athena-serve");
    let s = Command::new(&serve).args([label.as_str(), &format!("{}/health", api.trim_end_matches('/')), "--kickstart", "--timeout", "90"])
        .output().map_err(|e| format!("{serve} not runnable: {e}"))?;
    if !s.status.success() {
        return Err(format!("{serve} {label}: {} — staging released but not healthy", String::from_utf8_lossy(&s.stderr).trim()));
    }
    Ok("staging athena-make: released, back on the installed binary".into())
}

#[cfg(test)]
mod staging_pointer_tests {
    use super::*;

    /// #4578 — NEGATIVE PROOF: a card build that is missing, or not executable,
    /// is refused and the pointer is never written; no build = nothing written.
    #[test]
    fn staging_points_only_at_a_real_card_build() {
        let dir = std::env::temp_dir().join(format!("ptr-4578-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ptr = dir.join("athena-make.bin").display().to_string();
        assert_eq!(point_staging_at(None, &ptr).unwrap(), None);
        assert_eq!(point_staging_at(Some("  "), &ptr).unwrap(), None);
        assert!(!std::path::Path::new(&ptr).exists());
        let e = point_staging_at(Some("/nonexistent/athena-make"), &ptr).unwrap_err();
        assert!(e.contains("not an executable file"), "{e}");
        let plain = dir.join("plain"); std::fs::write(&plain, "x").unwrap();
        assert!(point_staging_at(Some(&plain.display().to_string()), &ptr).is_err(), "a non-executable file is refused");
        assert!(!std::path::Path::new(&ptr).exists(), "a refused build writes no pointer");
        let bin = dir.join("athena-make");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let line = point_staging_at(Some(&bin.display().to_string()), &ptr).unwrap().unwrap();
        assert!(line.contains("serving the card build"));
        assert_eq!(std::fs::read_to_string(&ptr).unwrap().trim(), bin.display().to_string());
        // NEGATIVE PROOF: the next card that built none clears an earlier card's pointer
        assert_eq!(point_staging_at(None, &ptr).unwrap(), None);
        assert!(!std::path::Path::new(&ptr).exists(), "a stale pointer would serve another card's build");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #4578 — staging serves a slot build only for a card that changed athena-make,
    /// and only when that build is newer than the card's last athena-make commit.
    #[test]
    fn staging_serves_the_slot_build_only_when_the_card_changed_it_and_it_is_fresh() {
        let slot = "/w/wren-bin/athena-make";
        assert_eq!(staging_card_bin(false, slot, Some(1), Some(9)).unwrap(), None, "a card that never touched athena-make");
        assert_eq!(staging_card_bin(true, slot, Some(10), Some(9)).unwrap().as_deref(), Some(slot));
        assert_eq!(staging_card_bin(true, slot, Some(10), None).unwrap().as_deref(), Some(slot), "uncommitted change, built after");
        // NEGATIVE PROOF: changed, but the slot is missing or older than the card's commit
        assert!(staging_card_bin(true, slot, None, Some(9)).unwrap_err().contains("does not exist"));
        let e = staging_card_bin(true, slot, Some(5), Some(9)).unwrap_err();
        assert!(e.contains("built at 1970-01-01T00:00:05Z") && e.contains("commit at 1970-01-01T00:00:09Z"), "{e}");
        assert_eq!(iso_utc(1_792_243_800), "2026-10-17T13:30:00Z");
        assert_eq!(slot_bin("/w/wren-4578", "athena-make"), slot);
        assert_eq!(slot_bin("/w/abby-normal-4530", "athena-make"), "/w/abby-normal-bin/athena-make");
        // #4578 reopen — the DAL's slot build, refused by its own name when stale
        let model = slot_bin("/w/wren-4576", "athena-model");
        assert_eq!(model, "/w/wren-bin/athena-model");
        let e = staging_card_bin(true, &model, Some(5), Some(9)).unwrap_err();
        assert!(e.contains("last athena-model commit"), "{e}");
    }
}

/// #4576 — one staged row change: a call through athena-make, made as `owner`.
/// `{"method":"POST|PATCH|PUT","path":"/v1/<domain>/<class>[/<name>]","owner":"<role>",
///   "card":"<n>","status":"staged","body":{...}}` with `body` the LAST key.
#[derive(Debug, PartialEq)]
pub struct RowChange { pub method: String, pub path: String, pub owner: String, pub body: String }

pub fn parse_row_change(line: &str) -> Result<RowChange, String> {
    let t = line.trim();
    let field = |k: &str| -> Option<String> {
        let at = t.find(&format!("\"{k}\""))? + k.len() + 2;
        let rest = t[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
        Some(rest[..rest.find('"')?].to_string())
    };
    let method = field("method").ok_or("row change has no method")?;
    if !["POST", "PATCH", "PUT"].contains(&method.as_str()) {
        return Err(format!("row change method {method:?}: only POST, PATCH or PUT (a delete is a retirement)"));
    }
    let path = field("path").ok_or("row change has no path")?;
    if !path.starts_with("/v1/") || path.contains("..") || path.contains('?') {
        return Err(format!("row change path {path:?} is not a /v1/ door path"));
    }
    let owner = field("owner").filter(|o| !o.is_empty() && o.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .ok_or("row change has no owner (rows post as their owner)")?;
    let at = t.find("\"body\"").ok_or("row change has no body")?;
    let body = t[at + 6..].trim_start().strip_prefix(':').ok_or("row change body has no value")?.trim();
    let body = body.strip_suffix('}').ok_or("row change is not a JSON object")?.trim();
    if !body.starts_with('{') || !body.ends_with('}') {
        return Err("row change body must be a JSON object and the last key".into());
    }
    Ok(RowChange { method, path, owner, body: body.to_string() })
}

/// Post every line, in order, each as its owner. A POST the door answers 409
/// (the row is already there: a re-run on the same staging copy) is retried as
/// PUT to the row's own path, so a re-run converges instead of failing. Any other
/// refusal stops the run, naming the line and the door's words.
pub fn post_row_changes(text: &str, api: &str) -> Result<Vec<String>, String> {
    let root = env_or("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus");
    let mut out = Vec::new();
    for (i, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let rc = parse_row_change(line).map_err(|e| format!("row-changes line {}: {e}", i + 1))?;
        let id = Command::new(format!("{root}/platform/scripts/chorus-identity-token")).arg(&rc.owner).output()
            .ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("row-changes line {}: no identity for owner '{}'", i + 1, rc.owner))?;
        let send = |method: &str, path: &str| -> (String, String) {
            let url = format!("{}{}", api.trim_end_matches('/'), path);
            let auth = format!("Authorization: Bearer {id}");
            let o = Command::new("curl").args(["-s", "-m", "30", "-X", method, "-H", "Content-Type: application/json",
                "-H", &auth, "--data-binary", &rc.body, "-w", "\n%{http_code}", &url]).output();
            let text = o.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
            let (b, c) = text.rsplit_once('\n').unwrap_or(("", text.as_str()));
            (c.trim().to_string(), b.trim().to_string())
        };
        let (mut code, mut said) = send(&rc.method, &rc.path);
        if rc.method == "POST" && code == "409" {
            if let Some(name) = body_name(&rc.body) {
                let row = format!("{}/{name}", rc.path.trim_end_matches('/'));
                (code, said) = send("PUT", &row);
            }
        }
        if !code.starts_with('2') {
            return Err(format!("row-changes line {} ({} {} as {}): HTTP {code}: {said}", i + 1, rc.method, rc.path, rc.owner));
        }
        out.push(format!("row-change {} {} {} as {}: HTTP {code}", i + 1, rc.method, rc.path, rc.owner));
    }
    Ok(out)
}

fn body_name(body: &str) -> Option<String> {
    let at = body.find("\"name\"")? + 6;
    let rest = body[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

#[cfg(test)]
mod row_change_tests {
    use super::*;

    /// #4576 — NEGATIVE PROOFS: a delete, a non-door path, an ownerless line and a
    /// body that is not last are each refused by name; a good line parses whole.
    #[test]
    fn row_change_lines_parse_or_are_refused_by_name() {
        let ok = r#"{"method":"POST","path":"/v1/roles/teammates","owner":"wren","card":"4576","status":"staged","body":{"name":"teammate-kade","signsInAs":"principal-kade"}}"#;
        let rc = parse_row_change(ok).unwrap();
        assert_eq!(rc, RowChange { method: "POST".into(), path: "/v1/roles/teammates".into(), owner: "wren".into(),
            body: r#"{"name":"teammate-kade","signsInAs":"principal-kade"}"#.into() });
        assert_eq!(body_name(&rc.body).as_deref(), Some("teammate-kade"));
        let bad = |l: &str, want: &str| { let e = parse_row_change(l).unwrap_err(); assert!(e.contains(want), "{l} -> {e}"); };
        bad(r#"{"method":"DELETE","path":"/v1/a/b/c","owner":"wren","body":{}}"#, "a delete is a retirement");
        bad(r#"{"method":"PATCH","path":"/other/a/b","owner":"wren","body":{}}"#, "not a /v1/ door path");
        bad(r#"{"method":"PATCH","path":"/v1/a/b/c","body":{}}"#, "no owner");
        bad(r#"{"method":"PATCH","path":"/v1/a/b/c","owner":"wren","body":{"x":"y"},"card":"1"}"#, "last key");
    }
}
