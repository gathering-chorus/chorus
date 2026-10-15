// @test-type: unit — signal:api is fixture-data
// @domain: athena
//! #4533 — athena-deploy finds the store's write credential on its own. Since
//! #4515 the land runs it through the runner, which carries no password, and
//! #4531's model deploy failed 2026-10-14 20:29 with a bare 401.
use athena_deploy::{fuseki_password, no_credential_refusal};

#[test]
fn the_credential_comes_from_the_environment_else_the_file_else_a_named_refusal() {
    let dir = std::env::temp_dir().join(format!("cred-4533-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("fuseki-write.env");
    std::env::set_var("FUSEKI_WRITE_ENV", &file);

    // the environment wins
    std::env::set_var("FUSEKI_ADMIN_PASSWORD", "from-env");
    std::fs::write(&file, "FUSEKI_ADMIN_PASSWORD=from-file\n").unwrap();
    assert_eq!(fuseki_password().as_deref(), Some("from-env"));

    // no environment: the file the bash writers read
    std::env::remove_var("FUSEKI_ADMIN_PASSWORD");
    assert_eq!(fuseki_password().as_deref(), Some("from-file"));

    // NEGATIVE PROOF: nowhere -> None, and the refusal names the file it looked for
    std::fs::remove_file(&file).unwrap();
    assert_eq!(fuseki_password(), None);
    let r = no_credential_refusal("staging-load");
    assert!(r.contains("staging-load-http-401-no-credential"), "{r}");
    assert!(r.contains(file.to_str().unwrap()), "{r}");
    assert!(!r.contains("from-file") && !r.contains("from-env"), "never prints a credential: {r}");

    std::env::remove_var("FUSEKI_WRITE_ENV");
    let _ = std::fs::remove_dir_all(&dir);
}
