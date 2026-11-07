// @domain: identity — placed by chorus-crawl's domain rules (#4538)
// @card: #4716
// @owner: kade
//! #4716 AC3 — src/lib.rs `witness_role`. Every DAL write is identity-gated,
//! yet 24h of Loki (10-24 06:00) shows ~11.6k model writes and every one logs
//! as "unattributed": the witness read DEPLOY_ROLE, which the door strips
//! (#3687). The label now comes from the identity the DAL VERIFIED, and an
//! unverified caller's env string never labels anything.
use athena_model::{verify_identity_token, witness_role, Store, TokenVerifier, R};

struct IdStore { webid: &'static str }
impl Store for IdStore {
    fn ask(&self, sparql: &str) -> R<bool> {
        Ok(sparql.contains("principal-kade>"))
    }
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        if sparql.contains("chorus:webId") && sparql.contains(self.webid) {
            return Ok(vec!["kade".to_string()]);
        }
        Ok(vec![])
    }
    fn update(&self, _s: &str) -> R<()> { Ok(()) }
}

struct StubVerifier(Result<String, String>);
impl TokenVerifier for StubVerifier {
    fn verify_webid(&self, _t: &str, _n: u64) -> Result<String, String> { self.0.clone() }
}
const KADE: &str = "https://id.lightlifeurbangardens.com/kade/profile/card#me";

#[test]
fn a_verified_write_is_labelled_with_its_real_author() {
    let s = IdStore { webid: KADE };
    assert_eq!(witness_role(), "unattributed", "nothing is verified before the token is");
    verify_identity_token("tok", &StubVerifier(Ok(KADE.into())), &s, 0).unwrap();
    assert_eq!(witness_role(), "kade");
}

#[test]
fn negative_proof_an_unverified_caller_stays_unattributed_even_with_a_forged_env() {
    // The env string is what #3687 stopped trusting: it must not label either.
    // a fresh thread: VERIFIED_ROLE is per thread, so nothing another test
    // verified can leak in and make this pass or fail for the wrong reason
    std::thread::spawn(|| {
        std::env::set_var("DEPLOY_ROLE", "silas");
        let s = IdStore { webid: KADE };
        let e = verify_identity_token("forged", &StubVerifier(Err("bad signature".into())), &s, 0);
        assert!(e.is_err());
        assert_eq!(witness_role(), "unattributed");
    })
    .join()
    .expect("the negative proof ran");
}

/// src/main.rs — the TBox mint (`class`/`property`/`shape`) recorded "who" from
/// DEPLOY_ROLE in the vocabulary ledger. Now it asks for the verified identity
/// first, as retire-claim does, so a typed role cannot sign a vocabulary bump.
#[test]
fn negative_proof_a_tbox_mint_signed_only_by_deploy_role_is_refused_and_writes_nothing() {
    let dir = std::env::temp_dir().join(format!("attr-4716-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("probe.ttl");
    std::fs::write(&file, "# untouched\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_athena-model"))
        .args(["class", "--name", "AttrProbe", "--file", file.to_str().unwrap()])
        .env("DEPLOY_ROLE", "kade")
        .env_remove("CHORUS_IDENTITY_TOKEN")
        .env("CHORUS_FUSEKI", "http://127.0.0.1:1")
        .output()
        .expect("binary runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "a DEPLOY_ROLE-only mint must refuse: {}", err);
    assert!(err.contains("identity-token-required"), "refusal names the cause: {}", err);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# untouched\n", "nothing appended");
}
