// #3519 — THE WITNESS-GATED LAND verdict (integration test).
//
// demo() present-and-exits (ALWAYS exit 2, #3279/#3511) — it can never signal "proven"
// via its exit code. werk.yml gated merge on demo exit-0 (#3499). The two CONTRADICT →
// the merge step never fired → nothing landed (101 landed historically, 0 in the recent
// window). The fix: the land keys on the WITNESS, not demo's exit code. verdict_proven is
// that predicate — proven iff, for THIS patch, the witness shows every required gate +
// BOTH peer gathers replied + Jeff's recorded go. This test pins the contract, including
// the exact #3519 failure mode: gates + gathers present but the go absent → NOT proven.
use werk_demo::verdict_proven;

const GATES: [&str; 5] = ["product", "code", "quality", "arch", "ops"];
const GO: &str = r#"{"ts":1,"event":"demo.go","role":"jeff","card_id":3519,"trace_id":"t","round":"r1","patch_id":"pX"}"#;

fn gate(card: u64, g: &str) -> String {
    format!(
        r#"{{"ts":1,"event":"demo.gate.result","role":"wren","card_id":{},"trace_id":"t","gate":"{}","round":"r1","result":"pass"}}"#,
        card, g
    )
}
fn gather(card: u64, peer: &str) -> String {
    format!(
        r#"{{"ts":1,"event":"demo.gather.replied","role":"silas","card_id":{},"trace_id":"t","peer":"{}","round":"r1","note":""}}"#,
        card, peer
    )
}

#[test]
fn verdict_proven_requires_gates_gathers_and_go() {
    let mut lines: Vec<String> = GATES.iter().map(|g| gate(3519, g)).collect();
    lines.push(gather(3519, "wren")); // peers of silas = wren, kade
    lines.push(gather(3519, "kade"));
    let full = format!("{}\n{}", lines.join("\n"), GO);

    // gates + both gathers + go all present for r1 → PROVEN (exit 0 → merge fires)
    assert!(
        verdict_proven(&full, 3519, "silas", "r1", "pX"),
        "gates + gathers + go all on the witness → proven"
    );

    // the exact #3519 failure mode: gates + gathers in, GO absent → NOT proven
    assert!(
        !verdict_proven(&lines.join("\n"), 3519, "silas", "r1", "pX"),
        "no go → not proven (presented, not landed)"
    );

    // a missing peer gather → NOT proven (the land must not fire on partial review)
    let no_kade = format!(
        "{}\n{}\n{}",
        GATES.iter().map(|g| gate(3519, g)).collect::<Vec<_>>().join("\n"),
        gather(3519, "wren"),
        GO
    );
    assert!(
        !verdict_proven(&no_kade, 3519, "silas", "r1", "pX"),
        "missing kade's gather → not proven"
    );
}

// #3944 — THE ROUND-IDENTITY REFUSALS.
//
// Jeff, 2026-08-20: "i expect an assertion via werk-demo that the card is ready" and
// "i said go and now the whole test cycle is running again thats wrong!"
//
// verdict_proven already takes (round, patch_id) — the identity assertion exists. What
// was never tested is that it REFUSES on a mismatch. Without these, the predicate cannot
// distinguish "the round Jeff approved" from "some round", which is the whole reason a
// land could not trust the witness and re-ran the pipeline instead.
//
// Both are negative proofs (#3734): each pins a state the check exists to reject.

#[test]
fn a_go_on_one_round_does_not_prove_another() {
    // Everything approved for r1. The werk then moved on: a new round r2 exists.
    // Landing r2 on r1's approval would merge content Jeff never saw.
    let mut lines: Vec<String> = GATES.iter().map(|g| gate(3519, g)).collect();
    lines.push(gather(3519, "wren"));
    lines.push(gather(3519, "kade"));
    let witness = format!("{}\n{}", lines.join("\n"), GO);

    assert!(
        verdict_proven(&witness, 3519, "silas", "r1", "pX"),
        "control: r1 IS proven"
    );
    assert!(
        !verdict_proven(&witness, 3519, "silas", "r2", "pX"),
        "r1's gates/gathers/go must NOT prove r2"
    );
}

#[test]
fn a_commit_after_the_demo_invalidates_the_proof() {
    // Same round, but the werk gained a commit — the patch id no longer matches what was
    // demoed. This is Jeff's case exactly: the approval was for pX; the tree is now pY.
    // The land must refuse and demand a fresh round, never merge unproven content.
    let mut lines: Vec<String> = GATES.iter().map(|g| gate(3519, g)).collect();
    lines.push(gather(3519, "wren"));
    lines.push(gather(3519, "kade"));
    let witness = format!("{}\n{}", lines.join("\n"), GO);

    assert!(
        !verdict_proven(&witness, 3519, "silas", "r1", "pY"),
        "a post-demo commit (pX -> pY) must NOT be proven by the old go"
    );
}

#[test]
fn another_cards_approval_does_not_prove_this_card() {
    // A go recorded for card 9999 must not land 3519.
    let other_go = r#"{"ts":1,"event":"demo.go","role":"jeff","card_id":9999,"trace_id":"t","round":"r1","patch_id":"pX"}"#;
    let mut lines: Vec<String> = GATES.iter().map(|g| gate(3519, g)).collect();
    lines.push(gather(3519, "wren"));
    lines.push(gather(3519, "kade"));
    let witness = format!("{}\n{}", lines.join("\n"), other_go);

    assert!(
        !verdict_proven(&witness, 3519, "silas", "r1", "pX"),
        "a go for another card must NOT prove this one"
    );
}
