// @domain: domains
// @test-type: unit — model slice read from a TTL fixture; no store, no services
//! #4995 (AC1) — Jeff 11-12: "athena-gen is supposed to be athena-make (v2) for generating
//! domain apis". The domain is the generator's unit: `--domain principles` builds one API
//! serving only principles, from the domain's own rows and settings, with no product row.
//! Covers src/model (domain), src/emit (routes, crate_files_ops) and src/main (--domain).
use athena_gen::model::{domain, load, settings};

mod fixture_4995;
use fixture_4995::{fixture, FIXTURE};

#[test]
fn a_domain_is_built_alone_from_its_own_rows_with_no_product_row() {
    let (store, _) = load(&[fixture("unit", FIXTURE)]).unwrap();
    let p = domain(&store, "principles").expect("principles builds with no product row");
    assert_eq!(p.name, "principles");
    let names: Vec<_> = p.domains.iter().map(|d| (d.name.as_str(), d.composed)).collect();
    assert_eq!(names, vec![("principles", false)], "exactly the one domain, owned");
    let classes: Vec<_> = p.domains[0].classes.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(classes, vec!["Principle"]);
}

#[test]
fn the_domain_api_serves_only_principles_routes() {
    let (store, _) = load(&[fixture("routes", FIXTURE)]).unwrap();
    let p = domain(&store, "principles").unwrap();
    // the door's route minus /v1 (Borg Queen 18:19): one level, never /principles/principles/principles
    assert_eq!(athena_gen::emit::routes(&p), vec!["/principles/principles".to_string()]);
}

#[test]
fn the_settings_are_the_domains_own() {
    let (store, _) = load(&[fixture("settings", FIXTURE)]).unwrap();
    let s = settings(&store, "principles").unwrap();
    assert!(s.iter().any(|(k, v)| k == "server.bind" && v == "127.0.0.1:0"), "{s:?}");
    assert!(athena_gen::model::missing_settings(&s).is_empty(), "no borrowed settings needed");
}

/// The built binary takes `--domain` and writes the crate (a verb test at the binary
/// level, not only the library: #4895's lesson).
#[test]
fn the_binary_generates_a_crate_for_one_domain() {
    let root = std::env::temp_dir().join(format!("athena-gen-4995-bin-{}", std::process::id()));
    let model = root.join("designing/data");
    std::fs::create_dir_all(&model).unwrap();
    std::fs::write(model.join("m.ttl"), FIXTURE).unwrap();
    // the verifier checkout the generated auth links to
    let oidc = root.join("platform/services/chorus-oidc");
    std::fs::create_dir_all(&oidc).unwrap();
    let out = root.join("out");
    let st = std::process::Command::new(env!("CARGO_BIN_EXE_athena-gen"))
        .args(["generate", "--root"]).arg(&root).args(["--domain", "principles", "--out"]).arg(&out)
        .output().unwrap();
    assert!(st.status.success(), "stderr: {}", String::from_utf8_lossy(&st.stderr));
    let model_rs = std::fs::read_to_string(out.join("src/model.rs")).unwrap();
    assert!(model_rs.contains("pub const PRODUCT: &str = \"principles\""), "crate named for the domain");
    assert!(model_rs.contains("pub const DOMAIN_UNIT: bool = true"), "the crate serves /<domain>/<segment>");
    assert!(!model_rs.contains("\"values\""), "no other domain in the crate");
}

/// The real model, not a fixture (Borg Queen 18:18): the repo's principles domain row
/// carries its own settings and builds with no product row.
#[test]
fn the_repos_principles_domain_builds_from_its_own_rows() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let (store, skipped) = load(&athena_gen::model::model_files(&root)).unwrap();
    assert!(skipped.is_empty(), "model files that do not parse: {skipped:?}");
    let p = domain(&store, "principles").unwrap_or_else(|e| panic!("{}", e.0));
    assert_eq!(athena_gen::emit::routes(&p), vec!["/principles/principles".to_string()]);
    let s = settings(&store, "principles").unwrap();
    assert!(athena_gen::model::missing_settings(&s).is_empty(), "missing: {:?}", athena_gen::model::missing_settings(&s));
}

