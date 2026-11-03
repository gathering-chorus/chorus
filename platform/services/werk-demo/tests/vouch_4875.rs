// @test-type: unit — pure: the vouch decision over a witness string; no door, no network.
// @domain: decisions
// @card: #4875
//! #4875 AC2/AC3: the go reaches Jeff only when every gate passed for this round, the owner
//! vouched with what was shown, proven and not proven, and the navigator (a different role)
//! vouched too. Each missing piece is named; a negative proof for each.
use werk_demo::{may_post_go, ac_checklist, ac_unfinished, borg_prove_beside, go_already_asked, jeff_go_message, reachable_link, vouch_line, vouch_missing};

const R: &str = "round-abc";
const P: &str = "patch-1";

fn gates(result: &str) -> String {
    ["product", "code", "quality", "arch", "ops"].iter()
        .map(|g| format!("{{\"ts\":1,\"event\":\"demo.gate.result\",\"role\":\"wren\",\"card_id\":7,\"trace_id\":\"t\",\"gate\":\"{g}\",\"result\":\"{result}\",\"round\":\"{R}\",\"patch_id\":\"{P}\"}}\n"))
        .collect()
}
fn owner() -> String { vouch_line(7, "wren", "owner", R, P, "http://192.168.86.36:3503/shown/7", "AC1-3 by demo-order.bats", "AC5 moderator", "") }
fn nav() -> String { vouch_line(7, "kade", "navigator", R, P, "", "", "", "reviewed every slice") }

#[test]
fn all_gates_green_and_both_vouches_leave_nothing_missing() {
    let w = gates("pass") + &owner() + &nav();
    assert!(vouch_missing(&w, 7, R, P, "wren").is_empty(), "{:?}", vouch_missing(&w, 7, R, P, "wren"));
    let m = jeff_go_message(&w, 7, R, P, "wren", "https://x/go/approvals", "✓ AC1: the brief goes to the owner");
    for want in ["http://192.168.86.36:3503/shown/7", "AC1-3 by demo-order.bats", "AC5 moderator", "wren: I'd click go", "kade: I'd click go", "https://x/go/approvals", "✓ AC1: the brief goes to the owner"] {
        assert!(m.contains(want), "{want} missing from: {m}");
    }
}

#[test]
fn negative_proof_a_red_gate_blocks_the_go() {
    let w = gates("fail") + &owner() + &nav();
    assert!(vouch_missing(&w, 7, R, P, "wren").iter().any(|m| m.contains("product fail")));
}

#[test]
fn negative_proof_no_navigator_vouch_blocks_the_go() {
    let w = gates("pass") + &owner();
    assert!(vouch_missing(&w, 7, R, P, "wren").iter().any(|m| m.contains("navigator")));
}

#[test]
fn negative_proof_the_owner_cannot_vouch_as_their_own_navigator() {
    let self_nav = vouch_line(7, "wren", "navigator", R, P, "", "", "", "me again");
    let w = gates("pass") + &owner() + &self_nav;
    assert!(vouch_missing(&w, 7, R, P, "wren").iter().any(|m| m.contains("navigator")));
}

#[test]
fn negative_proof_an_owner_vouch_without_a_show_blocks_the_go() {
    let unshown = vouch_line(7, "wren", "owner", R, P, "", "tests", "none", "");
    let w = gates("pass") + &unshown + &nav();
    assert!(vouch_missing(&w, 7, R, P, "wren").iter().any(|m| m.contains("shown")));
}

#[test]
fn negative_proof_a_vouch_for_another_round_does_not_count() {
    let stale = vouch_line(7, "kade", "navigator", "old-round", "old-patch", "", "", "", "old");
    let w = gates("pass") + &owner() + &stale;
    assert!(vouch_missing(&w, 7, R, P, "wren").iter().any(|m| m.contains("navigator")));
}

#[test]
fn negative_proof_an_older_rounds_passing_gates_do_not_count() {
    // Kade b1c438aeb (2): gates passed for the old round; this round has no gate results.
    let old = gates("pass").replace(R, "old-round").replace(P, "old-patch");
    let w = old + &owner() + &nav();
    assert!(vouch_missing(&w, 7, R, P, "wren").iter().any(|m| m == "product -"), "{:?}", vouch_missing(&w, 7, R, P, "wren"));
}

#[test]
fn negative_proof_an_owner_vouch_with_nothing_proven_blocks_the_go() {
    // Kade b1c438aeb (3)
    let unproven = vouch_line(7, "wren", "owner", R, P, "http://192.168.86.36:3503/shown/7", "  ", "none", "");
    let w = gates("pass") + &unproven + &nav();
    assert!(vouch_missing(&w, 7, R, P, "wren").iter().any(|m| m.contains("nothing proven")));
}

