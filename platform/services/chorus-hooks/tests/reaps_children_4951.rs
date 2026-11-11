// @test-type: unit — spawns `true` children from this test process and reads ps; no daemon, no store
// @domain: chorus
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

/// AC1 guard — no source in chorus-hooks drops a spawned Child, and chorus-runner no
/// longer forgets its task children. A new `.spawn();` that throws the Child away
/// fails here, naming the file.
#[test]
fn ac1_no_site_drops_a_spawned_child() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut offenders = Vec::new();
    let mut walk = vec![std::path::PathBuf::from(format!("{crate_dir}/src"))];
    while let Some(d) = walk.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() { walk.push(p); continue; }
            if p.extension().and_then(|x| x.to_str()) != Some("rs") { continue; }
            let text = std::fs::read_to_string(&p).unwrap();
            for (i, l) in text.lines().enumerate() {
                let t = l.trim();
                if t.starts_with("//") { continue; }
                if t == ".spawn();" || (t.starts_with("let _ =") && t.ends_with(".spawn();")) {
                    offenders.push(format!("{}:{}", p.display(), i + 1));
                }
            }
        }
    }
    let runner = std::fs::read_to_string(format!("{crate_dir}/../chorus-runner/src/tasks.rs")).unwrap();
    if runner.contains("std::mem::forget(child)") { offenders.push("chorus-runner/src/tasks.rs: mem::forget(child)".into()); }
    assert!(offenders.is_empty(), "children spawned and never waited on:\n{}", offenders.join("\n"));
}
