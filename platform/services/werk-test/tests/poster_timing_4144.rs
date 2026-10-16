// @test-type: perf — a wall-clock limit on the poster hand-off; in-process, no live service
// @domain: builds
// @needs: nightly — a 50ms wall-clock limit measures the box (#4539: card runs leave it to the nightly)
//! #4144 AC3, timed — the half of poster_4144.rs that reads the clock: three
//! hand-offs to a poster whose posts each take 100ms return in under 50ms,
//! because the worker never waits on the post.
use std::time::{Duration, Instant};
use werk_test::{spawn_poster, PostJob};

#[test]
fn three_hand_offs_return_before_one_post_would() {
    let (tx, poster) = spawn_poster(|job: &PostJob<u32>| {
        std::thread::sleep(Duration::from_millis(100));
        job.expected
    });
    let t = Instant::now();
    for i in 0..3 {
        tx.send(PostJob { unit: format!("u{i}"), payload: i, expected: 2 }).unwrap();
    }
    assert!(t.elapsed() < Duration::from_millis(50), "three hand-offs took {:?} — the worker waited on the post", t.elapsed());
    drop(tx);
    assert_eq!(poster.join().unwrap(), 6);
}
