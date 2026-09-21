//! #4254 — generate the vocabulary from the model instead of authoring it.
//!
//! Jeff, 2026-09-21: "i expect all classes and attributes to be in scope" and
//! "not a new creation". Every class the model declares and every attribute a
//! shape constrains becomes a skos:Concept whose definition is the
//! `rdfs:comment` the model ALREADY carries. Nothing is invented: a term with
//! no comment is emitted WITHOUT a definition and counted as a gap, which is
//! the honest state rather than a sentence made up for it.
//!
//! TWO FILES, ONE GRAPH — Wren's point, and why this is structural rather than
//! a convention:
//!
//!   vocabulary-generated-4254.ttl   written whole, only ever by this program
//!   vocabulary-identity-4254.ttl    opened READ-ONLY here; no write path exists
//!
//! A marker inside one shared file would be one bad merge away from
//! regenerating Jeff's ruled collisions out of existence. Separate files and
//! disjoint IRIs (`vocab:gen-*` here, bare names there) make "the generator
//! does not overwrite the authored terms" true because no code could.
//!
//! EVERY SKIP IS NAMED, not merely counted — a skip nobody counts is the same
//! silence as a check that cannot fail. The two causes are reported apart,
//! because one number would hide which of them moved.
//!
//! REMOVAL IS THE STORE'S PROBLEM, NOT THIS FILE'S. Rewriting the file whole
//! makes a removed class vanish from it trivially, so proving removal here
//! would prove nothing. athena-deploy merges per subject and keeps siblings, so
//! a concept can survive in the graph after its class is gone (the #4250 ghost
//! class). The vocabulary set is therefore the one set flagged `replace` in the
//! manifest: it owns its graph outright, so delete-by-absence is safe by
//! construction, and the removal proof queries the STORE.
//!
//! Three sh:path terms are blank nodes — inStream, hasDomain, hosts, all
//! inverse paths. An inverse path has no IRI of its own, so it cannot carry an
//! exactMatch and cannot become a concept. That is why the attribute count is
//! 197 and not the 200 a bare sh:path query returns.
//!
//! COUNTS ARE PINNED at generation and written into the file. The same query
//! returned 197 at 11:52 and 198 at 12:11 on 2026-09-21 while another role was
//! writing the model: a coverage figure with no recorded denominator cannot
//! tell a numerator rising from a denominator shrinking.
//!
//! Run: vocab-gen [output.ttl]   (env: FUSEKI_QUERY, ONTOLOGY_GRAPH, CHORUS_ROOT)

use std::collections::BTreeMap;
use std::process::Command;

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Class,
    Attribute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTerm {
    pub iri: String,
    pub local: String,
    pub comment: Option<String>,
    pub kind: Kind,
}

pub fn local_name(iri: &str) -> String {
    iri.rsplit(['#', '/']).next().unwrap_or(iri).to_string()
}

/// Fold (iri, comment) rows into one term each. A term appears once per shape
/// that constrains it — `filePath` arrives seven times — so the first non-empty
/// comment wins and a later blank row never erases it.
pub fn terms_from_rows(rows: &[(String, String)], kind: Kind) -> Vec<ModelTerm> {
    let mut by_iri: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (iri, comment) in rows {
        if iri.trim().is_empty() {
            continue;
        }
        let slot = by_iri.entry(iri.trim().to_string()).or_insert(None);
        if slot.is_none() && !comment.trim().is_empty() {
            *slot = Some(comment.trim().to_string());
        }
    }
    by_iri
        .into_iter()
        .map(|(iri, comment)| ModelTerm { local: local_name(&iri), iri, comment, kind })
        .collect()
}

/// Escape a literal for Turtle. A model comment carrying a quote or a newline
/// would otherwise produce a file that does not parse, and the deploy would
/// refuse the whole set for a reason that reads as unrelated.
pub fn ttl_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
        .replace('\t', "\\t")
}

/// The prefLabels the hand-authored file already claims, lowercased. Read-only:
/// this is the only thing the generator ever does with that file.
pub fn claimed_labels(authored_ttl: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in authored_ttl.lines() {
        let code = line.split('#').next().unwrap_or("").trim();
        if let Some(rest) = code.strip_prefix("skos:prefLabel") {
            if let Some(w) = rest.split('"').nth(1) {
                out.push(w.to_lowercase());
            }
        }
    }
    out
}

pub struct Rendered {
    pub ttl: String,
    pub emitted: usize,
    pub undefined: usize,
    /// Named, not counted: labels the authored set already claims. If this
    /// grows, someone widened that set, and the names say which terms.
    pub skipped_authored: Vec<String>,
    /// A class and an attribute sharing one local name.
    pub skipped_duplicate: Vec<String>,
    /// Terms whose exactMatch points at something the store does not declare.
    ///
    /// A THIRD state, and the reason it is separate: a term with no exactMatch
    /// is ungrounded and counted as such, while a term pointing at a dead IRI
    /// looked grounded and was not. A check that cannot tell those two apart
    /// reports the second as fine forever (Wren found eleven, 2026-09-21).
    pub dangling: Vec<String>,
}

