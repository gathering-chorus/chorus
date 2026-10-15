// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4144 AC5 — a failed suite's text survives the next run. Before: one file per
//! unit under ~/.chorus/nightly-failures, overwritten by the next run, so the
//! 03:00 detail was gone after an 08:00 run.
use werk_test::nightly_run::{keep_fail_text, prune_run_dirs, run_fail_dir};

fn world(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("wt-4144-{}-{}-{}", tag, std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn the_first_runs_failure_text_is_still_there_after_the_second_run() {
    let d = world("two-runs");
    let dir = d.to_string_lossy().to_string();
    keep_fail_text(&dir, "nr-100-1", "bats-x.log", "run one: assertion A failed");
    keep_fail_text(&dir, "nr-200-2", "bats-x.log", "run two: assertion B failed");
    let first = std::fs::read_to_string(format!("{}/bats-x.log", run_fail_dir(&dir, "nr-100-1"))).unwrap();
    assert_eq!(first, "run one: assertion A failed");
    // the top-level file is still the latest, for the readers that read it
    assert_eq!(std::fs::read_to_string(d.join("bats-x.log")).unwrap(), "run two: assertion B failed");
}

// NEGATIVE PROOF — the top-level file alone (the old behaviour) loses run one.
#[test]
fn negative_proof_the_top_level_file_alone_keeps_only_the_last_run() {
    let d = world("top-only");
    let dir = d.to_string_lossy().to_string();
    keep_fail_text(&dir, "nr-100-1", "bats-x.log", "run one");
    keep_fail_text(&dir, "nr-200-2", "bats-x.log", "run two");
    assert_ne!(std::fs::read_to_string(d.join("bats-x.log")).unwrap(), "run one");
}

#[test]
fn only_the_newest_run_folders_are_kept() {
    let d = world("prune");
    let dir = d.to_string_lossy().to_string();
    for id in ["nr-100-1", "nr-200-2", "nr-300-3"] {
        keep_fail_text(&dir, id, "a.log", id);
    }
    assert_eq!(prune_run_dirs(&dir, 2), 1);
    assert!(!std::path::Path::new(&run_fail_dir(&dir, "nr-100-1")).exists());
    assert!(std::path::Path::new(&run_fail_dir(&dir, "nr-300-3")).exists());
}
