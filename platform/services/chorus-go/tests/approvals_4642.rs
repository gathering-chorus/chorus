// @test-type: unit — a scratch go store in a temp dir and a software P-256 key standing in for Jeff's phone; no network, no live store.
// @domain: security
// @card: #4642
// @owner: wren
use chorus_go::*;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::EncodePublicKey;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const RP: &str = "lightlifeurbangardens.com";
const ORIGIN: &str = "https://lightlifeurbangardens.com";

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

// #4642 — a go waits on the same list: Jeff's passkey signs one card's run at one round,
// the land re-verifies that signature, and a go and a card ask never cross-replay.
const HEAD: &str = "5174a0592261bbd5b452ec1287cda4e5e2fda45a";
const OLD: &str = "e04787ca93b83d7d1a610cbaafa64d337c9a9ab5";

fn go_json(card: u64, round: &str) -> String {
    format!(r#"{{"card":{card},"run":"034qFPOewc9praHJbg6SYH","round":"{round}"}}"#)
}
fn go_row(action: &str, now: Option<&str>) -> ApprovalRow {
    ApprovalRow { id: "approval-go-4642".into(), state: "pending".into(), action: action.into(), kind: "go".into(), round_now: now.map(str::to_string) }
}
/// Decide a row the way the page does; returns the decision and the proof the server stores.
fn sign(store: &Store, phone: &mut Phone, row: &ApprovalRow, verdict: &str) -> Result<(ApprovalDecision, serde_json::Value), String> {
    let sha = action_sha(&row.action);
    let nonce = store.mint_approval_challenge(&row.id, &sha).unwrap();
    let (ad, cd, sig) = phone.sign(&approval_challenge(&row.id, &sha, verdict, &nonce), RP, ORIGIN, 0x05);
    let d = decide_approval(store, &nonce, verdict, row, &ad, &cd, &sig)?;
    Ok((d, serde_json::json!({"nonce": nonce, "authData": ad, "clientDataJSON": cd, "signature": sig})))
}

#[test]
fn a_go_signed_for_this_round_is_a_go_the_land_accepts() {
    let (store, mut phone) = registered("go-ok");
    let r = go_row(&go_json(4642, HEAD), Some(HEAD));
    let (d, proof) = sign(&store, &mut phone, &r, "approve").unwrap();
    assert_eq!(d.go.as_ref().unwrap().round, HEAD);
    assert!(d.argv.is_empty(), "a go replays no cards add");
    let pk = store.passkey().unwrap().unwrap();
    check_go_decision(&pk, &r.id, &r.action, "approve", &proof, 4642, HEAD).unwrap();
}

#[test]
fn negative_proof_a_go_for_a_stale_round_is_refused_at_decide() {
    let (store, mut phone) = registered("go-stale");
    let r = go_row(&go_json(4642, OLD), Some(HEAD));
    let e = sign(&store, &mut phone, &r, "approve").unwrap_err();
    assert!(e.contains("the demo moved on"), "{e}");
}

#[test]
fn negative_proof_go_and_card_actions_never_cross_replay() {
    let (store, mut phone) = registered("go-cross");
    let r = go_row(r#"["add","T","--owner","kade"]"#, Some(HEAD));
    assert!(sign(&store, &mut phone, &r, "approve").unwrap_err().contains("not a go action"));
    let c = ApprovalRow { id: "approval-wren-x".into(), state: "pending".into(), action: go_json(4642, HEAD), kind: "card".into(), round_now: None };
    assert!(sign(&store, &mut phone, &c, "approve").unwrap_err().contains("not a cards add argv"));
    for bad in [r#"{"card":4642,"run":"r1","round":"abc1234","extra":1}"#, r#"{"card":4642,"run":"a b","round":"abc1234"}"#,
                r#"{"card":0,"run":"r1","round":"abc1234"}"#, r#"{"card":4642,"run":"r1","round":"HEAD"}"#,
                // a short prefix is not a round (Silas): the land compares full HEADs
                r#"{"card":4642,"run":"r1","round":"5174a0592261"}"#] {
        assert!(go_action(bad).unwrap_err().contains("not a go action"), "{bad}");
    }
}

#[test]
fn negative_proof_the_land_refuses_a_go_row_its_signature_does_not_back() {
    let (store, mut phone) = registered("go-land");
    let r = go_row(&go_json(4642, HEAD), Some(HEAD));
    let (_, proof) = sign(&store, &mut phone, &r, "approve").unwrap();
    let pk = store.passkey().unwrap().unwrap();
    assert!(check_go_decision(&pk, &r.id, &r.action, "approve", &proof, 4505, HEAD).unwrap_err().contains("not #4505"));
    assert!(check_go_decision(&pk, &r.id, &r.action, "approve", &proof, 4642, OLD).unwrap_err().contains("signed for round"));
    assert!(check_go_decision(&pk, &r.id, &r.action, "decline", &proof, 4642, HEAD).unwrap_err().contains("declined"));
    assert!(check_go_decision(&pk, &r.id, &r.action, "approve", &serde_json::json!({}), 4642, HEAD).unwrap_err().contains("no signature"));
    let mut forged = proof.clone();
    forged["nonce"] = serde_json::json!("another-nonce");
    assert!(check_go_decision(&pk, &r.id, &r.action, "approve", &forged, 4642, HEAD).unwrap_err().contains("not signed by Jeff's passkey"));
    // the same signature does not carry over to an action rewritten after Jeff signed
    assert!(check_go_decision(&pk, &r.id, &go_json(4642, OLD), "approve", &proof, 4642, OLD).is_err());
}

/// The door's rows for one go and Jeff's decision on it, as the land reads them.
fn door_rows(r: &ApprovalRow, proof: &serde_json::Value, owner: &str, verdict: &str) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    (vec![serde_json::json!({"name": format!("chorus:{}", r.id), "approvalKind": "go", "approvalAction": r.action})],
     vec![serde_json::json!({"name": "decision-x", "decides": format!("chorus:{}", r.id), "ownedBy": format!("chorus:{owner}"),
                             "approvalVerdict": verdict, "passkeyProof": proof.to_string()})])
}

#[test]
fn the_land_finds_jeffs_signed_go_in_the_door_rows() {
    let (store, mut phone) = registered("land-rows");
    let r = go_row(&go_json(4642, HEAD), Some(HEAD));
    let (_, proof) = sign(&store, &mut phone, &r, "approve").unwrap();
    let pk = store.passkey().unwrap().unwrap();
    let (a, d) = door_rows(&r, &proof, "principal-jeff", "approve");
    land_go_from_rows(&pk, &a, &d, 4642, HEAD).unwrap();
}

// #4710 — the lander writes Jeff's decision as principal-lander; his signature still decides.
#[test]
fn a_lander_owned_row_with_jeffs_signature_lands_and_with_a_bad_one_does_not() {
    let (store, mut phone) = registered("land-lander");
    let r = go_row(&go_json(4642, HEAD), Some(HEAD));
    let (_, proof) = sign(&store, &mut phone, &r, "approve").unwrap();
    let pk = store.passkey().unwrap().unwrap();
    let (a, d) = door_rows(&r, &proof, "principal-lander", "approve");
    land_go_from_rows(&pk, &a, &d, 4642, HEAD).unwrap();
    // NEGATIVE PROOF (Silas 18:09): owner alone never passes — a lander-owned row whose proof
    // is not Jeff's signature over this card, run and round is refused
    let (a, d) = door_rows(&r, &serde_json::json!({}), "principal-lander", "approve");
    assert!(land_go_from_rows(&pk, &a, &d, 4642, HEAD).is_err());
    let mut forged = proof.clone();
    forged["signature"] = serde_json::json!("AAAA");
    let (a, d) = door_rows(&r, &forged, "principal-lander", "approve");
    assert!(land_go_from_rows(&pk, &a, &d, 4642, HEAD).is_err());
}

#[test]
fn negative_proof_the_land_refuses_rows_that_are_not_jeffs_signed_go() {
    let (store, mut phone) = registered("land-neg");
    let r = go_row(&go_json(4642, HEAD), Some(HEAD));
    let (_, proof) = sign(&store, &mut phone, &r, "approve").unwrap();
    let pk = store.passkey().unwrap().unwrap();
    // a decision a role wrote decides nothing (the door stamps owners; the land checks it too)
    let (a, d) = door_rows(&r, &proof, "principal-kade", "approve");
    assert!(land_go_from_rows(&pk, &a, &d, 4642, HEAD).unwrap_err().contains("not principal-jeff or principal-lander"));
    // no go row for this round, and no rows at all
    let (a, d) = door_rows(&r, &proof, "principal-jeff", "approve");
    assert!(land_go_from_rows(&pk, &a, &d, 4642, OLD).unwrap_err().contains("no go signed"));
    assert!(land_go_from_rows(&pk, &[], &[], 4642, HEAD).unwrap_err().contains("no go signed"));
    // the action comes from the Approval row: a row rewritten after Jeff signed does not land
    let mut rewritten = a.clone();
    rewritten[0]["approvalAction"] = serde_json::json!(go_json(4642, HEAD).replace("034qFPOewc9praHJbg6SYH", "034qOtherRunAAAAAAAAAA"));
    assert!(land_go_from_rows(&pk, &rewritten, &d, 4642, HEAD).unwrap_err().contains("not signed by Jeff's passkey"));
    // a decline is not a go
    let (a, d) = door_rows(&r, &proof, "principal-jeff", "decline");
    assert!(land_go_from_rows(&pk, &a, &d, 4642, HEAD).is_err());
}

#[test]
fn a_declined_go_is_recorded_and_lands_nothing() {
    let (store, mut phone) = registered("go-no");
    let r = go_row(&go_json(4642, HEAD), Some(HEAD));
    let (d, proof) = sign(&store, &mut phone, &r, "decline").unwrap();
    assert_eq!(d.decision, "decline");
    let pk = store.passkey().unwrap().unwrap();
    assert!(check_go_decision(&pk, &r.id, &r.action, "decline", &proof, 4642, HEAD).is_err());
    // nor does its local record land
    assert!(land_go_from_local(&pk, &store, 4642, HEAD).is_err());
}

// #4642 break-glass (Silas 19:27): the door never got Jeff's row (no identity minted for jeff),
// so the land reads the signed record chorus-go wrote first, re-verified the same way.
#[test]
fn break_glass_a_signed_go_lands_from_the_local_record_alone() {
    let (store, mut phone) = registered("glass-ok");
    let r = go_row(&go_json(4642, HEAD), Some(HEAD));
    let (d, _) = sign(&store, &mut phone, &r, "approve").unwrap();
    // the server keeps a go's proof once its merge happened (review of 113c9fe51)
    keep_decision_proof(&store, &d).unwrap();
    let pk = store.passkey().unwrap().unwrap();
    land_go_from_local(&pk, &store, 4642, HEAD).unwrap();
    assert!(land_go_from_local(&pk, &store, 4642, OLD).is_err(), "another round does not land");
    assert!(land_go_from_local(&pk, &store, 4505, HEAD).is_err(), "another card does not land");
}

#[test]
fn negative_proof_break_glass_refuses_a_forged_or_edited_local_record() {
    let (store, mut phone) = registered("glass-neg");
    let r = go_row(&go_json(4642, HEAD), Some(HEAD));
    let (d, _) = sign(&store, &mut phone, &r, "approve").unwrap();
    keep_decision_proof(&store, &d).unwrap();
    let pk = store.passkey().unwrap().unwrap();
    let path = store.dir.join("approvals").join("decided-approval-go-4642.json");
    let original = std::fs::read_to_string(&path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&original).unwrap();
    // a signature that is not Jeff's phone
    v["nonce"] = serde_json::json!("another-nonce");
    std::fs::write(&path, v.to_string()).unwrap();
    assert!(land_go_from_local(&pk, &store, 4642, HEAD).unwrap_err().contains("not signed by Jeff's passkey"));
    // an action edited after signing (its recorded sha no longer matches)
    let mut v: serde_json::Value = serde_json::from_str(&original).unwrap();
    v["action"] = serde_json::json!(go_json(4642, HEAD).replace("034qFPOewc9praHJbg6SYH", "034qOtherRunAAAAAAAAAA"));
    std::fs::write(&path, v.to_string()).unwrap();
    assert!(land_go_from_local(&pk, &store, 4642, HEAD).unwrap_err().contains("does not match what was signed"));
    // a record a role wrote with no signature at all
    std::fs::write(&path, serde_json::json!({"row": "approval-go-4642", "kind": "go", "decision": "approve",
        "action": go_json(4642, HEAD), "action_sha": action_sha(&go_json(4642, HEAD))}).to_string()).unwrap();
    assert!(land_go_from_local(&pk, &store, 4642, HEAD).is_err());
}


// #4710 review of 113c9fe51 (A) — a go's signed proof is kept only after its merge happened. A go
// whose merge was refused leaves no decided file: the land has nothing to fall back to, and Jeff
// can sign the same row again.
#[test]
fn negative_proof_a_go_whose_merge_was_refused_leaves_nothing_and_can_be_signed_again() {
    let (store, mut phone) = registered("refused-merge");
    let r = go_row(&go_json(4642, HEAD), Some(HEAD));
    sign(&store, &mut phone, &r, "approve").unwrap();
    // the merge was refused, so the server never kept the proof
    let path = store.dir.join("approvals").join("decided-approval-go-4642.json");
    assert!(!path.exists(), "a go with no merge must leave no decided file");
    let pk = store.passkey().unwrap().unwrap();
    assert!(land_go_from_local(&pk, &store, 4642, HEAD).is_err(), "break-glass must not see an unmerged go as signed");
    // Jeff signs again once it is fixed: accepted, and this time the merge happened
    let (d, _) = sign(&store, &mut phone, &r, "approve").expect("a refused merge must stay re-signable");
    keep_decision_proof(&store, &d).unwrap();
    land_go_from_local(&pk, &store, 4642, HEAD).unwrap();
    // and once kept, it is never decided twice here
    assert!(sign(&store, &mut phone, &r, "approve").unwrap_err().contains("already decided here"));
}
