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
pub mod github;

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

/// #4771 AC2 — `dagu status` (the whole answer) says this go run was superseded: stopped
/// (Aborted), or answered no by cicd-supersede (Succeeded with its land step skipped). A go
/// Jeff signed also reads Succeeded, but its land ran — that is never called superseded.
pub fn superseded_status(status: &str) -> bool {
    match status.split(" - ").next().unwrap_or("").trim() {
        "Aborted" => true,
        "Succeeded" => status.lines().any(|l| l.contains("cicd-land") && l.contains("[skipped]")),
        _ => false,
    }
}

/// #4771 — when a dagu run started, in ms since the epoch, read from its id: dagu run ids
/// are UUIDv7 in base62 (22 chars), whose top 48 bits are the enqueue instant. `go-<id>`
/// (the go half) reads its cicd run's id. None when it is not such an id. Pure.
pub fn run_started_ms(run: &str) -> Option<u64> {
    const A: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let id = run.strip_prefix("go-").unwrap_or(run);
    if id.len() != 22 { return None; }
    let mut n: u128 = 0;
    for c in id.chars() {
        n = n.checked_mul(62)?.checked_add(A.find(c)? as u128)?;
    }
    if n >> 122 != 0 { return None; }
    Some((n >> 80) as u64)
}

/// #4771 AC2 — one go per card on Jeff's list: of a card's go rows only the one whose
/// cicd run STARTED last is kept (Silas 17:36: not when its row posted — a slow older run
/// posts late). Cards' asks, other cards' gos and a go whose run id is unreadable stay. A
/// run that reached its go after a newer run started is still waiting in dagu, so the list,
/// not dagu, says which go is the card's. Pure.
pub fn newest_go_per_card(rows: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let s = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let key = |v: &serde_json::Value| go_action(&s(v, "action")).ok()
        .and_then(|g| run_started_ms(&g.run).map(|t| (g.card, t)));
    let mut newest: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    for (c, t) in rows.iter().filter(|r| s(r, "kind") == "go").filter_map(key) {
        let e = newest.entry(c).or_insert(t);
        if t > *e { *e = t; }
    }
    rows.into_iter().filter(|r| {
        if s(r, "kind") != "go" { return true; }
        match key(r) { Some((c, t)) => newest.get(&c).map(|n| *n == t).unwrap_or(true), None => true }
    }).collect()
}

/// #4771 — read curl's `-w "\n%{http_code}"` answer to the decision POST: Ok on a 2xx, else an
/// error naming the code and the door's own message (first 300 chars), never a guess. Pure.
pub fn decision_post_result(ran: bool, stdout: &str, stderr: &str) -> Result<(), String> {
    let (body, code) = stdout.trim_end().rsplit_once('\n').unwrap_or(("", stdout.trim()));
    if ran && code.starts_with('2') { return Ok(()); }
    let why: String = if body.trim().is_empty() { stderr.trim() } else { body.trim() }.chars().take(300).collect();
    let code = if code.is_empty() { "no answer" } else { code };
    Err(format!("signed, but the door refused Jeff's decision row ({code}): {why}"))
}

