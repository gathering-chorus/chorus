//! #4465 — the drift check on the committed werk v2 workflow:
//! platform/pipelines/cicd.yaml (dagu's dags_dir) must be exactly what
//! chorus-make generates from the cicd rows. A hand edit goes red here.
use chorus_make::{drift, generate, parse_rows};

const COMMITTED: &str = "platform/pipelines/cicd.yaml";

fn repo() -> std::path::PathBuf {
    // run time, not env!(): the werk-test 4030 guard refuses a compile-time path
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::path::Path::new(&dir).join("../../..")
}

fn expected() -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let rows = parse_rows(&std::fs::read_to_string(format!("{dir}/tests/fixtures/cicd-rows.tsv")).unwrap()).unwrap();
    generate("cicd", &rows).unwrap()
}

#[test]
fn the_committed_workflow_matches_the_rows() {
    let on_disk = std::fs::read_to_string(repo().join(COMMITTED))
        .unwrap_or_else(|e| panic!("{COMMITTED} must exist (a missing file fails, never passes): {e}"));
    assert_eq!(drift(&expected(), &on_disk), Ok(()));
}

// NEGATIVE PROOF: the same check on a hand-edited copy goes red.
#[test]
fn a_hand_edited_copy_is_drift() {
    let edited = std::fs::read_to_string(repo().join(COMMITTED)).unwrap().replace("werk-test", "werk-test --skip-all");
    assert!(drift(&expected(), &edited).is_err());
}
