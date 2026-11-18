// @test-type: unit — hermetic: the door's write rule for Session rows over a given table; no store.
// @domain: security
// @card: #4517
//! #4517 AC5 — the door is the only writer of Session rows. Today each role's
//! chorus-principal POSTs its own Session row after its own login, so a refused
//! role can't record its refusal and a failed write leaves status guessing
//! (Jeff 11-17: "we have done 10-20 cards trying to fix sessions"). A Session shape
//! carries writeAuthority "door": no role session may write it; the door writes it
//! when it first verifies a new login (ownedBy the role, startedBy the door).
use athena_make::{handle_write_scoped, RouteTable, NS};

const GRAPH: &str = "urn:chorus:domains:identity";

fn table(write_authority: &str) -> RouteTable {
    RouteTable { unbounded: vec![], write_authority: write_authority.into(), domain: String::new(), base_path: String::new(),
        class: format!("{NS}Session"), fields: vec!["channel|datatype:string".into(), "ownedBy|edge:Principal".into()],
        routes: vec![], secured: vec![], mandatory: vec![], write_required: vec![], allowed_values: vec![],
        repo_target: String::new(), exposure: vec![], instances_graph: GRAPH.into(), tree_edges: vec![], tree_order: None,
        model_version: "v2".into() }
}

fn granted() -> Vec<String> { vec![GRAPH.to_string()] }

/// NEGATIVE PROOF: a role holding Write on the identity graph still cannot write a
/// Session row once the shape says the door writes it. Same role, same body, a
/// shape without the rule is not refused by this check.
#[test]
fn a_role_cannot_write_a_session_row_the_door_writes() {
    let body = r#"{"name":"kade-s1","channel":"pane"}"#;
    let (code, why) = handle_write_scoped("POST", "/sessions", body, &table("door"), "kade", "", "", &granted());
    assert_eq!(code, 403, "{why}");
    assert!(why.contains("door"), "the refusal names who writes it: {why}");
    for (m, p) in [("PATCH", "/sessions/kade-s1"), ("PUT", "/sessions/kade-s1"), ("DELETE", "/sessions/kade-s1")] {
        let (c, w) = handle_write_scoped(m, p, r#"{"channel":"x"}"#, &table("door"), "kade", "", "", &granted());
        assert_eq!(c, 403, "{m} {p}: {w}");
    }
    let (c, w) = handle_write_scoped("POST", "/sessions", body, &table(""), "kade", "", "", &granted());
    assert_ne!(c, 403, "without the rule this check must not refuse: {w}");
}
