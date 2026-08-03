//! #3651 — the DAL self-stamp bypass is closed. Red-first (DEC-1674).
//!
//! The audit's top confirmed gap: chorus-model defaulted DEPLOY_ROLE to a role
//! identity ("silas" in witness, "system" in the creator stamp), so any process
//! shelling the binary bypassed the owl-api door AND self-attributed. These tests
//! pin the fix: a mutating verb REQUIRES a verified `Identity`; the only
//! constructor is `verify_identity`, which fail-closes on absent / malformed /
//! unregistered claims against the Principal registry (urn:chorus:domains:security).
//! AC-facing behavior only: what a caller sees (refusal vs write), what the store
//! receives (creator stamp), never internals.

use athena_model::{verify_identity, verify_identity_token, write, Store, TokenVerifier, WriteReq, R};
use std::cell::RefCell;

/// Hermetic stub (the constraint_enforcement.rs pattern): Principal-registry
/// ASKs route on the security graph; everything else answers permissively so
/// identity is the only variable under test.
struct IdStore {
    principals: Vec<String>, // registered claims, e.g. "kade"
    webids: Vec<(String, String)>, // #3356 — verified WebID → claim (chorus:webId lookup)
    asks: RefCell<usize>,    // every ASK that reached the store
    updates: RefCell<Vec<String>>,
}
impl IdStore {
    fn with(principals: &[&str]) -> Self {
        Self {
            principals: principals.iter().map(|s| s.to_string()).collect(),
            webids: Vec::new(),
            asks: RefCell::new(0),
            updates: RefCell::new(Vec::new()),
        }
    }
}
impl Store for IdStore {
    fn ask(&self, sparql: &str) -> R<bool> {
        *self.asks.borrow_mut() += 1;
        if sparql.contains("urn:chorus:domains:security") {
            return Ok(self.principals.iter().any(|p| sparql.contains(&format!("principal-{}>", p))));
        }
        Ok(true) // referential-integrity asks: permissive — not under test here
    }
    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        // #3356 — the verified-WebID → Principal lookup (chorus:webId). Everything
        // else: empty (no shape constraints, no existing created stamp).
        if sparql.contains("chorus:webId") {
            for (w, claim) in &self.webids {
                if sparql.contains(w.as_str()) {
                    return Ok(vec![claim.clone()]);
                }
            }
            return Ok(vec![]);
        }
        Ok(vec![])
    }
    fn update(&self, s: &str) -> R<()> {
        self.updates.borrow_mut().push(s.to_string());
        Ok(())
    }
}

// ── AC 1 — absent identity refuses, nothing written, no default minted ──────

#[test]
fn absent_claim_is_refused_fail_closed() {
    let s = IdStore::with(&["kade"]);
    let e = verify_identity(None, &s).unwrap_err();
    assert!(e.starts_with("identity-missing"), "{}", e);
    assert_eq!(*s.asks.borrow(), 0, "refusal happens before any store contact");
}

#[test]
fn empty_or_blank_claim_is_refused_like_absent() {
    let s = IdStore::with(&["kade"]);
    assert!(verify_identity(Some(""), &s).unwrap_err().starts_with("identity-missing"));
    assert!(verify_identity(Some("  "), &s).unwrap_err().starts_with("identity-missing"));
}

// ── AC 2 — unregistered identity refuses ────────────────────────────────────

#[test]
fn unregistered_claim_is_refused() {
    let s = IdStore::with(&["kade", "wren"]);
    let e = verify_identity(Some("intruder"), &s).unwrap_err();
    assert!(e.starts_with("identity-unknown"), "{}", e);
}

#[test]
fn malformed_claim_is_refused_before_any_query() {
    // injection-shaped claims never reach SPARQL — refused on syntax alone.
    let s = IdStore::with(&["kade"]);
    for bad in ["kade> } } ; DROP", "Kade", "a b", &"x".repeat(64)] {
        let e = verify_identity(Some(bad), &s).unwrap_err();
        assert!(e.starts_with("identity-malformed"), "claim {:?} → {}", bad, e);
    }
    assert_eq!(*s.asks.borrow(), 0, "malformed claims must not touch the store");
}

// ── AC 3 — a registered identity writes, stamped with its REAL name ─────────

