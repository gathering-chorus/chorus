// @test-type: unit — signal:api is fixture-data
// @domain: cards
//! #4531 AC6 (Jeff 2026-10-14 19:13, "yes") — /card from an agent session files
//! the card as Jeff: clearing-card-jeff runs as its owner (jeff) for any verified
//! caller, so cards add sees DEPLOY_ROLE=jeff and no bouncer fires.
use chorus_runner::skills::{run_identity, skill_tools, TOOL_INPUTS};
use serde_json::{Map, Value};

fn tools() -> Vec<chorus_runner::skills::SkillTool> {
    let rows: Vec<Value> = serde_json::from_str(include_str!("fixtures/clearing-rows-4515.json")).unwrap();
    let inputs: Map<String, Value> = serde_json::from_str(TOOL_INPUTS).unwrap();
    let (tools, unmeasured) = skill_tools(&rows, &inputs);
    assert!(unmeasured.is_empty(), "{unmeasured:?}");
    tools
}

#[test]
fn an_agent_calling_card_jeff_runs_it_as_jeff() {
    let t = tools().into_iter().find(|t| t.name == "clearing-card-jeff").unwrap();
    for agent in ["wren", "silas", "kade", "abby-normal"] {
        assert_eq!(run_identity(&t, agent).unwrap(), "jeff", "{agent}");
    }
}

/// NEGATIVE PROOF: the ordinary card tool still runs as the caller, so an
/// agent's own card still goes through the bouncer.
#[test]
fn negative_proof_clearing_card_runs_as_the_caller() {
    let t = tools().into_iter().find(|t| t.name == "clearing-card").unwrap();
    assert_eq!(run_identity(&t, "wren").unwrap(), "wren");
}

/// NEGATIVE PROOF: a plain owner-run tool still refuses anyone but its owner and jeff.
#[test]
fn negative_proof_an_owner_tool_still_refuses_other_callers() {
    let mut t = tools().into_iter().find(|t| t.name == "clearing-card-jeff").unwrap();
    t.runs_as = chorus_runner::skills::RunsAs::Owner;
    assert!(run_identity(&t, "wren").is_err());
    assert_eq!(run_identity(&t, "jeff").unwrap(), "jeff");
}

#[test]
fn an_unverified_caller_runs_nothing() {
    let t = tools().into_iter().find(|t| t.name == "clearing-card-jeff").unwrap();
    assert!(run_identity(&t, "unverified").is_err());
    assert!(run_identity(&t, "").is_err());
}
