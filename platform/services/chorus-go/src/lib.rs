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
        for d in [dir.to_path_buf(), dir.join("challenges"), dir.join("records"), dir.join("approvals")] {
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
    /// #4505: an approval challenge lives in its own dir, so a go nonce can never be spent here.
    pub fn mint_approval_challenge(&self, row: &str, action_sha: &str) -> R<String> {
        safe_name(row)?;
        let nonce = random_token()?;
        write_private(&self.dir.join("approvals").join(format!("{nonce}.json")),
            &json!({"row": row, "action_sha": action_sha, "at": now()}).to_string())?;
        Ok(nonce)
    }

    /// One use: the file is removed before the answer is checked.
    pub fn take_approval_challenge(&self, nonce: &str) -> R<(String, String)> {
        let p = self.dir.join("approvals").join(format!("{}.json", safe_name(nonce)?));
        let v = read_json(&p).ok_or("this approval was already used, or never asked for")?;
        fs::remove_file(&p).map_err(|e| format!("could not spend the challenge: {e}"))?;
        if now().saturating_sub(v.get("at").and_then(Value::as_u64).unwrap_or(0)) > CHALLENGE_TTL_SECS {
            return Err("this approvals page is too old; reload it".into());
        }
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string).unwrap_or_default();
        Ok((s("row"), s("action_sha")))
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

// ---- #4505: approvals — one row Jeff decides on, signed by his passkey ----

/// The bytes Jeff's phone signs for an approval: one row, one exact action, one decision, one nonce.
pub fn approval_challenge(row: &str, action_sha: &str, decision: &str, nonce: &str) -> [u8; 32] {
    sha256(format!("chorus-go/v1|approval|{row}|{action_sha}|{decision}|{nonce}").as_bytes())
}

/// The action's fingerprint: what the challenge binds, so an edited row can't ride an old signature.
pub fn action_sha(action: &str) -> String {
    b64url(&sha256(action.as_bytes()))
}

/// The only flags an approved ask may carry. Each takes one value. Anything else is refused,
/// above all `--desc-file` (it would read any file Jeff can read into a card) and `--desc -` (stdin).
pub const CARDS_ADD_FLAGS: &[&str] = &["--owner", "--priority", "--domain", "--type", "--origin", "--desc", "--sequence"];

/// An approval's action must be a `cards add` argv: a JSON array of strings, "add", a title,
/// then allow-listed `--flag value` pairs. It is replayed as data (no shell), so anything else
/// is refused here, at post and at replay.
pub fn cards_add_argv(action: &str) -> R<Vec<String>> {
    let v: Value = serde_json::from_str(action).map_err(|_| "not a cards add argv: not JSON".to_string())?;
    let arr = v.as_array().ok_or("not a cards add argv: not an array")?;
    let mut out = Vec::with_capacity(arr.len());
    for a in arr {
        let s = a.as_str().ok_or("not a cards add argv: every element is a string")?;
        if s.contains('\0') {
            return Err("not a cards add argv: NUL in an argument".into());
        }
        out.push(s.to_string());
    }
    if out.len() < 2 || out[0] != "add" || out[1].is_empty() || out[1].starts_with('-') {
        return Err("not a cards add argv: it must start with \"add\" and a title".into());
    }
    let mut i = 2;
    while i < out.len() {
        let flag = out[i].as_str();
        if !CARDS_ADD_FLAGS.contains(&flag) {
            return Err(format!("not a cards add argv: flag '{flag}' is not allowed"));
        }
        let val = out.get(i + 1).ok_or_else(|| format!("not a cards add argv: '{flag}' has no value"))?;
        if val == "-" || val.starts_with("--") {
            return Err(format!("not a cards add argv: '{flag}' takes a value, not '{val}'"));
        }
        i += 2;
    }
    Ok(out)
}

/// A row as the door serves it, the fields a decision reads.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalRow {
    pub id: String,
    pub state: String,
    pub action: String,
    /// #4642: "card" or "go"; it picks the parser, and the action must match its shape.
    pub kind: String,
    /// #4642: for a go row, the werk's HEAD now (the server reads it); a go for another round is refused.
    pub round_now: Option<String>,
}

/// #4642 — what a go row asks for: one card's cicd run, at one round (the werk HEAD the demo showed).
#[derive(Clone, Debug, PartialEq)]
pub struct GoAction {
    pub card: u64,
    pub run: String,
    pub round: String,
}

/// A go row's action is a JSON object {card, run, round} and nothing else. A cards-add argv
/// (an array) is refused here, as a go object is refused by cards_add_argv: they never cross-replay.
pub fn go_action(action: &str) -> R<GoAction> {
    let v: Value = serde_json::from_str(action).map_err(|_| "not a go action: not JSON".to_string())?;
    let o = v.as_object().ok_or("not a go action: not an object")?;
    if o.len() != 3 {
        return Err("not a go action: it carries exactly card, run and round".into());
    }
    let card = o.get("card").and_then(Value::as_u64).filter(|c| *c > 0).ok_or("not a go action: no card")?;
    let run = o.get("run").and_then(Value::as_str).ok_or("not a go action: no run")?;
    safe_name(run).map_err(|_| "not a go action: the run is not an id".to_string())?;
    let round = o.get("round").and_then(Value::as_str).ok_or("not a go action: no round")?;
    // The full 40-hex commit: a prefix against the land's full HEAD would be a false refusal or an
    // ambiguous match (Silas, f7f580465 review).
    if round.len() != 40 || !round.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err("not a go action: the round is not a full commit".into());
    }
    Ok(GoAction { card, run: run.to_string(), round: round.to_string() })
}

