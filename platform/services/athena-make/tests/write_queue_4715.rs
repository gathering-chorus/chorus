// @test-type: unit — hermetic: the door's writer lock under a planted burst; no store.
// @domain: domains — placed by chorus-crawl's domain rules (#4538)
// @card: #4715
// @owner: kade
//! #4715 AC4 — src/lib.rs `with_write_lock`. TDB2 takes one write transaction at
//! a time and the door served a thread per connection, so concurrent writes hit
//! Fuseki together. Every door write (single, /batch, /create-linked) now runs
//! inside one writer lock: a 5-writer burst is queued, never overlapped.
use athena_make::with_write_lock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn burst(locked: bool) -> usize {
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let hs: Vec<_> = (0..5).map(|_| {
        let (a, p) = (active.clone(), peak.clone());
        std::thread::spawn(move || {
            let write = || {
                let now = a.fetch_add(1, Ordering::SeqCst) + 1;
                p.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(40)); // a write in flight
                a.fetch_sub(1, Ordering::SeqCst);
            };
            if locked { with_write_lock(write) } else { write() }
        })
    }).collect();
    for h in hs { h.join().unwrap(); }
    peak.load(Ordering::SeqCst)
}

#[test]
fn a_five_writer_burst_runs_one_at_a_time() {
    assert_eq!(burst(true), 1, "two writes were in flight together");
}

// negative proof: the same burst without the lock overlaps, so the check above can fail
#[test]
fn negative_proof_without_the_lock_the_burst_overlaps() {
    assert!(burst(false) > 1, "the planted burst must overlap when nothing queues it");
}

#[test]
fn a_write_that_panicked_does_not_jam_the_queue() {
    let _ = std::thread::spawn(|| with_write_lock(|| panic!("a write died holding the lock"))).join();
    assert_eq!(with_write_lock(|| 7), 7, "the next write still runs");
}
