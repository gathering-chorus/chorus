//! #4944 — a fake door for the sweep: a curl that answers the roles and sessions
//! listings and records every write (method, url, body) to `calls.log`, a token
//! bin, and a spine log bin that records each event's args to `spine.log`.
#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn tempdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("t4944-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn exe(p: &Path, body: &str) {
    std::fs::write(p, body).unwrap();
    Command::new("chmod").arg("+x").arg(p).status().unwrap();
}

/// An expired, open session of `role` as the door's listing returns it: every field the
/// shape holds, plus the ones a whole-row PUT was refused for (`status`, minted links).
pub fn expired_row(role: &str) -> String {
    format!(r#"{{"name":"{role}-s-1","status":"","sessionState":"open","tokenId":"tok-1","ownedBy":"principal-{role}","actsAs":"{role}","issuedAt":"2026-11-09T10:00:00Z","expiresAt":"2026-11-09T10:10:00Z","runOf":"session-{role}-s-1"}}"#)
}

/// The fake door. `write_code` is what it answers every PATCH/PUT with.
pub fn door(home: &Path, role: &str, write_code: &str) { door_with_runs(home, role, write_code, r#"{"data":[]}"#) }

/// The fake door with `runs` as its open-runs listing (identity/sessionruns).
pub fn door_with_runs(home: &Path, role: &str, write_code: &str, runs: &str) {
    let roles = format!(r#"{{"data":[{{"name":"{role}","roleKind":"agent"}}]}}"#);
    let sessions = format!(r#"{{"data":[{}]}}"#, expired_row(role));
    let log = home.join("calls.log");
    exe(&home.join("curl"), &format!(r#"#!/bin/sh
method=GET; url=""; body=""
while [ $# -gt 0 ]; do
  case "$1" in
    -X) method="$2"; shift ;;
    --data-binary) f="${{2#@}}"; body=$(cat "$f"); shift ;;
    -H|-w|-o|--max-time) shift ;;
    http*) url="$1" ;;
  esac
  shift
done
case "$method" in
  PATCH|PUT|POST) printf '%s %s %s\n' "$method" "$url" "$body" >> '{log}'
    if [ '{code}' = 200 ]; then printf '{{}}\n200'; else printf '{{"error":"shape","why":"off-model property status"}}\n{code}'; fi ;;
  *) case "$url" in
       *roles/roles*) printf '%s' '{roles}' ;;
       *identity/sessionruns*) printf '%s' '{runs}' ;;
       *identity/sessions*) printf '%s' '{sessions}' ;;
       *) printf '{{"data":[]}}' ;;
     esac ;;
esac
"#, log = log.display(), code = write_code, roles = roles, sessions = sessions, runs = runs));
    exe(&home.join("token-bin"), "#!/bin/sh\necho tok\n");
    exe(&home.join("log-bin"), &format!("#!/bin/sh\necho \"$*\" >> '{}'\n", home.join("spine.log").display()));
}

/// Run `chorus-principal sweep` against the fake door; returns (exit code, stdout+stderr).
pub fn sweep(home: &Path) -> (i32, String) {
    let out = Command::new(bin()).arg("sweep")
        .env("HOME", home)
        .env("AWAKE_CURL", home.join("curl"))
        .env("CHORUS_TOKEN_BIN", home.join("token-bin"))
        .env("CHORUS_LOG_BIN", home.join("log-bin"))
        .env("CHORUS_IDENTITY_DIR", home.join("identity"))
        // #4948 — the registry is the test's own, never the caller's live one
        .env("CHORUS_SESSIONS_DIR", home.join(".chorus/sessions"))
        .env("CHORUS_ROOT", home)
        .env("CHORUS_API_URL", "http://door.test")
        .output().unwrap();
    (out.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

pub fn read(p: PathBuf) -> String { std::fs::read_to_string(p).unwrap_or_default() }

/// The built binary, beside this test (target/<profile>/).
pub fn bin() -> PathBuf {
    let me = std::env::current_exe().unwrap();
    me.parent().and_then(|d| d.parent()).unwrap().join("chorus-principal")
}
