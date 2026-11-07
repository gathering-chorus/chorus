// @domain: domains — placed by chorus-crawl's domain rules (#4645)
// @test-type: unit — pure function over a settings list; no store, no services
//! #4576 — the generator builds only a product that runs its own server. Athena
//! declares model.namespace (#4519) but is served by athena-make v1, so it must
//! not be generated; pulse declares server.bind and must be.
use athena_gen::model::runs_own_server;

fn kv(keys: &[&str]) -> Vec<(String, String)> {
    keys.iter().map(|k| (k.to_string(), "x".to_string())).collect()
}

#[test]
fn a_product_with_server_bind_is_generated() {
    assert!(runs_own_server(&kv(&["model.namespace", "server.bind", "store.kind"])));
}

/// NEGATIVE PROOF: Athena's real settings (model.namespace, store.graph_prefix) are
/// not a generated product; the old model.namespace key would have said it was.
#[test]
fn athena_namespace_alone_is_not_generated() {
    assert!(!runs_own_server(&kv(&["model.namespace", "store.graph_prefix"])));
    assert!(!runs_own_server(&[]));
}
