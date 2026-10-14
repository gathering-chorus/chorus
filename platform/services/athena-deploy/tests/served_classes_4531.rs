// @test-type: unit — signal:api is fixture-data
// @domain: athena
//! #4531 — the land's served check skips shape classes. #4515's model deploy
//! failed 10-14 19:10 with "ToolInput DEPLOYED BUT NOT SERVED": ToolInput's
//! instances are SHACL shapes (tool arguments), so no collection route serves it.
use athena_deploy::card::served_classes;

const SKILL: &str = "<https://jeffbridwell.com/chorus#Skill> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .\n";
const TOOL_INPUT: &str = "<https://jeffbridwell.com/chorus#ToolInput> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .\n";
const IS_SHAPE: &str = "<https://jeffbridwell.com/chorus#ToolInput> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://www.w3.org/ns/shacl#NodeShape> .\n";

#[test]
fn a_shape_class_is_not_asked_for_a_route() {
    let nt = format!("{SKILL}{TOOL_INPUT}{IS_SHAPE}");
    assert_eq!(served_classes(&nt), vec!["Skill".to_string()]);
}

/// NEGATIVE PROOF: without the subClassOf line ToolInput is checked like any class.
#[test]
fn negative_proof_a_plain_class_is_still_checked() {
    let nt = format!("{SKILL}{TOOL_INPUT}");
    assert_eq!(served_classes(&nt), vec!["Skill".to_string(), "ToolInput".to_string()]);
}

#[test]
fn the_model_declares_tool_input_a_shape_class() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let ttl = std::fs::read_to_string(std::path::Path::new(&dir).join("../../../roles/wren/ontology/skills-4467.ttl")).unwrap();
    let decl = ttl.split("chorus:ToolInput a owl:Class").nth(1).expect("ToolInput declared");
    let end = decl.find(" .\n").unwrap_or(decl.len());
    assert!(decl[..end].contains("rdfs:subClassOf sh:NodeShape"), "{}", &decl[..end]);
}
