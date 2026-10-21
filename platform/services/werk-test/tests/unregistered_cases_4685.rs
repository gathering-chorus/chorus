// @test-type: unit
// @domain: tests
//! #4685 — a card's own runs keep every Rust case they ran, new test files
//! included (src/main.rs drops an unmatched case; src/lib.rs resolves it).
//! Found by #4683's AC6 proof: its two new test files had no case events in
//! any of its runs, because they were not registered until the crawler synced
//! after the land, so borg-prove read their ACs unmeasured.
use werk_test::{parse_nextest_case_runs, resolve_cargo_case, unregistered_case_file};

const CRATE: &str = "platform/services/borg-prove";
const NEW: &str = "platform/services/borg-prove/tests/land_gate_4683.rs";

fn exists(p: &str) -> bool { p == NEW }

#[test]
fn an_integration_target_with_no_row_resolves_to_its_test_file() {
    assert_eq!(unregistered_case_file("borg-prove::land_gate_4683", CRATE, exists), Some(NEW.to_string()));
}

#[test]
fn negative_proof_a_lib_or_bin_case_or_a_missing_file_stays_unresolved() {
    assert_eq!(unregistered_case_file("borg-prove", CRATE, exists), None, "a lib case has no file of its own");
    assert_eq!(unregistered_case_file("borg-prove::bin/borg-prove", CRATE, exists), None);
    assert_eq!(unregistered_case_file("borg-prove::not_here", CRATE, exists), None, "never a file that is not there");
    assert_eq!(unregistered_case_file("other-crate::land_gate_4683", CRATE, exists), None, "another crate's target");
}

// AC2 — a new FAILING test file, never registered, is kept with its verdict:
// its case event names the file and the case, so the red has a name.
#[test]
fn a_new_failing_case_with_no_registered_row_is_kept_with_its_file_and_verdict() {
    let out = "        FAIL [   0.010s] borg-prove::land_gate_4683 one_red_ac_files_exactly_one_child\n";
    let runs = parse_nextest_case_runs(out);
    assert_eq!(runs.len(), 1);
    let (path, target, verdict) = &runs[0];
    assert_eq!(verdict, "fail");
    let fp = resolve_cargo_case(path, target, CRATE, &[], &[], exists);
    assert_eq!(fp.as_deref(), Some(NEW), "an unregistered failing case is never dropped");
}

// AC3 — the resolve the runner uses keeps an unmatched case; a resolve that
// only matched registered rows (the old drop) returns None here.
#[test]
fn negative_proof_resolve_keeps_a_case_no_registered_row_names() {
    assert!(resolve_cargo_case("land_gate_4683::every_ac_met", "borg-prove::land_gate_4683", CRATE, &[], &[], exists).is_some());
    assert!(werk_test::match_cargo_case_path("land_gate_4683::every_ac_met", CRATE, &[], &[]).is_none(),
        "the registered-only match drops it: the state this card fixes");
}
