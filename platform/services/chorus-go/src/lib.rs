//! #4710 — Jeff's go, signed by his phone's passkey.
//!
//! Before this a go was a dagu human task anyone on this Mac could complete:
//! on 10-24 at 14:43 Borg Queen completed #4717's through dagu's HTTP API with
//! `accepter=jeff` typed by her (bypass 6). Now the go page asks Jeff's phone
//! to sign a challenge bound to one card, one cicd run and one round (the
//! werk's HEAD), the land re-verifies that signature, and a go nobody's phone
//! signed is refused. The private key never leaves his phone, so no process
//! here can make one.
//!
//! What this does NOT close yet (the card's other AC): every Claude role still
//! runs as jeffbridwell, so a role could swap the stored public key or the
//! code that checks it. #4710 AC1/AC3 move the store and the checker into the
//! lander's own account.
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod serve;

pub type R<T> = Result<T, String>;

/// A challenge is good for this long after the page asked for it.
pub const CHALLENGE_TTL_SECS: u64 = 600;
/// A registration link is good for this long after it was made.
pub const REGISTER_TTL_SECS: u64 = 900;

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn b64url(b: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(b)
}

/// Browsers send base64url without padding; tolerate padding and standard base64 too.
pub fn unb64(s: &str) -> R<Vec<u8>> {
    let t = s.trim().trim_end_matches('=');
    URL_SAFE_NO_PAD.decode(t).or_else(|_| STANDARD.decode(s.trim())).map_err(|e| format!("bad base64: {e}"))
}

fn sha256(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

pub fn random_token() -> R<String> {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).map_err(|e| format!("no randomness: {e}"))?;
    Ok(b64url(&b))
}

/// The bytes Jeff's phone signs for a go: one card, one run, one round, one nonce.
pub fn go_challenge(card: u64, run: &str, round: &str, nonce: &str) -> [u8; 32] {
    sha256(format!("chorus-go/v1|go|{card}|{run}|{round}|{nonce}").as_bytes())
}

/// The bytes Jeff's phone signs when it registers its passkey.
pub fn register_challenge(token: &str) -> [u8; 32] {
    sha256(format!("chorus-go/v1|register|{token}").as_bytes())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Passkey {
    pub cred_id: String,
    pub spki: String,
    pub rp_id: String,
    pub origin: String,
    pub sign_count: u32,
}

impl Passkey {
    fn to_json(&self) -> Value {
        json!({"credId": self.cred_id, "spki": self.spki, "rpId": self.rp_id, "origin": self.origin, "signCount": self.sign_count})
    }
    fn from_json(v: &Value) -> R<Self> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string).ok_or(format!("passkey: no {k}"));
        Ok(Passkey {
            cred_id: s("credId")?,
            spki: s("spki")?,
            rp_id: s("rpId")?,
            origin: s("origin")?,
            sign_count: v.get("signCount").and_then(Value::as_u64).unwrap_or(0) as u32,
        })
    }
    pub fn key(&self) -> R<VerifyingKey> {
        VerifyingKey::from_public_key_der(&unb64(&self.spki)?).map_err(|e| format!("passkey: not a P-256 public key: {e}"))
    }
}

/// Check clientDataJSON: its type, its challenge and its origin.
fn check_client_data(client_data_json: &[u8], kind: &str, challenge: &[u8], origin: &str) -> R<()> {
    let cd: Value = serde_json::from_slice(client_data_json).map_err(|e| format!("clientDataJSON: {e}"))?;
    let got = |k: &str| cd.get(k).and_then(Value::as_str).unwrap_or("");
    if got("type") != kind {
        return Err(format!("clientDataJSON type is '{}', not {kind}", got("type")));
    }
    if unb64(got("challenge")).ok().as_deref() != Some(challenge) {
        return Err("the signed challenge is not this card's go".into());
    }
    if got("origin") != origin {
        return Err(format!("signed on '{}', not {origin}", got("origin")));
    }
    Ok(())
}

