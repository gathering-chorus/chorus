// #3570 — TDD-first (red before green). The domains.* data/noun spine:
// owl-api derives a kind's instance HOME graph from the model.
// #3640 (ADR-051): rule tightened — explicit chorus:instancesGraph wins; else
// urn:chorus:domains:<domain> (the spine); else REFUSE. The silent back-compat
// default (urn:chorus:instances) is deleted: a class the model never placed was
// how Product instances scattered across graphs unnoticed (the July incident).
use owl_api::resolve_instances_graph;

#[test]
fn instances_graph_override_else_domain_else_refuse() {
    // explicit chorus:instancesGraph override (migration target / bespoke)
    assert_eq!(
        resolve_instances_graph(Some("  urn:chorus:domains:tests  "), Some("tests")).unwrap(),
        "urn:chorus:domains:tests",
        "declared override wins, trimmed"
    );
    // no override but domain known → project the domains.* spine
    assert_eq!(
        resolve_instances_graph(None, Some("tests")).unwrap(),
        "urn:chorus:domains:tests",
        "no override → urn:chorus:domains:<domain>"
    );
    assert_eq!(
        resolve_instances_graph(None, Some("photos")).unwrap(),
        "urn:chorus:domains:photos",
        "spine generalizes per domain"
    );
    // whitespace-only override falls through to the domain projection
    assert_eq!(
        resolve_instances_graph(Some("   "), Some("tests")).unwrap(),
        "urn:chorus:domains:tests",
        "whitespace override → domain projection"
    );
}

#[test]
fn undeclared_class_is_refused_not_defaulted() {
    // #3640 — neither shape-declared nor domain-declared → typed refusal that
    // names the fix, never the old silent urn:chorus:instances fallback.
    let err = resolve_instances_graph(None, None).unwrap_err();
    assert!(
        err.contains("land the model first"),
        "refusal names the fix: {}",
        err
    );
    assert!(
        err.contains("ADR-051"),
        "refusal cites the governing decision: {}",
        err
    );
    // whitespace-only on BOTH slots is the same refusal
    assert!(resolve_instances_graph(Some("  "), Some(" ")).is_err());
}
