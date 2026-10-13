// @domain: domains
// @test-type: unit
//! #4519 — a product's generated API is its own: its namespace, its prefix and its
//! graphs come from its Property rows, never Chorus's. Jeff 2026-10-13 (via Silas, on
//! the wine diary #4518): "fix the gaps, don't work around them."
use athena_gen::emit::crate_files;
use athena_gen::model::{load, missing_settings, model_version, product, settings};
use std::path::PathBuf;

const WINE: &str = r#"
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
c:wine a c:Product ; c:hasDomain c:cellar ;
  c:hasProperty c:w1, c:w2, c:w3 .
c:w1 c:propertyKey "model.namespace" ; c:propertyValue "https://wine.example/diary#" .
c:w2 c:propertyKey "model.prefix" ; c:propertyValue "wine" .
c:w3 c:propertyKey "store.graph_prefix" ; c:propertyValue "urn:wine:" .
c:cellar c:definesVocabulary c:Bottle .
c:BottleShape a sh:NodeShape ; sh:targetClass c:Bottle ;
  sh:property [ sh:path c:vintage ; sh:datatype xsd:integer ] .
"#;

fn fixture(name: &str, ttl: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("athena-gen-4519-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("m.ttl");
    std::fs::write(&f, ttl).unwrap();
    f
}

/// Every Chorus name an API's own code would carry: its namespace and its graphs.
fn chorus_names(files: &std::collections::BTreeMap<String, String>) -> Vec<String> {
    files.iter()
        .filter(|(path, _)| path.starts_with("src/"))
        .flat_map(|(path, text)| text.lines().enumerate()
            .filter(|(_, l)| l.contains("urn:chorus") || l.contains("jeffbridwell.com/chorus") || l.contains("\"chorus:"))
            .map(move |(i, l)| format!("{path}:{}: {}", i + 1, l.trim())))
        .collect()
}

#[test]
fn a_wine_diary_api_names_its_own_namespace_and_graphs() {
    let (store, _) = load(&[fixture("wine", WINE)]).unwrap();
    let p = product(&store, "wine").unwrap();
    assert_eq!(p.domains[0].classes[0].graph, "urn:wine:cellar", "a class with no instancesGraph lives under the product's prefix");
    let files = crate_files(&p, &model_version(&store).unwrap(), &settings(&store, "wine").unwrap());
    let model = &files["src/model.rs"];
    assert!(model.contains("pub const NS: &str = \"https://wine.example/diary#\";"), "{model}");
    assert!(model.contains("pub const PREFIX: &str = \"wine\";"), "{model}");
    assert!(model.contains("pub const GRAPH_PREFIX: &str = \"urn:wine:\";"), "{model}");
    let left = chorus_names(&files);
    assert!(left.is_empty(), "the wine diary's own code still names Chorus:\n{}", left.join("\n"));
}

#[test]
fn negative_proof_the_check_catches_chorus_names_in_a_chorus_api() {
    // the same check over an API whose settings are Chorus's must find them, or it proves nothing
    let chorus = WINE.replace("https://wine.example/diary#", "https://jeffbridwell.com/chorus#").replace("urn:wine:", "urn:chorus:domains:");
    let (store, _) = load(&[fixture("chorus", &chorus)]).unwrap();
    let p = product(&store, "wine").unwrap();
    let files = crate_files(&p, &model_version(&store).unwrap(), &settings(&store, "wine").unwrap());
    assert!(!chorus_names(&files).is_empty(), "the check could not see Chorus names");
}

#[test]
fn negative_proof_no_graph_prefix_is_refused_never_a_chorus_default() {
    let bare = WINE.replace("c:hasProperty c:w1, c:w2, c:w3 .", ".");
    let (store, _) = load(&[fixture("bare", &bare)]).unwrap();
    let e = product(&store, "wine").unwrap_err().to_string();
    assert!(e.contains("store.graph_prefix"), "{e}");
    let missing = missing_settings(&settings(&store, "wine").unwrap());
    for k in ["model.namespace", "model.prefix", "store.graph_prefix"] {
        assert!(missing.contains(&k), "{k} missing is not named: {missing:?}");
    }
}
