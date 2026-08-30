//! #4030 — no test may bake `CARGO_MANIFEST_DIR` in at compile time.
//!
//! The nightly shares ONE cargo target dir (`~/.chorus/nightly-cargo-target`)
//! across every werk that runs `werk-test --nightly`. A test binary compiled
//! in werk A is reused by the run in werk B (same source hash → same
//! fingerprint), and `env!("CARGO_MANIFEST_DIR")` inside it still points at
//! werk A. The 2026-08-30 nightly read two reds from exactly this:
//! werk-teardown's zero-external-deps looked for
//! `chorus-werk/kade-4022/.../Cargo.toml` (torn down) and chorus-hooks'
//! stall-specimen test loaded an empty vocabulary from the same dead tree.
//!
//! `std::env::var("CARGO_MANIFEST_DIR")` is set by cargo/nextest at RUN time
//! and is correct for whichever tree is running. This guard scans every crate
//! under platform/services for the compile-time form. Negative proof below:
//! the scanner is shown to FIRE on a violating line (#3734).

use std::path::{Path, PathBuf};

/// Lines (1-based) that use the compile-time macro on `CARGO_MANIFEST_DIR`.
/// Comment lines are ignored so the rule can be documented next to itself.
fn compile_time_manifest_sites(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim_start();
            !t.starts_with("//") && t.contains("env!(\"CARGO_MANIFEST_DIR\")")
        })
        .map(|(i, _)| i + 1)
        .collect()
}

fn services_dir() -> PathBuf {
    Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .unwrap()
        .to_path_buf()
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() {
            if name == "target" || name == "node_modules" { continue; }
            rs_files(&p, out);
        } else if p.extension().map(|x| x == "rs").unwrap_or(false) {
            out.push(p);
        }
    }
}

#[test]
fn no_service_crate_bakes_manifest_dir_in_at_compile_time() {
    let mut files = Vec::new();
    rs_files(&services_dir(), &mut files);
    // A guard whose target vanished must fail loudly, never pass vacuously.
    assert!(files.len() > 50, "scanned only {} .rs files under {} — wrong root?", files.len(), services_dir().display());
    let mut hits = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap_or_default();
        for line in compile_time_manifest_sites(&text) {
            hits.push(format!("{}:{}", f.strip_prefix(services_dir()).unwrap_or(f).display(), line));
        }
    }
    assert!(hits.is_empty(),
        "compile-time CARGO_MANIFEST_DIR in {} place(s) — use std::env::var(\"CARGO_MANIFEST_DIR\") at run time (shared nightly target dir reuses binaries across werks):\n{}",
        hits.len(), hits.join("\n"));
}

#[test]
fn negative_proof_scanner_fires_on_the_compile_time_form() {
    let violating = "fn p() -> PathBuf {\n    PathBuf::from(env!(\"CARGO_MANIFEST_DIR\")).join(\"x\")\n}\n";
    assert_eq!(compile_time_manifest_sites(violating), vec![2], "the violation on line 2 must be caught");
}

#[test]
fn scanner_accepts_the_run_time_form_and_comments() {
    let clean = "// env!(\"CARGO_MANIFEST_DIR\") is the banned form\nlet d = std::env::var(\"CARGO_MANIFEST_DIR\").unwrap();\n";
    assert!(compile_time_manifest_sites(clean).is_empty());
}
