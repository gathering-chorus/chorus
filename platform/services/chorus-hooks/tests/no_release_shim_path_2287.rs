//! #2287 — a test runs the shim cargo built for it, never
//! `<root>/platform/services/chorus-hooks/target/release/chorus-hook-shim`.
//! A werk never builds release, so that path is NotFound there (perf_suite on
//! 2026-10-03 03:50, then demo_gate_removal and session_start_defensive_regen at
//! 06:05) and is a stale binary everywhere else.

fn offenders(files: &[(String, String)]) -> Vec<String> {
    let needle = ["target", "release", "chorus-hook-shim"].join("/");
    let mut out = Vec::new();
    for (name, body) in files {
        for (i, line) in body.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if code.contains(&needle) { out.push(format!("{name}:{}", i + 1)); }
        }
    }
    out
}

#[test]
fn no_test_runs_the_release_shim() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests");
    let files: Vec<(String, String)> = std::fs::read_dir(dir).unwrap()
        .filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    // a guard whose input vanished must fail, never pass on nothing
    assert!(files.len() > 20, "read only {} test files", files.len());
    assert_eq!(offenders(&files), Vec::<String>::new());
}

#[test]
fn negative_proof_the_0350_shape_is_named() {
    // built at runtime: this file must not carry the literal it guards against (#3725)
    let path = ["{}/platform/services/chorus-hooks", "target", "release", "chorus-hook-shim"].join("/");
    let body = format!("fn shim_bin() -> String {{ format!(\"{path}\", root()) }}\n");
    assert_eq!(offenders(&[("perf_suite.rs".into(), body)]), vec!["perf_suite.rs:1"]);
    // a comment that names the path is not a use
    let comment = format!("// never {}\n", ["target", "release", "chorus-hook-shim"].join("/"));
    assert!(offenders(&[("x.rs".into(), comment)]).is_empty());
}
