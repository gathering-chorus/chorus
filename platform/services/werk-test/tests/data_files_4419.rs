// @domain: tests — placed by chorus-crawl's domain rules (#4538)
//! #4419 reopen — a data file is exercised by the tests that read it. 8 card
//! runs on 8 cards (2026-10-09/10) were refused for fixtures, goldens and crate
//! config their own tests read; each refusal cost a whole build first.
use werk_test::{data_file_keys, exercisers_of, is_data_file, TestRow};

fn row(p: &str) -> TestRow {
    TestRow { file_path: p.into(), covers: String::new(), pyramid_layer: String::new(), hermeticity: String::new(), test_concern: String::new(), needs: String::new() }
}
fn crate_of(f: &str) -> Option<String> {
    f.strip_prefix("platform/services/").and_then(|r| r.split('/').next()).map(|c| format!("platform/services/{c}"))
}

#[test]
fn data_files_are_the_ones_tests_read() {
    assert!(is_data_file("platform/tests/fixtures/ac-1007/4446.tsv"));
    assert!(is_data_file("platform/services/chorus-hooks/.cargo/config.toml"));
    assert!(!is_data_file("platform/api/src/server.ts"));
    assert!(!is_data_file("platform/tests/4456-card-ac-model.test.sh"));
}

#[test]
fn keys_are_the_name_the_stem_and_a_specific_folder() {
    assert_eq!(data_file_keys("platform/tests/fixtures/ac-1007/4446.tsv"), vec!["4446.tsv", "4446", "ac-1007"]);
    assert_eq!(data_file_keys("platform/tests/fixtures/card-ac-4456.ttl"), vec!["card-ac-4456.ttl", "card-ac-4456"]);
    // generic names never count: every test says "config"
    assert!(data_file_keys("platform/services/chorus-hooks/.cargo/config.toml").is_empty());
}

#[test]
fn the_tests_that_name_a_fixture_or_its_folder_exercise_it() {
    let rows = vec![row("platform/services/borg-prove/tests/fixtures_4456.rs"), row("platform/tests/other.test.sh")];
    let text = |p: &str| Some(if p.contains("fixtures_4456") { "read(\"fixtures/ac-1007\")" } else { "nothing here" }.to_string());
    let ex = exercisers_of("platform/tests/fixtures/ac-1007/4446.tsv", &rows, &text, &crate_of, None);
    assert_eq!(ex, vec!["platform/services/borg-prove/tests/fixtures_4456.rs"]);
}

#[test]
fn a_crates_config_falls_to_the_crates_tests() {
    let rows = vec![row("platform/services/chorus-hooks/tests/free_disk_seam_4497.rs"), row("platform/api/tests/a.test.ts")];
    let text = |_: &str| Some("config everywhere".to_string());
    let ex = exercisers_of("platform/services/chorus-hooks/.cargo/config.toml", &rows, &text, &crate_of, None);
    assert_eq!(ex, vec!["platform/services/chorus-hooks/tests/free_disk_seam_4497.rs"]);
}

#[test]
fn negative_proof_a_data_file_no_test_reads_is_still_unexercised() {
    let rows = vec![row("platform/tests/other.test.sh")];
    let text = |_: &str| Some("nothing about it".to_string());
    assert!(exercisers_of("platform/tests/fixtures/orphan-9999.tsv", &rows, &text, &crate_of, None).is_empty());
}
