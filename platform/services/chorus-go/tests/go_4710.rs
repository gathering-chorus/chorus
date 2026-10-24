// @test-type: unit — a scratch go store in a temp dir and a software P-256 key standing in for Jeff's phone; no network, no live store.
// @domain: security
// @card: #4710
// @owner: silas
//! #4710 — src/lib.rs: a go lands only when Jeff's passkey signed it, for
//! that card and that round. The 10-24 14:43 case: Borg Queen completed
//! #4717's go step with `accepter=jeff` typed by her. Each negative below is a
//! way a go nobody's phone signed could have landed.
use chorus_go::*;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::EncodePublicKey;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const RP: &str = "lightlifeurbangardens.com";
const ORIGIN: &str = "https://lightlifeurbangardens.com";
const ROUND: &str = "da13a20c285bc7620e81ac3109f9ada644012319";

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("chorus-go-{tag}-{}-{}", std::process::id(), now_nanos()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn now_nanos() -> u128 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() }

/// A phone: a key, and what it answers to create() and get().
struct Phone { key: SigningKey, counter: u32 }
impl Phone {
    fn new() -> Phone { Phone { key: SigningKey::random(&mut p256::elliptic_curve::rand_core::OsRng), counter: 0 } }
    fn spki(&self) -> String { b64url(self.key.verifying_key().to_public_key_der().unwrap().as_bytes()) }
    fn client_data(kind: &str, challenge: &[u8], origin: &str) -> Vec<u8> {
        format!(r#"{{"type":"{kind}","challenge":"{}","origin":"{origin}","crossOrigin":false}}"#, b64url(challenge)).into_bytes()
    }
    /// sign as an authenticator would; flags 0x05 = user present + verified
    fn sign(&mut self, challenge: &[u8], rp: &str, origin: &str, flags: u8) -> (String, String, String) {
        self.counter += 1;
        let mut ad: Vec<u8> = Sha256::digest(rp.as_bytes()).to_vec();
        ad.push(flags);
        ad.extend_from_slice(&self.counter.to_be_bytes());
        let cd = Phone::client_data("webauthn.get", challenge, origin);
        let mut msg = ad.clone();
        msg.extend_from_slice(&Sha256::digest(&cd));
        let sig: Signature = self.key.sign(&msg);
        (b64url(&ad), b64url(&cd), b64url(sig.to_der().as_bytes()))
    }
}

fn registered(tag: &str) -> (Store, Phone) {
    let store = Store::open(&tmp(tag)).unwrap();
    let phone = Phone::new();
    let token = store.mint_register_token().unwrap();
    let cd = Phone::client_data("webauthn.create", &register_challenge(&token), ORIGIN);
    register(&store, &token, "cred-1", &phone.spki(), &cd, RP, ORIGIN).unwrap();
    (store, phone)
}

/// Ask for a go the way the page does, and sign the given decision.
fn decide(store: &Store, phone: &mut Phone, card: u64, round: &str, decision: &str) -> Result<GoRecord, String> {
    let nonce = store.mint_challenge(card, "034lcDEndMqLApO6LeLeee", round).unwrap();
    let ch = go_challenge(card, "034lcDEndMqLApO6LeLeee", &format!("{round}|{decision}"), &nonce);
    let (ad, cd, sig) = phone.sign(&ch, RP, ORIGIN, 0x05);
    accept(store, &nonce, decision, &ad, &cd, &sig)
}

#[test]
fn a_go_signed_by_jeffs_phone_for_this_round_lands() {
    let (store, mut phone) = registered("ok");
    decide(&store, &mut phone, 4717, ROUND, "go").unwrap();
    assert_eq!(land_check(&store, 4717, ROUND).unwrap(), LandGo::Signed);
}

#[test]
fn negative_proof_a_go_nobodys_phone_signed_does_not_land() {
    let (store, _) = registered("unsigned");
    let e = land_check(&store, 4717, ROUND).unwrap_err();
    assert!(e.contains("no go signed by Jeff's passkey for #4717"), "{e}");
}

#[test]
fn negative_proof_a_go_for_an_older_round_does_not_land_this_one() {
    let (store, mut phone) = registered("round");
    decide(&store, &mut phone, 4717, "8e3f522e218ab121ee3f917c3d26c9e2486b8ddd", "go").unwrap();
    let e = land_check(&store, 4717, ROUND).unwrap_err();
    assert!(e.contains("not this round"), "{e}");
}

#[test]
fn negative_proof_a_go_for_another_card_does_not_land_this_one() {
    let (store, mut phone) = registered("card");
    let rec = decide(&store, &mut phone, 4716, ROUND, "go").unwrap();
    let pk = store.passkey().unwrap().unwrap();
    assert!(check_record(&pk, &rec, 4717, ROUND).unwrap_err().contains("for #4716, not #4717"));
}

#[test]
fn negative_proof_a_signed_no_cannot_be_edited_into_a_go() {
    let (store, mut phone) = registered("no");
    let mut rec = decide(&store, &mut phone, 4717, ROUND, "no").unwrap();
    assert!(land_check(&store, 4717, ROUND).unwrap_err().contains("decision is 'no'"));
    rec.decision = "go".into();
    store.save_record(&rec).unwrap();
    let e = land_check(&store, 4717, ROUND).unwrap_err();
    assert!(e.contains("not this card's go"), "an edited decision must break the signature: {e}");
}

#[test]
fn negative_proof_a_key_that_is_not_the_registered_passkey_is_refused() {
    let (store, _) = registered("otherkey");
    let mut other = Phone::new();
    let e = decide(&store, &mut other, 4717, ROUND, "go").unwrap_err();
    assert!(e.contains("not Jeff's passkey"), "{e}");
    assert!(store.record(4717).unwrap().is_none(), "a refused go stores nothing");
}

#[test]
fn negative_proof_a_challenge_is_spent_once() {
    let (store, mut phone) = registered("replay");
    let nonce = store.mint_challenge(4717, "run1", ROUND).unwrap();
    let ch = go_challenge(4717, "run1", &format!("{ROUND}|go"), &nonce);
    let (ad, cd, sig) = phone.sign(&ch, RP, ORIGIN, 0x05);
    accept(&store, &nonce, "go", &ad, &cd, &sig).unwrap();
    let e = accept(&store, &nonce, "go", &ad, &cd, &sig).unwrap_err();
    assert!(e.contains("already used"), "{e}");
}

#[test]
fn negative_proof_no_face_id_wrong_site_or_wrong_origin_is_refused() {
    let (store, mut phone) = registered("flags");
    for (rp, origin, flags, want) in [
        (RP, ORIGIN, 0x01u8, "not verified"),
        ("evil.example", ORIGIN, 0x05, "another site"),
        (RP, "https://evil.example", 0x05, "signed on"),
    ] {
        let nonce = store.mint_challenge(4717, "run1", ROUND).unwrap();
        let ch = go_challenge(4717, "run1", &format!("{ROUND}|go"), &nonce);
        let (ad, cd, sig) = phone.sign(&ch, rp, origin, flags);
        let e = accept(&store, &nonce, "go", &ad, &cd, &sig).unwrap_err();
        assert!(e.contains(want), "{want}: {e}");
    }
}

#[test]
fn negative_proof_a_counter_that_goes_backwards_is_refused() {
    let (store, mut phone) = registered("counter");
    phone.counter = 10;
    decide(&store, &mut phone, 4717, ROUND, "go").unwrap();
    phone.counter = 3;
    assert!(decide(&store, &mut phone, 4717, ROUND, "go").unwrap_err().contains("counter went backwards"));
}

#[test]
fn with_no_passkey_registered_the_land_says_so() {
    let store = Store::open(&tmp("none")).unwrap();
    assert_eq!(land_check(&store, 4717, ROUND).unwrap(), LandGo::NoPasskey);
}

#[test]
fn negative_proof_a_registration_link_works_once() {
    let store = Store::open(&tmp("reg")).unwrap();
    let phone = Phone::new();
    let token = store.mint_register_token().unwrap();
    let cd = Phone::client_data("webauthn.create", &register_challenge(&token), ORIGIN);
    register(&store, &token, "cred-1", &phone.spki(), &cd, RP, ORIGIN).unwrap();
    let e = register(&store, &token, "cred-2", &Phone::new().spki(), &cd, RP, ORIGIN).unwrap_err();
    assert!(e.contains("no registration link is open"), "{e}");
    assert_eq!(store.passkey().unwrap().unwrap().cred_id, "cred-1");
    // and a made-up token never registers
    let _ = store.mint_register_token().unwrap();
    assert!(register(&store, "made-up", "cred-3", &phone.spki(), &cd, RP, ORIGIN).is_err());
}

/// Kade's review: a challenge is spent even when its answer fails, so a bad
/// answer cannot be retried into a good one. Jeff's retry is a page reload,
/// which asks for a fresh challenge, and that one works.
#[test]
fn a_failed_answer_spends_its_challenge_and_a_fresh_one_still_works() {
    let (store, mut phone) = registered("retry");
    let nonce = store.mint_challenge(4717, "run1", ROUND).unwrap();
    let ch = go_challenge(4717, "run1", &format!("{ROUND}|go"), &nonce);
    let (ad, cd, _) = phone.sign(&ch, RP, ORIGIN, 0x05);
    let bad = b64url(b"not a signature");
    assert!(accept(&store, &nonce, "go", &ad, &cd, &bad).is_err());
    let (ad, cd, sig) = phone.sign(&ch, RP, ORIGIN, 0x05);
    assert!(accept(&store, &nonce, "go", &ad, &cd, &sig).unwrap_err().contains("already used"), "the spent challenge stays spent");
    decide(&store, &mut phone, 4717, ROUND, "go").unwrap();
    assert_eq!(land_check(&store, 4717, ROUND).unwrap(), LandGo::Signed);
}
