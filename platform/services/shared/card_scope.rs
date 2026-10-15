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
    /// #4515 landed mode: the card's squash commit on origin/main. `werk` is then
    /// canonical ($CHORUS_HOME) and `sources` what that one commit changed.
    pub commit: Option<String>,
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
    Ok(Card { card, role, werk, sources, commit: None })
}

// ---- #4515 landed mode: `--card <n> --landed` ----
//
// The athena land runs AFTER the werk land: the werk may already be torn down
// (werk-accept), so the card's change is read from canonical instead, as the
// one commit the land squashed onto origin/main. Range `<sha>^..<sha>`.

/// `--landed` in argv.
pub fn landed_arg(args: &[String]) -> bool {
    args.iter().any(|a| a == "--landed")
}

/// Canonical: $CHORUS_HOME, else ~/CascadeProjects/chorus.
pub fn chorus_home() -> String {
    std::env::var("CHORUS_HOME").ok().filter(|h| !h.is_empty()).unwrap_or_else(|| {
        format!("{}/CascadeProjects/chorus", std::env::var("HOME").unwrap_or_default())
    })
}

/// The card's squash commit on origin/main, by the land's subject: "#<card>
/// (<role>) (#<pr>)" or "<role>: #<card> — <summary> (#<pr>)" (the same grep
/// scripts/athena-land uses). Ok(None) = the card has not landed. #77 is never
/// #770: the subject must carry the number followed by " (" or " ".
pub fn landed_commit(home: &str, card: &str, role: &str) -> Result<Option<String>, String> {
    let out = std::process::Command::new("git")
        .args(["-C", home, "log", "origin/main", "-1", "--format=%H"])
        .arg(format!("--grep=^#{card} ({role})"))
        .arg(format!("--grep=^{role}: #{card} "))
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git log origin/main failed in {home}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok((!sha.is_empty()).then_some(sha))
}

/// Resolve a landed card from the environment (canonical = $CHORUS_HOME).
pub fn resolve_landed(card: &str) -> Result<Card, String> {
    resolve_landed_from(card, env_role(), &chorus_home())
}

/// Resolve a landed card: its squash commit must be on origin/main in `home`.
/// No landed commit is REFUSED by name. A landed commit that touches no model
/// source is NOT refused here: it resolves with no sources, and the caller says
/// "none" (the land had nothing for athena; that is not an error after the fact).
pub fn resolve_landed_from(card: &str, role: Option<String>, home: &str) -> Result<Card, String> {
    let card = card.trim().trim_start_matches('#').to_string();
    if card.is_empty() || !card.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("card '{card}': not a card number — refusing"));
    }
    let role = role.ok_or_else(|| format!("card #{card}: no role — set CHORUS_ROLE (or ROLE) so the landed commit can be found"))?;
    if !std::path::Path::new(home).is_dir() {
        return Err(format!("card #{card}: no canonical tree at {home}"));
    }
    let sha = landed_commit(home, &card, &role)
        .map_err(|e| format!("card #{card}: {e}"))?
        .ok_or_else(|| format!("card #{card}: no landed commit for #{card} ({role}) on origin/main in {home} — nothing landed to deploy"))?;
    let range = format!("{sha}^..{sha}");
    let sources = changed_sources(home, &range).map_err(|e| format!("card #{card}: {e}"))?;
    Ok(Card { card, role, werk: home.trim_end_matches('/').to_string(), sources, commit: Some(sha) })
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

