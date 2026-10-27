// @test-type: unit
// @domain: code
// #4760 — the land runs record itself, as Borg Queen's account (src/lib
// queen_hop, src/main record): record run by anyone else hands itself to
// chorus-borg-queen through sudo -n, so the verdict is hers and nobody waits
// on her session. (#4691's ask / her step / check are retired with it.)

const Q: &str = "chorus-borg-queen";

// #4760 — the land runs record itself, as her account: record, run by anyone
// else, hands itself to chorus-borg-queen through sudo -n (src/lib queen_hop),
// so her verdict file still carries her signature and nobody waits on her.
#[test]
fn record_run_by_another_account_hops_to_hers_with_the_run_named() {
    use borg_prove::queen_hop;
    let hop = queen_hop("jeffbridwell", Q, "/x/borg-prove", "4760", "silas", "run1", "/home/chorus").unwrap();
    assert_eq!(hop, ["-n", "-u", Q, "/x/borg-prove", "record", "4760", "silas", "run1", "/home/chorus"]);
}

// NEGATIVE PROOF: run as her, record never hops again (no loop, no sudo)
#[test]
fn record_run_as_her_does_not_hop() {
    use borg_prove::queen_hop;
    assert_eq!(queen_hop(Q, Q, "/x/borg-prove", "4760", "silas", "run1", "/h"), None);
}
