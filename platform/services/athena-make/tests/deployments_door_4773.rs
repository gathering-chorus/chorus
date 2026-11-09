// @test-type: unit
// @domain: cicd
// #4773 AC2: POST /cicd/deployments door proof with spy store.
// Valid Deployment -> 201; missing cdhash -> 422.

use athena_make::{handle_write_scoped, RouteTable};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

const GRAPH: &str = "urn:chorus:domains:cicd";

fn deployment_table() -> RouteTable {
    RouteTable {
        unbounded: vec![],
        write_authority: String::new(),
        domain: "cicd".into(),
        base_path: "/cicd/deployments".into(),
        class: "https://jeffbridwell.com/chorus#Deployment".into(),
        fields: vec![
            "cdhash|datatype:string".into(),
            "cardId|datatype:string".into(),
            "deployedCommit|datatype:string".into(),
            "deployedUnit|datatype:string".into(),
            "sha256|datatype:string".into(),
            "deployedAt|datatype:dateTime".into(),
            "deployedBy|edge:Principal".into(),
            "landedFor|edge:Role".into(),
            "targetMachine|edge:Machine".into(),
            "previous|edge:Deployment".into(),
            "rollbackOf|edge:Deployment".into(),
        ],
        routes: vec!["GET /cicd/deployments".into(), "POST /cicd/deployments".into()],
        secured: vec![],
        mandatory: vec!["cdhash".into()],
        write_required: vec!["cdhash".into()],
        allowed_values: vec![],
        repo_target: "generated/cicd".into(),
        exposure: vec![],
        instances_graph: GRAPH.into(),
        tree_edges: vec![],
        tree_order: None,
        model_version: "v2".to_string(),
    }
}

fn setup_dal_spy() -> std::path::PathBuf {
    let spy_bin = std::env::temp_dir().join("dal-spy-4773");
    let mut f = std::fs::File::create(&spy_bin).expect("create spy");
    writeln!(f, "#!/bin/sh\nexit 0").expect("write spy");
    std::fs::set_permissions(&spy_bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    spy_bin
}

#[test]
fn deployment_table_matches_deployment_shape_in_werk_domains() {
    let ttl = include_str!("../../../../roles/kade/ontology/werk-domains.ttl");
    let start = ttl.find("chorus:DeploymentShape").expect("find DeploymentShape");
    let end = ttl[start..].find('.').expect("end of DeploymentShape") + start;
    let block = &ttl[start..end];

    let mut shape_paths: Vec<String> = Vec::new();
    let mut min_count_1_paths: Vec<String> = Vec::new();

    for line in block.lines() {
        if let Some(pos) = line.find("sh:path chorus:") {
            let rem = &line[pos + "sh:path chorus:".len()..];
            let name = rem.split_whitespace().next().unwrap().trim_matches(';');
            shape_paths.push(name.to_string());
            if line.contains("sh:minCount 1") {
                min_count_1_paths.push(name.to_string());
            }
        }
    }

    let table = deployment_table();
    let table_fields: Vec<String> = table.fields.iter().map(|f| f.split('|').next().unwrap().to_string()).collect();

    assert_eq!(table_fields, shape_paths, "RouteTable fields must match DeploymentShape paths exactly");
    assert_eq!(table.mandatory, min_count_1_paths, "mandatory must match minCount 1 paths");
}

#[test]
fn missing_cdhash_is_refused_at_door_with_422() {
    let spy = setup_dal_spy();
    std::env::set_var("CHORUS_MODEL_BIN", spy.to_str().unwrap());

    let body = r#"{"name":"dep-001","cardId":"4773","deployedCommit":"abc"}"#;
    let (code, resp) = handle_write_scoped("POST", "/cicd/deployments", body, &deployment_table(), "silas", "", "", &[GRAPH.to_string()]);
    assert_eq!(code, 422, "missing cdhash must be 422: {resp}");
    assert!(resp.contains("cdhash"), "response must name missing field: {resp}");
}

#[test]
fn valid_deployment_returns_201_via_dal_spy() {
    let spy = setup_dal_spy();
    std::env::set_var("CHORUS_MODEL_BIN", spy.to_str().unwrap());

    let body = r#"{"name":"dep-002","cdhash":"1234567890123456789012345678901234567890","cardId":"4773","deployedCommit":"abcdef"}"#;
    let (code, resp) = handle_write_scoped("POST", "/cicd/deployments", body, &deployment_table(), "silas", "", "", &[GRAPH.to_string()]);
    assert_eq!(code, 201, "valid deployment with cdhash must return 201: {resp}");
}

#[test]
fn deployer_refused_outside_cicd() {
    use athena_make::{create_refusal, WriteOp};
    // #4773 AC8 negative proof: deployer holds Write only on urn:chorus:domains:cicd
    let deployer_scopes = vec!["urn:chorus:domains:cicd".to_string()];

    // Allowed on urn:chorus:domains:cicd
    let res = create_refusal(&WriteOp::CreateEntity, "urn:chorus:domains:cicd", &deployer_scopes, "deployer");
    assert!(res.is_none(), "deployer must be allowed to write cicd domain");

    // Refused on urn:chorus:domains:cards
    let (code, body) = create_refusal(&WriteOp::CreateEntity, "urn:chorus:domains:cards", &deployer_scopes, "deployer")
        .expect("writing cards domain must be refused for deployer");
    assert_eq!(code, 403, "expected 403 on cards domain: {body}");

    // Refused on urn:chorus:domains:security
    let (code, body) = create_refusal(&WriteOp::CreateEntity, "urn:chorus:domains:security", &deployer_scopes, "deployer")
        .expect("writing security domain must be refused for deployer");
    assert_eq!(code, 403, "expected 403 on security domain: {body}");
}
