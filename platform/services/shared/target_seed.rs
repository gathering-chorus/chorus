// #4494 — a new werk's crate starts from prod's build cache, not from nothing.
//
// Every card gets a fresh git worktree, so every crate it builds or tests began
// with an empty target/: a card that touched no Rust compiled rocksdb from scratch
// (#4465 run 2, 2026-10-11: build 16 min). Silas, 09:19: not one shared target/
// (parallel runs fight its lock) — clone canonical's target/ into the werk with
// `cp -c` (an APFS clone: the files share blocks until one is written). Measured
// 2026-10-11 10:05: athena-make 12,637 files in 2.8s, chorus-hooks 109,864 in 27s.
// Cargo still rebuilds whatever the card changed; what it keeps is everything
// else, the third-party crates first. Included by werk-build and werk-test. std only.

/// Should a crate's target/ be seeded? Only when the werk has none yet and
/// canonical has one. A werk's own target/ is never replaced: it holds this
/// card's builds.
pub fn should_seed_target(werk_target_exists: bool, canonical_target_exists: bool) -> bool {
    !werk_target_exists && canonical_target_exists
}

/// Seed `<werk>/<crate_rel>/target` from `<canonical>/<crate_rel>/target` with an
/// APFS clone. Returns what happened, for the run log: `seeded <rel> in 2.8s`,
/// `kept <rel>` (already there), `cold <rel>` (canonical has none), or
/// `seed-failed <rel>: …` (the build goes on cold; a failed seed never fails it).
pub fn seed_target(werk: &str, canonical: &str, crate_rel: &str) -> String {
    let dst = format!("{}/{}/target", werk, crate_rel);
    let src = format!("{}/{}/target", canonical, crate_rel);
    let (have, can) = (std::path::Path::new(&dst).exists(), std::path::Path::new(&src).is_dir());
    if !should_seed_target(have, can) {
        return format!("{} {}", if have { "kept" } else { "cold" }, crate_rel);
    }
    let started = std::time::Instant::now();
    let tmp = format!("{}.seeding", dst);
    let ok = std::process::Command::new("cp")
        .args(["-c", "-R", &src, &tmp])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    // clone to a side name, then rename: a half-cloned target/ never looks complete
    if ok && std::fs::rename(&tmp, &dst).is_ok() {
        format!("seeded {} in {:.1}s", crate_rel, started.elapsed().as_secs_f64())
    } else {
        let _ = std::fs::remove_dir_all(&tmp);
        format!("seed-failed {}: cp -c from {} did not complete", crate_rel, src)
    }
}
