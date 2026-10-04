use chorus_agent::{
    config::{self, Config},
    contract::*,
    server::Supervisor,
    store::{self, Store},
};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt, sync::Arc, time::Duration};

struct Fixture {
    _dir: tempfile::TempDir,
    app: Arc<Supervisor>,
    // #4424 — the SessionRun rows chorus-principal login would have written:
    // name → runEndedAt ("" = live).
    runs: Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>,
    credential: String,
    cwd: String,
    identity: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.identity.abort();
    }
}
/// #4424 — every test in this binary logs through one fake chorus-log that
/// appends its arguments to a shared sink, so no test reaches the real spine.
fn spine_sink() -> &'static std::path::PathBuf {
    static SINK: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    SINK.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap().keep();
        let sink = dir.join("spine.txt");
        let fake = dir.join("chorus-log");
        fs::write(&fake, format!("#!/bin/sh\necho \"$@\" >> '{}'\n", sink.display())).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("CHORUS_LOG_BIN", &fake);
        sink
    })
}
async fn fixture(mode: &str) -> Fixture {
    spine_sink();
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("fake-codex");
    fs::write(&executable,"#!/bin/sh\nif [ \"$1\" = '--version' ]; then echo 'fixture-codex 1'; exit 0; fi\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"thread.started\",\"thread_id\":\"native-fixture\"}' '{\"type\":\"turn.started\"}' '{\"type\":\"item.completed\",\"item\":{\"id\":\"m1\",\"type\":\"agent_message\",\"text\":\"done\"}}' '{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":4}}'\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    // #4424 — a real setup writes the Codex hook file into the workspace
    // (agent-config.py); enrollment now refuses a workspace without it.
    fs::create_dir_all(dir.path().join(".codex")).unwrap();
    fs::write(dir.path().join(".codex/hooks.json"), r#"{"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"chorus-hook-shim runtime-hook codex PreToolUse"}]}]}}"#).unwrap();
    let credential = dir.path().join("token");
    fs::write(&credential, "wren-token").unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runs: Arc<std::sync::Mutex<std::collections::HashMap<String, String>>> = Default::default();
    let served = runs.clone();
    let identity = tokio::spawn(async move {
        let router=axum::Router::new().route("/runs/{name}",axum::routing::get(move |axum::extract::Path(name):axum::extract::Path<String>|{
            let served=served.clone();
            async move {
                match served.lock().unwrap().get(&name).cloned() {
                    Some(ended)=>(axum::http::StatusCode::OK,axum::Json(json!({"name":name,"ownedBy":"principal-wren","runEndedAt":ended}))),
                    None=>(axum::http::StatusCode::NOT_FOUND,axum::Json(json!({"error":"not found"}))),
                }
            }
        })).route("/verify",axum::routing::post(|headers:axum::http::HeaderMap|async move{
            if headers.get("authorization").and_then(|v|v.to_str().ok())!=Some("Bearer wren-token"){return (axum::http::StatusCode::UNAUTHORIZED,axum::Json(json!({"error":"invalid"})));}
            (axum::http::StatusCode::OK,axum::Json(json!({"principal":"https://identity/wren","role":"wren","scopes":["werk:write"]})))
        }));
        axum::serve(listener, router).await.unwrap();
    });
    let config:Config=serde_json::from_value(json!({"version":1,"profiles":{"test":{"runtime":"codex","mode":mode,"enforcement":"trusted","approved_gaps":["filesystem isolation and hook coverage require deployment conformance"],"executable":executable,"timeout_secs":5}},"role_workspaces":{"wren":dir.path()},"max_concurrent_jobs":1,"runs_url":format!("http://{addr}/runs")})).unwrap();
    let app = Supervisor::new(
        config,
        Store::open(dir.path().join("state")).unwrap(),
        format!("http://{addr}/verify"),
    )
    .unwrap();
    Fixture {
        cwd: dir.path().to_string_lossy().into(),
        credential: credential.to_string_lossy().into(),
        _dir: dir,
        app,
        runs,
        identity,
    }
}
/// A fresh live SessionRun per start, as a fresh login would mint.
fn start(f: &Fixture) -> StartRequest {
    let run = format!("wren-run-{}", f.runs.lock().unwrap().len() + 1);
    f.runs.lock().unwrap().insert(run.clone(), String::new());
    serde_json::from_value(json!({"version":1,"profile":"test","role":"wren","cwd":f.cwd,"credential_file":f.credential,"primary":true,"session_run":run})).unwrap()
}
fn send(id: &str, text: &str) -> SendRequest {
    SendRequest {
        version: 1,
        message_id: id.into(),
        input: text.into(),
        kind: "peer_message".into(),
    }
}
async fn wait_idle(f: &Fixture, id: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if f.app.store.lock().await.get(id).unwrap().state == State::Idle {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn enrollment_binds_identity_primary_role_and_operator_trust() {
    let f = fixture("native").await;
    let mut wrong = start(&f);
    wrong.role = "kade".into();
    assert!(f.app.start(wrong).await.unwrap_err().contains("role"));
    let s = f.app.start(start(&f)).await.unwrap();
    assert!(f
        .app
        .start(start(&f))
        .await
        .unwrap_err()
        .contains("primary"));
    assert!(s.public().get("credential_file").is_none());
    assert!(!s.capabilities.gaps.is_empty());
    let mut forged = serde_json::to_value(start(&f)).unwrap();
    forged["enforcement"] = json!("trusted");
    assert!(serde_json::from_value::<StartRequest>(forged).is_err());
    assert_eq!(
        f.app
            .send(&s.session_id, send("pulse:1", "hello 🌍\nline 2"))
            .await
            .unwrap()["status"],
        "queued"
    );
    assert!(f
        .app
        .store
        .lock()
        .await
        .get(&s.session_id)
        .unwrap()
        .message_receipts
        .is_empty());
}

#[tokio::test]
async fn invalid_credentials_and_symlinks_are_refused_without_fallback() {
    let f = fixture("native").await;
    fs::write(&f.credential, "expired").unwrap();
    assert!(f.app.start(start(&f)).await.unwrap_err().contains("401"));
    fs::write(&f.credential, "wren-token").unwrap();
    fs::set_permissions(&f.credential, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(f
        .app
        .start(start(&f))
        .await
        .unwrap_err()
        .contains("owner-only"));
    fs::set_permissions(&f.credential, fs::Permissions::from_mode(0o600)).unwrap();
    let link = f._dir.path().join("linked");
    std::os::unix::fs::symlink(&f.credential, &link).unwrap();
    let mut req = start(&f);
    req.credential_file = link.to_string_lossy().into();
    assert!(f.app.start(req).await.is_err());
}

#[tokio::test]
async fn managed_delivery_is_deduplicated_and_preserves_receipt_after_restart() {
    let f = fixture("managed").await;
    let s = f.app.start(start(&f)).await.unwrap();
    let result = f
        .app
        .send(&s.session_id, send("pulse:100", "héllo\nworld"))
        .await
        .unwrap();
    assert_eq!(result["status"], "transport_accepted");
    wait_idle(&f, &s.session_id).await;
    assert_eq!(
        f.app
            .send(&s.session_id, send("pulse:100", "héllo\nworld"))
            .await
            .unwrap()["status"],
        "context_delivered"
    );
    assert!(f
        .app
        .send(&s.session_id, send("pulse:100", "changed"))
        .await
        .unwrap_err()
        .contains("different content"));
    let store = f.app.store.lock().await;
    let events = store.events(&s.session_id, 0).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == "input.accepted")
            .count(),
        1
    );
    let root = store.root.clone();
    drop(store);
    let reopened = Store::open(root).unwrap();
    let restored = reopened.get(&s.session_id).unwrap();
    assert_eq!(restored.state, State::Disconnected);
    assert_eq!(
        restored.native_session_id.as_deref(),
        Some("native-fixture")
    );
    assert_eq!(restored.message_receipts["pulse:100"], "context_delivered");
}

#[tokio::test]
async fn event_replay_does_not_rewind_lifecycle_and_native_identity_cannot_change() {
    let f = fixture("native").await;
    let s = f.app.start(start(&f)).await.unwrap();
    let started = store::event(&s.session_id, "turn.started", json!({}));
    f.app.record_event(started.clone()).await.unwrap();
    f.app
        .record_event(store::event(&s.session_id, "turn.completed", json!({})))
        .await
        .unwrap();
    f.app.record_event(started.clone()).await.unwrap();
    assert_eq!(
        f.app.store.lock().await.get(&s.session_id).unwrap().state,
        State::Idle
    );
    let mut changed = started;
    changed.data = json!({"forged":true});
    assert!(f.app.record_event(changed).await.is_err());
    let mut event = store::event(&s.session_id, "session.started", json!({}));
    event.native_session_id = Some("original".into());
    f.app.record_event(event).await.unwrap();
    let mut event = store::event(&s.session_id, "session.started", json!({}));
    event.native_session_id = Some("wrong".into());
    assert!(f.app.record_event(event).await.is_err());
    f.app.stop(&s.session_id, true).await.unwrap();
    f.app
        .record_event(store::event(&s.session_id, "turn.completed", json!({})))
        .await
        .unwrap();
    assert_eq!(
        f.app.store.lock().await.get(&s.session_id).unwrap().state,
        State::Stopped
    );
}

#[tokio::test]
async fn crash_keeps_lease_and_makes_unfinished_delivery_uncertain() {
    let f = fixture("native").await;
    let mut s = f.app.start(start(&f)).await.unwrap();
    s.state = State::Running;
    s.message_receipts
        .insert("pulse:9".into(), "transport_accepted".into());
    f.app.store.lock().await.update(s.clone()).unwrap();
    let root = f.app.store.lock().await.root.clone();
    let mut reopened = Store::open(root).unwrap();
    assert_eq!(
        reopened.get(&s.session_id).unwrap().message_receipts["pulse:9"],
        "uncertain"
    );
    let mut replacement = s.clone();
    replacement.session_id = chorus_agent::id();
    assert!(reopened
        .insert(replacement)
        .unwrap_err()
        .contains("primary"));
}

#[tokio::test]
async fn journal_rejects_partial_records_and_out_of_order_events() {
    let f = fixture("native").await;
    let s = f.app.start(start(&f)).await.unwrap();
    let mut out_of_order = store::event(&s.session_id, "turn.started", json!({}));
    out_of_order.sequence = 999;
    assert!(f.app.record_event(out_of_order).await.is_err());
    let root = f.app.store.lock().await.root.clone();
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(
            root.join("agent-events")
                .join(format!("{}.jsonl", s.session_id)),
        )
        .unwrap()
        .write_all(b"{\"type\":")
        .unwrap();
    let reopened = Store::open(root).unwrap();
    assert!(reopened
        .events(&s.session_id, 0)
        .unwrap_err()
        .contains("incomplete"));
    assert!(reopened
        .append(store::event(&s.session_id, "turn.started", json!({})))
        .is_err());
}

#[tokio::test]
async fn handoff_transaction_recovers_primary_and_context_together() {
    let f = fixture("native").await;
    let old = f.app.start(start(&f)).await.unwrap();
    let mut req = start(&f);
    req.primary = false;
    let new = f.app.start(req).await.unwrap();
    let root = f.app.store.lock().await.root.clone();
    let mut stopped = old.clone();
    stopped.state = State::Stopped;
    stopped.primary = false;
    let mut replacement = new.clone();
    replacement.primary = true;
    replacement.pending_context =
        vec!["Open obligation: finish card 42, evidence in branch".into()];
    // Simulate power loss after writing intent and before either session snapshot.
    store::atomic_json(
        &root.join("sessions/v2/handoff.pending"),
        &vec![stopped, replacement],
    )
    .unwrap();
    let reopened = Store::open(root).unwrap();
    assert!(!reopened.get(&old.session_id).unwrap().primary);
    let restored = reopened.get(&new.session_id).unwrap();
    assert!(restored.primary);
    assert_eq!(restored.pending_context.len(), 1);
}

#[tokio::test]
async fn journal_replays_native_identity_after_snapshot_write_is_interrupted() {
    let f = fixture("native").await;
    let session = f.app.start(start(&f)).await.unwrap();
    let mut learned = store::event(&session.session_id, "session.started", json!({}));
    learned.native_session_id = Some("durable-native-id".into());
    let root = {
        let store = f.app.store.lock().await;
        store.append(learned.clone()).unwrap();
        assert!(store
            .get(&session.session_id)
            .unwrap()
            .native_session_id
            .is_none());
        store.root.clone()
    };
    let recovered = Store::open(root).unwrap();
    let restored = recovered.get(&session.session_id).unwrap();
    assert_eq!(
        restored.native_session_id.as_deref(),
        Some("durable-native-id")
    );
    assert_eq!(restored.state, State::Disconnected);
    f.app.record_event(learned).await.unwrap();
    assert_eq!(
        f.app
            .store
            .lock()
            .await
            .get(&session.session_id)
            .unwrap()
            .native_session_id,
        restored.native_session_id
    );
}

#[tokio::test]
async fn stop_reaps_owned_cli_before_releasing_primary_and_shutdown_retains_lease() {
    let f = fixture("managed").await;
    let executable = f._dir.path().join("fake-codex");
    fs::write(&executable,format!("#!/bin/sh\nif [ \"$1\" = '--version' ]; then echo 'fixture-codex 1'; exit 0; fi\necho $$ > '{}/pid'\ncat >/dev/null\nsleep 30 &\nwait\n",f._dir.path().display())).unwrap();
    let session = f.app.start(start(&f)).await.unwrap();
    f.app
        .send(&session.session_id, send("pulse:slow", "do work"))
        .await
        .unwrap();
    let pid = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(text) = fs::read_to_string(f._dir.path().join("pid")) {
                if let Ok(pid) = text.trim().parse::<i32>() {
                    break pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(3),
        f.app.stop(&session.session_id, true),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    let old = f.app.store.lock().await.get(&session.session_id).unwrap();
    assert_eq!(old.state, State::Stopped);
    assert!(!old.primary);
    assert_eq!(old.message_receipts["pulse:slow"], "uncertain");
    let replacement = f.app.start(start(&f)).await.unwrap();
    f.app.shutdown().await;
    let after = f
        .app
        .store
        .lock()
        .await
        .get(&replacement.session_id)
        .unwrap();
    assert!(after.primary);
    assert_eq!(after.state, State::Disconnected);
    assert!(f.app.start(start(&f)).await.unwrap_err().contains("paused"));
}

#[test]
fn profile_configuration_rejects_protocol_and_secret_confusion() {
    let base = json!({"runtime":"external","mode":"managed","enforcement":"trusted","no_tools":true,"timeout_secs":5});
    let mut value = base.clone();
    value["provider"] =
        json!({"protocol":"openai-chat","base_url":"https://user:secret@example.com/v1"});
    assert!(config::validate_profile(&serde_json::from_value(value).unwrap()).is_err());
    let mut value = base.clone();
    value["provider"] = json!({"protocol":"openai-chat","base_url":"http://example.com/v1"});
    assert!(config::validate_profile(&serde_json::from_value(value).unwrap()).is_err());
    let mut value = base.clone();
    value["provider"] = json!({"protocol":"openai-chat","base_url":"http://127.0.0.1:8000/v1","api_key_env":"MODEL_TOKEN"});
    assert!(config::validate_profile(&serde_json::from_value(value).unwrap()).is_ok());
    let mut value = base;
    value["enforcement"] = json!("verified");
    assert!(config::validate_profile(&serde_json::from_value(value).unwrap()).is_err());
    let request: Value = json!({"replacement":{"version":1,"profile":"test","role":"wren","cwd":"/tmp","credential_file":"/tmp/credential"},"context":"obligations"});
    assert!(serde_json::from_value::<HandoffRequest>(request).is_ok());
}

#[tokio::test]
async fn reload_rejects_live_profile_and_binding_changes_atomically() {
    let f = fixture("native").await;
    let s = f.app.start(start(&f)).await.unwrap();
    let original = f.app.config_snapshot();
    for state in [State::Idle, State::Disconnected, State::Failed] {
        let mut session = s.clone();
        session.state = state;
        f.app.store.lock().await.update(session).unwrap();
        let mut changed = original.clone();
        changed.profiles.get_mut("test").unwrap().model = Some("changed".into());
        assert!(f
            .app
            .reload_config(changed)
            .await
            .unwrap_err()
            .contains("live session"));
        let mut changed = original.clone();
        changed
            .role_workspaces
            .insert("wren".into(), "/tmp/new-anchor".into());
        assert!(f
            .app
            .reload_config(changed)
            .await
            .unwrap_err()
            .contains("role workspace"));
        assert_eq!(
            config::hash(&f.app.config_snapshot().profiles["test"]),
            s.profile_hash
        );
    }
    let mut session = s.clone();
    session.card = Some(42);
    f.app.store.lock().await.update(session).unwrap();
    let mut changed = original.clone();
    changed.worktree_base = Some("/tmp/other-base".into());
    assert!(f
        .app
        .reload_config(changed)
        .await
        .unwrap_err()
        .contains("worktree base"));
    f.app.stop(&s.session_id, true).await.unwrap();
    let mut changed = original.clone();
    changed.profiles.get_mut("test").unwrap().model = Some("now-allowed".into());
    f.app.reload_config(changed).await.unwrap();
    assert_eq!(
        f.app.config_snapshot().profiles["test"].model.as_deref(),
        Some("now-allowed")
    );
}

#[tokio::test]
async fn reload_rejects_invalid_snapshot_and_concurrency_without_partial_changes() {
    let f = fixture("native").await;
    let original = f.app.config_snapshot();
    let mut invalid = original.clone();
    invalid.version = 2;
    assert!(f
        .app
        .reload_config(invalid)
        .await
        .unwrap_err()
        .contains("major"));
    let mut invalid = original.clone();
    invalid.roles.insert("wren".into(), "absent".into());
    assert!(f
        .app
        .reload_config(invalid)
        .await
        .unwrap_err()
        .contains("missing profile"));
    let mut invalid = original.clone();
    invalid.profiles.get_mut("test").unwrap().timeout_secs = 0;
    assert!(f
        .app
        .reload_config(invalid)
        .await
        .unwrap_err()
        .contains("timeout_secs"));
    let mut invalid = original.clone();
    invalid.max_concurrent_jobs = 2;
    assert!(f
        .app
        .reload_config(invalid)
        .await
        .unwrap_err()
        .contains("restart"));
    assert_eq!(
        serde_json::to_value(f.app.config_snapshot()).unwrap(),
        serde_json::to_value(original).unwrap()
    );
}

#[test]
fn doctor_refuses_unknown_profile_without_executing_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("profiles.json");
    fs::write(&config, r#"{"version":1,"profiles":{}}"#).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_chorus-agent"))
        .args(["doctor", "absent"])
        .env("CHORUS_AGENT_CONFIG", config)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unknown operator profile: absent"));
}

#[tokio::test]
async fn interrupted_disconnect_is_uncertain_and_never_becomes_a_clean_switch() {
    let f = fixture("managed").await;
    let executable = f._dir.path().join("fake-codex");
    fs::write(&executable, "#!/bin/sh\nif [ \"$1\" = '--version' ]; then echo 'fixture-codex 1'; exit 0; fi\ncat >/dev/null\nsleep 30\n").unwrap();
    let session = f.app.start(start(&f)).await.unwrap();
    f.app
        .send(&session.session_id, send("pulse:interrupted", "work"))
        .await
        .unwrap();
    let detached = f.app.disconnect(&session.session_id).await.unwrap();
    assert_eq!(detached["state"], "disconnected");
    assert_eq!(detached["cleanly_detached"], false);
    let session = f.app.store.lock().await.get(&session.session_id).unwrap();
    assert_eq!(session.message_receipts["pulse:interrupted"], "uncertain");
    assert!(session.primary);
    assert!(!session.switch_blockers().is_empty());
    let mut superficially_idle = session.clone();
    superficially_idle.state = State::Idle;
    f.app.store.lock().await.update(superficially_idle).unwrap();
    assert!(f
        .app
        .send(&session.session_id, send("operator:new", "more work"))
        .await
        .unwrap_err()
        .contains("uncertain"));
    f.app.store.lock().await.update(session.clone()).unwrap();
    let detached = f.app.disconnect(&session.session_id).await.unwrap();
    assert_eq!(detached["cleanly_detached"], false);
}

// #4424 AC1 — a role session does not start when its runtime's guards are not
// installed in the workspace. The operator may opt out explicitly per profile.
#[tokio::test]
async fn role_session_is_refused_when_its_guards_are_not_installed() {
    let f = fixture("native").await;
    let hooks = std::path::Path::new(&f.cwd).join(".codex/hooks.json");
    fs::write(&hooks, r#"{"hooks":{}}"#).unwrap();
    let err = f.app.start(start(&f)).await.unwrap_err();
    assert!(err.contains("guards are not installed"), "{err}");
    fs::remove_file(&hooks).unwrap();
    let err = f.app.start(start(&f)).await.unwrap_err();
    assert!(err.contains("guards are not installed"), "{err}");
}

#[tokio::test]
async fn installed_guards_let_the_role_session_start() {
    let f = fixture("native").await;
    assert!(f.app.start(start(&f)).await.is_ok());
}

// #4424 AC5 — the supervisor takes its session id from the SessionRun that
// chorus-principal login wrote, and only runEndedAt on that row says whether
// the session is alive — never the supervisor's own snapshot.
#[tokio::test]
async fn session_id_is_the_login_session_run() {
    let f = fixture("native").await;
    let req = start(&f);
    let run = req.session_run.clone().unwrap();
    let s = f.app.start(req).await.unwrap();
    assert_eq!(s.session_id, run);
}

#[tokio::test]
async fn start_without_a_live_session_run_is_refused() {
    let f = fixture("native").await;
    let mut req = start(&f);
    req.session_run = None;
    assert!(f.app.start(req).await.unwrap_err().contains("session run"));
    let mut req = start(&f);
    req.session_run = Some("never-logged-in".into());
    assert!(f.app.start(req).await.unwrap_err().contains("session run"));
    let req = start(&f);
    f.runs.lock().unwrap().insert(req.session_run.clone().unwrap(), "2026-10-04T10:00:00Z".into());
    assert!(f.app.start(req).await.unwrap_err().contains("ended"));
}

#[tokio::test]
async fn a_dead_run_is_refused_even_when_its_snapshot_exists() {
    let f = fixture("native").await;
    let s = f.app.start(start(&f)).await.unwrap();
    f.runs.lock().unwrap().insert(s.session_id.clone(), "2026-10-04T10:00:00Z".into());
    // The supervisor still holds the snapshot, and it still says live.
    assert!(f.app.store.lock().await.get(&s.session_id).unwrap().live());
    let err = f.app.send(&s.session_id, send("pulse:9", "hello")).await.unwrap_err();
    assert!(err.contains("ended"), "{err}");
}

// #4424 — the supervisor's spine events go through main's chorus-log writer
// with the role and the principal, not appended raw to <state>/chorus.log.
#[tokio::test]
async fn spine_events_go_through_chorus_log_with_role_and_principal() {
    let sink = spine_sink().clone();
    let f = fixture("native").await;
    let s = f.app.start(start(&f)).await.unwrap();
    let mut seen = String::new();
    for _ in 0..100 {
        seen = fs::read_to_string(&sink).unwrap_or_default();
        if seen.contains(&s.session_id) { break; }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let line = seen.lines().find(|l| l.contains(&s.session_id)).unwrap_or_else(|| panic!("no chorus-log call: {seen:?}"));
    assert!(line.starts_with("agent."), "{line}");
    assert!(line.contains(" wren "), "{line}");
    assert!(line.contains("principal=https://identity/wren"), "{line}");
    // Negative proof: nothing is appended raw to the supervisor's own log.
    assert!(!std::path::Path::new(&f.cwd).join("state/chorus.log").exists());
}

// #4424 AC1 — Gemini silently ignores a project's hooks in a folder it does
// not trust (found live: it wrote into canonical). A Gemini role session is
// refused unless the folder is trusted in ~/.gemini/trustedFolders.json.
#[test]
fn gemini_is_refused_unless_its_folder_is_trusted() {
    use chorus_agent::server::guards_installed_in;
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let cwd = work.path().canonicalize().unwrap();
    fs::create_dir_all(cwd.join(".gemini")).unwrap();
    fs::write(cwd.join(".gemini/settings.json"), r#"{"hooks":{"BeforeTool":[{"hooks":[{"command":"chorus-hook-shim runtime-hook gemini BeforeTool"}]}]}}"#).unwrap();
    fs::create_dir_all(home.path().join(".gemini")).unwrap();
    let trust = home.path().join(".gemini/trustedFolders.json");
    // No trust file: refused even though the hook file is there.
    let err = guards_installed_in(&Runtime::Gemini, &cwd, home.path()).unwrap_err();
    assert!(err.contains("not trusted"), "{err}");
    // Trusted through a parent folder, compared case-insensitively as Gemini does.
    fs::write(&trust, format!(r#"{{"{}": "TRUST_FOLDER"}}"#, cwd.parent().unwrap().to_string_lossy().to_lowercase())).unwrap();
    assert!(guards_installed_in(&Runtime::Gemini, &cwd, home.path()).is_ok());
    // Explicitly untrusted: refused.
    fs::write(&trust, format!(r#"{{"{}": "DO_NOT_TRUST"}}"#, cwd.to_string_lossy())).unwrap();
    assert!(guards_installed_in(&Runtime::Gemini, &cwd, home.path()).is_err());
    // Codex has no folder-trust gate of this kind.
    fs::create_dir_all(cwd.join(".codex")).unwrap();
    fs::write(cwd.join(".codex/hooks.json"), "runtime-hook codex").unwrap();
    assert!(guards_installed_in(&Runtime::Codex, &cwd, home.path()).is_ok());
}
