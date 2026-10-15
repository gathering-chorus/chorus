// @test-type: unit
// @domain: tests
//! #4536 — each Test row holds its latest result, updated in place. Only the
//! cases whose result flipped are written (351 of 9,705 on 10-15, measured by
//! Silas), and no TestResult row is minted.

use werk_test::{latest_patches, parse_prior_results, CaseResult};

fn case(f: &str, n: &str, r: &str) -> CaseResult {
    CaseResult { file_path: f.into(), test_name: n.into(), result: r.into() }
}

// filePath covers layer name entity hermeticity concern result
const TSV: &str = "a.rs\tchorus\tunit\tt1\ttest-1\thermetic\t\tpass\n\
                   a.rs\tchorus\tunit\tt2\ttest-2\thermetic\t\tfail\n\
                   b.rs\tchorus\tunit\tt3\ttest-3\thermetic\t\t\n";

#[test]
fn prior_results_line_up_with_the_registry_rows() {
    let (rows, _, entities) = werk_test::parse_rows_and_names(TSV);
    let priors = parse_prior_results(TSV);
    assert_eq!(priors.len(), rows.len());
    assert_eq!(entities, vec!["test-1", "test-2", "test-3"]);
    assert_eq!(priors, vec!["pass", "fail", ""]);
}

#[test]
fn only_cases_whose_result_flipped_are_written() {
    let (_, _, entities) = werk_test::parse_rows_and_names(TSV);
    let priors = parse_prior_results(TSV);
    let joined = vec![
        (case("a.rs", "t1", "pass"), "test-1".to_string(), "t1".to_string()), // unchanged
        (case("a.rs", "t2", "pass"), "test-2".to_string(), "t2".to_string()), // fail → pass
        (case("b.rs", "t3", "fail"), "test-3".to_string(), "t3".to_string()), // never ran → fail
    ];
    let p = latest_patches(&joined, &entities, &priors, "4536", 1_760_000_000_000, &|c| {
        if c.result == "fail" { "assert failed".into() } else { String::new() }
    });
    let names: Vec<&str> = p.iter().map(|(e, _)| e.as_str()).collect();
    assert_eq!(names, vec!["test-2", "test-3"]);
    assert!(p[0].1.contains("\"result\":\"pass\""));
    assert!(p[0].1.contains("\"cardId\":\"4536\""));
    assert!(p[0].1.contains("\"runTs\":"));
    // fail→pass removes the reason (merge-patch null), never sets it to "" (Silas, 07:50)
    assert!(p[0].1.contains("\"failureReason\":null"), "a pass removes the old reason: {}", p[0].1);
    assert!(!p[0].1.contains("\"failureReason\":\"\""), "never an empty reason: {}", p[0].1);
    assert!(p[1].1.contains("\"failureReason\":\"assert failed\""));
}

#[test]
fn a_nightly_patch_carries_no_card() {
    let joined = vec![(case("b.rs", "t3", "pass"), "test-3".to_string(), "t3".to_string())];
    let p = latest_patches(&joined, &["test-3".to_string()], &["".to_string()], "", 1, &|_| String::new());
    assert_eq!(p.len(), 1);
    assert!(!p[0].1.contains("cardId"), "typed absence: {}", p[0].1);
}

#[test]
fn a_patch_never_mints_a_testresult_row() {
    let joined = vec![(case("b.rs", "t3", "fail"), "test-3".to_string(), "t3".to_string())];
    let p = latest_patches(&joined, &["test-3".to_string()], &["pass".to_string()], "1", 1, &|_| String::new());
    assert!(!p[0].1.contains("ofTest"), "a patch updates the Test, it is not a TestResult: {}", p[0].1);
    assert!(!p[0].1.contains("\"name\""), "the name is the Test's own, in the path: {}", p[0].1);
}
