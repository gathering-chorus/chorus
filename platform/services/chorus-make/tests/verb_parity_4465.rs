//! #4465 / ADR-062 decision 9 — MCP and dagu wrap the same verb list. Jeff,
//! 2026-10-09 (via Silas): MCP exposes one verb to an agent, dagu runs them in
//! order; neither has a verb the other lacks.
use chorus_make::{mcp_verbs, parse_rows, skill_verbs, verb_parity};

fn fixture(name: &str) -> String {
    // run time, not env!(): the werk-test 4030 guard refuses a compile-time path
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/{name}")).expect("fixture")
}

#[test]
fn mcp_verbs_are_the_werk_tools_not_the_pipeline_trigger() {
    let v = mcp_verbs(&fixture("mcp-server-names-20261009.txt"));
    assert!(v.contains("werk-pull") && v.contains("werk-review"));
    assert!(!v.contains("chorus_werk"), "the trigger is not a verb: {v:?}");
    assert_eq!(v.len(), 9);
}

#[test]
fn skill_verbs_are_the_binaries_of_deterministic_rows() {
    let rows = parse_rows(&fixture("cicd-rows.tsv")).unwrap();
    let v = skill_verbs(&rows);
    // werk-deploy runs three skills (werk, env-up, canonical) but is one verb
    assert!(v.contains("werk-deploy") && v.contains("werk-sync"));
    assert!(!v.iter().any(|b| b.is_empty()), "the human go has no binary: {v:?}");
    assert_eq!(v.len(), 10);
}

// NEGATIVE PROOF, measured live 2026-10-09 14:49: MCP has pull and unpull with
// no Skill row; the skills have test, demo and sync with no MCP tool. The check
// must go red on this and name all five.
#[test]
fn todays_lists_differ_and_the_check_names_every_gap() {
    let rows = parse_rows(&fixture("cicd-rows.tsv")).unwrap();
    let err = verb_parity(&skill_verbs(&rows), &mcp_verbs(&fixture("mcp-server-names-20261009.txt")))
        .expect_err("today's lists differ");
    for v in ["werk-pull", "werk-unpull", "werk-test", "werk-demo", "werk-sync"] {
        assert!(err.contains(v), "{v} not named in: {err}");
    }
}

#[test]
fn the_same_list_on_both_sides_passes() {
    let rows = parse_rows(&fixture("cicd-rows.tsv")).unwrap();
    let s = skill_verbs(&rows);
    assert_eq!(verb_parity(&s, &s.clone()), Ok(()));
}

// An empty side is a read failure, never parity.
#[test]
fn an_empty_list_is_refused_not_matched() {
    let empty = std::collections::BTreeSet::new();
    assert!(verb_parity(&empty, &empty).is_err());
}
