// @domain: cicd — placed by chorus-crawl's domain rules (#4538)
// #4710 AC1/AC2 — the land's GitHub credential becomes a GitHub App's, held by the lander.
//
// Borg Queen's bypass 1 (10-23): every role could reach Jeff's admin GitHub login, so a role
// could lift branch protection or post the jeff-go status itself. #4771 moved the land onto
// one file, ~/.chorus/secrets/github-deploy, of the account the land runs as. That account is
// still Jeff's, and a role's werk code runs there during a land. The fix (Silas + Wren
// 11-11 17:15): the lander account alone holds a GitHub App's private key; at each land it
// signs a 10-minute JWT and trades it for a 1-hour installation credential, which then rides
// the same git/gh seam as before (deploy_credential.rs's CRED_FILE_ENV). github-deploy is never
// read here: no App key is a refusal, never a fallback (Jeff: no fallbacks).
//
// The App directory, ~/.chorus/secrets/github-app/ of the running account:
//   app-id            the App's numeric id (line 1)
//   installation-id   its installation on gathering-chorus/chorus (line 1)
//   key.pem           the App's private key, as GitHub gave it
// Each file 0600 and owned by the running account (check_credential_file).
//
// BOUNDARY (shared/, #3513): pure, dep-free, single-file include!, alongside
// deploy_credential.rs (which it needs). Signing shells to `openssl`, the HTTPS call to `curl`.

pub const APP_DIR_REL: &str = ".chorus/secrets/github-app";
pub const APP_DIR_ENV: &str = "WERK_GITHUB_APP_DIR";
pub const GITHUB_API_ENV: &str = "WERK_GITHUB_API";

/// The App directory: $WERK_GITHUB_APP_DIR, else $HOME/.chorus/secrets/github-app.
pub fn app_dir() -> Result<std::path::PathBuf, String> {
    if let Some(p) = std::env::var_os(APP_DIR_ENV).filter(|p| !p.is_empty()) {
        return Ok(std::path::PathBuf::from(p));
    }
    std::env::var_os("HOME").filter(|h| !h.is_empty())
        .map(|h| std::path::PathBuf::from(h).join(APP_DIR_REL))
        .ok_or_else(|| format!("auth-fail: no HOME to find ~/{} in", APP_DIR_REL))
}

/// base64url, no padding (RFC 7515 §2).
pub fn b64url(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        if c.len() > 1 { out.push(T[(n >> 6 & 63) as usize] as char); }
        if c.len() > 2 { out.push(T[(n & 63) as usize] as char); }
    }
    out
}

/// The unsigned JWT GitHub asks of an App: issued a minute back (clock drift), good for nine.
pub fn app_jwt_unsigned(app_id: &str, now: u64) -> String {
    let header = b64url(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = b64url(format!("{{\"iat\":{},\"exp\":{},\"iss\":\"{}\"}}", now - 60, now + 540, app_id).as_bytes());
    format!("{header}.{claims}")
}

/// RS256 over `signing_input` with the key at `key`, by `openssl dgst -sha256 -sign`.
/// Errors name the key's path, never its contents.
pub fn rs256(signing_input: &str, key: &std::path::Path) -> Result<String, String> {
    use std::io::Write;
    let mut child = std::process::Command::new("/usr/bin/openssl")
        .args(["dgst", "-sha256", "-sign"]).arg(key)
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| format!("auth-fail: openssl did not start: {e}"))?;
    child.stdin.take().ok_or("auth-fail: openssl stdin")?.write_all(signing_input.as_bytes())
        .map_err(|e| format!("auth-fail: openssl stdin: {e}"))?;
    let out = child.wait_with_output().map_err(|e| format!("auth-fail: openssl: {e}"))?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(format!("auth-fail: openssl could not sign with the App key {}", key.display()));
    }
    Ok(b64url(&out.stdout))
}

fn first_line(p: &std::path::Path) -> Result<String, String> {
    check_credential_file(p, current_uid())?;
    let t = std::fs::read_to_string(p).map_err(|e| format!("auth-fail: {} unreadable: {}", p.display(), e))?;
    let l = credential_line(&t).to_string();
    if l.is_empty() || !l.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("auth-fail: {} must hold a number on line 1", p.display()));
    }
    Ok(l)
}

/// The App's id, installation id and checked key path, or the refusal.
/// No App directory is `auth-fail: no GitHub App key` — never a fallback to github-deploy.
pub fn app_parts(dir: &std::path::Path) -> Result<(String, String, std::path::PathBuf), String> {
    if !dir.is_dir() {
        return Err(format!("auth-fail: no GitHub App key: {} does not exist; only the lander holds it (#4710)", dir.display()));
    }
    let key = dir.join("key.pem");
    check_credential_file(&key, current_uid())?;
    Ok((first_line(&dir.join("app-id"))?, first_line(&dir.join("installation-id"))?, key))
}

/// The value of `"token":"…"` in GitHub's access_tokens answer.
pub fn token_from_answer(body: &str) -> Option<String> {
    let i = body.find("\"token\"")?;
    let rest = &body[i + 7..];
    let rest = &rest[rest.find('"')? + 1..];
    let t = &rest[..rest.find('"')?];
    (!t.is_empty()).then(|| t.to_string())
}

