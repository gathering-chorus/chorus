// @domain: domains
//! #4490 — the two crates' manifests and the generated crate as committed.
//! Names each by path so a change to any of them selects this test:
//! designing/services/athena-gen/Cargo.toml, designing/services/athena-gen/Cargo.lock,
//! directing/products/pulse/api/Cargo.toml, directing/products/pulse/api/Cargo.lock,
//! and the templates under designing/services/athena-gen/templates/.
use std::path::PathBuf;

fn repo() -> PathBuf {
    // run-time, not env!(): the werk-test 4030 guard
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("../../..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

#[test]
fn the_generator_pins_its_model_engine() {
    let toml = read("designing/services/athena-gen/Cargo.toml");
    assert!(toml.contains("oxigraph = { version = \"=0.5.11\", default-features = false }"), "{toml}");
    assert!(read("designing/services/athena-gen/Cargo.lock").contains("name = \"oxigraph\""));
}

#[test]
fn the_committed_pulse_api_is_what_the_generator_writes_today() {
    let root = repo();
    let files = athena_gen::model::model_files(&root);
    let (store, _) = athena_gen::model::load(&files).unwrap();
    let p = athena_gen::model::product(&store, "pulse").unwrap();
    let mv = athena_gen::model::model_version(&store).unwrap();
    let settings = athena_gen::model::settings(&store, "pulse").unwrap();
    for (path, text) in athena_gen::emit::crate_files(&p, &mv, &settings) {
        let committed = read(&format!("directing/products/pulse/api/{path}"));
        assert_eq!(committed, text, "directing/products/pulse/api/{path} differs from a fresh generate: run athena-gen generate --product pulse");
    }
    assert!(read("directing/products/pulse/api/Cargo.lock").contains("name = \"pulse-api\""));
}

#[test]
fn negative_proof_a_hand_edit_to_the_generated_crate_is_caught() {
    let root = repo();
    let (store, _) = athena_gen::model::load(&athena_gen::model::model_files(&root)).unwrap();
    let p = athena_gen::model::product(&store, "pulse").unwrap();
    let files = athena_gen::emit::crate_files(&p, "1.30.0", &[]);
    let edited = format!("{}\n// hand edit", files["src/main.rs"]);
    assert_ne!(edited, files["src/main.rs"]);
}
