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

/// Borg Queen 18:45 — a domain at two steps has no one home: refused, never the first.
#[test]
fn a_domain_at_two_steps_is_refused_by_name() {
    let ttl = format!("{FIXTURE}\n{TEMPLATE}\nc:principles c:atStep c:value-stream-step-designing .\n");
    let (store, _) = load(&[fixture("out-two-steps", &ttl)]).unwrap();
    let e = domain_out(&store, "principles").expect_err("refused").0;
    assert!(e.contains("principles declares 2 atStep values"), "{e}");
}

/// AC6's other clauses (Borg Queen 18:46): repoTarget, athena-make's live route mount,
/// is untouched, and no existing crate moved.
#[test]
fn repo_target_and_existing_crates_are_untouched() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let (store, _) = load(&athena_gen::model::model_files(&root)).unwrap();
    let q = "SELECT ?v WHERE { <https://jeffbridwell.com/chorus#principles> <https://jeffbridwell.com/chorus#repoTarget> ?v }";
    let r = oxigraph::sparql::SparqlEvaluator::new().parse_query(q).unwrap().on_store(&store).execute().unwrap();
    let vals: Vec<String> = match r {
        oxigraph::sparql::QueryResults::Solutions(s) => s.map(|x| x.unwrap().get("v").unwrap().to_string()).collect(),
        _ => panic!("not a select"),
    };
    assert_eq!(vals, vec!["\"loom/principles\"".to_string()]);
    assert!(root.join("directing/products/pulse/api/Cargo.toml").is_file(), "pulse's crate did not move");
}

/// NEGATIVE PROOF at the binary: a refused domain writes nothing.
#[test]
fn the_binary_writes_nothing_when_refused() {
    for (tag, cut) in [("no-step", "c:principles c:atStep c:value-stream-step-shaping ."), ("no-row", "\"athena-gen.domain_out\"")] {
        let root = std::env::temp_dir().join(format!("athena-gen-4995-refused-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("designing/data")).unwrap();
        let t = if tag == "no-row" { TEMPLATE.replace(cut, "\"something.else\"") } else { TEMPLATE.replace(cut, "") };
        std::fs::write(root.join("designing/data/m.ttl"), format!("{FIXTURE}\n{t}")).unwrap();
        std::fs::create_dir_all(root.join("platform/services/chorus-oidc")).unwrap();
        let st = std::process::Command::new(env!("CARGO_BIN_EXE_athena-gen"))
            .args(["generate", "--root"]).arg(&root).args(["--domain", "principles"]).output().unwrap();
        assert!(!st.status.success(), "{tag}: must refuse");
        assert!(!root.join("shaping").exists(), "{tag}: nothing may be written");
        let _ = std::fs::remove_dir_all(&root);
    }
}