/// Verify a WebAuthn assertion (ES256): the rpId hash, user present + verified
/// (Face ID / Touch ID), the challenge, the origin, and the signature over
/// authenticatorData || SHA-256(clientDataJSON). Returns the signature counter.
pub fn verify_assertion(pk: &Passkey, challenge: &[u8], auth_data: &[u8], client_data_json: &[u8], sig_der: &[u8]) -> R<u32> {
    if auth_data.len() < 37 {
        return Err("authenticatorData is too short".into());
    }
    if auth_data[..32] != sha256(pk.rp_id.as_bytes()) {
        return Err(format!("signed for another site, not {}", pk.rp_id));
    }
    let flags = auth_data[32];
    if flags & 0x01 == 0 {
        return Err("user not present".into());
    }
    if flags & 0x04 == 0 {
        return Err("user not verified (no Face ID / Touch ID)".into());
    }
    check_client_data(client_data_json, "webauthn.get", challenge, &pk.origin)?;
    let mut msg = auth_data.to_vec();
    msg.extend_from_slice(&sha256(client_data_json));
    let sig = Signature::from_der(sig_der).map_err(|e| format!("signature: {e}"))?;
    pk.key()?.verify(&msg, &sig).map_err(|_| "the signature is not Jeff's passkey".to_string())?;
    Ok(u32::from_be_bytes([auth_data[33], auth_data[34], auth_data[35], auth_data[36]]))
}

#[derive(Clone, Debug, PartialEq)]
pub struct GoRecord {
    pub card: u64,
    pub run: String,
    pub round: String,
    pub nonce: String,
    pub decision: String,
    pub auth_data: String,
    pub client_data: String,
    pub signature: String,
    pub at: u64,
}

impl GoRecord {
    fn to_json(&self) -> Value {
        json!({"card": self.card, "run": self.run, "round": self.round, "nonce": self.nonce, "decision": self.decision,
               "authData": self.auth_data, "clientDataJSON": self.client_data, "signature": self.signature, "at": self.at})
    }
    fn from_json(v: &Value) -> R<Self> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string).ok_or(format!("go record: no {k}"));
        Ok(GoRecord {
            card: v.get("card").and_then(Value::as_u64).ok_or("go record: no card")?,
            run: s("run")?,
            round: s("round")?,
            nonce: s("nonce")?,
            decision: s("decision")?,
            auth_data: s("authData")?,
            client_data: s("clientDataJSON")?,
            signature: s("signature")?,
            at: v.get("at").and_then(Value::as_u64).unwrap_or(0),
        })
    }
}

/// The decision is part of what the phone signs: a "no" can never be replayed as a "go".
pub fn go_challenge_for(rec: &GoRecord) -> [u8; 32] {
    go_challenge(rec.card, &rec.run, &format!("{}|{}", rec.round, rec.decision), &rec.nonce)
}

/// Re-verify a stored go: it must be a go, for this card and this round, signed by the passkey.
pub fn check_record(pk: &Passkey, rec: &GoRecord, card: u64, round: &str) -> R<()> {
    if rec.card != card {
        return Err(format!("the signed go is for #{}, not #{card}", rec.card));
    }
    if rec.round != round {
        return Err(format!("the signed go is for round {}, not this round {}", short(&rec.round), short(round)));
    }
    if rec.decision != "go" {
        return Err(format!("Jeff's signed decision is '{}'", rec.decision));
    }
    verify_assertion(pk, &go_challenge_for(rec), &unb64(&rec.auth_data)?, &unb64(&rec.client_data)?, &unb64(&rec.signature)?)
        .map(|_| ())
}

pub fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

/// #4758 — where the go of a /go link's run waits. The link names the card
/// half's run (cicd); its go waits in the go half, run `go-<run>` on cicd-go
/// (chorus-make GO_RUN_PREFIX), because a go resumed on `werk` queued behind
/// test runs. A link from before #4758 names a cicd run that still holds its
/// own go: when dagu has no go-half run for it, the cicd run is the target.
pub fn go_target(run: &str, go_half_status: &str) -> (&'static str, String) {
    if go_half_status.trim().is_empty() {
        ("cicd", run.to_string())
    } else {
        ("cicd-go", format!("go-{run}"))
    }
}

