//! Pure-helper unit tests (#3299) — the CLI seam, the board-state validation core
//! (typed reasons), the zero-dep JSON field extractor, and the witness contracts.

use werk_unpull::{
    branch_name, json_str_field, jsonl_line, parse_unpull_args, spine_args, validate_card,
};

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parse_recognizes_atomic_anywhere_and_falls_back_to_deploy_role() {
    let (c, r, a) = parse_unpull_args(&args(&["3299", "kade"]), None).unwrap();
    assert_eq!((c, r.as_str(), a), (3299, "kade", false));
    let (c, r, a) = parse_unpull_args(&args(&["--atomic", "3299", "kade"]), None).unwrap();
    assert_eq!((c, r.as_str(), a), (3299, "kade", true), "--atomic anywhere, never mis-read as role");
    let (c, r, a) = parse_unpull_args(&args(&["3299", "--atomic"]), Some("silas".into())).unwrap();
    assert_eq!((c, r.as_str(), a), (3299, "silas", true), "DEPLOY_ROLE fallback");
    assert!(parse_unpull_args(&args(&["notanum", "kade"]), None).is_err());
    assert!(parse_unpull_args(&args(&[]), None).is_err());
    assert!(parse_unpull_args(&args(&["3299"]), None).is_err(), "no role + no DEPLOY_ROLE = usage");
}

#[test]
fn validate_card_types_wrong_status_and_wrong_owner() {
    assert!(validate_card("WIP", "Kade", "kade", 1).is_ok(), "title-cased owner matches case-insensitively");
    let (reason, detail) = validate_card("Done", "Kade", "kade", 7).unwrap_err();
    assert_eq!(reason, "wrong-status");
    assert!(detail.contains("'Done'") && detail.contains("must be WIP"), "{detail}");
    let (reason, detail) = validate_card("WIP", "Silas", "kade", 7).unwrap_err();
    assert_eq!(reason, "wrong-owner");
    assert!(detail.contains("'Silas'") && detail.contains("must be kade"), "{detail}");
}

#[test]
fn json_str_field_reads_cards_view_output() {
    let j = r#"{ "id": 3299, "status" : "WIP", "owner": "Kade" }"#;
    assert_eq!(json_str_field(j, "status").as_deref(), Some("WIP"));
    assert_eq!(json_str_field(j, "owner").as_deref(), Some("Kade"));
    assert_eq!(json_str_field(j, "missing"), None);
    assert_eq!(json_str_field(r#"{"n": 5}"#, "n"), None, "non-string values yield None");
}

#[test]
fn branch_and_witness_contracts() {
    assert_eq!(branch_name("kade", 3299), "kade/3299");
    let line = jsonl_line(1, "unpull.completed", "kade", 3299, "t", ",\"prior_branch\":\"kade/3299\"");
    assert!(line.contains("\"event\":\"unpull.completed\"") && line.contains("\"card_id\":3299") && line.ends_with('\n'));
    let a = spine_args("card.unpulled", "kade", 3299, "tr", &[("prior_branch", "kade/3299")]);
    assert_eq!(a, vec!["card.unpulled", "kade", "card=3299", "trace=tr", "prior_branch=kade/3299"]);
}