/// What a verified decision hands the server to record (and, on approve, replay).
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalDecision {
    pub row: String,
    pub decision: String,
    pub argv: Vec<String>,
    pub at: u64,
    /// #4642: on a go row, what was signed (the land re-verifies it).
    pub go: Option<GoAction>,
    /// #4642: the nonce the phone signed over, kept on the decision row so the land can re-verify.
    pub nonce: String,
}

/// Jeff decides one row. Order matters: spend the nonce, check the row is still the one
/// he was shown (same id, still pending, same action), check the action, then the signature.
pub fn decide_approval(store: &Store, nonce: &str, decision: &str, row_now: &ApprovalRow,
                       auth_data: &str, client_data: &str, signature: &str) -> R<ApprovalDecision> {
    if decision != "approve" && decision != "decline" {
        return Err(format!("decision is approve or decline, not '{decision}'"));
    }
    let mut pk = store.passkey()?.ok_or("no passkey is registered yet")?;
    let (row, sha) = store.take_approval_challenge(nonce)?;
    if row != row_now.id {
        return Err("this approval was asked for another row".into());
    }
    if row_now.state != "pending" {
        return Err(format!("this row was already decided ({})", row_now.state));
    }
    if action_sha(&row_now.action) != sha {
        return Err("the row's action changed after the page was shown; reload it".into());
    }
    // #4642: the kind picks the parser; the action must have that kind's shape.
    let (argv, go) = match row_now.kind.as_str() {
        // A decline replays nothing, so a junk card row can still be cleared.
        "card" => (if decision == "approve" { cards_add_argv(&row_now.action)? } else { Vec::new() }, None),
        "go" => {
            let g = go_action(&row_now.action)?;
            if row_now.round_now.as_deref() != Some(g.round.as_str()) {
                return Err(format!("this go is for round {}, but the werk is at {}; the demo moved on",
                    short(&g.round), row_now.round_now.as_deref().map(short).unwrap_or("nothing")));
            }
            (Vec::new(), Some(g))
        }
        k => return Err(format!("an approval of kind '{k}' cannot be decided here")),
    };
    let ch = approval_challenge(&row, &sha, decision, nonce);
    let count = verify_assertion(&pk, &ch, &unb64(auth_data)?, &unb64(client_data)?, &unb64(signature)?)?;
    if (count != 0 || pk.sign_count != 0) && count <= pk.sign_count {
        return Err("the passkey's counter went backwards (a copied key?)".into());
    }
    pk.sign_count = count;
    store.save_passkey(&pk)?;
    let d = ApprovalDecision { row, decision: decision.to_string(), argv, at: now(), go, nonce: nonce.to_string() };
    // The proof, kept like a go record: what Jeff's phone signed, for this row.
    write_private(&store.dir.join("approvals").join(format!("decided-{}.json", d.row)), &json!({
        "row": d.row, "decision": d.decision, "action_sha": sha, "nonce": nonce, "at": d.at,
        "authData": auth_data, "clientDataJSON": client_data, "signature": signature}).to_string())?;
    Ok(d)
}

/// #4642 — the land's check of a go decision row: Jeff's passkey signed an approve of this row's
/// exact action, and that action is this card at this round. A jeff-owned row alone is not a go
/// (Kade 18:41): the signature is re-verified here, every land.
///
/// `action` is the approvalAction of the Approval row the decision `decides`, read from that row,
/// never from the decision row: a decision cannot name what it approved (Silas, f7f580465 review).
pub fn check_go_decision(pk: &Passkey, row: &str, action: &str, verdict: &str, proof: &Value, card: u64, round: &str) -> R<()> {
    if verdict != "approve" {
        return Err(format!("the go for #{card} was declined"));
    }
    let g = go_action(action)?;
    if g.card != card {
        return Err(format!("that go was for #{}, not #{card}", g.card));
    }
    if g.round != round {
        return Err(format!("the go for #{card} was signed for round {}, not {}", short(&g.round), short(round)));
    }
    let f = |k: &str| proof.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let nonce = f("nonce");
    if nonce.is_empty() {
        return Err("the go decision carries no signature to check".into());
    }
    let ch = approval_challenge(row, &action_sha(action), verdict, &nonce);
    verify_assertion(pk, &ch, &unb64(&f("authData"))?, &unb64(&f("clientDataJSON"))?, &unb64(&f("signature"))?)
        .map(|_| ())
        .map_err(|e| format!("the go for #{card} is not signed by Jeff's passkey: {e}"))
}
