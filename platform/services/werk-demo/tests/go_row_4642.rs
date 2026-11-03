// @test-type: unit — pure: the go row body and the list link; no door, no network.
// @domain: decisions
// @card: #4642
//! #4642 — src/lib.rs `go_approval_body` / `approvals_link`: a ready demo puts its go on Jeff's
//! one list as a row whose action is exactly {card, run, round} at the werk's full HEAD; the
//! announce points at the list, never a per-run sign link.
use werk_demo::{approvals_link, go_approval_body};

const HEAD: &str = "5174a0592261bbd5b452ec1287cda4e5e2fda45a";

#[test]
fn the_go_row_carries_kind_go_and_an_action_of_exactly_card_run_round() {
    let b = go_approval_body(4642, "034qFPOewc9praHJbg6SYH", HEAD, "Approvals round 2").unwrap();
    assert!(b.contains(r#""approvalKind":"go""#), "{b}");
    // #4771: the bare name; the door's mint adds "approval-" (a prefixed name is a 422 double-prefix)
    assert!(b.contains(r#""name":"go-4642-034qFPOewc9praHJbg6SYH""#), "{b}");
    assert!(!b.contains(r#""name":"approval-"#), "a prefixed name is refused by the door: {b}");
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

// #4875 retired go_ask and the per-run sign page: the go reaches Jeff only through the owner's
// vouch (`werk-demo vouch`), and a refused go row is a loud error to the owner, never a fallback ask.

/// #4771 — the go binds what this run pushed (and so tested), not the werk's HEAD at demo time.
/// NEGATIVE PROOF: another run's push, another card's push, a push.started line and a missing
/// trace bind nothing — 11-02 run 11 tested affda76 while its go stamped daf6848.
#[test]
fn the_go_binds_the_commit_this_run_pushed() {
    use werk_demo::pushed_sha;
    let a = "a".repeat(40); let b = "b".repeat(40); let c = "c".repeat(40);
    let log = format!(concat!(
        "{{\"ts\":1,\"event\":\"push.completed\",\"role\":\"kade\",\"card_id\":4771,\"trace_id\":\"run11\",\"sha\":\"{a}\"}}\n",
        "{{\"ts\":2,\"event\":\"push.completed\",\"role\":\"kade\",\"card_id\":4771,\"trace_id\":\"run12\",\"sha\":\"{b}\"}}\n",
        "{{\"ts\":3,\"event\":\"push.completed\",\"role\":\"kade\",\"card_id\":47710,\"trace_id\":\"run11\",\"sha\":\"{c}\"}}\n",
        "{{\"ts\":4,\"event\":\"push.started\",\"role\":\"kade\",\"card_id\":4771,\"trace_id\":\"run11\",\"sha\":\"{c}\"}}\n"),
        a = a, b = b, c = c);
    assert_eq!(pushed_sha(&log, 4771, "run11").as_deref(), Some(a.as_str()));
    assert_eq!(pushed_sha(&log, 4771, "run12").as_deref(), Some(b.as_str()));
    assert_eq!(pushed_sha(&log, 4771, "run13"), None, "a run with no push binds nothing");
    assert_eq!(pushed_sha(&log, 4771, ""), None);
}

/// #4771 — the go row's subject names the card once (Jeff 16:44: "#4771 #4771 …").
/// NEGATIVE PROOF: a title without the prefix keeps its text whole.
#[test]
fn the_go_row_names_the_card_once() {
    let b = go_approval_body(4771, "r1", HEAD, "#4771 Fix go automation").unwrap();
    assert!(b.contains(r##""approvalSubject":"#4771 Fix go automation""##), "{b}");
    let b = go_approval_body(4771, "r1", HEAD, "Fix go automation").unwrap();
    assert!(b.contains(r##""approvalSubject":"#4771 Fix go automation""##), "{b}");
}
