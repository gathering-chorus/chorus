// @domain: domains
//! #4490 — the emitted crate is a pure function of the model.
use athena_gen::emit::{crate_files, kebab};
use athena_gen::model::{Class, Domain, Field, Kind, Product};

fn settings() -> Vec<(String, String)> {
    vec![("server.bind".into(), "127.0.0.1:3392".into()), ("store.kind".into(), "sparql".into())]
}

fn pulse() -> Product {
    Product {
        name: "pulse".into(),
        domains: vec![Domain {
            name: "awareness".into(),
            classes: vec![Class {
                name: "RoleState".into(),
                segment: "rolestates".into(),
                graph: "urn:chorus:domains:awareness".into(),
                fields: vec![Field {
                    name: "stateValue".into(),
                    kind: Kind::Literal { datatype: "string".into() },
                    required: true,
                    many: false,
                    allowed: vec!["idle".into(), "building".into()],
                }],
            }],
        }],
    }
}

#[test]
fn same_model_same_bytes() {
    assert_eq!(crate_files(&pulse(), "1.30.0", &settings()), crate_files(&pulse(), "1.30.0", &settings()));
}

#[test]
fn the_crate_carries_the_product_its_routes_and_its_versions() {
    let files = crate_files(&pulse(), "1.30.0", &settings());
    assert_eq!(files.keys().collect::<Vec<_>>(), ["Cargo.toml", "src/auth.rs", "src/embedded_tests.rs", "src/main.rs", "src/model.rs", "src/requirements_tests.rs", "src/rules.rs", "src/settings.rs", "src/store.rs", "tests/model.rs"]);
    assert!(files["Cargo.toml"].contains("name = \"pulse-api\""));
    let m = &files["src/model.rs"];
    for want in ["PRODUCT: &str = \"pulse\"", "MODEL_VERSION: &str = \"1.30.0\"", "segment: \"rolestates\"",
                 "kind: \"role-state\"", "allowed: &[\"idle\", \"building\"]", "\"server.bind\", \"127.0.0.1:3392\""] {
        assert!(m.contains(want), "model.rs lacks {want}:\n{m}");
    }
}

#[test]
fn negative_proof_a_model_change_changes_the_crate() {
    let mut changed = pulse();
    changed.domains[0].classes[0].fields[0].allowed.push("waiting".into());
    assert_ne!(crate_files(&pulse(), "1.30.0", &settings())["src/model.rs"], crate_files(&changed, "1.30.0", &settings())["src/model.rs"]);
}

#[test]
fn kinds_are_kebab() {
    assert_eq!(kebab("StreamEvent"), "stream-event");
    assert_eq!(kebab("RoleState"), "role-state");
    assert_eq!(kebab("Heartbeat"), "heartbeat");
}

#[test]
fn the_verifier_path_follows_where_the_crate_is_written() {
    let root = std::env::temp_dir().join(format!("athena-gen-oidc-{}", std::process::id()));
    let deep = root.join("directing/products/pulse/api");
    let shallow = root.join("x");
    std::fs::create_dir_all(&deep).unwrap();
    assert_eq!(athena_gen::emit::oidc_path(&root, &deep).unwrap(), "../../../../platform/services/chorus-oidc");
    assert_eq!(athena_gen::emit::oidc_path(&root, &shallow).unwrap(), "../platform/services/chorus-oidc");
    // NEGATIVE PROOF: a crate written outside the repo is refused, never given a broken path
    assert!(athena_gen::emit::oidc_path(&root, &std::env::temp_dir().join("elsewhere-4490")).is_err());
}

// #4519 — a product repo outside Chorus (the wine diary) names the verifier checkout
#[test]
fn a_crate_outside_the_repo_finds_the_verifier_it_is_given() {
    let base = std::env::temp_dir().join(format!("athena-gen-ver-{}", std::process::id()));
    let verifier = base.join("chorus/platform/services/chorus-oidc");
    std::fs::create_dir_all(&verifier).unwrap();
    std::fs::write(verifier.join("Cargo.toml"), "[package]\nname = \"chorus-oidc\"\n").unwrap();
    let out = base.join("wine-diary/directing/products/wine-diary/api");
    assert_eq!(athena_gen::emit::verifier_path(&verifier, &out).unwrap(), "../../../../../chorus/platform/services/chorus-oidc");
    // NEGATIVE PROOF: a folder that is not a crate is refused, never written as a dependency
    assert!(athena_gen::emit::verifier_path(&base.join("wine-diary"), &out).is_err());
}
