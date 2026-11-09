// @test-type: integration — runs the built binary's establish with a stub door and spine; no network
// @domain: identity
//! #4934 AC2 reopen — prod 2026-11-09 09:48:40, borg-queen's login
//! (trace login-login-borg-queen-1794235717840-68338) RESUMED its still-open session
//! (`session.resumed`), and no `session.domains.established` was written: the domains
//! were named inside do_login only, which a resumed session never runs.
//!
//! NEGATIVE PROOF (#3734): with the domains call back inside do_login alone, the
//! resumed case below finds no `session.domains.established` line and goes red.
use std::fs;
use std::process::{Command, Stdio};

fn me() -> String {
    String::from_utf8_lossy(&Command::new("id").arg("-un").output().unwrap().stdout).trim().to_string()
}

/// A home with fixture role `fx`, a stub door (curl) and a stub spine (chorus-log).
/// `resumed`: the login state names an open session, so establish resumes it.
fn run_establish(tag: &str, resumed: bool) -> (i32, String, String) {
    let home = std::env::temp_dir().join(format!("resumed-4934-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let id = home.join("identity/fx");
    fs::create_dir_all(&id).unwrap();
    let spine = home.join("spine.log");
    let log = home.join("chorus-log");
    fs::write(&log, format!("#!/bin/bash\necho \"$*\" >> '{}'\n", spine.display())).unwrap();
    // the door: the principal (hostAccount = this account), the permissions listing, and
    // 200 {} for anything else; -w '\n%{{http_code}}' callers get the code on its own line
    let curl = home.join("curl");
    fs::write(&curl, format!(r#"#!/bin/bash
url="${{@: -1}}"; w=0; for a in "$@"; do [ "$a" = "-w" ] && w=1; done
case "$url" in
  */identity/principals/fx) body='{{"data":{{"name":"principal-fx","hostAccount":"{me}"}}}}' ;;
  */security/permissions*) body='{{"data":[{{"agent":"principal-fx","mode":"http://www.w3.org/ns/auth/acl#Write","accessTo":"urn:chorus:domains:cards"}}]}}' ;;
  *) body='{{}}' ;;
esac
if [ $w = 1 ]; then printf '%s\n200' "$body"; else printf '%s' "$body"; fi
"#, me = me())).unwrap();
    let token = home.join("token");
    fs::write(&token, "#!/bin/bash\necho a.b.c\n").unwrap();
    for f in [&log, &curl, &token] { Command::new("chmod").args(["+x", f.to_str().unwrap()]).status().unwrap(); }
    if resumed {
        fs::write(id.join("login.json"), r#"{"state":"recorded","session":"fx-s1-1a1","pid":null,"at":"2026-11-09T14:48:00Z"}"#).unwrap();
        fs::write(id.join("session.row.json"), r#"{"name":"fx-s1-1a1","sessionState":"open","actsAs":"fx"}"#).unwrap();
    }
    let o = Command::new(env!("CARGO_BIN_EXE_chorus-principal")).args(["establish", "fx"])
        .env("HOME", &home).env("CHORUS_IDENTITY_DIR", home.join("identity"))
        .env("CHORUS_SESSIONS_DIR", home.join("sessions")).env("CHORUS_LOG_BIN", &log)
        .env("AWAKE_CURL", &curl).env("CHORUS_TOKEN_BIN", &token).env("CHORUS_API_URL", "http://door.invalid")
        .env("AWAKE_RELAY_PROJECT", "none").env("TMUX_BIN", "/usr/bin/false")
        .env_remove("CHORUS_ROLE").env_remove("TMUX_PANE").env_remove("CHORUS_TRACE_ID")
        .stdin(Stdio::null()).output().unwrap();
    let spine_text = fs::read_to_string(&spine).unwrap_or_default();
    let _ = fs::remove_dir_all(&home);
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stderr).to_string(), spine_text)
}

#[test]
fn a_resumed_session_names_its_domains_like_a_fresh_login() {
    let (code, err, spine) = run_establish("resumed", true);
    assert!(spine.contains("session.resumed fx"), "the resume path ran: code {code} {err}\n{spine}");
    let line = spine.lines().find(|l| l.starts_with("session.domains.established fx"))
        .unwrap_or_else(|| panic!("a resumed login named no domains: code {code} {err}\n{spine}"));
    assert!(line.contains("session=fx-s1-1a1") && line.contains("domains=cards:Write"), "{line}");
}
