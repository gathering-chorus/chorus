// #4184 — the decision core, unit-tested with fixtures. The bats suite drives the
// built binary with stub claude/tmux/ps for the integration proofs.
use chorus_awake::{decide, parse_registry, proof_line, projects_dir_for, Live};

const NOW: u128 = 1_789_000_000_000;
fn agent(id: &str, sid: &str, pid: &str, started_ago_h: f64, state: &str) -> String {
    let started = NOW - (started_ago_h * 3600.0 * 1000.0) as u128;
    format!(r#"{{"pid":"{}","id":"{}","cwd":"/r/kade","kind":"background","startedAt":"{}","sessionId":"{}","name":"n","state":"{}"}}"#, pid, id, started, sid, state)
}

#[test]
fn registry_entry_parses_and_proof_line_reads_like_the_spec() {
    let l = parse_registry(r#"{"role":"kade","pid":56344,"tty":"/dev/ttys004","host":"tmux","tmux":"%0","registered_at":"1"}"#).unwrap();
    assert_eq!(l, Live { pid: 56344, tty: "/dev/ttys004".into(), host: "tmux".into(), pane: "%0".into() });
    assert_eq!(proof_line("kade", &l, "already awake"), "awake: kade  pid 56344  tty /dev/ttys004  pane %0  registered yes  via already awake");
    assert!(parse_registry(r#"{"role":"kade","tty":"/dev/ttys004"}"#).is_none(), "no pid is not a session");
}

#[test]
fn non_tmux_host_is_named_in_the_line_because_nudges_need_a_pane() {
    let l = parse_registry(r#"{"pid":1,"tty":"/dev/ttys001","host":"vscode"}"#).unwrap();
    assert!(proof_line("wren", &l, "x").contains("yes-but-host=vscode"));
    assert_eq!(l.pane, "-");
}

#[test]
fn attaches_only_the_background_session_that_is_the_latest_conversation() {
    let json = format!("[{}]", agent("79906dc2", "79906dc2-1681-44e5", "87866", 0.5, "working"));
    let d = decide(&json, Some("79906dc2-1681-44e5"), 24.0, NOW);
    assert_eq!(d.attach.as_deref(), Some("79906dc2"));
    assert!(d.stale.is_empty());
}

#[test]
fn negative_proof_an_older_helper_is_never_attached_when_a_newer_conversation_exists() {
    // first live run 2026-09-16: the 08:25 helper (8faa3fa4) was attached instead of the 13:00 conversation
    let json = format!("[{}]", agent("8faa3fa4", "8faa3fa4-80ae", "1", 5.0, "working"));
    let d = decide(&json, Some("38b6cebe-6041"), 24.0, NOW);
    assert_eq!(d.attach, None, "must fall to claude -c");
}

#[test]
fn exited_or_pidless_background_sessions_are_not_attachable() {
    let json = format!("[{},{}]", agent("aaaa", "aaaa-1", "", 0.1, "working"), agent("bbbb", "bbbb-1", "9", 0.1, "exited"));
    assert_eq!(decide(&json, Some("aaaa-1"), 24.0, NOW).attach, None);
    assert_eq!(decide(&json, Some("bbbb-1"), 24.0, NOW).attach, None);
}

#[test]
fn stale_is_older_than_the_window_and_never_the_attach_target() {
    let json = format!("[{},{},{}]", agent("old1", "o1", "1", 30.0, "blocked"), agent("old2", "o2", "2", 26.0, "blocked"), agent("cur", "38b6-1", "3", 30.0, "working"));
    let d = decide(&json, Some("38b6-1"), 24.0, NOW);
    assert_eq!(d.attach.as_deref(), Some("cur"));
    assert_eq!(d.stale, vec!["old1".to_string(), "old2".to_string()]);
    let d2 = decide(&json, Some("38b6-1"), 48.0, NOW);
    assert!(d2.stale.is_empty(), "a wider window names nothing");
}

#[test]
fn garbage_or_empty_session_list_means_continue_and_nothing_stale() {
    assert_eq!(decide("[]", Some("x"), 24.0, NOW), chorus_awake::Decision { attach: None, stale: vec![] });
    assert_eq!(decide("not json", Some("x"), 24.0, NOW).attach, None);
    assert_eq!(decide("[]", None, 24.0, NOW).attach, None);
}

#[test]
fn projects_dir_is_the_role_dir_with_slashes_as_dashes() {
    assert_eq!(projects_dir_for("/Users/j", "/Users/j/CascadeProjects/chorus/roles/kade").to_string_lossy(), "/Users/j/.claude/projects/-Users-j-CascadeProjects-chorus-roles-kade");
}

// ---------------------------------------------------------------- #4202 login

mod login_4202 {
    use chorus_awake::*;

    fn tok(webid: &str, jti: &str, iat: u64, exp: u64) -> String {
        let payload = format!(r#"{{"webid":"{}","jti":"{}","iat":{},"exp":{}}}"#, webid, jti, iat, exp);
        // base64url, no padding
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let b = payload.as_bytes();
        let mut out = String::new();
        for ch in b.chunks(3) {
            let n = ((ch[0] as u32) << 16) | ((*ch.get(1).unwrap_or(&0) as u32) << 8) | (*ch.get(2).unwrap_or(&0) as u32);
            out.push(T[((n >> 18) & 63) as usize] as char);
            out.push(T[((n >> 12) & 63) as usize] as char);
            if ch.len() > 1 { out.push(T[((n >> 6) & 63) as usize] as char); }
            if ch.len() > 2 { out.push(T[(n & 63) as usize] as char); }
        }
        format!("eyJhbGciOiJFUzI1NiJ9.{}.sig", out)
    }
    const KADE: &str = "https://id.lightlifeurbangardens.com/kade/profile/card#me";

    #[test]
    fn b64url_round_trips_a_payload() {
        assert_eq!(b64url_decode("eyJhIjoxfQ").unwrap(), b"{\"a\":1}");
        assert_eq!(b64url_decode("eyJhIjoxfQ==").unwrap(), b"{\"a\":1}");
        assert!(b64url_decode("not base64!").is_none());
    }

    #[test]
    fn a_token_naming_the_role_is_a_login() {
        let l = login_check("kade", &tok(KADE, "jti-1", 100, 700), 200).unwrap();
        assert_eq!(l.webid, KADE);
        assert_eq!(l.jti, "jti-1");
        assert_eq!((l.iat, l.exp), (100, 700));
    }

    /// NEGATIVE PROOF — a valid token for ANOTHER role is not this role's login.
    #[test]
    fn another_roles_token_is_refused_as_wrong_principal() {
        let e = login_check("kade", &tok("https://id.lightlifeurbangardens.com/silas/profile/card#me", "j", 1, 999), 2).unwrap_err();
        assert!(e.starts_with("wrong principal"), "{e}");
        assert!(e.contains("silas"), "{e}");
    }

    /// NEGATIVE PROOF — an expired token is not a login, even for the right role.
    #[test]
    fn an_expired_token_is_refused() {
        let e = login_check("kade", &tok(KADE, "j", 1, 50), 60).unwrap_err();
        assert!(e.starts_with("expired"), "{e}");
    }

    /// NEGATIVE PROOF — garbage is "no session", not a panic and not a login.
    #[test]
    fn garbage_is_no_session() {
        assert!(login_check("kade", "garbage", 1).unwrap_err().starts_with("no session"));
        assert!(login_check("kade", "", 1).unwrap_err().starts_with("no session"));
        let no_jti = login_check("kade", &tok(KADE, "", 1, 999), 2).unwrap_err();
        assert!(no_jti.contains("jti"), "{no_jti}");
    }

    #[test]
    fn the_session_row_is_owned_by_the_principal_and_never_carries_the_token() {
        let l = login_check("kade", &tok(KADE, "abc-jti-0001", 1758124800, 1758125400), 1758124801).unwrap();
        let (name, body) = session_row("kade", &l, "chorus-kade");
        assert_eq!(name, "session-kade-jti-0001");
        assert_eq!(body["ownedBy"], "principal-kade");
        assert_eq!(body["tokenId"], "abc-jti-0001");
        assert_eq!(body["sessionState"], "open");
        assert_eq!(body["hostAccount"], "chorus-kade");
        assert_eq!(body["issuedAt"], "2025-09-17T16:00:00Z");
        assert_eq!(body["expiresAt"], "2025-09-17T16:10:00Z");
        assert!(!body.to_string().contains("eyJ"));
    }
}
