// @test-type: unit — hermetic: the write handler against a store that does not answer.
// @domain: domains — placed by chorus-crawl's domain rules (#4538)
// @card: #4715
// @owner: kade
//! #4715 AC3 — src/lib.rs `entity_exists`. A Fuseki error read as "does not
//! exist", so a PUT or PATCH during an outage answered 404 (#4708's class): the
//! caller was told the row is gone when the store had simply not answered.
//! An unanswered existence read is 503 with its reason, never 404.
use athena_make::{entity_exists, exists_unavailable, handle_write_scoped, write_status, RouteTable};

const GRAPH: &str = "urn:chorus:domains:policies";

fn table() -> RouteTable {
    // ownerless class: the existence read is the first store read on PUT/PATCH
    RouteTable { unbounded: vec![], write_authority: String::new(), domain: String::new(), base_path: String::new(),
        class: "https://jeffbridwell.com/chorus#Policy".into(),
        fields: vec!["comment|datatype:string".into()],
        routes: vec!["GET /policies".into()],
        secured: vec![], mandatory: vec!["comment".into()], write_required: vec!["comment".into()],
        allowed_values: vec![], repo_target: "generated/policy".into(), exposure: vec![],
        instances_graph: GRAPH.into(), tree_edges: vec![], tree_order: None, model_version: "v2".to_string(),
    }
}

// Today the owner read (#4277) is the first store read and already answers 503,
// so this guards the order; the existence read below is the #4715 fix itself.
#[test]
fn a_patch_or_put_against_a_store_that_does_not_answer_is_503_not_404() {
    std::env::set_var("CHORUS_FUSEKI", "http://127.0.0.1:9/pods"); // nothing listens on the discard port
    let scope = vec![GRAPH.to_string()];
    for (method, body) in [("PATCH", r#"{"comment":"x"}"#), ("PUT", r#"{"comment":"x"}"#)] {
        let (code, resp) = handle_write_scoped(method, "/policies/p1", body, &table(), "kade", "", "", &scope);
        assert_eq!(code, 503, "{method}: {resp}");
        assert!(resp.contains("did not answer"), "{method} names why: {resp}");
        assert!(!resp.contains("does not exist"), "{method} must not claim the row is gone: {resp}");
    }
}

// negative proof: the old entity_exists returned false on a store error, which the
// handler turned into 404 whenever the owner read had answered and the existence
// read then failed. It must be an Err, and that Err must map to 503 with its reason.
#[test]
fn negative_proof_an_unanswered_existence_read_is_an_error_not_absent() {
    std::env::set_var("CHORUS_FUSEKI", "http://127.0.0.1:9/pods");
    let r = entity_exists("https://jeffbridwell.com/chorus#Policy", "p1", GRAPH);
    assert!(r.is_err(), "a store that did not answer is not 'does not exist': {r:?}");
    let (tag, msg) = exists_unavailable(&r.unwrap_err());
    assert_eq!(write_status(&tag).0, 503);
    assert!(msg.contains("did not answer") && msg.contains("nothing was written"), "{msg}");
}
