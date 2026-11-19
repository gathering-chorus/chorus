// @domain: domains
// @test-type: unit — model slice read from TTL fixtures; one binary run into a temp tree
//! #4995 (AC6, Jeff 18:40 via Wren 18:41) — where a domain's generated API lands is a
//! setting, not code: ONE athena-gen Property row holds `{step}/domains/{domain}/api`
//! (ADR-041), the step is the domain's atStep. Jeff changes one row, not 38.
//! NEGATIVE PROOF: a domain with no atStep, or a model with no template row, is refused
//! by name; nothing is written.
//! Covers src/model (domain_out) and src/main (--domain with no --out).
use athena_gen::model::{domain_out, load};

mod fixture_4995;
use fixture_4995::{fixture, FIXTURE};

const TEMPLATE: &str = r#"
@prefix c: <https://jeffbridwell.com/chorus#> .
c:principles c:atStep c:value-stream-step-shaping .
c:service-athena c:hasProperty c:property-athena-gen-domain-out .
c:property-athena-gen-domain-out c:propertyKey "athena-gen.domain_out" ; c:propertyValue "{step}/domains/{domain}/api" .
"#;

#[test]
fn the_domain_lands_where_the_template_row_says() {
    let (store, _) = load(&[fixture("out-ok", &format!("{FIXTURE}\n{TEMPLATE}"))]).unwrap();
    assert_eq!(domain_out(&store, "principles").unwrap(), "shaping/domains/principles/api");
}

#[test]
fn changing_the_one_row_moves_every_domain() {
    let ttl = format!("{FIXTURE}\n{}", TEMPLATE.replace("{step}/domains/{domain}/api", "{step}/generated/{domain}"));
    let (store, _) = load(&[fixture("out-moved", &ttl)]).unwrap();
    assert_eq!(domain_out(&store, "principles").unwrap(), "shaping/generated/principles");
}

#[test]
fn a_domain_with_no_atstep_is_refused_by_name() {
    let ttl = format!("{FIXTURE}\n{}", TEMPLATE.replace("c:principles c:atStep c:value-stream-step-shaping .", ""));
    let (store, _) = load(&[fixture("out-no-step", &ttl)]).unwrap();
    let e = domain_out(&store, "principles").expect_err("refused").0;
    assert!(e.contains("principles declares no atStep"), "{e}");
}

#[test]
fn no_template_row_is_refused_by_name() {
    let ttl = format!("{FIXTURE}\n{}", TEMPLATE.replace("\"athena-gen.domain_out\"", "\"something.else\""));
    let (store, _) = load(&[fixture("out-no-row", &ttl)]).unwrap();
    let e = domain_out(&store, "principles").expect_err("refused").0;
    assert!(e.contains("athena-gen.domain_out"), "{e}");
}

/// The binary: `--domain` with no `--out` writes where the row says, under --root.
#[test]
fn the_binary_writes_to_the_resolved_path() {
    let root = std::env::temp_dir().join(format!("athena-gen-4995-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("designing/data")).unwrap();
    std::fs::write(root.join("designing/data/m.ttl"), format!("{FIXTURE}\n{TEMPLATE}")).unwrap();
    std::fs::create_dir_all(root.join("platform/services/chorus-oidc")).unwrap();
    let st = std::process::Command::new(env!("CARGO_BIN_EXE_athena-gen"))
        .args(["generate", "--root"]).arg(&root).args(["--domain", "principles"]).output().unwrap();
    assert!(st.status.success(), "stderr: {}", String::from_utf8_lossy(&st.stderr));
    assert!(root.join("shaping/domains/principles/api/src/model.rs").is_file());
    let _ = std::fs::remove_dir_all(&root);
}

/// The repo's own model carries the row and principles' atStep.
#[test]
fn the_repo_resolves_principles_to_its_adr_041_home() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let (store, _) = load(&athena_gen::model::model_files(&root)).unwrap();
    assert_eq!(domain_out(&store, "principles").unwrap_or_else(|e| panic!("{}", e.0)), "shaping/domains/principles/api");
}