/// #4753 — whether the go page may open on this run, from the first line of
/// `dagu status cicd --run-id <run>` ("Waiting - <time>"). One go per card:
/// a run cicd-supersede stopped reads Aborted and is refused before Jeff signs.
pub fn go_run_open(status_line: &str) -> Result<(), String> {
    match status_line.split(" - ").next().unwrap_or("").trim() {
        "Waiting" => Ok(()),
        "Aborted" => Err("this run was superseded by a newer run of the card; open the newest go link".into()),
        "" => Err("cannot read this run's state from dagu; nothing to sign".into()),
        s => Err(format!("this run is {}, not waiting at the go: already answered or not there yet", s.to_lowercase())),
    }
}

/// The go store: the passkey, outstanding challenges, and the signed gos.
pub struct Store {
    pub dir: PathBuf,
}

fn write_private(path: &Path, body: &str) -> R<()> {
    use std::os::unix::fs::PermissionsExt;
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(&tmp, body).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", path.display()))
}

fn read_json(path: &Path) -> Option<Value> {
    fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok())
}

fn safe_name(s: &str) -> R<&str> {
    if !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        Ok(s)
    } else {
        Err(format!("not a valid id: '{s}'"))
    }
}

impl Store {
    pub fn default_dir() -> PathBuf {
        std::env::var("CHORUS_GO_DIR").map(PathBuf::from).unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".chorus/go")
        })
    }

    pub fn open(dir: &Path) -> R<Store> {
        use std::os::unix::fs::PermissionsExt;
        for d in [dir.to_path_buf(), dir.join("challenges"), dir.join("records")] {
            fs::create_dir_all(&d).map_err(|e| format!("mkdir {}: {e}", d.display()))?;
            fs::set_permissions(&d, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        }
        Ok(Store { dir: dir.to_path_buf() })
    }

    pub fn passkey(&self) -> R<Option<Passkey>> {
        match read_json(&self.dir.join("passkey.json")) {
            None if !self.dir.join("passkey.json").exists() => Ok(None),
            None => Err("passkey.json is unreadable".into()),
            Some(v) => Passkey::from_json(&v).map(Some),
        }
    }

    pub fn save_passkey(&self, pk: &Passkey) -> R<()> {
        write_private(&self.dir.join("passkey.json"), &pk.to_json().to_string())
    }

    pub fn mint_register_token(&self) -> R<String> {
        let t = random_token()?;
        write_private(&self.dir.join("register-token.json"), &json!({"token": t, "at": now()}).to_string())?;
        Ok(t)
    }

    /// One use, and only before it expires.
    pub fn take_register_token(&self, token: &str) -> R<()> {
        let p = self.dir.join("register-token.json");
        let v = read_json(&p).ok_or("no registration link is open; make one with `chorus-go register-link`")?;
        let _ = fs::remove_file(&p);
        if v.get("token").and_then(Value::as_str) != Some(token) {
            return Err("this registration link is not the open one".into());
        }
        if now().saturating_sub(v.get("at").and_then(Value::as_u64).unwrap_or(0)) > REGISTER_TTL_SECS {
            return Err("this registration link has expired".into());
        }
        Ok(())
    }

    pub fn mint_challenge(&self, card: u64, run: &str, round: &str) -> R<String> {
        safe_name(run)?;
        let nonce = random_token()?;
        write_private(&self.dir.join("challenges").join(format!("{nonce}.json")),
            &json!({"card": card, "run": run, "round": round, "at": now()}).to_string())?;
        Ok(nonce)
    }

    /// One use: the file is removed before the answer is checked.
    pub fn take_challenge(&self, nonce: &str) -> R<(u64, String, String)> {
        let p = self.dir.join("challenges").join(format!("{}.json", safe_name(nonce)?));
        let v = read_json(&p).ok_or("this go was already used, or never asked for")?;
        fs::remove_file(&p).map_err(|e| format!("could not spend the challenge: {e}"))?;
        if now().saturating_sub(v.get("at").and_then(Value::as_u64).unwrap_or(0)) > CHALLENGE_TTL_SECS {
            return Err("this go page is too old; reload it".into());
        }
        let card = v.get("card").and_then(Value::as_u64).ok_or("challenge: no card")?;
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string).unwrap_or_default();
        Ok((card, s("run"), s("round")))
    }

    pub fn save_record(&self, rec: &GoRecord) -> R<()> {
        write_private(&self.dir.join("records").join(format!("{}.json", rec.card)), &rec.to_json().to_string())
    }

    pub fn record(&self, card: u64) -> R<Option<GoRecord>> {
        match read_json(&self.dir.join("records").join(format!("{card}.json"))) {
            None => Ok(None),
            Some(v) => GoRecord::from_json(&v).map(Some),
        }
    }
}

