// @domain: domains
//! #4490 — write a product's API crate from its slice of the model.
//!
//! Everything here is a pure function of the model: the same model gives the same
//! bytes (tested), so a regenerated crate shows a diff only when the model changed.

use crate::model::{Kind, Product, NS};
use std::collections::BTreeMap;

const MAIN: &str = include_str!("../templates/main.rs");
const STORE: &str = include_str!("../templates/store.rs");
const SETTINGS: &str = include_str!("../templates/settings.rs");
<<<<<<< HEAD
<<<<<<< HEAD
const AUTH: &str = include_str!("../templates/auth.rs");
const RULES: &str = include_str!("../templates/rules.rs");
<<<<<<< HEAD
const REQUIREMENTS: &str = include_str!("../templates/requirements_tests.rs");
const MODEL_TEST: &str = include_str!("../templates/model_test.rs");
=======
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
=======
const AUTH: &str = include_str!("../templates/auth.rs");
>>>>>>> 4c9755287 (wren: #4490 — pulse-api writes require a signed caller: ES256 token through the shared chorus-oidc verifier, principal and grants read from the model, typed refusals (no-token 401, token-invalid 401, identity-unreachable 503, not-a-principal 403); live check: wren's token resolves to principal wren)
=======
>>>>>>> 9ac820daf (wren: #4490 — pulse-api writes (create, replace, patch, delete) with the door rules generated in: shape checks (off-model, sh:in, one-value, required), links as IRIs with dangling targets refused, owner or Write Permission, prior version kept before every change, owl.write event per outcome; live: wren's RoleState created and patched, v1 reads it back, version-rolestates-wren-1 kept)

/// The crate's own version. Step 1 starts every product at 1.0.0; the bump rule
/// (#4478 Versions: removed field = major, added = minor) arrives with the diff check.
pub const CRATE_VERSION: &str = "1.0.0";

<<<<<<< HEAD
<<<<<<< HEAD
=======
>>>>>>> 4c9755287 (wren: #4490 — pulse-api writes require a signed caller: ES256 token through the shared chorus-oidc verifier, principal and grants read from the model, typed refusals (no-token 401, token-invalid 401, identity-unreachable 503, not-a-principal 403); live check: wren's token resolves to principal wren)
/// The shared token verifier, by path from `<step>/products/<product>/api/`. A crate
/// shipped away from this repo vendors it (the export step, a later card).
pub const OIDC_PATH: &str = "../../../../platform/services/chorus-oidc";

<<<<<<< HEAD
=======
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
=======
>>>>>>> 4c9755287 (wren: #4490 — pulse-api writes require a signed caller: ES256 token through the shared chorus-oidc verifier, principal and grants read from the model, typed refusals (no-token 401, token-invalid 401, identity-unreachable 503, not-a-principal 403); live check: wren's token resolves to principal wren)
/// StreamEvent → stream-event: the kind that prefixes a row's IRI (v1's mint rule).
pub fn kebab(class: &str) -> String {
    let mut out = String::new();
    for (i, ch) in class.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn lit(s: &str) -> String {
    format!("{s:?}")
}

<<<<<<< HEAD
pub fn model_rs(p: &Product, model_version: &str, settings: &[(String, String)]) -> String {
=======
/// The settings every product API reads, with their defaults. Step 1 declares them here;
/// AC6 moves them into the model as Property rows on the product (#3435).
pub fn settings(port: u16) -> Vec<(String, String)> {
    vec![
        ("events.command".into(), "".into()),
        ("identity.issuer".into(), "https://id.lightlifeurbangardens.com/".into()),
        ("server.bind".into(), format!("127.0.0.1:{port}")),
        ("store.endpoint".into(), "http://localhost:3030/pods".into()),
        ("store.kind".into(), "sparql".into()),
        ("store.password_var".into(), "FUSEKI_ADMIN_PASSWORD".into()),
        ("store.user_var".into(), "FUSEKI_ADMIN_USER".into()),
    ]
}

pub fn model_rs(p: &Product, model_version: &str, port: u16) -> String {
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
    let mut s = String::new();
    s.push_str(&format!(
        "//! {name}'s slice of the model. Generated by athena-gen — DO NOT EDIT; change the model.\n\n\
         pub const PRODUCT: &str = {pn};\npub const CRATE_VERSION: &str = env!(\"CARGO_PKG_VERSION\");\n\
         pub const API_VERSION: &str = \"v1\";\npub const MODEL_VERSION: &str = {mv};\npub const NS: &str = {ns};\n\n",
        name = p.name, pn = lit(&p.name), mv = lit(model_version), ns = lit(NS)
    ));
<<<<<<< HEAD
    s.push_str("/// Declared settings and their defaults, from the product's Property rows (see settings.rs for the lookup order).\npub const SETTINGS: &[(&str, &str)] = &[\n");
    for (k, v) in settings {
=======
    s.push_str("/// Declared settings and their defaults (see settings.rs for the lookup order).\npub const SETTINGS: &[(&str, &str)] = &[\n");
    for (k, v) in settings(port) {
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
        s.push_str(&format!("    ({}, {}),\n", lit(&k), lit(&v)));
    }
    s.push_str("];\n\n");
    s.push_str(
<<<<<<< HEAD
<<<<<<< HEAD
        "pub struct FieldDef {\n    pub name: &'static str,\n    pub link: bool,\n    pub datatype: &'static str,\n    pub required: bool,\n    pub many: bool,\n    pub allowed: &'static [&'static str],\n}\n\n\
         pub struct ClassDef {\n    pub domain: &'static str,\n    pub name: &'static str,\n    pub kind: &'static str,\n    pub plural: &'static str,\n    pub segment: &'static str,\n    pub graph: &'static str,\n    pub fields: &'static [FieldDef],\n}\n\n\
=======
        "pub struct FieldDef {\n    pub name: &'static str,\n    pub link: bool,\n    pub required: bool,\n    pub many: bool,\n    pub allowed: &'static [&'static str],\n}\n\n\
         pub struct ClassDef {\n    pub domain: &'static str,\n    pub name: &'static str,\n    pub kind: &'static str,\n    pub segment: &'static str,\n    pub graph: &'static str,\n    pub fields: &'static [FieldDef],\n}\n\n\
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
=======
        "pub struct FieldDef {\n    pub name: &'static str,\n    pub link: bool,\n    pub datatype: &'static str,\n    pub required: bool,\n    pub many: bool,\n    pub allowed: &'static [&'static str],\n}\n\n\
         pub struct ClassDef {\n    pub domain: &'static str,\n    pub name: &'static str,\n    pub kind: &'static str,\n    pub plural: &'static str,\n    pub segment: &'static str,\n    pub graph: &'static str,\n    pub fields: &'static [FieldDef],\n}\n\n\
>>>>>>> 9ac820daf (wren: #4490 — pulse-api writes (create, replace, patch, delete) with the door rules generated in: shape checks (off-model, sh:in, one-value, required), links as IRIs with dangling targets refused, owner or Write Permission, prior version kept before every change, owl.write event per outcome; live: wren's RoleState created and patched, v1 reads it back, version-rolestates-wren-1 kept)
         pub const CLASSES: &[ClassDef] = &[\n",
    );
    for d in &p.domains {
        for c in &d.classes {
            s.push_str(&format!(
<<<<<<< HEAD
<<<<<<< HEAD
                "    ClassDef {{\n        domain: {}, name: {}, kind: {}, plural: {}, segment: {}, graph: {},\n        fields: &[\n",
                lit(&d.name), lit(&c.name), lit(&kebab(&c.name)), lit(&crate::model::pluralize(&c.name)), lit(&c.segment), lit(&c.graph)
=======
                "    ClassDef {{\n        domain: {}, name: {}, kind: {}, segment: {}, graph: {},\n        fields: &[\n",
                lit(&d.name), lit(&c.name), lit(&kebab(&c.name)), lit(&c.segment), lit(&c.graph)
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
=======
                "    ClassDef {{\n        domain: {}, name: {}, kind: {}, plural: {}, segment: {}, graph: {},\n        fields: &[\n",
                lit(&d.name), lit(&c.name), lit(&kebab(&c.name)), lit(&crate::model::pluralize(&c.name)), lit(&c.segment), lit(&c.graph)
>>>>>>> 9ac820daf (wren: #4490 — pulse-api writes (create, replace, patch, delete) with the door rules generated in: shape checks (off-model, sh:in, one-value, required), links as IRIs with dangling targets refused, owner or Write Permission, prior version kept before every change, owl.write event per outcome; live: wren's RoleState created and patched, v1 reads it back, version-rolestates-wren-1 kept)
            ));
            for f in &c.fields {
                let allowed: Vec<String> = f.allowed.iter().map(|a| lit(a)).collect();
                s.push_str(&format!(
<<<<<<< HEAD
<<<<<<< HEAD
=======
>>>>>>> 9ac820daf (wren: #4490 — pulse-api writes (create, replace, patch, delete) with the door rules generated in: shape checks (off-model, sh:in, one-value, required), links as IRIs with dangling targets refused, owner or Write Permission, prior version kept before every change, owl.write event per outcome; live: wren's RoleState created and patched, v1 reads it back, version-rolestates-wren-1 kept)
                    "            FieldDef {{ name: {}, link: {}, datatype: {}, required: {}, many: {}, allowed: &[{}] }},\n",
                    lit(&f.name),
                    matches!(f.kind, Kind::Link { .. }),
                    lit(match &f.kind { Kind::Literal { datatype } => datatype.as_str(), Kind::Link { .. } => "" }),
                    f.required,
                    f.many,
                    allowed.join(", ")
<<<<<<< HEAD
=======
                    "            FieldDef {{ name: {}, link: {}, required: {}, many: {}, allowed: &[{}] }},\n",
                    lit(&f.name), matches!(f.kind, Kind::Link { .. }), f.required, f.many, allowed.join(", ")
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
=======
>>>>>>> 9ac820daf (wren: #4490 — pulse-api writes (create, replace, patch, delete) with the door rules generated in: shape checks (off-model, sh:in, one-value, required), links as IRIs with dangling targets refused, owner or Write Permission, prior version kept before every change, owl.write event per outcome; live: wren's RoleState created and patched, v1 reads it back, version-rolestates-wren-1 kept)
                ));
            }
            s.push_str("        ],\n    },\n");
        }
    }
    s.push_str("];\n");
    s
}

fn cargo_toml(p: &Product) -> String {
    format!(
        "# Generated by athena-gen (#4490) — DO NOT EDIT; change the model or the generator.\n\
         [package]\nname = \"{n}-api\"\nversion = \"{CRATE_VERSION}\"\nedition = \"2021\"\n\
         description = \"The {n} product's API, generated from the Chorus model.\"\n\n\
<<<<<<< HEAD
<<<<<<< HEAD
         [dependencies]\nchorus-oidc = {{ path = \"{oidc}\" }}\nserde_json = \"1\"\ntiny_http = \"0.12\"\nureq = {{ version = \"2\", features = [\"json\"] }}\n\n\
         [[bin]]\nname = \"{n}-api\"\npath = \"src/main.rs\"\n",
        n = p.name, oidc = OIDC_PATH
=======
         [dependencies]\nserde_json = \"1\"\ntiny_http = \"0.12\"\nureq = {{ version = \"2\", features = [\"json\"] }}\n\n\
         [[bin]]\nname = \"{n}-api\"\npath = \"src/main.rs\"\n",
        n = p.name
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
=======
         [dependencies]\nchorus-oidc = {{ path = \"{oidc}\" }}\nserde_json = \"1\"\ntiny_http = \"0.12\"\nureq = {{ version = \"2\", features = [\"json\"] }}\n\n\
         [[bin]]\nname = \"{n}-api\"\npath = \"src/main.rs\"\n",
        n = p.name, oidc = OIDC_PATH
>>>>>>> 4c9755287 (wren: #4490 — pulse-api writes require a signed caller: ES256 token through the shared chorus-oidc verifier, principal and grants read from the model, typed refusals (no-token 401, token-invalid 401, identity-unreachable 503, not-a-principal 403); live check: wren's token resolves to principal wren)
    )
}

/// A template is tagged with the generator's own domain; the copy in a product's crate
/// carries the product's first domain instead, so the crawler places it (#4419).
fn retag(template: &str, domain: &str) -> String {
    let body = template.strip_prefix("// @domain: domains\n").unwrap_or(template);
    format!("// @domain: {domain}\n{body}")
}

/// Every file of the product's crate, keyed by path inside the crate.
<<<<<<< HEAD
pub fn crate_files(p: &Product, model_version: &str, settings: &[(String, String)]) -> BTreeMap<String, String> {
    let domain = p.domains.first().map(|d| d.name.as_str()).unwrap_or("domains");
    let mut out = BTreeMap::new();
    out.insert("Cargo.toml".into(), cargo_toml(p));
    out.insert("src/auth.rs".into(), retag(AUTH, domain));
    out.insert("src/main.rs".into(), retag(MAIN, domain));
    out.insert("src/rules.rs".into(), retag(RULES, domain));
    out.insert("src/requirements_tests.rs".into(), retag(REQUIREMENTS, domain));
    out.insert("tests/model.rs".into(), retag(MODEL_TEST, domain));
    out.insert("src/store.rs".into(), retag(STORE, domain));
    out.insert("src/settings.rs".into(), retag(SETTINGS, domain));
    out.insert("src/model.rs".into(), format!("// @domain: {domain}\n{}", model_rs(p, model_version, settings)));
=======
pub fn crate_files(p: &Product, model_version: &str, port: u16) -> BTreeMap<String, String> {
    let domain = p.domains.first().map(|d| d.name.as_str()).unwrap_or("domains");
    let mut out = BTreeMap::new();
    out.insert("Cargo.toml".into(), cargo_toml(p));
    out.insert("src/auth.rs".into(), retag(AUTH, domain));
    out.insert("src/main.rs".into(), retag(MAIN, domain));
    out.insert("src/rules.rs".into(), retag(RULES, domain));
    out.insert("src/store.rs".into(), retag(STORE, domain));
    out.insert("src/settings.rs".into(), retag(SETTINGS, domain));
    out.insert("src/model.rs".into(), format!("// @domain: {domain}\n{}", model_rs(p, model_version, port)));
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
    out
}