/// curl's argv for the access-token exchange: what `ps` shows. Holds no credential: the JWT
/// header is read from stdin (`-H @-`).
pub fn token_request_argv(api: &str, inst: &str) -> Vec<String> {
    ["-sS", "--max-time", "20", "-X", "POST", "-H", "@-",
     "-H", "Accept: application/vnd.github+json", "-H", "X-GitHub-Api-Version: 2022-11-28"]
        .iter().map(|s| s.to_string())
        .chain(std::iter::once(format!("{api}/app/installations/{inst}/access_tokens")))
        .collect()
}

/// Sign, trade the JWT for an installation credential, write it 0600 to `<dir>/installation`,
/// and arm it for every later git/gh call of this process (deploy_credential.rs's seam).
pub fn arm_app_credential() -> Result<std::path::PathBuf, String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let dir = app_dir()?;
    let (app_id, inst, key) = app_parts(&dir)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    let unsigned = app_jwt_unsigned(&app_id, now);
    let jwt = format!("{unsigned}.{}", rs256(&unsigned, &key)?);
    let api = std::env::var(GITHUB_API_ENV).unwrap_or_else(|_| "https://api.github.com".into());
    // the JWT goes to curl on stdin as a header file (-H @-), never on argv (token_request_argv)
    let mut child = std::process::Command::new("/usr/bin/curl")
        .args(token_request_argv(&api, &inst))
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| format!("auth-fail: curl did not start: {e}"))?;
    child.stdin.take().ok_or("auth-fail: curl stdin")?
        .write_all(format!("Authorization: {} {jwt}\n", "Bearer").as_bytes())
        .map_err(|e| format!("auth-fail: curl stdin: {e}"))?;
    let out = child.wait_with_output().map_err(|e| format!("auth-fail: curl: {e}"))?;
    let tok = token_from_answer(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| format!("auth-fail: GitHub gave the App (id {app_id}, installation {inst}) no installation credential"))?;
    let path = dir.join("installation");
    let _ = std::fs::remove_file(&path);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)
        .map_err(|e| format!("auth-fail: {} not writable: {}", path.display(), e))?;
    f.write_all(format!("{tok}\n").as_bytes()).map_err(|e| format!("auth-fail: {}: {}", path.display(), e))?;
    std::env::set_var(CRED_FILE_ENV, &path);
    Ok(path)
}

#[cfg(test)]
mod app_credential_4710 {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ac4710-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn put(p: &std::path::Path, body: &str) {
        std::fs::write(p, body).unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn b64url_decode(s: &str) -> Vec<u8> {
        let v = |c: u8| -> u32 { match c { b'A'..=b'Z' => (c - b'A') as u32, b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32, b'-' => 62, _ => 63 } };
        let b = s.as_bytes(); let mut out = Vec::new();
        for c in b.chunks(4) {
            let n = c.iter().enumerate().fold(0u32, |a, (i, &x)| a | v(x) << (18 - 6 * i));
            out.push((n >> 16) as u8);
            if c.len() > 2 { out.push((n >> 8) as u8); }
            if c.len() > 3 { out.push(n as u8); }
        }
        out
    }

    #[test]
    fn b64url_matches_the_rfc_alphabet_without_padding() {
        assert_eq!(b64url(b"f"), "Zg");
        assert_eq!(b64url(b"fo"), "Zm8");
        assert_eq!(b64url(b"foo"), "Zm9v");
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
    }

    // NEGATIVE PROOF (Silas 17:16): with no App key the land refuses, and a github-deploy
    // planted in the same HOME is never read, never armed: no fallback.
    #[test]
    fn no_app_key_refuses_and_never_reads_github_deploy() {
        let _g = CRED_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let home = tmp("nokey");
        std::fs::create_dir_all(home.join(".chorus/secrets")).unwrap();
        let planted = home.join(DEPLOY_CREDENTIAL_REL);
        put(&planted, "PLANTED-DEPLOY\n");
        std::env::remove_var(CRED_FILE_ENV);
        std::env::remove_var(APP_DIR_ENV);
        let old = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);
        let r = arm_app_credential();
        match old { Some(h) => std::env::set_var("HOME", h), None => std::env::remove_var("HOME") }
        let e = r.unwrap_err();
        assert!(e.contains("no GitHub App key"), "{e}");
        assert!(!e.contains("github-deploy") && !e.contains("PLANTED"), "{e}");
        assert!(armed_credential().is_none(), "something was armed with no App key");
    }

