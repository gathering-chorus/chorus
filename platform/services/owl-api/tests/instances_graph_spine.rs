// #3570 — TDD-first (red before green). The domains.* data/noun spine:
// owl-api derives a kind's instance HOME graph from the model.
// Rule: explicit chorus:instancesGraph wins; else urn:chorus:domains:<domain>
// (the spine); else the back-compat default urn:chorus:instances (no big-bang).
use owl_api::{resolve_instances_graph, INSTANCES_GRAPH};

#[test]
fn instances_graph_override_else_domain_else_default() {
    // explicit chorus:instancesGraph override (migration target / bespoke)
    assert_eq!(
        resolve_instances_graph(Some("  urn:chorus:domains:tests  "), Some("tests")),
        "urn:chorus:domains:tests",
        "declared override wins, trimmed"
    );
    // no override but domain known → project the domains.* spine
    assert_eq!(
        resolve_instances_graph(None, Some("tests")),
        "urn:chorus:domains:tests",
        "no override → urn:chorus:domains:<domain>"
    );
    assert_eq!(
        resolve_instances_graph(None, Some("photos")),
        "urn:chorus:domains:photos",
        "spine generalizes per domain"
    );
    // neither override nor domain → back-compat default, never empty
    assert_eq!(
        resolve_instances_graph(None, None),
        INSTANCES_GRAPH,
        "underivable → default urn:chorus:instances (back-compat)"
    );
    // whitespace-only override falls through to the domain projection
    assert_eq!(
        resolve_instances_graph(Some("   "), Some("tests")),
        "urn:chorus:domains:tests",
        "whitespace override → domain projection"
    );
}
