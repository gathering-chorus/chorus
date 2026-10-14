// @test-type: unit — signal:api is fixture-data
// @domain: services
//! #4515 — the Skill rows Wren writes through the door at land, for the tools
//! the old server served (logs, pain, flow, blast radius, gemba, services):
//! 13 rows, 6 of them http routes. Every one must load as a tool on this
//! runner: a shape athena-gen generated, a binary or route, and who it runs as.
//! NEGATIVE PROOF: a row naming a shape that was never generated is
//! unmeasured, not served.
use chorus_runner::skills::{skill_tools, TOOL_INPUTS};
use serde_json::{Map, Value};

fn rows() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/wren-rows-4515.json")).unwrap()
}
fn inputs() -> Map<String, Value> {
    serde_json::from_str(TOOL_INPUTS).unwrap()
}

#[test]
fn every_wren_row_loads_as_a_tool_named_with_hyphens() {
    let rows = rows();
    let (tools, unmeasured) = skill_tools(&rows, &inputs());
    assert!(unmeasured.is_empty(), "{unmeasured:?}");
    assert_eq!(tools.len(), rows.len());
    for t in &tools {
        assert!(t.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{} breaks skill-<name>", t.name);
    }
    assert_eq!(tools.len(), 13);
    assert_eq!(tools.iter().filter(|t| t.http).count(), 6);
}

#[test]
fn negative_proof_a_row_with_an_ungenerated_shape_is_unmeasured() {
    let mut rows = rows();
    let broken = rows[0]["name"].as_str().unwrap().to_string();
    rows[0]["inputShape"] = Value::String("NoSuchInput4515".into());
    let (tools, unmeasured) = skill_tools(&rows, &inputs());
    assert_eq!(unmeasured.len(), 1, "{unmeasured:?}");
    assert_eq!(unmeasured[0].skill, broken);
    assert_eq!(tools.len(), rows.len() - 1);
    assert!(!tools.iter().any(|t| t.name == broken));
}
