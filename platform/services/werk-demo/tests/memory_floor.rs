// #3625 AC2 — memory floor before spawning headless claude gates/reviews.
//
// Jeff's experience under test: a demo never piles gate claudes onto a box
// already spiraling (2026-07-07: swap 4.7→20GB in 15 min → hard power-off).
// When memory is tight the spawn DEFERS (bounded wait), and if pressure never
// clears the gate records a visible memory-pressure error instead of running.

use werk_demo::{
    memory_floor_verdict, parse_free_pct, parse_swap_used_bytes, wait_for_floor_core,
};

const GB: u64 = 1024 * 1024 * 1024;

// --- parsing the real macOS tool output shapes ---

#[test]
fn parses_swap_used_from_sysctl_vm_swapusage() {
    let out = "vm.swapusage: total = 2048.00M  used = 1021.88M  free = 1026.12M  (encrypted)";
    let bytes = parse_swap_used_bytes(out).unwrap();
    assert_eq!(bytes, (1021.88 * 1024.0 * 1024.0) as u64);
}

#[test]
fn parses_swap_used_in_gb_units() {
    let out = "vm.swapusage: total = 20480.00M  used = 19.50G  free = 0.50G  (encrypted)";
    let bytes = parse_swap_used_bytes(out).unwrap();
    assert_eq!(bytes, (19.5 * 1024.0 * 1024.0 * 1024.0) as u64);
}

#[test]
fn swap_parse_fails_open_on_garbage() {
    assert_eq!(parse_swap_used_bytes("no such oid"), None);
    assert_eq!(parse_swap_used_bytes(""), None);
}

#[test]
fn parses_free_pct_from_memory_pressure() {
    let out = "The system has 17179869184 (1048576 pages with a page size of 16384).\n\
               System-wide memory free percentage: 47%\n";
    assert_eq!(parse_free_pct(out), Some(47));
}

#[test]
fn free_pct_parse_fails_open_on_garbage() {
    assert_eq!(parse_free_pct("memory_pressure: not found"), None);
}

// --- the floor decision ---

#[test]
fn proceeds_when_both_metrics_are_healthy() {
    assert!(memory_floor_verdict(Some(3 * GB), Some(40), 8 * GB, 10));
}

#[test]
fn defers_when_swap_is_over_the_floor() {
    // The incident signature: swap past 8GB while % free still looked okay.
    assert!(!memory_floor_verdict(Some(9 * GB), Some(30), 8 * GB, 10));
}

#[test]
fn defers_when_free_pct_is_under_the_floor() {
    assert!(!memory_floor_verdict(Some(2 * GB), Some(5), 8 * GB, 10));
}

#[test]
fn unmeasurable_metrics_fail_open() {
    // Hosted CI / non-macOS: no sysctl, no memory_pressure — never block there.
    assert!(memory_floor_verdict(None, None, 8 * GB, 10));
    // One measurable + healthy, one absent → still proceed.
    assert!(memory_floor_verdict(Some(2 * GB), None, 8 * GB, 10));
    // But a measurable BAD metric still defers even if the other is absent.
    assert!(!memory_floor_verdict(Some(9 * GB), None, 8 * GB, 10));
}

// --- the bounded wait ---

#[test]
fn wait_returns_immediately_when_first_sample_is_healthy() {
    let samples = vec![(Some(3 * GB), Some(40))];
    let (ok, checks) = wait_for_floor_core(samples.into_iter(), 8 * GB, 10, 20);
    assert!(ok);
    assert_eq!(checks, 1);
}

#[test]
fn wait_clears_when_pressure_drains_mid_wait() {
    let samples = vec![
        (Some(9 * GB), Some(20)), // tight
        (Some(9 * GB), Some(25)), // still tight
        (Some(5 * GB), Some(35)), // drained — go
    ];
    let (ok, checks) = wait_for_floor_core(samples.into_iter(), 8 * GB, 10, 20);
    assert!(ok);
    assert_eq!(checks, 3);
}

#[test]
fn wait_gives_up_after_max_checks_and_reports_refusal() {
    let samples = std::iter::repeat((Some(12 * GB), Some(8)));
    let (ok, checks) = wait_for_floor_core(samples, 8 * GB, 10, 5);
    assert!(!ok);
    assert_eq!(checks, 5);
}
