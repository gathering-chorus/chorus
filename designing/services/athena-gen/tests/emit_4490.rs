// @domain: domains
//! #4490 — the emitted crate is a pure function of the model.
use athena_gen::emit::{crate_files, kebab};
use athena_gen::model::{Class, Domain, Field, Kind, Product};

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
    assert_eq!(crate_files(&pulse(), "1.30.0", 3392), crate_files(&pulse(), "1.30.0", 3392));
}

#[test]
fn the_crate_carries_the_product_its_routes_and_its_versions() {
    let files = crate_files(&pulse(), "1.30.0", 3392);
    assert_eq!(files.keys().collect::<Vec<_>>(), ["Cargo.toml", "src/auth.rs", "src/main.rs", "src/model.rs", "src/rules.rs", "src/settings.rs", "src/store.rs"]);
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
    assert_ne!(crate_files(&pulse(), "1.30.0", 3392)["src/model.rs"], crate_files(&changed, "1.30.0", 3392)["src/model.rs"]);
}

#[test]
fn kinds_are_kebab() {
    assert_eq!(kebab("StreamEvent"), "stream-event");
    assert_eq!(kebab("RoleState"), "role-state");
    assert_eq!(kebab("Heartbeat"), "heartbeat");
}
