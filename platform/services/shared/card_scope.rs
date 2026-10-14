// card_scope — #4515: which card an athena verb works for.
//
// `--card <n>` is one rule for athena-validate, athena-gen and athena-deploy:
// the card's werk is $CHORUS_WERK_BASE/<role>-<n> (role = CHORUS_ROLE, else
// ROLE), and the card's model change is what `athena-deploy scope` calls a model
// source, read from the werk's diff since it left main. A missing werk, or a card
// whose diff touches no model source, is REFUSED by name: a verb that silently
// finds nothing to check would pass every card.
//
// Included (`include!`) by the three crates. Std only; git and riot run as
// subprocesses, like everywhere else in the athena verbs.

/// The one definition of "model source" (shared/model_scope.rs, #4186).
pub mod card_model_scope {
    include!("model_scope.rs");
}

/// The range a card's change is read from: everything the werk's branch did
/// since it left main. Three dots, not two: `origin/main..HEAD` in `git diff`
/// compares the two TREES, so a model file another card landed on main after
/// this werk was cut reads as this card's change (and a card with no model
/// change at all would deploy someone else's).
pub const CARD_RANGE: &str = "origin/main...HEAD";

/// The card's werk and its model change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub card: String,
    pub role: String,
    pub werk: String,
    /// Werk-relative model sources the card changed (TTL and the retirements file).
    pub sources: Vec<String>,
}

impl Card {
    /// The card's changed TTL files, werk-relative.
    pub fn ttl(&self) -> Vec<String> {
        self.sources.iter().filter(|s| s.ends_with(".ttl")).cloned().collect()
    }
    /// A werk-relative path made absolute.
    pub fn abs(&self, rel: &str) -> String {
        format!("{}/{}", self.werk.trim_end_matches('/'), rel)
    }
}

/// `--card <n>` from argv. None = not asked; Some(Err) = asked without a value.
pub fn card_arg(args: &[String]) -> Option<Result<String, String>> {
    let i = args.iter().position(|a| a == "--card")?;
    Some(match args.get(i + 1) {
        Some(v) if !v.starts_with("--") && !v.is_empty() => Ok(v.trim_start_matches('#').to_string()),
        _ => Err("--card needs a card number".into()),
    })
}

/// Where a role's card werk lives.
pub fn werk_path(base: &str, role: &str, card: &str) -> String {
    format!("{}/{}-{}", base.trim_end_matches('/'), role, card)
}

/// The werk base from env, with the same fallback the old athena.yml used.
pub fn werk_base() -> String {
    std::env::var("CHORUS_WERK_BASE").ok().filter(|b| !b.is_empty()).unwrap_or_else(|| {
        format!("{}/CascadeProjects/chorus-werk", std::env::var("HOME").unwrap_or_default())
    })
}

/// The role from env: CHORUS_ROLE, else ROLE.
pub fn env_role() -> Option<String> {
    ["CHORUS_ROLE", "ROLE"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
}

/// Resolve a card from the environment.
pub fn resolve(card: &str) -> Result<Card, String> {
    resolve_from(card, env_role(), &werk_base())
}

/// Resolve a card: its werk must exist and its diff since main must touch a
/// model source. Every refusal names the card.
pub fn resolve_from(card: &str, role: Option<String>, base: &str) -> Result<Card, String> {
    let card = card.trim().trim_start_matches('#').to_string();
    if card.is_empty() || !card.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("card '{card}': not a card number — refusing"));
    }
    let role = role.ok_or_else(|| format!("card #{card}: no role — set CHORUS_ROLE (or ROLE) so the werk can be found"))?;
    let werk = werk_path(base, &role, &card);
    if !std::path::Path::new(&werk).is_dir() {
        return Err(format!("card #{card}: no werk at {werk} — nothing to scope, no model change to check"));
    }
    let sources = changed_sources(&werk, CARD_RANGE).map_err(|e| format!("card #{card}: {e}"))?;
    if sources.is_empty() {
        return Err(format!(
            "card #{card}: no model change — {CARD_RANGE} in {werk} touches no model source (roles/*/ontology/*.ttl, designing/schemas/model-retirements.jsonl)"
        ));
    }
    Ok(Card { card, role, werk, sources })
}

/// The model sources a range touched in a tree. Deleted files are not sources
/// (#4064: their rows leave through the retirement ledger). An unreadable range
/// is an error, never an empty list.
pub fn changed_sources(root: &str, range: &str) -> Result<Vec<String>, String> {
    let out = std::process::Command::new("git")
        .args(["-C", root, "diff", "--name-only", "--diff-filter=d", range])
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git diff --name-only {range} failed in {root}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(card_model_scope::changed_model_sources(&String::from_utf8_lossy(&out.stdout)))
}