/// Register a passkey from the browser's create() answer. The public key is the
/// SPKI the browser hands back (AuthenticatorAttestationResponse.getPublicKey()).
pub fn register(store: &Store, token: &str, cred_id: &str, spki: &str, client_data_json: &[u8], rp_id: &str, origin: &str) -> R<Passkey> {
    store.take_register_token(token)?;
    check_client_data(client_data_json, "webauthn.create", &register_challenge(token), origin)?;
    let pk = Passkey { cred_id: cred_id.to_string(), spki: spki.to_string(), rp_id: rp_id.to_string(), origin: origin.to_string(), sign_count: 0 };
    pk.key()?;
    store.save_passkey(&pk)?;
    Ok(pk)
}

/// Accept a signed decision from the go page: spend the challenge, verify the
/// signature, keep the counter moving forward, and store the record.
pub fn accept(store: &Store, nonce: &str, decision: &str, auth_data: &str, client_data: &str, signature: &str) -> R<GoRecord> {
    if decision != "go" && decision != "no" {
        return Err(format!("decision is go or no, not '{decision}'"));
    }
    let mut pk = store.passkey()?.ok_or("no passkey is registered yet")?;
    let (card, run, round) = store.take_challenge(nonce)?;
    let rec = GoRecord {
        card, run, round, nonce: nonce.to_string(), decision: decision.to_string(),
        auth_data: auth_data.to_string(), client_data: client_data.to_string(), signature: signature.to_string(), at: now(),
    };
    let count = verify_assertion(&pk, &go_challenge_for(&rec), &unb64(auth_data)?, &unb64(client_data)?, &unb64(signature)?)?;
    if (count != 0 || pk.sign_count != 0) && count <= pk.sign_count {
        return Err("the passkey's counter went backwards (a copied key?)".into());
    }
    pk.sign_count = count;
    store.save_passkey(&pk)?;
    store.save_record(&rec)?;
    Ok(rec)
}

/// What the land asks: is there a signed go from Jeff's passkey for this card's round?
/// With no passkey registered the land keeps today's behaviour and says so.
#[derive(Debug, PartialEq)]
pub enum LandGo {
    Signed,
    NoPasskey,
}

pub fn land_check(store: &Store, card: u64, round: &str) -> R<LandGo> {
    let Some(pk) = store.passkey()? else { return Ok(LandGo::NoPasskey) };
    let rec = store.record(card)?.ok_or(format!("no go signed by Jeff's passkey for #{card}; a go typed by anyone else does not land (#4710)"))?;
    check_record(&pk, &rec, card, round).map_err(|e| format!("{e} (#4710)"))?;
    Ok(LandGo::Signed)
}

/// The commit a role's werk holds (the round a go is for). Abby's and Borg
/// Queen's werks belong to their own Mac accounts, so git refuses them to this
/// service ("dubious ownership") and the go page said "no werk" until Jeff
/// added each werk to his global safe.directory (10-25: #4732, #4736, #4739).
/// git ignores safe.directory given with `-c` (measured: the first cut failed
/// its own test), so this one call reads a global config naming only this
/// werk; Jeff's ~/.gitconfig is not touched.
pub fn werk_head(werk: &str) -> Result<String, String> {
    let cfg = std::env::temp_dir().join(format!("chorus-go-safe-{}-{}", std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    // git compares the resolved path (/var → /private/var on macOS)
    let real = fs::canonicalize(werk).map_err(|e| e.to_string())?;
    fs::write(&cfg, format!("[safe]\n\tdirectory = {}\n", real.display())).map_err(|e| e.to_string())?;
    let o = std::process::Command::new("git")
        .env("GIT_CONFIG_GLOBAL", &cfg)
        .args(["-C", werk, "rev-parse", "HEAD"])
        .output();
    let _ = fs::remove_file(&cfg);
    let o = o.map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}
