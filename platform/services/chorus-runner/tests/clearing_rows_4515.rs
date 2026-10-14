// @test-type: unit — signal:api is fixture-data
// @domain: services
//! #4515 — the Clearing tools the sessions call (nudge, card, card-jeff, wip,
//! prioritize, tag, sequence, surface, sweep). Measured 2026-10-14 17:25: the
//! demo runner listed 30 tools and none of these, because their rows were never
//! written and three of their shapes never generated. After the land the old
//! server is gone, so a missing row means a role can no longer nudge or file a card.
//! Every row here must load as a tool, and every tool a skill, hook or script
//! names must be one this runner serves.
use chorus_runner::skills::{argv, check_args, coerce_args, skill_tools, TOOL_INPUTS};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn clearing_rows() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/clearing-rows-4515.json")).unwrap()
}
fn wren_rows() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/wren-rows-4515.json")).unwrap()
}
fn inputs() -> Map<String, Value> {
    serde_json::from_str(TOOL_INPUTS).unwrap()
}

/// The werk and athena tools already live on the runner (measured 17:25 by
/// tools/list as wren against the demo runner).
const LIVE: &[&str] = &[
    "athena-add", "athena-deploy", "athena-gen", "athena-get", "athena-list", "athena-model", "athena-set",
    "athena-validate", "herald-run", "task_status", "werk-accept", "werk-build", "werk-commit", "werk-demo",
    "werk-deploy", "werk-deploy-crate", "werk-deploy-demo", "werk-deploy-env-down", "werk-deploy-env-up",
    "werk-deploy-prod", "werk-merge", "werk-pull", "werk-push", "werk-review", "werk-review-verdict",
    "werk-sync", "werk-test", "werk-unpull",
];

fn served(rows: &[Value]) -> BTreeSet<String> {
    let (tools, _) = skill_tools(rows, &inputs());
    tools.into_iter().map(|t| t.name).chain(LIVE.iter().map(|s| s.to_string())).collect()
}

fn repo() -> PathBuf {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    Path::new(&dir).join("../../..").canonicalize().unwrap()
}

/// Every `mcp__chorus-api__<tool>` a skill, hook source or script names.
fn named_by_callers(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack: Vec<PathBuf> = ["skills", "platform/scripts", "platform/services/chorus-hooks/src", "directing/clearing/src"]
        .iter().map(|d| root.join(d)).collect();
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in std::fs::read_dir(&p).unwrap().flatten() { stack.push(e.path()); }
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        for (i, _) in text.match_indices("mcp__chorus-api__") {
            let name: String = text[i + "mcp__chorus-api__".len()..]
                .chars().take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_').collect();
            if !name.is_empty() { out.insert(name); }
        }
    }
    out
}

#[test]
fn every_clearing_row_loads_as_a_tool() {
    let rows = clearing_rows();
    let (tools, unmeasured) = skill_tools(&rows, &inputs());
    assert!(unmeasured.is_empty(), "{unmeasured:?}");
    assert_eq!(tools.len(), 9);
}

#[test]
fn clearing_card_files_a_card_with_the_fields_the_cli_needs() {
    let (tools, _) = skill_tools(&clearing_rows(), &inputs());
    let t = tools.iter().find(|t| t.name == "clearing-card").unwrap();
    let raw: Map<String, Value> = [("title", "Fix x — y"), ("owner", "kade"), ("priority", "P2"), ("domain", "chorus"),
        ("type", "fix"), ("origin", "reactive"), ("desc", "## Experience\nx\n## AC\n- [ ] y")]
        .iter().map(|(k, v)| (k.to_string(), Value::String(v.to_string()))).collect();
    let args = coerce_args(&raw, &t.schema);
    check_args(&args, &t.schema).unwrap();
    let a = argv(&t.template, &args, "wren");
    assert_eq!(a[0], "add");
    assert!(a.contains(&"--owner=kade".to_string()), "{a:?}");
    assert!(!a.iter().any(|x| x.starts_with("--sequence")), "an absent optional flag is dropped: {a:?}");
}

#[test]
fn every_tool_a_caller_names_is_served() {
    let mut rows = clearing_rows();
    rows.extend(wren_rows());
    let served = served(&rows);
    let missing: Vec<String> = named_by_callers(&repo()).into_iter().filter(|n| !served.contains(n)).collect();
    assert!(missing.is_empty(), "named by a skill, hook or script but served by no row: {missing:?}");
}

/// NEGATIVE PROOF: drop the nudge row and the check names it.
#[test]
fn negative_proof_a_named_tool_with_no_row_is_caught() {
    let rows: Vec<Value> = clearing_rows().into_iter().filter(|r| r["name"] != "clearing-nudge").collect();
    let served = served(&rows);
    let named = named_by_callers(&repo());
    assert!(named.contains("clearing-nudge"), "a skill names clearing-nudge");
    assert!(!served.contains("clearing-nudge"));
}