/// #4995 AC3 finding — a field outside the unit's namespace keeps its own IRI (28 empty
/// label/comment values were served when it read <ns>label). Borg Queen 18:32: a shape
/// can declare chorus:label AND rdfs:label; they are ONE field `label` with both IRIs,
/// the namespace's first (athena-make reads chorus: then rdfs:, writes both).
/// NEGATIVE PROOF: a chorus-only field keeps exactly its one chorus IRI.
#[test]
fn a_field_keeps_its_own_iris_and_twins_are_one_field() {
    let ttl = FIXTURE.replace(
        "sh:property [ sh:path c:statement ; sh:datatype xsd:string ; sh:minCount 1 ] .",
        "sh:property [ sh:path c:statement ; sh:datatype xsd:string ; sh:minCount 1 ] , \
         [ sh:path <http://www.w3.org/2000/01/rdf-schema#label> ; sh:datatype xsd:string ] , \
         [ sh:path c:label ; sh:datatype xsd:string ] , \
         [ sh:path <http://www.w3.org/2000/01/rdf-schema#comment> ; sh:datatype xsd:string ] .");
    let (store, _) = load(&[fixture("iri", &ttl)]).unwrap();
    let p = domain(&store, "principles").unwrap();
    let f = &p.domains[0].classes[0].fields;
    let of = |n: &str| f.iter().filter(|x| x.name == n).map(|x| x.iris.clone()).collect::<Vec<_>>();
    assert_eq!(of("label"), vec![vec!["https://jeffbridwell.com/chorus#label".to_string(), "http://www.w3.org/2000/01/rdf-schema#label".to_string()]],
        "one label field, chorus first");
    assert_eq!(of("comment"), vec![vec!["http://www.w3.org/2000/01/rdf-schema#comment".to_string()]]);
    assert_eq!(of("statement"), vec![vec!["https://jeffbridwell.com/chorus#statement".to_string()]]);
    let model_rs = athena_gen::emit::model_rs(&p, "test", &settings(&store, "principles").unwrap());
    assert!(model_rs.contains("iris: &[\"https://jeffbridwell.com/chorus#label\", \"http://www.w3.org/2000/01/rdf-schema#label\"]"), "the crate carries both IRIs");
}

/// Borg Queen 18:36 — a twin field's IRIs belong to its class: in one unit, Principle
/// declares label under chorus: and rdfs:, Tenet only under chorus:. NEGATIVE PROOF:
/// Tenet's label must not pick up rdfs:label, and the crate says so per class.
#[test]
fn twin_iris_stay_in_their_class() {
    let ttl = FIXTURE.replace("c:definesVocabulary c:Principle ;", "c:definesVocabulary c:Principle, c:Tenet ;").replace(
        "sh:property [ sh:path c:statement ; sh:datatype xsd:string ; sh:minCount 1 ] .",
        "sh:property [ sh:path c:label ; sh:datatype xsd:string ] , \
         [ sh:path <http://www.w3.org/2000/01/rdf-schema#label> ; sh:datatype xsd:string ] .\n\
         c:TenetShape a sh:NodeShape ; sh:targetClass c:Tenet ; c:instancesGraph \"urn:chorus:domains:principles\" ;\n\
           sh:property [ sh:path c:label ; sh:datatype xsd:string ] .");
    let (store, _) = load(&[fixture("scoped", &ttl)]).unwrap();
    let p = domain(&store, "principles").unwrap();
    let label = |class: &str| p.domains[0].classes.iter().find(|c| c.name == class).unwrap()
        .fields.iter().find(|f| f.name == "label").unwrap().iris.clone();
    assert_eq!(label("Principle").len(), 2, "{:?}", label("Principle"));
    assert_eq!(label("Tenet"), vec!["https://jeffbridwell.com/chorus#label".to_string()]);
}
