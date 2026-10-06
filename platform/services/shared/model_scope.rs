// model_scope — the ONE definition of "which changed files are model sources"
// (#4186, Kade's cold-eyes finding: the rule had grown
// three copies — werk-deploy, werk.yml, athena.yml — and would drift silently).
// Included by `#[path]` into werk-deploy (the witnessed hand-off) and athena-deploy
// (the `scope` verb the workflows call). Pure; unit-tested in both crates.

/// Model sources: any TTL under a role's ontology dir, plus the staged
/// retirements file (#3752 — landing a card that stages one must run the model
/// deploy so the retirement section executes it).
pub fn is_model_source(path: &str) -> bool {
    let l = path.trim();
    (l.ends_with(".ttl")
        && l.starts_with("roles/")
        && l.splitn(3, '/').nth(2).is_some_and(|rest| rest.starts_with("ontology/")))
        || l == "designing/schemas/model-retirements.jsonl"
}

// #4432 (Jeff 2026-10-06: "why do we reload this data as part of our deploy"):
// there are no seed sources. Instance rows live in the store and change through
// the door; a land never replays row files over newer rows.

pub fn changed_model_sources(diff: &str) -> Vec<String> {
    diff.lines().map(str::trim).filter(|l| is_model_source(l)).map(str::to_string).collect()
}
