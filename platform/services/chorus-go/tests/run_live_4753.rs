// @test-type: unit — dagu's own status line as given; no dagu, no network.
// @domain: security
// @card: #4753
// @owner: kade
//! #4753 — src/lib.rs `go_run_open`: the go page opens only on a run still
//! waiting at the go. A run cicd-supersede stopped (a newer run of the card
//! exists) is refused before Jeff signs, by name, never "signed, but the run
//! did not take it" after he has.
use chorus_go::*;

#[test]
fn a_run_waiting_at_the_go_opens() {
    assert!(go_run_open("Waiting - 2026-10-26T11:20:47-04:00").is_ok());
}

/// NEGATIVE PROOF: a stopped (superseded) run, a finished run and an unreadable
/// status are each refused, with the reason in Jeff's words.
#[test]
fn a_superseded_finished_or_unknown_run_is_refused_before_signing() {
    let e = go_run_open("Aborted - 2026-10-26T12:20:00-04:00").unwrap_err();
    assert!(e.contains("superseded by a newer run"), "{e}");
    let e = go_run_open("Succeeded - 2026-10-26T12:20:00-04:00").unwrap_err();
    assert!(e.contains("already answered"), "{e}");
    let e = go_run_open("").unwrap_err();
    assert!(e.contains("cannot read"), "{e}");
}

/// #4771 — a pending go whose run cicd-supersede answered no (Succeeded, land skipped) is
/// dropped from the list like an aborted one.
/// NEGATIVE PROOF: a go Jeff signed (Succeeded, land ran), Waiting, Failed and an unreadable
/// status are never called superseded (Silas 17:29: "superseded" for his own go is false).
#[test]
fn an_answered_no_or_aborted_go_is_superseded_and_jeffs_landed_go_is_not() {
    use chorus_go::superseded_status;
    let no = "Succeeded - 2026-11-02T17:30:00-05:00\n\ndag: cicd-go\n├─skill-werk-go (1s) [succeeded]\n├─cicd-land (0s) [skipped]\n";
    let jeff = "Succeeded - 2026-11-02T16:39:52-05:00\n\ndag: cicd-go\n├─skill-werk-go (36m) [succeeded]\n├─cicd-land (1.0s) [succeeded]\n";
    assert!(superseded_status(no));
    assert!(superseded_status("Aborted - 2026-11-02T17:30:00-05:00"));
    assert!(!superseded_status(jeff), "Jeff's go is not superseded");
    assert!(!superseded_status("Succeeded - 2026-11-02T17:30:00-05:00"), "no step lines: not provably a no");
    assert!(!superseded_status("Waiting - 2026-11-02T17:30:00-05:00"));
    assert!(!superseded_status("Failed - 2026-11-02T17:30:00-05:00"));
    assert!(!superseded_status(""));
}

/// #4771 — newest_go_per_card keeps the go whose run STARTED last (from the UUIDv7 run id),
/// whatever order the rows posted in; card asks, other cards and unreadable ids stay.
/// NEGATIVE PROOF: the older run's row posted later (and in a different zone) is still dropped;
/// a lone go, another card's go, a card ask and an unreadable run id are never dropped.
#[test]
fn only_the_go_of_the_last_started_run_of_each_card_is_kept() {
    use serde_json::json;
    let g = |row: &str, card: u64, run: &str, at: &str| json!({"row": row, "kind": "go", "at": at,
        "action": format!(r#"{{"card":{card},"run":"{run}","round":"{}"}}"#, "a".repeat(40))});
    let rows = vec![
        g("run11", 4771, "034qq6XgiAeVGOv2S6DEhR", "2026-11-02T17:40:00-05:00"),   // started first, posted last
        g("run12", 4771, "034qq9S3EbXzSGzpLWcIXj", "2026-11-02T22:31:00Z"),
        g("other", 4642, "034qnl2lQX1God1BGw4iGq", "2026-11-02T20:00:00Z"),
        g("odd", 4771, "not-a-run-id", "x"),
        json!({"row": "ask", "kind": "card", "at": "x"})];
    let kept: Vec<String> = chorus_go::newest_go_per_card(rows).iter().map(|r| r["row"].as_str().unwrap().to_string()).collect();
    assert_eq!(kept, vec!["run12", "other", "odd", "ask"]);
    assert!(chorus_go::run_started_ms("go-034qq6XgiAeVGOv2S6DEhR") < chorus_go::run_started_ms("034qq9S3EbXzSGzpLWcIXj"));
    assert_eq!(chorus_go::run_started_ms("034qq6XgiAeVGOv2S6DEhR"), Some(1793658521365));
}