#[test]
fn registered_claim_verifies_and_stamps_creator() {
    let s = IdStore::with(&["kade"]);
    let id = verify_identity(Some("kade"), &s).expect("registered principal verifies");
    assert_eq!(id.role(), "kade");

    let req = WriteReq { kind: "domain".into(), name: "x".into(), ..Default::default() };
    write(&s, &req, &id).expect("verified identity writes");
    let ups = s.updates.borrow();
    assert_eq!(ups.len(), 1);
    assert!(
        ups[0].contains("creator> \"kade\""),
        "creator stamped with the verified identity, not a default: {}",
        ups[0]
    );
    assert!(!ups[0].contains("\"system\""), "the 'system' default is gone");
}

// ── AC 4 — the bypass path itself: shelling the binary cannot self-attribute ─
// #3718 — the binary is `athena-model` now; `chorus-model` is a fail-loud stub
// that exits 2. These three tests caught the rename by failing, which is the
// stub doing its job: a silent alias would have let them keep passing against
// the old name forever.
// Spawn the REAL athena-model binary. DEPLOY_ROLE is removed from its env and
// the store points at a dead port: the refusal must be identity-missing, i.e.
// it fires BEFORE any store/network contact — no default identity ever forms.

#[test]
fn cli_write_without_deploy_role_is_refused_before_store_contact() {
    // #3651 pinned "no default identity ever forms, refusal precedes store". #3687
    // flipped the gate to token-first, so an env-less caller now refuses with
    // identity-token-required (not identity-missing) — same guarantee, new cause.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_athena-model"))
        .args(["add", "--kind", "domain", "--name", "bypass-probe"])
        .env_remove("DEPLOY_ROLE")
        .env_remove("CHORUS_IDENTITY_TOKEN")
        .env("CHORUS_FUSEKI", "http://127.0.0.1:1") // dead — must never be reached
        .output()
        .expect("binary runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "bypass write must refuse");
    assert!(err.contains("identity-token-required"), "refusal names the cause: {}", err);
    assert!(!err.to_lowercase().contains("fuseki"), "refusal precedes store contact: {}", err);
}

#[test]
fn cli_write_with_unregistered_identity_is_refused() {
    // With a claim present the registry must answer; a dead store = fail closed,
    // never fail open. Either way: no write, non-zero exit.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_athena-model"))
        .args(["add", "--kind", "domain", "--name", "bypass-probe"])
        .env("DEPLOY_ROLE", "intruder")
        .env("CHORUS_FUSEKI", "http://127.0.0.1:1")
        .output()
        .expect("binary runs");
    assert!(!out.status.success(), "unverifiable identity must refuse");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("written:"));
}

// ── #3687 — the flip: DEPLOY_ROLE env-trust is retired. A write with a
// DEPLOY_ROLE set but NO verified token must refuse, fail-closed, BEFORE any
// store contact — the env string was as forgeable as `export DEPLOY_ROLE=`.
// This is the behavior that was allowed (the fallback path) until step 3.

