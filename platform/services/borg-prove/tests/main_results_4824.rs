// @test-type: unit
// @domain: tests
//! #4824 — #4804, #4772 and #4805 landed true and still read Unproven: a `test`
//! AC whose test the diff did not touch never ran in the card's runs (the
//! nightly passed 4535 at 04:23 on 10-31, card_id ""), and no check kind could
//! read the crawl ("unknown check kind 'chorus-crawl'").

use borg_prove::{checks_from_card, crawl_row, judge, latest_case, newer, test_logql_main, Outcome};

fn body(rows: &[(&str, &str)]) -> String {
    let vals: Vec<String> = rows.iter()
        .map(|(ts, ev)| format!(r#"["{ts}","{{\"event\":\"test.case.{ev}\",\"file\":\"x.sh\"}}"]"#))
        .collect();
    format!(r#"{{"data":{{"result":[{{"stream":{{}},"values":[{}]}}]}}}}"#, vals.join(","))
}

#[test]
fn main_counts_only_the_nightly_never_another_cards_branch() {
    let q = test_logql_main("platform/tests/4535-shared-state-ratchet.test.sh");
    assert!(q.starts_with(r#"{job="platform-chorus", appName="chorus-events", role="nightly"}"#), "{q}");
    assert!(q.contains(r#"file="platform/tests/4535-shared-state-ratchet.test.sh""#), "{q}");
}

#[test]
fn a_nightly_pass_since_the_land_answers_when_the_card_never_ran_the_test() {
    let main = latest_case(&body(&[("300", "passed")]));
    assert_eq!(newer(None, main).as_deref(), Some("pass"));
}

// NEGATIVE PROOF: a fail newer than the pass is the answer, from either side
#[test]
fn the_newer_result_wins_and_a_newer_fail_is_not_met() {
    let own = latest_case(&body(&[("100", "passed")]));
    let main = latest_case(&body(&[("300", "failed")]));
    assert_eq!(newer(own.clone(), main.clone()).as_deref(), Some("fail"));
    assert_eq!(newer(main, own).as_deref(), Some("fail"), "order of the two answers does not matter");
    assert_eq!(newer(None, None), None, "no answer is never a pass");
}

const VALIDATE: &str = r#"{"data":{"latest":{"head":"cdd700cdcd54aa","rows":[
  {"domain":"tests","class":"Test (declared domain)","missing":[],"stale":[]},
  {"domain":"endpoints","class":"Endpoint","missing":[],"stale":["GET /a","GET /b"]},
  {"domain":"tests","class":"Test (files)","missing":["x.rs"],"stale":[]}]}}}"#;

#[test]
fn a_crawl_check_reads_one_rows_counts() {
    assert_eq!(crawl_row(VALIDATE, "Test (declared domain)"), Some(("cdd700cdcd54aa".into(), "missing=0 stale=0".into())));
    assert_eq!(crawl_row(VALIDATE, "Test (files)").map(|r| r.1), Some("missing=1 stale=0".into()));
    assert_eq!(crawl_row(VALIDATE, "No such class"), None);
}

#[test]
fn the_card_line_names_a_crawl_check_and_its_finish_line() {
    let desc = "## AC\n- [x] ac: the 8 files get a header  check: crawl Test (declared domain)\n";
    let c = &checks_from_card(4805, desc)[0];
    assert_eq!((c.kind.as_str(), c.target.as_str(), c.expect.as_str()), ("crawl", "Test (declared domain)", "missing=0"));
    assert!(matches!(judge(c, &Some("missing=0 stale=0 head=cdd700cdc".into())), Outcome::Met(_)));
}

// NEGATIVE PROOF: missing>0 is not met; no crawl since the land is unmeasured
#[test]
fn a_crawl_with_gaps_or_before_the_land_never_proves() {
    let c = &checks_from_card(1, "## AC\n- [ ] a  check: crawl Test (files)\n")[0];
    assert!(matches!(judge(c, &Some("missing=1 stale=0 head=abc".into())), Outcome::NotMet(_)));
    match judge(c, &None) {
        Outcome::Unmeasured(e) => assert!(e.contains("no crawl of main at or after the land"), "{e}"),
        o => panic!("{o:?}"),
    }
}
