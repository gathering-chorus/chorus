// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4144 AC3 — a unit's results post off the worker's path. Measured on the
//! 2026-10-11 03:00 nightly: 444 one-unit posts, 7.9 min of writeback, each
//! holding its pool worker ~1.07 s before the next unit could start (≈5 min of
//! wall at the run's 1.54 average concurrency). The worker now hands the post
//! to one poster and moves on; the run waits for the poster before it counts.
use std::time::Duration;
use werk_test::{spawn_poster, PostJob};

#[test]
fn a_worker_hands_off_its_post_and_moves_on() {
    let (tx, poster) = spawn_poster(|job: &PostJob<u32>| {
        std::thread::sleep(Duration::from_millis(100));
        job.expected
    });
    for i in 0..3 {
        tx.send(PostJob { unit: format!("u{i}"), payload: i, expected: 2 }).unwrap();
    }
    // #4539 — how fast the hand-off is, is a wall-clock limit: poster_timing_4144.rs, nightly only
    drop(tx);
    assert_eq!(poster.join().unwrap(), 6, "every queued post lands before the run counts");
}

#[test]
fn posts_land_in_the_order_the_units_finished() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let s = seen.clone();
    let (tx, poster) = spawn_poster(move |job: &PostJob<()>| { s.lock().unwrap().push(job.unit.clone()); 1 });
    for u in ["a", "b", "c"] {
        tx.send(PostJob { unit: u.into(), payload: (), expected: 1 }).unwrap();
    }
    drop(tx);
    poster.join().unwrap();
    assert_eq!(*seen.lock().unwrap(), vec!["a", "b", "c"]);
}

// NEGATIVE PROOF — a post that stores nothing is counted short, so the run's
// expected-vs-stored check (#4015 RESULTS LOST) still sees it; never silent.
#[test]
fn negative_proof_a_failed_post_is_counted_short() {
    let (tx, poster) = spawn_poster(|job: &PostJob<bool>| if job.payload { job.expected } else { 0 });
    tx.send(PostJob { unit: "ok".into(), payload: true, expected: 3 }).unwrap();
    tx.send(PostJob { unit: "lost".into(), payload: false, expected: 2 }).unwrap();
    drop(tx);
    let stored = poster.join().unwrap();
    assert_eq!(stored, 3);
    assert_eq!(werk_test::results_lost(5, stored), 2);
}
