// @test-type: unit — pure: the vouch decision over a witness string; no door, no network.
// @domain: decisions
// @card: #4875
//! #4875 AC2/AC3: the go reaches Jeff only when every gate passed for this round, the owner
//! vouched with what was shown, proven and not proven, and the navigator (a different role)
//! vouched too. Each missing piece is named; a negative proof for each.
use werk_demo::{jeff_go_message, vouch_line, vouch_missing};

const R: &str = "round-abc";
const P: &str = "patch-1";

fn gates(result: &str) -> String {
    ["product", "code", "quality", "arch", "ops"].iter()
        .map(|g| format!("{{\"ts\":1,\"event\":\"demo.gate.result\",\"role\":\"wren\",\"card_id\":7,\"trace_id\":\"t\",\"gate\":\"{g}\",\"result\":\"{result}\",\"round\":\"{R}\"}}\n"))
        .collect()
}
fn owner() -> String { vouch_line(7, "wren", "owner", R, P, "http://shown/7", "AC1-3 by demo-order.bats", "AC5 moderator", "") }
fn nav() -> String { vouch_line(7, "kade", "navigator", R, P, "", "", "", "reviewed every slice") }

#[test]
fn all_gates_green_and_both_vouches_leave_nothing_missing() {
    let w = gates("pass") + &owner() + &nav();
    assert!(vouch_missing(&w, 7, R, P, "wren").is_empty(), "{:?}", vouch_missing(&w, 7, R, P, "wren"));
    let m = jeff_go_message(&w, 7, R, P, "wren", "https://x/go/approvals");
    for want in ["http://shown/7", "AC1-3 by demo-order.bats", "AC5 moderator", "wren: I'd click go", "kade: I'd click go", "https://x/go/approvals"] {
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
