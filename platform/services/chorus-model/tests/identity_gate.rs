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

use chorus_model::{verify_identity, write, Store, WriteReq, R};
use std::cell::RefCell;

/// Hermetic stub (the constraint_enforcement.rs pattern): Principal-registry
/// ASKs route on the security graph; everything else answers permissively so
/// identity is the only variable under test.
struct IdStore {
    principals: Vec<String>, // registered claims, e.g. "kade"
    asks: RefCell<usize>,    // every ASK that reached the store
    updates: RefCell<Vec<String>>,
}
impl IdStore {
    fn with(principals: &[&str]) -> Self {
        Self {
            principals: principals.iter().map(|s| s.to_string()).collect(),
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
    fn select_v(&self, _sparql: &str) -> R<Vec<String>> {
        Ok(vec![]) // no shape constraints, no existing created stamp
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
// Spawn the REAL chorus-model binary. DEPLOY_ROLE is removed from its env and
// the store points at a dead port: the refusal must be identity-missing, i.e.
// it fires BEFORE any store/network contact — no default identity ever forms.

#[test]
fn cli_write_without_deploy_role_is_refused_before_store_contact() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_chorus-model"))
        .args(["add", "--kind", "domain", "--name", "bypass-probe"])
        .env_remove("DEPLOY_ROLE")
        .env("CHORUS_FUSEKI", "http://127.0.0.1:1") // dead — must never be reached
        .output()
        .expect("binary runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "bypass write must refuse");
    assert!(err.contains("identity-missing"), "refusal names the cause: {}", err);
    assert!(!err.contains("fuseki"), "refusal precedes store contact: {}", err);
}

#[test]
fn cli_write_with_unregistered_identity_is_refused() {
    // With a claim present the registry must answer; a dead store = fail closed,
    // never fail open. Either way: no write, non-zero exit.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_chorus-model"))
        .args(["add", "--kind", "domain", "--name", "bypass-probe"])
        .env("DEPLOY_ROLE", "intruder")
        .env("CHORUS_FUSEKI", "http://127.0.0.1:1")
        .output()
        .expect("binary runs");
    assert!(!out.status.success(), "unverifiable identity must refuse");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("written:"));
}

#[test]
fn cli_dry_run_needs_no_identity() {
    // --dry-run writes nothing — it stays usable without an identity (sketch surface).
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_chorus-model"))
        .args(["add", "--kind", "domain", "--name", "sketch", "--dry-run"])
        .env_remove("DEPLOY_ROLE")
        .output()
        .expect("binary runs");
    assert!(out.status.success(), "dry-run is identity-free: {}", String::from_utf8_lossy(&out.stderr));
}
