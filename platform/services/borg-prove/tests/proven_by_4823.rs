// @test-type: unit
// @domain: tests
//! #4823 AC2 — an AC only a later card can prove (#4772 AC2, proven by #4805)
//! says so on its line, `check: by #4805`. The land is not stranded on it: the
//! AC reads "deferred to #4805" until that card is Done, then "proven by #4805".
use borg_prove::{checks_from_card, intake, judge, reds, results_from_tsv, results_to_tsv, verdict, Check, Outcome};

fn by(n: &str) -> Check {
    checks_from_card(4772, &format!("## AC\n- [ ] AC2: the runner isolates targets check: by #{n}\n")).remove(0)
}

#[test]
fn an_ac_proven_by_a_later_card_reads_its_card() {
    let c = by("4805");
    assert_eq!((c.kind.as_str(), c.target.as_str(), c.expect.as_str()), ("by", "4805", "Done"));
    assert!(intake(&[c]).is_ok(), "a `by #N` check is runnable");
}

#[test]
fn negative_proof_by_with_no_card_number_is_refused_at_intake() {
    let c = checks_from_card(4772, "## AC\n- [ ] AC2: x check: by soon\n").remove(0);
    assert!(intake(&[c]).is_err(), "`by` must name a card");
}

#[test]
fn the_later_card_done_proves_it_otherwise_it_is_deferred_never_met() {
    let c = by("4805");
    assert_eq!(judge(&c, &Some("Done".into())), Outcome::Met("proven by #4805 (Done)".into()));
    match judge(&c, &Some("Later".into())) {
        Outcome::Deferred(e) => assert!(e.contains("#4805") && e.contains("Later"), "{e}"),
        o => panic!("a later card not Done must defer, got {o:?}"),
    }
    assert!(matches!(judge(&c, &None), Outcome::Unmeasured(_)), "a board that does not answer proves nothing");
}

#[test]
fn a_deferred_ac_does_not_strand_the_land() {
    let met = (by("1"), Outcome::Met("ok".into()));
    let deferred = (by("4805"), Outcome::Deferred("deferred to #4805, which is Later".into()));
    assert_eq!(verdict(&[met.clone(), deferred.clone()]), "Proven");
    assert_eq!(deferred.1.word(), "deferred");
}

/// Negative proof: deferral never hides a real gap.
#[test]
fn negative_proof_deferral_never_hides_an_unmeasured_or_failed_ac() {
    let deferred = (by("4805"), Outcome::Deferred("deferred".into()));
    assert_eq!(verdict(&[(by("1"), Outcome::Unmeasured("no result".into())), deferred.clone()]), "Unproven");
    assert_eq!(verdict(&[(by("1"), Outcome::NotMet("red".into())), deferred.clone()]), "Not met");
    assert_eq!(verdict(&[deferred]), "Unproven", "a card whose every AC is deferred proves nothing itself");
}

/// A deferred AC files no "not proven" child (its card already exists), and
/// survives the prod → record hand-over (the results file).
#[test]
fn a_deferred_ac_files_no_child_and_round_trips_through_the_results_file() {
    let r = vec![(by("4805"), Outcome::Deferred("deferred to #4805, which is Later".into())), (by("1"), Outcome::Unmeasured("x".into()))];
    let red: Vec<&str> = reds(&r).iter().map(|(c, _)| c.target.as_str()).collect();
    assert_eq!(red, vec!["1"], "only the unmeasured AC gets a child");
    assert_eq!(results_from_tsv(&results_to_tsv(&r)).unwrap(), r);
}
