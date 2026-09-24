use chorus_agent::{
    config::Config,
    contract::*,
    server::{self, Supervisor},
    store::{self, Store},
};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::Mutex;

struct Fixture {
    _dir: tempfile::TempDir,
    app: Arc<Supervisor>,
    client: reqwest::Client,
    start: StartRequest,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

async fn fixture() -> Fixture {
    // Keep the UDS path below macOS's sockaddr_un limit; /tmp also exists on Linux CI.
    let dir = tempfile::Builder::new()
        .prefix("agent-boundary-")
        .tempdir_in("/tmp")
        .unwrap();
    let executable = dir.path().join("fake-codex");
    fs::write(&executable, "#!/bin/sh\necho 'fixture-codex 1'\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let credential = dir.path().join("token");
    fs::write(&credential, "boundary-fixture-token").unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    let identity = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = identity.local_addr().unwrap();
    let identity_task = tokio::spawn(async move {
        let router = axum::Router::new().route("/verify", axum::routing::post(|headers: axum::http::HeaderMap| async move {
            if headers.get("authorization").and_then(|s| s.to_str().ok()) == Some("Bearer different-principal-token") {
                return (axum::http::StatusCode::OK, axum::Json(json!({"principal":"https://identity.test/other-wren", "role":"wren", "scopes":[]})));
            }
            if headers.get("authorization").and_then(|s| s.to_str().ok()) != Some("Bearer boundary-fixture-token") {
                return (axum::http::StatusCode::UNAUTHORIZED, axum::Json(json!({"error":"invalid"})));
            }
            (axum::http::StatusCode::OK, axum::Json(json!({"principal":"https://identity.test/wren", "role":"wren", "scopes":[]})))
        }));
        axum::serve(identity, router).await.unwrap();
    });
    let config: Config = serde_json::from_value(json!({"version":1,"profiles":{"native":{"runtime":"codex","mode":"native","enforcement":"trusted","approved_gaps":["filesystem isolation and hook coverage require deployment conformance"],"executable":executable,"timeout_secs":5}},"role_workspaces":{"wren":dir.path()},"max_concurrent_jobs":2})).unwrap();
    let config_file = dir.path().join("agent-profiles.json");
    store::atomic_json(&config_file, &config).unwrap();
    let app = Supervisor::new_with_config_path(
        config,
        Store::open(dir.path().join("state")).unwrap(),
        format!("http://{address}/verify"),
        config_file,
    )
    .unwrap();
    let socket = dir.path().join("supervisor.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let service = app.clone();
    let supervisor_task = tokio::spawn(async move {
        axum::serve(listener, server::router(service))
            .await
            .unwrap();
    });
    let client = reqwest::Client::builder()
        .unix_socket(socket)
        .build()
        .unwrap();
    let start = serde_json::from_value(json!({"version":1,"profile":"native","role":"wren","cwd":dir.path(),"credential_file":credential,"primary":true,"native_session_id":"native-original"})).unwrap();
    Fixture {
        _dir: dir,
        app,
        client,
        start,
        tasks: vec![identity_task, supervisor_task],
    }
}
async fn post(f: &Fixture, path: &str, body: Value) -> (reqwest::StatusCode, Value) {
    let response = f
        .client
        .post(format!("http://localhost{path}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    (response.status(), response.json().await.unwrap())
}
struct EnvRestore(Vec<(&'static str, Option<String>)>);
impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (name, value) in &self.0 {
            if let Some(value) = value {
                std::env::set_var(name, value);
            } else {
                std::env::remove_var(name);
            }
        }
    }
}

#[tokio::test]
async fn managed_terminal_ingress_cannot_claim_delivery_and_role_cannot_approve() {
    let f = fixture().await;
    let session = f.app.start(f.start.clone()).await.unwrap();
    {
        let mut store = f.app.store.lock().await;
        let mut s = store.get(&session.session_id).unwrap();
        s.mode = Mode::Managed;
        s.state = State::Running;
        s.message_receipts
            .insert("pulse:9".into(), "transport_accepted".into());
        store.update(s).unwrap();
    }
    let event = store::event(&session.session_id, "turn.completed", json!({}));
    let (code, _) = post(&f, "/v1/events", serde_json::to_value(event).unwrap()).await;
    assert_eq!(code, 200);
    let state = f.app.store.lock().await.get(&session.session_id).unwrap();
    assert_eq!(state.state, State::Running);
    assert_eq!(state.message_receipts["pulse:9"], "transport_accepted");
    let event = store::event(
        &session.session_id,
        "tool.completed",
        json!({"runtime":"gemini"}),
    );
    assert_eq!(
        post(&f, "/v1/events", serde_json::to_value(event).unwrap())
            .await
            .0,
        400
    );
    let approval = ApprovalRequest {
        credential_file: f.start.credential_file.clone(),
        request_id: "req".into(),
        decision: Some("once".into()),
        option_id: None,
    };
    assert!(f
        .app
        .approve(&session.session_id, approval)
        .await
        .unwrap_err()
        .contains("role"));
}

#[tokio::test]
async fn uds_boundary_preserves_pulse_claim_and_requires_successful_ack_before_delivery() {
    let f = fixture().await;
    let session = f.app.start(f.start.clone()).await.unwrap();
    {
        let mut store = f.app.store.lock().await;
        let mut s = store.get(&session.session_id).unwrap();
        s.pending_context =
            vec!["handoff obligation: preserve this until stdout is acknowledged".into()];
        store.update(s).unwrap();
    }
    let received = Arc::new(Mutex::new(Vec::<Value>::new()));
    let acknowledged = Arc::new(AtomicBool::new(false));
    let refuse_ack = Arc::new(AtomicBool::new(false));
    let pulse = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = pulse.local_addr().unwrap();
    let expected_id = session.session_id.clone();
    let handler = {
        let received = received.clone();
        let acknowledged = acknowledged.clone();
        let refuse_ack = refuse_ack.clone();
        move |axum::extract::Path(action): axum::extract::Path<String>,
              headers: axum::http::HeaderMap,
              axum::Json(body): axum::Json<Value>| {
            let received = received.clone();
            let acknowledged = acknowledged.clone();
            let refuse_ack = refuse_ack.clone();
            let expected_id = expected_id.clone();
            async move {
                if headers
                    .get("x-chorus-pulse-secret")
                    .and_then(|v| v.to_str().ok())
                    != Some("boundary-shared-secret")
                {
                    return (
                        axum::http::StatusCode::FORBIDDEN,
                        axum::Json(json!({"error":"unauthorized"})),
                    );
                }
                assert_eq!(body["role"], "wren");
                assert_eq!(body["session_id"], expected_id);
                received
                    .lock()
                    .await
                    .push(json!({"action":action,"body":body}));
                if action == "claim" {
                    let messages = if acknowledged.load(Ordering::SeqCst) {
                        json!([])
                    } else {
                        json!([{"id":41,"message_id":"pulse:41","from":"silas","content":"Peer evidence 🌍\nkeep line two", "kind":"peer_message"}])
                    };
                    (
                        axum::http::StatusCode::OK,
                        axum::Json(json!({"ok":true,"messages":messages})),
                    )
                } else {
                    if refuse_ack.load(Ordering::SeqCst) {
                        return (
                            axum::http::StatusCode::SERVICE_UNAVAILABLE,
                            axum::Json(json!({"error":"fixture outage"})),
                        );
                    }
                    assert_eq!(body["ids"], json!([41]));
                    let changed = !acknowledged.swap(true, Ordering::SeqCst);
                    (
                        axum::http::StatusCode::OK,
                        axum::Json(
                            json!({"ok":true,"acknowledged":if changed {1} else {0},"status":"context_delivered"}),
                        ),
                    )
                }
            }
        }
    };
    let pulse_task = tokio::spawn(async move {
        axum::serve(
            pulse,
            axum::Router::new().route("/api/agent-inbox/{action}", axum::routing::post(handler)),
        )
        .await
        .unwrap();
    });
    let _env = EnvRestore(
        ["CHORUS_PULSE_BASE_URL", "CHORUS_PULSE_SECRET"]
            .into_iter()
            .map(|name| (name, std::env::var(name).ok()))
            .collect(),
    );
    std::env::set_var(
        "CHORUS_PULSE_BASE_URL",
        format!("http://{address}/api/nudge"),
    );
    std::env::set_var("CHORUS_PULSE_SECRET", "wrong-secret");
    let path = format!("/v1/sessions/{}/boundary", session.session_id);
    let (_, rejected) = post(&f, &path, json!({"native_session_id":"native-original"})).await;
    assert_eq!(rejected["messages"], json!([]));
    assert!(rejected["delivery_error"].as_str().unwrap().contains("403"));
    assert!(received.lock().await.is_empty());
    std::env::set_var("CHORUS_PULSE_SECRET", "boundary-shared-secret");
    let (status, first) = post(&f, &path, json!({"native_session_id":"native-original"})).await;
    assert!(status.is_success());
    assert_eq!(
        first["messages"][0]["content"],
        "Peer evidence 🌍\nkeep line two"
    );
    let (_, repeated) = post(&f, &path, json!({"native_session_id":"native-original"})).await;
    assert_eq!(repeated["messages"], first["messages"]);
    assert!(!acknowledged.load(Ordering::SeqCst));
    assert!(!f
        .app
        .store
        .lock()
        .await
        .events(&session.session_id, 0)
        .unwrap()
        .iter()
        .any(|e| e.event_type == "context.delivered"));
    let ack = format!("/v1/sessions/{}/ack", session.session_id);
    refuse_ack.store(true, Ordering::SeqCst);
    assert!(!post(
        &f,
        &ack,
        json!({"ids":[41],"context_token":first["context_token"]})
    )
    .await
    .0
    .is_success());
    assert_eq!(
        f.app
            .store
            .lock()
            .await
            .get(&session.session_id)
            .unwrap()
            .pending_context
            .len(),
        1
    );
    refuse_ack.store(false, Ordering::SeqCst);
    let (status, receipt) = post(
        &f,
        &ack,
        json!({"ids":[41],"context_token":first["context_token"]}),
    )
    .await;
    assert!(status.is_success());
    assert_eq!(receipt["status"], "context_delivered");
    assert!(f
        .app
        .store
        .lock()
        .await
        .get(&session.session_id)
        .unwrap()
        .pending_context
        .is_empty());
    assert_eq!(
        post(&f, &ack, json!({"ids":[41]})).await.1["acknowledged"],
        0
    );
    assert_eq!(
        post(&f, &path, json!({"native_session_id":"native-original"}))
            .await
            .1["messages"],
        json!([])
    );
    pulse_task.abort();
}

#[tokio::test]
async fn concurrent_primary_start_resume_and_handoff_keep_exactly_one_live_primary() {
    let f = fixture().await;
    let (a, b) = tokio::join!(f.app.start(f.start.clone()), f.app.start(f.start.clone()));
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let original = a.or(b).unwrap();
    f.app
        .record_event(store::event(
            &original.session_id,
            "session.disconnected",
            json!({}),
        ))
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        f.app.resume(&original.session_id),
        f.app.resume(&original.session_id)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let mut replacement = f.start.clone();
    replacement.native_session_id = None;
    let body = json!({"replacement":replacement,"context":"Finish the card; branch evidence and open obligations are preserved here."});
    let path = format!("/v1/sessions/{}/handoff", original.session_id);
    let (a, b) = tokio::join!(post(&f, &path, body.clone()), post(&f, &path, body));
    assert_eq!(
        usize::from(a.0.is_success()) + usize::from(b.0.is_success()),
        1
    );
    let store = f.app.store.lock().await;
    let live = store
        .sessions
        .values()
        .filter(|s| s.role == "wren" && s.primary && s.live())
        .collect::<Vec<_>>();
    assert_eq!(live.len(), 1);
    assert_ne!(live[0].session_id, original.session_id);
    assert_eq!(live[0].pending_context.len(), 1);
    assert_eq!(
        store.get(&original.session_id).unwrap().state,
        State::Stopped
    );
}

#[tokio::test]
async fn resume_cannot_reacquire_native_conversation_already_owned_by_replacement() {
    let f = fixture().await;
    let original = f.app.start(f.start.clone()).await.unwrap();
    f.app.stop(&original.session_id, true).await.unwrap();
    let replacement = f.app.start(f.start.clone()).await.unwrap();
    assert_ne!(original.session_id, replacement.session_id);
    assert!(
        f.app.resume(&original.session_id).await.is_err(),
        "resume must refuse a native conversation leased to the replacement"
    );
    assert_eq!(
        f.app
            .store
            .lock()
            .await
            .sessions
            .values()
            .filter(|s| s.live() && s.native_session_id.as_deref() == Some("native-original"))
            .count(),
        1
    );
}

#[tokio::test]
async fn operator_reload_uses_configured_file_and_preserves_live_sessions() {
    let f = fixture().await;
    let session = f.app.start(f.start.clone()).await.unwrap();
    let path = f._dir.path().join("agent-profiles.json");
    let mut candidate = f.app.config_snapshot();
    let mut next = candidate.profiles["native"].clone();
    next.model = Some("fixture-new-model".into());
    candidate.profiles.insert("next".into(), next);
    candidate.roles.insert("wren".into(), "next".into());
    store::atomic_json(&path, &candidate).unwrap();
    let (status, body) = post(&f, "/v1/config/reload", json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["sessions_preserved"], 1);
    let state = f.app.store.lock().await.get(&session.session_id).unwrap();
    assert_eq!(state.profile, "native");
    assert_eq!(state.state, State::Idle);
    assert!(state.primary);
    let loaded: Value = f
        .client
        .get("http://localhost/v1/config")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(loaded["roles"]["wren"], "next");
    assert_eq!(loaded["profiles"]["next"]["model"], "fixture-new-model");
    assert!(loaded["profiles"]["native"].get("executable").is_none());
    assert!(loaded["profiles"]["native"].get("adapter_config").is_none());
    candidate.profiles.remove("native");
    store::atomic_json(&path, &candidate).unwrap();
    let (status, body) = post(&f, "/v1/config/reload", json!({})).await;
    assert_eq!(status, 400);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("live session"));
    assert!(f.app.config_snapshot().profiles.contains_key("native"));
    fs::write(&path, "{broken").unwrap();
    assert_eq!(post(&f, "/v1/config/reload", json!({})).await.0, 400);
    assert_eq!(f.app.config_snapshot().roles["wren"], "next");
}

#[tokio::test]
async fn operator_switch_preserves_card_lease_context_and_credential_without_replaying_history() {
    let f = fixture().await;
    let worktree = f._dir.path().join("wren-42");
    fs::create_dir(&worktree).unwrap();
    let mut config = f.app.config_snapshot();
    config.worktree_base = Some(f._dir.path().to_string_lossy().into());
    let mut next = config.profiles["native"].clone();
    next.model = Some("new-model".into());
    config.profiles.insert("next".into(), next);
    f.app.reload_config(config).await.unwrap();
    let mut start = f.start.clone();
    start.cwd = worktree.to_string_lossy().into();
    start.card = Some(42);
    let original = f.app.start(start).await.unwrap();
    {
        let mut store = f.app.store.lock().await;
        let mut old = store.get(&original.session_id).unwrap();
        old.pending_context
            .push("earlier undelivered obligation".into());
        store.update(old).unwrap();
    }
    let (code, new) = post(&f, &format!("/v1/sessions/{}/switch", original.session_id), json!({"profile":"next","context":"New handoff: finish card 42; evidence stays in its worktree."})).await;
    assert_eq!(code, 200, "{new}");
    assert_eq!(new["cwd"], original.cwd);
    assert_eq!(new["card"], 42);
    assert_eq!(new["role"], original.role);
    assert_eq!(new["principal"], original.principal);
    assert_eq!(new["primary"], true);
    assert_eq!(new["model"], "new-model");
    assert!(new["native_session_id"].is_null());
    assert!(new.get("credential_file").is_none());
    let store = f.app.store.lock().await;
    let old = store.get(&original.session_id).unwrap();
    assert_eq!(old.state, State::Stopped);
    assert!(!old.primary);
    let replacement = store.get(new["session_id"].as_str().unwrap()).unwrap();
    assert_eq!(replacement.credential_file, original.credential_file);
    assert_eq!(replacement.pending_context.len(), 2);
    assert_eq!(
        replacement.pending_context[0],
        "earlier undelivered obligation"
    );
    assert!(replacement.pending_context[1].contains("finish card 42"));
}

#[tokio::test]
async fn operator_switch_reports_busy_and_uncertain_states_and_preserves_failed_probe_lease() {
    let f = fixture().await;
    let mut config = f.app.config_snapshot();
    let mut unavailable = config.profiles["native"].clone();
    unavailable.executable = Some(
        f._dir
            .path()
            .join("uninstalled-runtime")
            .to_string_lossy()
            .into(),
    );
    config.profiles.insert("unavailable".into(), unavailable);
    f.app.reload_config(config).await.unwrap();
    let original = f.app.start(f.start.clone()).await.unwrap();
    for state in [
        State::Running,
        State::AwaitingApproval,
        State::Disconnected,
        State::Failed,
        State::Idle,
    ] {
        let mut s = original.clone();
        s.state = state.clone();
        if state == State::Idle {
            s.message_receipts
                .insert("pulse:uncertain".into(), "uncertain".into());
        }
        f.app.store.lock().await.update(s).unwrap();
        let status: Value = f
            .client
            .get(format!(
                "http://localhost/v1/sessions/{}",
                original.session_id
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(status["switch_ready"], false);
        assert!(!status["switch_blockers"].as_array().unwrap().is_empty());
        let (code, _) = post(
            &f,
            &format!("/v1/sessions/{}/switch", original.session_id),
            json!({"profile":"native","context":"finish obligations"}),
        )
        .await;
        assert_eq!(code, 400, "state {state:?}");
        assert_eq!(f.app.store.lock().await.sessions.len(), 1);
        assert!(
            f.app
                .store
                .lock()
                .await
                .get(&original.session_id)
                .unwrap()
                .primary
        );
    }
    f.app.store.lock().await.update(original.clone()).unwrap();
    let (code, _) = post(
        &f,
        &format!("/v1/sessions/{}/switch", original.session_id),
        json!({"profile":"unavailable","context":"finish obligations"}),
    )
    .await;
    assert_eq!(code, 400);
    let preserved = f.app.store.lock().await.get(&original.session_id).unwrap();
    assert_eq!(preserved.state, State::Idle);
    assert!(preserved.primary);
    assert_eq!(f.app.store.lock().await.sessions.len(), 1);
}

#[tokio::test]
async fn operator_switch_cannot_replace_the_authenticated_principal() {
    let f = fixture().await;
    let original = f.app.start(f.start.clone()).await.unwrap();
    let alternate = f._dir.path().join("alternate-token");
    fs::write(&alternate, "different-principal-token").unwrap();
    fs::set_permissions(&alternate, fs::Permissions::from_mode(0o600)).unwrap();
    let (code, body) = post(
        &f,
        &format!("/v1/sessions/{}/switch", original.session_id),
        json!({"profile":"native","context":"retain obligations","credential_file":alternate}),
    )
    .await;
    assert_eq!(code, 400);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("principal"));
    let store = f.app.store.lock().await;
    assert_eq!(store.sessions.len(), 1);
    let old = store.get(&original.session_id).unwrap();
    assert!(old.primary);
    assert_eq!(old.state, State::Idle);
}

#[tokio::test]
async fn clean_operator_disconnect_can_resume_or_switch_without_releasing_lease() {
    let f = fixture().await;
    let original = f.app.start(f.start.clone()).await.unwrap();
    let route = format!("/v1/sessions/{}", original.session_id);
    let (code, detached) = post(&f, &format!("{route}/disconnect"), json!({})).await;
    assert_eq!(code, 200, "{detached}");
    assert_eq!(detached["cleanly_detached"], true);
    assert_eq!(detached["state"], "disconnected");
    assert_eq!(detached["primary"], true);
    let status: Value = f
        .client
        .get(format!("http://localhost{route}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status["switch_ready"], true);
    let recovered = Store::open(f.app.store.lock().await.root.clone()).unwrap();
    assert!(
        recovered
            .get(&original.session_id)
            .unwrap()
            .cleanly_detached
    );
    let (code, resumed) = post(&f, &format!("{route}/resume"), json!({})).await;
    assert_eq!(code, 200, "{resumed}");
    assert_eq!(resumed["cleanly_detached"], false);
    assert_eq!(resumed["native_session_id"], "native-original");
    assert_eq!(
        post(&f, &format!("{route}/disconnect"), json!({})).await.0,
        200
    );
    let (code, switched) = post(&f, &format!("{route}/switch"), json!({"profile":"native","context":"Retain explicit obligations after closing the old terminal."})).await;
    assert_eq!(code, 200, "{switched}");
    assert_ne!(switched["session_id"], original.session_id);
    assert!(switched["native_session_id"].is_null());
}

#[tokio::test]
async fn initial_handoff_context_is_durable_bounded_and_does_not_start_a_turn() {
    let f = fixture().await;
    let original = f.app.start(f.start.clone()).await.unwrap();
    let route = format!("/v1/sessions/{}/context", original.session_id);
    let (code, queued) = post(
        &f,
        &route,
        json!({"text":"Legacy obligation: finish card 42 🌍\nEvidence in its worktree."}),
    )
    .await;
    assert_eq!(code, 200, "{queued}");
    assert_eq!(queued["model_called"], false);
    let store = f.app.store.lock().await;
    let session = store.get(&original.session_id).unwrap();
    assert_eq!(session.state, State::Idle);
    assert_eq!(session.pending_context.len(), 1);
    assert!(session.message_receipts.is_empty());
    assert_eq!(
        store
            .events(&original.session_id, 0)
            .unwrap()
            .iter()
            .filter(|e| e.event_type == "context.enqueued")
            .count(),
        1
    );
    let root = store.root.clone();
    drop(store);
    let reopened = Store::open(root).unwrap();
    assert_eq!(
        reopened.get(&original.session_id).unwrap().pending_context,
        session.pending_context
    );
    assert_eq!(post(&f, &route, json!({"text":" "})).await.0, 400);
    assert_eq!(
        post(&f, &route, json!({"text":"x".repeat(MAX_INPUT_BYTES/2)}))
            .await
            .0,
        400
    );
    f.app
        .record_event(store::event(
            &original.session_id,
            "turn.started",
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(
        post(&f, &route, json!({"text":"do not inject mid-turn"}))
            .await
            .0,
        400
    );
}
