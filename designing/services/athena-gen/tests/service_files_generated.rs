// @domain: domains
// @test-type: unit — the repo's model through the generator's own functions; no service, no store
//! #5042 AC1 — a generated API's launcher comes from the model's rows: a fresh generate of
//! principles (and pulse, the other generated API, under the same convention) yields the launch
//! wrapper beside its crate exactly as it is committed. NEGATIVE PROOF: a hand edit is named.
//! (The LaunchAgent half is held on its own card: Jeff's A, 11-27 11:25.)
//! Covers shaping/domains/principles/api/principles-api-launch.sh and
//! directing/products/pulse/api/pulse-api-launch.sh.
use std::path::PathBuf;

fn repo() -> PathBuf {
    // run-time, not env!(): the werk-test 4030 guard
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("../../..")
}

/// (repo-relative path, text) of the launcher a fresh generate writes for a unit.
fn fresh(unit: &str, crate_dir: &str) -> Vec<(String, String)> {
    let root = repo();
    let (store, _) = athena_gen::model::load(&athena_gen::model::model_files(&root)).unwrap();
    let p = if crate_dir.starts_with("shaping/domains/") {
        athena_gen::model::domain(&store, unit)
    } else {
        athena_gen::model::product(&store, unit)
    }
    .unwrap_or_else(|e| panic!("{}", e.0));
    let settings = athena_gen::model::settings(&store, unit).unwrap();
    vec![(format!("{crate_dir}/{unit}-api-launch.sh"), athena_gen::surfaces::launcher_sh(&p, &settings))]
}

/// The paths whose committed text differs from `files`; each wanted text is left beside the
/// temp dir so the difference can be read (`diff <committed> <wanted>`).
fn drift(files: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (path, text) in files {
        if std::fs::read_to_string(repo().join(path)).ok().as_deref() != Some(text.as_str()) {
            let want = std::env::temp_dir().join(format!("athena-gen-5042-{}", path.replace('/', "_")));
            let _ = std::fs::write(&want, text);
            out.push(format!("{path} (the generator writes {})", want.display()));
        }
    }
    out
}

const UNITS: [(&str, &str); 2] = [("principles", "shaping/domains/principles/api"), ("pulse", "directing/products/pulse/api")];

#[test]
fn the_committed_launchers_are_what_the_generator_writes() {
    // every unit's drift is named, not only the first one's
    let d: Vec<String> = UNITS.iter().flat_map(|(unit, dir)| drift(&fresh(unit, dir))).collect();
    assert!(d.is_empty(), "differs from a fresh generate in {d:?}: run athena-gen generate");
}

#[test]
fn the_files_are_made_from_the_rows() {
    let f = fresh("principles", "shaping/domains/principles/api");
    let launch = &f[0].1;
    // the credential names are the rows' own; nothing here names them by hand
    assert!(launch.contains("export FUSEKI_ADMIN_PASSWORD="), "{launch}");
    assert!(launch.contains("$HOME/.chorus/bin/principles-api"), "{launch}");
    // pulse's store kind comes from its row, never from a line in its launcher (#5042, Wren 10:17)
    let p = fresh("pulse", "directing/products/pulse/api");
    assert!(!p[0].1.contains("STORE_KIND"), "the pulse launcher sets the store kind by hand: {}", p[0].1);
}

#[test]
fn negative_proof_a_hand_edit_to_the_launcher_is_named() {
    let f = fresh("principles", "shaping/domains/principles/api");
    for (i, file) in f.clone().iter().enumerate() {
        let mut edited = f.clone();
        edited[i].1 = format!("{}# a hand edit\n", file.1);
        let d = drift(&edited);
        assert_eq!(d.len(), 1, "a hand edit to {} was not named: {d:?}", file.0);
        assert!(d[0].starts_with(&file.0), "{d:?}");
    }
}
