// @test-type: unit — hermetic: the ICD map query text; no store.
// @domain: alerts
// @card: #4714
// @owner: kade
//! #4714 — src/lib.rs `icd_query`, used by src/main.rs `icd_maps`. A gathering
//! restart PUT-replaces urn:gathering:icd/current and dropped the Grafana maps,
//! so the 03:30 sync check could not build rules. The maps now have their own
//! graph, and the query reads both.
use convergence_land::{icd_query, ICD_GRAPHS};

#[test]
fn the_map_query_reads_both_graphs() {
    let q = icd_query("urn:gathering:icd#provider-grafana");
    for g in ICD_GRAPHS {
        assert!(q.contains(&format!("<{g}>")), "{g} not read: {q}");
    }
    assert!(q.contains("GRAPH ?g"), "{q}");
}

// negative proof: the old query named one graph, so maps in the Grafana graph were never read
#[test]
fn negative_proof_a_map_only_in_the_grafana_graph_is_reachable() {
    let q = icd_query("urn:gathering:icd#provider-grafana");
    assert!(q.contains("<urn:chorus:icd/grafana>"));
    assert!(!q.contains("GRAPH <urn:gathering:icd/current>"), "pinned to the graph a restart wipes: {q}");
}
