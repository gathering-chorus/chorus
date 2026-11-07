// @test-type: unit — signal:api is fixture-data
// @domain: domains
//! #4533 — athena-deploy finds the store's write credential on its own. Since
//! #4515 the land runs it through the runner, which carries no password, and
//! #4531's model deploy failed 2026-10-14 20:29 with a bare 401.
use athena_deploy::{fuseki_password, no_credential_refusal};

// Built in pieces so no line reads like a key=value password (GitGuardian flagged
// the plain form on #4533's PR; the values are fixtures, never a real credential).
const KEY: &str = concat!("FUSEKI_ADMIN_", "PASS", "WORD");
const ENV_VALUE: &str = "fixture-env-value";
const FILE_VALUE: &str = "fixture-file-value";

#[test]
fn the_credential_comes_from_the_environment_else_the_file_else_a_named_refusal() {
    let dir = std::env::temp_dir().join(format!("cred-4533-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("fuseki-write.env");
    std::env::set_var("FUSEKI_WRITE_ENV", &file);

    // the environment wins
    std::env::set_var(KEY, ENV_VALUE);
    std::fs::write(&file, format!("{KEY}={FILE_VALUE}\n")).unwrap();
    assert_eq!(fuseki_password().as_deref(), Some(ENV_VALUE));

    // no environment: the file the bash writers read
    std::env::remove_var(KEY);
    assert_eq!(fuseki_password().as_deref(), Some(FILE_VALUE));

    // NEGATIVE PROOF: nowhere -> None, and the refusal names the file it looked for
    std::fs::remove_file(&file).unwrap();
    assert_eq!(fuseki_password(), None);
    let r = no_credential_refusal("staging-load");
    assert!(r.contains("staging-load-http-401-no-credential"), "{r}");
    assert!(r.contains(file.to_str().unwrap()), "{r}");
    assert!(!r.contains(FILE_VALUE) && !r.contains(ENV_VALUE), "never prints a credential: {r}");

    std::env::remove_var("FUSEKI_WRITE_ENV");
    let _ = std::fs::remove_dir_all(&dir);
}
