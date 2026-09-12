//! #4152 — the three nightly reds of 2026-09-12, each with its negative proof.
use std::process::Command;
use std::time::{Duration, Instant};
use werk_test::{plan_parallel_units, run_capped};

/// AC1 — a child that writes more than a pipe buffer to stderr and exits 0
/// must come back in seconds with rc 0. Before the drain fix the child blocks
/// on its full 64 KB stderr pipe and run_capped kills it at the cap (rc 124):
/// the exact shape of the chorus-hooks coverage red.
#[test]
fn ac1_child_writing_1mb_stderr_returns_promptly_rc0() {
    let mut c = Command::new("sh");
    c.args(["-c", "yes xxxxxxxxxxxxxxx | head -c 1048576 >&2; echo done; exit 0"]);
    let t0 = Instant::now();
    let (rc, out) = run_capped(c, Duration::from_secs(8));
    let took = t0.elapsed();
    assert_eq!(rc, 0, "rc={} out(tail)={}", rc, out.chars().rev().take(80).collect::<String>());
    assert!(took < Duration::from_secs(5), "took {:?}: child was starved on its pipe", took);
    assert!(out.len() >= 1_048_576, "output was not drained: {} bytes", out.len());
    assert!(out.contains("done"), "stdout lost");
}

/// AC1 — the cap still kills a child that truly overruns.
#[test]
fn ac1_cap_still_kills_a_real_overrun() {
    let mut c = Command::new("sh");
    c.args(["-c", "sleep 30"]);
    let t0 = Instant::now();
    let (rc, out) = run_capped(c, Duration::from_secs(1));
    assert_eq!(rc, 124, "{}", out);
    assert!(t0.elapsed() < Duration::from_secs(5));
}

fn isolation_conf() -> Vec<String> {
    // #4030 — no compile-time manifest dir; cargo test runs with cwd = the crate
    let root = std::env::current_dir().expect("cwd").join("../../..");
    let text = std::fs::read_to_string(root.join("platform/scripts/nightly-isolation.conf"))
        .expect("platform/scripts/nightly-isolation.conf must exist");
    text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()
}

/// AC3 — with the committed isolation conf, the npm plan runs the cards
/// package alone and leaves platform/api in the pool.
#[test]
fn ac3_cards_runs_alone_in_the_npm_plan() {
    let conf = isolation_conf();
    let pkgs: Vec<String> = ["directing/products/cards", "platform/api", "platform/pulse"].iter().map(|s| s.to_string()).collect();
    let plan = plan_parallel_units(&pkgs, &|u| conf.iter().any(|e| e == u));
    assert_eq!(plan.serialized, vec!["directing/products/cards".to_string()], "conf line for cards missing: {:?}", conf);
    assert_eq!(plan.parallel, vec!["platform/api".to_string(), "platform/pulse".to_string()]);
}

/// AC3 negative proof — the same plan with the cards line removed from the
/// conf puts cards back in the pool. The guard above cannot pass vacuously.
#[test]
fn ac3_without_the_conf_line_cards_would_pool() {
    let conf: Vec<String> = isolation_conf().into_iter().filter(|l| l != "directing/products/cards").collect();
    let pkgs: Vec<String> = ["directing/products/cards", "platform/api"].iter().map(|s| s.to_string()).collect();
    let plan = plan_parallel_units(&pkgs, &|u| conf.iter().any(|e| e == u));
    assert!(plan.serialized.is_empty());
    assert_eq!(plan.parallel.len(), 2);
}
