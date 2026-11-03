//! @test-type unit
//! #4882 AC4 — the product gate decides from proof: AC boxes are set from the checks' results.
use borg_prove::{ac_lines, tick_from_results, Outcome};

const DESC: &str = "## Experience\nx\n\n## AC\n- [ ] AC1: a. check: test a.rs\n- [x] AC2: b. check: test b.rs\n- [ ] AC3: c (by hand)\n- [x] AC4: d. check: test d.rs\nNot in scope: y\n";

fn results(o: [Outcome; 4]) -> Vec<(borg_prove::Check, Outcome)> {
    let checks = ac_lines(4882, DESC).into_iter().map(|(_, c)| c);
    checks.zip(o).collect()
}

#[test]
fn a_proven_but_unticked_ac_is_ticked_4882() {
    let r = results([Outcome::Met("pass".into()), Outcome::Met("pass".into()), Outcome::Unmeasured("no check".into()), Outcome::Met("pass".into())]);
    let (d, changed) = tick_from_results(DESC, &r);
    assert_eq!(changed, 1);
    let boxes: Vec<bool> = ac_lines(4882, &d).into_iter().map(|(b, _)| b).collect();
    assert_eq!(boxes, vec![true, true, false, true]);
    // negative proof (#3734): before ticking, the card reads AC1 unticked (what sent run 1 red)
    assert!(!ac_lines(4882, DESC)[0].0);
    assert!(d.contains("## Experience\nx\n") && d.ends_with("Not in scope: y\n"), "the rest of the card is untouched");
}

#[test]
fn a_ticked_ac_whose_check_failed_is_unticked_4882() {
    let r = results([Outcome::Met("pass".into()), Outcome::NotMet("fail".into()), Outcome::Unmeasured("none".into()), Outcome::Met("pass".into())]);
    let (d, _) = tick_from_results(DESC, &r);
    assert!(!ac_lines(4882, &d)[1].0, "a hand tick does not survive a failed check");
}

#[test]
fn an_ac_with_no_answer_keeps_its_box_4882() {
    let r = results([Outcome::Unmeasured("loki down".into()), Outcome::Unmeasured("x".into()), Outcome::Unmeasured("x".into()), Outcome::Deferred("by".into())]);
    let (d, changed) = tick_from_results(DESC, &r);
    assert_eq!((d.as_str(), changed), (DESC, 0), "no proof either way: nothing changes");
}
