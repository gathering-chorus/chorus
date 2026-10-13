//! #4499 reopen 3 — Jeff, 2026-10-13: "can we fix that gap here?" A script in
//! no crate or package answered with the whole root package (452 tests). Each
//! test file now names the files it exercises, by werk-test's own rule.
use chorus_crawl::code_run::{exercise_edges, exercise_patches};

fn texts(p: &str) -> Option<String> {
    match p {
        "platform/tests/index-crawler.bats" => Some("run \"$ROOT/platform/scripts/index-crawler-snapshots.sh\"\n".into()),
        "platform/tests/other.bats" => Some("run cards list\n".into()),
        "platform/services/werk-push/tests/a.rs" => Some("use werk_push::x;\n".into()),
        _ => None,
    }
}
fn crate_of(p: &str) -> Option<String> {
    p.starts_with("platform/services/werk-push/").then(|| "platform/services/werk-push".to_string())
}
fn tests() -> Vec<String> {
    ["platform/tests/index-crawler.bats", "platform/tests/other.bats", "platform/services/werk-push/tests/a.rs"].iter().map(|s| s.to_string()).collect()
}
fn files() -> Vec<String> {
    ["platform/scripts/index-crawler-snapshots.sh", "platform/scripts/lonely.sh", "platform/services/werk-push/src/lib.rs"].iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_test_exercises_the_script_it_runs() {
    let e = exercise_edges(&tests(), &files(), &texts, &crate_of);
    let get = |t: &str| e.iter().find(|(x, _)| x == t).map(|(_, v)| v.clone()).unwrap();
    assert_eq!(get("platform/tests/index-crawler.bats"), vec!["platform/scripts/index-crawler-snapshots.sh"]);
}

#[test]
fn a_crate_test_exercises_its_crate_files() {
    let e = exercise_edges(&tests(), &files(), &texts, &crate_of);
    let get = |t: &str| e.iter().find(|(x, _)| x == t).map(|(_, v)| v.clone()).unwrap();
    assert_eq!(get("platform/services/werk-push/tests/a.rs"), vec!["platform/services/werk-push/src/lib.rs"]);
}

#[test]
fn negative_proof_a_script_no_test_names_is_exercised_by_nothing() {
    let e = exercise_edges(&tests(), &files(), &texts, &crate_of);
    assert!(e.iter().all(|(_, v)| !v.contains(&"platform/scripts/lonely.sh".to_string())));
    // and a test that names nothing has an empty entry, not a missing one
    assert_eq!(e.iter().find(|(t, _)| t == "platform/tests/other.bats").map(|(_, v)| v.len()), Some(0));
}

#[test]
fn only_rows_that_differ_are_patched_and_an_empty_set_clears() {
    let want = vec![
        ("t/a.bats".to_string(), vec!["s/x.sh".to_string()]),
        ("t/b.bats".to_string(), vec![]),
        ("t/c.bats".to_string(), vec!["s/y.sh".to_string()]),
    ];
    let stored = |t: &str| -> Vec<String> {
        match t { "t/a.bats" => vec!["n:s/x.sh".into()], "t/b.bats" => vec!["n:s/old.sh".into()], _ => vec![] }
    };
    let name = |p: &str| format!("n:{p}");
    let p = exercise_patches(&want, &stored, &name);
    assert_eq!(p, vec![
        ("n:t/b.bats".to_string(), "{\"exercises\":null}".to_string()),
        ("n:t/c.bats".to_string(), "{\"exercises\":[\"n:s/y.sh\"]}".to_string()),
    ]);
}
