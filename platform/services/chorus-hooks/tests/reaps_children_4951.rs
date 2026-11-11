// @test-type: unit — spawns `true` children from this test process and reads ps; no daemon, no store
// @domain: spine
// @card: #4951
//! #4951 — chorus-hooks and chorus-runner reap every child they spawn. On 2026-11-10
//! chorus-hooks (pid 4376, up 2d 11h) held 2,285 <defunct> children, jeffbridwell hit
//! kern.maxprocperuid (2,666), and every fork as Jeff failed: dagu, deep-health, curl,
//! the roles door, his own zsh. The daemon spawned helpers and dropped the Child.
use std::process::{Command, Stdio};

/// <defunct> children of this process right now.
fn my_zombies() -> usize {
    let me = std::process::id().to_string();
    let out = Command::new("ps").args(["-A", "-o", "ppid=,stat="]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines()
        .filter(|l| { let mut f = l.split_whitespace(); f.next() == Some(me.as_str()) && f.next().is_some_and(|s| s.starts_with('Z')) })
        .count()
}

fn settle() { std::thread::sleep(std::time::Duration::from_millis(400)); }

/// NEGATIVE PROOF: today's shape, spawn and drop the Child, leaves zombies, and this
/// counter sees them. Without this the reaped test below could pass vacuously.
#[test]
fn negative_proof_a_dropped_child_is_a_zombie_and_is_counted() {
    let before = my_zombies();
    for _ in 0..5 {
        let _ = Command::new("true").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }
    settle();
    assert!(my_zombies() >= before + 5, "the counter cannot see dropped children");
}

/// AC1 — the helper every fire-and-forget site uses leaves no zombie.
#[test]
fn ac1_spawn_reaped_leaves_no_zombie() {
    // runs in its own process so the negative proof's zombies are not counted here
    if std::env::var("REAP_CHILD").is_err() {
        let exe = std::env::current_exe().unwrap();
        let ok = Command::new(exe).args(["--exact", "ac1_spawn_reaped_leaves_no_zombie", "--nocapture"])
            .env("REAP_CHILD", "1").status().unwrap();
        assert!(ok.success());
        return;
    }
    for _ in 0..5 {
        let mut c = Command::new("true");
        c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        chorus_hooks::shared::reap::spawn_reaped(&mut c).expect("spawned");
    }
    settle();
    assert_eq!(my_zombies(), 0, "spawn_reaped left zombies");
}

/// The lines of `src` that throw a spawned Child away: a statement that ends in
/// `.spawn()` (a trailing comment allowed) or `.spawn().ok()`, or a forgotten Child.
fn dropped_spawns(src: &str) -> Vec<usize> {
    src.lines().enumerate().filter_map(|(i, l)| {
        let code = l.split("//").next().unwrap_or("").trim();
        let dropped = code.ends_with(".spawn();") || code.ends_with(".spawn().ok();") || code.contains("mem::forget(child)");
        dropped.then_some(i + 1)
    }).collect()
}

/// NEGATIVE PROOF for the guard (code review, round 3fce0a9): each shape it must catch.
#[test]
fn negative_proof_the_guard_sees_every_dropped_shape() {
    for s in ["        .spawn();", "        .spawn(); // fire-and-forget", "    let _ = c.spawn();", "    c.spawn().ok();", "    std::mem::forget(child);"] {
        assert_eq!(dropped_spawns(s), vec![1], "missed: {s}");
    }
    for s in ["    let child = c.spawn()?;", "    spawn_reaped(&mut c)?;", "    // .spawn();"] {
        assert!(dropped_spawns(s).is_empty(), "false hit: {s}");
    }
}

/// AC1 guard — no source in chorus-hooks or chorus-runner drops a spawned Child. A
/// new one fails here, naming the file and line.
#[test]
fn ac1_no_site_drops_a_spawned_child() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut offenders = Vec::new();
    let mut walk = vec![std::path::PathBuf::from(format!("{crate_dir}/src")), std::path::PathBuf::from(format!("{crate_dir}/../chorus-runner/src"))];
    while let Some(d) = walk.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() { walk.push(p); continue; }
            if p.extension().and_then(|x| x.to_str()) != Some("rs") { continue; }
            let text = std::fs::read_to_string(&p).unwrap();
            for n in dropped_spawns(&text) { offenders.push(format!("{}:{n}", p.display())); }
        }
    }
    assert!(offenders.is_empty(), "children spawned and never waited on:\n{}", offenders.join("\n"));
}