    #[test]
    fn a_key_open_to_group_or_a_non_number_id_is_refused_by_path() {
        let d = tmp("bad");
        put(&d.join("key.pem"), "k"); put(&d.join("app-id"), "123\n"); put(&d.join("installation-id"), "456\n");
        assert!(app_parts(&d).is_ok());
        std::fs::set_permissions(d.join("key.pem"), std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(app_parts(&d).unwrap_err().contains("must be 0600"));
        std::fs::set_permissions(d.join("key.pem"), std::fs::Permissions::from_mode(0o600)).unwrap();
        put(&d.join("app-id"), "abc\n");
        assert!(app_parts(&d).unwrap_err().contains("must hold a number"));
    }

    // The JWT GitHub would get verifies against the key's public half, and says who and when.
    #[test]
    fn the_jwt_is_rs256_over_header_and_claims_and_verifies() {
        let d = tmp("jwt");
        let key = d.join("key.pem");
        let ok = std::process::Command::new("/usr/bin/openssl").args(["genrsa", "-out"]).arg(&key).arg("2048")
            .stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
        if !ok { eprintln!("unmeasured: no openssl genrsa here"); return; }
        let unsigned = app_jwt_unsigned("123456", 1_800_000_000);
        let sig = rs256(&unsigned, &key).unwrap();
        let claims = String::from_utf8(b64url_decode(unsigned.split('.').nth(1).unwrap())).unwrap();
        assert_eq!(claims, r#"{"iat":1799999940,"exp":1800000540,"iss":"123456"}"#);
        let pubk = d.join("pub.pem");
        assert!(std::process::Command::new("/usr/bin/openssl").args(["rsa", "-pubout", "-in"]).arg(&key).arg("-out").arg(&pubk)
            .stderr(std::process::Stdio::null()).status().unwrap().success());
        let sigf = d.join("sig"); std::fs::write(&sigf, b64url_decode(&sig)).unwrap();
        let datf = d.join("dat"); std::fs::write(&datf, &unsigned).unwrap();
        let verify = |data: &std::path::Path| std::process::Command::new("/usr/bin/openssl")
            .args(["dgst", "-sha256", "-verify"]).arg(&pubk).arg("-signature").arg(&sigf).arg(data)
            .stdout(std::process::Stdio::null()).status().unwrap().success();
        assert!(verify(&datf), "signature does not verify");
        // NEGATIVE PROOF: the same signature over other claims does not verify
        let other = d.join("other"); std::fs::write(&other, app_jwt_unsigned("999", 1_800_000_000)).unwrap();
        assert!(!verify(&other), "a signature verified over claims it did not sign");
    }

    #[test]
    fn the_installation_credential_is_read_from_githubs_answer() {
        assert_eq!(token_from_answer(r#"{"token": "ghs_abc", "expires_at":"x"}"#).as_deref(), Some("ghs_abc"));
        assert_eq!(token_from_answer(r#"{"message":"Bad credentials"}"#), None);
    }
    /// A stand-in GitHub on loopback: answers one request with `reply`, hands back what it got.
    fn stand_in(reply: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", l.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = vec![0u8; 65536]; let mut got = Vec::new();
            loop {
                let n = s.read(&mut buf).unwrap_or(0); got.extend_from_slice(&buf[..n]);
                let t = String::from_utf8_lossy(&got).to_string();
                if n == 0 || t.contains("\r\n\r\n") { break; }
            }
            let _ = write!(s, "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply);
            let _ = tx.send(String::from_utf8_lossy(&got).to_string());
        });
        (addr, rx)
    }

    // NEGATIVE PROOF (Silas 17:18): the signed JWT and the installation credential never sit on
    // a command line, where `ps` shows them: curl's argv holds neither, the JWT arrives at GitHub
    // as the Authorization header (it went on stdin), and the answer lands 0600 in a file.
    #[test]
    fn the_jwt_and_the_installation_credential_never_reach_argv() {
        let _g = CRED_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let d = tmp("argv");
        let app = d.join("app"); std::fs::create_dir_all(&app).unwrap();
        let key = app.join("key.pem");
        let ok = std::process::Command::new("/usr/bin/openssl").args(["genrsa", "-out"]).arg(&key).arg("2048")
            .stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
        if !ok { eprintln!("unmeasured: no openssl genrsa here"); return; }
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
        put(&app.join("app-id"), "123\n"); put(&app.join("installation-id"), "456\n");
        let (api, got) = stand_in(r#"{"token":"ghs_FAKE4710"}"#);
        std::env::set_var(APP_DIR_ENV, &app);
        std::env::set_var(GITHUB_API_ENV, &api);
        let r = arm_app_credential();
        std::env::remove_var(APP_DIR_ENV); std::env::remove_var(GITHUB_API_ENV);
        let armed = r.unwrap();
        std::env::remove_var(CRED_FILE_ENV);
        let req = got.recv().unwrap();
        assert!(req.starts_with("POST /app/installations/456/access_tokens"), "{req}");
        let auth = req.lines().find(|l| l.to_lowercase().starts_with("authorization:")).expect("no Authorization header reached GitHub");
        let jwt = auth.rsplit(' ').next().unwrap().trim();
        assert_eq!(jwt.split('.').count(), 3, "{auth}");
        let argv = token_request_argv(&api, "456").join(" ");
        assert!(!argv.contains(jwt) && !argv.to_lowercase().contains("authorization"), "the JWT is on argv: {argv}");
        use std::os::unix::fs::MetadataExt;
        assert_eq!(std::fs::metadata(&armed).unwrap().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&armed).unwrap(), "ghs_FAKE4710\n");
    }
}

