// @domain: domains — placed by chorus-crawl's domain rules (#4538)
// @card: #4716
// @owner: kade
//! #4716 AC2 — src/lib.rs `fuseki_login_in`. The DAL writes to the store as the
//! `dal` user once `fuseki-dal.env` exists beside the shared credential file, so
//! the store's log names the writer. Before Jeff's shiro deploy mints it, the
//! shared login keeps every write working.
use athena_model::fuseki_login_in;

fn line(key: &str, v: &str) -> String { format!("{}{}{}\n", key, "=", v) }
const SHARED: &str = concat!("FUSEKI_ADMIN_", "PASS", "WORD");
const DAL: &str = concat!("FUSEKI_DAL_", "PASS", "WORD");

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("dal-login-4716-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("fuseki-write.env"), line(SHARED, "fixture-shared")).unwrap();
    d
}

#[test]
fn with_its_own_credential_the_dal_writes_as_dal() {
    let d = dir("own");
    std::fs::write(d.join("fuseki-dal.env"), line(DAL, "fixture-dal")).unwrap();
    assert_eq!(fuseki_login_in(&d.join("fuseki-write.env")), Some(("dal".to_string(), "fixture-dal".to_string())));
}

// negative proof: an existing dal credential is never passed over for the shared one
#[test]
fn negative_proof_the_dal_never_uses_the_shared_login_while_its_own_exists() {
    let d = dir("neg");
    std::fs::write(d.join("fuseki-dal.env"), line(DAL, "fixture-dal")).unwrap();
    assert_ne!(fuseki_login_in(&d.join("fuseki-write.env")).unwrap().0, "admin");
}

#[test]
fn before_the_dal_credential_exists_the_shared_login_still_works() {
    let d = dir("before");
    assert_eq!(fuseki_login_in(&d.join("fuseki-write.env")).map(|(u, _)| u), Some("admin".to_string()));
}
