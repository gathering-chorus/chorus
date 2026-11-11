//! #4710 AC3 (Silas 18:19) — the lander's own door login, minted in process. Before this the go
//! page ran scripts/chorus-identity-token, a program jeffbridwell and roles can write; as the
//! lander that would run with the App key readable. Same grant as that script (#3690): OAuth2
//! client_credentials against the local CSS, with the trusted-proxy headers it honours on
//! loopback so it issues as the logical issuer.
//!
//! The credential is chorus-lander's own: ~/.chorus/identity/lander/cred.json, a regular file,
//! 0600, owned by the running account, bound to it (hostAccount). The client secret goes to curl
//! on stdin (`-H @-`), never argv; an empty or foreign-webid answer is refused, never used.
use crate::R;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde_json::Value;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const PRINCIPAL: &str = "lander";

/// ~/.chorus/identity/<principal>/cred.json of the running account.
pub fn cred_path(home: &Path) -> PathBuf {
    home.join(".chorus/identity").join(PRINCIPAL).join("cred.json")
}

fn me() -> (u32, String) {
    extern "C" { fn geteuid() -> u32; }
    let uid = unsafe { geteuid() };
    let name = std::env::var("USER").unwrap_or_default();
    (uid, name)
}

/// The credential, checked: 0600, owned by this account, bound to it. Errors name the path only.
pub fn read_cred(path: &Path, uid: u32, account: &str) -> R<Value> {
    let m = std::fs::metadata(path).map_err(|e| format!("no lander credential at {}: {e}", path.display()))?;
    if !m.is_file() || m.len() == 0 { return Err(format!("{} is empty or not a file", path.display())); }
    if m.mode() & 0o077 != 0 { return Err(format!("{} is mode {:o}; it must be 0600", path.display(), m.mode() & 0o777)); }
    if m.uid() != uid { return Err(format!("{} is owned by uid {}, not this account", path.display(), m.uid())); }
    let c: Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
        .map_err(|_| format!("{} is not JSON", path.display()))?;
    for k in ["id", "secret", "issuer", "webId"] {
        if c.get(k).and_then(Value::as_str).unwrap_or("").is_empty() { return Err(format!("{} has no {k}", path.display())); }
    }
    match c.get("hostAccount").and_then(Value::as_str) {
        Some(h) if h == account => Ok(c),
        Some(h) => Err(format!("{} is bound to account {h}, this process runs as {account}", path.display())),
        None => Err(format!("{} names no hostAccount", path.display())),
    }
}

/// The headers curl reads from stdin: the Basic credential and the proxy headers. Never argv.
pub fn mint_headers(c: &Value) -> String {
    let s = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let host = s("issuer").split("://").nth(1).unwrap_or("").trim_end_matches('/').to_string();
    let basic = STANDARD.encode(format!("{}:{}", s("id"), s("secret")));
    format!("Authorization: Basic {basic}\nHost: {host}\nX-Forwarded-Proto: https\nX-Forwarded-Host: {host}\n")
}

/// curl's argv for the mint: what `ps` shows. No credential.
pub fn mint_argv(css: &str) -> Vec<String> {
    ["-sS", "--max-time", "6", "-X", "POST", "-H", "@-", "-H", "Content-Type: application/x-www-form-urlencoded",
     "--data", "grant_type=client_credentials"].iter().map(|s| s.to_string())
        .chain(std::iter::once(format!("{}/.oidc/token", css.trim_end_matches('/')))).collect()
}

/// The access token from CSS's answer, only if its webid is the credential's.
pub fn token_from(body: &str, web_id: &str) -> R<String> {
    let v: Value = serde_json::from_str(body).map_err(|_| "the identity answer is not JSON".to_string())?;
    let tok = v.get("access_token").and_then(Value::as_str).unwrap_or("").to_string();
    if tok.is_empty() { return Err("the identity answer holds no token".into()); }
    let claims = tok.split('.').nth(1).and_then(|p| URL_SAFE_NO_PAD.decode(p.trim_end_matches('=')).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok()).ok_or("the token is not a JWT")?;
    if claims.get("webid").and_then(Value::as_str) != Some(web_id) {
        return Err("the minted token's webid is not the lander's".into());
    }
    Ok(tok)
}

/// Mint the lander's door login.
pub fn lander_token() -> R<String> {
    let home = std::env::var("HOME").map_err(|_| "no HOME".to_string())?;
    let (uid, account) = me();
    let c = read_cred(&cred_path(Path::new(&home)), uid, &account)?;
    let css = std::env::var("CHORUS_CSS_LOCAL").unwrap_or_else(|_| "http://localhost:3001".into());
    let mut child = Command::new("/usr/bin/curl").args(mint_argv(&css))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|e| format!("identity: curl: {e}"))?;
    child.stdin.take().ok_or("identity: curl stdin")?.write_all(mint_headers(&c).as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    token_from(&String::from_utf8_lossy(&out.stdout), c["webId"].as_str().unwrap_or(""))
}