fn riot_bin() -> String {
    std::env::var("RIOT_BIN").ok().filter(|b| !b.is_empty()).unwrap_or_else(|| "riot".into())
}

/// riot's parse check on one file. Err carries riot's own words.
pub fn riot_validate(path: &str) -> Result<(), String> {
    let out = std::process::Command::new(riot_bin())
        .arg("--validate")
        .arg(path)
        .output()
        .map_err(|e| format!("riot not runnable ({e}) — unmeasured"))?;
    let msg = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() && !msg.contains("ERROR") {
        return Ok(());
    }
    Err(msg.lines().find(|l| l.contains("ERROR")).unwrap_or(msg.trim()).trim().to_string())
}

/// Several files as one N-Triples text, through riot.
pub fn ntriples(paths: &[String]) -> Result<String, String> {
    if paths.is_empty() {
        return Ok(String::new());
    }
    let out = std::process::Command::new(riot_bin())
        .arg("--quiet")
        .arg("--output=nt")
        .args(paths)
        .output()
        .map_err(|e| format!("riot not runnable ({e}) — unmeasured"))?;
    if !out.status.success() {
        return Err(format!("riot could not read the model: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every TTL a tree's model is written in: roles/*/ontology, designing/data,
/// designing/ontology. Sorted.
pub fn model_tree_files(root: &str) -> Vec<String> {
    let root = std::path::Path::new(root);
    let mut dirs = vec![root.join("designing/data"), root.join("designing/ontology")];
    if let Ok(rd) = std::fs::read_dir(root.join("roles")) {
        for r in rd.flatten() {
            dirs.push(r.path().join("ontology"));
        }
    }
    let mut out = Vec::new();
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for f in rd.flatten() {
                let p = f.path();
                if p.extension().is_some_and(|e| e == "ttl") {
                    out.push(p.display().to_string());
                }
            }
        }
    }
    out.sort();
    out
}

const RDF_TYPE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
const CLASS_TYPES: &[&str] = &["<http://www.w3.org/2002/07/owl#Class>", "<http://www.w3.org/2000/01/rdf-schema#Class>"];
const NODE_SHAPE: &str = "<http://www.w3.org/ns/shacl#NodeShape>";
const TARGET_CLASS: &str = "<http://www.w3.org/ns/shacl#targetClass>";

fn nt_triple(line: &str) -> Option<(&str, &str, &str)> {
    let mut it = line.split_whitespace();
    let s = it.next().filter(|s| s.starts_with('<'))?;
    let p = it.next()?;
    let o = it.next().filter(|o| o.starts_with('<'))?;
    Some((s, p, o))
}

/// The local name of `<...#Name>` or `<.../Name>`.
pub fn local_name(iri: &str) -> String {
    iri.trim_matches(|c| c == '<' || c == '>').rsplit(['#', '/']).next().unwrap_or("").to_string()
}

/// The classes N-Triples declare (`?c a owl:Class` or `rdfs:Class`), as full
/// IRIs in angle brackets, sorted and unique. Blank-node classes are not named
/// classes and are left out.
pub fn declared_classes(nt: &str) -> Vec<String> {
    let mut out: Vec<String> = nt
        .lines()
        .filter_map(nt_triple)
        .filter(|(_, p, o)| *p == RDF_TYPE && CLASS_TYPES.contains(o))
        .map(|(s, _, _)| s.to_string())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The shape a class has in the model, by local name: a node shape whose
/// sh:targetClass is the class, else a node shape named `<Class>Shape` in the
/// class's namespace (the old athena.yml `shape` step's rule). None = no shape.
pub fn shape_for(class_iri: &str, model_nt: &str) -> Option<String> {
    let triples: Vec<(&str, &str, &str)> = model_nt.lines().filter_map(nt_triple).collect();
    let is_shape = |s: &str| triples.iter().any(|(ss, p, o)| *ss == s && *p == RDF_TYPE && *o == NODE_SHAPE);
    if let Some((s, _, _)) = triples.iter().find(|(s, p, o)| *p == TARGET_CLASS && *o == class_iri && is_shape(s)) {
        return Some(local_name(s));
    }
    let named = format!("{}Shape>", class_iri.trim_end_matches('>'));
    triples
        .iter()
        .find(|(s, p, o)| *s == named && *p == RDF_TYPE && *o == NODE_SHAPE)
        .map(|(s, _, _)| local_name(s))
}
