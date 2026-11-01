// @test-type: unit — a scratch go store in a temp dir and a software P-256 key standing in for Jeff's phone; no network, no live store.
// @domain: security
// @card: #4505
// @owner: wren
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


// #4505 — an approval lands only on Jeff's passkey, for that row, that exact action, that decision.
const ARGV: &str = r#"["add","Retire the X","--owner","kade","--priority","P3"]"#;

fn row(action: &str, state: &str) -> ApprovalRow {
    ApprovalRow { id: "approval-wren-1".into(), state: state.into(), action: action.into() }
}

fn decide_row(store: &Store, phone: &mut Phone, shown: &ApprovalRow, now: &ApprovalRow, decision: &str) -> Result<ApprovalDecision, String> {
    let sha = action_sha(&shown.action);
    let nonce = store.mint_approval_challenge(&shown.id, &sha).unwrap();
    let (ad, cd, sig) = phone.sign(&approval_challenge(&shown.id, &sha, decision, &nonce), RP, ORIGIN, 0x05);
    decide_approval(store, &nonce, decision, now, &ad, &cd, &sig)
}

#[test]
fn jeffs_approve_returns_the_argv_to_replay() {
    let (store, mut phone) = registered("ap-ok");
    let r = row(ARGV, "pending");
    let d = decide_row(&store, &mut phone, &r, &r, "approve").unwrap();
    assert_eq!(d.argv[0], "add");
    assert_eq!(d.argv[1], "Retire the X");
    assert_eq!(d.decision, "approve");
}

#[test]
fn negative_proof_an_unsigned_decision_is_refused() {
    let (store, _) = registered("ap-unsigned");
    let mut other = Phone::new();
    let r = row(ARGV, "pending");
    let e = decide_row(&store, &mut other, &r, &r, "approve").unwrap_err();
    assert!(!e.is_empty());
}

#[test]
fn negative_proof_a_signed_decline_cannot_be_replayed_as_approve() {
    let (store, mut phone) = registered("ap-flip");
    let r = row(ARGV, "pending");
    let sha = action_sha(ARGV);
    let nonce = store.mint_approval_challenge(&r.id, &sha).unwrap();
    let (ad, cd, sig) = phone.sign(&approval_challenge(&r.id, &sha, "decline", &nonce), RP, ORIGIN, 0x05);
    assert!(decide_approval(&store, &nonce, "approve", &r, &ad, &cd, &sig).is_err());
}

#[test]
fn negative_proof_a_row_edited_after_the_page_was_shown_is_refused() {
    let (store, mut phone) = registered("ap-edit");
    let shown = row(ARGV, "pending");
    let edited = row(r#"["add","Retire the X","--owner","jeff"]"#, "pending");
    let e = decide_row(&store, &mut phone, &shown, &edited, "approve").unwrap_err();
    assert!(e.contains("action changed"), "{e}");
}

#[test]
fn negative_proof_a_decided_row_is_not_decided_again() {
    let (store, mut phone) = registered("ap-twice");
    let shown = row(ARGV, "pending");
    let decided = row(ARGV, "approved");
    let e = decide_row(&store, &mut phone, &shown, &decided, "approve").unwrap_err();
    assert!(e.contains("already decided"), "{e}");
}

#[test]
fn negative_proof_a_go_nonce_cannot_be_spent_on_an_approval() {
    let (store, mut phone) = registered("ap-go-nonce");
    let r = row(ARGV, "pending");
    let go_nonce = store.mint_challenge(4505, "034lcDEndMqLApO6LeLeee", ROUND).unwrap();
    let (ad, cd, sig) = phone.sign(&approval_challenge(&r.id, &action_sha(ARGV), "approve", &go_nonce), RP, ORIGIN, 0x05);
    let e = decide_approval(&store, &go_nonce, "approve", &r, &ad, &cd, &sig).unwrap_err();
    assert!(e.contains("already used, or never asked for"), "{e}");
}

#[test]
fn negative_proof_an_action_that_is_not_a_cards_add_argv_is_refused() {
    for bad in [r#"["rm","-rf","/"]"#, r#""add x; rm -rf /""#, r#"["add"]"#, r#"["add",1]"#, "not json"] {
        let e = cards_add_argv(bad).unwrap_err();
        assert!(e.contains("not a cards add argv"), "{bad}: {e}");
    }
    let (store, mut phone) = registered("ap-argv");
    let r = row(r#"["move","4505","Done"]"#, "pending");
    let e = decide_row(&store, &mut phone, &r, &r, "approve").unwrap_err();
    assert!(e.contains("not a cards add argv"), "{e}");
}

#[test]
fn negative_proof_an_approval_nonce_is_spent_once() {
    let (store, mut phone) = registered("ap-once");
    let r = row(ARGV, "pending");
    let sha = action_sha(ARGV);
    let nonce = store.mint_approval_challenge(&r.id, &sha).unwrap();
    let (ad, cd, sig) = phone.sign(&approval_challenge(&r.id, &sha, "approve", &nonce), RP, ORIGIN, 0x05);
    decide_approval(&store, &nonce, "approve", &r, &ad, &cd, &sig).unwrap();
    assert!(decide_approval(&store, &nonce, "approve", &r, &ad, &cd, &sig).is_err());
}

#[test]
fn negative_proof_only_allow_listed_flags_ride_an_approval() {
    for bad in [r#"["add","T","--desc-file","/etc/hosts"]"#,
                r#"["add","T","--desc","-"]"#, r#"["add","T","--owner"]"#,
                r#"["add","--owner","kade"]"#, r#"["add","T","--owner","--desc-file"]"#] {
        let e = cards_add_argv(bad).unwrap_err();
        assert!(e.contains("not a cards add argv"), "{bad}: {e}");
    }
    assert!(cards_add_argv(r###"["add","T","--desc","## Experience: x","--sequence","werk"]"###).is_ok());
}

#[test]
fn a_decline_clears_a_junk_row_and_replays_nothing() {
    let (store, mut phone) = registered("ap-junk");
    let r = row(r#"["rm","-rf","/"]"#, "pending");
    let d = decide_row(&store, &mut phone, &r, &r, "decline").unwrap();
    assert!(d.argv.is_empty());
}

#[test]
fn a_decision_keeps_its_signed_proof() {
    let (store, mut phone) = registered("ap-proof");
    let r = row(ARGV, "pending");
    decide_row(&store, &mut phone, &r, &r, "approve").unwrap();
    let p = store.dir.join("approvals").join("decided-approval-wren-1.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    assert_eq!(v["decision"], "approve");
    assert!(!v["signature"].as_str().unwrap().is_empty());
}
