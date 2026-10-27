// @test-type: unit
// @domain: code
// #4760 — borg-prove reads each AC from the card itself (src/lib checks_from_card):
// an AC line carries its own check (`check: <kind> <target> [expect]`), so the
// data exists when the card is written and is never copied into CardAC rows.
// A line with no check judges unmeasured and fails intake, naming the line.
use borg_prove::*;

const DESC: &str = "## Experience
Jeff sees the land prove itself.

## AC
- [ ] pulse answers  check: http /health 200
- [x] the store has no CardAC  check: sparql ASK { ?a a <https://jeffbridwell.com/chorus#CardAC> } false
- [ ] the parser reads lines  check: test platform/services/borg-prove/tests/card_ac_lines_4760.rs
- [ ] Jeff likes it

## Notes
- [ ] not an AC: outside the AC section  check: http /nope 200
";

#[test]
fn each_ac_line_in_the_ac_section_becomes_one_check_in_card_order() {
    let cs = checks_from_card(4760, DESC);
    assert_eq!(cs.len(), 4, "{cs:?}");
    let names: Vec<&str> = cs.iter().map(|c| c.ac.as_str()).collect();
    assert_eq!(names, ["ac-4760-1", "ac-4760-2", "ac-4760-3", "ac-4760-4"]);
    assert_eq!(cs[0].statement, "pulse answers");
}

#[test]
fn the_check_part_gives_kind_target_and_expect() {
    let cs = checks_from_card(4760, DESC);
    assert_eq!((cs[0].kind.as_str(), cs[0].target.as_str(), cs[0].expect.as_str()), ("http", "/health", "200"));
    assert_eq!(cs[1].kind, "sparql");
    assert_eq!(cs[1].target, "ASK { ?a a <https://jeffbridwell.com/chorus#CardAC> }");
    assert_eq!(cs[1].expect, "false");
    // a test check passes when the test's latest result is pass
    assert_eq!((cs[2].kind.as_str(), cs[2].expect.as_str()), ("test", "pass"));
    assert_eq!(cs[2].target, "platform/services/borg-prove/tests/card_ac_lines_4760.rs");
}

#[test]
fn an_http_check_with_no_expect_wants_200() {
    let cs = checks_from_card(1, "## AC\n- [ ] up  check: http /health\n");
    assert_eq!(cs[0].expect, "200");
}

// NEGATIVE PROOF: an AC with no check is never met, and intake names it
#[test]
fn an_ac_line_with_no_check_is_unmeasured_and_refused_at_intake() {
    let cs = checks_from_card(4760, DESC);
    assert_eq!(cs[3].kind, "none");
    assert_eq!(judge(&cs[3], &Some("anything".into())).word(), "unmeasured");
    let errs = intake(&cs).unwrap_err();
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("ac-4760-4") && errs[0].contains("Jeff likes it"), "{errs:?}");
}

// NEGATIVE PROOF: intake must go red on a card with no AC lines at all
#[test]
fn a_card_with_no_ac_lines_is_refused_at_intake() {
    let cs = checks_from_card(9, "## Experience\nno AC here\n");
    assert!(cs.is_empty());
    let errs = intake(&cs).unwrap_err();
    assert!(errs[0].contains("no AC"), "{errs:?}");
}

// NEGATIVE PROOF: a failing check is not met, so the land's verdict is not Proven
#[test]
fn a_failing_ac_check_holds_the_verdict() {
    let cs = checks_from_card(4760, "## AC\n- [ ] up  check: http /health 200\n");
    let o = judge(&cs[0], &Some("503".into()));
    assert_eq!(o.word(), "not met");
    assert_ne!(verdict(&[(cs[0].clone(), o)]), "Proven");
}

// a red AC's child card carries the parent's check on its one AC line, so the
// child passes intake at pull and proves the same thing (src/lib child_desc)
#[test]
fn a_child_card_carries_its_parents_check_and_passes_intake() {
    let parent = checks_from_card(4760, "## AC\n- [ ] up  check: http /health 200\n");
    let desc = child_desc(4760, &parent[0], &Outcome::NotMet("503".into()));
    let child = checks_from_card(4799, &desc);
    assert_eq!(child.len(), 1, "{desc}");
    assert_eq!((child[0].kind.as_str(), child[0].target.as_str(), child[0].expect.as_str()), ("http", "/health", "200"));
    assert!(intake(&child).is_ok(), "{desc}");
}
