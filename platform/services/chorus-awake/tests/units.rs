// #4184 — the decision core, unit-tested with fixtures. The bats suite drives the
// built binary with stub claude/tmux/ps for the integration proofs.
use chorus_awake::{transcript_is_poisoned, tz_offset_secs, answered_recently, awake_verdict, chrono_secs, Awake, decide, parse_registry, proof_line, projects_dir_for, Live, login_posture, Start};

const NOW: u128 = 1_789_000_000_000;
fn agent(id: &str, sid: &str, pid: &str, started_ago_h: f64, state: &str) -> String {
    let started = NOW - (started_ago_h * 3600.0 * 1000.0) as u128;
    format!(r#"{{"pid":"{}","id":"{}","cwd":"/r/kade","kind":"background","startedAt":"{}","sessionId":"{}","name":"n","state":"{}"}}"#, pid, id, started, sid, state)
}

#[test]
fn registry_entry_parses_and_proof_line_reads_like_the_spec() {
    let l = parse_registry(r#"{"role":"kade","pid":56344,"tty":"/dev/ttys004","host":"tmux","tmux":"%0","registered_at":"1"}"#).unwrap();
    assert_eq!(l, Live { pid: 56344, tty: "/dev/ttys004".into(), host: "tmux".into(), pane: "%0".into() });
    assert_eq!(proof_line("kade", &l, "logged in", "already awake"), "awake: kade  pid 56344  tty /dev/ttys004  pane %0  logged in  via already awake");
    assert_eq!(proof_line("kade", &l, "", "x"), "awake: kade  pid 56344  tty /dev/ttys004  pane %0  NOT logged in  via x", "no login word is never read as fine");
    assert!(parse_registry(r#"{"role":"kade","tty":"/dev/ttys004"}"#).is_none(), "no pid is not a session");
}

#[test]
fn non_tmux_host_is_named_in_the_line_because_nudges_need_a_pane() {
    let l = parse_registry(r#"{"pid":1,"tty":"/dev/ttys001","host":"vscode"}"#).unwrap();
    assert!(proof_line("wren", &l, "logged in", "x").contains("host=vscode (nudges need tmux)"));
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
    fn the_session_name_is_the_one_the_dal_stores() {
        // #4295 — live 2026-09-25: sent silas-3U01z2C--1a0d90f31b5, stored silas-3u01z2c-1a0d90f31b5
        let l = login_check("silas", &tok("https://id.lightlifeurbangardens.com/silas/profile/card#me", "XoHOD4cSCW-P03U01z2C-", 1, 99_999_999_999), 2).unwrap();
        let (name, _) = session_row("silas", &l, "jeffbridwell", "1a0d90f31b5");
        assert_eq!(name, "silas-3u01z2c-1a0d90f31b5");
        assert_eq!(chorus_awake::slug("A--b__C-"), "a-b-c");
    }

    #[test]
    fn the_session_row_is_owned_by_the_principal_and_never_carries_the_token() {
        let l = login_check("kade", &tok(KADE, "abc-jti-0001", 1758124800, 1758125400), 1758124801).unwrap();
        let (name, body) = session_row("kade", &l, "chorus-kade", "1a2b3c");
        // #4202 — the BARE name. The mint adds the `session-` prefix and refuses
        // a name that already carries it ("double-prefix ... pass the bare
        // name"), which is a 422 the first real login earned.
        // #4215 — the name carries the START, not only the token. Two starts inside
        // one cached token's life used to mint the same name and the second 409'd,
        // which stopped the role booting at all.
        assert_eq!(name, "kade-jti-0001-1a2b3c");
        assert_eq!(body["ownedBy"], "principal-kade");
        assert_eq!(body["tokenId"], "abc-jti-0001");
        assert_eq!(body["sessionState"], "open");
        assert_eq!(body["hostAccount"], "chorus-kade");
        assert_eq!(body["issuedAt"], "2025-09-17T16:00:00Z");
        assert_eq!(body["expiresAt"], "2025-09-17T16:10:00Z");
        assert!(!body.to_string().contains("eyJ"));
    }

    /// #4215 NEGATIVE PROOF — the whole defect was that this could NOT be true.
    /// Same login, two starts: the names must differ, or the second collides with
    /// the first's row and the role does not boot. Mutating session_row back to the
    /// jti-only name makes this assertion fail, which is the point.
    #[test]
    fn two_starts_on_one_cached_token_get_different_session_names() {
        let l = login_check("kade", &tok(KADE, "abc-jti-0001", 1758124800, 1758125400), 1758124801).unwrap();
        let (first, _)  = session_row("kade", &l, "chorus-kade", "aaa111");
        let (second, _) = session_row("kade", &l, "chorus-kade", "bbb222");
        assert_ne!(first, second, "two starts on one token must not share a session name");
        assert!(first.starts_with("kade-jti-0001-"), "{first}");
        assert!(second.starts_with("kade-jti-0001-"), "{second}");
    }
}

/// #4215 — a role is awake when it SPOKE, not when a file says so.
#[test]
fn a_session_that_has_not_spoken_is_not_awake_however_live_the_file_looks() {
    let now = chrono_secs("2026-09-19T07:33:00").unwrap();
    let spine = concat!(
        "{\"timestamp\":\"2026-09-19T06:01:00\",\"event\":\"reply.published\",\"role\":\"kade\"}\n",
        "{\"timestamp\":\"2026-09-19T07:32:00\",\"event\":\"reply.published\",\"role\":\"silas\"}\n");
    // silas spoke a minute ago
    assert!(answered_recently(spine, "silas", 600, now, 0));
    // kade's last word was 92 minutes ago — the exact shape of the 2026-09-19
    // fugue, where his pid and registry entry were both perfectly healthy
    assert!(!answered_recently(spine, "kade", 600, now, 0));
}

/// #4215 NEGATIVE PROOF — an unparseable or absent stamp must read as NOT awake.
/// A liveness check that treats "I could not tell" as "yes" is the shape that
/// let a dead session look alive for an hour.
#[test]
fn unreadable_or_missing_activity_is_not_awake() {
    let now = chrono_secs("2026-09-19T07:33:00").unwrap();
    assert!(!answered_recently("", "kade", 600, now, 0));
    assert!(!answered_recently("{\"timestamp\":\"not-a-date\",\"event\":\"reply.published\",\"role\":\"kade\"}", "kade", 600, now, 0));
    // a heartbeat is not a reply: the process being alive is the thing we stopped trusting
    assert!(!answered_recently("{\"timestamp\":\"2026-09-19T07:32:59\",\"event\":\"system.heartbeat\",\"role\":\"kade\"}", "kade", 600, now, 0));
}

/// #4215 — the branch ACTS on liveness: a mute session is replaced, not blessed.
#[test]
fn a_mute_session_is_replaced_and_a_talking_one_is_left_alone() {
    assert_eq!(awake_verdict(true,  true,  true), Awake::AlreadyAwake);
    assert_eq!(awake_verdict(true,  false, true), Awake::ReplaceMute);
    assert_eq!(awake_verdict(false, false, true), Awake::Fresh);
}

/// #4215 NEGATIVE PROOF — with the check off, the SAME mute session reads as
/// fine. That is exactly what shipped before today, and what left Kade with a
/// healthy pid, a healthy registry entry and no way to answer for over an hour.
#[test]
fn without_the_liveness_check_the_same_mute_session_is_blessed() {
    assert_eq!(awake_verdict(true, false, false), Awake::AlreadyAwake);
    assert_ne!(awake_verdict(true, false, false), awake_verdict(true, false, true));
}

// ---- #4215 — a login failure must not be the reason a role does not run ----

#[test]
fn the_api_being_down_still_starts_the_role() {
    // no token because chorus-identity-token could not reach the API
    match login_posture(Some("no token: connection refused"), false) {
        Start::Degraded(w) => assert!(w.contains("connection refused")),
        other => panic!("the API being down must not stop a start, got {:?}", other),
    }
}

#[test]
fn a_shape_complaint_or_5xx_still_starts_the_role() {
    for why in ["http=422 duplicate 'tokenId'", "http=500", "curl failed: timeout"] {
        match login_posture(Some(why), false) {
            Start::Degraded(_) => {}
            other => panic!("{} must degrade, not refuse; got {:?}", why, other),
        }
    }
}

#[test]
fn a_credential_naming_another_principal_still_refuses() {
    // the ONE refusal: starting here would file kade's work under silas
    let why = "wrong principal: the token names https://x/kade/profile/card#me not silas";
    match login_posture(Some(why), false) {
        Start::Refuse(w) => assert!(w.contains("wrong principal")),
        other => panic!("a wrong-principal credential must refuse, got {:?}", other),
    }
}

#[test]
fn a_clean_login_is_just_go() {
    assert_eq!(login_posture(None, false), Start::Go);
}

#[test]
fn negative_proof_the_old_refuse_everything_posture_would_have_blocked_the_same_start() {
    // The two postures must DIFFER on the input that cost Jeff an hour of Kade.
    // If this ever passes with them equal, the degrade branch is decorative.
    let why = "http=422 duplicate 'tokenId' across all session";
    let now = login_posture(Some(why), false);
    let before = login_posture(Some(why), true);
    assert_eq!(before, Start::Refuse(why.to_string()));
    assert_ne!(now, before);

    // and the one real refusal is NOT the thing that changed
    let wrong = "wrong principal: the token names kade not silas";
    assert_eq!(login_posture(Some(wrong), false), login_posture(Some(wrong), true));
}


#[test]
fn a_local_spine_stamp_is_not_four_hours_stale() {
    // #4215 — the spine writes local time with no offset. Read as UTC it put
    // every Boston stamp 4h in the past, which would have ended a role that had
    // just answered.
    let line = r#"{"role":"kade","event":"reply.published","timestamp":"2026-09-19T07:33:01"}"#;
    let now_utc = 1_789_000_000u64; // irrelevant: we compute both frames below
    let _ = now_utc;
    let local_secs = chrono_secs("2026-09-19T07:33:01").unwrap();
    let edt = -4 * 3600;
    let now = local_secs + 4 * 3600 + 60; // one minute later, in UTC
    assert!(answered_recently(line, "kade", 900, now, edt), "a stamp one minute old must read as recent");
    assert!(!answered_recently(line, "kade", 900, now, 0), "reading local as UTC is what made it look mute");
}

#[test]
fn an_unreadable_offset_is_none_and_a_readable_one_parses() {
    assert_eq!(tz_offset_secs("-0400"), Some(-4 * 3600));
    assert_eq!(tz_offset_secs("+0530"), Some(5 * 3600 + 1800));
    assert_eq!(tz_offset_secs(""), None);
    assert_eq!(tz_offset_secs("nonsense"), None);
}

// ---- #4219 — never resume a conversation the API has stopped accepting ----

#[test]
fn a_transcript_ending_in_refusals_is_poisoned() {
    let t = "{\"type\":\"user\"}\n{\"content\":\"API Error: safeguards flagged this message\"}\n";
    assert!(transcript_is_poisoned(t, 40, 1));
}

#[test]
fn a_healthy_transcript_is_not_poisoned() {
    let t = "{\"type\":\"user\"}\n{\"type\":\"assistant\",\"text\":\"on it\"}\n";
    assert!(!transcript_is_poisoned(t, 40, 1));
}

#[test]
fn one_old_refusal_far_back_is_noise_not_a_dead_conversation() {
    // kade's real shape: refusals scattered through 3,160 lines. What decides is
    // whether they are at the END — a conversation that recovered is resumable.
    let mut t = String::from("{\"content\":\"API Error: reasoning_extraction\"}\n");
    for _ in 0..100 { t.push_str("{\"type\":\"assistant\",\"text\":\"fine\"}\n"); }
    assert!(!transcript_is_poisoned(&t, 40, 1), "an old refusal must not condemn a working conversation");
}

#[test]
fn negative_proof_the_window_is_what_makes_the_two_differ() {
    // Same transcript, two windows: if a wider window did not change the verdict
    // the check is not reading position at all.
    let mut t = String::from("{\"content\":\"safeguards flagged\"}\n");
    for _ in 0..100 { t.push_str("{\"type\":\"assistant\"}\n"); }
    assert!(!transcript_is_poisoned(&t, 40, 1));
    assert!(transcript_is_poisoned(&t, 200, 1));
}