#[test]
fn cli_write_with_deploy_role_but_no_token_is_refused() {
    // Pre-#3687 this SUCCEEDED (resolve fell back to verify_identity(DEPLOY_ROLE)).
    // Post-flip: no CHORUS_IDENTITY_TOKEN → refuse, and the refusal fires before
    // the registry ASK (dead store must never be reached).
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_athena-model"))
        .args(["add", "--kind", "domain", "--name", "flip-probe"])
        .env("DEPLOY_ROLE", "kade") // a REGISTERED role — still refused without a token
        .env_remove("CHORUS_IDENTITY_TOKEN")
        .env("CHORUS_FUSEKI", "http://127.0.0.1:1") // dead — must never be reached
        .output()
        .expect("binary runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "a DEPLOY_ROLE-only write must refuse post-flip: {}", err);
    assert!(
        err.contains("identity-token-required"),
        "refusal names the retired-env cause: {}",
        err
    );
    assert!(!err.to_lowercase().contains("fuseki"), "refusal precedes store contact: {}", err);
}

#[test]
fn cli_dry_run_needs_no_identity() {
    // --dry-run writes nothing — it stays usable without an identity (sketch surface).
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_athena-model"))
        .args(["add", "--kind", "domain", "--name", "sketch", "--dry-run"])
        .env_remove("DEPLOY_ROLE")
        .output()
        .expect("binary runs");
    assert!(out.status.success(), "dry-run is identity-free: {}", String::from_utf8_lossy(&out.stderr));
}

// ── #3356 step 1 — the identity TOKEN path (verified WebID → Principal) ──────
// Additive to the DEPLOY_ROLE path; forging a writer now needs the credential.

/// Stub verifier — the concrete ES256/JWKS verify is the shared oidc crate; here
/// we pin the DAL's BINDING logic (verified WebID → Principal), not the crypto.
struct StubVerifier(Result<String, String>);
impl TokenVerifier for StubVerifier {
    fn verify_webid(&self, _token: &str, _now: u64) -> Result<String, String> {
        self.0.clone()
    }
}
const A_WEBID: &str = "https://id.lightlifeurbangardens.com/kade/profile/card#me";

#[test]
fn token_valid_and_webid_registered_yields_identity_from_graph() {
    let mut s = IdStore::with(&["kade"]);
    s.webids = vec![(A_WEBID.to_string(), "kade".to_string())];
    let v = StubVerifier(Ok(A_WEBID.to_string()));
    let id = verify_identity_token("a-real-token", &v, &s, 0).unwrap();
    assert_eq!(id.role(), "kade", "identity is bound from the verified WebID, not a string");
}

#[test]
fn forged_token_refuses_fail_closed() {
    let s = IdStore::with(&["kade"]);
    let v = StubVerifier(Err("bad signature".to_string()));
    let e = verify_identity_token("forged", &v, &s, 0).unwrap_err();
    assert!(e.starts_with("identity-token-invalid"), "{}", e);
}

#[test]
fn verified_webid_owning_no_principal_refuses() {
    // token verifies, but its WebID maps to no chorus:Principal → fail-closed.
    let s = IdStore::with(&["kade"]); // webids empty → no binding
    let v = StubVerifier(Ok("https://id.lightlifeurbangardens.com/stranger/profile/card#me".to_string()));
    let e = verify_identity_token("tok", &v, &s, 0).unwrap_err();
    assert!(e.starts_with("identity-webid-unregistered"), "{}", e);
}

#[test]
fn deploy_role_path_unchanged_by_token_addition() {
    // Non-breaking regression: the env path behaves exactly as before.
    let s = IdStore::with(&["wren"]);
    assert_eq!(verify_identity(Some("wren"), &s).unwrap().role(), "wren");
    assert!(verify_identity(Some("noone"), &s).unwrap_err().starts_with("identity-unknown"));
}

// ── #3356 AC4 — per-graph authz (DAL side): a VERIFIED writer still cannot ────
// reach the DBA-path graphs. The DAL writes instances + domain graphs; the
// ontology (schema) and security (Principal registry) graphs are refused even
// with a good identity — closing the priv-esc the design names (an instance
// writer minting itself a Principal or rewriting the shapes that validate it).

#[test]
fn dal_refuses_write_to_the_ontology_graph() {
    let s = IdStore::with(&["kade"]);
    let id = verify_identity(Some("kade"), &s).unwrap();
    let req = WriteReq {
        kind: "domain".into(),
        name: "x".into(),
        graph: Some("urn:chorus:ontology".into()),
        ..Default::default()
    };
    let e = write(&s, &req, &id).unwrap_err();
    assert!(e.starts_with("graph-dba-only"), "{}", e);
    assert!(s.updates.borrow().is_empty(), "nothing written to the ontology graph");
}

#[test]
fn dal_refuses_write_to_the_security_graph() {
    // The priv-esc case: a verified role trying to write the Principal registry.
    let s = IdStore::with(&["kade"]);
    let id = verify_identity(Some("kade"), &s).unwrap();
    let req = WriteReq {
        kind: "domain".into(),
        name: "x".into(),
        graph: Some("urn:chorus:domains:security".into()),
        ..Default::default()
    };
    let e = write(&s, &req, &id).unwrap_err();
    assert!(e.starts_with("graph-dba-only"), "{}", e);
    assert!(s.updates.borrow().is_empty(), "a writer cannot mint itself a Principal");
}

#[test]
fn dal_allows_instance_and_domain_graphs() {
    // The DAL's actual lane: the default instances graph and any domain graph.
    let s = IdStore::with(&["kade"]);
    let id = verify_identity(Some("kade"), &s).unwrap();
    // default (None → urn:chorus:instances)
    write(&s, &WriteReq { kind: "domain".into(), name: "a".into(), ..Default::default() }, &id)
        .expect("instances graph writes");
    // an explicit domain graph
    write(
        &s,
        &WriteReq {
            kind: "domain".into(),
            name: "b".into(),
            graph: Some("urn:chorus:domains:photos".into()),
            ..Default::default()
        },
        &id,
    )
    .expect("a domain graph writes");
    assert_eq!(s.updates.borrow().len(), 2, "both in-lane writes went through");
}
