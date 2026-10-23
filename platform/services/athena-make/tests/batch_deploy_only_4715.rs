// @test-type: unit — hermetic: the /batch deploy-only rule over given lines and tables; no store.
// @domain: security — placed by chorus-crawl's domain rules (#4538)
// @card: #4715
// @owner: kade
//! #4715 AC2 — src/lib.rs `batch_deploy_refusal` / `handle_batch_checked`.
//! /batch took raw triples with only a scope check, so a role with Write on the
//! identity graph could INS a Principal row, the exact write #4220 forbids every
//! session. A batch now runs the same deploy-only check single writes run.
use athena_make::{batch_deploy_refusal, handle_batch_checked, RouteTable, NS};

fn table(class: &str, write_authority: &str) -> RouteTable {
    RouteTable { unbounded: vec![], write_authority: write_authority.into(), domain: String::new(), base_path: String::new(),
        class: format!("{NS}{class}"), fields: vec!["comment|datatype:string".into()], routes: vec![],
        secured: vec![], mandatory: vec![], write_required: vec![], allowed_values: vec![], repo_target: String::new(),
        exposure: vec![], instances_graph: "urn:chorus:domains:identity".into(), tree_edges: vec![], tree_order: None,
        model_version: "v2".into() }
}
fn tables() -> Vec<RouteTable> { vec![table("Principal", "deploy"), table("Permission", "")] }
fn line(s: &str, p: &str, o: &str) -> (String, String, String) { (s.into(), p.into(), o.into()) }
const RDF_TYPE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";

// negative proof: each of these went straight to the DAL before #4715
#[test]
fn negative_proof_a_batch_touching_a_deploy_only_row_is_refused() {
    let row = format!("<{NS}principal-mallory>");
    let why = batch_deploy_refusal(&[line(&row, &format!("<{NS}holdsRole>"), &format!("<{NS}role-kade>"))], &tables());
    assert!(why.as_deref().is_some_and(|w| w.contains("Principal") && w.contains("deploy")), "{why:?}");
    let typed = batch_deploy_refusal(&[line(&format!("<{NS}zz-new>"), RDF_TYPE, &format!("<{NS}Principal>"))], &tables());
    assert!(typed.is_some(), "typing a row as Principal is refused too");
}

#[test]
fn a_batch_on_ordinary_rows_passes_this_check() {
    let ok = [line(&format!("<{NS}permission-p1>"), &format!("<{NS}grants>"), "\"x\""),
              line(&format!("<{NS}permission-p1>"), RDF_TYPE, &format!("<{NS}Permission>"))];
    assert_eq!(batch_deploy_refusal(&ok, &tables()), None);
    assert_eq!(batch_deploy_refusal(&[line(&format!("<{NS}principal-x>"), "<p>", "<o>")], &[table("Principal", "")]), None,
        "a class without writeAuthority deploy is not held by this rule");
}

#[test]
fn the_handler_answers_403_before_any_dal_call() {
    let body = format!("INS\t<{NS}principal-mallory>\t<{NS}holdsRole>\t<{NS}role-jeff>\n");
    let (code, resp) = handle_batch_checked("urn:chorus:domains:identity", &body, "kade", "", &tables());
    assert_eq!(code, 403, "{resp}");
    assert!(resp.contains("only the model deploy"), "{resp}");
}
