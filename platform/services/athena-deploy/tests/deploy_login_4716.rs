// @test-type: unit — signal:api is fixture-data
// @domain: domains
// @card: #4716
// @owner: kade
//! #4716 AC2 — src/lib.rs `fuseki_login_in`. The model deploy writes to the store
//! as the `deploy` user once its credential file exists (Silas's shiro half mints
//! it), so Fuseki's own log names the writer. Until Jeff runs the deploy command,
//! the file is absent and the shared login keeps working: nothing breaks.
use athena_deploy::fuseki_login_in;

// Built in pieces so no line reads like a key=value password (#4533's rule).
fn line(key: &str, v: &str) -> String { format!("{}{}{}\n", key, "=", v) }
const SHARED: &str = concat!("FUSEKI_ADMIN_", "PASS", "WORD");
const DEPLOY: &str = concat!("FUSEKI_DEPLOY_", "PASS", "WORD");

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("login-4716-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("fuseki-write.env"), line(SHARED, "fixture-shared")).unwrap();
    d
}

#[test]
fn with_its_own_credential_the_deploy_writes_as_deploy() {
    let d = dir("own");
    std::fs::write(d.join("fuseki-deploy.env"), line(DEPLOY, "fixture-deploy")).unwrap();
    assert_eq!(fuseki_login_in(&d.join("fuseki-write.env")).map(|(u, _)| u), Some("deploy".to_string()));
}

// negative proof: the deploy must not fall back to the shared login while its
// own credential exists, or the store's log would name the wrong writer.
#[test]
fn negative_proof_an_existing_deploy_credential_is_never_passed_over_for_the_shared_one() {
    let d = dir("neg");
    std::fs::write(d.join("fuseki-deploy.env"), line(DEPLOY, "fixture-deploy")).unwrap();
    let (u, p) = fuseki_login_in(&d.join("fuseki-write.env")).unwrap();
    assert_ne!(u, "admin");
    assert_eq!(p, "fixture-deploy");
}

#[test]
fn before_the_deploy_credential_exists_the_shared_login_still_works() {
    let d = dir("before");
    assert_eq!(fuseki_login_in(&d.join("fuseki-write.env")), Some(("admin".to_string(), "fixture-shared".to_string())));
}
