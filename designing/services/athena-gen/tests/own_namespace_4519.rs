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
@prefix wd: <https://wine.example/diary#> .
# #4519 reopen (Silas + Wren, 15:09): modelling terms are Chorus's (c:), product terms are the product's (wd:)
c:wine a c:Product ; c:hasDomain c:cellar ;
  c:hasProperty c:w1, c:w2, c:w3 .
c:w1 c:propertyKey "model.namespace" ; c:propertyValue "https://wine.example/diary#" .
c:w2 c:propertyKey "model.prefix" ; c:propertyValue "wine" .
c:w3 c:propertyKey "store.graph_prefix" ; c:propertyValue "urn:wine:" .
c:cellar c:definesVocabulary wd:Bottle .
wd:BottleShape a sh:NodeShape ; sh:targetClass wd:Bottle ;
  sh:property [ sh:path wd:vintage ; sh:datatype xsd:integer ] ;
  sh:property [ sh:path wd:wine ; sh:class wd:Wine ] .
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
    let bottle = &p.domains[0].classes[0];
    assert_eq!(bottle.name, "Bottle");
    let names: Vec<_> = bottle.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["vintage", "wine"], "fields found by their own-namespace paths");
    let files = crate_files(&p, &model_version(&store).unwrap(), &settings(&store, "wine").unwrap());
    let model = &files["src/model.rs"];
    assert!(model.contains("pub const NS: &str = \"https://wine.example/diary#\";"), "{model}");
    assert!(model.contains("pub const PREFIX: &str = \"wine\";"), "{model}");
    assert!(model.contains("pub const GRAPH_PREFIX: &str = \"urn:wine:\";"), "{model}");
    // #4519 reopen 2 — the verifier is told whose people to read, at boot
    assert!(files["src/main.rs"].contains("IdentityModel::new(model::NS, &s.identity_graph_prefix)"), "boot never configures the verifier");
    assert!(files["src/settings.rs"].contains("value(\"identity.graph_prefix\""), "identity.graph_prefix is not read");
    let left = chorus_names(&files);
    assert!(left.is_empty(), "the wine diary's own code still names Chorus:\n{}", left.join("\n"));
}

#[test]
fn negative_proof_the_check_catches_chorus_names_in_a_chorus_api() {
    // the same check over an API whose settings are Chorus's must find them, or it proves nothing
    let chorus = WINE.replace("<https://wine.example/diary#>", "<https://jeffbridwell.com/chorus#>").replace("\"https://wine.example/diary#\"", "\"https://jeffbridwell.com/chorus#\"").replace("urn:wine:", "urn:chorus:domains:");
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
    assert!(e.contains("model.namespace"), "{e}");
    // with a namespace but no graph prefix, the graph prefix is what is named
    let half = WINE.replace("c:hasProperty c:w1, c:w2, c:w3 .", "c:hasProperty c:w1 .");
    let (store2, _) = load(&[fixture("half", &half)]).unwrap();
    let e2 = product(&store2, "wine").unwrap_err().to_string();
    assert!(e2.contains("store.graph_prefix"), "{e2}");
    let missing = missing_settings(&settings(&store, "wine").unwrap());
    for k in ["identity.graph_prefix", "model.namespace", "model.prefix", "store.graph_prefix"] {
        assert!(missing.contains(&k), "{k} missing is not named: {missing:?}");
    }
}

#[test]
fn negative_proof_a_class_outside_the_products_namespace_is_refused_by_name() {
    let stray = WINE.replace("c:cellar c:definesVocabulary wd:Bottle .", "c:cellar c:definesVocabulary wd:Bottle, c:RoleState .");
    let (store, _) = load(&[fixture("stray", &stray)]).unwrap();
    let e = product(&store, "wine").unwrap_err().to_string();
    assert!(e.contains("RoleState") && e.contains("not in wine's namespace"), "{e}");
}
