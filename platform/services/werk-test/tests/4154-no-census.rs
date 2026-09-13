//! #4154 — the runner reads the graph's registry and runs what is there. It no
//! longer counts what it did not run: the census ("reconcile|tests-domain" row),
//! the never-ran fold and the `--reconcile` verb were a second crawler living
//! inside the test runner (33 of 37 red rows on 2026-09-12 06:00 were that row
//! naming files a land had deleted). This guard fails if any of it returns.
fn src(name: &str) -> String {
    // #4030 — no compile-time manifest dir; cargo test runs with cwd = the crate
    let p = std::env::current_dir().expect("cwd").join("src").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn the_runner_has_no_census_reconcile_or_never_ran_path() {
    let files = ["lib.rs", "main.rs", "nightly_all.rs", "nightly_run.rs"];
    let mut hits = Vec::new();
    for f in files {
        let text = src(f);
        assert!(text.len() > 1000, "{f} read empty — wrong root?"); // the guard cannot pass vacuously
        for needle in ["fn census(", "fn census_row(", "fn never_ran_units(", "fn reconcile_gap(", "fn classify_gap(", "\"--reconcile\"", "SuiteRow::new(\"reconcile\""] {
            for (i, line) in text.lines().enumerate() {
                if line.contains(needle) && !line.trim_start().starts_with("//") {
                    hits.push(format!("{f}:{}: {}", i + 1, line.trim()));
                }
            }
        }
    }
    assert!(hits.is_empty(), "census/reconcile machinery is back in the runner (#4154):\n{}", hits.join("\n"));
}
