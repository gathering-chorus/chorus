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
    clone_target(&src, &dst, crate_rel)
}

/// APFS-clone `src` to `dst` via a side name, then rename: a half-cloned
/// target/ never looks complete.
fn clone_target(src: &str, dst: &str, crate_rel: &str) -> String {
    let started = std::time::Instant::now();
    let tmp = format!("{}.seeding", dst);
    let ok = std::process::Command::new("cp")
        .args(["-c", "-R", src, &tmp])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok && std::fs::rename(&tmp, dst).is_ok() {
        format!("seeded {} in {:.1}s", crate_rel, started.elapsed().as_secs_f64())
    } else {
        let _ = std::fs::remove_dir_all(&tmp);
        format!("seed-failed {}: cp -c from {} did not complete", crate_rel, src)
    }
}

// #4772 — one build folder per account. A role's werk is written by the role
// (its own cargo) and by the pipeline's account (dagu runs every step as
// jeffbridwell). Cargo cannot rewrite a file another account owns: a build
// script's std::fs::copy ends in fchmod, which only the owner may call, so
// libsqlite3-sys build.rs:39 panicked "Operation not permitted" (#4805,
// 2026-10-31 05:30: bq's werk target/ held 1529 chorus-borg-queen files and
// 351 jeffbridwell ones). Each account builds in <crate>/target/<account>.

/// The account this process runs as: `id -un`, else $USER.
pub fn current_account() -> String {
    std::process::Command::new("id")
        .arg("-un")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("USER").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "unknown".to_string())
}

/// `<crate_dir>/target/<account>`: the CARGO_TARGET_DIR for `account`'s builds.
pub fn account_target_dir(crate_dir: &str, account: &str) -> String {
    format!("{}/target/{}", crate_dir, account)
}

/// Seed `dst` (an account's build folder in a werk) from canonical's cache for
/// the same crate: the same account's folder there when it has one, else
/// canonical's target/. Same outcomes as `seed_target`.
pub fn seed_account_target(dst: &str, canonical_crate: &str, account: &str, crate_rel: &str) -> String {
    let own = account_target_dir(canonical_crate, account);
    let src = if std::path::Path::new(&own).is_dir() { own } else { format!("{}/target", canonical_crate) };
    let (have, can) = (std::path::Path::new(dst).exists(), std::path::Path::new(&src).is_dir());
    if !should_seed_target(have, can) {
        return format!("{} {}", if have { "kept" } else { "cold" }, crate_rel);
    }
    if let Some(parent) = std::path::Path::new(dst).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    clone_target(&src, dst, crate_rel)
}
