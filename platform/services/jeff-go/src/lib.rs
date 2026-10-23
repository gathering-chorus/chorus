// @domain: security
//! #4707 AC1 — Jeff's go, proven. Every role runs as Jeff's Mac user, so any
//! string a role can type (accepter=jeff) or any key file it can read (the
//! Clearing signer's client key) can be used to fake his go. A passkey
//! assertion cannot: the authenticator signs only after Touch ID (user
//! verification), and the challenge binds the go to one card, one cicd run
//! and one werk HEAD, with a nonce that is spent on first use.
//!
//! `verify` is pure. The caller supplies the stored credential, the nonces
//! already spent, and what this land expects; it gets back the new
//! signature counter to store, or a typed refusal.
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// What one land expects the go to be for.
#[derive(Debug, Clone)]
pub struct Expected {
    pub card: u64,
    pub run_id: String,
    pub werk_head: String,
    pub nonce: String,
    /// The page Jeff answered on, e.g. "http://192.168.86.242:3000".
    pub origin: String,
    /// The relying party id the credential was registered for, e.g. "192.168.86.242".
    pub rp_id: String,
}

/// Jeff's registered passkey: SEC1 public key (65 bytes, uncompressed) and
/// the last signature counter seen.
#[derive(Debug, Clone)]
pub struct Credential {
    pub public_key_sec1: Vec<u8>,
    pub sign_count: u32,
}

/// The three parts of a WebAuthn assertion, as the browser returns them.
#[derive(Debug, Clone)]
pub struct Assertion {
    pub client_data_json: Vec<u8>,
    pub authenticator_data: Vec<u8>,
    /// ASN.1 DER ECDSA signature (ES256).
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Malformed(String),
    WrongType(String),
    WrongChallenge,
    WrongOrigin(String),
    WrongRelyingParty,
    NoUserPresence,
    NoUserVerification,
    NonceSpent,
    CounterWentBack { stored: u32, got: u32 },
    BadSignature,
    BadKey,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Malformed(w) => write!(f, "no-go: the assertion is malformed ({w})"),
            Refusal::WrongType(t) => write!(f, "no-go: clientData type is {t}, not webauthn.get"),
            Refusal::WrongChallenge => write!(f, "no-go: the go is for another card, run or HEAD"),
            Refusal::WrongOrigin(o) => write!(f, "no-go: answered on {o}, not the go page"),
            Refusal::WrongRelyingParty => write!(f, "no-go: the passkey is for another site"),
            Refusal::NoUserPresence => write!(f, "no-go: no user presence"),
            Refusal::NoUserVerification => write!(f, "no-go: no Touch ID (user verification flag off)"),
            Refusal::NonceSpent => write!(f, "no-go: this go was already used"),
            Refusal::CounterWentBack { stored, got } => write!(f, "no-go: signature counter {got} is not past {stored} (cloned key?)"),
            Refusal::BadSignature => write!(f, "no-go: the signature is not Jeff's"),
            Refusal::BadKey => write!(f, "no-go: the stored public key is unreadable"),
        }
    }
}

