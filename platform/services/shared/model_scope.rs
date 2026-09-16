// model_scope — the ONE definition of "which changed files are model sources and
// which are seed sources" (#4186, Kade's cold-eyes finding: the rule had grown
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

/// Seed sources (#4096): authored instance rows under designing/data and the
/// manifest that lists them.
pub fn is_seed_source(path: &str) -> bool {
    let l = path.trim();
    (l.starts_with("designing/data/") && l.ends_with(".ttl"))
        || l == "platform/config/instance-seed-manifest.txt"
}

pub fn changed_model_sources(diff: &str) -> Vec<String> {
    diff.lines().map(str::trim).filter(|l| is_model_source(l)).map(str::to_string).collect()
}

pub fn changed_seed_sources(diff: &str) -> Vec<String> {
    diff.lines().map(str::trim).filter(|l| is_seed_source(l)).map(str::to_string).collect()
}
