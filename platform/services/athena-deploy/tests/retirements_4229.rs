//! #4229 — the staged retirement legs. The bash's are the only delete paths in
//! the deploy, and two of the three take no backup at all. AC3 says every one
//! of them unloads before it deletes and refuses if the dump fails, so these
//! cover the decision layer: which entries execute, and which never should.
use athena_deploy::{parse_retirement, retirement_action, RetireAction};

fn r(line: &str) -> athena_deploy::Retirement {
    parse_retirement(line).expect("parses").expect("not blank")
}

#[test]
fn a_subject_entry_retires_one_subject_in_its_graph() {
    let e = r(r#"{"retire_subject":"https://x#a","graph":"urn:chorus:domains:tests","status":"staged"}"#);
    assert_eq!(
        retirement_action(&e, "urn:chorus:ontology"),
        RetireAction::Subject { iri: "https://x#a".into(), graph: "urn:chorus:domains:tests".into() }
    );
}

#[test]
fn an_entry_with_no_graph_falls_back_to_the_ontology_graph() {
    let e = r(r#"{"retire_subject":"https://x#a","status":"staged"}"#);
    assert_eq!(
        retirement_action(&e, "urn:chorus:ontology"),
        RetireAction::Subject { iri: "https://x#a".into(), graph: "urn:chorus:ontology".into() }
    );
}

#[test]
fn class_and_whole_graph_entries_are_their_own_kinds() {
    let c = r(r#"{"retire_class":"https://x#File","graph":"urn:g","status":"staged"}"#);
    assert_eq!(retirement_action(&c, "urn:o"),
        RetireAction::Class { class: "https://x#File".into(), graph: "urn:g".into() });
    let g = r(r#"{"retire_graph":"urn:dead","status":"staged"}"#);
    assert_eq!(retirement_action(&g, "urn:o"), RetireAction::WholeGraph { graph: "urn:dead".into() });
}

#[test]
fn a_claim_entry_is_neither_a_subject_nor_a_graph() {
    let e = r(r#"{"subject_domain":"chorus","object_class":"Borg","status":"staged"}"#);
    assert_eq!(retirement_action(&e, "urn:o"),
        RetireAction::Claim { domain: "chorus".into(), class: "Borg".into() });
}

#[test]
fn negative_proof_an_entry_that_already_ran_does_not_run_again() {
    // #3788, and it is the most expensive line in this file. Ten entries fired
    // as intended on 2026-08-06, the records were restored from a verified
    // backup, and seven fired AGAIN during that card's land — removing jeff,
    // marknakib and all three agents from the allow-set the doors read. If
    // this ever returns anything but Skip, that lockout is reachable again.
    for done in ["executed", "retracted", "done", "anything-not-staged"] {
        let line = format!(r#"{{"retire_subject":"https://x#a","status":"{done}"}}"#);
        assert_eq!(
            retirement_action(&r(&line), "urn:o"),
            RetireAction::Skip { status: done.into(), target: "https://x#a".into() },
            "status {done:?} must not execute"
        );
    }
}

#[test]
fn a_missing_status_reads_as_staged_the_way_the_bash_defaults_it() {
    let e = r(r#"{"retire_subject":"https://x#a"}"#);
    assert!(matches!(retirement_action(&e, "urn:o"), RetireAction::Subject { .. }));
}

#[test]
fn negative_proof_a_malformed_line_refuses_rather_than_being_skipped() {
    // Fail-closed (#3752). A line that is present but unreadable must stop the
    // deploy — skipping it silently drops a retirement someone staged.
    for bad in ["not json at all", r#"{"retire_subject":}"#, r#"{"retire_subject":123}"#] {
        assert!(parse_retirement(bad).is_err(), "{bad:?} must refuse");
    }
    assert_eq!(parse_retirement("   ").unwrap(), None, "a blank line is not an error");
}

#[test]
fn negative_proof_empty_fields_stay_empty() {
    // The bash needed a \x1f separator here: TAB collapsed adjacent empty
    // fields and shifted them left, turning a claim entry into a subject
    // retirement of its own graph name on the first test run.
    let e = r(r#"{"subject_domain":"","object_class":"","retire_subject":"","graph":"urn:g","status":"staged"}"#);
    assert_eq!(retirement_action(&e, "urn:o"), RetireAction::Claim { domain: String::new(), class: String::new() });
}

// ---- AC3: every delete path backs up first ------------------------------
use athena_deploy::{delete_guard, DeleteGuard};

#[test]
fn a_complete_backup_lets_the_delete_run() {
    assert_eq!(delete_guard(Some(226), Some(226), "<urn:g> subject x"),
        DeleteGuard::Proceed { backed_up: 226 });
    assert_eq!(delete_guard(Some(3), Some(9), "x"), DeleteGuard::Proceed { backed_up: 9 });
}

#[test]
fn nothing_live_is_idempotent_not_an_error() {
    assert_eq!(delete_guard(Some(0), Some(0), "x"), DeleteGuard::AlreadyAbsent);
}

#[test]
fn negative_proof_a_failed_dump_stops_the_delete() {
    // The CONSTRUCT is best-effort; a 500 leaves a one-line error file. Before
    // this rule reached subject and class retirement, 226 rows could be
    // deleted with nothing written down.
    let g = delete_guard(Some(226), Some(1), "<urn:g> subject x");
    match g {
        DeleteGuard::Refuse(why) => {
            assert!(why.contains("226"), "the refusal must name what was at risk: {why}");
            assert!(why.contains("NOT deleting"), "{why}");
        }
        other => panic!("a short backup must refuse, got {other:?}"),
    }
    assert!(matches!(delete_guard(Some(5), None, "x"), DeleteGuard::Refuse(_)));
}

#[test]
fn negative_proof_an_uncountable_target_is_not_treated_as_empty() {
    // "The store did not answer" and "there is nothing there" are different
    // states, and a check that cannot tell them apart deletes on a timeout.
    assert!(matches!(delete_guard(None, Some(0), "x"), DeleteGuard::Refuse(_)));
    assert_ne!(delete_guard(None, Some(0), "x"), delete_guard(Some(0), Some(0), "x"));
}