/// #4771 AC2 — the refusal for a go whose run was superseded: it names the
/// card's newest go to open instead, or says to wait for the new demo.
pub fn stale_go_refusal(card: u64, why: &str, newest_row: Option<&str>) -> String {
    match newest_row {
        Some(r) => format!("#{card}: {why}: open the newest go ({r})"),
        None => format!("#{card}: {why}: its new run has not reached the go yet; wait for that demo"),
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
        for d in [dir.to_path_buf(), dir.join("approvals")] {
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

/// What the land asks: is there a signed go from Jeff's passkey for this card's round?
/// With no passkey registered the land keeps today's behaviour and says so.
#[derive(Debug, PartialEq)]
pub enum LandGo {
    Signed,
    NoPasskey,
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
    let o = std::process::Command::new("/usr/bin/git")
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
    /// #4885: on a step row, the run's human step Jeff answered.
    pub step: Option<StepAction>,
}

/// #4885 — what a step row asks for: Jeff's answer to one human step of one dagu run (the domain
/// flow's bar). The action is exactly {card, dag, run, step}, so the signature binds that run's
/// step and nothing else (Kade 11-04 04:03); a go object (3 keys) or a cards argv is refused here.
#[derive(Clone, Debug, PartialEq)]
pub struct StepAction {
    pub card: u64,
    pub dag: String,
    pub run: String,
    pub step: String,
}

pub fn step_action(action: &str) -> R<StepAction> {
    let v: Value = serde_json::from_str(action).map_err(|_| "not a step action: not JSON".to_string())?;
    let o = v.as_object().ok_or("not a step action: not an object")?;
    if o.len() != 4 {
        return Err("not a step action: it carries exactly card, dag, run and step".into());
    }
    let card = o.get("card").and_then(Value::as_u64).filter(|c| *c > 0).ok_or("not a step action: no card")?;
    let id = |k: &str| -> R<String> {
        let x = o.get(k).and_then(Value::as_str).ok_or_else(|| format!("not a step action: no {k}"))?;
        safe_name(x).map(str::to_string).map_err(|_| format!("not a step action: the {k} is not an id"))
    };
    Ok(StepAction { card, dag: id("dag")?, run: id("run")?, step: id("step")? })
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
    // #4642: this server already decided the row (its signed proof is on disk): never twice, even
    // if the door has not shown the decision yet or the card filing failed after it.
    if store.dir.join("approvals").join(format!("decided-{row}.json")).exists() {
        return Err("this row was already decided here".into());
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
        // #4885: a step answer replays nothing itself; the server answers the step after this check
        "step" => { step_action(&row_now.action)?; (Vec::new(), None) }
        k => return Err(format!("an approval of kind '{k}' cannot be decided here")),
    };
    let step = if row_now.kind == "step" { Some(step_action(&row_now.action)?) } else { None };
    let ch = approval_challenge(&row, &sha, decision, nonce);
    let count = verify_assertion(&pk, &ch, &unb64(auth_data)?, &unb64(client_data)?, &unb64(signature)?)?;
    if (count != 0 || pk.sign_count != 0) && count <= pk.sign_count {
        return Err("the passkey's counter went backwards (a copied key?)".into());
    }
    pk.sign_count = count;
    store.save_passkey(&pk)?;
    let d = ApprovalDecision { row, decision: decision.to_string(), argv, at: now(), go, nonce: nonce.to_string(), step };
    // The proof, kept like a go record: what Jeff's phone signed, for this row.
    // Written BEFORE the door POST (Silas 19:27): the break-glass record the land falls back to
    // when the door has no row (a failed mint). It carries the action, so it re-verifies alone.
    write_private(&store.dir.join("approvals").join(format!("decided-{}.json", d.row)), &json!({
        "row": d.row, "decision": d.decision, "kind": row_now.kind, "action": row_now.action, "action_sha": sha,
        "nonce": nonce, "at": d.at, "authData": auth_data, "clientDataJSON": client_data, "signature": signature}).to_string())?;
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

/// #4642 — the land's one go path: from the door's rows, Jeff's go for this card at this round.
/// `approvals` and `decisions` are the door's /decisions/approvals and /approvaldecisions rows.
/// The action is read from the Approval row the decision decides, never from the decision; only a
/// decision owned by principal-jeff counts; the signature is re-verified (check_go_decision).
pub fn land_go_from_rows(pk: &Passkey, approvals: &[Value], decisions: &[Value], card: u64, round: &str) -> R<()> {
    let s = |v: &Value, k: &str| -> String {
        match v.get(k) {
            Some(Value::String(x)) => x.clone(),
            Some(Value::Object(o)) => o.get("@id").or_else(|| o.get("id")).and_then(Value::as_str).unwrap_or("").to_string(),
            _ => String::new(),
        }
    };
    let local = |x: &str| x.rsplit(|c| c == '#' || c == '/' || c == ':').next().unwrap_or("").to_string();
    let mut last = format!("no go signed by Jeff's passkey for #{card} at round {}; a go typed by anyone else does not land (#4710)", short(round));
    for a in approvals.iter().filter(|a| s(a, "approvalKind") == "go") {
        let action = s(a, "approvalAction");
        let Ok(g) = go_action(&action) else { continue };
        if g.card != card || g.round != round { continue; }
        let row = local(&s(a, "name"));
        for d in decisions.iter().filter(|d| local(&s(d, "decides")) == row) {
            // #4710 — the lander writes Jeff's decision row as itself (principal-lander); the
            // passkey signature below is the authority either way, re-verified every time, and an
            // owner alone never passes. No other principal's row decides anything.
            let owner = local(&s(d, "ownedBy"));
            if owner != "principal-jeff" && owner != "principal-lander" {
                last = format!("a go decision for #{card} owned by {owner}, not principal-jeff or principal-lander, decides nothing");
                continue;
            }
            let proof: Value = serde_json::from_str(&s(d, "passkeyProof")).unwrap_or(Value::Null);
            match check_go_decision(pk, &row, &action, &s(d, "approvalVerdict"), &proof, card, round) {
                Ok(()) => return Ok(()),
                Err(e) => last = e,
            }
        }
    }
    Err(last)
}

/// #4642 break-glass — Jeff's go from chorus-go's own 0700 store, for when the door has no row
/// (his identity could not be minted to write it). The passkey re-verify is the authority here
/// exactly as on the door path: a forged, edited or foreign record is refused.
pub fn land_go_from_local(pk: &Passkey, store: &Store, card: u64, round: &str) -> R<()> {
    let mut last = format!("no locally recorded go for #{card} at round {}", short(round));
    let Ok(rd) = fs::read_dir(store.dir.join("approvals")) else { return Err(last) };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with("decided-") || !name.ends_with(".json") { continue; }
        let Some(v) = read_json(&e.path()) else { continue };
        let f = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        if f("kind") != "go" { continue; }
        let action = f("action");
        let Ok(g) = go_action(&action) else { continue };
        if g.card != card || g.round != round { continue; }
        if action_sha(&action) != f("action_sha") {
            last = "a local go record whose action does not match what was signed".into();
            continue;
        }
        match check_go_decision(pk, &f("row"), &action, &f("decision"), &v, card, round) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// #4885 — a pipeline's human step, answered only from Jeff's passkey: from the door's rows, his
/// signed approve of a step row naming exactly this dag, run and step. Like the land's go check: the
/// action is read from the Approval row, only a principal-jeff decision counts, and the signature is
/// re-verified here. A role that typed `by=jeff` into dagu left no such row, so it is refused.
pub fn land_step_from_rows(pk: &Passkey, approvals: &[Value], decisions: &[Value], dag: &str, run: &str, step: &str) -> R<()> {
    let s = |v: &Value, k: &str| -> String {
        match v.get(k) {
            Some(Value::String(x)) => x.clone(),
            Some(Value::Object(o)) => o.get("@id").or_else(|| o.get("id")).and_then(Value::as_str).unwrap_or("").to_string(),
            _ => String::new(),
        }
    };
    let local = |x: &str| x.rsplit(|c| c == '#' || c == '/' || c == ':').next().unwrap_or("").to_string();
    let mut last = format!("{dag} run {run} step {step} is not signed by Jeff's passkey; an answer typed by anyone else counts for nothing");
    for a in approvals.iter().filter(|a| s(a, "approvalKind") == "step") {
        let action = s(a, "approvalAction");
        let Ok(st) = step_action(&action) else { continue };
        if st.dag != dag || st.run != run || st.step != step { continue; }
        let row = local(&s(a, "name"));
        for d in decisions.iter().filter(|d| local(&s(d, "decides")) == row) {
            if local(&s(d, "ownedBy")) != "principal-jeff" {
                last = format!("a step decision not owned by principal-jeff decides nothing ({run} {step})");
                continue;
            }
            let verdict = s(d, "approvalVerdict");
            if verdict != "approve" {
                last = format!("Jeff declined {dag} run {run} step {step}");
                continue;
            }
            let proof: Value = serde_json::from_str(&s(d, "passkeyProof")).unwrap_or(Value::Null);
            let f = |k: &str| proof.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            if f("nonce").is_empty() {
                last = "the step decision carries no signature to check".into();
                continue;
            }
            let ch = approval_challenge(&row, &action_sha(&action), &verdict, &f("nonce"));
            match verify_assertion(pk, &ch, &unb64(&f("authData"))?, &unb64(&f("clientDataJSON"))?, &unb64(&f("signature"))?) {
                Ok(_) => return Ok(()),
                Err(e) => last = format!("{dag} run {run} step {step} is not signed by Jeff's passkey: {e}"),
            }
        }
    }
    Err(last)
}
