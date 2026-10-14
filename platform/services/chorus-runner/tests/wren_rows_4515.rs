// @test-type: unit — signal:api is fixture-data
// @domain: services
//! #4515 — the Skill rows Wren writes through the door at land, for the tools
//! the old server served (logs, pain, flow, blast radius, gemba, services),
//! plus the pipeline starters chorus-werk, chorus-athena and chorus-borg (Jeff
//! 2026-10-14): 16 rows, 6 of them http routes. Every one must load as a tool on this
//! runner: a shape athena-gen generated, a binary or route, and who it runs as.
//! NEGATIVE PROOF: a row naming a shape that was never generated is
//! unmeasured, not served.
use chorus_runner::skills::{argv, check_args, coerce_args, skill_tools, TOOL_INPUTS};
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
    assert_eq!(tools.len(), 16);
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

fn starter_argv(name: &str, pairs: &[(&str, &str)]) -> Result<Vec<String>, String> {
    let (tools, _) = skill_tools(&rows(), &inputs());
    let t = tools.iter().find(|t| t.name == name).ok_or(format!("no tool {name}"))?;
    let raw: Map<String, Value> = pairs.iter().map(|(k, v)| (k.to_string(), Value::String(v.to_string()))).collect();
    let args = coerce_args(&raw, &t.schema);
    check_args(&args, &t.schema)?;
    assert_eq!(t.binary, "pipeline-start");
    Ok(argv(&t.template, &args, "wren"))
}

/// Jeff 2026-10-14: `chorus-<product> [stream]` starts that product's pipeline
/// for a card, as the caller. A starter is no stream step, so it names none.
#[test]
fn each_starter_starts_its_products_pipeline_as_the_caller() {
    assert_eq!(starter_argv("chorus-werk", &[("card", "4515")]).unwrap(), vec!["cicd", "4515", "wren"]);
    assert_eq!(starter_argv("chorus-athena", &[("card", "4515")]).unwrap(), vec!["athena", "4515", "wren"]);
    assert_eq!(starter_argv("chorus-borg", &[("card", "4515"), ("stream", "prove")]).unwrap(), vec!["borg-prove", "4515", "wren"]);
    // operate reaches pipeline-start, which refuses it by name (no generated workflow yet)
    assert_eq!(starter_argv("chorus-borg", &[("card", "4515"), ("stream", "operate")]).unwrap(), vec!["borg-operate", "4515", "wren"]);
    for r in rows().iter().filter(|r| r["name"].as_str().unwrap_or("").starts_with("chorus-")) {
        assert!(r.get("forStreamStep").is_none(), "{} is a starter, not a step", r["name"]);
        assert_eq!(r["runsAs"], "caller");
    }
}

#[test]
fn negative_proof_a_starter_refuses_a_bad_stream_or_no_card() {
    assert!(starter_argv("chorus-borg", &[("card", "4515"), ("stream", "land")]).unwrap_err().contains("stream must be one of"));
    assert!(starter_argv("chorus-borg", &[("card", "4515")]).is_err());
    assert!(starter_argv("chorus-werk", &[]).is_err());
    assert!(starter_argv("chorus-werk", &[("card", "x")]).is_err());
}
