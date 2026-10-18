// @domain: tests — Kade (#4579 reopen)
//! #4579 reopen — Jeff 10-18 19:10: the graph stops carrying per-run test
//! results. werk-test writes no TestSuiteRun (nightly suite rows, live rows, a
//! werk run's write-back); /nightly reads the run log. TestResult went the same
//! way on 10-17. This guard reads werk-test's own source for a write to either
//! collection, so an old writer coming back fails here before it reaches a store.

use std::path::Path;

/// The graph collections werk-test must never write to again.
const RETIRED: [&str; 4] = ["testsuiteruns", "suiteruns", "testresults", "tests/results"];

/// Every retired collection a source text names in a write, as `file: name`.
fn writes_to_retired(name: &str, text: &str) -> Vec<String> {
    RETIRED
        .iter()
        .filter(|c| ["\"", "/"].iter().any(|pre| ["\"", "/"].iter().any(|post| text.contains(&format!("{pre}{c}{post}")))))
        .map(|c| format!("{name}: {c}"))
        .collect()
}

fn src_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).expect("werk-test src is readable").flatten() {
        let p = e.path();
        if p.is_dir() {
            src_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn werk_test_writes_no_suite_or_result_rows_to_the_graph() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    src_files(&src, &mut files);
    // a guard whose target is gone must fail loudly, never pass on nothing
    assert!(files.len() > 5, "found only {} source files under {}", files.len(), src.display());
    let hits: Vec<String> = files
        .iter()
        .flat_map(|f| writes_to_retired(&f.display().to_string(), &std::fs::read_to_string(f).unwrap_or_default()))
        .collect();
    assert!(hits.is_empty(), "werk-test writes a retired graph collection again: {hits:?}");
}

#[test]
fn negative_proof_the_old_writers_are_caught() {
    // the three shapes the removed code had (all_scope.rs record_row / unit_mark, main.rs post_suite_run)
    assert_eq!(writes_to_retired("x.rs", r#"self.post_graph("testsuiteruns", &role, &body, &tok)"#), vec!["x.rs: testsuiteruns"]);
    assert_eq!(writes_to_retired("x.rs", r#"self.send_graph("DELETE", &format!("testsuiteruns/{}", name), "#), vec!["x.rs: testsuiteruns"]);
    assert_eq!(writes_to_retired("x.rs", r#""http://localhost:3360/testsuiteruns".to_string()"#), vec!["x.rs: testsuiteruns"]);
    // and a source that writes neither is clean
    assert!(writes_to_retired("x.rs", r#"self.append_log(&row.line());"#).is_empty());
}
