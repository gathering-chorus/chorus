// #4456 AC4 — the 2026-10-07 replay rows parse, one file per card, each with a
// runnable check or a stated reason. Fixtures by repo path:
//   platform/tests/fixtures/ac-1007/4446.tsv
//   platform/tests/fixtures/ac-1007/4420.tsv
//   platform/tests/fixtures/ac-1007/4419.tsv
//   platform/tests/fixtures/ac-1007/4440.tsv
//   platform/tests/fixtures/ac-1007/4444.tsv
//   platform/tests/fixtures/ac-1007/4445.tsv
//   platform/tests/fixtures/ac-1007/4447.tsv
//   platform/tests/fixtures/ac-1007/4432.tsv
//   platform/tests/fixtures/ac-1007/4452.tsv
//   platform/tests/fixtures/ac-1007/4358.tsv
//   platform/tests/fixtures/ac-1007/4438.tsv
//   platform/tests/fixtures/ac-1007/README
//   platform/tests/fixtures/ac-4456.tsv
// The crate these tests build:
//   platform/services/borg-prove/Cargo.toml
//   platform/services/borg-prove/Cargo.lock
use borg_prove::checks_from_tsv;

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../..")
}

#[test]
fn every_replay_card_has_rows_and_the_readme_names_it() {
    let dir = root().join("platform/tests/fixtures/ac-1007");
    let readme = std::fs::read_to_string(dir.join("README")).unwrap();
    for card in ["4446", "4420", "4419", "4440", "4444", "4445", "4447", "4432", "4452", "4358", "4438"] {
        let tsv = std::fs::read_to_string(dir.join(format!("{card}.tsv"))).unwrap_or_else(|_| panic!("{card}.tsv missing"));
        let rows = checks_from_tsv(&tsv).unwrap();
        assert!(!rows.is_empty(), "#{card} has no rows");
        assert!(rows.iter().all(|r| r.ac.starts_with(&format!("ac-{card}-"))), "#{card} rows are named ac-{card}-<n>");
        assert!(readme.contains(&format!("#{card}")), "#{card}'s verdict is not written down");
    }
    let own = checks_from_tsv(&std::fs::read_to_string(root().join("platform/tests/fixtures/ac-4456.tsv")).unwrap()).unwrap();
    assert_eq!(own.len(), 4);
}