/// The challenge the go page asks Jeff's passkey to sign. One card, one run,
/// one HEAD, one nonce: a go for anything else does not match.
pub fn challenge(card: u64, run_id: &str, werk_head: &str, nonce: &str) -> String {
    let digest = Sha256::digest(format!("go|{card}|{run_id}|{werk_head}|{nonce}").as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

const FLAG_UP: u8 = 0x01;
const FLAG_UV: u8 = 0x04;

/// Verify one go. Ok(new sign counter) or a typed refusal. Pure.
pub fn verify(a: &Assertion, cred: &Credential, exp: &Expected, spent: &HashSet<String>) -> Result<u32, Refusal> {
    let cd: serde_json::Value = serde_json::from_slice(&a.client_data_json)
        .map_err(|e| Refusal::Malformed(format!("clientDataJSON: {e}")))?;
    let field = |k: &str| cd.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    if field("type") != "webauthn.get" {
        return Err(Refusal::WrongType(field("type")));
    }
    if field("challenge") != challenge(exp.card, &exp.run_id, &exp.werk_head, &exp.nonce) {
        return Err(Refusal::WrongChallenge);
    }
    if field("origin") != exp.origin {
        return Err(Refusal::WrongOrigin(field("origin")));
    }
    let ad = &a.authenticator_data;
    if ad.len() < 37 {
        return Err(Refusal::Malformed("authenticatorData shorter than 37 bytes".into()));
    }
    if ad[..32] != Sha256::digest(exp.rp_id.as_bytes())[..] {
        return Err(Refusal::WrongRelyingParty);
    }
    let flags = ad[32];
    if flags & FLAG_UP == 0 {
        return Err(Refusal::NoUserPresence);
    }
    if flags & FLAG_UV == 0 {
        return Err(Refusal::NoUserVerification);
    }
    let key = VerifyingKey::from_sec1_bytes(&cred.public_key_sec1).map_err(|_| Refusal::BadKey)?;
    let sig = Signature::from_der(&a.signature).map_err(|_| Refusal::BadSignature)?;
    let mut signed = ad.clone();
    signed.extend_from_slice(&Sha256::digest(&a.client_data_json));
    key.verify(&signed, &sig).map_err(|_| Refusal::BadSignature)?;
    // Only a verified go can spend or be refused for a spent nonce.
    if spent.contains(&exp.nonce) {
        return Err(Refusal::NonceSpent);
    }
    let got = u32::from_be_bytes([ad[33], ad[34], ad[35], ad[36]]);
    // Apple passkeys report 0 on every use; a counter, once moving, must move up.
    if !(got == 0 && cred.sign_count == 0) && got <= cred.sign_count {
        return Err(Refusal::CounterWentBack { stored: cred.sign_count, got });
    }
    Ok(got)
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;

    const ORIGIN: &str = "http://192.168.86.242:3000";
    const RP: &str = "192.168.86.242";

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32].into()).expect("fixture key")
    }

    fn cred(k: &SigningKey, count: u32) -> Credential {
        Credential { public_key_sec1: k.verifying_key().to_encoded_point(false).as_bytes().to_vec(), sign_count: count }
    }

    fn exp() -> Expected {
        Expected { card: 4707, run_id: "034ktQIUdeT16HznvkjFIn".into(), werk_head: "6e0a5fee94a5".into(),
                   nonce: "n-1".into(), origin: ORIGIN.into(), rp_id: RP.into() }
    }

    /// What Jeff's authenticator returns after Touch ID, for the go `for_`.
    fn assertion(k: &SigningKey, for_: &Expected, flags: u8, count: u32) -> Assertion {
        let cd = serde_json::json!({"type": "webauthn.get",
            "challenge": challenge(for_.card, &for_.run_id, &for_.werk_head, &for_.nonce),
            "origin": for_.origin, "crossOrigin": false});
        let client_data_json = serde_json::to_vec(&cd).unwrap();
        let mut ad = Sha256::digest(for_.rp_id.as_bytes()).to_vec();
        ad.push(flags);
        ad.extend_from_slice(&count.to_be_bytes());
        let mut signed = ad.clone();
        signed.extend_from_slice(&Sha256::digest(&client_data_json));
        let sig: Signature = k.sign(&signed);
        Assertion { client_data_json, authenticator_data: ad, signature: sig.to_der().as_bytes().to_vec() }
    }

    fn none() -> HashSet<String> { HashSet::new() }

    #[test]
    fn jeffs_go_for_this_card_run_and_head_verifies() {
        let k = key(7);
        assert_eq!(verify(&assertion(&k, &exp(), FLAG_UP | FLAG_UV, 0), &cred(&k, 0), &exp(), &none()), Ok(0));
        assert_eq!(verify(&assertion(&k, &exp(), FLAG_UP | FLAG_UV, 5), &cred(&k, 4), &exp(), &none()), Ok(5));
    }

    #[test]
    fn negative_a_go_for_another_card_is_refused() {
        let k = key(7);
        let other = Expected { card: 4702, ..exp() };
        assert_eq!(verify(&assertion(&k, &other, FLAG_UP | FLAG_UV, 0), &cred(&k, 0), &exp(), &none()), Err(Refusal::WrongChallenge));
    }

    #[test]
    fn negative_a_go_for_another_run_is_refused() {
        let k = key(7);
        let other = Expected { run_id: "034kq648mZbmpjQAkSgxKZ".into(), ..exp() };
        assert_eq!(verify(&assertion(&k, &other, FLAG_UP | FLAG_UV, 0), &cred(&k, 0), &exp(), &none()), Err(Refusal::WrongChallenge));
    }

    #[test]
    fn negative_head_moved_after_the_go_is_refused() {
        let k = key(7);
        let moved = Expected { werk_head: "b08dccd24fc4".into(), ..exp() };
        assert_eq!(verify(&assertion(&k, &exp(), FLAG_UP | FLAG_UV, 0), &cred(&k, 0), &moved, &none()), Err(Refusal::WrongChallenge));
    }

    #[test]
    fn negative_a_replayed_go_is_refused() {
        let k = key(7);
        let spent: HashSet<String> = ["n-1".to_string()].into();
        assert_eq!(verify(&assertion(&k, &exp(), FLAG_UP | FLAG_UV, 0), &cred(&k, 0), &exp(), &spent), Err(Refusal::NonceSpent));
    }

    #[test]
    fn negative_no_touch_id_is_refused() {
        let k = key(7);
        assert_eq!(verify(&assertion(&k, &exp(), FLAG_UP, 0), &cred(&k, 0), &exp(), &none()), Err(Refusal::NoUserVerification));
        assert_eq!(verify(&assertion(&k, &exp(), FLAG_UV, 0), &cred(&k, 0), &exp(), &none()), Err(Refusal::NoUserPresence));
    }

    #[test]
    fn negative_another_key_is_refused() {
        // e.g. a role signing with a key it minted, or the Clearing signer's key
        let (jeff, other) = (key(7), key(9));
        assert_eq!(verify(&assertion(&other, &exp(), FLAG_UP | FLAG_UV, 0), &cred(&jeff, 0), &exp(), &none()), Err(Refusal::BadSignature));
    }

    #[test]
    fn negative_tampered_client_data_is_refused() {
        let k = key(7);
        let mut a = assertion(&k, &exp(), FLAG_UP | FLAG_UV, 0);
        let s = String::from_utf8(a.client_data_json.clone()).unwrap().replace("\"crossOrigin\":false", "\"crossOrigin\":true");
        a.client_data_json = s.into_bytes();
        assert_eq!(verify(&a, &cred(&k, 0), &exp(), &none()), Err(Refusal::BadSignature));
    }

    #[test]
    fn negative_wrong_origin_relying_party_type_and_counter_are_refused() {
        let k = key(7);
        let page = Expected { origin: "http://localhost:3483".into(), ..exp() };
        assert!(matches!(verify(&assertion(&k, &page, FLAG_UP | FLAG_UV, 0), &cred(&k, 0), &exp(), &none()), Err(Refusal::WrongOrigin(_))));
        let site = Expected { rp_id: "localhost".into(), ..exp() };
        assert_eq!(verify(&assertion(&k, &site, FLAG_UP | FLAG_UV, 0), &cred(&k, 0), &exp(), &none()), Err(Refusal::WrongRelyingParty));
        assert_eq!(verify(&assertion(&k, &exp(), FLAG_UP | FLAG_UV, 3), &cred(&k, 3), &exp(), &none()),
                   Err(Refusal::CounterWentBack { stored: 3, got: 3 }));
        let mut create = assertion(&k, &exp(), FLAG_UP | FLAG_UV, 0);
        create.client_data_json = String::from_utf8(create.client_data_json).unwrap().replace("webauthn.get", "webauthn.create").into_bytes();
        assert!(matches!(verify(&create, &cred(&k, 0), &exp(), &none()), Err(Refusal::WrongType(_))));
    }

    #[test]
    fn the_challenge_matches_the_go_page_byte_for_byte() {
        // same vector as directing/clearing-v2/tests/go-passkey-4707.test.ts
        assert_eq!(challenge(4707, "034ktQIUdeT16HznvkjFIn", "6e0a5fee94a5", "n-1"), "oX36ie-6fcYvZgJu7DR8Kc_LIkUg5M9wHK7NoHpjEH0");
    }

    #[test]
    fn the_challenge_binds_every_part() {
        let base = challenge(4707, "r", "h", "n");
        for other in [challenge(4708, "r", "h", "n"), challenge(4707, "r2", "h", "n"), challenge(4707, "r", "h2", "n"), challenge(4707, "r", "h", "n2")] {
            assert_ne!(base, other);
        }
    }
}