#[test]
fn negative_proof_a_go_already_asked_this_round_is_not_asked_again() {
    // Kade b1c438aeb (1): a third vouch after both must not send Jeff a second row or message.
    let asked = format!("{{\"ts\":2,\"event\":\"demo.go.asked\",\"role\":\"wren\",\"card_id\":7,\"trace_id\":\"run\",\"round\":\"{R}\",\"patch_id\":\"{P}\"}}\n");
    let w = gates("pass") + &owner() + &nav();
    assert!(!go_already_asked(&w, 7, R, P), "not yet asked");
    assert!(go_already_asked(&(w.clone() + &asked), 7, R, P), "asked once");
    assert!(!go_already_asked(&(w + &asked.replace(R, "next").replace(P, "next-p")), 7, R, P), "another round's ask does not count");
}

#[test]
fn negative_proof_a_localhost_show_link_blocks_the_go() {
    // AC8 (Jeff 08:53): "all links must be wifi not localhost"
    for bad in ["http://localhost:3503/x", "http://127.0.0.1:3345/api", "https://0.0.0.0/", "file:///tmp/x.png", "see the terminal", "http://[::1]:3503/x", "http://[::]/"] {
        assert!(!reachable_link(bad), "{bad}");
        let o = vouch_line(7, "wren", "owner", R, P, bad, "AC1-3", "", "");
        let w = gates("pass") + &o + &nav();
        assert!(vouch_missing(&w, 7, R, P, "wren").iter().any(|m| m.contains("not reachable")), "{bad}");
    }
    for ok in ["http://192.168.86.36:3503/rooms", "https://lightlifeurbangardens.com/clearing/rooms"] {
        assert!(reachable_link(ok), "{ok}");
    }
}

#[test]
fn the_go_message_lists_each_ac_checked_or_not() {
    // Jeff 09:17 "yes add the ac": one phone-sized line per AC, the check: receipt cut off.
    let view = "✓\tAC1: the announce goes to the owner.\ttest\n☐\tAC2: the go reaches Jeff only via the vouch.\ttest\nborg-prove: noise line";
    let c = ac_checklist(view);
    assert_eq!(c, "✓ AC1: the announce goes to the owner.\n☐ AC2: the go reaches Jeff only via the vouch.", "{c}");
    // negative proof: a view with no AC lines yields nothing, never a made-up list
    assert_eq!(ac_checklist("Status: WIP\nno boxes here"), "");
}

#[test]
fn negative_proof_one_unchecked_ac_blocks_the_go() {
    // Kade ba6102a8f / Jeff 09-09: "i NEVER want to be asked for go on a card with unfinished ac"
    let done = "✓\tAC1: a.\ttest\n✓\tAC2: b.\ttest";
    assert!(ac_unfinished(done).is_empty());
    let one_open = "✓\tAC1: a.\ttest\n☐\tAC2: b.\ttest";
    assert_eq!(ac_unfinished(one_open), vec!["unfinished AC2: b.".to_string()]);
}

#[test]
fn negative_proof_an_unreadable_card_blocks_the_go() {
    // Kade 47ef7e508: `cards view` failing gives "", which must refuse, not read as all-done.
    assert_eq!(ac_unfinished(""), vec!["card AC unreadable".to_string()]);
    assert_eq!(ac_unfinished("Status: WIP\nno boxes"), vec!["card AC unreadable".to_string()]);
}

#[test]
fn the_ac_is_read_by_the_borg_prove_beside_this_werk_demo() {
    // Kade 26af1b142: a slot's werk-demo must use its own borg-prove, not prod's (no `ac` yet).
    let d = std::env::temp_dir().join(format!("bp-4875-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("borg-prove"), "#!/bin/sh\n").unwrap();
    assert_eq!(borg_prove_beside(Some(&d), "wrapper".into()), d.join("borg-prove").to_string_lossy());
    // negative proof: with no sibling binary it falls back to the wrapper, never a missing path
    let empty = d.join("empty"); std::fs::create_dir_all(&empty).unwrap();
    assert_eq!(borg_prove_beside(Some(&empty), "wrapper".into()), "wrapper");
    assert_eq!(borg_prove_beside(None, "wrapper".into()), "wrapper");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_vouch_row_carries_a_real_time() {
    // Kade 09:37: vouch rows stamped "ts":0
    let l = vouch_line(7, "wren", "owner", R, P, "http://192.168.86.36:3503/x", "p", "", "");
    assert!(!l.starts_with("{\"ts\":0,"), "{l}");
}

#[test]
fn negative_proof_only_the_owners_call_posts_the_go() {
    assert!(may_post_go("wren", "wren"));
    assert!(!may_post_go("kade", "wren"), "a navigator vouching last records, never posts as the owner");
    assert!(!may_post_go("", ""), "no owner, no post");
}
