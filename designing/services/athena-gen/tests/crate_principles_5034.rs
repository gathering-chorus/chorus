// @domain: domains
// @test-type: unit — the repo's model through the generator's own functions; no service, no store
//! #5034 — the principles twin of crates_4490: every file athena-gen writes for
//! `generate --domain principles` (the crate under src/ and the surfaces/) is committed
//! exactly as a fresh generate writes it. A hand edit to the generated crate, made in step
//! with a template, is held by this test, not by care.
//! Names each by path so a change to any of them selects this test:
//! shaping/domains/principles/api/, designing/services/athena-gen/templates/.
//! NEGATIVE PROOF: a copy with one hand-edited file and one missing file fails naming both.
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    // run-time, not env!(): the werk-test 4030 guard
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("../../..")
}

fn crate_dir() -> PathBuf {
    repo().join("shaping/domains/principles/api")
}

/// What `athena-gen generate --domain principles` writes, by the same calls its main makes,
/// resolved from where the committed crate sits (handler and verifier paths are relative to it).
fn fresh() -> Vec<(String, String)> {
    let root = repo();
    let (store, _) = athena_gen::model::load(&athena_gen::model::model_files(&root)).unwrap();
    let p = athena_gen::model::domain(&store, "principles").unwrap_or_else(|e| panic!("{}", e.0));
    let mv = athena_gen::model::model_version(&store).unwrap();
    let settings = athena_gen::model::settings(&store, "principles").unwrap();
    let out = crate_dir();
    let handlers: Vec<(String, String)> = p.operations.iter()
        .map(|o| (o.name.replace('-', "_"), athena_gen::emit::handler_path(&root, &out, o).unwrap()))
        .collect();
    let oidc = athena_gen::emit::oidc_path(&root, &out).unwrap();
    let mut files: Vec<(String, String)> = athena_gen::emit::crate_files_ops(&p, &mv, &settings, &oidc, &handlers).into_iter().collect();
    files.extend(athena_gen::surfaces::files(&p, &settings));
    files
}

/// The paths under `dir` that are missing or differ from the fresh generate.
fn drift(dir: &Path, files: &[(String, String)]) -> Vec<String> {
    files.iter()
        .filter(|(path, text)| std::fs::read_to_string(dir.join(path)).ok().as_deref() != Some(text.as_str()))
        .map(|(path, _)| path.clone())
        .collect()
}

#[test]
fn the_committed_principles_api_is_what_the_generator_writes_today() {
    let files = fresh();
    // the files #5034 hand-edits must be among those compared, or this passes vacuously
    for want in ["src/main.rs", "src/requirements_tests.rs", "surfaces/Principle.openapi.json"] {
        assert!(files.iter().any(|(p, _)| p == want), "the generator no longer writes {want}; this test would not hold it");
    }
    let d = drift(&crate_dir(), &files);
    assert!(d.is_empty(), "shaping/domains/principles/api differs from a fresh generate in {d:?}: run athena-gen generate --domain principles");
}

#[test]
fn negative_proof_a_hand_edit_and_a_missing_file_are_named() {
    let files = fresh();
    let dir = std::env::temp_dir().join(format!("athena-gen-5034-twin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in &files {
        let f = dir.join(path);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, text).unwrap();
    }
    assert!(drift(&dir, &files).is_empty(), "an exact copy must show no drift");
    std::fs::write(dir.join("src/main.rs"), format!("{}\n// hand edit\n", files.iter().find(|(p, _)| p == "src/main.rs").unwrap().1)).unwrap();
    std::fs::remove_file(dir.join("surfaces/Principle.html")).unwrap();
    let d = drift(&dir, &files);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(d.len(), 2, "{d:?}");
    assert!(d.contains(&"src/main.rs".to_string()) && d.contains(&"surfaces/Principle.html".to_string()), "{d:?}");
}
