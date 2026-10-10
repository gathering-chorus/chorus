// @domain: domains
//! #4490 — the emitted crate is a pure function of the model.
use athena_gen::emit::{crate_files, kebab};
use athena_gen::model::{Class, Domain, Field, Kind, Product};

<<<<<<< HEAD
fn settings() -> Vec<(String, String)> {
    vec![("server.bind".into(), "127.0.0.1:3392".into()), ("store.kind".into(), "sparql".into())]
}

=======
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
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
<<<<<<< HEAD
    assert_eq!(crate_files(&pulse(), "1.30.0", &settings()), crate_files(&pulse(), "1.30.0", &settings()));
=======
    assert_eq!(crate_files(&pulse(), "1.30.0", 3392), crate_files(&pulse(), "1.30.0", 3392));
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
}

#[test]
fn the_crate_carries_the_product_its_routes_and_its_versions() {
<<<<<<< HEAD
    let files = crate_files(&pulse(), "1.30.0", &settings());
    assert_eq!(files.keys().collect::<Vec<_>>(), ["Cargo.toml", "src/auth.rs", "src/main.rs", "src/model.rs", "src/requirements_tests.rs", "src/rules.rs", "src/settings.rs", "src/store.rs", "tests/model.rs"]);
=======
    let files = crate_files(&pulse(), "1.30.0", 3392);
<<<<<<< HEAD
<<<<<<< HEAD
    assert_eq!(files.keys().collect::<Vec<_>>(), ["Cargo.toml", "src/main.rs", "src/model.rs", "src/settings.rs", "src/store.rs"]);
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
=======
    assert_eq!(files.keys().collect::<Vec<_>>(), ["Cargo.toml", "src/auth.rs", "src/main.rs", "src/model.rs", "src/settings.rs", "src/store.rs"]);
>>>>>>> 4c9755287 (wren: #4490 — pulse-api writes require a signed caller: ES256 token through the shared chorus-oidc verifier, principal and grants read from the model, typed refusals (no-token 401, token-invalid 401, identity-unreachable 503, not-a-principal 403); live check: wren's token resolves to principal wren)
=======
    assert_eq!(files.keys().collect::<Vec<_>>(), ["Cargo.toml", "src/auth.rs", "src/main.rs", "src/model.rs", "src/rules.rs", "src/settings.rs", "src/store.rs"]);
>>>>>>> 9ac820daf (wren: #4490 — pulse-api writes (create, replace, patch, delete) with the door rules generated in: shape checks (off-model, sh:in, one-value, required), links as IRIs with dangling targets refused, owner or Write Permission, prior version kept before every change, owl.write event per outcome; live: wren's RoleState created and patched, v1 reads it back, version-rolestates-wren-1 kept)
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
<<<<<<< HEAD
    assert_ne!(crate_files(&pulse(), "1.30.0", &settings())["src/model.rs"], crate_files(&changed, "1.30.0", &settings())["src/model.rs"]);
=======
    assert_ne!(crate_files(&pulse(), "1.30.0", 3392)["src/model.rs"], crate_files(&changed, "1.30.0", 3392)["src/model.rs"]);
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
}

#[test]
fn kinds_are_kebab() {
    assert_eq!(kebab("StreamEvent"), "stream-event");
    assert_eq!(kebab("RoleState"), "role-state");
    assert_eq!(kebab("Heartbeat"), "heartbeat");
}
