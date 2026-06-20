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
