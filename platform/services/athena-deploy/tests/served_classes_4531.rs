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

// #4536 run 9 (10-15 13:15) — the staging served check failed 14 werk-v1
// classes (Witness, GateResult, …) the card never touched, because it judged
// every class in werk-domains.ttl. It answers only for classes the card changed.
use athena_deploy::card::changed_served_classes;

const SKILL_LABEL: &str = "<https://jeffbridwell.com/chorus#Skill> <http://www.w3.org/2000/01/rdf-schema#label> \"skill\" .\n";

#[test]
fn a_class_unchanged_from_main_is_not_the_cards_to_serve() {
    let main = format!("{SKILL}{TOOL_INPUT}");
    assert_eq!(changed_served_classes(&main, &main), Vec::<String>::new());
}

/// NEGATIVE PROOF: a class the card adds, or changes, in the same file is still checked.
#[test]
fn negative_proof_an_added_or_changed_class_is_still_checked() {
    assert_eq!(changed_served_classes(&format!("{SKILL}{TOOL_INPUT}"), SKILL), vec!["ToolInput".to_string()]);
    assert_eq!(changed_served_classes(&format!("{SKILL}{SKILL_LABEL}"), SKILL), vec!["Skill".to_string()]);
}
