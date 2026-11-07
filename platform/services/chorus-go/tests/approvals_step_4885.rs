// @test-type: unit — a scratch go store in a temp dir and a software P-256 key standing in for Jeff's phone; no network, no live store.
// @domain: security
// @card: #4885
// @owner: silas
//! #4885 — a pipeline's human step that only Jeff may answer (the domain flow's bar) is answered
//! from his passkey, like the go. A role typing `by=jeff` into the step is not his answer
//! (Wren 11-04 04:00, the accepter=jeff hole). The signature binds one run's one step (Kade 04:03),
//! and a step answer and a go never cross-replay.
use chorus_go::*;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::EncodePublicKey;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const RP: &str = "lightlifeurbangardens.com";
const ORIGIN: &str = "https://lightlifeurbangardens.com";
const HEAD: &str = "5174a0592261bbd5b452ec1287cda4e5e2fda45a";
const RUN: &str = "034reUUglbqhkVc0IKNs2F";

fn tmp(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("chorus-go-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
struct Phone { key: SigningKey, counter: u32 }
impl Phone {
    fn new() -> Phone { Phone { key: SigningKey::random(&mut p256::elliptic_curve::rand_core::OsRng), counter: 0 } }
    fn spki(&self) -> String { b64url(self.key.verifying_key().to_public_key_der().unwrap().as_bytes()) }
    fn client_data(kind: &str, challenge: &[u8]) -> Vec<u8> {
        format!(r#"{{"type":"{kind}","challenge":"{}","origin":"{ORIGIN}","crossOrigin":false}}"#, b64url(challenge)).into_bytes()
    }
    fn sign(&mut self, challenge: &[u8]) -> (String, String, String) {
        self.counter += 1;
        let mut ad: Vec<u8> = Sha256::digest(RP.as_bytes()).to_vec();
        ad.push(0x05);
        ad.extend_from_slice(&self.counter.to_be_bytes());
        let cd = Phone::client_data("webauthn.get", challenge);
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
    let cd = Phone::client_data("webauthn.create", &register_challenge(&token));
    register(&store, &token, "cred-1", &phone.spki(), &cd, RP, ORIGIN).unwrap();
    (store, phone)
}
fn step_json(run: &str, step: &str) -> String {
    format!(r#"{{"card":4885,"dag":"domain-flow","run":"{run}","step":"{step}"}}"#)
}
fn row(kind: &str, action: &str) -> ApprovalRow {
    ApprovalRow { id: format!("approval-{kind}-4885"), state: "pending".into(), action: action.into(), kind: kind.into(), round_now: Some(HEAD.into()) }
}
fn sign(store: &Store, phone: &mut Phone, r: &ApprovalRow, verdict: &str) -> Result<(ApprovalDecision, serde_json::Value), String> {
    let sha = action_sha(&r.action);
    let nonce = store.mint_approval_challenge(&r.id, &sha).unwrap();
    let (ad, cd, sig) = phone.sign(&approval_challenge(&r.id, &sha, verdict, &nonce));
    let d = decide_approval(store, &nonce, verdict, r, &ad, &cd, &sig)?;
    Ok((d, serde_json::json!({"nonce": nonce, "authData": ad, "clientDataJSON": cd, "signature": sig})))
}
fn door(r: &ApprovalRow, proof: &serde_json::Value, owner: &str, verdict: &str) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    (vec![serde_json::json!({"name": format!("chorus:{}", r.id), "approvalKind": r.kind, "approvalAction": r.action})],
     vec![serde_json::json!({"name": "decision-x", "decides": format!("chorus:{}", r.id), "ownedBy": format!("chorus:{owner}"),
                             "approvalVerdict": verdict, "passkeyProof": proof.to_string()})])
}

#[test]
fn a_step_jeff_signed_is_the_answer_its_run_accepts() {
    let (store, mut phone) = registered("step-ok");
    let r = row("step", &step_json(RUN, "skill_domain_flow_bar"));
    let (d, proof) = sign(&store, &mut phone, &r, "approve").unwrap();
    let s = d.step.as_ref().expect("a step decision names its step");
    assert_eq!((s.dag.as_str(), s.run.as_str(), s.step.as_str()), ("domain-flow", RUN, "skill_domain_flow_bar"));
    assert!(d.go.is_none() && d.argv.is_empty(), "a step answer is neither a go nor a card");
    let pk = store.passkey().unwrap().unwrap();
    let (a, dd) = door(&r, &proof, "principal-jeff", "approve");
    land_step_from_rows(&pk, &a, &dd, "domain-flow", RUN, "skill_domain_flow_bar").unwrap();
}

#[test]
fn negative_proof_an_unsigned_step_is_refused() {
    let (store, mut phone) = registered("step-unsigned");
    let r = row("step", &step_json(RUN, "skill_domain_flow_bar"));
    let (_, proof) = sign(&store, &mut phone, &r, "approve").unwrap();
    let pk = store.passkey().unwrap().unwrap();
    // no decision row at all: a role that typed by=jeff into dagu left nothing here
    let (a, _) = door(&r, &proof, "principal-jeff", "approve");
    assert!(land_step_from_rows(&pk, &a, &[], "domain-flow", RUN, "skill_domain_flow_bar").unwrap_err().contains("not signed"));
    // a decision with no signature, and one owned by a role, decide nothing
    let (a, d) = door(&r, &serde_json::json!({}), "principal-jeff", "approve");
    assert!(land_step_from_rows(&pk, &a, &d, "domain-flow", RUN, "skill_domain_flow_bar").is_err());
    let (a, d) = door(&r, &proof, "principal-silas", "approve");
    assert!(land_step_from_rows(&pk, &a, &d, "domain-flow", RUN, "skill_domain_flow_bar").unwrap_err().contains("principal-jeff"));
    // a declined bar is no bar
    let (a, d) = door(&r, &proof, "principal-jeff", "decline");
    assert!(land_step_from_rows(&pk, &a, &d, "domain-flow", RUN, "skill_domain_flow_bar").is_err());
}

#[test]
fn negative_proof_a_step_signature_does_not_replay_to_another_run_or_step() {
    let (store, mut phone) = registered("step-replay");
    let r = row("step", &step_json(RUN, "skill_domain_flow_bar"));
    let (_, proof) = sign(&store, &mut phone, &r, "approve").unwrap();
    let pk = store.passkey().unwrap().unwrap();
    let (a, d) = door(&r, &proof, "principal-jeff", "approve");
    assert!(land_step_from_rows(&pk, &a, &d, "domain-flow", "034otherRun0000000000", "skill_domain_flow_bar").is_err());
    assert!(land_step_from_rows(&pk, &a, &d, "domain-flow", RUN, "skill_domain_flow_wire").is_err());
    assert!(land_step_from_rows(&pk, &a, &d, "cicd-go", RUN, "skill_domain_flow_bar").is_err());
    // the same signature does not back an action rewritten to another run after Jeff signed
    let moved = ApprovalRow { action: step_json("034otherRun0000000000", "skill_domain_flow_bar"), ..r.clone() };
    let (a, d) = door(&moved, &proof, "principal-jeff", "approve");
    assert!(land_step_from_rows(&pk, &a, &d, "domain-flow", "034otherRun0000000000", "skill_domain_flow_bar").unwrap_err().contains("not signed by Jeff's passkey"));
}

#[test]
fn negative_proof_step_and_go_never_cross_replay() {
    let (store, mut phone) = registered("step-cross");
    // a step action on a go row is not a go
    let g = row("go", &step_json(RUN, "skill_domain_flow_bar"));
    assert!(sign(&store, &mut phone, &g, "approve").unwrap_err().contains("not a go action"));
    // a go action on a step row is not a step
    let go = format!(r#"{{"card":4885,"run":"{RUN}","round":"{HEAD}"}}"#);
    let s = row("step", &go);
    assert!(sign(&store, &mut phone, &s, "approve").unwrap_err().contains("not a step action"));
    // and a signed step never answers the land's go check
    let r = row("step", &step_json(RUN, "skill_domain_flow_bar"));
    let (_, proof) = sign(&store, &mut phone, &r, "approve").unwrap();
    let pk = store.passkey().unwrap().unwrap();
    let (a, d) = door(&r, &proof, "principal-jeff", "approve");
    assert!(land_go_from_rows(&pk, &a, &d, 4885, HEAD).is_err());
    for bad in [r#"{"card":4885,"dag":"domain-flow","run":"r1"}"#, r#"{"card":4885,"dag":"domain-flow","run":"a b","step":"s"}"#,
                r#"{"card":4885,"dag":"domain-flow","run":"r1","step":"s","extra":1}"#, r#"{"card":0,"dag":"d","run":"r1","step":"s"}"#] {
        assert!(step_action(bad).unwrap_err().contains("not a step action"), "{bad}");
    }
}
