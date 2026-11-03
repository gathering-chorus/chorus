// @test-type: unit — pure: the AC walk with box state; no board, no network.
// @domain: cards
// @card: #4875
//! #4875 (Jeff 09:28 "yes"): the go message reads the card's AC through borg-prove's one parser.
use borg_prove::{ac_lines, checks_from_card};

const DESC: &str = "## Experience\n- [x] not an AC, outside the section\n## AC\n- [x] AC1: the brief goes to the owner. check: test a.bats\n- [ ] AC2: the go needs both vouches. check: test b.rs\n## Notes\n- [ ] also not an AC";

#[test]
fn each_ac_under_the_ac_heading_carries_its_box_state() {
    let l = ac_lines(4875, DESC);
    assert_eq!(l.len(), 2, "only lines under ## AC");
    assert!(l[0].0 && l[0].1.statement == "AC1: the brief goes to the owner.");
    assert!(!l[1].0 && l[1].1.kind == "test");
}

#[test]
fn negative_proof_checks_from_card_is_the_same_walk() {
    // the prover and the go message cannot drift: same AC, same order, same statements
    let a: Vec<String> = ac_lines(4875, DESC).into_iter().map(|(_, c)| c.statement).collect();
    let b: Vec<String> = checks_from_card(4875, DESC).into_iter().map(|c| c.statement).collect();
    assert_eq!(a, b);
}
