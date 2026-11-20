// @domain: domains
//! #4995 — the principles settings file is a domain set: the card deploy loads it into
//! urn:chorus:domains:properties, never the schema graph, where athena-validate read its
//! 14 rows as v1-row (athena run 1, 2026-11-19 19:22). Read from the repo's own manifest.
//! NEGATIVE PROOF: a settings file the manifest does not list goes to the ontology.
use athena_deploy::card::plan;
use athena_deploy::parse_domain_sets;

const FILE: &str = "roles/kade/ontology/principles-settings-4995.ttl";

fn sets() -> Vec<athena_deploy::DomainSet> {
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("../../..");
    parse_domain_sets(&std::fs::read_to_string(root.join("platform/config/domain-set-manifest.txt")).unwrap()).unwrap()
}

#[test]
fn the_principles_settings_load_into_the_properties_graph() {
    let p = plan(&[FILE.to_string()], "/w", &sets());
    assert!(p.ontology.is_empty(), "nothing goes to the schema graph: {:?}", p.ontology);
    assert_eq!(p.sets.iter().map(|s| s.graph.as_str()).collect::<Vec<_>>(), vec!["urn:chorus:domains:properties"]);
}

#[test]
fn negative_proof_an_unlisted_settings_file_goes_to_the_ontology() {
    let p = plan(&["roles/kade/ontology/not-in-the-manifest.ttl".to_string()], "/w", &sets());
    assert!(p.sets.is_empty());
    assert_eq!(p.ontology, vec!["/w/roles/kade/ontology/not-in-the-manifest.ttl".to_string()]);
}
