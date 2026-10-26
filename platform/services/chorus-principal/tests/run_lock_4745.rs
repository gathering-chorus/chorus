// @test-type: unit
// #4745 — 4328-session-live red: wren's one session had 3 live runs, written by
// three processes in 600 ms that each read "no live run" before any saved one.
// Covers platform/services/chorus-principal/src/lib.rs (run_lock, record_run).
use chorus_principal::run_lock;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::time::Duration;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("run-lock-4745-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// record_run's shape: read the saved live run, end it, save a new live one.
/// Every process that read "none" leaves its own run live.
fn record(dir: &Path, id: usize, locked: bool) {
    let _held = if locked { Some(run_lock(dir, Duration::from_secs(10), Duration::from_secs(60)).expect("lock")) } else { None };
    let live = dir.join("live");
    let previous = fs::read_to_string(&live).ok();
    std::thread::sleep(Duration::from_millis(30)); // the API round trip
    if let Some(p) = previous { fs::remove_file(dir.join(format!("run-{}", p.trim()))).ok(); }
    fs::write(dir.join(format!("run-{id}")), "").unwrap();
    fs::write(&live, id.to_string()).unwrap();
}

fn live_runs_after_three_at_once(dir: &Path, locked: bool) -> usize {
    let gate = Arc::new(Barrier::new(3));
    let hs: Vec<_> = (0..3).map(|i| {
        let (g, d) = (gate.clone(), dir.to_path_buf());
        std::thread::spawn(move || { g.wait(); record(&d, i, locked) })
    }).collect();
    for h in hs { h.join().unwrap(); }
    fs::read_dir(dir).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with("run-")).count()
}

#[test]
fn three_starts_at_once_leave_one_live_run() {
    assert_eq!(live_runs_after_three_at_once(&scratch("locked"), true), 1);
}

/// NEGATIVE PROOF: the same three starts without the lock leave three live runs,
/// the state 4328 caught on wren, kade and silas.
#[test]
fn negative_proof_without_the_lock_three_runs_stay_live() {
    assert_eq!(live_runs_after_three_at_once(&scratch("unlocked"), false), 3);
}

#[test]
fn a_held_lock_refuses_a_second_holder_until_it_is_dropped() {
    let d = scratch("held");
    let first = run_lock(&d, Duration::from_millis(10), Duration::from_secs(60)).expect("first");
    assert!(run_lock(&d, Duration::from_millis(200), Duration::from_secs(60)).is_none(), "a second holder got the lock");
    drop(first);
    assert!(run_lock(&d, Duration::from_millis(200), Duration::from_secs(60)).is_some());
}

/// Kade's review: a slow holder that was taken over as stale must not, when it
/// finally finishes, remove the lock its successor now holds.
#[test]
fn a_taken_over_holder_does_not_release_its_successors_lock() {
    let d = scratch("successor");
    let slow = run_lock(&d, Duration::from_millis(10), Duration::from_secs(60)).expect("slow");
    std::thread::sleep(Duration::from_millis(1100));
    let next = run_lock(&d, Duration::from_millis(100), Duration::from_secs(1)).expect("took over the stale lock");
    drop(slow);
    assert!(run_lock(&d, Duration::from_millis(200), Duration::from_secs(60)).is_none(), "the slow holder released its successor's lock");
    drop(next);
    assert!(run_lock(&d, Duration::from_millis(200), Duration::from_secs(60)).is_some());
}

#[test]
fn a_dead_holders_lock_is_taken_over() {
    let d = scratch("stale");
    fs::create_dir_all(d.join("run.lock")).unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    assert!(run_lock(&d, Duration::from_millis(100), Duration::from_secs(1)).is_some(), "a stale lock blocked forever");
}