/// Is this IRI one the model is expected to declare? rdfs:label and
/// rdfs:comment are somebody else's vocabulary and always resolve to nothing
/// here — counting them would put two permanent entries in a list whose whole
/// purpose is that it can reach zero.
pub fn ours(iri: &str) -> bool {
    iri.starts_with("https://jeffbridwell.com/")
}

/// Render the generated TTL. Pure, so every counting rule is testable without
/// a store.
pub fn render(
    terms: &[ModelTerm],
    claimed: &[String],
    declared: &[String],
    stamp: &str,
) -> Rendered {
    let mut body = String::new();
    let (mut emitted, mut undefined) = (0usize, 0usize);
    let mut skipped_authored: Vec<String> = Vec::new();
    let mut skipped_duplicate: Vec<String> = Vec::new();
    let mut dangling: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for t in terms {
        let key = t.local.to_lowercase();
        if claimed.contains(&key) {
            skipped_authored.push(t.local.clone());
            continue; // the hand-authored decision wins
        }
        // A class and an attribute can share a local name. The second would be
        // a within-scheme collision reported on every deploy forever, so it is
        // skipped — and named, so nobody has to guess which one lost.
        if seen.contains(&key) {
            skipped_duplicate.push(t.local.clone());
            continue;
        }
        seen.push(key);

        let scheme = match t.kind {
            Kind::Class => "vocab:model-classes",
            Kind::Attribute => "vocab:model-attributes",
        };
        body.push_str(&format!(
            "\nvocab:gen-{} a skos:Concept ;\n    skos:inScheme {scheme} ;\n    skos:prefLabel \"{}\" ;\n",
            t.local,
            ttl_escape(&t.local)
        ));
        match &t.comment {
            Some(c) => body.push_str(&format!("    skos:definition \"{}\" ;\n", ttl_escape(c))),
            None => undefined += 1,
        }
        body.push_str(&format!("    skos:exactMatch <{}> .\n", t.iri));
        if ours(&t.iri) && !declared.contains(&t.iri) {
            dangling.push(t.local.clone());
        }
        emitted += 1;
    }

    let classes = terms.iter().filter(|t| t.kind == Kind::Class).count();
    let attrs = terms.iter().filter(|t| t.kind == Kind::Attribute).count();
    let authored_list =
        if skipped_authored.is_empty() { "none".into() } else { skipped_authored.join(", ") };
    let dup_list =
        if skipped_duplicate.is_empty() { "none".into() } else { skipped_duplicate.join(", ") };
    let dangling_list: String =
        if dangling.is_empty() { "none".into() } else { dangling.join(", ") };

    let header = format!(
        "# GENERATED by vocab-gen (#4254) at {stamp}. Do not hand-edit — the next\n\
# run overwrites this file whole. Every concept here is one class or one\n\
# attribute the model already declares, carrying the rdfs:comment the model\n\
# already has as its definition. Nothing is invented: a term with no comment\n\
# is emitted WITHOUT a definition and counted below, which is the honest state.\n\
#\n\
# COUNTS PINNED AT GENERATION — they move while you measure. The same query\n\
# returned 197 attributes at 11:52 and 198 at 12:11 on 2026-09-21, because\n\
# another role was writing the model between the two. A coverage figure with\n\
# no recorded denominator cannot tell a numerator rising from a denominator\n\
# shrinking, so both are written here rather than quoted from memory.\n\
#\n\
#   classes in the model      {classes}\n\
#   attributes in the model   {attrs}   (blank-node inverse paths excluded: an\n\
#                                        inverse path has no IRI of its own, so\n\
#                                        it cannot carry an exactMatch)\n\
#   concepts emitted          {emitted}\n\
#   of those, no definition   {undefined}\n\
#\n\
# SKIPS ARE NAMED, not just counted — a skip nobody counts is the same silence\n\
# as a check that cannot fail. Two causes, reported apart, because one number\n\
# would hide which of them moved:\n\
#\n\
#   skipped, label claimed by the authored set ({}):\n\
#     {authored_list}\n\
#   skipped, a class and an attribute share one name ({}):\n\
#     {dup_list}\n\
#\n\
# TERMS POINTING AT SOMETHING THAT IS NOT DECLARED ({}). This is a third state,\n\
# not a flavour of the two above: a term with NO exactMatch is ungrounded and\n\
# counted as such, while a term pointing at a dead IRI looked grounded and was\n\
# not. Most of these are properties a shape constrains that nothing declares —\n\
# a real gap in the model, found by generating from it. rdfs:* is excluded:\n\
# that is someone else's vocabulary and would sit in this list forever.\n\
#\n\
#     {dangling_list}\n\
#\n\
# The hand-authored file beside this one is NOT generated and is opened\n\
# read-only here. Those terms are the collisions — one idea with six spellings\n\
# — and which word wins is a decision, not a derivation. Separate files and\n\
# disjoint IRIs are why \"the generator does not overwrite them\" is structural\n\
# rather than a convention: there is no code that could.\n\n\
@prefix skos:    <http://www.w3.org/2004/02/skos/core#> .\n\
@prefix dcterms: <http://purl.org/dc/terms/> .\n\
@prefix vocab:   <urn:chorus:domains:vocabulary#> .\n\n\
vocab:model-classes a skos:ConceptScheme ;\n    \
skos:prefLabel \"Chorus model classes\" ;\n    \
dcterms:description \"Every class the model declares, as a term. Generated from definesVocabulary, so this scheme cannot drift from the model — nothing writes it by hand.\" .\n\n\
vocab:model-attributes a skos:ConceptScheme ;\n    \
skos:prefLabel \"Chorus model attributes\" ;\n    \
dcterms:description \"Every attribute a shape constrains, as a term. Generated from sh:path.\" .\n",
        skipped_authored.len(),
        skipped_duplicate.len(),
        dangling.len()
    );

    Rendered { ttl: header + &body, emitted, undefined, skipped_authored, skipped_duplicate, dangling }
}

