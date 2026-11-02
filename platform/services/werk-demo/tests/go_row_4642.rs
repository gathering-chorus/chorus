// @test-type: unit — pure: the go row body and the list link; no door, no network.
// @domain: decisions
// @card: #4642
//! #4642 — src/lib.rs `go_approval_body` / `approvals_link`: a ready demo puts its go on Jeff's
//! one list as a row whose action is exactly {card, run, round} at the werk's full HEAD; the
//! announce points at the list, never a per-run sign link.
use werk_demo::{approvals_link, go_approval_body, go_ask};

const HEAD: &str = "5174a0592261bbd5b452ec1287cda4e5e2fda45a";

#[test]
fn the_go_row_carries_kind_go_and_an_action_of_exactly_card_run_round() {
    let b = go_approval_body(4642, "034qFPOewc9praHJbg6SYH", HEAD, "Approvals round 2").unwrap();
    assert!(b.contains(r#""approvalKind":"go""#), "{b}");
    assert!(b.contains(r#""name":"approval-go-4642-034qFPOewc9praHJbg6SYH""#), "{b}");
    // the action is a JSON string holding the 3-key object chorus-go's go_action accepts
    let want = format!(r#""approvalAction":"{{\"card\":4642,\"run\":\"034qFPOewc9praHJbg6SYH\",\"round\":\"{HEAD}\"}}""#);
    assert!(b.contains(&want), "{b}\nwant {want}");
    assert!(b.contains(r##""approvalSubject":"#4642 Approvals round 2""##), "{b}");
}

#[test]
fn a_title_with_quotes_stays_valid_json() {
    let b = go_approval_body(1, "r1", HEAD, r#"say "go" \ now"#).unwrap();
    assert!(b.contains(r##""approvalSubject":"#1 say \"go\" \\ now""##), "{b}");
}

#[test]
fn negative_proof_a_short_round_or_a_bad_run_posts_no_row() {
    assert!(go_approval_body(4642, "r1", "5174a0592261", "t").unwrap_err().contains("full HEAD"));
    assert!(go_approval_body(4642, "r1", &HEAD.to_uppercase(), "t").unwrap_err().contains("full HEAD"));
    assert!(go_approval_body(4642, "a b", HEAD, "t").unwrap_err().contains("not an id"));
    assert!(go_approval_body(4642, "", HEAD, "t").unwrap_err().contains("not an id"));
}

#[test]
fn the_announce_points_at_the_one_list_not_a_per_run_link() {
    let l = approvals_link();
    assert!(l.ends_with("/go/approvals"), "{l}");
    assert!(!l.contains("card=") && !l.contains("run="), "{l}");
}

#[test]
fn a_posted_go_row_asks_on_the_list() {
    let a = go_ask("Demo ready — #4642", &Ok(()), 4642, "034run", "wren");
    assert!(a.ends_with(" · decide: https://lightlifeurbangardens.com/go/approvals") || a.contains("/go/approvals"), "{a}");
    assert!(!a.contains("sign: "), "{a}");
}

// #4642 NEGATIVE PROOF (its own 19:57 demo): the door refused the go row (its model had no kind
// go yet). The demo must still ask, on the page today's chorus-go serves, and say why.
#[test]
fn negative_proof_a_refused_go_row_still_asks_with_the_sign_page_and_the_reason() {
    let a = go_ask("Demo ready — #4642", &Err("the door answered 403 to the go row".into()), 4642, "034run", "wren");
    assert!(a.contains("sign: ") && a.contains("/go/?card=4642&run=034run&role=wren"), "{a}");
    assert!(a.contains("go row not on the list: the door answered 403"), "{a}");
}
