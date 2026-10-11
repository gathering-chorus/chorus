// #4456 card AC: each AC is met, not met or unmeasured with evidence; an AC
// with no runnable check is unmeasured, never met; a known-false AC is not met.
use borg_prove::*;

fn c(kind: &str, target: &str, expect: &str) -> Check {
    Check { ac: "ac-1-1".into(), statement: "s".into(), kind: kind.into(), target: target.into(), expect: expect.into() }
}

#[test]
fn a_check_that_sees_what_it_expects_is_met_with_its_evidence() {
    let o = judge(&c("http", "/health", "ok"), &Some("{\"status\":\"ok\"}".into()));
    assert_eq!(o.word(), "met");
    assert!(o.evidence().contains("/health"), "{o:?}");
}

#[test]
fn negative_proof_a_known_false_ac_is_not_met() {
    let o = judge(&c("http", "/health", "ok"), &Some("{\"status\":\"down\"}".into()));
    assert_eq!(o.word(), "not met", "{o:?}");
    let o = judge(&c("sparql", "ASK {}", "true"), &Some("{ \"head\": {}, \"boolean\": false }".into()));
    assert_eq!(o.word(), "not met", "{o:?}");
}

#[test]
fn no_check_or_no_answer_is_unmeasured_never_met() {
    assert_eq!(judge(&c("none", "", ""), &Some("anything".into())).word(), "unmeasured");
    assert_eq!(judge(&c("http", "/x", "ok"), &None).word(), "unmeasured");
    assert_eq!(judge(&c("http", "/x", ""), &Some("ok".into())).word(), "unmeasured");
    assert_eq!(judge(&c("shell", "rm -rf /", "x"), &Some("x".into())).word(), "unmeasured", "card text is never run");
}

#[test]
fn the_verdict_is_proven_only_when_every_ac_is_met() {
    let met = (c("http", "/a", "ok"), Outcome::Met("e".into()));
    let un = (c("none", "", ""), Outcome::Unmeasured("e".into()));
    assert_eq!(verdict(&[met.clone()]), "Proven");
    assert_eq!(verdict(&[met, un]), "Failed");
    assert_eq!(verdict(&[]), "Failed", "a card with no AC is not proven");
}

#[test]
fn intake_rejects_an_ac_with_no_runnable_check_and_names_it() {
    assert!(intake(&[c("http", "/a", "ok")]).is_ok());
    let errs = intake(&[c("http", "/a", "ok"), Check { ac: "ac-1-2".into(), ..c("none", "", "") }]).unwrap_err();
    assert_eq!(errs.len(), 1);
    assert!(errs[0].starts_with("ac-1-2"), "{errs:?}");
    assert!(intake(&[]).is_err(), "no AC rows is rejected");
}

#[test]
fn a_failure_routes_to_reopen_before_the_land_and_a_ticket_in_operating() {
    let routes = parse_routes("skill-prove-stage=value-stream-step-build,skill-prove-in-prod=value-stream-step-borg-surface");
    assert_eq!(route_for("skill-prove-stage", &routes, "silas"), Route::Reopen { step: "value-stream-step-build".into() });
    assert_eq!(
        route_for("skill-prove-in-prod", &routes, "silas"),
        Route::Ticket { step: "value-stream-step-borg-surface".into(), key: "silas|skill-prove-in-prod|failed".into() }
    );
    assert_eq!(route_for("skill-nope", &routes, "silas"), Route::Unrouted, "never guessed");
}

#[test]
fn cardac_rows_read_from_the_store_answer_in_ac_order() {
    let tsv = "?ac\t?statement\t?kind\t?target\t?expect\n<https://jeffbridwell.com/chorus#ac-4456-1>\t\"runs each check\"\t\"http\"\t\"/health\"\t\"ok\"\n<https://jeffbridwell.com/chorus#ac-4456-2>\t\"no check\"\t\"none\"\n";
    let cs = checks_from_tsv(tsv).unwrap();
    assert_eq!(cs.len(), 2);
    assert_eq!((cs[0].ac.as_str(), cs[0].kind.as_str(), cs[1].kind.as_str(), cs[1].target.as_str()), ("ac-4456-1", "http", "none", ""));
    assert!(checks_from_tsv("<html>").is_err());
    assert!(checks_query(4456).contains("chorus:card-4456"));
}

#[test]
fn the_failed_step_is_read_from_the_runs_status() {
    let st = "Running - x\n├─skill-prove-intake (1s) [success]\n├─skill-prove-stage (3s) [failed]\n└─skill-werk-go [none]\n";
    assert_eq!(failed_in_status(st), "skill-prove-stage");
    assert_eq!(failed_in_status("├─skill-a (1s) [success]\n"), "", "no failed step names none");
}

#[test]
fn a_status_expect_is_the_whole_answer_and_a_test_with_no_result_is_unmeasured() {
    assert_eq!(judge(&c("http", "/health", "200"), &Some("200".into())).word(), "met");
    assert_eq!(judge(&c("http", "/health", "200"), &Some("2004".into())).word(), "not met", "200 is not a substring match");
    assert_eq!(judge(&c("test", "t.sh", "pass"), &Some("fail".into())).word(), "not met");
    assert_eq!(tsv_value("?result\n"), None, "no recorded result → the probe saw nothing");
    assert_eq!(tsv_value("?result\n\"pass\"\n").as_deref(), Some("pass"));
}