/// Split a two-column CSV line, honouring one level of quoting — a model
/// comment routinely contains commas.
pub fn split_csv2(line: &str) -> (String, String) {
    let mut cols: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_q && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => in_q = !in_q,
            ',' if !in_q => cols.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    cols.push(cur);
    (cols.first().cloned().unwrap_or_default(), cols.get(1).cloned().unwrap_or_default())
}

fn query_csv(endpoint: &str, sparql: &str) -> Vec<(String, String)> {
    let out = Command::new("curl")
        .args([
            "-s",
            "--data-urlencode",
            &format!("query={sparql}"),
            "-H",
            "Accept: text/csv",
            endpoint,
        ])
        .output()
        .expect("curl runs");
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().skip(1).map(split_csv2).collect()
}

fn main() {
    let query = env_or("FUSEKI_QUERY", "http://localhost:3030/pods/query");
    let graph = env_or("ONTOLOGY_GRAPH", "urn:chorus:ontology");
    let root = env_or("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus");
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("{root}/roles/silas/ontology/vocabulary-generated-4254.ttl"));

    let class_q = format!(
        "PREFIX chorus: <https://jeffbridwell.com/chorus#> \
         PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> \
         SELECT ?t ?c WHERE {{ GRAPH <{graph}> {{ ?d chorus:definesVocabulary ?t . \
         OPTIONAL {{ ?t rdfs:comment ?c }} }} }}"
    );
    let attr_q = format!(
        "PREFIX sh: <http://www.w3.org/ns/shacl#> \
         PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> \
         SELECT ?t ?c WHERE {{ GRAPH <{graph}> {{ ?b sh:path ?t . \
         OPTIONAL {{ ?t rdfs:comment ?c }} FILTER(!isBlank(?t)) }} }}"
    );

    let classes = terms_from_rows(&query_csv(&query, &class_q), Kind::Class);
    let attrs = terms_from_rows(&query_csv(&query, &attr_q), Kind::Attribute);

    // Refuse rather than write an empty vocabulary. An unreachable store and a
    // genuinely empty model produce identical output otherwise, and because the
    // vocabulary set is `replace`, the next deploy would then delete every term
    // in the graph — which is precisely the failure the replace flag makes
    // possible and this guard exists to prevent.
    if classes.is_empty() {
        eprintln!(
            "vocab-gen: REFUSED — the model at <{graph}> returned no classes. That is either an \
             unreachable store or an empty graph, and the vocabulary set is `replace`, so \
             writing this file would delete every term in the graph on the next deploy."
        );
        std::process::exit(1);
    }

    let authored = std::fs::read_to_string(format!(
        "{root}/roles/silas/ontology/vocabulary-identity-4254.ttl"
    ))
    .unwrap_or_default();
    let claimed = claimed_labels(&authored);

    let mut all = classes.clone();
    all.extend(attrs.clone());

    let stamp = String::from_utf8_lossy(
        &Command::new("date")
            .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
            .output()
            .map(|o| o.stdout)
            .unwrap_or_default(),
    )
    .trim()
    .to_string();

    // Which of our own IRIs the store actually declares. A term can point at a
    // dead IRI and look grounded; this is what tells those two states apart.
    let declared_q = format!(
        "SELECT DISTINCT ?s WHERE {{ GRAPH <{graph}> {{ ?s ?p ?o }} }}"
    );
    let declared: Vec<String> =
        query_csv(&query, &declared_q).into_iter().map(|(s, _)| s.trim().to_string()).collect();

    let r = render(&all, &claimed, &declared, &stamp);
    std::fs::write(&out_path, &r.ttl).expect("write the generated vocabulary");

    println!(
        "vocab-gen: {} classes + {} attributes at <{graph}> ({stamp}) -> {} concepts, \
         {} with no definition, {} skipped as already authored, {} skipped as duplicate names, \
         {} pointing at an IRI the model does not declare -> {out_path}",
        classes.len(),
        attrs.len(),
        r.emitted,
        r.undefined,
        r.skipped_authored.len(),
        r.skipped_duplicate.len(),
        r.dangling.len()
    );
}
