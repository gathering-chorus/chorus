// @test-type: unit
// @domain: identity
//! #4925 round 4: person_signed_in reads the principal reply as JSON.
//! 11-22 16:50 (Silas): Jeff was signed in (session jeff-5030), but the check looked for the
//! text `"principalKind":"person"` while the door answers `"principalKind": "person"` (a
//! space), so no browser sign-in ever counted and a role's self-logout was refused.
use chorus_principal::rows::is_person_reply;

// the door's shape, pretty-printed as it answered on 11-22, then curl's "\n<code>"
const PERSON_SPACED: &str = "{\n  \"data\": {\n    \"name\": \"jeff\",\n    \"principalKind\": \"person\"\n  }\n}\n200";

#[test]
fn the_doors_spaced_reply_for_a_person_counts() {
    assert!(is_person_reply(PERSON_SPACED));
    assert!(is_person_reply("{\"data\":{\"principalKind\":\"person\"}}\n200"), "compact JSON counts too");
}

#[test]
fn negative_proof_the_old_text_match_misses_the_doors_reply() {
    // the line this replaced: red on the reply the door really sends
    assert!(!PERSON_SPACED.contains("\"principalKind\":\"person\""));
}

#[test]
fn negative_proof_a_non_person_or_a_failed_read_never_counts() {
    assert!(!is_person_reply("{\"data\": {\"principalKind\": \"agent\"}}\n200"));
    assert!(!is_person_reply("{\"data\": {\"principalKind\": \"service\"}}\n200"));
    assert!(!is_person_reply("{\"data\": {\"principalKind\": \"person\"}}\n404"), "a non-200 is not a person");
    assert!(!is_person_reply("{\"data\": {}}\n200"), "no kind is not a person");
    assert!(!is_person_reply("<html>Cannot GET</html>\n200"), "an unparseable body is not a person");
    assert!(!is_person_reply(""), "no answer is not a person");
    // a person kind mentioned elsewhere in the reply is not the row's own kind
    assert!(!is_person_reply("{\"data\": {\"principalKind\": \"agent\", \"note\": \"\\\"principalKind\\\": \\\"person\\\"\"}}\n200"));
}
