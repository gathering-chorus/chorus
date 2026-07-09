//! #3633 — invariant guard: acquiring the singleton must NEVER truncate the
//! pidfile. The flock singleton opens the pidfile create+read+write; if anyone
//! ever adds `.truncate(true)` an incoming instance would zero the file out from
//! under the live holder (destroying the recorded PID mid-lifetime). Rust's
//! OpenOptions defaults truncate=false, but #3633 makes it EXPLICIT (both to
//! satisfy clippy::suspicious_open_options and to pin the semantic) — this test
//! locks that behavior so a future `.truncate(true)` regresses loudly instead of
//! silently.

use chorus_hooks::shared::singleton::acquire_singleton;

fn tmp_pidfile(tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("chorus-trunc-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("chorus-hooks.pid").to_str().unwrap().to_string()
}

#[test]
fn acquiring_the_singleton_does_not_truncate_existing_pidfile_content() {
    let pidfile = tmp_pidfile("preserve");
    // Pre-seed the file with a recorded PID (as a live holder would have written).
    std::fs::write(&pidfile, "424242\n").unwrap();

    // Acquire — this opens the file create+read+write. It must NOT zero it.
    let lock = acquire_singleton(&pidfile).expect("acquire on an existing pidfile");

    let after = std::fs::read_to_string(&pidfile).unwrap();
    assert_eq!(
        after, "424242\n",
        "acquiring the singleton must not truncate the pidfile — content was destroyed"
    );
    drop(lock);
}