/// #4531 — classes whose instances are SHACL shapes (`rdfs:subClassOf
/// sh:NodeShape`), e.g. ToolInput: the arguments a tool takes. They are schema,
/// not rows, so no collection route serves them and a served check skips them.
pub fn shape_classes(nt: &str) -> Vec<String> {
    let mut out: Vec<String> = nt
        .lines()
        .filter_map(nt_triple)
        .filter(|(_, p, o)| {
            *p == "<http://www.w3.org/2000/01/rdf-schema#subClassOf>" && *o == "<http://www.w3.org/ns/shacl#NodeShape>"
        })
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

// #4547 reopen 1 — what a card changed, judged by triples rather than by file.
// Written by Kade on #4536 (athena-validate card.rs, bb8f85e71, normalised in
// 305cceea3) and moved here so validate and gen use one rule: a class or row
// is the card's only when its triples differ from main's.

/// Main's version of a card file as N-Triples. Empty when main has no such file,
/// so every class in it is the card's.
pub fn main_ntriples(werk: &str, rel: &str) -> String {
    ntriples_at(werk, "origin/main", rel)
}

/// #4536 — the revision a card's change is judged against: main for a werk,
/// the squash commit's parent for a landed card (main already holds the change).
pub fn base_rev(card: &Card) -> String {
    match &card.commit {
        Some(sha) => format!("{sha}^"),
        None => "origin/main".to_string(),
    }
}

/// A file as it was at `rev`, as N-Triples. Empty when `rev` has no such file.
pub fn ntriples_at(werk: &str, rev: &str, rel: &str) -> String {
    let out = std::process::Command::new("git")
        .args(["-C", werk, "show", &format!("{rev}:{rel}")])
        .output();
    let Ok(out) = out else { return String::new() };
    if !out.status.success() {
        return String::new();
    }
    let tmp = std::env::temp_dir().join(format!("card-scope-main-{}-{}.ttl", std::process::id(), rel.replace('/', "_")));
    if std::fs::write(&tmp, &out.stdout).is_err() {
        return String::new();
    }
    let nt = ntriples(&[tmp.to_string_lossy().into_owned()]).unwrap_or_default();
    let _ = std::fs::remove_file(&tmp);
    nt
}

/// The triples about one subject (`<iri>` with its angle brackets), sorted.
/// Blank-node labels differ per parse (`_:b0` vs `_:b7`), so they are
/// normalised first: the same list is not a change.
fn triples_about(nt: &str, subject: &str) -> Vec<String> {
    let key = format!("{subject} ");
    let bnode = |l: &str| l.split_whitespace().map(|t| if t.starts_with("_:") { "_:b" } else { t }).collect::<Vec<_>>().join(" ");
    let mut v: Vec<String> = nt.lines().filter(|l| l.starts_with(&key)).map(bnode).collect();
    v.sort();
    v
}

/// Classes declared in `werk_nt` that main does not declare with the same
/// triples about them: added, or changed.
pub fn changed_classes(werk_nt: &str, main_nt: &str) -> Vec<String> {
    declared_classes(werk_nt).into_iter().filter(|c| triples_about(werk_nt, c) != triples_about(main_nt, c)).collect()
}

/// Every named subject in `werk_nt` whose triples differ from main's: the rows
/// and shapes a card added or changed. Sorted, with angle brackets.
pub fn changed_subjects(werk_nt: &str, main_nt: &str) -> Vec<String> {
    // every line's subject, whatever its object (nt_triple keeps IRI objects only)
    let mut subjects: Vec<String> =
        werk_nt.lines().filter_map(|l| l.split_whitespace().next()).filter(|s| s.starts_with('<')).map(str::to_string).collect();
    subjects.sort();
    subjects.dedup();
    subjects.into_iter().filter(|s| triples_about(werk_nt, s) != triples_about(main_nt, s)).collect()
}

#[cfg(test)]
mod changed_4536 {
    use super::{changed_classes, changed_subjects};
    const OWL: &str = "<http://www.w3.org/2002/07/owl#Class>";
    const TYPE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    const LABEL: &str = "<http://www.w3.org/2000/01/rdf-schema#label>";
    fn class(n: &str, label: &str) -> String {
        format!("<urn:x#{n}> {TYPE} {OWL} .\n<urn:x#{n}> {LABEL} \"{label}\" .\n")
    }

    #[test]
    fn an_unchanged_class_from_main_is_not_the_cards() {
        let main = class("Old", "old");
        let werk = format!("{}{}", class("Old", "old"), class("New", "new"));
        assert_eq!(changed_classes(&werk, &main), vec!["<urn:x#New>".to_string()]);
    }

    #[test]
    fn a_class_whose_triples_changed_is_the_cards() {
        assert_eq!(changed_classes(&class("Old", "renamed"), &class("Old", "old")), vec!["<urn:x#Old>".to_string()]);
    }

    /// A list object parses to a fresh blank node each time; the same list is
    /// not a change (10-15: GateResult's mintKey list read as "changed").
    #[test]
    fn the_same_list_under_a_different_blank_node_is_not_a_change() {
        let main = format!("<urn:x#G> {TYPE} {OWL} .\n<urn:x#G> <urn:x#mintKey> _:b0 .\n");
        let werk = format!("<urn:x#G> {TYPE} {OWL} .\n<urn:x#G> <urn:x#mintKey> _:b7 .\n");
        assert!(changed_classes(&werk, &main).is_empty());
    }

    /// NEGATIVE PROOF: with no main version every declared class is the card's,
    /// so a new unshaped class can never hide behind the diff.
    #[test]
    fn negative_proof_a_new_file_answers_for_every_class() {
        let werk = format!("{}{}", class("A", "a"), class("B", "b"));
        assert_eq!(changed_classes(&werk, "").len(), 2);
    }

    #[test]
    fn changed_subjects_names_only_rows_whose_triples_moved() {
        let main = format!("{}<urn:x#row1> {LABEL} \"a\" .\n<urn:x#row2> {LABEL} \"b\" .\n", class("Old", "old"));
        let werk = format!("{}<urn:x#row1> {LABEL} \"a\" .\n<urn:x#row2> {LABEL} \"c\" .\n", class("Old", "old"));
        assert_eq!(changed_subjects(&werk, &main), vec!["<urn:x#row2>".to_string()]);
    }
}
