// @domain: domains — placed by chorus-crawl's domain rules (#4538)
//! #4022 — an enum written as sh:pattern `^(a|b)$` names its legal values, so a
//! generated create can send one (Presence.reachability, Session.sessionState
//! answered 422 on every walk because the manifest carried none).
use athena_make::pattern_alternatives;

#[test]
fn a_plain_word_list_pattern_names_its_values() {
    assert_eq!(pattern_alternatives("^(open|closed)$"), vec!["open", "closed"]);
    assert_eq!(pattern_alternatives("^(reachable|unreachable|unknown)$").len(), 3);
}

/// NEGATIVE PROOF: a real regex is not a value list; reading one as words would
/// send `[0-9]+` as a value and the walk would go red on our own input.
#[test]
fn a_regex_that_is_not_a_word_list_names_nothing() {
    assert!(pattern_alternatives("^[0-9]+$").is_empty());
    assert!(pattern_alternatives("^(a|b.*)$").is_empty());
    assert!(pattern_alternatives("open|closed").is_empty());
    assert!(pattern_alternatives("^()$").is_empty());
}
