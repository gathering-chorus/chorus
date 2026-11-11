//! #4710 AC3 — the lander's door login, minted in process (src/identity.rs), against a stand-in
//! CSS on loopback. Silas 18:19: cred 0600 in its home, the secret to curl on stdin never argv,
//! an empty token refused.
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chorus_go::identity::{self, mint_argv, mint_headers, read_cred, token_from};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;

static ENV: Mutex<()> = Mutex::new(());
const WEBID: &str = "https://id.lightlifeurbangardens.com/lander/profile/card#me";

fn me() -> (u32, String) {
    extern "C" { fn geteuid() -> u32; }
    (unsafe { geteuid() }, std::env::var("USER").unwrap_or_default())
}
fn home(tag: &str, mode: u32, host: &str) -> std::path::PathBuf {
    let h = std::env::temp_dir().join(format!("id4710-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&h);
    let p = identity::cred_path(&h);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, json!({"id": "lander-client", "secret": "SECRET-4710", "issuer": "https://id.lightlifeurbangardens.com/",
        "webId": WEBID, "hostAccount": host}).to_string()).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    h
}
fn jwt(webid: &str) -> String {
    format!("h.{}.s", URL_SAFE_NO_PAD.encode(json!({"webid": webid}).to_string()))
}

#[test]
fn the_cred_must_be_0600_this_accounts_and_bound_to_it() {
    let (uid, acct) = me();
    let ok = home("ok", 0o600, &acct);
    assert!(read_cred(&identity::cred_path(&ok), uid, &acct).is_ok());
    // NEGATIVE PROOFS: open to group, owned by another uid, bound to another account
    let open = home("open", 0o640, &acct);
    assert!(read_cred(&identity::cred_path(&open), uid, &acct).unwrap_err().contains("must be 0600"));
    assert!(read_cred(&identity::cred_path(&ok), uid + 1, &acct).unwrap_err().contains("owned by uid"));
    let other = home("other", 0o600, "chorus-kade");
    assert!(read_cred(&identity::cred_path(&other), uid, &acct).unwrap_err().contains("bound to account chorus-kade"));
}

// NEGATIVE PROOF: the secret never sits on argv; it is only in the stdin headers.
#[test]
fn the_secret_goes_on_stdin_never_argv() {
    let (uid, acct) = me();
    let c = read_cred(&identity::cred_path(&home("argv", 0o600, &acct)), uid, &acct).unwrap();
    let argv = mint_argv("http://localhost:3001").join(" ");
    assert!(!argv.contains("SECRET-4710") && !argv.to_lowercase().contains("authorization"), "{argv}");
    let h = mint_headers(&c);
    assert!(h.starts_with("Authorization: Basic ") && h.contains("Host: id.lightlifeurbangardens.com"), "{h}");
}

// NEGATIVE PROOF: an empty token, a non-JWT, or a token for another webid is refused, never used.
#[test]
fn an_empty_or_foreign_token_is_refused() {
    assert_eq!(token_from(&json!({"access_token": jwt(WEBID)}).to_string(), WEBID), Ok(jwt(WEBID)));
    assert!(token_from(r#"{"access_token":""}"#, WEBID).unwrap_err().contains("no token"));
    assert!(token_from(r#"{"error":"invalid_client"}"#, WEBID).unwrap_err().contains("no token"));
    assert!(token_from(&json!({"access_token": jwt("https://x/kade#me")}).to_string(), WEBID).unwrap_err().contains("not the lander's"));
    assert!(token_from(r#"{"access_token":"opaque"}"#, WEBID).is_err());
}

// End to end against a stand-in CSS: the request carries Basic + proxy headers, the token comes back.
#[test]
fn the_lander_mints_its_login_from_a_stand_in_css() {
    use std::io::{Read, Write};
    let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let (_, acct) = me();
    let h = home("e2e", 0o600, &acct);
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let css = format!("http://{}", l.local_addr().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    let body = json!({"access_token": jwt(WEBID)}).to_string();
    std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut b = [0u8; 65536]; let n = s.read(&mut b).unwrap_or(0);
        let _ = tx.send(String::from_utf8_lossy(&b[..n]).to_string());
        let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    });
    let old = std::env::var_os("HOME");
    std::env::set_var("HOME", &h);
    std::env::set_var("CHORUS_CSS_LOCAL", &css);
    let r = identity::lander_token();
    if let Some(o) = old { std::env::set_var("HOME", o); }
    std::env::remove_var("CHORUS_CSS_LOCAL");
    assert_eq!(r, Ok(jwt(WEBID)));
    let req = rx.recv().unwrap();
    assert!(req.starts_with("POST /.oidc/token"), "{req}");
    assert!(req.contains("Authorization: Basic ") && req.contains("X-Forwarded-Host: id.lightlifeurbangardens.com"), "{req}");
    assert!(req.contains("grant_type=client_credentials"), "{req}");
}
