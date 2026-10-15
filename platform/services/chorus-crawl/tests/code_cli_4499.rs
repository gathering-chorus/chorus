// @domain: code — picked by its owner, Kade, from the crawler's candidates (#4538)
//! #4499 reopen — `chorus-crawl code` as the land runs it, against a door that
//! is not there (127.0.0.1:9 refuses at once), so nothing can be written
//! anywhere. A dry run still reads the repo's build files and says what it
//! would write; a real run with no door is UNMEASURED and writes nothing.
use std::process::Command;

// exp 2100-01-01: the crawl never tries to mint
const TOKEN: &str = "e30.eyJleHAiOjQxMDI0NDQ4MDB9.x";

fn repo_root() -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    format!("{dir}/../../..")
}

fn crawl(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_chorus-crawl"))
        .arg("code")
        .args(args)
        .env("CHORUS_ROOT", repo_root())
        .env("CHORUS_OWL_API", "http://127.0.0.1:9")
        .env("CHORUS_ROLE", "kade")
        .env("CHORUS_IDENTITY_TOKEN", TOKEN)
        .env("CHORUS_IDENTITY_MINT", "/nonexistent")
        .output()
        .unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn dry_run_reads_the_build_files_and_writes_nothing() {
    let (rc, out) = crawl(&["--dry-run"]);
    assert_eq!(rc, 0, "{out}");
    let units_line = out.lines().find(|l| l.starts_with("code: ") && l.contains(" units (")).unwrap_or_else(|| panic!("{out}"));
    assert!(!units_line.starts_with("code: 0 units"), "{units_line}");
    assert!(out.contains("code:   platform/services/werk-accept (crate) → "), "werk-accept's edges are listed:\n{out}");
    assert!(out.contains("the door does not serve CodeUnit yet"), "{out}");
    assert!(out.contains("code: dry-run — NOTHING WRITTEN"), "{out}");
}

#[test]
fn a_real_run_with_no_door_is_unmeasured_never_no_units() {
    let (rc, out) = crawl(&[]);
    assert_eq!(rc, 1, "{out}");
    assert!(out.contains("UNMEASURED"), "{out}");
    assert!(!out.contains("code: wrote"), "{out}");
}

#[test]
fn an_unknown_arg_runs_nothing() {
    let (rc, out) = crawl(&["--apply"]);
    assert_eq!(rc, 2, "{out}");
    assert!(out.contains("unknown arg --apply"), "{out}");
    assert!(!out.contains(" units ("), "{out}");
}
