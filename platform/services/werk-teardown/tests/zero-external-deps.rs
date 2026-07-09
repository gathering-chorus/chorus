//! #3431 guardrail (Silas, 2026-07-08): the werk-teardown path-dep must never
//! become an external-dep backdoor. Every [dependencies] entry in this crate
//! and in the sibling crates that take it as a path dep (werk-accept,
//! werk-unpull, and since #3632 werk-commit) must be a workspace
//! `path = ...` dep — zero registry/network deps, preserving the #3045
//! blueprint's protections (deterministic offline build, standalone signing).

use std::fs;
use std::path::PathBuf;

fn services_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// Parse a Cargo.toml's [dependencies] section; return entries that are NOT
/// pure path deps (i.e. anything that could reach a registry).
fn non_path_deps(toml: &str) -> Vec<String> {
    let mut bad = Vec::new();
    let mut in_deps = false;
    for raw in toml.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_deps = line == "[dependencies]";
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Acceptable shape: name = { path = "..." } (optionally with features).
        // Anything with a bare version string or a `version`/`git` key can
        // reach outside the workspace.
        let is_path_only = line.contains("path")
            && !line.contains("version")
            && !line.contains("git =");
        if !is_path_only {
            bad.push(line.to_string());
        }
    }
    bad
}

#[test]
fn teardown_and_its_dependents_have_zero_external_deps() {
    let dir = services_dir();
    for crate_name in ["werk-teardown", "werk-accept", "werk-unpull", "werk-commit"] {
        let manifest = dir.join(crate_name).join("Cargo.toml");
        let toml = fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("read {}: {}", manifest.display(), e));
        let bad = non_path_deps(&toml);
        assert!(
            bad.is_empty(),
            "{} declares non-path dependencies (registry/external deps are forbidden by the #3045 blueprint): {:?}",
            crate_name, bad
        );
    }
}

#[test]
fn parser_flags_registry_deps() {
    assert!(non_path_deps("[dependencies]\nserde = \"1.0\"\n").len() == 1);
    assert!(non_path_deps("[dependencies]\nx = { version = \"1\", path = \"../x\" }\n").len() == 1);
    assert!(non_path_deps("[dependencies]\nx = { git = \"https://x\" }\n").len() == 1);
    assert!(non_path_deps("[dependencies]\nwerk-teardown = { path = \"../werk-teardown\" }\n").is_empty());
    assert!(non_path_deps("[dependencies]\n\n[lib]\nserde = \"1.0\"\n").is_empty(), "only [dependencies] is scanned");
}
